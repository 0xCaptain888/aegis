#!/usr/bin/env bash
# scripts/deploy.sh
# Builds the four Soroban contracts to wasm and deploys them to Stellar testnet.
# Deployment order:
#   1. groth16_bn254_verifier  — self-contained BN254 Groth16 verifier (no external dep)
#   2. por_verifier            — PoR attestation (delegates verify to #1)
#   3. eligibility_verifier    — investor eligibility (delegates verify to #1)
#   4. rwa_gate                — composes PoR freshness + eligibility
#
# Requires: stellar-cli (>= 22.x), a funded testnet identity, and the
# Groth16 VK files produced by scripts/build-circuits.sh:
#   build/proof_of_reserves_vk_soroban.json
#   build/eligibility_vk_soroban.json
#
# Usage:
#   export STELLAR_NETWORK=testnet
#   export STELLAR_IDENTITY=aegis-deployer   # created + funded automatically if missing
#   bash scripts/deploy.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NET="${STELLAR_NETWORK:-testnet}"
SRC="${STELLAR_IDENTITY:-aegis-deployer}"
OUT="$ROOT/build/deploy.${NET}.json"
mkdir -p "$ROOT/build"

echo "==> Network: $NET   Identity: $SRC"

# 0. Ensure identity exists & is funded (friendbot)
if ! stellar keys address "$SRC" >/dev/null 2>&1; then
  echo "==> Creating + funding identity '$SRC'"
  stellar keys generate "$SRC" --network "$NET" --fund
fi
ADMIN_ADDR="$(stellar keys address "$SRC")"
echo "    admin = $ADMIN_ADDR"

# 1. Build all contracts
echo "==> Building all contracts (wasm)"
( cd "$ROOT/contracts" && stellar contract build )
WASM_DIR="$ROOT/contracts/target/wasm32-unknown-unknown/release"

deploy () {
  local WASM="$1"
  stellar contract deploy --wasm "$WASM" --source "$SRC" --network "$NET" 2>/dev/null
}

# 2. Deploy contracts (verifier first — others depend on it)
echo "==> Deploying groth16_bn254_verifier (self-contained BN254 Groth16 verifier)"
GROTH16_ID="$(deploy "$WASM_DIR/groth16_bn254_verifier.wasm")"
echo "    groth16_bn254_verifier = $GROTH16_ID"

echo "==> Deploying por_verifier"
POR_ID="$(deploy "$WASM_DIR/por_verifier.wasm")"
echo "    por_verifier = $POR_ID"

echo "==> Deploying eligibility_verifier"
ELIG_ID="$(deploy "$WASM_DIR/eligibility_verifier.wasm")"
echo "    eligibility_verifier = $ELIG_ID"

echo "==> Deploying rwa_gate"
GATE_ID="$(deploy "$WASM_DIR/rwa_gate.wasm")"
echo "    rwa_gate = $GATE_ID"

# 3. Initialize contracts
echo "==> Initializing groth16_bn254_verifier"
stellar contract invoke --id "$GROTH16_ID" --source "$SRC" --network "$NET" -- \
  init --admin "$ADMIN_ADDR" || true

echo "==> Initializing por_verifier"
stellar contract invoke --id "$POR_ID" --source "$SRC" --network "$NET" -- \
  init --admin "$ADMIN_ADDR" --verifier "$GROTH16_ID" || true

echo "==> Initializing eligibility_verifier"
stellar contract invoke --id "$ELIG_ID" --source "$SRC" --network "$NET" -- \
  init --admin "$ADMIN_ADDR" --verifier "$GROTH16_ID" || true

echo "==> Initializing rwa_gate"
stellar contract invoke --id "$GATE_ID" --source "$SRC" --network "$NET" -- \
  init --admin "$ADMIN_ADDR" || true

# 4. Register VKs with the Groth16 verifier (requires circuit build artifacts)
# VK files are produced by scripts/build-circuits.sh → scripts/export-vk.mjs
POR_VK="$ROOT/build/proof_of_reserves_vk_soroban.json"
ELIG_VK="$ROOT/build/eligibility_vk_soroban.json"

if [ -f "$POR_VK" ] && [ -f "$ELIG_VK" ]; then
  echo "==> Registering VKs with groth16_bn254_verifier"
  node "$ROOT/scripts/register-vk.mjs" \
    --verifier "$GROTH16_ID" --vk-id 0 --vk-file "$POR_VK" \
    --source "$SRC" --network "$NET"
  node "$ROOT/scripts/register-vk.mjs" \
    --verifier "$GROTH16_ID" --vk-id 1 --vk-file "$ELIG_VK" \
    --source "$SRC" --network "$NET"
  echo "    VK 0 (PoR) and VK 1 (Eligibility) registered."
else
  echo "    [WARNING] VK files not found — run scripts/build-circuits.sh first,"
  echo "    then call scripts/register-vk.mjs manually to register VK 0 and VK 1."
fi

# 5. Persist addresses for the frontend / e2e / invoke scripts
cat > "$OUT" <<JSON
{
  "network": "$NET",
  "admin": "$ADMIN_ADDR",
  "groth16_verifier": "$GROTH16_ID",
  "por_verifier": "$POR_ID",
  "eligibility_verifier": "$ELIG_ID",
  "rwa_gate": "$GATE_ID"
}
JSON
echo ""
echo "✓ Deployed. Addresses written to $OUT"
cat "$OUT"
echo ""
echo "Explorer: https://stellar.expert/explorer/$NET/contract/$GATE_ID"

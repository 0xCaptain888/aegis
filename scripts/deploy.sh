#!/usr/bin/env bash
# scripts/deploy.sh
# Builds the three Soroban contracts to wasm and deploys them to Stellar testnet.
# Requires: stellar-cli (`stellar`), a funded testnet identity, and the community
# groth16_verifier contract address (set GROTH16_VERIFIER_ID or deploy your own).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NET="${STELLAR_NETWORK:-testnet}"
SRC="${STELLAR_IDENTITY:-aegis-deployer}"
OUT="$ROOT/build/deploy.${NET}.json"
mkdir -p "$ROOT/build"

echo "==> Network: $NET   Identity: $SRC"

# 0. Ensure identity exists & is funded (testnet friendbot)
if ! stellar keys address "$SRC" >/dev/null 2>&1; then
  echo "==> Creating + funding identity '$SRC'"
  stellar keys generate "$SRC" --network "$NET" --fund
fi
ADMIN_ADDR="$(stellar keys address "$SRC")"
echo "    admin = $ADMIN_ADDR"

# 1. Build contracts
echo "==> Building contracts (wasm)"
( cd "$ROOT/contracts" && stellar contract build )
WASM_DIR="$ROOT/contracts/target/wasm32-unknown-unknown/release"

deploy () {
  local WASM="$1"
  stellar contract deploy --wasm "$WASM" --source "$SRC" --network "$NET" 2>/dev/null
}

echo "==> Deploying por_verifier"
POR_ID="$(deploy "$WASM_DIR/por_verifier.wasm")"
echo "    por_verifier = $POR_ID"

echo "==> Deploying eligibility_verifier"
ELIG_ID="$(deploy "$WASM_DIR/eligibility_verifier.wasm")"
echo "    eligibility_verifier = $ELIG_ID"

echo "==> Deploying rwa_gate"
GATE_ID="$(deploy "$WASM_DIR/rwa_gate.wasm")"
echo "    rwa_gate = $GATE_ID"

# 2. The shared groth16 verifier (community contract). If you don't have one,
#    see docs/UPGRADE.md for deploying stellar/soroban-examples groth16_verifier.
GROTH16_ID="${GROTH16_VERIFIER_ID:-REPLACE_WITH_GROTH16_VERIFIER_CONTRACT_ID}"
echo "==> Using groth16 verifier = $GROTH16_ID"

# 3. Initialize
echo "==> Initializing contracts"
stellar contract invoke --id "$POR_ID"  --source "$SRC" --network "$NET" -- \
  init --admin "$ADMIN_ADDR" --verifier "$GROTH16_ID" || true
stellar contract invoke --id "$ELIG_ID" --source "$SRC" --network "$NET" -- \
  init --admin "$ADMIN_ADDR" --verifier "$GROTH16_ID" || true
stellar contract invoke --id "$GATE_ID" --source "$SRC" --network "$NET" -- \
  init --admin "$ADMIN_ADDR" || true

# 4. Persist addresses for the frontend / e2e script
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
echo "View on explorer: https://stellar.expert/explorer/$NET/contract/$GATE_ID"

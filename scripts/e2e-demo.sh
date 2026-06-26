#!/usr/bin/env bash
# scripts/e2e-demo.sh
# Full happy-path demo that produces the "aha moment" for the demo video:
#   1. Issuer publishes a Proof-of-Reserves commitment + attests on-chain.
#   2. Issuer issues an investor an eligibility credential.
#   3. Investor generates an eligibility proof (revealing only a boolean+nullifier).
#   4. RWA gate authorizes the receive ONLY when reserves are fresh AND investor
#      is eligible — and rejects a second use of the same nullifier.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NET="${STELLAR_NETWORK:-testnet}"
SRC="${STELLAR_IDENTITY:-aegis-deployer}"
DEPLOY="$ROOT/build/deploy.${NET}.json"
BUILD="$ROOT/build"
WORK="$ROOT/build/e2e"
mkdir -p "$WORK"

jqget () { node -e "console.log(require('$DEPLOY').$1)"; }
POR_ID="$(jqget por_verifier)"
ELIG_ID="$(jqget eligibility_verifier)"
GATE_ID="$(jqget rwa_gate)"
ADMIN="$(jqget admin)"

echo "######## STEP 1: Proof of Reserves ########"
( cd "$ROOT/prover" && node src/prove-reserves.js \
    --balances 5000000,3000000,2000000 --supply 9000000 --minBps 10000 --salt 12345 \
    --wasm "$BUILD/proof_of_reserves_js/proof_of_reserves.wasm" \
    --zkey "$BUILD/proof_of_reserves_final.zkey" \
    --out "$WORK/reserves_proof.json" )

COMMIT="$(node -e "console.log(require('$WORK/reserves_proof.json').reservesCommitment)")"
echo "  reservesCommitment = $COMMIT"
# (register policy + attest on-chain via stellar invoke — see UPGRADE.md for the
#  exact arg encoding of BytesN<32> and the proof struct)

echo "######## STEP 2: Issue investor credential ########"
( cd "$ROOT/prover" && node src/build-allowlist.js --codes 840,826,392,276 --out "$WORK/allowlist.json" )
( cd "$ROOT/prover" && ISSUER_SEED="${ISSUER_SEED:-aegis-dev-issuer-seed-DO-NOT-USE-IN-PROD}" \
    node src/issue-credential.js --kyc 2 --jurisdiction 840 --accredited 1 \
    --expiry 1900000000 --secret 123456789 --out "$WORK/credential.json" )

echo "######## STEP 3: Investor generates eligibility proof ########"
( cd "$ROOT/prover" && node src/prove-eligibility.js \
    --credential "$WORK/credential.json" --allowlist "$WORK/allowlist.json" \
    --requiredKyc 2 --requireAccredited 1 --actionId 777 \
    --wasm "$BUILD/eligibility_js/eligibility.wasm" \
    --zkey "$BUILD/eligibility_final.zkey" \
    --out "$WORK/eligibility_proof.json" )

NULL="$(node -e "console.log(require('$WORK/eligibility_proof.json').nullifier)")"
echo "  nullifier = $NULL"

echo ""
echo "✓ E2E artifacts generated in $WORK"
echo "  Use scripts/invoke-onchain.sh to push these proofs to the deployed"
echo "  contracts and watch the gate authorize (then reject the replay)."

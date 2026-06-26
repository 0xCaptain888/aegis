#!/usr/bin/env bash
# scripts/build-circuits.sh
# Compiles both circuits, runs the Groth16 trusted setup (Powers of Tau +
# circuit-specific phase 2), and exports verification keys + Solidity-free
# artifacts the prover and Soroban verifier consume.
#
# Prereqs (see SETUP.md): circom >= 2.1.9, npx snarkjs, node, and curl.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD="$ROOT/build"
PTAU="$BUILD/pot16_final.ptau"
mkdir -p "$BUILD"

echo "==> Installing prover deps (provides circomlib include path)"
( cd "$ROOT/prover" && npm install )

# circomlib must be resolvable from circuits/lib shims:
if [ ! -d "$ROOT/node_modules/circomlib" ]; then
  echo "==> Linking circomlib to repo root for circuit includes"
  ( cd "$ROOT" && npm init -y >/dev/null 2>&1 || true && npm install circomlib@2.0.5 )
fi

# ---- Powers of Tau (phase 1). 2^16 constraints is plenty for these circuits ----
if [ ! -f "$PTAU" ]; then
  echo "==> Powers of Tau (this is a DEV ceremony — DO NOT use in production)"
  npx snarkjs powersoftau new bn128 16 "$BUILD/pot16_0000.ptau" -v
  npx snarkjs powersoftau contribute "$BUILD/pot16_0000.ptau" "$BUILD/pot16_0001.ptau" \
    --name="aegis-dev-1" -v -e="$(head -c 64 /dev/urandom | base64)"
  npx snarkjs powersoftau prepare phase2 "$BUILD/pot16_0001.ptau" "$PTAU" -v
fi

build_circuit () {
  local NAME="$1"; local SRC="$2"
  echo "==> Compiling $NAME"
  circom "$SRC" --r1cs --wasm --sym -o "$BUILD" -l "$ROOT"
  echo "==> Groth16 setup for $NAME"
  npx snarkjs groth16 setup "$BUILD/$NAME.r1cs" "$PTAU" "$BUILD/${NAME}_0000.zkey"
  npx snarkjs zkey contribute "$BUILD/${NAME}_0000.zkey" "$BUILD/${NAME}_final.zkey" \
    --name="aegis-dev" -v -e="$(head -c 64 /dev/urandom | base64)"
  npx snarkjs zkey export verificationkey "$BUILD/${NAME}_final.zkey" "$BUILD/${NAME}_vkey.json"
  echo "==> Exporting Soroban VK for $NAME"
  node "$ROOT/scripts/export-vk.mjs" "$BUILD/${NAME}_vkey.json" "$BUILD/${NAME}_vk_soroban.json"
}

build_circuit "proof_of_reserves" "$ROOT/circuits/proof_of_reserves/proof_of_reserves.circom"
build_circuit "eligibility"       "$ROOT/circuits/eligibility/eligibility.circom"

echo ""
echo "✓ Circuits built. Artifacts in $BUILD:"
ls -1 "$BUILD" | sed 's/^/   /'
echo ""
echo "Next: register the *_vk_soroban.json keys with your groth16_verifier (scripts/deploy.sh)."

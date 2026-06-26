// src/prove-reserves.js
// Generates a Groth16 Proof-of-Reserves and formats it for the Soroban verifier.
//
// Usage:
//   node src/prove-reserves.js \
//     --balances 5000000,3000000,2000000 \
//     --supply 9000000 --minBps 10000 --salt 12345 \
//     --wasm ../build/proof_of_reserves_js/proof_of_reserves.wasm \
//     --zkey ../build/proof_of_reserves_final.zkey \
//     --out reserves_proof.json
//
// Prints the public reservesCommitment so the issuer can publish it on-chain.

import { groth16 } from "snarkjs";
import { poseidonHash, toFieldStr } from "./field.js";
import { formatProofForSoroban } from "./soroban-format.js";
import { writeFileSync } from "node:fs";

const N = 8; // must match the circuit's ProofOfReserves(8)

function arg(name, def) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 ? process.argv[i + 1] : def;
}

export async function buildReservesInput({ balances, supply, minBps, salt }) {
  const padded = [...balances.map((b) => BigInt(b))];
  if (padded.length > N) throw new Error(`max ${N} balances`);
  while (padded.length < N) padded.push(0n);

  const reservesCommitment = await poseidonHash([...padded, BigInt(salt)]);

  const input = {
    totalSupply: toFieldStr(supply),
    reservesCommitment: reservesCommitment.toString(),
    minCollateralBps: toFieldStr(minBps),
    balances: padded.map((b) => b.toString()),
    salt: toFieldStr(salt),
  };
  return { input, reservesCommitment };
}

async function main() {
  const balances = arg("balances", "5000000,3000000,2000000")
    .split(",")
    .map((s) => BigInt(s.trim()));
  const supply = BigInt(arg("supply", "9000000"));
  const minBps = BigInt(arg("minBps", "10000")); // 100%
  const salt = BigInt(arg("salt", "12345"));
  const wasm = arg("wasm", "../build/proof_of_reserves_js/proof_of_reserves.wasm");
  const zkey = arg("zkey", "../build/proof_of_reserves_final.zkey");
  const out = arg("out", "reserves_proof.json");

  const { input, reservesCommitment } = await buildReservesInput({ balances, supply, minBps, salt });

  console.log("→ generating witness + proof…");
  const { proof, publicSignals } = await groth16.fullProve(input, wasm, zkey);

  const formatted = formatProofForSoroban(proof, publicSignals);
  const result = {
    kind: "proof_of_reserves",
    reservesCommitment: reservesCommitment.toString(),
    publicSignalsRaw: publicSignals,
    soroban: formatted,
    rawProof: proof,
  };
  writeFileSync(out, JSON.stringify(result, null, 2));
  console.log(`✓ Reserves proof → ${out}`);
  console.log(`  publish reservesCommitment on-chain: ${reservesCommitment.toString()}`);
}

if (import.meta.url === `file://${process.argv[1]}`) {
  main().catch((e) => {
    console.error(e);
    process.exit(1);
  });
}

// scripts/encode-invoke-args.mjs
// Bridges the prover's JSON output (decimal field elements + the `soroban`
// formatted proof) into the argument shape `stellar contract invoke --arg-file`
// expects for the Aegis contracts.
//
// Usage:
//   node scripts/encode-invoke-args.mjs <proof.json>                       # print derived be32 fields
//   node scripts/encode-invoke-args.mjs <reserves_proof.json> attest       # -> attest args (stdout JSON)
//   node scripts/encode-invoke-args.mjs <eligibility_proof.json> authorize_receive --token T --receiver R
//
// The Groth16Proof / PublicSignals Soroban types are:
//   Groth16Proof { a: BytesN<64>, b: BytesN<128>, c: BytesN<64> }
//   PublicSignals = Vec<BytesN<32>>
// produced verbatim by prover/src/soroban-format.js.

import { readFileSync } from "node:fs";

const FP = 32;
function toBe32Hex(decOrHex) {
  let v = BigInt(decOrHex);
  if (v < 0n) throw new Error("negative field element");
  let h = v.toString(16);
  if (h.length > FP * 2) throw new Error("field element exceeds 32 bytes");
  return h.padStart(FP * 2, "0");
}

function argOf(name, def) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 ? process.argv[i + 1] : def;
}

const file = process.argv[2];
const mode = process.argv[3]; // undefined | "attest" | "authorize_receive"
if (!file) {
  console.error("usage: encode-invoke-args.mjs <proof.json> [attest|authorize_receive] [--token T --receiver R]");
  process.exit(1);
}

const data = JSON.parse(readFileSync(file, "utf8"));
const sor = data.soroban;
if (!sor || !sor.proof) throw new Error(`${file} missing .soroban.proof — regenerate with the current prover`);

// The Groth16Proof struct (hex strings without 0x for --arg-file friendliness).
const strip = (h) => (h.startsWith("0x") ? h.slice(2) : h);
const proofStruct = {
  a: strip(sor.proof.a),
  b: strip(sor.proof.b),
  c: strip(sor.proof.c),
};
// PublicSignals: Vec<BytesN<32>> as bare hex.
const signals = sor.publicSignals.map(strip);

if (!mode) {
  // Diagnostic: print the be32 values a human needs for set_policy / set_gate.
  const out = {
    publicSignals: signals,
    proof: proofStruct,
  };
  if (data.reservesCommitment) out.reservesCommitmentBe32 = toBe32Hex(data.reservesCommitment);
  if (data.allowedJurisdictionRoot) out.allowedJurisdictionRootBe32 = toBe32Hex(data.allowedJurisdictionRoot);
  if (data.nullifier) out.nullifierBe32 = toBe32Hex(data.nullifier);
  console.log(JSON.stringify(out, null, 2));
  process.exit(0);
}

if (mode === "attest") {
  // attest(token, claimed_supply, proof, signals) — token/supply are filled by the
  // shell caller; here we emit the proof + signals portion as an arg-file object.
  const supply = BigInt(data.publicSignalsRaw?.[0] ?? "0").toString();
  console.log(
    JSON.stringify(
      {
        claimed_supply: supply,
        proof: proofStruct,
        signals,
      },
      null,
      2
    )
  );
  process.exit(0);
}

if (mode === "authorize_receive") {
  const token = argOf("token");
  const receiver = argOf("receiver");
  if (!token || !receiver) throw new Error("authorize_receive needs --token and --receiver");
  console.log(
    JSON.stringify(
      {
        token,
        receiver,
        eligibility_proof: proofStruct,
        eligibility_signals: signals,
      },
      null,
      2
    )
  );
  process.exit(0);
}

throw new Error(`unknown mode: ${mode}`);

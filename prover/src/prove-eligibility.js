// src/prove-eligibility.js
// Investor-side: takes a signed credential.json + the issuer allowlist.json and
// produces a Groth16 eligibility proof revealing only `eligible` + a nullifier.
//
// Usage:
//   node src/prove-eligibility.js \
//     --credential credential.json --allowlist allowlist.json \
//     --requiredKyc 2 --requireAccredited 1 --actionId 777 \
//     --wasm ../build/eligibility_js/eligibility.wasm \
//     --zkey ../build/eligibility_final.zkey \
//     --out eligibility_proof.json

import { groth16 } from "snarkjs";
import { poseidonHash, toFieldStr } from "./field.js";
import { buildAllowlistTree } from "./merkle.js";
import { formatProofForSoroban } from "./soroban-format.js";
import { readFileSync, writeFileSync } from "node:fs";

const MERKLE_DEPTH = 16; // must match Eligibility(16)

function arg(name, def) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 ? process.argv[i + 1] : def;
}

export async function buildEligibilityInput({ credential, allowlist, requiredKyc, requireAccredited, currentTimestamp, actionId }) {
  // Rebuild the allowlist tree and locate the holder's jurisdiction.
  const codes = allowlist.codes.map((c) => BigInt(c));
  const tree = await buildAllowlistTree(codes, allowlist.depth ?? MERKLE_DEPTH);
  const idx = tree.indexOf(BigInt(credential.jurisdictionCode));
  if (idx < 0) {
    throw new Error(
      `jurisdiction ${credential.jurisdictionCode} is NOT in the issuer allowlist — investor is ineligible.`
    );
  }
  const { pathElements, pathIndices } = tree.proof(idx);

  const nullifier = await poseidonHash([BigInt(credential.credentialSecret), BigInt(actionId)]);

  const input = {
    // public
    issuerPubKeyX: credential.issuerPubKeyX,
    issuerPubKeyY: credential.issuerPubKeyY,
    requiredKycLevel: toFieldStr(requiredKyc),
    requireAccredited: toFieldStr(requireAccredited),
    allowedJurisdictionRoot: tree.root.toString(),
    currentTimestamp: toFieldStr(currentTimestamp),
    actionId: toFieldStr(actionId),
    nullifier: nullifier.toString(),
    // private credential
    kycLevel: credential.kycLevel,
    jurisdictionCode: credential.jurisdictionCode,
    accredited: credential.accredited,
    expiry: credential.expiry,
    credentialSecret: credential.credentialSecret,
    // private signature
    sigR8x: credential.sigR8x,
    sigR8y: credential.sigR8y,
    sigS: credential.sigS,
    // private merkle path
    jurPathElements: pathElements.map((e) => e.toString()),
    jurPathIndices: pathIndices.map((i) => String(i)),
  };

  return { input, nullifier, root: tree.root };
}

async function main() {
  const credential = JSON.parse(readFileSync(arg("credential", "credential.json"), "utf8"));
  const allowlist = JSON.parse(readFileSync(arg("allowlist", "allowlist.json"), "utf8"));
  const requiredKyc = BigInt(arg("requiredKyc", "2"));
  const requireAccredited = BigInt(arg("requireAccredited", "1"));
  const actionId = BigInt(arg("actionId", "777"));
  const currentTimestamp = BigInt(arg("now", String(Math.floor(Date.now() / 1000))));
  const wasm = arg("wasm", "../build/eligibility_js/eligibility.wasm");
  const zkey = arg("zkey", "../build/eligibility_final.zkey");
  const out = arg("out", "eligibility_proof.json");

  const { input, nullifier, root } = await buildEligibilityInput({
    credential, allowlist, requiredKyc, requireAccredited, currentTimestamp, actionId,
  });

  console.log("→ generating witness + proof…");
  const { proof, publicSignals } = await groth16.fullProve(input, wasm, zkey);

  const formatted = formatProofForSoroban(proof, publicSignals);
  const result = {
    kind: "eligibility",
    eligible: true, // proof only builds if all constraints pass
    nullifier: nullifier.toString(),
    allowedJurisdictionRoot: root.toString(),
    publicSignalsRaw: publicSignals,
    soroban: formatted,
    rawProof: proof,
  };
  writeFileSync(out, JSON.stringify(result, null, 2));
  console.log(`✓ Eligibility proof → ${out}`);
  console.log(`  nullifier (publish on-chain to prevent reuse): ${nullifier.toString()}`);
}

if (import.meta.url === `file://${process.argv[1]}`) {
  main().catch((e) => {
    console.error(e);
    process.exit(1);
  });
}

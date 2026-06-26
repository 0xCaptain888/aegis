// src/issue-credential.js
// Run by the AUTHORIZED ISSUER (e.g. a KYC provider). Produces a signed
// verifiable credential the investor later uses to generate an eligibility proof.
//
// Usage:
//   node src/issue-credential.js --kyc 2 --jurisdiction 840 --accredited 1 \
//        --expiry 1788000000 --out credential.json
//
// The issuer's private key lives ONLY here. The output credential.json is given
// to the investor and contains NO issuer secret.

import { getEddsa, getPoseidon, poseidonHash } from "./field.js";
import { writeFileSync } from "node:fs";
import crypto from "node:crypto";

function arg(name, def) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 ? process.argv[i + 1] : def;
}

// In production the issuer key is loaded from an HSM / env secret. For the demo
// we derive it from ISSUER_SEED (env) or a fixed dev seed so results are
// reproducible. NEVER use the dev seed in production.
function issuerPrivateKey() {
  const seed = process.env.ISSUER_SEED || "aegis-dev-issuer-seed-DO-NOT-USE-IN-PROD";
  return crypto.createHash("sha256").update(seed).digest(); // 32 bytes
}

export async function issueCredential({ kycLevel, jurisdictionCode, accredited, expiry, credentialSecret }) {
  const eddsa = await getEddsa();
  const poseidon = await getPoseidon();
  const F = poseidon.F;

  const prv = issuerPrivateKey();
  const pub = eddsa.prv2pub(prv);
  const issuerPubKeyX = F.toObject(pub[0]);
  const issuerPubKeyY = F.toObject(pub[1]);

  // commit to the holder's secret inside the signed payload
  const secretCommit = await poseidonHash([credentialSecret]);

  const credHash = await poseidonHash([
    kycLevel,
    jurisdictionCode,
    accredited,
    expiry,
    secretCommit,
  ]);

  const sig = eddsa.signPoseidon(prv, F.e(credHash));

  return {
    // public-ish credential fields (held privately by the investor, signed by issuer)
    kycLevel: String(kycLevel),
    jurisdictionCode: String(jurisdictionCode),
    accredited: String(accredited),
    expiry: String(expiry),
    credentialSecret: String(credentialSecret),
    // issuer identity (public)
    issuerPubKeyX: issuerPubKeyX.toString(),
    issuerPubKeyY: issuerPubKeyY.toString(),
    // signature
    sigR8x: F.toObject(sig.R8[0]).toString(),
    sigR8y: F.toObject(sig.R8[1]).toString(),
    sigS: sig.S.toString(),
    credentialHash: credHash.toString(),
  };
}

async function main() {
  const kycLevel = BigInt(arg("kyc", "2"));
  const jurisdictionCode = BigInt(arg("jurisdiction", "840")); // ISO 3166 numeric, 840 = US
  const accredited = BigInt(arg("accredited", "1"));
  const expiry = BigInt(arg("expiry", String(Math.floor(Date.now() / 1000) + 365 * 24 * 3600)));
  // holder secret: random unless provided (for reproducible demos)
  const credentialSecret = BigInt(
    arg("secret", "0x" + crypto.randomBytes(31).toString("hex"))
  );
  const out = arg("out", "credential.json");

  const cred = await issueCredential({ kycLevel, jurisdictionCode, accredited, expiry, credentialSecret });
  writeFileSync(out, JSON.stringify(cred, null, 2));
  console.log(`✓ Issued credential → ${out}`);
  console.log(`  issuerPubKey = (${cred.issuerPubKeyX.slice(0, 12)}…, ${cred.issuerPubKeyY.slice(0, 12)}…)`);
}

if (import.meta.url === `file://${process.argv[1]}`) {
  main().catch((e) => {
    console.error(e);
    process.exit(1);
  });
}

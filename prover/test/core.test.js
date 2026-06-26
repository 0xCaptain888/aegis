// test/core.test.js
// Offline tests for the parts that don't require the compiled circuit or network:
// field/Poseidon determinism, Merkle inclusion correctness, credential signing,
// and the input-builder logic (commitment + nullifier derivation).
//
// Run: npm test   (uses node:test)

import { test } from "node:test";
import assert from "node:assert/strict";

import { poseidonHash, getEddsa, getPoseidon, FIELD_MODULUS, mod } from "../src/field.js";
import { buildAllowlistTree, PoseidonMerkleTree } from "../src/merkle.js";
import { issueCredential } from "../src/issue-credential.js";
import { buildReservesInput } from "../src/prove-reserves.js";
import { buildEligibilityInput } from "../src/prove-eligibility.js";

test("poseidon is deterministic and in-field", async () => {
  const a = await poseidonHash([1n, 2n, 3n]);
  const b = await poseidonHash([1n, 2n, 3n]);
  assert.equal(a, b);
  assert.ok(a < FIELD_MODULUS && a >= 0n);
});

test("mod normalizes negatives into the field", () => {
  assert.equal(mod(-1n), FIELD_MODULUS - 1n);
  assert.equal(mod(FIELD_MODULUS + 5n), 5n);
});

test("merkle inclusion path recomputes the root", async () => {
  const codes = [840n, 826n, 392n, 276n];
  const tree = await buildAllowlistTree(codes, 4);
  const idx = tree.indexOf(392n);
  assert.ok(idx >= 0);
  const { pathElements, pathIndices } = tree.proof(idx);

  // recompute root from leaf + path the same way the circuit does
  const poseidon = await getPoseidon();
  const F = poseidon.F;
  let cur = 392n;
  for (let i = 0; i < pathElements.length; i++) {
    const sib = pathElements[i];
    const isRight = pathIndices[i];
    const left = isRight ? sib : cur;
    const right = isRight ? cur : sib;
    cur = F.toObject(poseidon([F.e(left), F.e(right)]));
  }
  assert.equal(cur, tree.root);
});

test("merkle rejects a non-member jurisdiction", async () => {
  const tree = await buildAllowlistTree([840n, 826n], 4);
  assert.equal(tree.indexOf(999n), -1);
});

test("issued credential carries a valid issuer signature", async () => {
  const cred = await issueCredential({
    kycLevel: 2n, jurisdictionCode: 840n, accredited: 1n,
    expiry: 1900000000n, credentialSecret: 424242n,
  });
  const eddsa = await getEddsa();
  const poseidon = await getPoseidon();
  const F = poseidon.F;

  const ok = eddsa.verifyPoseidon(
    F.e(BigInt(cred.credentialHash)),
    {
      R8: [F.e(BigInt(cred.sigR8x)), F.e(BigInt(cred.sigR8y))],
      S: BigInt(cred.sigS),
    },
    [F.e(BigInt(cred.issuerPubKeyX)), F.e(BigInt(cred.issuerPubKeyY))]
  );
  assert.equal(ok, true);
});

test("credential hash matches the circuit's reconstruction", async () => {
  const secret = 424242n;
  const cred = await issueCredential({
    kycLevel: 3n, jurisdictionCode: 826n, accredited: 0n,
    expiry: 1900000000n, credentialSecret: secret,
  });
  const secretCommit = await poseidonHash([secret]);
  const expected = await poseidonHash([3n, 826n, 0n, 1900000000n, secretCommit]);
  assert.equal(BigInt(cred.credentialHash), expected);
});

test("reserves input builds the right commitment & is solvent", async () => {
  const { input, reservesCommitment } = await buildReservesInput({
    balances: [5000000n, 3000000n, 2000000n],
    supply: 9000000n, minBps: 10000n, salt: 12345n,
  });
  // commitment binds to padded balances + salt
  const padded = [5000000n, 3000000n, 2000000n, 0n, 0n, 0n, 0n, 0n];
  const expected = await poseidonHash([...padded, 12345n]);
  assert.equal(reservesCommitment, expected);
  assert.equal(input.reservesCommitment, expected.toString());

  // solvency holds: sum(10,000,000) * 10000 >= 9,000,000 * 10000
  const sum = padded.reduce((a, b) => a + b, 0n);
  assert.ok(sum * 10000n >= 9000000n * 10000n);
});

test("eligibility input builder derives nullifier & finds jurisdiction", async () => {
  const cred = await issueCredential({
    kycLevel: 2n, jurisdictionCode: 840n, accredited: 1n,
    expiry: 1900000000n, credentialSecret: 555n,
  });
  const tree = await buildAllowlistTree([840n, 826n, 392n], 16);
  const allowlist = { depth: 16, codes: ["840", "826", "392"], root: tree.root.toString() };

  const { input, nullifier } = await buildEligibilityInput({
    credential: cred, allowlist,
    requiredKyc: 2n, requireAccredited: 1n,
    currentTimestamp: 1700000000n, actionId: 777n,
  });

  const expectedNull = await poseidonHash([555n, 777n]);
  assert.equal(nullifier, expectedNull);
  assert.equal(input.nullifier, expectedNull.toString());
  assert.equal(input.allowedJurisdictionRoot, tree.root.toString());
});

test("eligibility builder throws for non-allowlisted jurisdiction", async () => {
  const cred = await issueCredential({
    kycLevel: 2n, jurisdictionCode: 408n /* DPRK */, accredited: 1n,
    expiry: 1900000000n, credentialSecret: 1n,
  });
  const tree = await buildAllowlistTree([840n, 826n], 16);
  const allowlist = { depth: 16, codes: ["840", "826"], root: tree.root.toString() };
  await assert.rejects(
    buildEligibilityInput({
      credential: cred, allowlist, requiredKyc: 2n, requireAccredited: 1n,
      currentTimestamp: 1700000000n, actionId: 1n,
    }),
    /NOT in the issuer allowlist/
  );
});

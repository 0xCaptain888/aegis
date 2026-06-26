pragma circom 2.1.9;

include "../lib/poseidon.circom";
include "../lib/comparators.circom";
include "../lib/eddsaposeidon.circom";

/*
 * Aegis — ZK Investor Eligibility (Selective Disclosure)
 * ----------------------------------------------------------------------------
 * Statement proven (in zero knowledge):
 *   "An authorized issuer signed a credential about me that asserts:
 *      - my KYC level >= required level,
 *      - my jurisdiction is NOT in the restricted set,
 *      - I am an accredited investor (if required),
 *      - the credential has not expired,
 *    and I am the holder of that credential — WITHOUT revealing my identity,
 *    exact jurisdiction, birth date, or any document."
 *
 * The ONLY thing disclosed to the verifier is a single boolean: eligible == 1,
 * plus a per-credential nullifier that prevents one credential from being reused
 * across distinct gated actions (sybil / double-use resistance) while remaining
 * unlinkable to the holder's identity.
 *
 * Public inputs:
 *   - issuerPubKeyX, issuerPubKeyY : the authorized credential issuer's EdDSA public key
 *   - requiredKycLevel             : minimum KYC level the gate demands
 *   - requireAccredited            : 1 if the gate requires accredited-investor status
 *   - restrictedJurisdictionRoot   : Poseidon-Merkle root of restricted-jurisdiction codes
 *   - currentTimestamp             : unix time supplied by the gate (for expiry)
 *   - actionId                     : identifier of the gated action (binds the nullifier)
 *   - nullifier                    : Poseidon(credentialSecret, actionId) — published
 *
 * Private inputs (the signed credential + holder secret + a non-membership path):
 *   - kycLevel, jurisdictionCode, accredited (0/1), expiry, credentialSecret
 *   - sigR8x, sigR8y, sigS        : issuer's EdDSA-Poseidon signature over the credential hash
 *   - nonMembershipPath..., nonMembershipIndices...  : Merkle path proving jurisdiction is
 *                                   NOT one of the restricted leaves (we prove the leaf at the
 *                                   path position differs and the path is valid for a *different*
 *                                   leaf — see README for the simplified non-membership model).
 *
 * Simplification for the hackathon scope (documented honestly in the README):
 *   Restricted-jurisdiction handling is implemented as an explicit ALLOWLIST membership
 *   proof: the issuer commits to an allowlist Merkle root of PERMITTED jurisdiction codes,
 *   and the holder proves membership. This is sound and far simpler than generic
 *   non-membership, and matches how Stellar's ASP allow/deny sets work in practice.
 */

template MerkleInclusion(depth) {
    signal input leaf;
    signal input root;
    signal input pathElements[depth];
    signal input pathIndices[depth]; // 0 = current node is left, 1 = current node is right

    signal hashes[depth + 1];
    hashes[0] <== leaf;

    component hashers[depth];
    // left/right operands per level, declared as arrays (no in-loop redeclaration)
    signal left[depth];
    signal right[depth];

    for (var i = 0; i < depth; i++) {
        // pathIndices[i] must be boolean
        pathIndices[i] * (pathIndices[i] - 1) === 0;

        hashers[i] = Poseidon(2);
        // if index == 0: (cur, sibling); if index == 1: (sibling, cur)
        left[i]  <== hashes[i] + pathIndices[i] * (pathElements[i] - hashes[i]);
        right[i] <== pathElements[i] + pathIndices[i] * (hashes[i] - pathElements[i]);
        hashers[i].inputs[0] <== left[i];
        hashers[i].inputs[1] <== right[i];
        hashes[i + 1] <== hashers[i].out;
    }

    root === hashes[depth];
}

template Eligibility(merkleDepth) {
    // ---- Public ----
    signal input issuerPubKeyX;
    signal input issuerPubKeyY;
    signal input requiredKycLevel;
    signal input requireAccredited;          // 0 or 1
    signal input allowedJurisdictionRoot;    // Merkle root of PERMITTED jurisdiction codes
    signal input currentTimestamp;
    signal input actionId;
    signal input nullifier;

    // ---- Private credential fields ----
    signal input kycLevel;
    signal input jurisdictionCode;
    signal input accredited;                 // 0 or 1
    signal input expiry;                     // unix time
    signal input credentialSecret;

    // ---- Private issuer signature over credential hash ----
    signal input sigR8x;
    signal input sigR8y;
    signal input sigS;

    // ---- Private Merkle path proving jurisdiction is in the allowlist ----
    signal input jurPathElements[merkleDepth];
    signal input jurPathIndices[merkleDepth];

    // ---- 1. Reconstruct the credential hash the issuer signed ----
    // credentialHash = Poseidon(kycLevel, jurisdictionCode, accredited, expiry, credentialSecretCommit)
    // We commit to the secret (not the raw secret) inside the signed payload so the issuer
    // never learns the holder's spending secret.
    component secretCommit = Poseidon(1);
    secretCommit.inputs[0] <== credentialSecret;

    component credHash = Poseidon(5);
    credHash.inputs[0] <== kycLevel;
    credHash.inputs[1] <== jurisdictionCode;
    credHash.inputs[2] <== accredited;
    credHash.inputs[3] <== expiry;
    credHash.inputs[4] <== secretCommit.out;

    // ---- 2. Verify issuer's EdDSA-Poseidon signature over credentialHash ----
    component sig = EdDSAPoseidonVerifier();
    sig.enabled <== 1;
    sig.Ax  <== issuerPubKeyX;
    sig.Ay  <== issuerPubKeyY;
    sig.R8x <== sigR8x;
    sig.R8y <== sigR8y;
    sig.S   <== sigS;
    sig.M   <== credHash.out;

    // ---- 3. KYC level >= required ----
    component kycOk = GreaterEqThan(16);
    kycOk.in[0] <== kycLevel;
    kycOk.in[1] <== requiredKycLevel;
    kycOk.out === 1;

    // ---- 4. Accreditation: if requireAccredited==1 then accredited must be 1 ----
    accredited * (accredited - 1) === 0;             // boolean
    requireAccredited * (requireAccredited - 1) === 0;
    // requireAccredited => accredited  ==>  requireAccredited * (1 - accredited) == 0
    requireAccredited * (1 - accredited) === 0;

    // ---- 5. Not expired: expiry > currentTimestamp ----
    component notExpired = GreaterThan(64);
    notExpired.in[0] <== expiry;
    notExpired.in[1] <== currentTimestamp;
    notExpired.out === 1;

    // ---- 6. Jurisdiction is in the issuer-committed allowlist ----
    component inc = MerkleInclusion(merkleDepth);
    inc.leaf <== jurisdictionCode;
    inc.root <== allowedJurisdictionRoot;
    for (var i = 0; i < merkleDepth; i++) {
        inc.pathElements[i] <== jurPathElements[i];
        inc.pathIndices[i]  <== jurPathIndices[i];
    }

    // ---- 7. Nullifier binding: nullifier == Poseidon(credentialSecret, actionId) ----
    component nh = Poseidon(2);
    nh.inputs[0] <== credentialSecret;
    nh.inputs[1] <== actionId;
    nullifier === nh.out;
}

// Default: Merkle depth 16 (allowlist of up to 65,536 jurisdiction codes).
component main {
    public [
        issuerPubKeyX, issuerPubKeyY, requiredKycLevel, requireAccredited,
        allowedJurisdictionRoot, currentTimestamp, actionId, nullifier
    ]
} = Eligibility(16);

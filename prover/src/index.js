// src/index.js — public API of @aegis/prover
export { getPoseidon, getEddsa, poseidonHash, toFieldStr, mod, FIELD_MODULUS } from "./field.js";
export { PoseidonMerkleTree, buildAllowlistTree } from "./merkle.js";
export { issueCredential } from "./issue-credential.js";
export { buildReservesInput } from "./prove-reserves.js";
export { buildEligibilityInput } from "./prove-eligibility.js";
export { formatProofForSoroban, formatVkForSoroban, G2_FP2_ORDER } from "./soroban-format.js";

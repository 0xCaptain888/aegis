// src/field.js
// Shared field helpers built on circomlibjs (Poseidon + EdDSA over BabyJubjub)
// and the BN254 scalar field used by Circom/Groth16 on Stellar.

import { buildPoseidon, buildEddsa } from "circomlibjs";

// BN254 (alt_bn128) scalar field modulus — the field Circom signals live in.
export const FIELD_MODULUS =
  21888242871839275222246405745257275088548364400416034343698204186575808495617n;

let _poseidon = null;
let _eddsa = null;

export async function getPoseidon() {
  if (!_poseidon) _poseidon = await buildPoseidon();
  return _poseidon;
}

export async function getEddsa() {
  if (!_eddsa) _eddsa = await buildEddsa();
  return _eddsa;
}

// Poseidon hash returning a canonical bigint in the field.
export async function poseidonHash(inputs) {
  const poseidon = await getPoseidon();
  const F = poseidon.F;
  const h = poseidon(inputs.map((x) => F.e(x)));
  return F.toObject(h); // bigint
}

export function mod(x) {
  const m = ((x % FIELD_MODULUS) + FIELD_MODULUS) % FIELD_MODULUS;
  return m;
}

// Convert any bigint-ish to a decimal string snarkjs expects in the witness input.
export function toFieldStr(x) {
  return mod(BigInt(x)).toString();
}

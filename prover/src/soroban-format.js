// src/soroban-format.js
// Converts a snarkjs Groth16 proof + verification key into the exact byte
// encoding the Stellar `groth16_verifier` Soroban contract expects.
//
// Stellar Protocol 25 (X-Ray) exposes BN254 host functions. The community
// verifier (stellar/soroban-examples/groth16_verifier and the RISC Zero /
// Nethermind verifiers) consume G1/G2 points as big-endian uncompressed
// coordinates. We produce:
//   - proof: { a: G1, b: G2, c: G1 }   as 0x-hex byte strings
//   - publicSignals: Fr scalars as 32-byte big-endian
//   - vk: alpha/beta/gamma/delta + IC[] points
//
// IMPORTANT (documented honestly): the precise field-element endianness and the
// G2 coordinate ordering (c0,c1 vs c1,c0) differ between verifier builds. This
// module centralizes that mapping; `G2_FP2_ORDER` is the single knob to flip if
// the on-chain verifier rejects a proof that verifies fine off-chain. See
// docs/UPGRADE.md "Calibrating the Soroban encoding".

const FP_BYTES = 32;

export const G2_FP2_ORDER = "c1c0"; // try "c0c1" if on-chain verification fails

function toBE32(xDecOrHex) {
  let v = BigInt(xDecOrHex);
  if (v < 0n) throw new Error("negative field element");
  const bytes = new Uint8Array(FP_BYTES);
  for (let i = FP_BYTES - 1; i >= 0; i--) {
    bytes[i] = Number(v & 0xffn);
    v >>= 8n;
  }
  if (v !== 0n) throw new Error("field element exceeds 32 bytes");
  return bytes;
}

function hex(bytes) {
  return (
    "0x" +
    Array.from(bytes)
      .map((b) => b.toString(16).padStart(2, "0"))
      .join("")
  );
}

function concat(...arrs) {
  const total = arrs.reduce((n, a) => n + a.length, 0);
  const out = new Uint8Array(total);
  let off = 0;
  for (const a of arrs) {
    out.set(a, off);
    off += a.length;
  }
  return out;
}

// snarkjs G1 point is [x, y, "1"] (projective normalized). We take affine x,y.
function g1Bytes(p) {
  return concat(toBE32(p[0]), toBE32(p[1]));
}

// snarkjs G2 point is [[x_c0, x_c1], [y_c0, y_c1], ["1","0"]].
function g2Bytes(p) {
  const x0 = toBE32(p[0][0]);
  const x1 = toBE32(p[0][1]);
  const y0 = toBE32(p[1][0]);
  const y1 = toBE32(p[1][1]);
  if (G2_FP2_ORDER === "c1c0") {
    return concat(x1, x0, y1, y0);
  }
  return concat(x0, x1, y0, y1);
}

export function formatProofForSoroban(proof, publicSignals) {
  const a = g1Bytes(proof.pi_a);
  const b = g2Bytes(proof.pi_b);
  const c = g1Bytes(proof.pi_c);

  const pub = publicSignals.map((s) => hex(toBE32(s)));

  return {
    proof: { a: hex(a), b: hex(b), c: hex(c) },
    publicSignals: pub,
    // Raw concatenated bytes some verifiers ingest as a single blob:
    proofBytes: hex(concat(a, b, c)),
  };
}

export function formatVkForSoroban(vk) {
  return {
    alpha: hex(g1Bytes(vk.vk_alpha_1)),
    beta: hex(g2Bytes(vk.vk_beta_2)),
    gamma: hex(g2Bytes(vk.vk_gamma_2)),
    delta: hex(g2Bytes(vk.vk_delta_2)),
    ic: vk.IC.map((p) => hex(g1Bytes(p))),
    nPublic: vk.nPublic,
  };
}

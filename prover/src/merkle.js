// src/merkle.js
// A Poseidon-based binary Merkle tree matching the circuit's MerkleInclusion template.
// Leaves are jurisdiction codes (bigint). Tree is fixed-depth and zero-padded.

import { getPoseidon } from "./field.js";

export class PoseidonMerkleTree {
  constructor(depth) {
    this.depth = depth;
    this.poseidon = null;
    this.F = null;
    this.leaves = [];
    this.layers = [];
  }

  async init() {
    this.poseidon = await getPoseidon();
    this.F = this.poseidon.F;
    return this;
  }

  _h2(a, b) {
    return this.F.toObject(this.poseidon([this.F.e(a), this.F.e(b)]));
  }

  // Build from an array of bigint leaves; pads with 0 up to 2^depth.
  build(leaves) {
    const size = 2 ** this.depth;
    if (leaves.length > size) {
      throw new Error(`too many leaves: ${leaves.length} > ${size}`);
    }
    const padded = leaves.map((x) => BigInt(x));
    while (padded.length < size) padded.push(0n);

    this.leaves = padded;
    this.layers = [padded];
    let cur = padded;
    for (let d = 0; d < this.depth; d++) {
      const next = [];
      for (let i = 0; i < cur.length; i += 2) {
        next.push(this._h2(cur[i], cur[i + 1]));
      }
      this.layers.push(next);
      cur = next;
    }
    return this;
  }

  get root() {
    return this.layers[this.depth][0];
  }

  // Returns { pathElements: bigint[], pathIndices: number[] } for leaf at index.
  proof(index) {
    if (index < 0 || index >= this.leaves.length) {
      throw new Error(`leaf index out of range: ${index}`);
    }
    const pathElements = [];
    const pathIndices = [];
    let idx = index;
    for (let d = 0; d < this.depth; d++) {
      const isRight = idx % 2; // if current node is the right child
      const siblingIdx = isRight ? idx - 1 : idx + 1;
      pathElements.push(this.layers[d][siblingIdx]);
      pathIndices.push(isRight); // 1 if current is right, matching circuit convention
      idx = Math.floor(idx / 2);
    }
    return { pathElements, pathIndices };
  }

  indexOf(code) {
    const target = BigInt(code);
    return this.leaves.findIndex((x) => x === target);
  }
}

export async function buildAllowlistTree(codes, depth = 16) {
  const t = new PoseidonMerkleTree(depth);
  await t.init();
  t.build(codes.map((c) => BigInt(c)));
  return t;
}

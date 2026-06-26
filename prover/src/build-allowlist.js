// src/build-allowlist.js
// Builds the issuer's PERMITTED-jurisdiction allowlist Merkle tree and prints
// the root that gets published on-chain as a public input to eligibility proofs.
//
// Usage:
//   node src/build-allowlist.js --codes 840,826,392,276 --out allowlist.json

import { buildAllowlistTree } from "./merkle.js";
import { writeFileSync } from "node:fs";

function arg(name, def) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 ? process.argv[i + 1] : def;
}

async function main() {
  // Default: a few non-restricted ISO-3166 numeric country codes.
  const codes = arg("codes", "840,826,392,276,250,724")
    .split(",")
    .map((s) => BigInt(s.trim()));
  const depth = Number(arg("depth", "16"));
  const out = arg("out", "allowlist.json");

  const tree = await buildAllowlistTree(codes, depth);
  const data = {
    depth,
    codes: codes.map((c) => c.toString()),
    root: tree.root.toString(),
  };
  writeFileSync(out, JSON.stringify(data, null, 2));
  console.log(`✓ Allowlist built (${codes.length} codes, depth ${depth}) → ${out}`);
  console.log(`  root = ${tree.root.toString()}`);
}

if (import.meta.url === `file://${process.argv[1]}`) {
  main().catch((e) => {
    console.error(e);
    process.exit(1);
  });
}

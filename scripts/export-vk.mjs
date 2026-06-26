#!/usr/bin/env node
// scripts/export-vk.mjs <vkey.json> <out_soroban.json>
// Converts a snarkjs verification key into the Soroban byte layout.
import { readFileSync, writeFileSync } from "node:fs";
import { formatVkForSoroban } from "../prover/src/soroban-format.js";

const [, , inPath, outPath] = process.argv;
if (!inPath || !outPath) {
  console.error("usage: export-vk.mjs <vkey.json> <out_soroban.json>");
  process.exit(1);
}
const vk = JSON.parse(readFileSync(inPath, "utf8"));
const formatted = formatVkForSoroban(vk);
writeFileSync(outPath, JSON.stringify(formatted, null, 2));
console.log(`✓ Soroban VK → ${outPath} (nPublic=${formatted.nPublic}, IC=${formatted.ic.length})`);

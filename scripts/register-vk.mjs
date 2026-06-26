// scripts/register-vk.mjs
// Reads a *_vk_soroban.json file (produced by export-vk.mjs) and calls
// groth16_bn254_verifier.register_vk() on the deployed testnet contract.
//
// Usage:
//   node scripts/register-vk.mjs \
//     --verifier <CONTRACT_ID> --vk-id 0 --vk-file build/proof_of_reserves_vk_soroban.json \
//     --source aegis-deployer --network testnet

import { readFileSync } from "node:fs";
import { execSync } from "node:child_process";

function arg(name) {
  const i = process.argv.indexOf(`--${name}`);
  if (i < 0) throw new Error(`Missing --${name}`);
  return process.argv[i + 1];
}

const verifier = arg("verifier");
const vkId = parseInt(arg("vk-id"), 10);
const vkFile = arg("vk-file");
const source = arg("source");
const network = arg("network");

const vk = JSON.parse(readFileSync(vkFile, "utf8"));

// The VK soroban format from export-vk.mjs:
// { alpha: "0x...", beta: "0x...", gamma: "0x...", delta: "0x...", ic: ["0x...", ...], nPublic: N }
//
// Stellar CLI --arg-file passes a JSON object to `stellar contract invoke`.
// The register_vk function takes: vk_id, alpha, beta, gamma, delta, ic.
// BytesN<N> is passed as a hex string without 0x prefix.

function strip(hex) {
  return hex.startsWith("0x") ? hex.slice(2) : hex;
}

const argObj = {
  vk_id: vkId,
  alpha: strip(vk.alpha),
  beta: strip(vk.beta),
  gamma: strip(vk.gamma),
  delta: strip(vk.delta),
  ic: vk.ic.map(strip),
};

const tmpFile = `/tmp/register_vk_${vkId}_${Date.now()}.json`;
import { writeFileSync } from "node:fs";
writeFileSync(tmpFile, JSON.stringify(argObj, null, 2));

console.log(`Registering VK ${vkId} with ${verifier}…`);
const cmd = [
  "stellar contract invoke",
  `--id "${verifier}"`,
  `--source "${source}"`,
  `--network "${network}"`,
  `--arg-file "${tmpFile}"`,
  "-- register_vk",
].join(" ");

try {
  const out = execSync(cmd, { stdio: ["pipe", "pipe", "pipe"] });
  console.log(`✓ VK ${vkId} registered. Response: ${out.toString().trim()}`);
} catch (e) {
  console.error(`✗ register_vk failed:\n${e.stderr?.toString()}`);
  process.exit(1);
} finally {
  try { import("node:fs").then(fs => fs.default.unlinkSync(tmpFile)); } catch {}
}

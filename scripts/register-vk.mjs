// scripts/register-vk.mjs
// 读取 export-vk.mjs 生成的 *_vk_soroban.json，调用已部署的
// groth16_bn254_verifier.register_vk()。
//
// 用法:
//   node scripts/register-vk.mjs \
//     --verifier <CONTRACT_ID> --vk-id 0 --vk-file build/proof_of_reserves_vk_soroban.json \
//     --source aegis-deployer --network testnet
//
// 注意（stellar CLI ≥ 27.0）：
//   `--arg-file` 已被移除。复杂类型（结构体、Vec<T>）现在直接以
//   `--arg-name '<JSON 字符串>'` 形式传给 CLI，由 CLI 自己解析 JSON。
//   register_vk 的签名是:
//     fn register_vk(vk_id: u32, alpha: BytesN<64>, beta: BytesN<128>,
//                     gamma: BytesN<128>, delta: BytesN<128>,
//                     ic: Vec<BytesN<64>>)
//   标量/BytesN<N> 直接传十六进制字符串（不带 0x 前缀）；
//   Vec<BytesN<64>> 传一个 JSON 数组字符串，如 '["aa..","bb.."]'。

import { readFileSync } from "node:fs";
import { execFileSync } from "node:child_process";

function arg(name) {
  const i = process.argv.indexOf(`--${name}`);
  if (i < 0) throw new Error(`Missing --${name}`);
  return process.argv[i + 1];
}

const verifier = arg("verifier");
const vkId = arg("vk-id");
const vkFile = arg("vk-file");
const source = arg("source");
const network = arg("network");

const vk = JSON.parse(readFileSync(vkFile, "utf8"));

function strip(hex) {
  return hex.startsWith("0x") ? hex.slice(2) : hex;
}

const alpha = strip(vk.alpha);
const beta = strip(vk.beta);
const gamma = strip(vk.gamma);
const delta = strip(vk.delta);
const icJson = JSON.stringify(vk.ic.map(strip)); // -> '["aa..","bb.."]'

console.log(`Registering VK ${vkId} with ${verifier}…`);

// 用 execFileSync（而非 execSync + 拼接字符串）避免 shell 转义问题——
// JSON 数组字符串里含有引号和方括号，拼进 shell 命令很容易出错。
const args = [
  "contract", "invoke",
  "--id", verifier,
  "--source", source,
  "--network", network,
  "--",
  "register_vk",
  "--vk_id", String(vkId),
  "--alpha", alpha,
  "--beta", beta,
  "--gamma", gamma,
  "--delta", delta,
  "--ic", icJson,
];

try {
  const out = execFileSync("stellar", args, { stdio: ["pipe", "pipe", "pipe"] });
  console.log(`✓ VK ${vkId} registered. Response: ${out.toString().trim()}`);
} catch (e) {
  console.error(`✗ register_vk failed:`);
  console.error(e.stderr?.toString() || e.message);
  process.exit(1);
}

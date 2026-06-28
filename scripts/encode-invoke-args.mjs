// scripts/encode-invoke-args.mjs
// 把 prover 输出的 JSON（十进制域元素 + soroban 格式证明）转换为
// stellar CLI（≥27.0，已移除 --arg-file）可以直接使用的 JSON 字符串参数。
//
// 用法:
//   node scripts/encode-invoke-args.mjs <proof.json>
//     # 打印各字段的 be32 十六进制值（人工核对用，比如 set_policy/set_gate 需要的承诺值）
//
//   node scripts/encode-invoke-args.mjs <reserves_proof.json> attest
//     # 打印 attest 需要的 --proof / --signals JSON 字符串，以及 claimed_supply
//
//   node scripts/encode-invoke-args.mjs <eligibility_proof.json> authorize_receive
//     # 打印 authorize_receive 需要的 --eligibility_proof / --eligibility_signals JSON 字符串
//
// 注意（stellar CLI ≥ 27.0）：
//   `--arg-file` 已被移除。结构体类型（如 Groth16Proof）直接以
//   --proof '{"a":"..","b":"..","c":".."}' 的 JSON 对象字符串传入；
//   Vec<BytesN<32>> 类型（如 PublicSignals）直接以
//   --signals '["..","..",...]' 的 JSON 数组字符串传入。
//   本脚本默认打印 shell 可以直接 eval 的 export 语句，方便拼进调用命令。
//
// Groth16Proof / PublicSignals 的字段布局（来自 prover/src/soroban-format.js）:
//   Groth16Proof { a: BytesN<64>, b: BytesN<128>, c: BytesN<64> }
//   PublicSignals = Vec<BytesN<32>>

import { readFileSync } from "node:fs";

const FP = 32;
function toBe32Hex(decOrHex) {
  let v = BigInt(decOrHex);
  if (v < 0n) throw new Error("negative field element");
  let h = v.toString(16);
  if (h.length > FP * 2) throw new Error("field element exceeds 32 bytes");
  return h.padStart(FP * 2, "0");
}

function argOf(name, def) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 ? process.argv[i + 1] : def;
}

const file = process.argv[2];
const mode = process.argv[3]; // undefined | "attest" | "authorize_receive"
if (!file) {
  console.error("usage: encode-invoke-args.mjs <proof.json> [attest|authorize_receive] [--token T --receiver R]");
  process.exit(1);
}

const data = JSON.parse(readFileSync(file, "utf8"));
const sor = data.soroban;
if (!sor || !sor.proof) throw new Error(`${file} missing .soroban.proof — regenerate with the current prover`);

const strip = (h) => (h.startsWith("0x") ? h.slice(2) : h);

// Groth16Proof 作为一个 JSON 对象字符串（stellar CLI 会自己解析这个 JSON）
const proofJson = JSON.stringify({
  a: strip(sor.proof.a),
  b: strip(sor.proof.b),
  c: strip(sor.proof.c),
});

// PublicSignals 作为一个 JSON 数组字符串
const signalsJson = JSON.stringify(sor.publicSignals.map(strip));

if (!mode) {
  // 诊断模式：打印人工核对用的 be32 值（set_policy / set_gate 需要这些）
  const out = {
    proof: proofJson,
    signals: signalsJson,
  };
  if (data.reservesCommitment) out.reservesCommitmentBe32 = toBe32Hex(data.reservesCommitment);
  if (data.allowedJurisdictionRoot) out.allowedJurisdictionRootBe32 = toBe32Hex(data.allowedJurisdictionRoot);
  if (data.nullifier) out.nullifierBe32 = toBe32Hex(data.nullifier);
  console.log(JSON.stringify(out, null, 2));
  process.exit(0);
}

if (mode === "attest") {
  // por_verifier.attest(token, claimed_supply, proof, signals)
  // token 和 claimed_supply 由调用方（shell 脚本）单独传，这里只输出 proof/signals。
  const supply = BigInt(data.publicSignalsRaw?.[0] ?? "0").toString();
  // 打印 shell 可以 `eval` 的 export 语句，方便直接拼进 stellar contract invoke 命令
  console.log(`export AEGIS_CLAIMED_SUPPLY='${supply}'`);
  console.log(`export AEGIS_PROOF_JSON='${proofJson}'`);
  console.log(`export AEGIS_SIGNALS_JSON='${signalsJson}'`);
  process.exit(0);
}

if (mode === "authorize_receive") {
  const token = argOf("token");
  const receiver = argOf("receiver");
  if (!token || !receiver) throw new Error("authorize_receive needs --token and --receiver");
  console.log(`export AEGIS_TOKEN='${token}'`);
  console.log(`export AEGIS_RECEIVER='${receiver}'`);
  console.log(`export AEGIS_ELIG_PROOF_JSON='${proofJson}'`);
  console.log(`export AEGIS_ELIG_SIGNALS_JSON='${signalsJson}'`);
  process.exit(0);
}

throw new Error(`unknown mode: ${mode}`);

# 本次修复与硬化记录（CHANGES）

> 本文件记录在原 `aegis` 仓库基础上，针对你反馈的 5 个卡点所做的**根因修复**与
> **工程硬化**，以及交付前的自检结果。每条都标注了「为什么」和「改了哪个文件」。

---

## 一、根因层面的修复

### 1. eligibility 电路 Groth16 密钥生成超时（最大瓶颈）→ 移出受限环境
**根因**：6082 约束的 Phase-2 setup 是内存 + 计算密集型操作，沙箱在 5–10 分钟后
被杀进程。任何"在受限环境里本地跑 setup"的方案都治标不治本。

**根本解决**：新增 `.github/workflows/release.yml`。
- 在 GitHub Actions runner（约 7GB 内存、数小时时长）上**一次性**跑完
  `build-circuits.sh --local`（编译电路 + Phase-2 setup + 导出 VK）。
- 把 8 个产物（两套 `*_final.zkey` / `*_js.tar.gz` / `*_vkey.json` /
  `*_vk_soroban.json`）外加 `SHA256SUMS.txt` 上传到对应 tag 的 Release。
- 之后任何受限环境只跑 `bash scripts/build-circuits.sh`（Mode A），几秒下载完成，
  **永不**在本地跑 setup。

> 触发：push 一个 `v*` 标签，或手动 `workflow_dispatch`。

### 2. `npx snarkjs groth16 prove` 静默失败 → 内存上限 + 走 JS API
**根因**：受限主机上 snarkjs 因内存不足被 OOM 静默杀掉（不报错也不产文件）。

**修复**：
- `prove-reserves.js` / `prove-eligibility.js` 本就用 Node API
  `groth16.fullProve(...)` 而非 CLI——保留。
- `scripts/build-circuits.sh`（Mode B）新增
  `export NODE_OPTIONS=... --max-old-space-size=8192`，给 Node 足够堆，避免 setup
  阶段被 OOM 静默杀掉。

### 3. snarkjs 全局安装权限问题 → 本就不依赖全局安装
`build-circuits.sh` 的 `snarkjs()` 包装函数优先用
`prover/node_modules/.bin/snarkjs`，没有再退回 `npx --yes snarkjs`，
**从不** `npm install -g`。无需改动，已符合要求。

### 4. Stellar SDK 版本兼容 → 已固定到 26.x 并隔离 wrapper
- `contracts/Cargo.toml` 与 `groth16_bn254_verifier/Cargo.toml` 固定
  `soroban-sdk = "26.0.0"`。
- BN254 调用统一走 `env.crypto().bn254().{g1_add,g1_mul,multi_pairing_check}`，
  集中在 `groth16_bn254_verifier/src/lib.rs` 底部三个 wrapper 里，换 SDK 只需改这一处。
- `BytesN` 无 `Copy`，跨调用处均用 `.clone()`。
- ⚠️ **待你在真实 `cargo build` 上复核**：本环境无 cargo/网络，无法验证
  `env.crypto().bn254()` 在 26.0.0 的确切方法名。若编译报找不到方法，按
  《升级与开发指南》B.3 切换到 25.x 的 `env.crypto_hazmat().bn254_*()` 即可。

### 5. stellar CLI 移除 `--arg-file` → 改为直接传 JSON 字符串
`scripts/register-vk.mjs` 用 `execFileSync("stellar", [...])` 直接传十六进制 /
JSON 数组字符串；`scripts/invoke-onchain.sh` + `scripts/encode-invoke-args.mjs`
对结构体 / `Vec<T>` 一律传 JSON 字符串。已符合 CLI ≥ 27.0 的入参方式。

---

## 二、修复的真实 Bug（会导致 `cargo test` / CI 失败）

> 这三个是 Rust 合约**测试**里的问题。它们不影响部署的合约逻辑，但会让
> `cargo test --workspace`（即 CI 的绿勾）失败。

1. **`groth16_bn254_verifier/src/test.rs`**：原 `unauthorized_register` 用
   `Groth16Bn254Verifier { env, contract_id }` 构造合约结构体——但它是
   **单元结构体（unit struct）**，没有字段，**无法编译**。已重写为合法的
   `register_before_init_fails`（断言未 init 时 `register_vk` 返回 `NotInitialized`）。

2. **`por_verifier/src/test.rs`** 的 `MockVerifier::verify`：原签名是
   `(vk_id, proof: Groth16Proof, signals)`（3 参），但
   `groth16::verify_via_contract` 实际按 `(vk_id, a, b, c, signals)`（5 参）发起
   跨合约调用。**Soroban 跨调参数个数不匹配会 trap**，测试在运行期失败。
   已改为 5 参签名，与真实 `groth16_bn254_verifier::verify` 对齐。

3. **`eligibility_verifier/src/test.rs`** 的 `MockVerifier::verify`：同上 3 参 vs
   5 参不匹配，已修复为 5 参。

> 注：`rwa_gate/src/test.rs` 的 `MockElig::verify_eligibility` 是 3 参
> `(gate_id, proof, signals)`，与真实 `eligibility_verifier::verify_eligibility`
> 一致，**无需改动**。

---

## 三、工程硬化（非必须但更稳）

- **`build-circuits.sh`**：Mode A 的 Release URL 现在**自动从 `git remote origin`
  推导**（`AEGIS_RELEASE_URL` 优先 > git remote > 占位符），跑过 release 工作流后
  Mode A 开箱即用，无需手改脚本。
- **`invoke-onchain.sh`**：gate 策略的 `action_id` 改为**从证明的
  `publicSignalsRaw[6]` 推导**，不再硬编码 `0x309`，避免改了 `e2e-demo.sh` 的
  `--actionId` 后与链上策略对不上导致 `set_gate` 绑定校验失败。
- **`release.yml`**：构建后用 `snarkjs zkey verify` 校验本地 zkey 完整性，并生成
  `SHA256SUMS.txt` 一并发布，供下载方核对。

---

## 四、交付前自检结果（本环境可验证的部分）

| 检查 | 结果 |
|---|---|
| 全部 12 个 `.js`/`.mjs` `node --check` | ✅ 通过 |
| 全部 5 个 `.sh` `bash -n` | ✅ 通过 |
| `ci.yml` / `release.yml` YAML 合法性 | ✅ 通过 |
| `soroban-format.js` 纯字节编码逻辑独立单测 | ✅ 7/7（长度/大端/`c1c0` 交换/溢出拒绝） |
| `be32_sub`（G1 取负）借位逻辑人工推演 | ✅ 正确（无 (0xFF,0xFF00) 区间歧义） |
| 公开信号顺序 vs 合约 `signals.get(i)` 索引 | ✅ PoR 3 / Eligibility 8 一一对应 |

**本环境无法验证（需 cargo + circom + 网络 + 测试网）**：`cargo test`、真实电路编译、
zkey 生成、链上配对、测试网部署。这些请在你的联网机器或上面新增的 CI 上运行；
路径见《升级与开发指南》。

# SETUP — 环境准备与依赖版本

本文件列出从零跑通 Aegis 所需的全部工具链、精确版本与排错方法。

## 1. 必备工具

| 工具 | 推荐版本 | 用途 | 安装 |
|---|---|---|---|
| Node.js | ≥ 20 LTS（建议 20 或 22） | 运行 prover、生成证明 | https://nodejs.org |
| Rust | stable（≥ 1.79） | 编译 Soroban 合约 | `curl https://sh.rustup.rs -sSf \| sh` |
| `wasm32-unknown-unknown` target | — | 合约编译目标 | `rustup target add wasm32-unknown-unknown` |
| stellar-cli (`stellar`) | ≥ 22.x | 构建/部署/调用合约 | `cargo install --locked stellar-cli` |
| circom | ≥ 2.1.9 | 编译电路 | 见下方第 3 节 |
| snarkjs | ≥ 0.7.4 | 可信设置 + 生成证明 | 随 `prover` 的 `npm install` 一并安装 |
| Python 3 | 任意 3.x | 本地起前端静态服务（可选） | 系统自带 |

> macOS 用户额外建议：`brew install binaryen`（提供 `wasm-opt`，可压缩合约体积，非必需）。

## 2. 仅跑单元测试（最低门槛，无需 Rust/circom）

只验证 prover 的密码学核心逻辑（Poseidon / Merkle / 凭证签名 / 输入构造）：

```bash
cd prover
npm install
npm test
```

预期：`node:test` 全部通过（9 个测试用例）。这一步不需要编译电路、不需要网络访问链上。

## 3. 安装 circom（编译电路所需）

circom 用 Rust 编写，需源码安装：

```bash
git clone https://github.com/iden3/circom.git
cd circom
cargo build --release
cargo install --path circom
circom --version   # 应 ≥ 2.1.9
```

电路通过 `circuits/lib/*.circom` 中的 shim 引用 circomlib。`scripts/build-circuits.sh`
会在仓库根目录安装 `circomlib`，使 `include "../../node_modules/circomlib/..."` 可解析。

## 4. 编译电路 + 可信设置

```bash
bash scripts/build-circuits.sh
```

该脚本会：
1. 在 `prover/` 安装依赖；在仓库根安装 `circomlib`；
2. 生成 **开发用** Powers of Tau（`build/pot16_final.ptau`，2^16 约束，足够本项目）；
3. 编译两套电路 → `.r1cs / .wasm / .sym`；
4. 各自做 Groth16 setup，导出 `*_final.zkey` 与 `*_vkey.json`；
5. 调 `scripts/export-vk.mjs` 把验证密钥转成 Soroban 字节格式 `*_vk_soroban.json`。

> ⚠️ 这是开发仪式，**不可用于生产**。生产可信设置见 `UPGRADE.md`。

产物全部位于 `build/`（已在 `.gitignore` 中，不会误传 GitHub）。

## 5. 编译 + 部署合约到测试网

先准备一个测试网身份（脚本会自动创建并用 friendbot 充值）：

```bash
# 可选：自定义身份名与网络
export STELLAR_IDENTITY=aegis-deployer
export STELLAR_NETWORK=testnet
# 必填：已部署的 groth16_verifier 合约地址（部署方法见 UPGRADE.md）
export GROTH16_VERIFIER_ID=<你的_groth16_verifier_合约ID>

bash scripts/deploy.sh
```

部署完成后地址写入 `build/deploy.testnet.json`，前端与 e2e 脚本都会读取它。

## 6. 端到端演示

```bash
bash scripts/e2e-demo.sh
```

生成储备证明、签发投资者凭证、生成资格证明，产物在 `build/e2e/`。

## 7. 前端

```bash
cd frontend
python3 -m http.server 8080
# 打开 http://localhost:8080
```

## 8. 常见排错

| 现象 | 原因 / 解决 |
|---|---|
| `circom: command not found` | 未装 circom，见第 3 节 |
| `Cannot find module 'circomlib'` 编译电路时 | 在仓库根执行 `npm install circomlib@2.0.5` |
| `npm install` 在本机失败 | 检查网络代理；本仓库构建时所在沙箱禁网，你本机需联网 |
| `cargo build` 报缺少 BN254/crypto host 方法 | 把 `contracts/Cargo.toml` 的 `soroban-sdk` 升到最新 22.x，见 `UPGRADE.md` |
| 链下证明有效但链上被拒 | 翻转 `prover/src/soroban-format.js` 的 `G2_FP2_ORDER`，见 `UPGRADE.md` |
| 部署时 friendbot 充值失败 | 测试网偶发限流，重试；或手动 `stellar keys fund <identity> --network testnet` |

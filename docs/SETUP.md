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

`scripts/build-circuits.sh` 支持两种模式，**默认走快速模式**：

---

### 模式 A — 快速模式（推荐，默认）

从 GitHub Release 直接下载预生成好的 `.zkey` + `.wasm` + vkey 文件，完全跳过本地 Phase-2 setup（最慢的一步）。只需 Node.js，不需要 circom，不需要 snarkjs CLI，约 30 秒完成。

```bash
# 先设置 Release URL（替换成你自己的 GitHub 用户名）
export AEGIS_RELEASE_URL=https://github.com/<你的用户名>/aegis/releases/download/v1.0.0-dev

bash scripts/build-circuits.sh
```

`AEGIS_RELEASE_URL` 未设置时，脚本会打印操作说明后退出。

---

### 模式 B — 本地完整构建（修改了电路时使用）

在本机编译电路并跑 Groth16 Phase-2 setup，需要 circom ≥ 2.1.9 + snarkjs。

```bash
bash scripts/build-circuits.sh --local
```

该步骤会：
1. 安装 prover 依赖，下载 Hermez Phase-1 ptau（pot13，约 10-15 MB，覆盖最多 8192 个约束，足够当前两条电路使用，首次下载后缓存）
2. 编译两套电路 → `.r1cs / .wasm`
3. Phase-2 setup → `*_final.zkey`（Eligibility 电路约 3–10 分钟，视机器而定）
4. 导出 `*_vkey.json` + `*_vk_soroban.json`
5. 打包 `.tar.gz` 准备上传 Release

构建完成后，运行以下命令把产物发布到 GitHub Release，供模式 A 使用：

```bash
bash scripts/release-artifacts.sh
```

> 本地 Phase-2 贡献为单人开发版，**不可用于生产**。生产 Phase-2 仪式见 `UPGRADE.md` C 节。

---

产物全部位于 `build/`（已在 `.gitignore` 中，不会误提交到 GitHub）。

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
| ptau 下载卡住/超时 | 手动下载：`curl -L https://storage.googleapis.com/zkevm/ptau/powersOfTau28_hez_final_13.ptau -o build/pot13_final.ptau`（注：`hermez.s3-eu-west-1.amazonaws.com` 备用域名近期有用户反馈 403，优先用 storage.googleapis.com 这个地址）|
| `cargo build` 报缺少 BN254/crypto host 方法 | 把 `contracts/Cargo.toml` 的 `soroban-sdk` 升到最新 22.x，见 `UPGRADE.md` |
| 链下证明有效但链上被拒 | 翻转 `prover/src/soroban-format.js` 的 `G2_FP2_ORDER`，见 `UPGRADE.md` |
| 部署时 friendbot 充值失败 | 测试网偶发限流，重试；或手动 `stellar keys fund <identity> --network testnet` |

# groth16_bn254_verifier 开发指南

`contracts/groth16_bn254_verifier/` 是 Aegis 的第四个 Soroban 合约，也是整个栈的密码学基础。
本文档解释它的作用、接口、部署步骤，以及如何从其他合约或外部工具调用它。

---

## 1. 它是什么

一个**纯密码学库合约**，没有业务逻辑，只做一件事：

> 接收一个 BN254 Groth16 证明和公开信号，返回 `true`（有效）或 `false`（无效）。

### 为什么要独立成合约

Stellar Protocol 25（X-Ray）通过 CAP-0074 将 BN254 椭圆曲线运算作为原生 host functions 暴露出来：
`bn254_g1_add`、`bn254_g1_mul`、`bn254_multi_pairing_check`。
Aegis 把这些 host functions 封装成一个可跨合约复用的验证合约，理由有三：

1. **完全自包含**：不依赖任何外部社区地址，clone 仓库即可部署
2. **单点维护**：三个应用合约（por_verifier / eligibility_verifier / rwa_gate）共享同一个底层验证器，算法升级只改这里
3. **多电路支持**：用 `vk_id` 区分不同电路的验证密钥（0 = PoR，1 = Eligibility），一个合约服务全部

---

## 2. 合约接口

### 2.1 数据类型

```rust
// 存储在链上的验证密钥（每条电路各一份）
struct VerificationKey {
    alpha: BytesN<64>,     // α ∈ G1，64 字节大端非压缩
    beta:  BytesN<128>,    // β ∈ G2，128 字节
    gamma: BytesN<128>,    // γ ∈ G2，128 字节
    delta: BytesN<128>,    // δ ∈ G2，128 字节
    ic:    Vec<BytesN<64>>,// IC 数组，长度 = n_public + 1，每项 G1 64 字节
}

// 错误码
enum Error {
    AlreadyInitialized = 1,
    NotInitialized     = 2,
    Unauthorized       = 3,
    VkNotFound         = 4,
    InvalidInputCount  = 5,
    ProofInvalid       = 6, // 保留，当前版本未使用（bad proof → verify 返回 false）
}
```

### 2.2 函数列表

| 函数 | 权限 | 说明 |
|---|---|---|
| `init(admin)` | 部署者（一次性） | 初始化合约，设置管理员 |
| `register_vk(vk_id, alpha, beta, gamma, delta, ic)` | admin | 注册一条电路的验证密钥 |
| `vk(vk_id)` | 任何人 | 读取已注册的 VK（用于检查） |
| `verify(vk_id, proof_a, proof_b, proof_c, public_inputs)` | 任何人 | 验证一个证明，返回 bool |

### 2.3 `verify` 参数详解

```
vk_id          : u32            — 选择验证密钥（0 = PoR，1 = Eligibility）
proof_a        : BytesN<64>     — A ∈ G1，64 字节大端非压缩 (x32 ‖ y32)
proof_b        : BytesN<128>    — B ∈ G2，128 字节 (x1_32 ‖ x0_32 ‖ y1_32 ‖ y0_32)
proof_c        : BytesN<64>     — C ∈ G1，64 字节
public_inputs  : Vec<BytesN<32>>— 公开信号，顺序与电路 public[] 声明一致
```

**字节格式注意**：G2 的 Fp2 分量顺序（x1/x0）由 `prover/src/soroban-format.js` 的 `G2_FP2_ORDER` 控制。
如果 verify 返回 false 但链下验证通过，翻转该开关，见第 5 节。

---

## 3. 部署与初始化（完整步骤）

`scripts/deploy.sh` 会自动完成以下步骤，下面的手动命令用于理解或排错。

### 步骤 1：确认电路已构建

```bash
bash scripts/build-circuits.sh
# 产物：
#   build/proof_of_reserves_vk_soroban.json   ← PoR 电路的 VK（Soroban 格式）
#   build/eligibility_vk_soroban.json         ← Eligibility 电路的 VK
```

### 步骤 2：构建 wasm

```bash
cd contracts
stellar contract build
# 产物：target/wasm32-unknown-unknown/release/groth16_bn254_verifier.wasm
```

### 步骤 3：部署合约（必须最先部署，其他三个合约依赖它）

```bash
GROTH16_ID=$(stellar contract deploy \
  --wasm contracts/target/wasm32-unknown-unknown/release/groth16_bn254_verifier.wasm \
  --source aegis-deployer \
  --network testnet)
echo "groth16_verifier = $GROTH16_ID"
```

### 步骤 4：初始化

```bash
ADMIN=$(stellar keys address aegis-deployer)

stellar contract invoke \
  --id "$GROTH16_ID" --source aegis-deployer --network testnet \
  -- init --admin "$ADMIN"
```

### 步骤 5：注册 VK（每条电路一次，共两次）

用 `scripts/register-vk.mjs` 自动完成（deploy.sh 会调用它）：

```bash
# PoR 电路（vk_id = 0）
node scripts/register-vk.mjs \
  --verifier "$GROTH16_ID" \
  --vk-id 0 \
  --vk-file build/proof_of_reserves_vk_soroban.json \
  --source aegis-deployer \
  --network testnet

# Eligibility 电路（vk_id = 1）
node scripts/register-vk.mjs \
  --verifier "$GROTH16_ID" \
  --vk-id 1 \
  --vk-file build/eligibility_vk_soroban.json \
  --source aegis-deployer \
  --network testnet
```

### 步骤 6：验证注册成功

```bash
# 检查 PoR VK 是否在链上
stellar contract invoke \
  --id "$GROTH16_ID" --network testnet \
  -- vk --vk_id 0
# 应返回包含 alpha/beta/gamma/delta/ic 的 JSON
```

### 步骤 7：部署其余三个合约，传入 groth16 地址

```bash
stellar contract invoke --id "$POR_ID" ... -- init \
  --admin "$ADMIN" --verifier "$GROTH16_ID"

stellar contract invoke --id "$ELIG_ID" ... -- init \
  --admin "$ADMIN" --verifier "$GROTH16_ID"
```

---

## 4. 直接调用 verify（手动测试）

生成证明后（`scripts/e2e-demo.sh` 会输出 `build/e2e/reserves_proof.json`），可以手动验证：

```bash
# 用 encode-invoke-args.mjs 把 prover 输出转为 stellar-cli 参数格式
node scripts/encode-invoke-args.mjs build/e2e/reserves_proof.json \
  > /tmp/por_args_raw.json

# 从中提取 proof 和 signals
node -e "
const raw = require('/tmp/por_args_raw.json');
const args = {
  vk_id: 0,
  proof_a: raw.proof.a,
  proof_b: raw.proof.b,
  proof_c: raw.proof.c,
  public_inputs: raw.publicSignals
};
require('fs').writeFileSync('/tmp/verify_args.json', JSON.stringify(args, null, 2));
"

stellar contract invoke \
  --id "$GROTH16_ID" --source aegis-deployer --network testnet \
  --arg-file /tmp/verify_args.json \
  -- verify
# 应返回: true
```

---

## 5. 从其他合约调用

三个应用合约（por_verifier、eligibility_verifier、rwa_gate）都通过 `contracts/*/src/groth16.rs`
里的 `verify_via_contract()` 调用。示意如下，如果你想写自己的合约来调用：

```rust
// 在你的合约里
let result: bool = env.invoke_contract(
    &groth16_verifier_address,         // Address，init 时存进来的
    &soroban_sdk::Symbol::new(&env, "verify"),
    soroban_sdk::vec![
        &env,
        vk_id.into_val(&env),          // u32
        proof.a.clone().into_val(&env), // BytesN<64>
        proof.b.clone().into_val(&env), // BytesN<128>
        proof.c.clone().into_val(&env), // BytesN<64>
        signals.clone().into_val(&env), // Vec<BytesN<32>>
    ],
);
// result == true → 证明有效
```

---

## 6. vk_id 约定

| vk_id | 电路 | 公开信号数量 | 信号顺序 |
|---|---|---|---|
| `0` | Proof-of-Reserves | 3 | `[totalSupply, reservesCommitment, minCollateralBps]` |
| `1` | Eligibility | 8 | `[issuerPubKeyX, issuerPubKeyY, requiredKycLevel, requireAccredited, allowedJurisdictionRoot, currentTimestamp, actionId, nullifier]` |

自定义电路从 `vk_id = 2` 开始，任意 u32 均可。

---

## 7. 数学原理（Groth16 配对等式）

合约验证的是：

```
e(-A, B) · e(α, β) · e(Σ, γ) · e(C, δ) = 1
```

等价于经典 Groth16 等式 `e(A, B) = e(α, β) · e(Σ, γ) · e(C, δ)`，通过 `bn254_multi_pairing_check` 一次调用完成全部配对计算。

其中 `Σ = IC[0] + s₀·IC[1] + s₁·IC[2] + … + sₙ·IC[n+1]`，由 `compute_sigma()` 用 `bn254_g1_mul` + `bn254_g1_add` 迭代计算。

**G1 点取反**（-A）用大端 256-bit 减法实现：`y_neg = p − y`，其中 `p` 是 BN254 基域素数，硬编码在 `g1_negate()` 里。

---

## 8. 常见问题

### verify 返回 false，但链下 snarkjs.groth16.verify 通过

这是 G2 点的 Fp2 分量字节顺序不匹配。

**解法**：打开 `prover/src/soroban-format.js`，把第 21 行：
```js
export const G2_FP2_ORDER = "c1c0";
```
改为 `"c0c1"`（或反过来），然后重新生成证明和 VK，重新注册 VK。

### Error: VkNotFound

尚未注册该 `vk_id` 的验证密钥。先运行 `scripts/register-vk.mjs`（或检查 `deploy.sh` 是否报错）。

### Error: InvalidInputCount

传入的 `public_inputs` 长度与 VK 的 `ic.len() - 1` 不匹配。检查 `encode-invoke-args.mjs` 的输出，确认 `publicSignals` 数组长度正确（PoR = 3，Eligibility = 8）。

### cargo build 报错找不到 crypto_hazmat / bn254

soroban-sdk 版本低于 22.x，或未开启 `hazmat-crypto` feature。
把 `contracts/groth16_bn254_verifier/Cargo.toml` 的 soroban-sdk 依赖改为：

```toml
soroban-sdk = { version = "22.0.0", features = ["hazmat-crypto"] }
```

如果使用 25.x 以上 SDK 且想用原生更快路径，启用 `host-pairing` feature：

```toml
soroban-sdk = { version = "25.0.0", features = ["hazmat-crypto"] }
```
然后把 `lib.rs` 里三个 `bn254_*` wrapper 函数改为对应的 25.x API 名称（见 SDK changelog）。

---

## 9. 与其他方案对比

| 方案 | 优点 | 缺点 |
|---|---|---|
| **本合约（当前方案）** | 完全自包含，无外部依赖；直接用 Protocol 25 host functions | 需要部署多一个合约 |
| soroban-examples groth16_verifier | 官方示例，有参考价值 | BLS12-381 曲线，与 Circom/snarkjs 默认的 BN254 不兼容 |
| 依赖社区/第三方已部署合约 | 省去部署步骤 | 地址不确定，增加信任假设，hackathon 评委无法复现 |

# UPGRADE — 生产化、链上接线与需要你准备的资料

本文件回答两件事:
1. 把这个 hackathon 原型推进到生产/真实测试网,需要做哪些升级;
2. **你需要亲自准备哪些资料信息**(钱包地址、密钥、合约 ID 等)。

---

## A. 你需要准备的资料清单 ✅

下面这些是脚本与合约运行时**必须由你提供**的外部信息。建议先全部备齐再执行部署。

### A.1 钱包 / 身份(必需)

| 项目 | 说明 | 如何获得 |
|---|---|---|
| **部署者身份(secret key)** | 用于部署与初始化三个合约,即 `STELLAR_IDENTITY` | `stellar keys generate aegis-deployer --network testnet --fund` |
| **部署者公钥地址(G...)** | 即合约 `admin`,脚本自动读取 | `stellar keys address aegis-deployer` |
| **测试网 XLM 余额** | 支付部署/调用手续费 | friendbot 自动充值;或 `stellar keys fund aegis-deployer --network testnet` |

> 主网部署时:用真实托管钱包(建议硬件钱包 / 多签),**绝不要把 secret key 写进仓库或 .env 提交**。

### A.2 发行方(Issuer)资料(必需)

| 项目 | 说明 | 在哪用 |
|---|---|---|
| **Issuer EdDSA 私钥种子** | 签发投资者凭证用,设为环境变量 `ISSUER_SEED` | `prover/src/issue-credential.js` |
| **Issuer EdDSA 公钥 (X,Y)** | 写进 `eligibility_verifier` 的 `GatePolicy`,脚本会从凭证里带出 | 链上 gate 策略 |
| **RWA token 合约地址** | 你要门控的那枚资产 token(SAC 或自定义) | `por_verifier` / `rwa_gate` 配置 |
| **储备承诺 reservesCommitment** | `prove-reserves.js` 生成后公布 | `por_verifier.set_policy` |
| **辖区 allowlist 国家码** | 许可的 ISO-3166 数字码,如 840=US, 826=UK | `build-allowlist.js` |

### A.3 外部合约依赖(必需)

| 项目 | 说明 | 如何获得 |
|---|---|---|
| **`GROTH16_VERIFIER_ID`** | 已部署的社区 Groth16 验证合约地址 | 见下方 B.3 自行部署,或填官方/社区已部署实例 |

### A.4 可选 / 录 demo 用

| 项目 | 说明 |
|---|---|
| 2–3 分钟 demo 视频 | 评委要求的交付物之一 |
| 自定义辖区码、KYC 等级阈值、actionId | 调 `e2e-demo.sh` 参数即可 |

### A.5 环境变量汇总(部署前 export)

```bash
export STELLAR_NETWORK=testnet
export STELLAR_IDENTITY=aegis-deployer
export GROTH16_VERIFIER_ID=<groth16_verifier 合约ID>     # 必填,见 B.3
export ISSUER_SEED="<你的发行方私钥种子,勿用默认>"        # 签发凭证用
```

---

## B. 链上接线(把 SNARK 真正在 Soroban 上验证)

### B.1 校准字节编码 `G2_FP2_ORDER`

不同 Groth16 验证合约对 G2 点的 Fp2 分量顺序(c0c1 vs c1c0)期望不同。若你的证明
**链下 `snarkjs.groth16.verify` 通过、但链上被拒**,这是首要排查点:

1. 打开 `prover/src/soroban-format.js`;
2. 把 `export const G2_FP2_ORDER = "c1c0";` 改成 `"c0c1"`(或反向);
3. 重新跑 `scripts/export-vk.mjs` 与 `prove-*.js`,再链上验证。

只有这一个开关,两种取值必有其一与目标验证器匹配。

### B.2 验证器调用签名

`contracts/*/src/groth16.rs` 的 `verify_via_contract` 假定社区验证器暴露:

```
fn verify(vk_id: u32, proof: Groth16Proof, signals: Vec<BytesN<32>>) -> bool
```

若你选用的验证合约函数名/参数不同(例如把 VK 直接随调用传入而非用 `vk_id` 选择),
只需改这一个函数体即可,其余合约逻辑不动。

### B.3 部署一个 groth16_verifier

两条路线任选其一:

**路线 1 — 用 soroban-sdk 直接做 host 配对(若你的 SDK 版本暴露 BN254 API):**
启用 `groth16.rs` 里标注的 `host_pairing` 分支,直接用 `env.crypto()` 的 BN254 方法做
配对检查。优点:少一个外部依赖;缺点:host 方法名随 SDK 小版本变动,需对齐你本地
`soroban-sdk` 版本的确切 API。

**路线 2 — 部署社区验证合约(默认推荐):**
从 `stellar/soroban-examples` 或 Nethermind/相关开源仓库取 `groth16_verifier`,
`stellar contract deploy` 后,把每条电路的 `*_vk_soroban.json`(由 `export-vk.mjs` 生成)
注册进去,得到 `vk_id`,再在 `set_policy` / `set_gate` 时引用该 `vk_id`。

> 把得到的合约 ID 设进 `GROTH16_VERIFIER_ID` 后再跑 `scripts/deploy.sh`。

### B.4 注册 policy / gate(部署后一次性)

```bash
# 1) PoR 策略
stellar contract invoke --id <POR_ID> --source $STELLAR_IDENTITY --network testnet -- \
  set_policy --token <TOKEN_ID> --issuer <ISSUER_ADDR> \
  --reserves_commitment <COMMITMENT_BYTES32> --min_collateral_bps 10000 --vk_id <POR_VK_ID>

# 2) Eligibility gate 策略(GatePolicy 各字段)
stellar contract invoke --id <ELIG_ID> --source $STELLAR_IDENTITY --network testnet -- \
  set_gate --admin <ADMIN_ADDR> --gate_id <GATE_ID_BYTES32> --policy '<JSON>'

# 3) RWA gate 配置
stellar contract invoke --id <GATE_ID> --source $STELLAR_IDENTITY --network testnet -- \
  configure --admin <ADMIN_ADDR> --config '<JSON>'
```

> `BytesN<32>` 类参数用十六进制传入;复杂结构体用 stellar-cli 的 JSON 入参格式。
> 各结构体字段定义见对应合约 `lib.rs`。

---

## C. 生产可信设置(Trusted Setup)

`scripts/build-circuits.sh` 的 Powers of Tau 是**单人开发仪式,绝不可上生产**。
生产需要:

1. 采用公开的 Phase-1 PoT(如 Hermez/Perpetual Powers of Tau 的成熟产物),不要自己从零生成;
2. 对每条电路做 **多方 Phase-2 贡献**,参与者越多越好,每人贡献后公开 transcript;
3. 仪式结束做 `snarkjs zkey verify` 校验,并公布最终 `zkey` 的哈希供社区核对;
4. 保证至少一名诚实参与者销毁其随机性(toxic waste)即可保证安全。

每改动电路一行,Phase-2 与 VK 都要重做并重新注册到验证器。

---

## D. 从原型到生产的硬化清单

- [ ] 用成熟 Phase-1 + 多方 Phase-2 仪式替换开发版可信设置(C 节)
- [ ] 校准并固定 `G2_FP2_ORDER`,链上链下一致(B.1)
- [ ] 部署/接入正式 `groth16_verifier` 并注册 VK(B.3)
- [ ] Issuer 私钥迁移到 HSM/KMS,移除默认 dev seed
- [ ] 部署者改多签 / 硬件钱包,admin 权限可考虑转 timelock
- [ ] 合约存储 TTL / 续租策略(Soroban 状态过期),为 nullifier 与 attestation 设置合理 bump
- [ ] 凭证吊销机制(如吊销列表 Merkle 根)与 KYC 过期联动
- [ ] 辖区 allowlist 上链治理与更新流程
- [ ] 审计:电路约束完备性 + 合约权限 + 经济假设
- [ ] 主网部署前压测手续费与证明大小,评估批量验证
- [ ] 前端接线真实合约(替换 `frontend/index.html` 内 mock* 调用,B 节与 `frontend/README.md`)

---

## E. 升级合约本身(代码演进)

- 合约用 `crate-type=["cdylib","rlib"]`,可常规 `stellar contract build` 出新 wasm;
- Soroban 支持合约升级(`update_current_contract_wasm`),如需可升级性,在各合约加 admin 守卫的
  升级入口;当前版本未内置,以保持最小可信表面——生产可按需添加。
- 升级电路 → 必然伴随新 VK 与新 `vk_id`,旧证明对新 VK 失效,属预期行为。

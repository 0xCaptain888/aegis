# ARCHITECTURE — 技术设计详解

本文件解释 Aegis 的密码学构造、合约边界、信任假设与安全性论证。面向评委与后续维护者。

## 1. 系统目标

在 Stellar 上实现 **"隐私 + 合规"** 的 RWA 结算原语:既不暴露发行方的逐笔储备、也不暴露
投资者的身份,却能让任何人在链上验证两件事——

1. **偿付能力**:储备 ≥ 流通供应量 ×抵押率;
2. **持有资格**:接收方满足 KYC / 司法辖区 / 合格投资者 / 未过期。

二者由 `rwa_gate` 组合:**两者同时成立**才放行一次转账,且证明不可重放。

## 2. 两条电路

### 2.1 Proof-of-Reserves

```
public  : totalSupply, reservesCommitment, minCollateralBps
private : balances[8], salt
enforce :
  (a) Poseidon(balances[0..8], salt) == reservesCommitment
  (b) Σ balances · 10000 ≥ totalSupply · minCollateralBps
  (c) ∀ balance, totalSupply ∈ [0, 2^64)   // Num2Bits 范围检查
```

- **承诺绑定 (a)**:发行方事先把 `reservesCommitment` 公布到链上(存进 `por_verifier` 的
  `ReservePolicy`)。证明强制余额哈希等于该承诺,使发行方无法临场捏造一组好看的余额。
- **偿付约束 (b)**:左右两边各 ≤ 2^96,远小于 BN254 标量域(~2^254),不会溢出。
- **范围检查 (c)**:防止利用域回绕(field wrap)用一个"负数"余额伪造偿付。
- N=8 是编译期常量;更多托管账户改大 N 重新编译即可,未用槽位填 0。

### 2.2 Eligibility(选择性披露)

```
public  : issuerPubKeyX/Y, requiredKycLevel, requireAccredited,
          allowedJurisdictionRoot, currentTimestamp, actionId, nullifier
private : kycLevel, jurisdictionCode, accredited, expiry, credentialSecret,
          sigR8x/R8y/S, jurPathElements[16], jurPathIndices[16]
enforce :
  (a) EdDSAPoseidon.verify(issuerPubKey, sig, credHash) == 1
      credHash = Poseidon(kyc, jurisdiction, accredited, expiry, Poseidon(secret))
  (b) kycLevel ≥ requiredKycLevel
  (c) requireAccredited ⟹ accredited
  (d) expiry > currentTimestamp
  (e) MerkleInclusion(jurisdictionCode, allowedJurisdictionRoot)
  (f) nullifier == Poseidon(credentialSecret, actionId)
```

- **凭证 = 发行方签名 (a)**:KYC 提供方用 EdDSA-Poseidon 对凭证哈希签名。`credentialSecret`
  以承诺 `Poseidon(secret)` 形式进签名载荷,发行方因此学不到持有者的花费秘密。
- **选择性披露**:链上只学到一个布尔(证明能生成即代表全部约束成立)和一个 `nullifier`。
  身份、出生日期、精确国别永不出钱包。
- **辖区 allowlist (e)**:发行方承诺一棵"许可辖区"的 Poseidon-Merkle 树(深度 16,最多 65536
  个 ISO-3166 数字码),持有者证明自己的辖区在其中。比通用非成员证明更简单且健全,契合 Stellar
  ASP 的 allow/deny 集合语义。
- **nullifier (f)**:`Poseidon(secret, actionId)`。同一凭证对同一受控动作只能用一次,既防双花/女巫,
  又与身份不可关联。

## 3. 三个合约

```
groth16_verifier (社区合约,外部部署)  ← BN254 pairing,Protocol 25/26 host functions
        ▲ 跨合约调用 verify(vk_id, proof, signals)
        │
   ┌────┴──────────┐        ┌────────────────────────┐
   │ por_verifier  │        │ eligibility_verifier   │
   │ attest()      │        │ verify_eligibility()   │
   └────┬──────────┘        └───────────┬────────────┘
        │ last_attestation              │ 返回/消费 nullifier
        ▼                               ▼
            ┌───────────────────────────────┐
            │           rwa_gate            │
            │ authorize_receive():          │
            │   check_reserves() 新鲜 &&     │
            │   verify_eligibility() 通过    │
            └───────────────────────────────┘
```

### 3.1 `por_verifier`
- `ReservePolicy{ issuer, reservesCommitment, minCollateralBps, vk_id }`,按 token 存储。
- `attest()`:把公开信号 `[totalSupply, reservesCommitment, minCollateralBps]` 逐一绑定到已注册
  policy(承诺不符 → `CommitmentMismatch`,bps 不符 → `PolicyMismatch`,供应量不符 → `SupplyMismatch`),
  再跨合约验证 SNARK,通过后写入带时间戳的 `Attestation`。
- 任何人都能提交新证明刷新链上 attestation——把"月度 PoR 报告"变成"实时可验证事实"。

### 3.2 `eligibility_verifier`
- `GatePolicy` 绑定发行方公钥、最低 KYC、是否要求合格、辖区根、actionId、`max_skew_secs`。
- `verify_eligibility()`:逐一绑定全部 policy 相关公开信号 → 检查时间戳在 `±max_skew_secs`
  容差内(防陈旧/未来证明)→ 检查 `(gate_id, nullifier)` 未用过 → 验证 SNARK → 消费 nullifier。
- `is_nullifier_used()` 供外部查询。

### 3.3 `rwa_gate`
- `GateConfig{ token, por_verifier, eligibility_verifier, eligibility_gate_id, max_reserve_age_secs }`。
- `check_reserves()`:读 `por_verifier.last_attestation`,校验其 `verified_at` 在 `max_reserve_age_secs`
  内,否则 `ReservesStale` / `NoReserves`。
- `authorize_receive()`:先查储备新鲜,再跨合约调 `verify_eligibility`(失败则整笔原子回滚),
  成功发出 `authorized` 事件并返回被消费的 nullifier。

## 4. 信任假设

| 主体 | 被信任做什么 | 不被信任 |
|---|---|---|
| 发行方 | 诚实公布 `reservesCommitment`、维护辖区 allowlist | 无法伪造偿付(承诺+范围检查约束) |
| KYC 发行方 | 只给合规投资者签发凭证 | 学不到持有者花费秘密;无法替持有者花费 |
| 可信设置参与者 | 至少一方诚实销毁 toxic waste | 否则可伪造证明——故生产需多方仪式 |
| groth16_verifier | 正确实现 BN254 配对 | 我方合约把信号绑定后才交其验证 |
| 链/排序者 | 活性 | 无法伪造或重放(nullifier + 信号绑定) |

## 5. 安全性论证要点

- **重放**:`nullifier` 全局唯一消费 + 公开信号与 policy 强绑定,跨 gate / 跨 token 无法复用。
- **域回绕**:所有金额、供应量、时间戳均做 `Num2Bits` 范围检查。
- **策略调换**:合约对每个 policy 字段逐一 assert,证明无法"张冠李戴"。
- **陈旧证明**:eligibility 用 `currentTimestamp` 公开信号 + 合约侧 `max_skew_secs` 双重约束;
  reserves 用 `max_reserve_age_secs` 限定 attestation 年龄。
- **凭证伪造**:电路内验证发行方 EdDSA 签名,无签名无法构造合法 `credHash`。

## 6. 已知简化(诚实披露)

1. 可信设置为开发版仪式 → 生产需 MPC 仪式(`UPGRADE.md`)。
2. 链上验证经由社区 `groth16_verifier` 跨合约调用,而非直接 host 方法——规避 SDK 小版本 ABI 漂移。
3. 辖区用 allowlist 成员证明,而非通用非成员证明。
4. 前端为链上流程的忠实模拟,便于无钱包录制 demo;接线方法见 `frontend/README.md`。

## 7. 字节编码约定(prover ↔ 合约)

`prover/src/soroban-format.js` 把 snarkjs 证明转为合约期望的大端字节:
- G1 点(a、c):`x ‖ y`,各 32 字节 → 64 字节;
- G2 点(b):`x ‖ y`,每个是 Fp2 → 128 字节,Fp2 分量顺序由 `G2_FP2_ORDER` 控制;
- 公开信号:每个 32 字节大端域元素,顺序严格同电路 `public [...]` 声明。

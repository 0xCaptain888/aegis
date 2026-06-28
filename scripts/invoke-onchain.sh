#!/usr/bin/env bash
# scripts/invoke-onchain.sh
# 把 e2e-demo.sh 生成的证明推上链，跑完整的合规门流程：
#   1. 注册 PoR 策略 + 提交储备证明  -> 链上 Attestation
#   2. 注册 eligibility gate 策略
#   3. rwa_gate.authorize_receive() 用资格证明  -> ✅ 门打开
#   4. 用同一证明再调一次                        -> ❌ nullifier 重放被拒
#
# 这是 demo 视频的高潮镜头。前置条件:
#   - scripts/build-circuits.sh 已生成 zkey/VK
#   - scripts/deploy.sh 已写出 build/deploy.<net>.json
#   - scripts/e2e-demo.sh 已写出 build/e2e/{reserves_proof,eligibility_proof,allowlist,credential}.json
#
# 重要（stellar CLI ≥ 27.0）：
#   `--arg-file` 已被移除。结构体/Vec 类型参数现在直接以
#   --arg-name '<JSON 字符串>' 的形式传给 CLI。本脚本通过
#   scripts/encode-invoke-args.mjs 生成对应的 JSON 字符串，
#   再用 shell 变量直接拼进 stellar contract invoke 命令。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NET="${STELLAR_NETWORK:-testnet}"
SRC="${STELLAR_IDENTITY:-aegis-deployer}"
DEPLOY="$ROOT/build/deploy.${NET}.json"
WORK="$ROOT/build/e2e"

[ -f "$DEPLOY" ] || { echo "缺少 $DEPLOY — 先运行 scripts/deploy.sh"; exit 1; }
[ -f "$WORK/reserves_proof.json" ] || { echo "缺少证明文件 — 先运行 scripts/e2e-demo.sh"; exit 1; }

jqget () { node -e "console.log(require('$DEPLOY').$1)"; }
POR_ID="$(jqget por_verifier)"
ELIG_ID="$(jqget eligibility_verifier)"
GATE_ID="$(jqget rwa_gate)"
ADMIN="$(jqget admin)"

TOKEN="${RWA_TOKEN_ID:-$ADMIN}"
GATE_BYTES32="${ELIGIBILITY_GATE_ID:-0000000000000000000000000000000000000000000000000000000000000001}"

be32 () { node -e "let v=BigInt(process.argv[1]);console.log(v.toString(16).padStart(64,'0'))" "$1"; }

echo "######## 1. 注册 PoR 策略 + attest ########"
COMMIT_DEC="$(node -e "console.log(require('$WORK/reserves_proof.json').reservesCommitment)")"
COMMIT_BE32="$(be32 "$COMMIT_DEC")"
stellar contract invoke --id "$POR_ID" --source "$SRC" --network "$NET" -- \
  set_policy --token "$TOKEN" --issuer "$ADMIN" \
  --reserves_commitment "$COMMIT_BE32" --min_collateral_bps 10000 --vk_id 0

# encode-invoke-args.mjs 输出一组 export 语句，eval 后得到
# $AEGIS_CLAIMED_SUPPLY / $AEGIS_PROOF_JSON / $AEGIS_SIGNALS_JSON
eval "$(node "$ROOT/scripts/encode-invoke-args.mjs" "$WORK/reserves_proof.json" attest)"

stellar contract invoke --id "$POR_ID" --source "$SRC" --network "$NET" -- \
  attest --token "$TOKEN" --claimed_supply "$AEGIS_CLAIMED_SUPPLY" \
  --proof "$AEGIS_PROOF_JSON" --signals "$AEGIS_SIGNALS_JSON" || {
    echo "  (如果在这里失败，可能是字节序问题 — 翻转 G2_FP2_ORDER，见 docs/UPGRADE.md)"; exit 1; }

echo "######## 2. 注册 eligibility gate 策略 ########"
ISSUER_X="$(be32 "$(node -e "console.log(require('$WORK/credential.json').issuerPubKeyX)")")"
ISSUER_Y="$(be32 "$(node -e "console.log(require('$WORK/credential.json').issuerPubKeyY)")")"
ROOT_BE32="$(be32 "$(node -e "console.log(require('$WORK/allowlist.json').root)")")"
# action_id 直接从证明的公开信号[6] 推导，保证与 e2e-demo.sh 的 --actionId 一致，
# 不会因为改了 demo 参数而和这里硬编码的值对不上（否则 set_gate 绑定校验会失败）。
ACTION_BE32="$(be32 "$(node -e "console.log(require('$WORK/eligibility_proof.json').publicSignalsRaw[6])")")"

POLICY_JSON=$(node -e "
console.log(JSON.stringify({
  issuer_pubkey_x: '$ISSUER_X',
  issuer_pubkey_y: '$ISSUER_Y',
  required_kyc_level: 2,
  require_accredited: true,
  allowed_jurisdiction_root: '$ROOT_BE32',
  action_id: '$ACTION_BE32',
  vk_id: 1,
  max_skew_secs: 3600
}))
")

stellar contract invoke --id "$ELIG_ID" --source "$SRC" --network "$NET" -- \
  set_gate --admin "$ADMIN" --gate_id "$GATE_BYTES32" --policy "$POLICY_JSON"

echo "######## 3. 配置 gate + authorize_receive（预期 ✅）########"
CONFIG_JSON=$(node -e "
console.log(JSON.stringify({
  token: '$TOKEN',
  por_verifier: '$POR_ID',
  eligibility_verifier: '$ELIG_ID',
  eligibility_gate_id: '$GATE_BYTES32',
  max_reserve_age_secs: 86400
}))
")
stellar contract invoke --id "$GATE_ID" --source "$SRC" --network "$NET" -- \
  configure --admin "$ADMIN" --config "$CONFIG_JSON"

eval "$(node "$ROOT/scripts/encode-invoke-args.mjs" "$WORK/eligibility_proof.json" authorize_receive \
  --token "$TOKEN" --receiver "$ADMIN")"

echo ">>> 第一次调用（预期放行 ✅）:"
stellar contract invoke --id "$GATE_ID" --source "$SRC" --network "$NET" -- \
  authorize_receive --token "$AEGIS_TOKEN" --receiver "$AEGIS_RECEIVER" \
  --eligibility_proof "$AEGIS_ELIG_PROOF_JSON" --eligibility_signals "$AEGIS_ELIG_SIGNALS_JSON"

echo "######## 4. 用同一证明重放（预期 ❌ NullifierUsed）########"
echo ">>> 第二次调用（预期拒绝）:"
if stellar contract invoke --id "$GATE_ID" --source "$SRC" --network "$NET" -- \
     authorize_receive --token "$AEGIS_TOKEN" --receiver "$AEGIS_RECEIVER" \
     --eligibility_proof "$AEGIS_ELIG_PROOF_JSON" --eligibility_signals "$AEGIS_ELIG_SIGNALS_JSON"; then
  echo "  ✗ 异常：重放竟然成功了 — 检查 nullifier 存储逻辑"; exit 1
else
  echo "  ✓ 符合预期：重放被拒绝（nullifier 已被消费）"
fi

echo ""
echo "✓ 链上门演示完成（网络: $NET）"
echo "  浏览器查看: https://stellar.expert/explorer/$NET/contract/$GATE_ID"

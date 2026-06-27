#!/usr/bin/env bash
# scripts/release-artifacts.sh
# 把 build/ 下的预生成产物上传到 GitHub Release。
# 在联网、有 circom/snarkjs 的机器上跑完 --local 构建之后执行本脚本。
#
# 前提:
#   1. 已安装 GitHub CLI (gh)：https://cli.github.com
#   2. 已登录：gh auth login
#   3. 已把代码 push 到 GitHub
#   4. 已在本机跑完：bash scripts/build-circuits.sh --local
#
# 用法:
#   bash scripts/release-artifacts.sh [tag]
#   tag 默认: v1.0.0-dev
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD="$ROOT/build"
TAG="${1:-v1.0.0-dev}"

command -v gh >/dev/null 2>&1 || {
  echo "ERROR: 未安装 GitHub CLI。"
  echo "  安装: https://cli.github.com"
  echo "  或者手动把 build/ 里的文件拖到 GitHub Release 页面。"
  exit 1
}

FILES=(
  "$BUILD/proof_of_reserves_final.zkey"
  "$BUILD/proof_of_reserves_js.tar.gz"
  "$BUILD/proof_of_reserves_vkey.json"
  "$BUILD/proof_of_reserves_vk_soroban.json"
  "$BUILD/eligibility_final.zkey"
  "$BUILD/eligibility_js.tar.gz"
  "$BUILD/eligibility_vkey.json"
  "$BUILD/eligibility_vk_soroban.json"
)

# 检查文件是否存在
for f in "${FILES[@]}"; do
  [ -f "$f" ] || { echo "ERROR: 缺少 $f，先跑 bash scripts/build-circuits.sh --local"; exit 1; }
done

echo "==> 创建/更新 GitHub Release: $TAG"
# 如果 Release 不存在则创建，存在则继续（不覆盖）
gh release create "$TAG" \
  --title "Aegis $TAG — Pre-built ZK artifacts" \
  --notes "预生成的 Groth16 zkey + wasm + vkey 文件。使用方式：
\`\`\`bash
export AEGIS_RELEASE_URL=https://github.com/\$(gh repo view --json nameWithOwner -q .nameWithOwner)/releases/download/$TAG
bash scripts/build-circuits.sh
\`\`\`" \
  2>/dev/null || echo "  Release 已存在，继续上传文件..."

echo "==> 上传文件..."
for f in "${FILES[@]}"; do
  echo "  → $(basename "$f") ($(du -sh "$f" | cut -f1))"
  gh release upload "$TAG" "$f" --clobber
done

REPO_URL="$(gh repo view --json url -q .url)"
echo ""
echo "✓ 上传完成！"
echo ""
echo "其他人现在可以用以下命令跳过本地 setup："
echo "  export AEGIS_RELEASE_URL=$REPO_URL/releases/download/$TAG"
echo "  bash scripts/build-circuits.sh"

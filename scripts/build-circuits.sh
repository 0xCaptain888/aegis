#!/usr/bin/env bash
# scripts/build-circuits.sh
#
# 两种模式（自动检测）:
#
#   模式 A — 快速模式（默认，推荐）
#     从 GitHub Release 下载预生成的 .zkey + .wasm + vkey.json，
#     完全跳过本地 Phase-2 setup（snarkjs groth16 setup 非常慢）。
#     只需 Node.js，不需要 circom，不需要 snarkjs CLI。
#     下载总量约 5–15 MB，30 秒内完成。
#
#   模式 B — 完整本地构建（传 --local 参数）
#     在本机从源码编译电路并跑 Phase-2 setup。
#     需要 circom ≥ 2.1.9 + snarkjs。Phase-2 需数分钟。
#     用于修改了电路后重新生成密钥，或审计验证。
#
# 用法:
#   bash scripts/build-circuits.sh           # 模式 A（下载预生成产物）
#   bash scripts/build-circuits.sh --local   # 模式 B（本地完整构建）
#   bash scripts/build-circuits.sh --verify  # 模式 A 下载后额外跑 snarkjs zkey verify
#
# 环境变量:
#   AEGIS_RELEASE_URL  覆盖 GitHub Release 的基础 URL（默认见下方）
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD="$ROOT/build"
mkdir -p "$BUILD"

# ── 模式判断 ──────────────────────────────────────────────────────────────
LOCAL_BUILD=0
DO_VERIFY=0
for arg in "$@"; do
  case "$arg" in
    --local)   LOCAL_BUILD=1 ;;
    --verify)  DO_VERIFY=1 ;;
  esac
done

# ── snarkjs 调用方式（两种模式共用）─────────────────────────────────────
# 优先用 prover/node_modules/.bin/snarkjs（npm install 装的本地二进制，不需要
# 任何全局/sudo 权限）；没有就退回 npx（同样不需要全局安装）。
# 不要求 `npm install -g snarkjs` —— 很多环境（包括容器/沙箱）没有全局安装权限，
# 全局安装也容易在沙箱里因权限不足直接失败。
snarkjs() {
  local bin="$ROOT/prover/node_modules/.bin/snarkjs"
  if [ -x "$bin" ]; then
    "$bin" "$@"
  elif command -v npx >/dev/null 2>&1; then
    npx --yes snarkjs "$@"
  else
    echo "ERROR: 找不到 snarkjs。先运行 'cd prover && npm install'，或确保 npx 可用。" >&2
    return 1
  fi
}

# ── GitHub Release URL（自动从 git remote 推导，无需手改）──────────────────
# 优先级: 环境变量 AEGIS_RELEASE_URL > 从 git remote origin 推导 > 占位符
# 这样在跑过 .github/workflows/release.yml 自动发布产物后，Mode A 直接可用。
AEGIS_RELEASE_TAG="${AEGIS_RELEASE_TAG:-v1.0.0-dev}"
default_release_base() {
  local remote owner_repo
  remote="$(git -C "$ROOT" remote get-url origin 2>/dev/null || true)"
  if [ -n "$remote" ]; then
    # 归一化 git@github.com:owner/repo.git 和 https://github.com/owner/repo(.git)
    owner_repo="$(printf '%s' "$remote" \
      | sed -E 's#^git@github.com:##; s#^https?://github.com/##; s#\.git$##')"
    if printf '%s' "$owner_repo" | grep -Eq '^[^/]+/[^/]+$'; then
      printf 'https://github.com/%s/releases/download/%s' "$owner_repo" "$AEGIS_RELEASE_TAG"
      return
    fi
  fi
  printf 'https://github.com/YOUR_GITHUB_USERNAME/aegis/releases/download/%s' "$AEGIS_RELEASE_TAG"
}
REPO_BASE="${AEGIS_RELEASE_URL:-$(default_release_base)}"

# 预生成产物的文件名列表（和 GitHub Release 上的一致）
ARTIFACTS=(
  "proof_of_reserves_final.zkey"
  "proof_of_reserves_js.tar.gz"        # 包含 proof_of_reserves.wasm + generate_witness.js
  "proof_of_reserves_vkey.json"
  "proof_of_reserves_vk_soroban.json"
  "eligibility_final.zkey"
  "eligibility_js.tar.gz"
  "eligibility_vkey.json"
  "eligibility_vk_soroban.json"
)

# ═══════════════════════════════════════════════════════════════════════════
# 模式 A: 下载预生成产物
# ═══════════════════════════════════════════════════════════════════════════
if [ "$LOCAL_BUILD" -eq 0 ]; then
  echo "=== Aegis build-circuits: 模式 A（下载预生成产物）==="
  echo "    Release URL: $REPO_BASE"
  echo ""

  # 检查 URL 是否已替换
  if echo "$REPO_BASE" | grep -q "YOUR_GITHUB_USERNAME"; then
    echo "⚠️  尚未配置 GitHub Release URL。"
    echo ""
    echo "操作方法（二选一）："
    echo ""
    echo "  选项 1 — 配置 Release URL 后下载（推荐）："
    echo "    1. 把代码 push 到你的 GitHub 仓库"
    echo "    2. 在联网机器上跑一次 'bash scripts/build-circuits.sh --local'"
    echo "    3. 把 build/ 下的产物上传到 GitHub Release（见下方说明）"
    echo "    4. 设置环境变量后再运行:"
    echo "       export AEGIS_RELEASE_URL=https://github.com/<你的用户名>/aegis/releases/download/v1.0.0-dev"
    echo "       bash scripts/build-circuits.sh"
    echo ""
    echo "  选项 2 — 直接本地构建（需要 circom + snarkjs，数分钟）："
    echo "    bash scripts/build-circuits.sh --local"
    echo ""
    echo "详细说明见 docs/SETUP.md 第 4 节。"
    exit 1
  fi

  # 下载函数（支持 curl / wget）
  download() {
    local url="$1" dest="$2"
    if [ -f "$dest" ]; then
      echo "  已缓存: $(basename "$dest")，跳过"
      return
    fi
    echo "  下载: $(basename "$dest")"
    if command -v curl >/dev/null 2>&1; then
      curl -fsSL --progress-bar "$url" -o "$dest"
    elif command -v wget >/dev/null 2>&1; then
      wget -q --show-progress "$url" -O "$dest"
    else
      echo "ERROR: 需要 curl 或 wget"; exit 1
    fi
  }

  echo "==> 下载预生成产物..."
  for f in "${ARTIFACTS[@]}"; do
    download "$REPO_BASE/$f" "$BUILD/$f"
  done

  # 解压 wasm 包
  echo "==> 解压 wasm 文件..."
  tar -xzf "$BUILD/proof_of_reserves_js.tar.gz" -C "$BUILD/"
  tar -xzf "$BUILD/eligibility_js.tar.gz"       -C "$BUILD/"

  echo ""
  echo "✓ 产物已就绪。"

  # 可选：用 snarkjs 验证 zkey 完整性
  if [ "$DO_VERIFY" -eq 1 ]; then
    echo "==> 验证 zkey 完整性（需要 snarkjs）..."
    # prover/ 可能还没装依赖（模式 A 默认不需要），这里临时装一下以便拿到本地 snarkjs 二进制
    [ -d "$ROOT/prover/node_modules" ] || ( cd "$ROOT/prover" && npm install --silent )
    PTAU="$BUILD/pot13_final.ptau"
    if [ ! -f "$PTAU" ]; then
      echo "    下载 Hermez ptau 用于验证（pot13，约 10MB，覆盖最多 8192 个约束）..."
      download "https://storage.googleapis.com/zkevm/ptau/powersOfTau28_hez_final_13.ptau" "$PTAU"
    fi
    echo "    验证 proof_of_reserves_final.zkey ..."
    snarkjs zkey verify \
      "$BUILD/proof_of_reserves.r1cs" "$PTAU" \
      "$BUILD/proof_of_reserves_final.zkey" && echo "    ✓ PoR zkey OK"
    echo "    验证 eligibility_final.zkey ..."
    snarkjs zkey verify \
      "$BUILD/eligibility.r1cs" "$PTAU" \
      "$BUILD/eligibility_final.zkey" && echo "    ✓ Eligibility zkey OK"
  fi

  echo ""
  echo "下一步: bash scripts/deploy.sh"
  exit 0
fi

# ═══════════════════════════════════════════════════════════════════════════
# 模式 B: 本地完整构建
# ═══════════════════════════════════════════════════════════════════════════
echo "=== Aegis build-circuits: 模式 B（本地完整构建）==="
echo "    注意：Phase-2 setup 在低配机器上需要数分钟，请耐心等待。"
echo ""

# snarkjs 的 Groth16 setup 是内存密集型操作。给 Node 一个较大的堆上限，避免在
# 内存受限的主机（容器/沙箱）上被 OOM 静默杀掉（表现为"命令不报错也不产出文件"）。
# 这正是导致 `npx snarkjs groth16 prove/setup` 静默失败的根因之一。
export NODE_OPTIONS="${NODE_OPTIONS:-} --max-old-space-size=8192"

# 检查依赖
command -v circom   >/dev/null 2>&1 || { echo "ERROR: 未安装 circom（见 docs/SETUP.md 第 3 节）"; exit 1; }
command -v node     >/dev/null 2>&1 || { echo "ERROR: 未安装 Node.js"; exit 1; }

# 安装 prover 依赖 + circomlib（snarkjs 作为 prover 的依赖一并装好，不需要全局安装）
echo "==> 安装依赖..."
( cd "$ROOT/prover" && npm install --silent )
if [ ! -d "$ROOT/node_modules/circomlib" ]; then
  ( cd "$ROOT" && npm init -y >/dev/null 2>&1 || true && npm install circomlib@2.0.5 --silent )
fi
# snarkjs() 函数已在脚本顶部定义（两种模式共用），这里 prover/node_modules 装好后
# 它会自动解析到本地二进制 prover/node_modules/.bin/snarkjs。

# 下载 Hermez ptau（Phase-1，一次性）
#
# 用 pot13 (2^13 = 8192 个约束容量) 而不是 pot16 (2^16 = 65536)：
# eligibility 电路实测约 6082 个约束，pot13 已经留有余量（~35%），文件从
# ~220MB 降到 ~10-15MB，下载更快、本地 Phase-2 setup 计算量也显著降低
# （Phase-2 耗时与 ptau 阶数近似线性相关）。如果未来电路约束数超过 8192，
# 把下面两行的 "13" 改成更大的阶数（14/15/16...），Hermez 的文件命名规律
# 是 powersOfTau28_hez_final_<N>.ptau。
PTAU="$BUILD/pot13_final.ptau"
PTAU_URL="https://storage.googleapis.com/zkevm/ptau/powersOfTau28_hez_final_13.ptau"
if [ ! -f "$PTAU" ]; then
  echo "==> 下载 Hermez Phase-1 ptau（pot13，约 10-15 MB，远小于之前用的 pot16，首次下载后缓存）..."
  if command -v curl >/dev/null 2>&1; then
    curl -L --progress-bar "$PTAU_URL" -o "$PTAU"
  else
    wget -q --show-progress "$PTAU_URL" -O "$PTAU"
  fi
fi

# 构建单条电路
build_circuit() {
  local NAME="$1" SRC="$2"
  echo ""
  echo "══ 编译 + setup: $NAME ══"

  echo "  → 编译电路..."
  circom "$SRC" --r1cs --wasm --sym -o "$BUILD" -l "$ROOT"

  echo "  → Groth16 Phase-2 setup（最慢的一步，请等待）..."
  snarkjs groth16 setup \
    "$BUILD/${NAME}.r1cs" "$PTAU" \
    "$BUILD/${NAME}_0000.zkey"

  echo "  → Phase-2 贡献（开发随机性）..."
  snarkjs zkey contribute \
    "$BUILD/${NAME}_0000.zkey" "$BUILD/${NAME}_final.zkey" \
    --name="aegis-dev-$(date +%s)" -v \
    -e="$(head -c 64 /dev/urandom | base64)"

  echo "  → 导出 vkey..."
  snarkjs zkey export verificationkey \
    "$BUILD/${NAME}_final.zkey" "$BUILD/${NAME}_vkey.json"

  echo "  → 转换为 Soroban 格式..."
  node "$ROOT/scripts/export-vk.mjs" \
    "$BUILD/${NAME}_vkey.json" "$BUILD/${NAME}_vk_soroban.json"

  echo "  ✓ $NAME 完成"
}

build_circuit "proof_of_reserves" \
  "$ROOT/circuits/proof_of_reserves/proof_of_reserves.circom"

build_circuit "eligibility" \
  "$ROOT/circuits/eligibility/eligibility.circom"

# 打包 wasm 目录（方便上传到 GitHub Release）
echo ""
echo "==> 打包 wasm 文件（用于 GitHub Release）..."
tar -czf "$BUILD/proof_of_reserves_js.tar.gz" -C "$BUILD" "proof_of_reserves_js/"
tar -czf "$BUILD/eligibility_js.tar.gz"       -C "$BUILD" "eligibility_js/"

echo ""
echo "✓ 本地构建完成。产物在 $BUILD:"
ls -lh "$BUILD" | grep -v '\.ptau$\|_0000\.zkey' | sed 's/^/   /'

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "把以下文件上传到 GitHub Release（tag: v1.0.0-dev）："
echo "  proof_of_reserves_final.zkey"
echo "  proof_of_reserves_js.tar.gz"
echo "  proof_of_reserves_vkey.json"
echo "  proof_of_reserves_vk_soroban.json"
echo "  eligibility_final.zkey"
echo "  eligibility_js.tar.gz"
echo "  eligibility_vkey.json"
echo "  eligibility_vk_soroban.json"
echo ""
echo "上传后，其他人只需:"
echo "  export AEGIS_RELEASE_URL=https://github.com/<你的用户名>/aegis/releases/download/v1.0.0-dev"
echo "  bash scripts/build-circuits.sh    # 几秒内完成，无需 circom/snarkjs"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "下一步: bash scripts/deploy.sh"

#!/usr/bin/env bash
# Linux 打包：用 nfpm 生成 deb / rpm 产物。
#
# 背景：Tauri CLI 2.12.4 起，tauri-bundler 改用 rpm crate 0.30 生成 RPM，
# 该版本严格校验 RPM Version 字符集（不允许 '-'），而版本号形如 1.0.0-beta.7
# 含 '-'，导致 `tauri build` 在 RPM 阶段失败。nfpm 会把 semver 拆成
# Version + Prerelease，生成合法且排序正确的 1.0.0~beta.7。
#
# 产物文件名与 Tauri 命名保持一致：tauri-apps/tauri-action@v1 按固定路径收集
# 产物（不扫描目录），路径不一致会导致产物被静默跳过、不会上传到 Release。
#
# 前置：已执行 `pnpm tauri build --no-bundle` 产出 release 二进制。
# 用法：bash ci/linux-package.sh
set -euo pipefail

# nfpm 固定版本，升级时同步修改此处
NFPM_VERSION=2.48.0

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

BINARY="src-tauri/target/release/git-repo-sync"

# 1. 版本与产品名（tauri-action 也从 tauri.conf.json 取值，来源保持一致）
VERSION="$(node -p "require('./package.json').version")"
PRODUCT="$(node -p "require('./src-tauri/tauri.conf.json').productName")"
TAURI_VERSION="$(node -p "require('./src-tauri/tauri.conf.json').version")"
if [ "$VERSION" != "$TAURI_VERSION" ]; then
  echo "版本不一致：package.json=$VERSION，tauri.conf.json=$TAURI_VERSION（请先运行 version.ps1）" >&2
  exit 1
fi
export VERSION PRODUCT

# 2. 二进制必须已编译出来
if [ ! -f "$BINARY" ]; then
  echo "未找到二进制 $BINARY，请先执行 pnpm tauri build --no-bundle" >&2
  exit 1
fi

# 3. 安装 nfpm：固定版本 + sha256 校验（不写入系统目录，无需 sudo）
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT
TARBALL="nfpm_${NFPM_VERSION}_Linux_x86_64.tar.gz"
BASE_URL="https://github.com/goreleaser/nfpm/releases/download/v${NFPM_VERSION}"
curl -fsSL -o "$TMP_DIR/$TARBALL" "$BASE_URL/$TARBALL"
curl -fsSL -o "$TMP_DIR/checksums.txt" "$BASE_URL/checksums.txt"
EXPECTED_SHA="$(awk -v f="$TARBALL" '$2 == f || $2 == "*" f { print $1; exit }' "$TMP_DIR/checksums.txt")"
if [ -z "$EXPECTED_SHA" ]; then
  echo "checksums.txt 中没有 $TARBALL 的校验值" >&2
  exit 1
fi
ACTUAL_SHA="$(sha256sum "$TMP_DIR/$TARBALL" | awk '{ print $1 }')"
if [ "$EXPECTED_SHA" != "$ACTUAL_SHA" ]; then
  echo "nfpm 校验失败：期望 $EXPECTED_SHA，实际 $ACTUAL_SHA" >&2
  exit 1
fi
tar -xzf "$TMP_DIR/$TARBALL" -C "$TMP_DIR" nfpm
NFPM="$TMP_DIR/nfpm"

# 4. 生成 deb / rpm（产物路径与 tauri-action 的期望路径逐一对应）
DEB_PATH="src-tauri/target/release/bundle/deb/${PRODUCT}_${VERSION}_amd64.deb"
RPM_PATH="src-tauri/target/release/bundle/rpm/${PRODUCT}-${VERSION}-1.x86_64.rpm"
mkdir -p "$(dirname "$DEB_PATH")" "$(dirname "$RPM_PATH")"
"$NFPM" package --packager deb --config ci/nfpm.yaml --target "$DEB_PATH"
"$NFPM" package --packager rpm --config ci/nfpm.yaml --target "$RPM_PATH"

# 5. 自检：包内版本字段必须是 RPM / Debian 合法且排序正确的形式
if [[ "$VERSION" == *-* ]]; then
  EXPECTED_DEB_VERSION="${VERSION%%-*}~${VERSION#*-}"
else
  EXPECTED_DEB_VERSION="$VERSION"
fi
EXPECTED_RPM_VERSION="${EXPECTED_DEB_VERSION}-1"

if command -v dpkg-deb >/dev/null 2>&1; then
  DEB_VERSION="$(dpkg-deb --info "$DEB_PATH" | awk '/^Version:/ { print $2 }')"
  if [ "$DEB_VERSION" != "$EXPECTED_DEB_VERSION" ]; then
    echo "deb 版本不符：期望 $EXPECTED_DEB_VERSION，实际 $DEB_VERSION" >&2
    exit 1
  fi
  echo "deb 版本自检通过：$DEB_VERSION"
else
  echo "未找到 dpkg-deb，跳过 deb 版本自检"
fi

if command -v rpm >/dev/null 2>&1; then
  RPM_VERSION="$(rpm -qp --qf '%{VERSION}-%{RELEASE}' "$RPM_PATH")"
  if [ "$RPM_VERSION" != "$EXPECTED_RPM_VERSION" ]; then
    echo "rpm 版本不符：期望 $EXPECTED_RPM_VERSION，实际 $RPM_VERSION" >&2
    exit 1
  fi
  echo "rpm 版本自检通过：$RPM_VERSION"
else
  echo "未找到 rpm 命令，跳过 rpm 版本自检"
fi

# 6. 打印最终产物路径（与 tauri-action 期望路径核对）
for path in "$DEB_PATH" "$RPM_PATH"; do
  if [ ! -s "$path" ]; then
    echo "产物缺失或为空：$path" >&2
    exit 1
  fi
  echo "产物：$path（$(stat -c %s "$path") 字节）"
done

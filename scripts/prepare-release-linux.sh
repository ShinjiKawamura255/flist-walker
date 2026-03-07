#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  scripts/prepare-release-linux.sh <version> [target]

Examples:
  scripts/prepare-release-linux.sh v0.9.0
  scripts/prepare-release-linux.sh v0.9.0 x86_64-unknown-linux-gnu

Notes:
  - Requires rust/target/.../release/flistwalker
  - Produces dist/<version>/ with:
    - FlistWalker-<version>-linux-<arch>
    - FlistWalker-<version>-linux-<arch>.tar.gz
    - SHA256SUMS
USAGE
}

if [[ ${1:-} == "-h" || ${1:-} == "--help" || $# -lt 1 || $# -gt 2 ]]; then
  usage
  exit $([[ $# -ge 1 && $# -le 2 ]] && echo 0 || echo 1)
fi

VERSION="$1"
TARGET="${2:-}"
SAFE_VERSION="${VERSION#v}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

if [[ -n "${TARGET}" ]]; then
  SOURCE_BIN="${REPO_DIR}/rust/target/${TARGET}/release/flistwalker"
  TARGET_ARCH="${TARGET%%-*}"
else
  SOURCE_BIN="${REPO_DIR}/rust/target/release/flistwalker"
  TARGET_ARCH="$(uname -m)"
fi

case "${TARGET_ARCH}" in
  aarch64 | arm64)
    ARCH_LABEL="arm64"
    ;;
  x86_64 | amd64)
    ARCH_LABEL="x86_64"
    ;;
  *)
    ARCH_LABEL="${TARGET_ARCH}"
    ;;
esac

OUT_DIR="${REPO_DIR}/dist/${VERSION}"
ASSET_BASENAME="FlistWalker-${SAFE_VERSION}-linux-${ARCH_LABEL}"
BIN_NAME="${ASSET_BASENAME}"
TAR_NAME="${ASSET_BASENAME}.tar.gz"
TAR_BIN_NAME="flistwalker"

if [[ ! -f "${SOURCE_BIN}" ]]; then
  echo "バイナリが見つかりません: ${SOURCE_BIN}" >&2
  echo "先に Linux 向け release build を実行してください。" >&2
  exit 1
fi

mkdir -p "${OUT_DIR}"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "${WORK_DIR}"' EXIT

cp -f "${SOURCE_BIN}" "${OUT_DIR}/${BIN_NAME}"
chmod +x "${OUT_DIR}/${BIN_NAME}"

cp -f "${SOURCE_BIN}" "${WORK_DIR}/${TAR_BIN_NAME}"
chmod +x "${WORK_DIR}/${TAR_BIN_NAME}"
cat > "${WORK_DIR}/README.txt" <<README
FlistWalker ${VERSION}

Contents:
- ${TAR_BIN_NAME}
- README.txt

Run:
- chmod +x ./${TAR_BIN_NAME}
- ./${TAR_BIN_NAME}

Basic usage:
- 起動後に検索窓へ文字を入力すると、ファイル/フォルダを絞り込みます。
- Enter で開く/実行、Shift+Enter で格納フォルダを開く（同一フォルダは1回のみ）、Tab でピン留め複数選択、Ctrl+Shift+C でパスコピー。
- Root は左上の Browse... から切り替え可能です。

Search hints:
- トークンは AND 条件（例: main py）
- abc|foo|bar : OR 条件（スペースなしの | で連結）
- 'term : 完全一致トークン（例: 'main.py）
- !term : 除外トークン（例: main !test）
- ^term : 先頭一致を優先（例: ^src）
- term\$ : 末尾一致を優先（例: .rs\$）
- Regex チェックON時は正規表現検索

Keyboard shortcuts:
- Up/Down or Ctrl+P/Ctrl+N: move current row
- Ctrl+V / Alt+V: page down / page up
- Enter (or Ctrl+J/Ctrl+M): open/execute selected item(s)
- Shift+Enter: open containing folder(s) for selected item(s), deduplicated by directory
- Tab / Shift+Tab: toggle pin on current row
- Ctrl+Shift+C: copy selected path(s)
- Ctrl+R: fuzzy-search query history and load the selected entry
- Esc / Ctrl+G: clear query and pinned selection, or cancel history search
- Ctrl+L: focus query input

Index options:
- Use FileList: ルート直下の FileList.txt / filelist.txt を優先使用
- Files / Folders: 表示フィルタ（再インデックスなしで即時反映）
- Refresh Index: 現在Rootで再インデックス

Walker tuning (Environment variables):
- FLISTWALKER_WALKER_MAX_ENTRIES: Walkerの最大走査件数（既定: 500000）
- FLISTWALKER_WALKER_THREADS: Walkerの並列スレッド数（既定: 2、1でシリアル）
README

(
  cd "${WORK_DIR}"
  tar -czf "${OUT_DIR}/${TAR_NAME}" "${TAR_BIN_NAME}" README.txt
)

if command -v sha256sum >/dev/null 2>&1; then
  (
    cd "${OUT_DIR}"
    sha256sum "${BIN_NAME}" "${TAR_NAME}" > SHA256SUMS
  )
elif command -v shasum >/dev/null 2>&1; then
  (
    cd "${OUT_DIR}"
    shasum -a 256 "${BIN_NAME}" "${TAR_NAME}" > SHA256SUMS
  )
else
  echo "sha256sum/shasum が見つかりません。SHA256SUMS を生成できませんでした。" >&2
  exit 1
fi

echo "Release assets created: ${OUT_DIR}"
echo "- ${BIN_NAME}"
echo "- ${TAR_NAME}"
echo "- SHA256SUMS"

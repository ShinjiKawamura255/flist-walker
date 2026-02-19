#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  scripts/prepare-release.sh <version>

Example:
  scripts/prepare-release.sh v0.1.0

Notes:
  - Requires rust/target/x86_64-pc-windows-msvc/release/FlistWalker.exe
  - Produces dist/<version>/ with:
    - FlistWalker-<version>-windows-x86_64.exe
    - FlistWalker-<version>-windows-x86_64.zip
    - SHA256SUMS
USAGE
}

if [[ ${1:-} == "-h" || ${1:-} == "--help" || $# -ne 1 ]]; then
  usage
  exit $([[ $# -eq 1 ]] && echo 0 || echo 1)
fi

VERSION="$1"
SAFE_VERSION="${VERSION#v}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
TARGET="x86_64-pc-windows-msvc"
SOURCE_EXE="${REPO_DIR}/rust/target/${TARGET}/release/FlistWalker.exe"
OUT_DIR="${REPO_DIR}/dist/${VERSION}"
ASSET_BASENAME="FlistWalker-${SAFE_VERSION}-windows-x86_64"
EXE_NAME="${ASSET_BASENAME}.exe"
ZIP_NAME="${ASSET_BASENAME}.zip"
ZIP_EXE_NAME="flistwalker.exe"

if [[ ! -f "${SOURCE_EXE}" ]]; then
  echo "EXE が見つかりません: ${SOURCE_EXE}" >&2
  echo "先に scripts/build-rust-win.sh を実行してください。" >&2
  exit 1
fi

mkdir -p "${OUT_DIR}"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "${WORK_DIR}"' EXIT

cp -f "${SOURCE_EXE}" "${OUT_DIR}/${EXE_NAME}"
cp -f "${SOURCE_EXE}" "${WORK_DIR}/${ZIP_EXE_NAME}"
cat > "${WORK_DIR}/README.txt" <<README
FlistWalker ${VERSION}

Contents:
- ${ZIP_EXE_NAME}
- README.txt

Run:
- PowerShell: .\\${ZIP_EXE_NAME}
- CMD: ${ZIP_EXE_NAME}

Basic usage:
- 起動後に検索窓へ文字を入力すると、ファイル/フォルダを絞り込みます。
- Enter で開く/実行、Tab でピン留め複数選択、Ctrl+Shift+C でパスコピー。
- Root は左上の Browse... から切り替え可能です。

Search hints:
- トークンは AND 条件（例: main py）
- '\''term : 完全一致トークン（例: '\''main.py）
- !term : 除外トークン（例: main !test）
- ^term : 先頭一致を優先（例: ^src）
- term$ : 末尾一致を優先（例: .rs$）
- Regex チェックON時は正規表現検索

Keyboard shortcuts:
- Up/Down or Ctrl+P/Ctrl+N: move current row
- Ctrl+V / Alt+V: page down / page up
- Enter (or Ctrl+J/Ctrl+M): open/execute selected item(s)
- Tab / Shift+Tab: toggle pin and move next/prev
- Ctrl+Shift+C: copy selected path(s)
- Ctrl+G: clear query and pinned selection
- Ctrl+L: focus query input

Index options:
- Use FileList: ルート直下の FileList.txt / filelist.txt を優先使用
- Files / Folders: 表示フィルタ（再インデックスなしで即時反映）
- Refresh Index: 現在Rootで再インデックス
README

(
  cd "${WORK_DIR}"
  zip -q -9 "${OUT_DIR}/${ZIP_NAME}" "${ZIP_EXE_NAME}" README.txt
)

if command -v sha256sum >/dev/null 2>&1; then
  (
    cd "${OUT_DIR}"
    sha256sum "${EXE_NAME}" "${ZIP_NAME}" > SHA256SUMS
  )
elif command -v shasum >/dev/null 2>&1; then
  (
    cd "${OUT_DIR}"
    shasum -a 256 "${EXE_NAME}" "${ZIP_NAME}" > SHA256SUMS
  )
else
  echo "sha256sum/shasum が見つかりません。SHA256SUMS を生成できませんでした。" >&2
  exit 1
fi

echo "Release assets created: ${OUT_DIR}"
echo "- ${EXE_NAME}"
echo "- ${ZIP_NAME}"
echo "- SHA256SUMS"

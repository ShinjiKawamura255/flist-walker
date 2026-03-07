#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  scripts/prepare-release-macos.sh <version> [target]

Examples:
  scripts/prepare-release-macos.sh v0.8.0
  scripts/prepare-release-macos.sh v0.8.0 aarch64-apple-darwin

Notes:
  - Requires rust/target/.../release/flistwalker
  - Produces dist/<version>/ with:
    - FlistWalker-<version>-macos-<arch>
    - FlistWalker-<version>-macos-<arch>.app
    - FlistWalker-<version>-macos-<arch>-app.zip
    - FlistWalker-<version>-macos-<arch>.tar.gz
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
ASSET_BASENAME="FlistWalker-${SAFE_VERSION}-macos-${ARCH_LABEL}"
BIN_NAME="${ASSET_BASENAME}"
APP_NAME="${ASSET_BASENAME}.app"
APP_ZIP_NAME="${ASSET_BASENAME}-app.zip"
TAR_NAME="${ASSET_BASENAME}.tar.gz"
TAR_BIN_NAME="flistwalker"
APP_EXECUTABLE_NAME="FlistWalker"
APP_ICON_NAME="FlistWalker.icns"
APP_BUNDLE_ID="com.flistwalker.app"
ICON_SVG="${REPO_DIR}/rust/assets/flistwalker-icon.svg"

if [[ ! -f "${SOURCE_BIN}" ]]; then
  echo "バイナリが見つかりません: ${SOURCE_BIN}" >&2
  echo "先に scripts/build-rust-macos.sh を実行してください。" >&2
  exit 1
fi
if [[ ! -f "${ICON_SVG}" ]]; then
  echo "アイコンSVGが見つかりません: ${ICON_SVG}" >&2
  exit 1
fi
if ! command -v iconutil >/dev/null 2>&1; then
  echo "iconutil が見つかりません。macOS 環境で実行してください。" >&2
  exit 1
fi
if ! command -v sips >/dev/null 2>&1; then
  echo "sips が見つかりません。macOS 環境で実行してください。" >&2
  exit 1
fi

mkdir -p "${OUT_DIR}"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "${WORK_DIR}"' EXIT

cp -f "${SOURCE_BIN}" "${OUT_DIR}/${BIN_NAME}"
chmod +x "${OUT_DIR}/${BIN_NAME}"

APP_DIR="${WORK_DIR}/${APP_NAME}"
APP_CONTENTS="${APP_DIR}/Contents"
APP_MACOS_DIR="${APP_CONTENTS}/MacOS"
APP_RESOURCES_DIR="${APP_CONTENTS}/Resources"
mkdir -p "${APP_MACOS_DIR}" "${APP_RESOURCES_DIR}"
cp -f "${SOURCE_BIN}" "${APP_MACOS_DIR}/${APP_EXECUTABLE_NAME}"
chmod +x "${APP_MACOS_DIR}/${APP_EXECUTABLE_NAME}"

ICONSET_DIR="${WORK_DIR}/flistwalker.iconset"
mkdir -p "${ICONSET_DIR}"
BASE_PNG="${WORK_DIR}/icon_1024.png"
(
  cd "${REPO_DIR}/rust"
  cargo run --quiet --bin render_svg_png --release -- "${ICON_SVG}" "${BASE_PNG}" 1024
)
sips -s format png -z 16 16 "${BASE_PNG}" --out "${ICONSET_DIR}/icon_16x16.png" >/dev/null
sips -s format png -z 32 32 "${BASE_PNG}" --out "${ICONSET_DIR}/icon_16x16@2x.png" >/dev/null
sips -s format png -z 32 32 "${BASE_PNG}" --out "${ICONSET_DIR}/icon_32x32.png" >/dev/null
sips -s format png -z 64 64 "${BASE_PNG}" --out "${ICONSET_DIR}/icon_32x32@2x.png" >/dev/null
sips -s format png -z 128 128 "${BASE_PNG}" --out "${ICONSET_DIR}/icon_128x128.png" >/dev/null
sips -s format png -z 256 256 "${BASE_PNG}" --out "${ICONSET_DIR}/icon_128x128@2x.png" >/dev/null
sips -s format png -z 256 256 "${BASE_PNG}" --out "${ICONSET_DIR}/icon_256x256.png" >/dev/null
sips -s format png -z 512 512 "${BASE_PNG}" --out "${ICONSET_DIR}/icon_256x256@2x.png" >/dev/null
sips -s format png -z 512 512 "${BASE_PNG}" --out "${ICONSET_DIR}/icon_512x512.png" >/dev/null
sips -s format png -z 1024 1024 "${BASE_PNG}" --out "${ICONSET_DIR}/icon_512x512@2x.png" >/dev/null
iconutil -c icns "${ICONSET_DIR}" -o "${APP_RESOURCES_DIR}/${APP_ICON_NAME}"

cat > "${APP_CONTENTS}/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>
  <string>FlistWalker</string>
  <key>CFBundleDisplayName</key>
  <string>FlistWalker</string>
  <key>CFBundleIdentifier</key>
  <string>${APP_BUNDLE_ID}</string>
  <key>CFBundleVersion</key>
  <string>${SAFE_VERSION}</string>
  <key>CFBundleShortVersionString</key>
  <string>${SAFE_VERSION}</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleExecutable</key>
  <string>${APP_EXECUTABLE_NAME}</string>
  <key>CFBundleIconFile</key>
  <string>${APP_ICON_NAME}</string>
  <key>LSMinimumSystemVersion</key>
  <string>11.0</string>
</dict>
</plist>
PLIST

cp -R "${APP_DIR}" "${OUT_DIR}/${APP_NAME}"

(
  cd "${WORK_DIR}"
  ditto -c -k --sequesterRsrc --keepParent "${APP_NAME}" "${OUT_DIR}/${APP_ZIP_NAME}"
)

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
- Enter で開く/実行、Shift+Enter で格納フォルダを開く（同一フォルダは1回のみ）、Tab でピン留め複数選択、Cmd+Shift+C でパスコピー。
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
- Cmd+Shift+C: copy selected path(s)
- Esc / Ctrl+G: clear query and pinned selection, or cancel history search
- Cmd+L: focus query input

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

if command -v shasum >/dev/null 2>&1; then
  (
    cd "${OUT_DIR}"
    shasum -a 256 "${BIN_NAME}" "${APP_ZIP_NAME}" "${TAR_NAME}" > SHA256SUMS
  )
elif command -v sha256sum >/dev/null 2>&1; then
  (
    cd "${OUT_DIR}"
    sha256sum "${BIN_NAME}" "${APP_ZIP_NAME}" "${TAR_NAME}" > SHA256SUMS
  )
else
  echo "shasum/sha256sum が見つかりません。SHA256SUMS を生成できませんでした。" >&2
  exit 1
fi

echo "Release assets created: ${OUT_DIR}"
echo "- ${BIN_NAME}"
echo "- ${APP_NAME}"
echo "- ${APP_ZIP_NAME}"
echo "- ${TAR_NAME}"
echo "- SHA256SUMS"

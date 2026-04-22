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
    - FlistWalker-<version>-macos-<arch>.README.txt
    - FlistWalker-<version>-macos-<arch>.LICENSE.txt
    - FlistWalker-<version>-macos-<arch>.THIRD_PARTY_NOTICES.txt
    - FlistWalker-<version>-macos-<arch>.ignore.txt.example
    - SHA256SUMS
    - SHA256SUMS.sig (when FLISTWALKER_UPDATE_SIGNING_KEY_HEX is set)
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
README_SIDE_NAME="${ASSET_BASENAME}.README.txt"
LICENSE_SIDE_NAME="${ASSET_BASENAME}.LICENSE.txt"
NOTICES_SIDE_NAME="${ASSET_BASENAME}.THIRD_PARTY_NOTICES.txt"
IGNORE_SAMPLE_SIDE_NAME="${ASSET_BASENAME}.ignore.txt.example"
APP_EXECUTABLE_NAME="FlistWalker"
APP_ICON_NAME="FlistWalker.icns"
APP_BUNDLE_ID="com.flistwalker.app"
ICON_SVG="${REPO_DIR}/rust/assets/flistwalker-icon.svg"
ROOT_LICENSE="${REPO_DIR}/LICENSE"
ROOT_NOTICES="${REPO_DIR}/THIRD_PARTY_NOTICES.txt"
ROOT_IGNORE_SAMPLE="${REPO_DIR}/flistwalker.ignore.txt.example"

if [[ ! -f "${SOURCE_BIN}" ]]; then
  echo "バイナリが見つかりません: ${SOURCE_BIN}" >&2
  echo "先に scripts/build-rust-macos.sh を実行してください。" >&2
  exit 1
fi
if [[ ! -f "${ICON_SVG}" ]]; then
  echo "アイコンSVGが見つかりません: ${ICON_SVG}" >&2
  exit 1
fi
if [[ ! -f "${ROOT_LICENSE}" || ! -f "${ROOT_NOTICES}" ]]; then
  echo "LICENSE / THIRD_PARTY_NOTICES.txt が見つかりません。" >&2
  exit 1
fi
if [[ ! -f "${ROOT_IGNORE_SAMPLE}" ]]; then
  echo "flistwalker.ignore.txt.example が見つかりません。" >&2
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
cat > "${OUT_DIR}/${README_SIDE_NAME}" <<'README'
FlistWalker __VERSION__

Contents:
- __TAR_BIN_NAME__
- README.txt
- LICENSE.txt
- THIRD_PARTY_NOTICES.txt

Run:
- chmod +x ./__TAR_BIN_NAME__
- ./__TAR_BIN_NAME__

English:
- Type in the search box to narrow files and folders.
- Press Enter to open or execute. Press Shift+Enter to open the containing folder. Press Tab to pin rows. Press Cmd+Shift+C to copy selected paths.
- Use Browse... to switch the root.

Search hints:
- Tokens are ANDed by default, for example main py
- abc|foo|bar means OR
- exact match uses a leading apostrophe, for example 'main.py
- !term excludes a token, for example main !test
- ^term prefers prefix matches, for example ^src
- suffix match uses a trailing $, for example .rs$
- Turn on Regex to use regular-expression search

Ignore list:
- Put flistwalker.ignore.txt in the same folder as the executable.
- Blank lines and lines starting with # are ignored.
- Each token is treated like a search exclusion, so old and ~ behave like !old !~
- The Use Ignore List checkbox controls whether these rules apply. It is on by default.
- A sample ignore list is included as flistwalker.ignore.txt.example.

Runtime config:
- Runtime settings are stored in ~/.flistwalker_config.json in your home directory.
- On first launch, if the file is missing, FlistWalker creates it from the current FLISTWALKER_* environment values.
- Once the file exists, it becomes the source of truth for runtime settings and the matching environment variables are only an initial seed.
- The file is JSON and can be edited directly.
- Only the commonly useful toggles are documented here. Advanced keys are intentionally undocumented.
- walker_max_entries is also documented here because it affects large-root scans.

Keyboard shortcuts:
- Up/Down or Ctrl+P/Ctrl+N: move the current row
- Ctrl+V / Alt+V: page down / page up
- Enter or Ctrl+J/Ctrl+M: open or execute selected item(s)
- Shift+Enter: open the containing folder(s) for selected item(s), deduplicated by directory
- Tab / Shift+Tab: toggle pin on the current row
- Cmd+Shift+C: copy selected path(s)
- Esc / Ctrl+G: clear query and pinned selection, or cancel history search
- Cmd+L: focus the query input

Query history:
- Search history is shared across tabs and persisted up to 100 entries.
- History is committed after a short idle period or when result navigation starts.
- Intermediate IME composition text is not stored; only committed query text is saved.

Tab restore (opt-in):
- Set FLISTWALKER_RESTORE_TABS=1 to restore the previous tab state on the next launch.
- Restored fields include root, query, Use FileList, Regex, Files, Folders, and the active tab.
- If you explicitly pass --root or a startup query, those values take precedence.
- When this feature is enabled, Set as default is disabled because the startup root is determined by tab restore.

How to enable FLISTWALKER_RESTORE_TABS:
- macOS (zsh/bash, current session): export FLISTWALKER_RESTORE_TABS=1
- macOS (zsh persist): echo 'export FLISTWALKER_RESTORE_TABS=1' >> ~/.zshrc
- macOS (fish, persist): set -Ux FLISTWALKER_RESTORE_TABS 1
- Linux (bash, current session): export FLISTWALKER_RESTORE_TABS=1
- Linux (bash persist): echo 'export FLISTWALKER_RESTORE_TABS=1' >> ~/.bashrc
- Windows (PowerShell, persist per-user): [Environment]::SetEnvironmentVariable("FLISTWALKER_RESTORE_TABS", "1", "User")
- Windows (PowerShell, current session): \$env:FLISTWALKER_RESTORE_TABS = "1"

Index options:
- Use FileList: prefer FileList.txt / filelist.txt at the repository root
- Files / Folders: toggle visibility without reindexing
- Refresh Index: reindex the current root

Walker tuning (Environment variables):
- FLISTWALKER_WALKER_MAX_ENTRIES: walker maximum entry count (default: 500000)
- FLISTWALKER_WALKER_THREADS: walker parallel thread count (default: 2, 1 means serial)

日本語:
- 起動後に検索窓へ文字を入力すると、ファイル/フォルダを絞り込みます。
- Enter で開く/実行、Shift+Enter で格納フォルダを開く（同一フォルダは1回のみ）、Tab でピン留め複数選択、Cmd+Shift+C でパスコピー。
- Root は左上の Browse... から切り替え可能です。
- トークンは AND 条件（例: main py）
- abc|foo|bar : OR 条件（スペースなしの | で連結）
- 完全一致トークンは先頭に ' を付ける（例: 'main.py）
- !term : 除外トークン（例: main !test）
- ^term : 先頭一致を優先（例: ^src）
- 末尾一致は末尾に $ を付ける（例: .rs$）
- Regex チェックON時は正規表現検索

Ignore List:
- flistwalker.ignore.txt を実行ファイルと同じフォルダに置きます。
- 空行と # で始まる行は無視されます。
- 各トークンは検索の除外条件として扱われるため、old や ~ は !old !~ と同じ挙動になります。
- Use Ignore List チェックボックスで適用の ON/OFF を切り替えます。既定は ON です。
- サンプルの ignore list は flistwalker.ignore.txt.example として同梱しています。

Runtime config:
- runtime settings は home directory の ~/.flistwalker_config.json に保存されます。
- 初回起動でファイルが無い場合は、現在の FLISTWALKER_* 環境変数を seed にして自動生成します。
- 一度ファイルができたら、その内容が runtime settings の source of truth になり、同名 env は初期 seed としてのみ使われます。
 - ここでは一般的に使う項目だけを案内しています。高度な項目は意図的に記載していません。
- 検索履歴は全タブ共通で最大100件まで保持され、短い待機後または結果移動開始時に確定します。
- FLISTWALKER_RESTORE_TABS=1 を設定すると、終了時のタブ状態を次回起動時に復元できます。
- Use FileList はルート直下の FileList.txt / filelist.txt を優先使用します。
- Refresh Index は現在Rootで再インデックスします。
README
sed -i '' \
  -e "s#__VERSION__#${VERSION}#g" \
  -e "s#__TAR_BIN_NAME__#${TAR_BIN_NAME}#g" \
  "${OUT_DIR}/${README_SIDE_NAME}"
cp -f "${ROOT_LICENSE}" "${OUT_DIR}/${LICENSE_SIDE_NAME}"
cp -f "${ROOT_NOTICES}" "${OUT_DIR}/${NOTICES_SIDE_NAME}"
cp -f "${ROOT_IGNORE_SAMPLE}" "${OUT_DIR}/${IGNORE_SAMPLE_SIDE_NAME}"

APP_DIR="${WORK_DIR}/${APP_NAME}"
APP_CONTENTS="${APP_DIR}/Contents"
APP_MACOS_DIR="${APP_CONTENTS}/MacOS"
APP_RESOURCES_DIR="${APP_CONTENTS}/Resources"
mkdir -p "${APP_MACOS_DIR}" "${APP_RESOURCES_DIR}"
cp -f "${SOURCE_BIN}" "${APP_MACOS_DIR}/${APP_EXECUTABLE_NAME}"
chmod +x "${APP_MACOS_DIR}/${APP_EXECUTABLE_NAME}"
cp -f "${ROOT_LICENSE}" "${APP_RESOURCES_DIR}/LICENSE.txt"
cp -f "${ROOT_NOTICES}" "${APP_RESOURCES_DIR}/THIRD_PARTY_NOTICES.txt"
cp -f "${ROOT_IGNORE_SAMPLE}" "${APP_MACOS_DIR}/flistwalker.ignore.txt.example"

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
cp -f "${OUT_DIR}/${README_SIDE_NAME}" "${WORK_DIR}/README.txt"
cp -f "${ROOT_LICENSE}" "${WORK_DIR}/LICENSE.txt"
cp -f "${ROOT_NOTICES}" "${WORK_DIR}/THIRD_PARTY_NOTICES.txt"
cp -f "${ROOT_IGNORE_SAMPLE}" "${WORK_DIR}/flistwalker.ignore.txt.example"

(
  cd "${WORK_DIR}"
  tar -czf "${OUT_DIR}/${TAR_NAME}" "${TAR_BIN_NAME}" README.txt LICENSE.txt THIRD_PARTY_NOTICES.txt flistwalker.ignore.txt.example
)

if command -v shasum >/dev/null 2>&1; then
  (
    cd "${OUT_DIR}"
    shasum -a 256 "${BIN_NAME}" "${APP_ZIP_NAME}" "${TAR_NAME}" "${README_SIDE_NAME}" "${LICENSE_SIDE_NAME}" "${NOTICES_SIDE_NAME}" "${IGNORE_SAMPLE_SIDE_NAME}" > SHA256SUMS
  )
elif command -v sha256sum >/dev/null 2>&1; then
  (
    cd "${OUT_DIR}"
    sha256sum "${BIN_NAME}" "${APP_ZIP_NAME}" "${TAR_NAME}" "${README_SIDE_NAME}" "${LICENSE_SIDE_NAME}" "${NOTICES_SIDE_NAME}" "${IGNORE_SAMPLE_SIDE_NAME}" > SHA256SUMS
  )
else
  echo "shasum/sha256sum が見つかりません。SHA256SUMS を生成できませんでした。" >&2
  exit 1
fi

if [[ -n "${FLISTWALKER_UPDATE_SIGNING_KEY_HEX:-}" ]]; then
  cargo run --manifest-path "${REPO_DIR}/rust/Cargo.toml" --quiet --bin sign_update_manifest -- \
    "${OUT_DIR}/SHA256SUMS" "${OUT_DIR}/SHA256SUMS.sig"
fi

echo "Release assets created: ${OUT_DIR}"
echo "- ${BIN_NAME}"
echo "- ${APP_NAME}"
echo "- ${APP_ZIP_NAME}"
echo "- ${TAR_NAME}"
echo "- ${README_SIDE_NAME}"
echo "- ${LICENSE_SIDE_NAME}"
echo "- ${NOTICES_SIDE_NAME}"
echo "- ${IGNORE_SAMPLE_SIDE_NAME}"
echo "- SHA256SUMS"
if [[ -n "${FLISTWALKER_UPDATE_SIGNING_KEY_HEX:-}" ]]; then
  echo "- SHA256SUMS.sig"
fi

#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  scripts/prepare-release.sh <version>

Example:
  scripts/prepare-release.sh v0.1.0

Notes:
  - Requires rust/target/x86_64-pc-windows-gnu/release/FlistWalker.exe
  - Produces dist/<version>/ with:
    - FlistWalker-<version>-windows-x86_64.exe
    - FlistWalker-<version>-windows-x86_64.zip
    - FlistWalker-<version>-windows-x86_64.README.txt
    - FlistWalker-<version>-windows-x86_64.LICENSE.txt
    - FlistWalker-<version>-windows-x86_64.THIRD_PARTY_NOTICES.txt
    - SHA256SUMS
    - SHA256SUMS.sig (when FLISTWALKER_UPDATE_SIGNING_KEY_HEX is set)
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
TARGET="x86_64-pc-windows-gnu"
SOURCE_EXE="${REPO_DIR}/rust/target/${TARGET}/release/FlistWalker.exe"
OUT_DIR="${REPO_DIR}/dist/${VERSION}"
ASSET_BASENAME="FlistWalker-${SAFE_VERSION}-windows-x86_64"
EXE_NAME="${ASSET_BASENAME}.exe"
ZIP_NAME="${ASSET_BASENAME}.zip"
ZIP_EXE_NAME="flistwalker.exe"
README_SIDE_NAME="${ASSET_BASENAME}.README.txt"
LICENSE_SIDE_NAME="${ASSET_BASENAME}.LICENSE.txt"
NOTICES_SIDE_NAME="${ASSET_BASENAME}.THIRD_PARTY_NOTICES.txt"
ROOT_LICENSE="${REPO_DIR}/LICENSE"
ROOT_NOTICES="${REPO_DIR}/THIRD_PARTY_NOTICES.txt"

if [[ ! -f "${SOURCE_EXE}" ]]; then
  echo "EXE が見つかりません: ${SOURCE_EXE}" >&2
  echo "先に scripts/build-rust-win.sh を実行してください。" >&2
  exit 1
fi
if [[ ! -f "${ROOT_LICENSE}" || ! -f "${ROOT_NOTICES}" ]]; then
  echo "LICENSE / THIRD_PARTY_NOTICES.txt が見つかりません。" >&2
  exit 1
fi

mkdir -p "${OUT_DIR}"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "${WORK_DIR}"' EXIT

cp -f "${SOURCE_EXE}" "${OUT_DIR}/${EXE_NAME}"
cp -f "${SOURCE_EXE}" "${WORK_DIR}/${ZIP_EXE_NAME}"
cat > "${OUT_DIR}/${README_SIDE_NAME}" <<README
FlistWalker ${VERSION}

Contents:
- ${ZIP_EXE_NAME}
- README.txt
- LICENSE.txt
- THIRD_PARTY_NOTICES.txt

Run:
- PowerShell: .\\${ZIP_EXE_NAME}
- CMD: ${ZIP_EXE_NAME}

English:
- Type in the search box to narrow files and folders.
- Press Enter to open or execute. Press Shift+Enter to open the containing folder. Press Tab to pin rows. Press Ctrl+Shift+C to copy selected paths.
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
- Each token is treated like a search exclusion, so old and ~ behave like !old !~.
- The Ignore List checkbox controls whether these rules apply. It is on by default.

Runtime config:
- Runtime settings are stored in ~/.flistwalker_config.json in your home directory.
- On first launch, if the file is missing, FlistWalker creates it from the current FLISTWALKER_* environment values.
- Once the file exists, it becomes the source of truth for runtime settings and the matching environment variables are only an initial seed.
- The file covers search parallelism, walker limits, window trace settings, query history persistence, tab restore, and update policy.

Keyboard shortcuts:
- Up/Down or Ctrl+P/Ctrl+N: move the current row
- Ctrl+V / Alt+V: page down / page up
- Enter or Ctrl+J/Ctrl+M: open or execute selected item(s)
- Shift+Enter: open the containing folder(s) for selected item(s), deduplicated by directory
- Tab / Shift+Tab: toggle pin on the current row
- Ctrl+Shift+C: copy selected path(s)
- Ctrl+R: fuzzy-search query history and load the selected entry
- Esc / Ctrl+G: clear query and pinned selection, or cancel history search
- Ctrl+L: focus the query input

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
- Windows (PowerShell, persist per-user): [Environment]::SetEnvironmentVariable("FLISTWALKER_RESTORE_TABS", "1", "User")
- Windows (PowerShell, current session): \$env:FLISTWALKER_RESTORE_TABS = "1"
- Windows (CMD, current session): set FLISTWALKER_RESTORE_TABS=1
- macOS (zsh/bash, current session): export FLISTWALKER_RESTORE_TABS=1
- macOS (zsh persist): echo 'export FLISTWALKER_RESTORE_TABS=1' >> ~/.zshrc
- Linux (bash, current session): export FLISTWALKER_RESTORE_TABS=1
- Linux (bash persist): echo 'export FLISTWALKER_RESTORE_TABS=1' >> ~/.bashrc

Index options:
- Use FileList: prefer FileList.txt / filelist.txt at the repository root
- Files / Folders: toggle visibility without reindexing
- Refresh Index: reindex the current root

Walker tuning (Environment variables):
- FLISTWALKER_WALKER_MAX_ENTRIES: walker maximum entry count (default: 500000)
- FLISTWALKER_WALKER_THREADS: walker parallel thread count (default: 2, 1 means serial)

日本語:
- 起動後に検索窓へ文字を入力すると、ファイル/フォルダを絞り込みます。
- Enter で開く/実行、Shift+Enter で格納フォルダを開く（同一フォルダは1回のみ）、Tab でピン留め複数選択、Ctrl+Shift+C でパスコピー。
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
- Ignore List チェックボックスで適用の ON/OFF を切り替えます。既定は ON です。

Runtime config:
- runtime settings は home directory の ~/.flistwalker_config.json に保存されます。
- 初回起動でファイルが無い場合は、現在の FLISTWALKER_* 環境変数を seed にして自動生成します。
- 一度ファイルができたら、その内容が runtime settings の source of truth になり、同名 env は初期 seed としてのみ使われます。
- このファイルには search parallelism、walker limit、window trace、query history persistence、tab restore、update policy が入ります。
- 検索履歴は全タブ共通で最大100件まで保持され、短い待機後または結果移動開始時に確定します。
- FLISTWALKER_RESTORE_TABS=1 を設定すると、終了時のタブ状態を次回起動時に復元できます。
- Use FileList はルート直下の FileList.txt / filelist.txt を優先使用します。
- Refresh Index は現在Rootで再インデックスします。
README
cp -f "${ROOT_LICENSE}" "${OUT_DIR}/${LICENSE_SIDE_NAME}"
cp -f "${ROOT_NOTICES}" "${OUT_DIR}/${NOTICES_SIDE_NAME}"
cp -f "${OUT_DIR}/${README_SIDE_NAME}" "${WORK_DIR}/README.txt"
cp -f "${ROOT_LICENSE}" "${WORK_DIR}/LICENSE.txt"
cp -f "${ROOT_NOTICES}" "${WORK_DIR}/THIRD_PARTY_NOTICES.txt"

(
  cd "${WORK_DIR}"
  zip -q -9 "${OUT_DIR}/${ZIP_NAME}" "${ZIP_EXE_NAME}" README.txt LICENSE.txt THIRD_PARTY_NOTICES.txt
)

if command -v sha256sum >/dev/null 2>&1; then
  (
    cd "${OUT_DIR}"
    sha256sum "${EXE_NAME}" "${ZIP_NAME}" "${README_SIDE_NAME}" "${LICENSE_SIDE_NAME}" "${NOTICES_SIDE_NAME}" > SHA256SUMS
  )
elif command -v shasum >/dev/null 2>&1; then
  (
    cd "${OUT_DIR}"
    shasum -a 256 "${EXE_NAME}" "${ZIP_NAME}" "${README_SIDE_NAME}" "${LICENSE_SIDE_NAME}" "${NOTICES_SIDE_NAME}" > SHA256SUMS
  )
else
  echo "sha256sum/shasum が見つかりません。SHA256SUMS を生成できませんでした。" >&2
  exit 1
fi

if [[ -n "${FLISTWALKER_UPDATE_SIGNING_KEY_HEX:-}" ]]; then
  cargo run --manifest-path "${REPO_DIR}/rust/Cargo.toml" --quiet --bin sign_update_manifest -- \
    "${OUT_DIR}/SHA256SUMS" "${OUT_DIR}/SHA256SUMS.sig"
fi

echo "Release assets created: ${OUT_DIR}"
echo "- ${EXE_NAME}"
echo "- ${ZIP_NAME}"
echo "- ${README_SIDE_NAME}"
echo "- ${LICENSE_SIDE_NAME}"
echo "- ${NOTICES_SIDE_NAME}"
echo "- SHA256SUMS"
if [[ -n "${FLISTWALKER_UPDATE_SIGNING_KEY_HEX:-}" ]]; then
  echo "- SHA256SUMS.sig"
fi

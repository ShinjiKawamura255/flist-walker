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

if [[ ! -f "${SOURCE_EXE}" ]]; then
  echo "EXE が見つかりません: ${SOURCE_EXE}" >&2
  echo "先に scripts/build-rust-win.sh を実行してください。" >&2
  exit 1
fi

mkdir -p "${OUT_DIR}"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "${WORK_DIR}"' EXIT

cp -f "${SOURCE_EXE}" "${OUT_DIR}/${EXE_NAME}"
cp -f "${SOURCE_EXE}" "${WORK_DIR}/${EXE_NAME}"
cat > "${WORK_DIR}/README.txt" <<README
FlistWalker ${VERSION}

Contents:
- ${EXE_NAME}

Run:
- Double-click on Windows
- or execute from PowerShell/CMD
README

(
  cd "${WORK_DIR}"
  zip -q -9 "${OUT_DIR}/${ZIP_NAME}" "${EXE_NAME}" README.txt
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

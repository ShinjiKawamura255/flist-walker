#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
RUST_DIR="${REPO_DIR}/rust"
source "${SCRIPT_DIR}/common-win-gnu.sh"
TARGET="$(flistwalker_windows_target)"
BUILT_EXE_PATH="${RUST_DIR}/target/${TARGET}/release/flistwalker.exe"
EXE_PATH="${RUST_DIR}/target/${TARGET}/release/FlistWalker.exe"

if [[ ! -d "${RUST_DIR}" ]]; then
  echo "rust directory not found: ${RUST_DIR}" >&2
  exit 1
fi

if ! command -v rustup >/dev/null 2>&1 || ! command -v cargo >/dev/null 2>&1; then
  echo "rustup/cargo が見つかりません。WSL/Linux 側 Rust toolchain をセットアップしてください。" >&2
  exit 1
fi

if ! flistwalker_require_windows_gnu_tools; then
  flistwalker_print_windows_gnu_help
  exit 1
fi

echo "==> Ensure target: ${TARGET}"
rustup target add "${TARGET}"

if [[ -f "${EXE_PATH}" ]]; then
  rm -f "${EXE_PATH}"
fi
if [[ -f "${BUILT_EXE_PATH}" && "${BUILT_EXE_PATH}" != "${EXE_PATH}" ]]; then
  rm -f "${BUILT_EXE_PATH}"
fi

echo "==> Build (release): ${TARGET}"
(
  cd "${RUST_DIR}"
  cargo build --release --target "${TARGET}" "$@"
)

if [[ -f "${BUILT_EXE_PATH}" && ! -f "${EXE_PATH}" ]]; then
  cp -f "${BUILT_EXE_PATH}" "${EXE_PATH}"
fi

if [[ ! -f "${EXE_PATH}" ]]; then
  echo "Build finished but artifact not found: ${EXE_PATH}" >&2
  exit 1
fi

echo "==> Built: ${EXE_PATH}"

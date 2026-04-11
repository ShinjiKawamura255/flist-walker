#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
RUST_DIR="${REPO_DIR}/rust"
source "${SCRIPT_DIR}/common-win-gnu.sh"
TARGET="$(flistwalker_windows_target)"

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

echo "==> Clean: rust target directory"
(
  cd "${RUST_DIR}"
  cargo clean
)

echo "==> Build (release): ${TARGET}"
"${SCRIPT_DIR}/build-rust-win.sh" "$@"

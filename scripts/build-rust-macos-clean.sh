#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  scripts/build-rust-macos-clean.sh [target]

Examples:
  scripts/build-rust-macos-clean.sh
  scripts/build-rust-macos-clean.sh x86_64-apple-darwin
USAGE
}

if [[ ${1:-} == "-h" || ${1:-} == "--help" ]]; then
  usage
  exit 0
fi

TARGET="${1:-}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
RUST_DIR="${REPO_DIR}/rust"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo が見つかりません。Rust toolchain をインストールしてください。" >&2
  exit 1
fi

echo "==> Clean previous build artifacts"
(
  cd "${RUST_DIR}"
  if [[ -n "${TARGET}" ]]; then
    cargo clean --target "${TARGET}"
  else
    cargo clean
  fi
)

echo "==> Build macOS binary (clean)"
"${SCRIPT_DIR}/build-rust-macos.sh" "${TARGET}"

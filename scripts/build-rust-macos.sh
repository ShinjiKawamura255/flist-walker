#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  scripts/build-rust-macos.sh [target]

Examples:
  scripts/build-rust-macos.sh
  scripts/build-rust-macos.sh aarch64-apple-darwin
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

BUILD_ARGS=(build --release)
if [[ -n "${TARGET}" ]]; then
  BUILD_ARGS+=(--target "${TARGET}")
fi

echo "==> Build macOS binary"
(
  cd "${RUST_DIR}"
  cargo "${BUILD_ARGS[@]}"
)

if [[ -n "${TARGET}" ]]; then
  BIN_PATH="${RUST_DIR}/target/${TARGET}/release/flistwalker"
else
  BIN_PATH="${RUST_DIR}/target/release/flistwalker"
fi

echo "Built: ${BIN_PATH}"

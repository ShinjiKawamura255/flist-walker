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

if [[ -f "${HOME}/.cargo/env" ]]; then
  # GitHub-hosted macOS runners can install Rust without carrying Cargo's bin dir
  # into later shell steps, so make this script self-sufficient before probing.
  # shellcheck source=/dev/null
  source "${HOME}/.cargo/env"
fi
export PATH="${HOME}/.cargo/bin:${PATH}"

if command -v rustup >/dev/null 2>&1; then
  CARGO_CMD=(rustup run stable cargo)
elif command -v cargo >/dev/null 2>&1; then
  CARGO_CMD=(cargo)
else
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
  "${CARGO_CMD[@]}" "${BUILD_ARGS[@]}"
)

if [[ -n "${TARGET}" ]]; then
  BIN_PATH="${RUST_DIR}/target/${TARGET}/release/flistwalker"
else
  BIN_PATH="${RUST_DIR}/target/release/flistwalker"
fi

echo "Built: ${BIN_PATH}"

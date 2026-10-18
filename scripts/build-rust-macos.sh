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

TOOLCHAIN="$(sed -nE 's/^[[:space:]]*channel[[:space:]]*=[[:space:]]*"([0-9]+\.[0-9]+\.[0-9]+)"[[:space:]]*$/\1/p' "${RUST_DIR}/rust-toolchain.toml")"
if [[ ! "${TOOLCHAIN}" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "rust/rust-toolchain.toml に固定Rust版 X.Y.Z が必要です。" >&2
  exit 1
fi
if ! command -v rustup >/dev/null 2>&1; then
  echo "rustup が見つかりません。固定Rust版 ${TOOLCHAIN} をインストールしてください。" >&2
  exit 1
fi
# An explicit selector also prevents an inherited RUSTUP_TOOLCHAIN=stable from
# overriding the repository pin. A missing pinned toolchain must fail closed.
CARGO_CMD=(rustup run "${TOOLCHAIN}" cargo)

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
  FW_BIN_PATH="${RUST_DIR}/target/${TARGET}/release/fw"
else
  BIN_PATH="${RUST_DIR}/target/release/flistwalker"
  FW_BIN_PATH="${RUST_DIR}/target/release/fw"
fi

echo "Built: ${BIN_PATH}"
echo "Built: ${FW_BIN_PATH}"

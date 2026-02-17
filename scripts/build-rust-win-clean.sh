#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
RUST_DIR="${REPO_DIR}/rust"
TARGET="x86_64-pc-windows-msvc"
BUILD_ARTIFACT="${RUST_DIR}/target/${TARGET}/release/flistwalker.exe"
ARTIFACT="${RUST_DIR}/target/${TARGET}/release/FlistWalker.exe"

if [[ ! -d "${RUST_DIR}" ]]; then
  echo "rust/ ディレクトリが見つかりません: ${RUST_DIR}" >&2
  exit 1
fi

if [[ -f "${HOME}/.cargo/env" ]]; then
  # shellcheck disable=SC1090
  source "${HOME}/.cargo/env"
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo が見つかりません。rustup のセットアップを確認してください。" >&2
  exit 1
fi

if ! cargo xwin --version >/dev/null 2>&1; then
  echo "cargo-xwin が見つかりません。'cargo install cargo-xwin' を先に実行してください。" >&2
  exit 1
fi

echo "==> Ensure target: ${TARGET}"
rustup target add "${TARGET}" >/dev/null

echo "==> Clean: rust target directory"
cd "${RUST_DIR}"
cargo clean

echo "==> Build (release): ${TARGET}"
cargo xwin build --release --target "${TARGET}"

if [[ -f "${BUILD_ARTIFACT}" ]]; then
  cp -f "${BUILD_ARTIFACT}" "${ARTIFACT}"
fi

if [[ -f "${ARTIFACT}" ]]; then
  echo "==> Built: ${ARTIFACT}"
else
  echo "ビルドは完了しましたが成果物が見つかりません: ${ARTIFACT}" >&2
  exit 1
fi

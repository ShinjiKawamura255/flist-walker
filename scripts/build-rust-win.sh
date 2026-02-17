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

if [[ -f "${ARTIFACT}" ]]; then
  if ! rm -f "${ARTIFACT}" 2>/dev/null; then
    echo "既存の EXE を削除できませんでした。Windows 側で実行中の可能性があります。" >&2
    echo "対象: ${ARTIFACT}" >&2
    echo "アプリを終了してから再実行してください。" >&2
    exit 1
  fi
fi
if [[ -f "${BUILD_ARTIFACT}" ]]; then
  if ! rm -f "${BUILD_ARTIFACT}" 2>/dev/null; then
    echo "既存の EXE を削除できませんでした。Windows 側で実行中の可能性があります。" >&2
    echo "対象: ${BUILD_ARTIFACT}" >&2
    echo "アプリを終了してから再実行してください。" >&2
    exit 1
  fi
fi

echo "==> Build (release): ${TARGET}"
cd "${RUST_DIR}"
cargo xwin build --release --target "${TARGET}"

if [[ -f "${BUILD_ARTIFACT}" ]]; then
  if [[ "${BUILD_ARTIFACT,,}" != "${ARTIFACT,,}" ]]; then
    cp -f "${BUILD_ARTIFACT}" "${ARTIFACT}"
  fi
fi

if [[ -f "${ARTIFACT}" ]]; then
  echo "==> Built: ${ARTIFACT}"
else
  echo "ビルドは完了しましたが成果物が見つかりません: ${ARTIFACT}" >&2
  exit 1
fi

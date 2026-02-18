#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
PS_SCRIPT_WIN="$(wslpath -w "${SCRIPT_DIR}/build-rust-win.ps1")"

if [[ ! -f "${SCRIPT_DIR}/build-rust-win.ps1" ]]; then
  echo "PowerShell スクリプトが見つかりません: ${SCRIPT_DIR}/build-rust-win.ps1" >&2
  exit 1
fi

if ! command -v powershell.exe >/dev/null 2>&1; then
  echo "powershell.exe が見つかりません。WSL から Windows 連携が有効か確認してください。" >&2
  exit 1
fi

echo "==> Build via Windows PowerShell (for Explorer icon embedding)"
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "${PS_SCRIPT_WIN}"

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

Write-Error @"
scripts/build-rust-win-clean.ps1 は退役しました。

Windows 向けクリーンビルドは WSL/Linux 側の mingw-w64 + x86_64-pc-windows-gnu へ移行しています。
次を実行してください:

  ./scripts/build-rust-win-clean.sh
"@
exit 1

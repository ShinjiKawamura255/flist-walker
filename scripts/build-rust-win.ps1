[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

Write-Error @"
scripts/build-rust-win.ps1 は退役しました。

Windows 向けビルドは WSL/Linux 側の mingw-w64 + x86_64-pc-windows-gnu へ移行しています。
次を実行してください:

  ./scripts/build-rust-win.sh

必要なツール:
  - x86_64-w64-mingw32-gcc
  - x86_64-w64-mingw32-g++
  - x86_64-w64-mingw32-ar
  - x86_64-w64-mingw32-ranlib
  - x86_64-w64-mingw32-windres
"@
exit 1

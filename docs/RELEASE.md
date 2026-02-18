# RELEASE

## 初期リリース方針
- 配布形態は `exe単体 + zip` を採用する。
- `msi` は需要確認後の第2段階で追加する。

## アセット命名規則
- Windows x86_64:
- `FlistWalker-<version>-windows-x86_64.exe`
- `FlistWalker-<version>-windows-x86_64.zip`
- `SHA256SUMS`

例（v0.1.1）:
- `FlistWalker-0.1.1-windows-x86_64.exe`
- `FlistWalker-0.1.1-windows-x86_64.zip`
- `SHA256SUMS`

## zip に含めるもの
- `FlistWalker-<version>-windows-x86_64.exe`
- `README.txt`（最小実行手順）

## リリース手順（Windows アセット）
1. Windows 向け EXE をビルドする。
- WSL/Linux: `./scripts/build-rust-win.sh`（内部で PowerShell を呼び出し、Windows 側 Rust でビルド）
- PowerShell: `powershell -ExecutionPolicy Bypass -File .\scripts\build-rust-win.ps1`

2. リリースアセットを生成する。
- WSL/Linux: `./scripts/prepare-release.sh v0.1.1`
- PowerShell: `powershell -ExecutionPolicy Bypass -File .\scripts\prepare-release.ps1 -Version v0.1.1`

3. `dist/v0.1.1/`（例）内の3ファイルを GitHub Releases にアップロードする。
- `*.exe`
- `*.zip`
- `SHA256SUMS`

## SHA256SUMS 検証例
PowerShell:
```powershell
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.1.1-windows-x86_64.exe
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.1.1-windows-x86_64.zip
```

bash:
```bash
sha256sum -c SHA256SUMS
```

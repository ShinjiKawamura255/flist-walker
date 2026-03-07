# RELEASE

## 初期リリース方針
- Windows 配布形態は `exe単体 + zip` を採用する。
- macOS 配布形態は `バイナリ単体 + .app + zip/tar.gz` を採用する。
- `msi` は需要確認後の第2段階で追加する。

## アセット命名規則
- Linux x86_64:
- `FlistWalker-<version>-linux-x86_64`
- `FlistWalker-<version>-linux-x86_64.tar.gz`
- Windows x86_64:
- `FlistWalker-<version>-windows-x86_64.exe`
- `FlistWalker-<version>-windows-x86_64.zip`
- macOS arm64:
- `FlistWalker-<version>-macos-arm64`
- `FlistWalker-<version>-macos-arm64.app`
- `FlistWalker-<version>-macos-arm64-app.zip`
- `FlistWalker-<version>-macos-arm64.tar.gz`
- macOS x86_64:
- `FlistWalker-<version>-macos-x86_64`
- `FlistWalker-<version>-macos-x86_64.app`
- `FlistWalker-<version>-macos-x86_64-app.zip`
- `FlistWalker-<version>-macos-x86_64.tar.gz`
- `SHA256SUMS`

例（v0.2.0）:
- `FlistWalker-0.2.0-linux-x86_64`
- `FlistWalker-0.2.0-linux-x86_64.tar.gz`
- `FlistWalker-0.2.0-windows-x86_64.exe`
- `FlistWalker-0.2.0-windows-x86_64.zip`
- `FlistWalker-0.2.0-macos-arm64`
- `FlistWalker-0.2.0-macos-arm64.app`
- `FlistWalker-0.2.0-macos-arm64-app.zip`
- `FlistWalker-0.2.0-macos-arm64.tar.gz`
- `SHA256SUMS`

## zip に含めるもの
- `flistwalker.exe`
- `README.txt`（最小実行手順）

## tar.gz に含めるもの
- `flistwalker`
- `README.txt`（最小実行手順）

## リリース手順（Linux アセット）
1. Linux 向けバイナリをビルドする。
- bash: `cd rust && cargo build --release --locked`

2. リリースアセットを生成する。
- bash: `./scripts/prepare-release-linux.sh v0.2.0`

3. `dist/v0.2.0/`（例）内のファイルを GitHub Releases にアップロードする。
- `FlistWalker-*-linux-*`
- `FlistWalker-*-linux-*.tar.gz`
- `SHA256SUMS`

## リリース手順（Windows アセット）
1. Windows 向け EXE をビルドする。
- WSL/Linux: `./scripts/build-rust-win.sh`（内部で PowerShell を呼び出し、Windows 側 Rust でビルド）
- PowerShell: `powershell -ExecutionPolicy Bypass -File .\scripts\build-rust-win.ps1`

2. リリースアセットを生成する。
- WSL/Linux: `./scripts/prepare-release.sh v0.2.0`
- PowerShell: `powershell -ExecutionPolicy Bypass -File .\scripts\prepare-release.ps1 -Version v0.2.0`

3. `dist/v0.2.0/`（例）内の3ファイルを GitHub Releases にアップロードする。
- `*.exe`
- `*.zip`
- `SHA256SUMS`

## リリース手順（macOS アセット）
1. macOS 向けバイナリをビルドする。
- bash: `./scripts/build-rust-macos.sh`
- クリーンビルド: `./scripts/build-rust-macos-clean.sh`

2. リリースアセットを生成する。
- bash: `./scripts/prepare-release-macos.sh v0.2.0`

3. （推奨）Developer ID 署名 + notarization + staple を実施する。
- 初回のみ: `xcrun notarytool store-credentials flistwalker-notary --apple-id "<APPLE_ID>" --team-id "<TEAM_ID>" --password "<APP_SPECIFIC_PASSWORD>"`
- 署名ID設定: `export FLISTWALKER_MACOS_SIGN_IDENTITY="Developer ID Application: Example Corp (TEAMID1234)"`
- 実行: `./scripts/sign-notarize-macos.sh v0.2.0 arm64 flistwalker-notary`

4. `dist/v0.2.0/`（例）内のファイルを GitHub Releases にアップロードする。
- `FlistWalker-*-macos-*`（実行バイナリ）
- `FlistWalker-*-macos-*-app.zip`（`.app` 配布用）
- `FlistWalker-*-macos-*.tar.gz`
- `SHA256SUMS`

## GitHub Actions 自動リリース
1. `vX.Y.Z` 形式の新規 tag を push する。
2. GitHub Actions の `Release Tagged Build` workflow が Linux / Windows / macOS（x86_64, arm64）向け release build を実行する。
3. 各 job が生成した uploadable なアセットを集約し、その tag の draft release を自動作成する。
4. draft release には各 OS 向け実行バイナリ、配布 archive、統合 `SHA256SUMS` が添付される。

## SHA256SUMS 検証例
PowerShell:
```powershell
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.2.0-windows-x86_64.exe
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.2.0-windows-x86_64.zip
```

bash:
```bash
sha256sum -c SHA256SUMS
```

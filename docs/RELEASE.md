# RELEASE

## 初期リリース方針
- Windows 配布形態は `exe単体 + zip` を採用する。
- macOS 配布形態は `バイナリ単体 + .app + zip/tar.gz` を採用する。
- GitHub Releases へ添付する macOS アセットは `バイナリ単体 + app.zip + tar.gz` とし、`.app` bundle 自体は notarization 用に `dist/` にのみ残す。
- `msi` は需要確認後の第2段階で追加する。

## アセット命名規則
- Linux x86_64:
- `FlistWalker-<version>-linux-x86_64`
- `FlistWalker-<version>-linux-x86_64.tar.gz`
- `FlistWalker-<version>-linux-x86_64.LICENSE.txt`
- `FlistWalker-<version>-linux-x86_64.THIRD_PARTY_NOTICES.txt`
- Windows x86_64:
- `FlistWalker-<version>-windows-x86_64.exe`
- `FlistWalker-<version>-windows-x86_64.zip`
- `FlistWalker-<version>-windows-x86_64.LICENSE.txt`
- `FlistWalker-<version>-windows-x86_64.THIRD_PARTY_NOTICES.txt`
- macOS arm64:
- `FlistWalker-<version>-macos-arm64`
- `FlistWalker-<version>-macos-arm64-app.zip`
- `FlistWalker-<version>-macos-arm64.tar.gz`
- `FlistWalker-<version>-macos-arm64.LICENSE.txt`
- `FlistWalker-<version>-macos-arm64.THIRD_PARTY_NOTICES.txt`
- macOS x86_64:
- `FlistWalker-<version>-macos-x86_64`
- `FlistWalker-<version>-macos-x86_64-app.zip`
- `FlistWalker-<version>-macos-x86_64.tar.gz`
- `FlistWalker-<version>-macos-x86_64.LICENSE.txt`
- `FlistWalker-<version>-macos-x86_64.THIRD_PARTY_NOTICES.txt`
- `SHA256SUMS`

例（v0.2.0）:
- `FlistWalker-0.2.0-linux-x86_64`
- `FlistWalker-0.2.0-linux-x86_64.tar.gz`
- `FlistWalker-0.2.0-linux-x86_64.LICENSE.txt`
- `FlistWalker-0.2.0-linux-x86_64.THIRD_PARTY_NOTICES.txt`
- `FlistWalker-0.2.0-windows-x86_64.exe`
- `FlistWalker-0.2.0-windows-x86_64.zip`
- `FlistWalker-0.2.0-windows-x86_64.LICENSE.txt`
- `FlistWalker-0.2.0-windows-x86_64.THIRD_PARTY_NOTICES.txt`
- `FlistWalker-0.2.0-macos-arm64`
- `FlistWalker-0.2.0-macos-arm64-app.zip`
- `FlistWalker-0.2.0-macos-arm64.tar.gz`
- `FlistWalker-0.2.0-macos-arm64.LICENSE.txt`
- `FlistWalker-0.2.0-macos-arm64.THIRD_PARTY_NOTICES.txt`
- `SHA256SUMS`

## zip に含めるもの
- `flistwalker.exe`
- `README.txt`（最小実行手順）
- `LICENSE.txt`
- `THIRD_PARTY_NOTICES.txt`

## tar.gz に含めるもの
- `flistwalker`
- `README.txt`（最小実行手順）
- `LICENSE.txt`
- `THIRD_PARTY_NOTICES.txt`

## リリース手順（Linux アセット）
1. Linux 向けバイナリをビルドする。
- bash: `cd rust && cargo build --release --locked`

2. リリースアセットを生成する。
- bash: `./scripts/prepare-release-linux.sh v0.2.0`

3. `dist/v0.2.0/`（例）内のファイルを GitHub Releases にアップロードする。
- `FlistWalker-*-linux-*`
- `FlistWalker-*-linux-*.tar.gz`
- `FlistWalker-*-linux-*.LICENSE.txt`
- `FlistWalker-*-linux-*.THIRD_PARTY_NOTICES.txt`
- `SHA256SUMS`

## リリース手順（Windows アセット）
1. Windows 向け EXE をビルドする。
- WSL/Linux: `./scripts/build-rust-win.sh`
- `x86_64-pc-windows-gnu` + mingw-w64 を利用し、PowerShell や Windows 側 Rust は使用しない
- ビルド後は `flistwalker.exe` の `.rsrc` にアイコンリソースが含まれることを前提とし、Windows Explorer で埋め込みアイコンが表示されなければリリース不可
- 必要ツール:
  - `x86_64-w64-mingw32-gcc`
  - `x86_64-w64-mingw32-g++`
  - `x86_64-w64-mingw32-ar`
  - `x86_64-w64-mingw32-ranlib`
  - `x86_64-w64-mingw32-windres`
  - `x86_64-w64-mingw32-strip`
- Ubuntu / Debian 系: `sudo apt install -y gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64 binutils-mingw-w64-x86-64`
- release profile は `lto = "thin"`, `codegen-units = 1`, `panic = "abort"`, `strip = "symbols"` を使い、ビルド後に `x86_64-w64-mingw32-strip` で最終 EXE を追加圧縮する

2. リリースアセットを生成する。
- WSL/Linux: `./scripts/prepare-release.sh v0.2.0`
- PowerShell: `powershell -ExecutionPolicy Bypass -File .\scripts\prepare-release.ps1 -Version v0.2.0`

3. `dist/v0.2.0/`（例）内のファイルを GitHub Releases にアップロードする。
- `*.exe`
- `*.zip`
- `*.LICENSE.txt`
- `*.THIRD_PARTY_NOTICES.txt`
- `SHA256SUMS`

## Regression Guard
- 発生条件: Windows ビルドを `msvc` から `x86_64-pc-windows-gnu` へ切り替えた際、`windres` 生成物が最終 EXE に入らず Explorer 上のアイコンが既定表示へ退行する。
- 期待動作: WSL/Linux からの GNU ビルド後も `flistwalker.exe` / 配布用 `FlistWalker.exe` の両方で Explorer アイコンが維持される。
- 非対象範囲: 実行中ウィンドウのランタイムアイコン変更、ショートカット `.lnk` 側の個別アイコン設定。
- 関連テストID: TC-065.

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

4. `dist/v0.2.0/`（例）内の uploadable なファイルだけを GitHub Releases にアップロードする。
- `FlistWalker-*-macos-*`（実行バイナリ）
- `FlistWalker-*-macos-*-app.zip`（`.app` 配布用）
- `FlistWalker-*-macos-*.tar.gz`
- `FlistWalker-*-macos-*.LICENSE.txt`
- `FlistWalker-*-macos-*.THIRD_PARTY_NOTICES.txt`
- `SHA256SUMS`
- `.app` bundle 自体は notarization / staple 用に `dist/` へ保持するが、GitHub Releases には添付しない。

## GitHub Actions 自動リリース
1. `vX.Y.Z` 形式の新規 tag を push する。
2. GitHub Actions の `Release Tagged Build` workflow が Linux / Windows / macOS（x86_64, arm64）向け release build を実行する。
3. 各 job が生成した uploadable なアセットを集約し、その tag の draft release を自動作成する。
4. draft release には各 OS 向け実行バイナリ、配布 archive、sidecar notice (`*.LICENSE.txt`, `*.THIRD_PARTY_NOTICES.txt`)、統合 `SHA256SUMS` が添付される。macOS の `.app` bundle 自体およびその内部ファイル（`Info.plist` / `FlistWalker.icns` / `Contents/MacOS/FlistWalker` など）は添付対象外とする。
5. draft release の作成を確認したら、macOS 向け配布物の notarization 状態を別工程で確認する。現段階では workflow で notarization を強制していないため、確認前に publish してはならない。
6. notarization 確認後、Codex で GitHub Release 本文を最終化し、draft を本リリースへ publish する。

## Release 前チェック
- `rust/Cargo.toml` の `[package].version` が対象 release の `X.Y.Z` と一致していること。
- `rust/Cargo.lock` の `flist-walker` package version が同じ `X.Y.Z` へ更新済みであること。
- `CHANGELOG.md` の対象 version 節、git tag `vX.Y.Z`、release note の対象 version が一致していること。
- Codex で release 前チェックを行うときは `skills/flistwalker-release-preflight/SKILL.md` を使う。
- CI の Linux / macOS / Windows test と `cargo audit` が green であること。
- macOS を publish 対象に含める場合、`scripts/sign-notarize-macos.sh` などで notarization 完了を確認済みであること。

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

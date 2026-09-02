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
- `FlistWalker-<version>-linux-x86_64.README.txt`
- `FlistWalker-<version>-linux-x86_64.LICENSE.txt`
- `FlistWalker-<version>-linux-x86_64.THIRD_PARTY_NOTICES.txt`
- `fw-<version>-linux-x86_64`（CLI 専用 standalone）
- Windows x86_64:
- `FlistWalker-<version>-windows-x86_64.exe`
- `FlistWalker-<version>-windows-x86_64.zip`
- `FlistWalker-<version>-windows-x86_64.README.txt`
- `FlistWalker-<version>-windows-x86_64.LICENSE.txt`
- `FlistWalker-<version>-windows-x86_64.THIRD_PARTY_NOTICES.txt`
- `fw-<version>-windows-x86_64.exe`（CLI 専用 standalone）
- macOS arm64:
- `FlistWalker-<version>-macos-arm64`
- `FlistWalker-<version>-macos-arm64-app.zip`
- `FlistWalker-<version>-macos-arm64.tar.gz`
- `FlistWalker-<version>-macos-arm64.README.txt`
- `FlistWalker-<version>-macos-arm64.LICENSE.txt`
- `FlistWalker-<version>-macos-arm64.THIRD_PARTY_NOTICES.txt`
- `fw-<version>-macos-arm64`（CLI 専用 standalone）
- macOS x86_64:
- `FlistWalker-<version>-macos-x86_64`
- `FlistWalker-<version>-macos-x86_64-app.zip`
- `FlistWalker-<version>-macos-x86_64.tar.gz`
- `FlistWalker-<version>-macos-x86_64.README.txt`
- `FlistWalker-<version>-macos-x86_64.LICENSE.txt`
- `FlistWalker-<version>-macos-x86_64.THIRD_PARTY_NOTICES.txt`
- `fw-<version>-macos-x86_64`（CLI 専用 standalone）
- `SHA256SUMS`
- `SHA256SUMS.sig`

例（v0.2.0）:
- `FlistWalker-0.2.0-linux-x86_64`
- `FlistWalker-0.2.0-linux-x86_64.tar.gz`
- `FlistWalker-0.2.0-linux-x86_64.README.txt`
- `FlistWalker-0.2.0-linux-x86_64.LICENSE.txt`
- `FlistWalker-0.2.0-linux-x86_64.THIRD_PARTY_NOTICES.txt`
- `fw-0.2.0-linux-x86_64`
- `FlistWalker-0.2.0-windows-x86_64.exe`
- `FlistWalker-0.2.0-windows-x86_64.zip`
- `FlistWalker-0.2.0-windows-x86_64.README.txt`
- `FlistWalker-0.2.0-windows-x86_64.LICENSE.txt`
- `FlistWalker-0.2.0-windows-x86_64.THIRD_PARTY_NOTICES.txt`
- `fw-0.2.0-windows-x86_64.exe`
- `FlistWalker-0.2.0-macos-arm64`
- `FlistWalker-0.2.0-macos-arm64-app.zip`
- `FlistWalker-0.2.0-macos-arm64.tar.gz`
- `FlistWalker-0.2.0-macos-arm64.README.txt`
- `FlistWalker-0.2.0-macos-arm64.LICENSE.txt`
- `FlistWalker-0.2.0-macos-arm64.THIRD_PARTY_NOTICES.txt`
- `fw-0.2.0-macos-arm64`
- `FlistWalker-0.2.0-macos-x86_64`
- `FlistWalker-0.2.0-macos-x86_64-app.zip`
- `FlistWalker-0.2.0-macos-x86_64.tar.gz`
- `FlistWalker-0.2.0-macos-x86_64.README.txt`
- `FlistWalker-0.2.0-macos-x86_64.LICENSE.txt`
- `FlistWalker-0.2.0-macos-x86_64.THIRD_PARTY_NOTICES.txt`
- `fw-0.2.0-macos-x86_64`
- `SHA256SUMS`
- `SHA256SUMS.sig`

## zip に含めるもの
- `flistwalker.exe`
- `README.txt`（英語を先頭に、後半に日本語を置いた最小実行手順。ignore list と runtime settings files の場所・seed-only 挙動の説明を含める）
- `LICENSE.txt`
- `THIRD_PARTY_NOTICES.txt`
- `fw.exe` は既存 zip に追加せず、version 付き standalone asset として配布する。

## tar.gz に含めるもの
- `flistwalker`
- `README.txt`（英語を先頭に、後半に日本語を置いた最小実行手順。ignore list と runtime settings files の場所・seed-only 挙動の説明を含める）
- `LICENSE.txt`
- `THIRD_PARTY_NOTICES.txt`
- `fw` は既存 tar.gz に追加せず、version 付き standalone asset として配布する。

## リリース手順（Linux アセット）
1. Linux 向けバイナリをビルドする。
- bash: `cd rust && cargo build --release --locked`

2. リリースアセットを生成する。
- bash: `./scripts/prepare-release-linux.sh v0.2.0`

3. `dist/v0.2.0/`（例）内のファイルを GitHub Releases にアップロードする。
- `FlistWalker-*-linux-*`
- `FlistWalker-*-linux-*.tar.gz`
- `FlistWalker-*-linux-*.README.txt`
- `FlistWalker-*-linux-*.LICENSE.txt`
- `FlistWalker-*-linux-*.THIRD_PARTY_NOTICES.txt`
- `fw-*-linux-*`
- `SHA256SUMS`
- `SHA256SUMS.sig`（`FLISTWALKER_UPDATE_SIGNING_KEY_HEX` を設定して生成した場合）

## リリース手順（Windows アセット）
1. Windows 向け EXE をビルドする。
- PowerShell: `powershell -ExecutionPolicy Bypass -File .\scripts\build-rust-win.ps1 -NoInstall`
- WSL/Linux: `./scripts/build-rust-win.sh`
- どちらも `x86_64-pc-windows-gnu` + mingw-w64 を利用する。PowerShell 版は `-CheckOnly` / `-NoInstall` / `-InstallMissing` を提供し、通常モードでは不足依存を個別確認してから導入する
- ビルド後は `flistwalker.exe` と `fw.exe` の `.rsrc` にアイコン/manifest resource が含まれることを前提とし、欠落していればリリース不可
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
- 自動更新対象として配布する場合は、ビルド時に `FLISTWALKER_UPDATE_PUBLIC_KEY_HEX` を設定して公開鍵を埋め込み、パッケージ時に `FLISTWALKER_UPDATE_SIGNING_KEY_HEX` を設定して `SHA256SUMS.sig` を生成する

3. `dist/v0.2.0/`（例）内のファイルを GitHub Releases にアップロードする。
- `*.exe`
- `*.zip`
- `*.README.txt`
- `*.LICENSE.txt`
- `*.THIRD_PARTY_NOTICES.txt`
- `SHA256SUMS`
- `SHA256SUMS.sig`（`FLISTWALKER_UPDATE_SIGNING_KEY_HEX` を設定して生成した場合）

## Regression Guard
- 発生条件: Windows ビルドを `msvc` から `x86_64-pc-windows-gnu` へ切り替えた際、`windres` 生成物が最終 EXE に入らず Explorer 上のアイコンが既定表示へ退行する。
- 期待動作: WSL/Linux または PowerShell からの GNU ビルド後も `flistwalker.exe` / 配布用 `FlistWalker.exe` / `fw.exe` で resource と console subsystem が維持される。
- 非対象範囲: 実行中ウィンドウのランタイムアイコン変更、ショートカット `.lnk` 側の個別アイコン設定。
- 関連テストID: TC-090, TC-147, TC-148.

### Regression Guard: checksum manifest asset families
- 発生条件: `SHA256SUMS` に CLI standalone の `fw-*` asset が含まれる release を、`FlistWalker-*` だけを許可していた updater が検証する。
- 期待動作: `FlistWalker-*` と `fw-*` の両 asset family を strict parser が受け入れ、`fwx-*` などの近似 prefix や任意 basename は拒否する。
- 非対象範囲: checksum digest の計算、detached signature の検証、release asset の生成内容そのもの。
- 関連テストID: TC-194.
- 将来変更時の注意: CLI standalone asset family を追加・変更する場合は、release validator と updater parser の許可条件および回帰テストを同一変更で更新する。

### Regression Guard: public N-1 updater compatibility
- 発生条件: 公開済みv0.24.3のupdaterは`fw-*`行を含むchecksum manifest全体を拒否するため、v0.24.4へ自己更新できない。
- 期待動作: v0.24.3からは一度だけ手動でbinaryを置き換える。以後のreleaseはcandidate versionが直前の公開版よりstrictに新しいことと、直前の公開版contractがcandidateのexact 26-entry manifestを受理することを`check-updater-n-minus-one-compatibility.py`で検証し、例外・acknowledgementなしで両方を満たす場合だけpublishする。公開済みtagやassetは修正目的でも削除・上書きしない。
- 非対象範囲: v0.24.3 assetの差し替え、既存archiveへの`fw`混在、1transactionで両variantを置換する更新。
- 関連テストID: TC-194, TC-196.
- 将来変更時の注意: checker self-testと、生成したcandidate `SHA256SUMS`に対する直前公開版contractの実検証を別々に実行する。candidate側parserの修正を、更新前binaryの非互換を承認する理由にしてはならない。直前公開版はcheckerの明示的なshipped capability表に登録されていなければならず、version大小からparser能力を推測してはならない。
- 現行の shipped capability table には v0.24.3、v0.24.4、v0.24.5、v0.25.0 を明示登録し、各リリースの exact 26-entry candidate manifest を対応する predecessor contract で検証する。

#### v0.24.3 からの回復手順

1. FlistWalker / `fw` を終了する。
2. GitHub Releases から利用中variantと同じ最新binary（GUIは`FlistWalker-*-windows-x86_64.exe`、CLI専用は`fw-*-windows-x86_64.exe`）と`SHA256SUMS`をダウンロードする。
3. `SHA256SUMS`に記載された対象binaryのSHA-256とダウンロードしたfileのhashを照合する。
4. 既存binaryのbackupを取ってから手動で置き換えて起動し、表示versionを確認する。v0.24.4以降へ移行後は通常の自動更新を再利用できる。

この手順は既に配布済みのv0.24.3 parserをrepository側の修正で置き換えられないために必要であり、将来のreleaseで同様のbridgeを許可するものではない。

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
- `FlistWalker-*-macos-*.README.txt`
- `FlistWalker-*-macos-*.LICENSE.txt`
- `FlistWalker-*-macos-*.THIRD_PARTY_NOTICES.txt`
- `fw-*-macos-*`（CLI 専用 standalone）
- `SHA256SUMS`
- `SHA256SUMS.sig`（`FLISTWALKER_UPDATE_SIGNING_KEY_HEX` を設定して生成した場合）
- `.app` bundle 自体は notarization / staple 用に `dist/` へ保持するが、GitHub Releases には添付しない。

## GitHub Actions 自動リリース
1. `vX.Y.Z` 形式の新規 tag を push する。
2. `Release Tagged Build` workflow は最初に preflight として Linux / macOS / Windows native の `cargo test --locked` と `cargo clippy --locked --all-targets -- -D warnings`、および `cargo audit` を実行し、すべて成功した場合のみ release build へ進む。
3. preflight 成功後に Linux / Windows / macOS（x86_64, arm64）向け release build を実行する。
4. 各 job が生成した uploadable なアセットを集約し、その tag の draft release を自動作成する。同一tagのreleaseが既に存在する場合は停止し、既存assetを上書きしない。
5. draft release には各 OS 向け universal 実行バイナリ、CLI 専用 `fw` standalone、既存配布 archive、sidecar 文書 (`*.README.txt`, `*.LICENSE.txt`, `*.THIRD_PARTY_NOTICES.txt`)、統合 `SHA256SUMS` と `SHA256SUMS.sig` が添付される。`README.txt` は英語の案内を先頭に置き、その後に日本語の案内と`fw` standaloneの実行手順を続ける。remoteのversion付きsidecar assetは両variantで共有し、自己更新後のローカル文書はuniversalの通常名とCLIの`fw.` prefix付き名へ分離する。`SHA256SUMS` は artifact 集約後に再生成し、`SHA256SUMS.sig` は署名秘密鍵から導出した公開鍵、build時の公開鍵、署名検証鍵が一致する場合だけ生成する。`scripts/validate-release-bundle.sh` で期待28 asset、26 checksum entry、既存archiveのmember完全一致、archive/sidecarの `LICENSE.txt` / `THIRD_PARTY_NOTICES.txt` を検証する。macOS の `.app` bundle 自体およびその内部ファイル（`Info.plist` / `FlistWalker.icns` / `Contents/MacOS/FlistWalker` など）は添付対象外とする。
6. draft release の作成を確認したら、Codex で GitHub Release 本文を最終化する。
7. 当面の暫定運用として、macOS 向け配布物の notarization 確認は publish 前提条件にしない。notarization 環境が整うまでは、そのまま draft を本リリースへ publish してよい。
8. ただし publish 時は、GitHub Release 本文の `Security` または `Known issues` に macOS 配布物が未 notarized である旨を明記する。
9. 公開後に重大問題を検出した場合は `docs/RELEASE_INCIDENT_RUNBOOK.md` に従い、公開済みtag/assetを上書きせずに取得停止、警告、影響確認、patch releaseを行う。

## Release 前チェック
- `rust/Cargo.toml` の `[package].version` が対象 release の `X.Y.Z` と一致していること。
- `rust/Cargo.lock` の `flist-walker` package version が同じ `X.Y.Z` へ更新済みであること。
- `CHANGELOG.md` の対象 version 節、git tag `vX.Y.Z`、release note の対象 version が一致していること。
- `THIRD_PARTY_NOTICES.txt` が現在の `Cargo.toml` / `Cargo.lock` の direct dependency と license families を反映していること。自己更新系依存（例: `ed25519-dalek`, `rand_core`, `sha2`, `ureq`, `semver`）の追加・更新時は同一変更で見直すこと。
- 自動更新を有効にする配布ビルドでは、`FLISTWALKER_UPDATE_PUBLIC_KEY_HEX` が build 時に設定されていること。
- `SHA256SUMS.sig` を生成する release 作業では、`FLISTWALKER_UPDATE_SIGNING_KEY_HEX` が package / draft release 作成時に設定されていること。
- signing stepで公開鍵secretが64桁hexであり、署名秘密鍵から導出した公開鍵および配布buildへ埋め込む公開鍵と一致すること。
- `scripts/validate-release-bundle.sh vX.Y.Z <bundle-dir>` が成功し、期待28 asset、26 checksum entry、既存archive不変、archive/sidecarのlicense/noticeが揃うこと。
- checker self-testとは別に、直前の公開release versionと生成済みcandidate `SHA256SUMS`を`check-updater-n-minus-one-compatibility.py`へ渡し、candidateがstrictに新しくmanifest互換であること。非増加version、非互換の例外・acknowledgementは禁止し、失敗時はrelease blockerとする。
- Windows release build の固定 shallow 200-file fixture で TC-193（5 warmup + 25 sample、`fw` median / universal median ≤ 0.70、Shell32/User32を許容しGDI32/OpenGL32/imm32/psapi/dwmapi/uxthemeのGUI framework/rendering/window系importなし）が成功すること。
- 同一tagのreleaseが存在しないこと。既存release/assetは更新、削除、上書きしないこと。
- release candidate の Rust build / test / clippy / release asset build logs に warning が残っていないこと。warning が 1 件でもある場合は、原因を修正するか、release blocker ではない理由と follow-up を明記するまで publish しない。
- tag workflowのLinux/macOS/Windows native preflightでlocked clippyがすべて実行され、OS条件付きunused/dead code warningがasset build前に失敗すること。
- Codex で release 前チェックを行うときは `skills/flistwalker-release-preflight/SKILL.md` を使う。
- CI の Linux / macOS / Windows native test、Windows GNU cross build、`cargo audit` が green であること。
- notarization 環境が未整備な当面の間は、macOS を publish 対象に含める場合でも notarization 完了を必須条件にしない。その代わり release note に未 notarized である旨を記載すること。

## Build / Release 環境変数の扱い
- build/release secret:
  - `FLISTWALKER_UPDATE_SIGNING_KEY_HEX`
  - `FLISTWALKER_UPDATE_PUBLIC_KEY_HEX`
- build toolchain override:
  - `FLISTWALKER_WINDOWS_WINDRES`
  - `FLISTWALKER_WINDOWS_AR`
  - `FLISTWALKER_MACOS_SIGN_IDENTITY`
- 開発・手動試験用の update override 環境変数は release 手順の前提条件として扱わず、内部検証に限定する。公開向け release note や配布物の利用案内には記載しない。

## Signing Key Provisioning
- 初回導入または鍵ローテーション時は `cd rust && cargo run --quiet --bin generate_update_keypair` で `FLISTWALKER_UPDATE_SIGNING_KEY_HEX` と `FLISTWALKER_UPDATE_PUBLIC_KEY_HEX` を生成する。
- `FLISTWALKER_UPDATE_SIGNING_KEY_HEX` は GitHub Actions secret とローカルの安全な保管先にのみ置き、リポジトリへ保存しない。
- `FLISTWALKER_UPDATE_PUBLIC_KEY_HEX` は GitHub Actions secret と release build 環境へ設定し、配布ビルドへ埋め込む。

## SHA256SUMS 検証例
`SHA256SUMS.sig` はアプリ内部の更新検証に使う detached signature であり、一般ファイル操作で選んだ `.ps1` の既定動作とは無関係である。Windows の一般 `.ps1` は既定で直接実行せず、自己更新の内部 helper script のみ updater モジュールから起動する。

PowerShell:
```powershell
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.2.0-windows-x86_64.exe
Get-FileHash -Algorithm SHA256 .\FlistWalker-0.2.0-windows-x86_64.zip
```

bash:
```bash
sha256sum -c SHA256SUMS
```

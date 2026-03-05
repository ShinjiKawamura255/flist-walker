# FlistWalker

`fzf --walker` 相当の体験で、ファイル/フォルダを高速にファジー検索し、実行またはオープンできる Rust ツールです。

- 表示名: `FlistWalker`
- GitHub リポジトリ名: `flist-walker`
- 実行コマンド: `flistwalker`（Windows 成果物は `FlistWalker.exe`）

## 主要機能

- `FileList.txt` / `filelist.txt` 優先読み込み（ルート直下のみ）
- File / Folder の高速インデックスと検索
- 検索演算子: `'`（完全一致）, `!`（除外）, `^`（先頭）, `$`（末尾）
- 結果ハイライト、非一致非表示、ピン留め複数選択
- プレビュー（オンデマンドファイルは自動スキップ）
- `Create File List` で現在Rootから `FileList.txt` を生成

## クイックスタート（GUI）

```bash
cd rust
source ~/.cargo/env
cargo run -- --root ..
```

1. 検索窓に入力して候補を絞り込み
2. `Enter` で開く/実行
3. `Shift+Enter` で選択項目の格納フォルダを開く（同じフォルダは1回だけ開く）
4. `Tab` / `Shift+Tab` でピン留め複数選択
5. `Ctrl+Shift+C` で選択パスをコピー（macOS は `Cmd+Shift+C`）

## ショートカット差分（Windows/Linux と macOS）

macOS では次の「主要ショートカット」を `Ctrl` から `Cmd` に切り替えています。

- `Ctrl+T` / `Ctrl+W` / `Ctrl+Tab` / `Ctrl+Shift+Tab`
- `Ctrl+L`
- `Ctrl+Shift+C`

次は OS 共通で同じです。

- `Enter` / `Shift+Enter`
- `Tab` / `Shift+Tab`（PIN トグル）
- `ArrowUp` / `ArrowDown`
- `Ctrl+V` / `Alt+V`（ページ移動）
- `Ctrl+N` / `Ctrl+P`（Emacs/terminal系）
- `Ctrl+G` / `Ctrl+I` / `Ctrl+J` / `Ctrl+M`（Emacs/terminal系）

## Rust 実装

```bash
cd rust
source ~/.cargo/env
cargo run
```

CLI モード:

```bash
cd rust
source ~/.cargo/env
cargo run -- --cli "main" --root .. --limit 1000
```

## 挙動

- `FileList.txt` または `filelist.txt` がルート直下にある場合はそれを優先して読み込みます。
- リストがない場合は walker で再帰走査します。
- ファイル選択時は実行または既定アプリでオープン、フォルダ選択時はファイルマネージャでオープンします。

### オプションチェックボックス

- `Use FileList`: ONで `FileList.txt` / `filelist.txt` を優先利用
- `Files`: ファイル表示のON/OFF（表示フィルタ）
- `Folders`: フォルダ表示のON/OFF（表示フィルタ）
- `Regex`: 正規表現検索を有効化

## テスト

```bash
cd rust
source ~/.cargo/env
cargo test
```

## Windows 向けビルド

WSL / Linux シェルから:

```bash
./scripts/build-rust-win.sh
```

このスクリプトは WSL から `powershell.exe` を呼び出し、Windows 側の `rustup/cargo` でビルドします。
Explorer アイコンを正しく埋め込むため、Windows 側に Rust（MSVC ツールチェイン）をセットアップしてください。

クリーンビルド:

```bash
./scripts/build-rust-win-clean.sh
```

Windows PowerShell から:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build-rust-win.ps1
```

クリーンビルド:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build-rust-win-clean.ps1
```

成果物:

`rust/target/x86_64-pc-windows-msvc/release/FlistWalker.exe`

## macOS 向けビルド

通常ビルド:

```bash
./scripts/build-rust-macos.sh
```

クリーンビルド:

```bash
./scripts/build-rust-macos-clean.sh
```

成果物（ホストターゲット）:

`rust/target/release/flistwalker`

## リリースアセット生成

`exe単体 + zip` のアセットは次で生成できます。

```bash
./scripts/prepare-release.sh v0.1.1
```

```bash
./scripts/prepare-release-macos.sh v0.1.1
```

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\prepare-release.ps1 -Version v0.1.1
```

詳細は `docs/RELEASE.md` を参照してください。

生成物（例: `v0.2.1`）:
- `dist/v0.2.1/FlistWalker-0.2.1-windows-x86_64.exe`
- `dist/v0.2.1/FlistWalker-0.2.1-windows-x86_64.zip`
- `dist/v0.2.1/FlistWalker-0.2.1-macos-arm64`
- `dist/v0.2.1/FlistWalker-0.2.1-macos-arm64.app`
- `dist/v0.2.1/FlistWalker-0.2.1-macos-arm64-app.zip`
- `dist/v0.2.1/FlistWalker-0.2.1-macos-arm64.tar.gz`
- `dist/v0.2.1/SHA256SUMS`

注:
- ZIP内の実行ファイル名は `flistwalker.exe` です（単体配布exe名とは別）。

## macOS 署名と notarization

1. まず通常アセットを生成:

```bash
./scripts/prepare-release-macos.sh v0.8.0
```

2. notarytool プロフィールを作成（初回のみ）:

```bash
xcrun notarytool store-credentials flistwalker-notary --apple-id "<APPLE_ID>" --team-id "<TEAM_ID>" --password "<APP_SPECIFIC_PASSWORD>"
```

3. Developer ID 署名 + notarization + staple:

```bash
export FLISTWALKER_MACOS_SIGN_IDENTITY="Developer ID Application: Example Corp (TEAMID1234)"
./scripts/sign-notarize-macos.sh v0.8.0 arm64 flistwalker-notary
```

## プロトタイプ資産

旧プロトタイプは `prototype/python/` に移設済みです。

## License

MIT License（`LICENSE` を参照）

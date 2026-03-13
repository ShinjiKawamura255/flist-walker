# FlistWalker

`fzf --walker` 相当の体験で、ファイル/フォルダを高速にファジー検索し、実行またはオープンできる Rust ツールです。

- 表示名: `FlistWalker`
- GitHub リポジトリ名: `flist-walker`
- 実行コマンド: `flistwalker`（Windows 成果物は `FlistWalker.exe`）

## 主要機能

- マルチタブ
- `FileList.txt` / `filelist.txt` 優先読み込み（ルート直下のみ）
- File / Folder の高速インデックスと検索
- 検索演算子: `'`（完全一致）, `!`（除外）, `^`（先頭）, `$`（末尾）
- 結果ハイライト、非一致非表示、ピン留め複数選択
- プレビュー（オンデマンドファイルは自動スキップ）
- Root の保存、既定 root 設定
- 検索履歴（全タブ共通）
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
6. `Ctrl+R` で検索履歴をファジー検索し、`Enter` / `Ctrl+J` / `Ctrl+M` で検索欄へ展開

### 主なショートカット

- `Up` / `Down` または `Ctrl+P` / `Ctrl+N`: 現在行を移動
- `Ctrl+V` / `Alt+V`: ページ移動
- `Enter` / `Ctrl+J` / `Ctrl+M`: 開く / 実行
- `Shift+Enter`: 格納フォルダを開く
- `Tab` / `Shift+Tab` / `Ctrl+I`: 現在行のピン留め切り替え
- `Ctrl+Shift+C`: 選択パスをコピー
- `Esc` / `Ctrl+G`: query とピン留めをクリア
- `Ctrl+L`: 検索欄の focus 切り替え
- `Ctrl+T`: 新規タブ
- `Ctrl+W`: 現在タブを閉じる
- `Ctrl+Tab` / `Ctrl+Shift+Tab`: タブ切り替え

## ショートカット差分（Windows/Linux と macOS）

macOS では次の「主要ショートカット」を `Ctrl` から `Cmd` に切り替えています。

- `Ctrl+T` / `Ctrl+W` / `Ctrl+Tab` / `Ctrl+Shift+Tab`
- `Ctrl+L`
- `Ctrl+Shift+C`

### 入力履歴

- 検索履歴は全タブ共通で最大100件まで保持され、通常終了時の UI state へ永続化されます。
- 永続化先はホーム直下の平文ファイルです。検索語やパスに機微情報を含める運用では注意してください。
- `FLISTWALKER_DISABLE_HISTORY_PERSIST=1` を設定すると、検索履歴の読み込みと保存を無効化できます。
- `Ctrl+R` で履歴検索モードに入り、同じ検索欄で履歴をファジー検索できます。
- 履歴検索中は `Enter` / `Ctrl+J` / `Ctrl+M` で選択中の履歴を検索欄へ展開し、`Esc` / `Ctrl+G` でキャンセルして元の query に戻れます。
- 履歴は打鍵ごとには保存されず、入力が少し止まった時点、または結果移動を始めた時点で確定します。
- IME の未確定文字列は履歴に保存されず、確定後の検索語だけが残ります。

### セッション復元（opt-in）

- `FLISTWALKER_RESTORE_TABS=1` を設定すると、終了時のタブ状態を次回起動時に復元できます。
- 復元対象は `root`、`query`、`Use FileList`、`Regex`、`Files`、`Folders`、active tab です。
- 起動時負荷を抑えるため、起動直後に再インデックスするのは active tab のみで、他のタブは最初に開いた時点で遅延 reindex します。
- `--root` や起動時 query を明示した場合は復元よりそちらを優先します。
- この機能が有効な間は、起動 root がタブ復元で決まるため `Set as default` は無効化されます。

Windows PowerShell でユーザー環境変数として永続設定:

```powershell
[Environment]::SetEnvironmentVariable("FLISTWALKER_RESTORE_TABS", "1", "User")
```

Windows PowerShell でユーザー環境変数を削除:

```powershell
[Environment]::SetEnvironmentVariable("FLISTWALKER_RESTORE_TABS", $null, "User")
```

Windows PowerShell の現在セッションだけ有効化:

```powershell
$env:FLISTWALKER_RESTORE_TABS = "1"
```

Windows PowerShell の現在セッションだけ削除:

```powershell
Remove-Item Env:FLISTWALKER_RESTORE_TABS
```

検索履歴永続化を無効化する例:

```powershell
$env:FLISTWALKER_DISABLE_HISTORY_PERSIST = "1"
```

```bash
export FLISTWALKER_DISABLE_HISTORY_PERSIST=1
```

Windows CMD の現在セッションだけ有効化:

```cmd
set FLISTWALKER_RESTORE_TABS=1
```

macOS の現在セッションだけ有効化:

```bash
export FLISTWALKER_RESTORE_TABS=1
```

macOS で永続設定（zsh）:

```bash
echo 'export FLISTWALKER_RESTORE_TABS=1' >> ~/.zshrc
```

Linux の現在セッションだけ有効化:

```bash
export FLISTWALKER_RESTORE_TABS=1
```

Linux で永続設定（bash）:

```bash
echo 'export FLISTWALKER_RESTORE_TABS=1' >> ~/.bashrc
```

## Rust 実装

```bash
cd rust
source ~/.cargo/env
cargo run -- --root ..
```

CLI モード:

```bash
cd rust
source ~/.cargo/env
cargo run -- --cli "main" --root .. --limit 1000
```

CLI では:

- query 未指定時は候補一覧を `limit` 件まで表示します。
- query 指定時はスコア付きで結果を表示します。
- 現状の CLI は GUI と違って `Regex` 切り替えを持たず、通常検索のみです。

## 挙動

- `FileList.txt` または `filelist.txt` がルート直下にある場合はそれを優先して読み込みます。
- ルート直下の `FileList.txt` / `filelist.txt` に含まれる配下の `FileList.txt` / `filelist.txt` も必要に応じて展開します。
- リストがない場合は walker で再帰走査します。
- ファイル選択時は実行または既定アプリでオープン、フォルダ選択時はファイルマネージャでオープンします。
- `Create File List` は必要に応じて Walker ベースの新規タブへ切り替えて生成します。

### オプションチェックボックス

- `Use FileList`: ONで `FileList.txt` / `filelist.txt` を優先利用
- `Files`: ファイル表示のON/OFF
- `Folders`: フォルダ表示のON/OFF
- `Regex`: 正規表現検索を有効化
- `Preview`: プレビューペインの表示切り替え

### Root 操作

- `Browse...`: Root を変更
- `Set as default`: 次回起動時の既定 root を保存
- `Add to list`: 現在 root を保存済みリストへ追加
- `Remove from list`: 現在 root を保存済みリストから削除

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

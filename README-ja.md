# FlistWalker

`README.md` の英語版は [README.md](README.md) を参照してください。

`fzf --walker` 相当の体験で、ファイル/フォルダを高速にファジー検索し、選択結果を実行またはオープンできる Rust ツールです。

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
- 実行ファイル横の `flistwalker.ignore.txt` による Ignore List
- Windows では `%LocalAppData%\flistwalker\`、Linux/macOS では `~/.flistwalker/` による runtime config / session files

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
- タブのドラッグ&ドロップ: タブの並び替え

## ショートカット差分（Windows/Linux と macOS）

macOS では次の「主要ショートカット」を `Ctrl` から `Cmd` に切り替えています。

- `Ctrl+T` / `Ctrl+W`
- `Ctrl+L`
- `Ctrl+Shift+C`

タブ切り替えだけはブラウザなどと同様に、macOS でも `Ctrl+Tab` / `Ctrl+Shift+Tab` を使います。

### 入力履歴

- 検索履歴は全タブ共通で最大100件まで保持され、通常終了時の UI state へ永続化されます。
- 永続化先は Windows では `%LocalAppData%\flistwalker\`、Linux/macOS では `~/.flistwalker/` のセッションファイルです。検索語やパスに機微情報を含める運用では注意してください。
- `FLISTWALKER_DISABLE_HISTORY_PERSIST=1` を設定すると、検索履歴の読み込みと保存を無効化できます。
- `Ctrl+R` で履歴検索モードに入り、同じ検索欄で履歴をファジー検索できます。
- 履歴検索中は `Enter` / `Ctrl+J` / `Ctrl+M` で選択中の履歴を検索欄へ展開し、`Esc` / `Ctrl+G` でキャンセルして元の query に戻れます。
- 履歴は打鍵ごとには保存されず、入力が少し止まった時点、または結果移動を始めた時点で確定します。
- IME の未確定文字列は履歴に保存されず、確定後の検索語だけが残ります。

### セッション復元（opt-in）

- `FLISTWALKER_RESTORE_TABS=1` を設定すると、終了時のタブ状態を次回起動時に復元できます。
- 復元対象は `root`、`query`、`Use FileList`、`Regex`、`Files`、`Folders`、active tab です。
- 起動時に `--root` や query を明示した場合は、復元よりそちらを優先します。
- この機能が有効な間は、起動 root がタブ復元で決まるため `Set as default` は無効化されます。

Windows PowerShell でユーザー環境変数として永続設定:

```powershell
[Environment]::SetEnvironmentVariable("FLISTWALKER_RESTORE_TABS", "1", "User")
```

Windows PowerShell で現在セッションだけ有効化:

```powershell
$env:FLISTWALKER_RESTORE_TABS = "1"
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

### runtime config

- runtime settings は Windows では `%LocalAppData%\flistwalker\`、Linux/macOS では `~/.flistwalker/` とその関連ファイルに保存されます。
- 初回起動でファイルが無い場合は、現在の `FLISTWALKER_*` 環境変数を seed にして自動生成します。
- 初回生成時は、実際に環境変数で設定されている値だけを書き込み、未設定の項目は省略されます。未設定項目は読み込み時に既定値へフォールバックします。
- いったんファイルができたら、その内容が runtime settings の source of truth になり、同名 env は初期 seed としてのみ使われます。
- この Windows / home の保存先ルールは UI state、saved roots、window trace にも適用されます。
- 旧バージョンから更新した場合は、新しいファイルがまだ無いときに限って、実行ファイル横や home 直下の旧ファイルを新しい platform-specific 保存先へ自動移行します。
- このファイルは JSON なので直接編集できます。
- ここでは一般的に使う項目だけを案内しています。高度な項目は意図的に記載していません。
- ファイルを削除すると、次回起動時に現在の環境変数を seed にして再生成されます。
- `walker_max_entries` は大きい root で効くので、ここでは公開しています。

例:

```json
{
  "walker_max_entries": 500000,
  "history_persist_disabled": false,
  "restore_tabs_enabled": false
}
```

- boolean 系の値は `true` / `false` で書きます。

### 環境変数の公開区分

- runtime settings は Windows では `%LocalAppData%\flistwalker\` の config file、Linux/macOS では `~/.flistwalker/` と関連する session file に集約され、同名 env は seed 用です。
- signing / release build 用の環境変数は `docs/RELEASE.md` の build/release 用セクションだけで扱います。

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
- `--limit` は内部で 1000 件に丸めず、そのまま上限件数として扱います。
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
- `Use Ignore List`: 実行ファイル横の ignore ルールを有効化/無効化する。既定は ON。

### Ignore List

- `flistwalker.ignore.txt` を `flistwalker` / `FlistWalker.exe` と同じフォルダに置きます。
- 1 行 1 ルールが基本です。空行と `#` で始まる行は無視されます。
- 各トークンは検索の除外条件として扱われます。例えば `old` や `~` は `!old !~` と同じ挙動になります。
- 1 行に複数トークンをスペース区切りで書くこともできます。
- `Use Ignore List` チェックボックスで適用の ON/OFF を切り替えます。既定は ON です。
- サンプルは [flistwalker.ignore.txt.example](flistwalker.ignore.txt.example) を参照してください。
- サンプルが無い場合は、起動時に実行ファイル横へ自動生成します。
- そのファイルを実際の Ignore List として使う場合は、`flistwalker.ignore.txt` にリネームしてください。

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

## サポート / 不具合報告

不具合報告や機能要望は GitHub Issues のテンプレートを利用してください。報告前に [docs/SUPPORT.md](docs/SUPPORT.md) を確認し、ユーザー名、プロジェクト名、フルパス、トークンなどの機微情報は必ず伏せてください。

## Windows 向けビルド

WSL / Linux シェルから:

```bash
./scripts/build-rust-win.sh
```

このスクリプトは WSL / Linux 側だけで `x86_64-pc-windows-gnu` をビルドします。
Explorer アイコン埋め込みも WSL 側で行うため、PowerShell や Windows 側 Rust は不要です。
必要なツール:

- `x86_64-w64-mingw32-gcc`
- `x86_64-w64-mingw32-g++`
- `x86_64-w64-mingw32-ar`
- `x86_64-w64-mingw32-ranlib`
- `x86_64-w64-mingw32-windres`
- `x86_64-w64-mingw32-strip`

Ubuntu / Debian 系では `sudo apt install -y gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64 binutils-mingw-w64-x86-64` で揃います。

release profile では `lto = "thin"`, `codegen-units = 1`, `panic = "abort"`, `strip = "symbols"` を適用し、さらにビルド後に `x86_64-w64-mingw32-strip` を実行して Windows GNU バイナリサイズを抑えます。

クリーンビルド:

```bash
./scripts/build-rust-win-clean.sh
```

旧 `scripts/build-rust-win.ps1` / `scripts/build-rust-win-clean.ps1` は退役済みで、WSL/Linux 側ビルドへ誘導するエラーを返します。

成果物:

`rust/target/x86_64-pc-windows-gnu/release/FlistWalker.exe`

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

## ライセンスと配布 notices

- `LICENSE`
- `THIRD_PARTY_NOTICES.txt`
- 配布ルール: [docs/RELEASE.md](docs/RELEASE.md)

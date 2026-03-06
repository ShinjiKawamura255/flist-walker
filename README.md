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
- 検索履歴（タブ単位）
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
5. `Ctrl+Shift+C` で選択パスをコピー
6. `Ctrl+R` / `Ctrl+Shift+R` で検索履歴を戻る/進む

### 主なショートカット

- `Up` / `Down` または `Ctrl+P` / `Ctrl+N`: 現在行を移動
- `Ctrl+V` / `Alt+V`: ページ移動
- `Enter` / `Ctrl+J` / `Ctrl+M`: 開く / 実行
- `Shift+Enter`: 格納フォルダを開く
- `Tab` / `Shift+Tab` / `Ctrl+I`: 現在行のピン留め切り替え
- `Ctrl+Shift+C`: 選択パスをコピー
- `Ctrl+G`: query とピン留めをクリア
- `Ctrl+L`: 検索欄の focus 切り替え
- `Ctrl+T`: 新規タブ
- `Ctrl+W`: 現在タブを閉じる
- `Ctrl+Tab` / `Ctrl+Shift+Tab`: タブ切り替え

### 入力履歴

- 検索履歴はタブごとに保持されます。
- `Ctrl+R` で過去の検索語、`Ctrl+Shift+R` で新しい検索語へ移動できます。
- 履歴は打鍵ごとには保存されず、入力が少し止まった時点、または結果移動を始めた時点で確定します。
- IME の未確定文字列は履歴に保存されず、確定後の検索語だけが残ります。

### セッション復元（opt-in）

- `FLISTWALKER_RESTORE_TABS=1` を設定すると、終了時のタブ状態を次回起動時に復元できます。
- 復元対象は `root`、`query`、`Use FileList`、`Regex`、`Files`、`Folders`、active tab です。
- 起動時負荷を抑えるため、起動直後に再インデックスするのは active tab のみで、他のタブは最初に開いた時点で遅延 reindex します。
- `--root` や起動時 query を明示した場合は復元よりそちらを優先します。
- この機能は環境変数が無効な限り動作せず、通常の `Set as default` の挙動は変わりません。

PowerShell でユーザー環境変数として永続設定:

```powershell
[Environment]::SetEnvironmentVariable("FLISTWALKER_RESTORE_TABS", "1", "User")
```

PowerShell でユーザー環境変数を削除:

```powershell
[Environment]::SetEnvironmentVariable("FLISTWALKER_RESTORE_TABS", $null, "User")
```

現在の PowerShell セッションだけ有効化:

```powershell
$env:FLISTWALKER_RESTORE_TABS = "1"
```

現在の PowerShell セッションだけ削除:

```powershell
Remove-Item Env:FLISTWALKER_RESTORE_TABS
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

## リリースアセット生成

`exe単体 + zip` のアセットは次で生成できます。

```bash
./scripts/prepare-release.sh v0.1.1
```

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\prepare-release.ps1 -Version v0.1.1
```

詳細は `docs/RELEASE.md` を参照してください。

生成物（例: `v0.2.1`）:
- `dist/v0.2.1/FlistWalker-0.2.1-windows-x86_64.exe`
- `dist/v0.2.1/FlistWalker-0.2.1-windows-x86_64.zip`
- `dist/v0.2.1/SHA256SUMS`

注:
- ZIP内の実行ファイル名は `flistwalker.exe` です（単体配布exe名とは別）。

## プロトタイプ資産

旧プロトタイプは `prototype/python/` に移設済みです。

## License

MIT License（`LICENSE` を参照）

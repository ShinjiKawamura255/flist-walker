# FastFileFinder

`fzf --walker` 風の高速ファジー検索ツールの Python 試作です。

## セットアップ

```bash
python -m venv .venv
source .venv/bin/activate
pip install -e .[dev,gui]
```

## 使い方

```bash
fast-file-finder --root . --limit 20
fast-file-finder "main" --root .
fast-file-finder --gui --root .
fast-file-finder-gui --root .
./scripts/run-gui.sh --root .
powershell -ExecutionPolicy Bypass -File .\scripts\run-gui.ps1 --root .
```

- `FileList.txt` または `filelist.txt` がルート直下にある場合はそれを優先。
- なければ walker 方式で再帰走査。
- 検索結果の番号を選ぶと、ファイルは実行/オープン、フォルダはオープン。

## GUI 試作の仕様（Python）

- 上部: `Root` と `Source`（`FileList` か `Walker`）を表示。
- ルート切替: `Browse...` で選択した時点で検索対象ディレクトリを即時反映。
- 絞り込みトグル: `Use FileList` / `Files` / `Folders` で候補ソースと種別を切替。
- 検索モード: `Regex` トグルで正規表現検索の ON/OFF を切替。
- 検索入力: 入力 120ms 後に再検索（打鍵ごとの過剰再描画を抑制）。
- 左ペイン: ファイル/フォルダ種別付き結果リスト（一致文字をハイライト、非マッチは非表示）。
- 表示件数: 検索あり/なしとも最大 1000 件を表示（超過時はステータスバーで上限到達を表示）。
- 右ペイン: プレビュー（フォルダは子要素数、テキストファイルは先頭20行）。
- アクション: `Open / Execute` ボタン、結果ダブルクリック、Enter キーで実行。
- `Tab` でピン留めが1件以上ある場合、Enter 実行対象はピン留め項目のみ（カーソル項目は対象外）。
- `Copy Path(s)` で対象パスをクリップボードへコピー（複数時は改行区切り）。
- `Clear Selected` で Tab ピン留め項目を一括解除。
- `Create File List` で現在ルートを再走査して `FileList.txt` を生成。
- キー操作: `↑/↓` と `Ctrl+P / Ctrl+N` で選択移動、`Enter` と `Ctrl+M / Ctrl+J` で実行。
- `Tab` で現在行をピン留め/解除して次行へ、`Shift+Tab` でピン留め/解除して前行へ。
- リスト左端は「カーソル位置(▶)」と「ピン留め(◆)」を別表示。
- Emacs 編集操作（検索欄）: `Ctrl+A/E/B/F` 移動、`Ctrl+H/D` 削除、`Ctrl+W/K` カット、`Ctrl+Y` ペースト、`Ctrl+U` でカーソル左側を全削除。
- 補助: `Refresh Index` で再走査。ステータスバーに件数と操作ヒントを表示。

### 検索クエリ構文

- 通常語: ファジー検索（AND 条件）
- `'word`: `word` の完全一致条件（半角スペースまで）
- `!word`: `word` を含む候補を除外（半角スペースまで）
- `^word`: Regex OFF では `w`（`^` 隣接文字）が先頭条件、全体はファジー
- `word$`: Regex OFF では `d`（`$` 隣接文字）が末尾条件、全体はファジー
- `Regex` ON 時の通常語: 正規表現として評価（`'` と `!` はそのまま有効）

## テスト

```bash
pytest
```

## Rust 実装

Rust 版は `rust/` 配下です。

```bash
cd rust
cargo run
```

詳細は `rust/README.md` を参照してください。

### Windows 向けビルド

初回のみ:

```bash
source ~/.cargo/env
cargo install cargo-xwin
```

WSL / Linux シェルから:

```bash
./scripts/build-rust-win.sh
```

クリーンビルドしたい場合:

```bash
./scripts/build-rust-win-clean.sh
```

Windows PowerShell から:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build-rust-win.ps1
```

クリーンビルドしたい場合:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build-rust-win-clean.ps1
```

PowerShell スクリプトは、Windows 側に `rustup` / `cargo` が導入済みである前提です。

成果物:

`rust/target/x86_64-pc-windows-msvc/release/FastFileFinder.exe`

注記:
生成済み EXE を実行中のままだと上書きビルドに失敗します。ビルド前にアプリを終了してください。

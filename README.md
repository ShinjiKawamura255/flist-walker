# FlistWalker

`fzf --walker` 相当の体験で、ファイル/フォルダを高速にファジー検索し、実行またはオープンできる Rust ツールです。

- 表示名: `FlistWalker`
- GitHub リポジトリ名: `flist-walker`
- 実行コマンド: `flistwalker`（Windows 成果物は `FlistWalker.exe`）

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

## プロトタイプ資産

旧プロトタイプは `prototype/python/` に移設済みです。

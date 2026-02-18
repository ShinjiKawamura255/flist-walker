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

## プロトタイプ資産

旧プロトタイプは `prototype/python/` に移設済みです。

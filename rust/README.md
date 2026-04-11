# FlistWalker Rust

FlistWalker の Rust 本実装です。

## Build (WSL/Linux host)

```bash
cd rust
source ~/.cargo/env
cargo build
```

## Build (macOS host)

```bash
cd ..
./scripts/build-rust-macos.sh
# clean build
./scripts/build-rust-macos-clean.sh
```

## Build (Windows EXE with Explorer icon)

WSL/Linux 側だけで `x86_64-pc-windows-gnu` をビルド:

```bash
cd ..
./scripts/build-rust-win.sh
```

必要なツール:

- `x86_64-w64-mingw32-gcc`
- `x86_64-w64-mingw32-g++`
- `x86_64-w64-mingw32-ar`
- `x86_64-w64-mingw32-ranlib`
- `x86_64-w64-mingw32-windres`

Ubuntu / Debian 系では `sudo apt install -y gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64 binutils-mingw-w64-x86-64` で揃います。

## Regression Guard

- 発生条件: `x86_64-pc-windows-gnu` へ切り替えた後、`windres` で生成した `resource.o` が最終 `flistwalker.exe` にリンクされず、Windows Explorer でアプリアイコンが既定アイコンになる。
- 期待動作: GNU ビルドでも `flistwalker.exe` の `.rsrc` にアイコンリソースが含まれ、Explorer 上で埋め込みアイコンが表示される。
- 非対象範囲: 実行中ウィンドウのタイトルバーアイコン差し替えや、関連付けされた別ファイル形式のアイコン表示。
- 関連テストID: TC-090.

## Run (GUI)

```bash
cd rust
source ~/.cargo/env
cargo run
# または
cargo run -- --root .. --limit 1000
```

## Run (CLI)

```bash
cd rust
source ~/.cargo/env
cargo run -- --cli "main"
# バージョン確認
cargo run -- --version
```

## Environment variables

- `FLISTWALKER_RESTORE_TABS=1`
  終了時のタブ状態を次回起動時に復元します。
- `FLISTWALKER_DISABLE_HISTORY_PERSIST=1`
  query history の読み込みと保存を無効化します。検索語は既定で平文保存されるため、機微情報を扱う運用ではこの設定を推奨します。

Examples:

```bash
export FLISTWALKER_RESTORE_TABS=1
export FLISTWALKER_DISABLE_HISTORY_PERSIST=1
```

```powershell
$env:FLISTWALKER_RESTORE_TABS = "1"
$env:FLISTWALKER_DISABLE_HISTORY_PERSIST = "1"
```

## Test

```bash
cd rust
source ~/.cargo/env
cargo test
```

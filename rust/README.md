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

## Develop (Windows host)

Windows では配布用 Windows build と同じ GNU target 系を日常開発の第一候補にします:

```powershell
cd ..
.\scripts\dev-check-windows.ps1
```

個別に実行する場合:

```powershell
cargo test --target x86_64-pc-windows-gnu
cargo build --target x86_64-pc-windows-gnu
cargo clippy --target x86_64-pc-windows-gnu --all-targets -- -D warnings
```

GNU toolchain が未導入の環境で一時的に確認する場合だけ `..\scripts\dev-check-windows.ps1 -Toolchain msvc` を使います。

初回 setup:

```powershell
rustup target add x86_64-pc-windows-gnu
```

MSYS2 を `C:\msys64` へインストールし、MSYS2 shell から MINGW64 GCC package を入れます:

```bash
pacman -S mingw-w64-x86_64-gcc
```

## Build (Windows EXE with Explorer icon)

Windows PowerShell から:

```powershell
cd ..
.\scripts\build-rust-win.ps1
# clean build
.\scripts\build-rust-win-clean.ps1
```

不足依存の検出だけは `-CheckOnly`、自動導入なしは `-NoInstall`、明示的な
一括導入承認は `-InstallMissing` を指定します。

WSL/Linux から:

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

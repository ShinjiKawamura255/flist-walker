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

WSL から Windows 側 Rust を使ってビルド:

```bash
cd ..
./scripts/build-rust-win.sh
```

PowerShell から直接ビルド:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build-rust-win.ps1
```

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

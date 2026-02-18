# FlistWalker Rust

FlistWalker の Rust 本実装です。

## Build (WSL/Linux host)

```bash
cd rust
source ~/.cargo/env
cargo build
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

## Test

```bash
cd rust
source ~/.cargo/env
cargo test
```

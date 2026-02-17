# FlistWalker Rust

Python プロトタイプの仕様を Rust に移植した実装です。

## Build

```bash
cd rust
source ~/.cargo/env
cargo build
```

Windows 向けクロスコンパイル（WSL）:

```bash
cd ..
./scripts/build-rust-win.sh
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

# OSS Compliance

依存追加・依存更新・配布導線変更のたびに、OSS 配布義務を同一変更で確認するための運用メモ。

## Scope
- Rust 依存の追加・削除・更新（`rust/Cargo.toml`, `rust/Cargo.lock`）
- release script / workflow / updater の変更で、配布物へ含める OSS 文書や asset 構成が変わる場合
- `LICENSE`, `THIRD_PARTY_NOTICES.txt`, sidecar asset, archive 同梱物に影響する変更

## Required outputs
- repo root の `LICENSE`
- repo root の `THIRD_PARTY_NOTICES.txt`
- release asset に含める sidecar:
  - `*.LICENSE.txt`
  - `*.THIRD_PARTY_NOTICES.txt`
- archive / app bundle / self-update 配置先へ入る:
  - `LICENSE.txt`
  - `THIRD_PARTY_NOTICES.txt`

## Checklist
1. 追加・更新した依存を洗い出す。
   - `git diff -- rust/Cargo.toml rust/Cargo.lock`
   - 依存増減だけでなく feature 変更も対象にする。
   - release 対象ごとの resolve graph を確認する。
     - `cd rust && cargo metadata --locked --format-version 1 --filter-platform x86_64-pc-windows-gnu`
     - `cd rust && cargo metadata --locked --format-version 1 --filter-platform x86_64-unknown-linux-gnu`
     - `cd rust && cargo metadata --locked --format-version 1 --filter-platform x86_64-apple-darwin`
     - `cd rust && cargo metadata --locked --format-version 1 --filter-platform aarch64-apple-darwin`
2. 追加依存の license / notice 要否を確認する。
   - crates.io / upstream repository / license file を見て、再配布条件と notice 要件を確認する。
   - copyleft や追加条件付き license は、そのまま採用せず影響を整理する。
   - 観測性・診断用依存（例: `tracing`, `tracing-subscriber`）も対象外にしない。
   - `THIRD_PARTY_NOTICES.txt` に残っている crate が現行 resolve graph から消えていないかも確認する。
3. `THIRD_PARTY_NOTICES.txt` を更新する。
   - direct dependency と配布上重要な transitive dependency の変化を反映する。
   - release / self-update / sidecar 導線で配る内容と矛盾させない。
4. 配布導線を確認する。
   - `scripts/prepare-release*.sh|ps1`
   - `.github/workflows/release-tagged.yml`
   - `rust/src/updater.rs`
   - archive / sidecar / self-update 復元先に `LICENSE` / `THIRD_PARTY_NOTICES` が残ることを確認する。
5. docs を更新する。
   - 運用や配布物の前提が変わる場合は `docs/RELEASE.md`, `.github/release-template.md`, `AGENTS.md` を同一変更で更新する。
6. 検証結果を残す。
   - PR または作業報告に「どの依存が増えたか」「どの OSS 対応を更新したか」「何で確認したか」を明記する。

## Minimum evidence in completion report
- 追加・更新した依存名
- 更新した OSS 関連ファイル
- 確認した配布導線
- 実行した検証コマンド

## Repository-specific notes
- release asset と standalone 配布では `LICENSE` / `THIRD_PARTY_NOTICES` の sidecar を必須とする。
- CLI 専用 `fw` standalone は同一 platform の version 付き README/LICENSE/THIRD_PARTY_NOTICES release asset を universal binary と共有し、既存 archive の内容は変更しない。
- self-update 後も universal の `LICENSE.txt` / `THIRD_PARTY_NOTICES.txt` と CLI の `fw.LICENSE.txt` / `fw.THIRD_PARTY_NOTICES.txt` をvariant別に保持し、同一ディレクトリでversion skewがあっても互いを上書きしない。
- 依存変更時は、少なくとも `docs/RELEASE.md` に書かれている配布物一覧と矛盾しないことを確認する。

## resvg 0.48 dependency review (2026-08-19)

- Direct and build dependency: `resvg` / `usvg` move from `0.43.0` to `0.48.1`. The current releases are `Apache-2.0 OR MIT`, so the previous MPL-2.0 notice section is no longer part of the packaged resolve graph.
- The refresh removes unmaintained `rustybuzz 0.18.0` and `ttf-parser 0.24.1`; text shaping and font parsing now use maintained `harfrust 0.12.0`, `skrifa 0.44.0`, `read-fonts 0.41.0`, and `font-types 0.12.3` through `usvg`.
- Updated rendering dependencies include `fontdb 0.24.0`, `gif 0.14.2`, `image-webp 0.2.4`, `imagesize 0.15.0`, `svgtypes 0.16.1`, and BSD-3-Clause `tiny-skia` / `tiny-skia-path 0.12.0`. Metadata review found no new copyleft family or standalone notice requirement.
- `cargo metadata --locked` resolves for all four packaged targets, and `cargo audit` exits successfully with no warning output. Release archive, standalone sidecar, macOS bundle, and self-update notice paths are unchanged.

## eframe 0.35 dependency review (2026-08-01)

- Direct dependency: `eframe` and the selected `egui` / `egui_glow` / `egui-winit` stack move from `0.34.3` to `0.35.0`; `epaint_default_fonts` moves to `0.35.0` and retains its existing `(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0` notice requirement.
- Newly resolved crates are permissively licensed: `glifo 0.1.1` (Apache-2.0 OR MIT), `guillotiere 0.7.0` (MIT/Apache-2.0), `harfrust 0.7.0` (MIT), `hashbrown 0.17.1` (MIT OR Apache-2.0), and `unicode-general-category 1.1.0` (Apache-2.0). Updated transitive crates `fearless_simd 0.4.1`, `read-fonts 0.39.2`, `skrifa 0.42.1`, `vello_common 0.0.9`, and `vello_cpu 0.0.9` remain MIT/Apache-2.0-family licensed.
- `THIRD_PARTY_NOTICES.txt` updates the egui stack and bundled-font versions. No new copyleft family or additional standalone notice text enters the packaged target graphs.
- Packaging is unchanged; release archives, standalone sidecars, macOS app bundles, and self-update continue to use the existing `LICENSE` / `THIRD_PARTY_NOTICES` paths.

## CLI argument-limit dependency review (2026-07-29)

- Direct dependency: Unix targets add `libc 0.2` (MIT OR Apache-2.0) to query `_SC_ARG_MAX`; the crate was already present in the resolved transitive graph, so no new package or license family enters release artifacts.
- `THIRD_PARTY_NOTICES.txt` now lists `libc` in the direct dependency summary. No additional notice or copyleft condition applies.
- Packaging is unchanged; existing archive, standalone sidecar, macOS bundle, and self-update `LICENSE` / `THIRD_PARTY_NOTICES` paths remain authoritative and require verification through the normal VM-005 checks.

## CLI terminal dependency review (2026-07-26)

- Direct dependencies: `crossterm 0.28.1` (MIT, <https://github.com/crossterm-rs/crossterm>) and `unicode-width 0.1.14` (MIT OR Apache-2.0, <https://github.com/unicode-rs/unicode-width>).
- Newly resolved terminal dependencies were reviewed as permissive: `crossterm_winapi 0.9.1` (MIT), `mio 1.1.1` (MIT), `signal-hook 0.3.18` (Apache-2.0/MIT), `signal-hook-mio 0.2.5` (MIT OR Apache-2.0), and `signal-hook-registry 1.4.8` (MIT OR Apache-2.0).
- `THIRD_PARTY_NOTICES.txt` の direct dependency summary を更新した。追加 notice や copyleft 条件はない。
- 配布物の構成は変更しない。release archive、standalone sidecar、macOS app bundle、self-update 後の `LICENSE` / `THIRD_PARTY_NOTICES` 同梱導線を既存 script/workflowで再検証する。

## Resolved audit warnings

### RUSTSEC-2026-0192 / RUSTSEC-2026-0206: unmaintained `ttf-parser` / `rustybuzz`
- Status: resolved on 2026-08-19.
- Resolution: upgraded `resvg` / `usvg` from `0.43.0` to `0.48.1`, replacing `ttf-parser 0.24.1` and `rustybuzz 0.18.0` with the maintained `skrifa` / `harfrust` stack.
- Previous observed path: `ttf-parser 0.24.1` via `fontdb 0.21.0` / `rustybuzz 0.18.0` -> `usvg 0.43.0` -> `resvg 0.43.0` -> `flist-walker`.
- Current lockfile check: `rust/Cargo.lock` no longer contains `ttf-parser` or `rustybuzz`; `cargo audit` exits successfully without warnings.
- Packaging impact: `resvg` / `usvg` are now `Apache-2.0 OR MIT`; `THIRD_PARTY_NOTICES.txt` removes the obsolete MPL-2.0 section and records `tiny-skia 0.12.0` under BSD-3-Clause.

### RUSTSEC-2026-0257: `webbrowser` Unix `BROWSER` handling argument injection
- Status: resolved on 2026-08-17.
- Resolution: refreshed `rust/Cargo.lock` from `webbrowser 1.2.0` to the
  fixed `1.2.2` release required by the advisory.
- Previous observed path from `cargo audit`: `webbrowser 1.2.0 -> egui-winit
  0.35.0 -> eframe 0.35.0 -> flist-walker`.
- Current lockfile check: `rust/Cargo.lock` contains `webbrowser 1.2.2`.
- Packaging impact: the updated transitive browser integration and its macOS
  backend remain permissively licensed; `THIRD_PARTY_NOTICES.txt` records the
  current version and the existing archive, sidecar, bundle, and self-update
  notice paths are unchanged.

### RUSTSEC-2026-0204: `crossbeam-epoch 0.9.18` invalid pointer dereference in pointer formatting
- Status: resolved on 2026-07-09.
- Resolution: refreshed `Cargo.lock` to `crossbeam-epoch 0.9.20`.
- Previous observed path from `cargo audit`: `crossbeam-epoch 0.9.18 -> crossbeam-deque 0.8.6 -> rayon-core 1.13.0 -> rayon 1.11.0 -> flist-walker`.
- Current lockfile check: `rust/Cargo.lock` contains `crossbeam-epoch 0.9.20`.

### RUSTSEC-2026-0190: `anyhow 1.0.101` unsound downcast
- Status: resolved on 2026-07-09.
- Resolution: refreshed `Cargo.lock` to `anyhow 1.0.103`.
- Previous observed path from `cargo audit`: direct dependency `anyhow 1.0.101 -> flist-walker`.
- Current lockfile check: `rust/Cargo.lock` contains `anyhow 1.0.103`.

### RUSTSEC-2026-0186: `memmap2 0.9.10` unchecked pointer offset
- Status: resolved on 2026-07-09.
- Resolution: refreshed `Cargo.lock` to `memmap2 0.9.11`.
- Previous observed path from `cargo audit`: `memmap2 0.9.10` via `winit 0.30.13` / `glutin-winit 0.5.0` / `eframe 0.34.x` and via `usvg 0.43.0` / `resvg 0.43.0`.
- Current lockfile check: `rust/Cargo.lock` contains `memmap2 0.9.11`.

### RUSTSEC-2024-0436: `paste 1.0.15` unmaintained
- Status: resolved on 2026-04-26.
- Resolution: upgraded the GUI stack from `eframe 0.29.1` to `eframe 0.34.1`, which updated `egui` / `wgpu` and removed the locked `metal -> paste` path.
- Previous observed path from `cargo audit`: `paste 1.0.15 -> metal 0.29.0 -> wgpu-hal 22.0.0 -> wgpu-core/wgpu -> egui-wgpu 0.29.1 -> eframe 0.29.1 -> flist-walker`.
- Current `cargo audit` behavior: exits successfully with no warning output.
- Current lockfile check: `rust/Cargo.lock` no longer contains `paste` or `metal` package entries.
- Compatibility note: `eframe 0.34.1` requires Rust `1.92`; repository CI and local development use the version-addressed `rust/rust-toolchain.toml` pin, currently Rust `1.97.1`.
- Required evidence for future regressions:
  - `cd rust && cargo audit`
  - verify `rust/Cargo.lock` does not reintroduce `paste` / `metal`
  - if GUI dependencies change again, run `cd rust && cargo test --locked`, `cd rust && cargo clippy --all-targets -- -D warnings`, notice/license review, and release asset sidecar review.

## Accepted audit warnings

### RUSTSEC-2026-0194 / RUSTSEC-2026-0195: `quick-xml 0.39.4` via `wayland-scanner`
- Status: accepted on 2026-07-09 as a transitive build-time advisory surfaced by `cargo audit`.
- Observed path: `quick-xml 0.39.4 -> wayland-scanner 0.31.10 -> wayland-client` / `wayland-protocols` / `smithay-client-toolkit` -> `winit 0.30.13` / `egui-winit 0.35.x` -> `eframe 0.35.x` -> `flist-walker`.
- Exposure note: the vulnerable crate is used through Wayland protocol code generation in the GUI stack; FlistWalker does not parse untrusted XML at runtime through this path, and the crate is not part of the application input surface.
- Resolution check: `cargo update -p quick-xml` can only move to `0.39.4` under current `wayland-scanner 0.31.10`; the completed `eframe 0.35.0` migration retains this build-time path, so the existing acceptance remains necessary.
- Owner: Rust dependency maintainer for release preflight.
- Review cadence: recheck on each `Cargo.lock` refresh and during every release preflight.
- Re-evaluation trigger: `wayland-scanner` updates to a `quick-xml >=0.41.0` dependency, `eframe` / `winit` / Wayland stack refresh, or any new runtime exposure.
- Central configuration: cargo-auditのproject-local探索規則に従う`rust/.cargo/audit.toml` contains only `RUSTSEC-2026-0194` and `RUSTSEC-2026-0195`; the required command is `cd rust && cargo audit`.

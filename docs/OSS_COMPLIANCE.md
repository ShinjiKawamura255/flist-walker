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
- self-update 後も配置先ディレクトリに `LICENSE.txt` / `THIRD_PARTY_NOTICES.txt` が残る必要がある。
- 依存変更時は、少なくとも `docs/RELEASE.md` に書かれている配布物一覧と矛盾しないことを確認する。

## Resolved audit warnings

### RUSTSEC-2024-0436: `paste 1.0.15` unmaintained
- Status: resolved on 2026-04-26.
- Resolution: upgraded the GUI stack from `eframe 0.29.1` to `eframe 0.34.1`, which updated `egui` / `wgpu` and removed the locked `metal -> paste` path.
- Previous observed path from `cargo audit`: `paste 1.0.15 -> metal 0.29.0 -> wgpu-hal 22.0.0 -> wgpu-core/wgpu -> egui-wgpu 0.29.1 -> eframe 0.29.1 -> flist-walker`.
- Current `cargo audit` behavior: exits successfully with no warning output.
- Current lockfile check: `rust/Cargo.lock` no longer contains `paste` or `metal` package entries.
- Compatibility note: `eframe 0.34.1` requires Rust `1.92`; repository CI and local development use Rust stable, and the local validation toolchain was `rustc 1.93.1`.
- Required evidence for future regressions:
  - `cd rust && cargo audit`
  - verify `rust/Cargo.lock` does not reintroduce `paste` / `metal`
  - if GUI dependencies change again, run `cd rust && cargo test --locked`, `cd rust && cargo clippy --all-targets -- -D warnings`, notice/license review, and release asset sidecar review.

## Accepted audit warnings

### RUSTSEC-2026-0186: `memmap2 0.9.10` unchecked pointer offset
- Status: accepted on 2026-06-23 as a transitive advisory surfaced by `cargo audit`.
- Observed path: `memmap2 0.9.10` via `winit 0.30.13` / `glutin-winit 0.5.0` / `eframe 0.34.1` and via `usvg 0.43.0` / `resvg 0.43.0` / `eframe 0.34.1`.
- Owner: Rust dependency maintainer for release preflight.
- Review cadence: recheck on each `Cargo.lock` refresh and during every release preflight.
- Re-evaluation trigger: upstream advisory fix, `eframe` / `winit` / `resvg` / `usvg` dependency refresh, or any audit output change.
- Current `cargo audit` behavior: exits successfully with one allowed warning, so release can proceed only while this note remains current.

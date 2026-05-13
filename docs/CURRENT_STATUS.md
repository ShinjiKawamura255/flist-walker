# CURRENT STATUS

## Snapshot
- Project state: Rust GUI/CLI implementation is the canonical product path.
- Current quality posture: `cargo test --locked` and stable `cargo clippy --all-targets -- -D warnings` are expected to pass before completion of Rust changes.
- CI posture: cross-platform native tests, Windows GNU cross build, cargo audit, clippy, coverage, and lightweight FileList perf gate are maintained in GitHub Actions.
- GUI validation posture: native GUI launch is not a normal PR gate; release candidates and GUI-adjacent changes require the documented `GSM-*` evidence path.

## Current Maintenance Priorities
1. Keep stable toolchain drift visible.
   - Run clippy with `-D warnings` after Rust changes, especially after stable Rust updates.
2. Raise coverage deliberately.
   - Current enforced gate is 75%; the 2026-05-14 fresh baseline is 79.08% line coverage.
   - Next target is 80% after app/GUI owner seams receive more tests.
3. Keep GUI evidence concrete.
   - Use `docs/GUI-TESTPLAN.md` and record release-candidate or GUI-adjacent smoke results in `docs/GUI-TESTREPORT.md` or `rust/target/gui-smoke/evidence/GUI-TESTREPORT.local.md`.
4. Treat `docs/TASKS.md` as history, not the main entrypoint.
   - Start from this file, then read `ARCHITECTURE.md`, `TESTPLAN.md`, or `TASKS.md` only as needed.
5. Reduce large Rust files in slices.
   - Use `docs/LARGE_RUST_FILE_REDUCTION_PLAN.md`; Slice A split oversized app test modules on 2026-05-14.
   - Remaining oversized production priorities start with `ui_model.rs`, `app/input.rs`, and `app/filelist.rs`.

## Daily Validation
```bash
cd rust
cargo test --locked
cargo clippy --all-targets -- -D warnings
cargo llvm-cov --locked --workspace --lcov --output-path target/llvm-cov/lcov.info --fail-under-lines 75
```

Use the validation matrix in `docs/TESTPLAN.md` for narrower or additional checks.

## Release-Candidate Validation Pointers
- Cross-platform CI and release workflow: `.github/workflows/ci-cross-platform.yml`, `.github/workflows/release-tagged.yml`
- GUI smoke: `docs/GUI-TESTPLAN.md`
- GUI report: `docs/GUI-TESTREPORT.md`
- Release process: `docs/RELEASE.md`
- OSS and audit posture: `docs/OSS_COMPLIANCE.md`

## History
- Detailed roadmap closures and previous evaluation notes live in `docs/TASKS.md`.
- Durable architecture and regression guards live in `docs/ARCHITECTURE.md`.
- The planned path for reducing oversized Rust files lives in `docs/LARGE_RUST_FILE_REDUCTION_PLAN.md`.

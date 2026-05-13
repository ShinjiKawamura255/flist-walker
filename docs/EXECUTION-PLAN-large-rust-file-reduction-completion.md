# Execution Plan: Large Rust File Reduction Completion

## Goal
Finish the remaining large Rust file reduction plan by reducing top-level production files above 800 lines and keeping behavior stable.

## Scope
- Slice D: move large `indexer` and `search` test modules into owner test files.
- Slice E: split remaining stateful production owners (`runtime_config.rs`, `app/session.rs`) if still above threshold.
- Slice F: split `app/index_worker.rs` and, if needed, `app/render_panels.rs` with extra care for GUI/indexing gates.
- Update `docs/LARGE_RUST_FILE_REDUCTION_PLAN.md` as slices close.

## Non-Goals
- No behavior redesign.
- No public API rename unless required by Rust visibility after module moves.
- No feature additions.

## Validation
- Slice D: `cargo test --locked indexer::`, `cargo test --locked search::`, VM-003 perf guards.
- Slice E: `cargo test --locked session`, `cargo test --locked runtime_config`.
- Slice F: VM-003 perf guards for index worker and `cargo test --locked render_tests` for render-panel moves.
- Final: `cargo fmt --all -- --check`, `cargo test --locked`, `cargo clippy --all-targets -- -D warnings`, `cargo llvm-cov --locked --workspace --lcov --output-path target/llvm-cov/lcov.info --fail-under-lines 75`, `git diff --check`.

## Review
Run a sub-agent review at the end, focused on accidental behavior changes, visibility leaks, missed validation, and plan completeness.

## Rollback
Each slice should be revertable independently. Prefer a commit after each validated slice.

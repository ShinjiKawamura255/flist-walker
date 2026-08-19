# Current Status

This document is the short current-state snapshot for maintainers. It does not own validation commands, active task queues, or completed history.

## Product Direction

- The Rust GUI/CLI implementation under `rust/` is the canonical product path.
- The Python implementation under `prototype/python/` is retained as a prototype, not as the feature-development target.
- GUI responsiveness remains the primary implementation constraint: indexing, search, preview, and FileList creation stay off the UI thread and stale worker responses must not roll state backward.

## CLI/TUI Usability Baseline (2026-07-26)

- Batch CLI provides script-safe sort-before-limit output, source/type/search controls, saved-root selection, authorized open/reveal actions, and transactional FileList creation.
- Interactive TUI provides asynchronous preview and persisted history, contextual help, direct default/saved-root startup, initial sort/ignore options, runtime options and sorting, saved-root switching and refresh, current-row authorized actions, and settled transactional FileList creation from a fresh all-kind walker snapshot. Candidate ingestion uses shared immutable batches and a bounded event-loop response budget; invalid roots surface as recoverable index failures.
- Full Rust regression, check/clippy/format/diff gates, focused TC-163 through TC-166 contracts, VM-003 indexing performance guards, and Windows ConPTY terminal evidence passed. Exact commit and evidence mapping is in [Durable History](history/durable-history.md); native action launch remains conditional on explicit manual approval and is covered automatically with a recording backend.

## Hardening Baseline (2026-07-22)

- Worker-side action authorization revalidates resolved targets immediately before OS interaction (`0274f1b`).
- Action, kind, and index scheduling are bounded, stale work is settled before I/O, and tab payloads transfer by ownership (`e9d1ae5`, `57d6eeb`).
- Query parsing, matching, ranking, and highlight spans share one compiled contract; the optimized 100k cold/warm query-shape gate is durable (`ee29108`).
- Updater staging is trust-first and bounded, while activation/recovery uses a persistent transactional state machine verified with inert Windows/Linux filesystem evidence (`cf05220`, `227fb7d`).
- FileList decoding is deterministic UTF-8 with an optional leading BOM and explicit rejection/cancellation behavior (`1b9f2d2`).
- GUI validation uses one Windows/WSL deterministic inventory and isolated staged liveness harness (`3054582`). The hardening program is closed with partial native validation: deterministic and liveness axes pass, while native interaction, Japanese IME, alternate DPI, multi-display, real UNC, and explicitly authorized external actions remain `NOT RUN` until their documented VM-002/VM-006 or release-candidate gate applies.
- The durable program record and exact commit mapping are in [Durable History](history/durable-history.md).

## Architecture Boundary Baseline (2026-07-30)

- Adaptive walker settings/classification/truncation are owned by UI-agnostic `walker_runtime/`; GUI protocol/metrics remain in `app/index_worker.rs`.
- Interactive TUI responsibilities are split into private protocol/state/worker/FileList/input/render/terminal owners with explicit dependencies; its public facade is unchanged.
- `main.rs` is a thin startup router. Typed CLI validation and batch behavior live in `cli/`, while native window bootstrap lives in `gui_launch.rs`.
- GUI dialog and top-panel rendering have private owners whose mutation seams remain existing owner methods and queued `RenderCommand` values; deterministic GUI groups pass, while native interaction and liveness remain separate release-candidate evidence axes.

## Project Issue Hardening (2026-08-19)

- The cancelled default-branch CI run for `4195e4f` was rerun successfully: Windows GNU cross-build and the aggregate `CI Gate` passed on attempt 2.
- Scheduled security-audit and latest-canary monitors now reconcile their lifecycle on the default branch: a successful recovery closes only the exact-title issue previously created by `app/github-actions`. Policy tests reject missing exact-title/bot-owner post-filters, misplaced job conditions, or multiple close operations.
- GUI owner coverage now exercises every root-list render intent and every persisted preset option label. Total line coverage is 77.28%; `render_dialogs/root_list.rs` improved from 1.18% to 12.18%, while the deterministic GUI suite remains green.
- `resvg` / `usvg` 0.48.1 replace the unmaintained `rustybuzz` / `ttf-parser` path with `harfrust` / `skrifa`; `cargo audit` completes with no warning output and packaged-target metadata resolves for Windows, Linux, and both macOS architectures.
- Native Japanese IME, alternate DPI, multi-display, real UNC, and explicitly authorized external-action axes remain `NOT RUN`; macOS release artifacts may remain unnotarized until signing infrastructure is available.

## Quality Posture

- Cross-platform native tests, Windows GNU cross-build coverage, clippy, coverage, audit, and performance checks are maintained in GitHub Actions.
- The enforced line-coverage gate is 75%; 80% remains an improvement target rather than a release prerequisite.
- Native headful GUI launch is not a normal pull-request gate. GUI-adjacent changes and release candidates use the documented `GSM-*` evidence path.
- Deterministic, Native interaction, and Liveness are independent GUI evidence axes. A deterministic or liveness PASS never promotes a required native `NOT RUN` axis to PASS.
- Rust implementation changes follow the change-specific checks in the [Validation Matrix](testplan/validation-matrix.md).

## Maintenance Priorities

1. Preserve asynchronous UI and request-ID response routing.
2. Keep pinned-toolchain warnings visible through the configured clippy gate and review latest canary drift.
3. Continue improving low-covered GUI owner seams, especially native rendering and launch boundaries, without weakening the existing threshold.
4. Keep FileList and walker performance guards aligned with indexing-path changes.
5. Record concrete GUI evidence when the validation matrix requires it.

## Continue From Here

| Need | Document |
| --- | --- |
| Choose documents or checks for a change | [INDEX.md](INDEX.md) |
| Locate source directories and entrypoints | [STRUCTURE.md](STRUCTURE.md) |
| Understand runtime ownership and invariants | [ARCHITECTURE_OVERVIEW.md](ARCHITECTURE_OVERVIEW.md), then [ARCHITECTURE.md](ARCHITECTURE.md) |
| Select validation commands | [TESTPLAN.md](TESTPLAN.md) and the [Validation Matrix](testplan/validation-matrix.md) |
| Understand task-state boundaries | [TASKS.md](TASKS.md) |
| Review completed maintenance work | [history/INDEX.md](history/INDEX.md) |
| Prepare or inspect a release | [RELEASE.md](RELEASE.md) and [releases/INDEX.md](releases/INDEX.md) |

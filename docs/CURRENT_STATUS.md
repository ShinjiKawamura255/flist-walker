# Current Status

This document is the short current-state snapshot for maintainers. It does not own validation commands, active task queues, or completed history.

## Product Direction

- The Rust GUI/CLI implementation under `rust/` is the canonical product path.
- The Python implementation under `prototype/python/` is retained as a prototype, not as the feature-development target.
- GUI responsiveness remains the primary implementation constraint: indexing, search, preview, and FileList creation stay off the UI thread and stale worker responses must not roll state backward.

## Quality Posture

- Cross-platform native tests, Windows GNU cross-build coverage, clippy, coverage, audit, and performance checks are maintained in GitHub Actions.
- The enforced line-coverage gate is 75%; 80% remains an improvement target rather than a release prerequisite.
- Native headful GUI launch is not a normal pull-request gate. GUI-adjacent changes and release candidates use the documented `GSM-*` evidence path.
- Rust implementation changes follow the change-specific checks in the [Validation Matrix](testplan/validation-matrix.md).

## Maintenance Priorities

1. Preserve asynchronous UI and request-ID response routing.
2. Keep stable-toolchain warnings visible through the configured clippy gate.
3. Improve app/GUI owner-seam coverage without weakening the existing threshold.
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

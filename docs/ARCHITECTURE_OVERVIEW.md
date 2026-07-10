# Architecture Overview for New Agents

This document is a short, non-normative orientation to FlistWalker runtime flow and code ownership. Use it to select initial files, then follow [ARCHITECTURE.md](ARCHITECTURE.md) for current module boundaries, [DESIGN.md](DESIGN.md) for normative DES content, or [DETAILED_DESIGN.md](DETAILED_DESIGN.md) for implementation mechanics.

## Product Shape
FlistWalker is a Rust GUI/CLI tool for fast file and folder search. It aims to provide an `fzf --walker`-like experience with FileList-first indexing, walker fallback, fzf-compatible query operators, highlighting, multi-select operations, and Windows-focused release workflows.

Primary docs:
- [REQUIREMENTS.md](REQUIREMENTS.md): product scope, FR/NFR/CON, acceptance criteria.
- [SPEC.md](SPEC.md): normative behavior contracts.
- [DESIGN.md](DESIGN.md): DES-### implementation design and trace.
- [ARCHITECTURE.md](ARCHITECTURE.md): detailed module ownership map.
- [TESTPLAN.md](TESTPLAN.md): validation matrix and TC trace.
- [RELEASE.md](RELEASE.md): release operations.

## Main Runtime Flow
1. `rust/src/main.rs` parses CLI arguments and chooses CLI or GUI mode.
2. GUI startup builds `FlistWalkerApp` in `rust/src/app/mod.rs`, with startup/session wiring delegated to `app/bootstrap.rs` and `app/session.rs`.
3. The app starts an index request. `rust/src/indexer/` chooses FileList reading when available and walker traversal otherwise.
4. Search requests compile the query through `rust/src/query.rs` and evaluate/rank candidates through `rust/src/search/`.
5. GUI frames enqueue work, poll background responses, update result state, and render through owner modules under `rust/src/app/`.
6. Preview, action, sort, kind resolution, FileList creation, and self-update work run through background worker channels and response routing.

## Ownership Map
| Area | First Files To Read | Main Responsibility |
| --- | --- | --- |
| Entrypoint / CLI | `rust/src/main.rs`, `rust/src/lib.rs` | CLI contract, GUI launch, shared module surface |
| Candidate model | `rust/src/entry.rs` | Shared `Entry` / `EntryKind` representation |
| Indexing | `rust/src/indexer/mod.rs`, `rust/src/indexer/filelist_reader.rs`, `rust/src/indexer/filelist_hierarchy.rs`, `rust/src/indexer/walker.rs`, `rust/src/app/index_worker.rs` | FileList detection/streaming, walker collection, incremental GUI ingestion |
| Search/query | `rust/src/query.rs`, `rust/src/search/mod.rs`, `rust/src/search/match_eval.rs`, `rust/src/search/rank.rs` | fzf-like token parsing, filtering, scoring, ranking |
| GUI coordinator | `rust/src/app/mod.rs`, `rust/src/app/state.rs`, `rust/src/app/pipeline.rs`, `rust/src/app/pipeline_owner.rs` | Top-level egui orchestration, app state bundles, index/search lifecycle |
| Rendering/input | `rust/src/app/render.rs`, `rust/src/app/render_panels.rs`, `rust/src/app/render_dialogs.rs`, `rust/src/app/input/mod.rs` | Frame rendering, dialog commands, shortcuts, text input |
| Tabs/session | `rust/src/app/tabs.rs`, `rust/src/app/tab_state.rs`, `rust/src/app/session.rs` | Tab snapshots, background response routing, persistence |
| Workers | `rust/src/app/worker_protocol.rs`, `rust/src/app/worker_bus.rs`, `rust/src/app/worker_tasks.rs`, `rust/src/app/worker_runtime.rs` | Request/response types, worker channels, worker bodies, shutdown |
| Actions / OS integration | `rust/src/actions.rs`, `rust/src/path_utils.rs`, `rust/src/app/shell_support.rs` | Open/execute, path normalization, platform-local shell helpers |
| Runtime config | `rust/src/runtime_config.rs`, `rust/src/app/session.rs`, `rust/src/app/shell_support.rs` | Config file bootstrap, saved state paths, config opening |
| Self-update / release | `rust/src/updater.rs`, `rust/src/updater/`, `rust/src/update_security.rs`, `scripts/prepare-release*`, `.github/workflows/` | Release discovery, signed update validation, staged apply, asset hygiene |

## Invariants To Preserve
- GUI must not run heavy I/O or long computation on the UI thread.
- Background responses must be correlated with request IDs and stale responses must not roll UI state backward.
- FileList detection rules, case priority, and root-only lookup behavior are compatibility-sensitive.
- Query operators (`'`, `!`, `^`, `$`, `|`) must stay consistent across CLI, GUI search, and highlighting.
- `FlistWalkerApp` should remain a coordinator. Feature state transitions belong in owner modules such as `filelist`, `update`, `tabs`, `pipeline`, `result_reducer`, `preview_flow`, and `response_flow`.
- Runtime config public docs must not mention development-only update override environment variables.
- Release asset and OSS notice changes must update release docs and notice packaging together.

## Where To Start By Change Type
- Docs-only change: read [TESTPLAN.md](TESTPLAN.md) and apply VM-001.
- Search behavior: start with [SPEC.md](SPEC.md), `rust/src/query.rs`, `rust/src/search/`, and query-related tests.
- FileList or walker behavior: start with [SPEC.md](SPEC.md), `rust/src/indexer/`, `rust/src/app/index_worker.rs`, and VM-003.
- GUI responsiveness or state routing: start with [ARCHITECTURE.md](ARCHITECTURE.md), `rust/src/app/mod.rs`, `app/pipeline.rs`, `app/response_flow.rs`, and VM-002.
- Runtime config: start with `rust/src/runtime_config.rs`, [docs/spec/operations-release-config.md](spec/operations-release-config.md), and VM-008.
- Release/update work: run the project-local release preflight skill first, then follow [RELEASE.md](RELEASE.md), [OSS_COMPLIANCE.md](OSS_COMPLIANCE.md), and VM-005.

## Test Entry Points
- General Rust validation: `cd rust && cargo test`.
- Indexing path validation: use VM-003, including the ignored perf tests listed in [docs/testplan/validation-matrix.md](testplan/validation-matrix.md).
- GUI structural smoke: use [GUI-TESTPLAN.md](GUI-TESTPLAN.md) and the `scripts/gui-smoke-*` helpers when rendering, focus, tabs, dialogs, or responsiveness change.
- Docs-only validation: review the doc diff, check local Markdown links, and use `rg` to verify IDs and references.

## Deeper References
- [docs/design/architecture-overview.md](design/architecture-overview.md): DES-level architecture entries.
- [docs/detailed-design/architecture-overview.md](detailed-design/architecture-overview.md): detailed design overview.
- [docs/testplan/validation-matrix.md](testplan/validation-matrix.md): validation rules by change type.

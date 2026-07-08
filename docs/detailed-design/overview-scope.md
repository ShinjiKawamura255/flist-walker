<a id="top"></a>

# Detailed Design Overview and Scope

## 1. Overview

This document is the repository-level detailed design for FlistWalker as of 2026-04-21. It complements the normative SDD documents:

- [REQUIREMENTS.md](../REQUIREMENTS.md)
- [SPEC.md](../SPEC.md)
- [DESIGN.md](../DESIGN.md)
- [ARCHITECTURE.md](../ARCHITECTURE.md)
- [TESTPLAN.md](../TESTPLAN.md)

Fact: FlistWalker is a Rust GUI/CLI tool that builds a file/folder candidate set from either `FileList.txt`/`filelist.txt` or a recursive walker, searches it with fzf-like operators, and opens or executes selected paths. The primary implementation lives under [rust/src](../../rust/src), with CLI/GUI entry selection in [main.rs](../../rust/src/main.rs) and shared library modules exported from [lib.rs](../../rust/src/lib.rs).

Rationale: the codebase keeps expensive I/O and search work outside the egui frame loop. GUI state is coordinated by `FlistWalkerApp`, but indexing, search, preview, sorting, actions, FileList writing, kind resolution, and update work are delegated to dedicated workers and owner modules.

Process note: this document was created and revised through the repository plan gate. The latest thickness review explicitly checked the existing SDD contracts and closed the in-scope gaps called out for FileList creation, Window/IME stability, CI/release hygiene, diagnostics, validation, and traceability.

[[↑ Back to Top]](#top)

## 2. Index

### Core Implementation Map

| Area | Primary files | Responsibility |
| --- | --- | --- |
| Entrypoint | [main.rs](../../rust/src/main.rs), [lib.rs](../../rust/src/lib.rs) | Parse CLI args, select CLI/GUI mode, configure tracing/window/icon, expose library modules. |
| Candidate model | [entry.rs](../../rust/src/entry.rs) | Canonical `Entry`, `EntryKind`, and file/dir/link display kind. |
| Indexing | [indexer/mod.rs](../../rust/src/indexer/mod.rs), [indexer/filelist_reader.rs](../../rust/src/indexer/filelist_reader.rs), [indexer/filelist_hierarchy.rs](../../rust/src/indexer/filelist_hierarchy.rs), [indexer/walker.rs](../../rust/src/indexer/walker.rs), [indexer/filelist_writer.rs](../../rust/src/indexer/filelist_writer.rs), [app/index_worker.rs](../../rust/src/app/index_worker.rs) | Index facade, FileList detection/streaming, nested FileList override, walker collection, FileList generation/ancestor propagation, GUI streaming batches. |
| Search | [query.rs](../../rust/src/query.rs), [search/mod.rs](../../rust/src/search/mod.rs), [search/match_eval.rs](../../rust/src/search/match_eval.rs), [search/cache.rs](../../rust/src/search/cache.rs), [search/execute.rs](../../rust/src/search/execute.rs), [search/rank.rs](../../rust/src/search/rank.rs) | fzf-like query parsing, public search facade, regex/plain matching/evaluation, ranking, prefix cache, sequential/parallel execution. |
| Ignore list | [ignore_list.rs](../../rust/src/ignore_list.rs), [query.rs](../../rust/src/query.rs), [app/bootstrap.rs](../../rust/src/app/bootstrap.rs), [app/session.rs](../../rust/src/app/session.rs), [app/ui_state.rs](../../rust/src/app/ui_state.rs), [app/shell_support.rs](../../rust/src/app/shell_support.rs), [app/render.rs](../../rust/src/app/render.rs), [app/render_panels.rs](../../rust/src/app/render_panels.rs), [main.rs](../../rust/src/main.rs) | exe-relative ignore file loading, persisted toggle state, and candidate exclusion. |
| GUI shell | [app/mod.rs](../../rust/src/app/mod.rs), [app/state.rs](../../rust/src/app/state.rs), [app/bootstrap.rs](../../rust/src/app/bootstrap.rs), [app/session.rs](../../rust/src/app/session.rs) | eframe app, shell bundles, startup/restore/persist/shutdown orchestration. |
| GUI flow owners | [app/pipeline.rs](../../rust/src/app/pipeline.rs), [app/pipeline_owner.rs](../../rust/src/app/pipeline_owner.rs), [app/tabs.rs](../../rust/src/app/tabs.rs), [app/response_flow.rs](../../rust/src/app/response_flow.rs), [app/result_reducer.rs](../../rust/src/app/result_reducer.rs) | Index/search polling, active/background tab routing, result state transitions. |
| Rendering/input | [app/render.rs](../../rust/src/app/render.rs), [app/render_panels.rs](../../rust/src/app/render_panels.rs), [app/render_dialogs.rs](../../rust/src/app/render_dialogs.rs), [app/render_tabs.rs](../../rust/src/app/render_tabs.rs), [app/render_snapshot.rs](../../rust/src/app/render_snapshot.rs), [app/render_theme.rs](../../rust/src/app/render_theme.rs), [app/input/mod.rs](../../rust/src/app/input/mod.rs), [app/query_state.rs](../../rust/src/app/query_state.rs) | Render facade/commands, UI panels/dialogs/tabs/snapshots/theme, shortcuts, query/history editing. |
| Shell/support helpers | [app/coordinator.rs](../../rust/src/app/coordinator.rs), [app/root_browser.rs](../../rust/src/app/root_browser.rs), [app/shell_support.rs](../../rust/src/app/shell_support.rs), [app/worker_support.rs](../../rust/src/app/worker_support.rs) | Status/notice helpers, root browser lifecycle, process/window/IME support, shared worker routing helpers. |
| Workers | [app/worker_protocol.rs](../../rust/src/app/worker_protocol.rs), [app/worker_bus.rs](../../rust/src/app/worker_bus.rs), [app/worker_tasks.rs](../../rust/src/app/worker_tasks.rs), [app/workers.rs](../../rust/src/app/workers.rs), [app/worker_runtime.rs](../../rust/src/app/worker_runtime.rs) | Request/response contracts, channel bundle, worker bodies, spawn registry, shutdown joins. |
| OS integration | [actions.rs](../../rust/src/actions.rs), [path_utils.rs](../../rust/src/path_utils.rs), [fs_atomic.rs](../../rust/src/fs_atomic.rs) | Open/execute behavior, path normalization, atomic file writes. |
| Self-update | [updater.rs](../../rust/src/updater.rs), [update_security.rs](../../rust/src/update_security.rs), [app/update.rs](../../rust/src/app/update.rs) | GitHub release candidate selection, signature/checksum verification, update UI flow. |

### Terms

| Term | Meaning |
| --- | --- |
| Candidate | A filesystem path exposed as a searchable `Entry`. |
| FileList source | A candidate source loaded from `FileList.txt` / `filelist.txt`. |
| Walker source | A candidate source collected by recursive filesystem traversal. |
| Active tab | The tab currently projected into `AppRuntimeState`. |
| Background tab | A tab held as `AppTabState`, with responses routed without mutating the active tab state. |
| request_id | Monotonic request identifier used to reject stale worker responses. |
| epoch | Kind resolver generation identifier used to prevent old metadata updates from corrupting current cache state. |

[[↑ Back to Top]](#top)

## 3. Audience and Reading Guide

For AI agents: read sections 5 through 9 before editing code. They define owner boundaries, state bundles, worker protocol contracts, and stale-response rules. Use [TESTPLAN.md](../TESTPLAN.md) validation matrix after choosing files to edit.

For existing developers: section 6 is the module ownership map. Section 12 records trade-offs that should not be accidentally reversed, especially FileList fast path, request routing, and action security.

For new contributors: read sections 1, 5, and 8 first. Then inspect the files linked from the relevant feature row in section 6.

[[↑ Back to Top]](#top)

## 4. Scope

### In Scope

- Rust GUI/CLI architecture.
- FileList and walker indexing.
- Query parsing, search, ranking, highlighting, and result sorting.
- egui app state ownership, worker model, tab routing, preview, action, FileList creation, and self-update flows.
- Error handling, security boundaries, release/runtime operations, and tests.

### Out of Scope

- Python prototype implementation under [prototype/python](../../prototype/python), except as historical context.
- Installer creation and macOS auto-update, both marked out of scope by [REQUIREMENTS.md](../REQUIREMENTS.md).
- Network-drive-specific optimization.
- Any new behavior not already described by existing SDD docs or code.

[[↑ Back to Top]](#top)

# Large Rust File Reduction Plan

## Purpose
FlistWalker の Rust 実装は責務分割が進んでいる一方で、1,000 行級の test / production file がまだ残っている。この計画は、挙動変更を避けながら巨大ファイルを段階的に小さくし、将来の機能追加・回帰修正・レビューを軽くするための実行順を定める。

この文書は巨大 Rust ファイル削減の恒久計画と進捗記録である。

## Status
- Slice A: DONE on 2026-05-14. Oversized app test modules were split by responsibility without production behavior changes.
- Slice B: DONE on 2026-05-14. `ui_model.rs` was converted into a facade plus display/highlight/preview/on-demand owner modules.
- Next recommended slice: Slice C (`app/input.rs`), because it is now the largest remaining production file.

## Current Snapshot
2026-05-14 時点の上位ファイル:

| File | Lines | Primary concern |
| --- | ---: | --- |
| `rust/src/app/tests/app_core.rs` | 1171 | cross-cutting app regression tests |
| `rust/src/app/tests/session_tabs.rs` | 1168 | tab lifecycle, tab routing, drag/reorder, background responses |
| `rust/src/app/tests/index_pipeline/search_filelist.rs` | 1063 | search refresh plus FileList creation lifecycle |
| `rust/src/ui_model.rs` | 1062 | display path, highlight, preview decoding, on-demand file skip |
| `rust/src/app/input.rs` | 1011 | shortcuts, navigation, root dropdown, history, IME dispatch |
| `rust/src/app/tests/shortcuts.rs` | 985 | keyboard and shortcut regression tests |
| `rust/src/indexer/mod.rs` | 913 | public index facade plus many unit tests |
| `rust/src/app/filelist.rs` | 892 | FileList manager, command dispatch, app flow integration |
| `rust/src/search/mod.rs` | 868 | search facade plus contract tests |
| `rust/src/runtime_config.rs` | 854 | config model, paths, env seed, migration, tests |
| `rust/src/app/index_worker.rs` | 830 | FileList stream, walker stream, kind classification, worker loop |
| `rust/src/app/session.rs` | 818 | UI state, saved roots/tabs, geometry, session persistence |

After Slice A on 2026-05-14, the largest app test modules are:

| File | Lines | Notes |
| --- | ---: | --- |
| `rust/src/app/tests/render_tests.rs` | 504 | existing render regression owner |
| `rust/src/app/tests/index_pipeline/dialogs_and_inflight.rs` | 485 | existing dialog/inflight owner |
| `rust/src/app/tests/session_restore.rs` | 454 | existing session restore owner |
| `rust/src/app/tests/index_pipeline/filelist_lifecycle.rs` | 444 | existing FileList lifecycle owner |
| `rust/src/app/tests/update_commands.rs` | 441 | existing update command owner |

## Non-Goals
- Do not redesign app architecture in one pass.
- Do not rename public APIs unless a slice explicitly proves the compatibility impact.
- Do not move tests only to reduce line count if the new location hides ownership.
- Do not change search, indexing, update, or GUI behavior as part of mechanical moves.

## Success Criteria
- No top-level Rust production file remains above 800 lines after the first implementation wave.
- No app test module remains above 800 lines after the first implementation wave.
- Each moved responsibility has an owner module and a matching owner test location.
- `cargo test --locked`, `cargo clippy --all-targets -- -D warnings`, and `cargo llvm-cov --locked --workspace --lcov --output-path target/llvm-cov/lcov.info --fail-under-lines 75` stay green.
- For indexer / index worker changes, VM-003 perf guards remain green.
- For render/input/session GUI-adjacent changes, `docs/GUI-TESTPLAN.md` evidence rules are followed.

## Execution Order

### Slice A: Split Oversized App Tests First
Status: DONE on 2026-05-14.

Goal: reduce review friction without changing production behavior.

Targets:
- `rust/src/app/tests/app_core.rs`
- `rust/src/app/tests/session_tabs.rs`
- `rust/src/app/tests/index_pipeline/search_filelist.rs`
- `rust/src/app/tests/shortcuts.rs`

Proposed moves:
- Move action/open/copy/shutdown/cache tests out of `app_core.rs` into owner-aligned modules such as `action_commands.rs`, `cache_tests.rs`, and `shutdown.rs`.
- Split `session_tabs.rs` into `tab_lifecycle.rs`, `tab_drag.rs`, `tab_background_responses.rs`, and `tab_contract.rs`.
- Split `index_pipeline/search_filelist.rs` into `search_refresh.rs`, `filelist_creation.rs`, `filelist_root_cleanup.rs`, and `filelist_background.rs`.
- Split `shortcuts.rs` into `shortcut_navigation.rs`, `shortcut_root_history.rs`, `shortcut_action.rs`, and `shortcut_pin_focus.rs`.

Validation:
- `cargo test --locked`
- `cargo clippy --all-targets -- -D warnings`
- `cargo llvm-cov --locked --workspace --lcov --output-path target/llvm-cov/lcov.info --fail-under-lines 75`

Rollback:
- Revert moved test modules as one unit. No production behavior should be affected.

Completed moves:
- `app_core.rs` now keeps core startup/UI state tests; action, cache, tab-result-cache, and shutdown tests moved to owner modules.
- `session_tabs.rs` was replaced by `tab_lifecycle.rs`, `tab_drag.rs`, `tab_background_responses.rs`, and `tab_contract.rs`.
- `index_pipeline/search_filelist.rs` was replaced by `search_refresh.rs`, `filelist_creation.rs`, `filelist_root_cleanup.rs`, and `filelist_background.rs`.
- `shortcuts.rs` was replaced by `shortcut_navigation.rs`, `shortcut_root_history.rs`, `shortcut_action.rs`, and `shortcut_pin_focus.rs`.

### Slice B: Split `ui_model.rs` by Display / Highlight / Preview
Status: DONE on 2026-05-14.

Goal: separate pure display/highlight logic from file preview and platform skip logic.

Proposed modules:
- `rust/src/ui_model/display.rs`
- `rust/src/ui_model/highlight.rs`
- `rust/src/ui_model/preview.rs`
- `rust/src/ui_model/on_demand.rs`
- `rust/src/ui_model/mod.rs` as the public facade

Rules:
- Keep existing public functions available from `crate::ui_model::*`.
- Move tests with the responsibility they protect.
- Keep action policy out of `ui_model`; `actions.rs` remains the owner.

Validation:
- `cargo test --locked ui_model`
- `cargo test --locked`
- `cargo clippy --all-targets -- -D warnings`

Completed moves:
- `ui_model/mod.rs` is now the public facade and re-exports the existing API.
- `display.rs` owns display path normalization facade tests.
- `highlight.rs` owns match positions and visible-match tests.
- `preview.rs` owns preview text, decoding, directory preview, and preview tests.
- `on_demand.rs` owns placeholder/on-demand skip detection and attribute tests.

### Slice C: Split Input Dispatch by Interaction Family
Goal: make shortcut and IME changes easier to review.

Proposed modules:
- `rust/src/app/input/navigation.rs`
- `rust/src/app/input/root_dropdown.rs`
- `rust/src/app/input/history.rs`
- `rust/src/app/input/actions.rs`
- `rust/src/app/input/ime.rs`
- `rust/src/app/input/mod.rs` as the dispatch facade

Rules:
- Preserve existing `FlistWalkerApp` method names where tests call them directly, or add thin facade methods first.
- Move one interaction family at a time and run targeted shortcut tests after each move.

Validation:
- `cargo test --locked shortcuts`
- `cargo test --locked window_ime`
- `cargo test --locked`
- GUI smoke evidence if focus, dialog, or text input behavior changes beyond mechanical moves.

### Slice D: Split Indexer/Search Facade Test Bulk
Goal: keep `indexer/mod.rs` and `search/mod.rs` as public facades rather than mixed implementation/test containers.

Proposed moves:
- Move large `indexer::tests` groups into `indexer/tests/filelist_detection.rs`, `indexer/tests/filelist_parse.rs`, `indexer/tests/filelist_write.rs`, `indexer/tests/perf.rs`, and `indexer/tests/hierarchy.rs`.
- Move large `search::tests` groups into `search/tests/query_contract.rs`, `search/tests/ranking.rs`, `search/tests/regex.rs`, `search/tests/cache_parallel.rs`, and `search/tests/perf.rs`.

Rules:
- Keep ignored perf tests discoverable by their current names.
- Keep public facade exports stable.

Validation:
- `cargo test --locked indexer::`
- `cargo test --locked search::`
- VM-003 perf guards for indexer-related movement.

### Slice E: Split FileList / Session / Runtime Config Owners
Goal: reduce production file size in stateful modules after lower-risk test and facade work is complete.

Candidates:
- `app/filelist.rs`
  - `filelist/manager.rs`
  - `filelist/commands.rs`
  - `filelist/dialog_flow.rs`
  - `filelist/apply_response.rs`
- `app/session.rs`
  - `session/state_files.rs`
  - `session/saved_roots.rs`
  - `session/tabs_restore.rs`
  - `session/window_geometry.rs`
- `runtime_config.rs`
  - `runtime_config/model.rs`
  - `runtime_config/env_seed.rs`
  - `runtime_config/paths.rs`
  - `runtime_config/migration.rs`

Validation:
- `cargo test --locked session`
- `cargo test --locked runtime_config`
- `cargo test --locked`
- `cargo clippy --all-targets -- -D warnings`

### Slice F: Split Index Worker and Render Panels
Goal: touch high-risk GUI/indexing production files only after test modules and owner boundaries are easier to navigate.

Candidates:
- `app/index_worker.rs`
  - `index_worker/filelist_stream.rs`
  - `index_worker/walker_stream.rs`
  - `index_worker/kind.rs`
  - `index_worker/trace.rs`
  - `index_worker/mod.rs`
- `app/render_panels.rs`
  - `render_top_panel.rs`
  - `render_status_panel.rs`
  - `render_results.rs`
  - `render_result_row.rs`
  - `render_preview.rs`

Validation:
- VM-003 perf guards for index worker.
- `cargo test --locked render_tests`
- GUI smoke evidence for render changes.

## Stop Conditions
- Any slice requires broad public API rename outside its owner area.
- Coverage drops below 75%.
- VM-003 perf guards regress.
- GUI-adjacent changes cannot produce required evidence for a release branch.
- A moved test becomes harder to map to the owner module than before.

## Recommended First PR / Commit Boundary
Start with Slice A only. It has the best risk-to-value ratio because it lowers review friction without touching production behavior. Do not combine Slice A with `ui_model.rs` or `input.rs` moves in the same commit.

## Tracking
- Add a short `docs/TASKS.md` entry when a slice starts and when it closes.
- Update `docs/ARCHITECTURE.md` only when production module ownership changes.
- Update `docs/TESTPLAN.md` only when validation ownership or command requirements change.

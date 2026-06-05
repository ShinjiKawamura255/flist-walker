<a id="top"></a>

# Module Detailed Design

## 6. Module Detailed Design

### 6.1 Entrypoint and CLI Adapter

Responsibility: [main.rs](../../rust/src/main.rs) owns `Args`, tracing setup, signal handler registration, root canonicalization, CLI execution, GUI startup, Windows DPI setup, and app icon loading.

Public interface:

- `flistwalker [query] [--root PATH] [--limit N] [--cli]`
- `run_cli(args)` builds an index and prints candidates or scored results.
- `run_gui(args)` creates `eframe::NativeOptions` and launches `FlistWalkerApp::from_launch`.

Inputs and outputs:

- Input: CLI args, process environment, filesystem root.
- Output: stdout/stderr in CLI mode, eframe native window in GUI mode.

Failure modes:

- Root canonicalization failure or non-directory root returns an `anyhow` error.
- GUI startup maps eframe errors into `anyhow`.
- CLI action does not execute selected results; it prints index/search output only.

### 6.2 Candidate and Entry Model

Responsibility: [entry.rs](../../rust/src/entry.rs) defines `Entry { path, kind }`, `EntryKind`, and `EntryDisplayKind`.

Important constraints:

- `Entry.kind` is optional because FileList and walker fast paths may initially avoid metadata calls.
- `Entry::is_visible_for_flags(include_files, include_dirs)` treats unknown kind as visible only when both files and dirs are enabled.
- `EntryKind::link(is_dir)` records both display type and directory behavior.

Rationale: separating path identity from kind resolution allows large FileList/walker streams to reach the UI quickly while slower metadata refinement happens later.

### 6.3 Indexing Domain

Responsibility: [indexer/mod.rs](../../rust/src/indexer/mod.rs) exposes synchronous index APIs and coordinates FileList-vs-walker selection. Nested FileList hierarchy override ownership lives in [indexer/filelist_hierarchy.rs](../../rust/src/indexer/filelist_hierarchy.rs).

Public interfaces:

- `build_index_with_metadata(root, use_filelist, include_files, include_dirs) -> IndexBuildResult`
- `build_index(...) -> Vec<PathBuf>`
- Re-exported FileList functions such as `find_filelist`, `parse_filelist_stream`, and `write_filelist_cancellable`.

Internal logic:

- If both include flags are false, return `IndexSource::None`.
- If FileList mode is enabled and a first-level FileList exists, parse FileList hierarchy.
- Otherwise fall back to walker.
- Nested FileList override only considers FileList entries already present in the loaded candidate set.

Failure modes:

- FileList read/parse errors propagate as `anyhow::Result`.
- Nested FileList supersede is represented as an error string in GUI worker paths and as an error in synchronous paths.

### 6.4 GUI Index Worker

Responsibility: [app/index_worker.rs](../../rust/src/app/index_worker.rs) adapts indexing to the GUI streaming model.

Important behaviors:

- Sends `IndexResponse::Started` before streaming candidates.
- Emits `Batch` responses to keep the UI incremental.
- Emits `ReplaceAll` when nested FileList overrides require replacing a subtree.
- Uses `latest_request_ids` by tab to cancel superseded index work.
- Uses walker `file_type` for fast file/dir classification and defers symlink/shortcut metadata when possible.
- Uses adaptive as the only walker backend. The old jwalk fallback and `developer.walker_backend` runtime config switch are removed.
- Adaptive walker can separately configure its initial and maximum concurrent read-dir limits via manual `developer.walker_adaptive_initial_limit` and `developer.walker_adaptive_max_limit` values. When omitted, the maximum uses half of the logical core count, rounded up with a minimum of 1 and a default cap of 8, and the initial limit uses half of that maximum, rounded up. Legacy `walker_threads` config values are removed during config load and do not clamp the adaptive maximum.
- Adaptive walker limit control keeps the previous probe direction. A successful or non-regressing increase keeps probing upward, a successful or non-regressing decrease keeps probing downward, and a regressing sample reverses direction.
- Adaptive limit control samples a small fixed batch of completed `read_dir` calls and compares the batch's throughput against the previous batch. The first probe only moves when the change crosses a small stability band; after a direction is established, non-regressing samples keep probing in that direction and regressing samples reverse it.
- Walker metrics summary includes `adaptive_limit_avg` and `adaptive_limit_change_count` so post-run analysis can compare average effective parallelism with the final limit. `adaptive_limit_avg` is a time-weighted average of the effective limit and may include a small shutdown/join tail, so it should be interpreted as a post-run summary value rather than a strict steady-state measurement.
- When the adaptive maximum is 1, the adaptive backend uses a serial fast path rather than the channel / condvar / worker-pool path.
- `developer.walker_adaptive_initial_limit` and `developer.walker_adaptive_max_limit` are developer-only tuning knobs. Do not expand these fields as public configuration.
- Adaptive walker skips Windows compatibility junctions that combine Hidden, System, and ReparsePoint attributes, and does not recurse through other reparse-point directories. This keeps Explorer-hidden legacy folders such as Documents/My Music out of adaptive Walker results.
- When manually added `developer.walker_metrics = true` is present, emits one bounded walker metrics summary at the indexing request terminal point. The metrics path intentionally avoids per-entry and per-directory logs. If `developer.walker_metrics_log_path` is also set, the same summary is appended to that file so release GUI builds can be measured without stderr capture.
- Caps walker results with `WALKER_MAX_ENTRIES_DEFAULT` and reports `Truncated`.

Rationale: GUI indexing is latency-sensitive. Streaming batches and request supersede prevent stale or long-running indexing from blocking user interaction.

### 6.5 Search Domain

Responsibility: [query.rs](../../rust/src/query.rs) parses user query syntax, [search/mod.rs](../../rust/src/search/mod.rs) exposes the public search facade, and [search/match_eval.rs](../../rust/src/search/match_eval.rs) compiles/evaluates private query matchers.

Public APIs include:

- `search_entries_with_scope(query, entries, limit, use_regex, ignore_case, root, prefer_relative)`
- `rank_search_results(entries, query, root, limit, use_regex, ignore_case, prefer_relative, prefix_cache)`

Design facts:

- Plain include tokens remain fuzzy/literal matchers even when regex mode is enabled.
- Regex compilation happens once per query for tokens that use regex syntax.
- Exact, exclude, anchored, OR alternative, literal bonus, and exact bonus terms are compiled into `CompiledQuery`.
- The execution path chooses sequential or parallel collection based on candidate count and environment-tuned thresholds.
- Prefix cache is used when a new query extends the previous query over the same snapshot.
- Ignore list filtering is a separate global candidate filter sourced from the executable directory; it uses the same `!`-style exclusion comparison as query exclude tokens, but is toggled via a persisted GUI checkbox before search dispatch and empty-query result rendering.

Failure modes:

- Invalid regex returns an error string to the GUI search response.
- Empty query and `limit == 0` are handled as non-error boundary cases.

### 6.6 GUI Shell and State Bundles

Responsibility: [app/mod.rs](../../rust/src/app/mod.rs) defines `FlistWalkerApp { shell: AppShellState }`. [app/state.rs](../../rust/src/app/state.rs) defines the major bundles.

The shell owns:

- `AppRuntimeState`: active root, query, filters, index snapshot, active results, selection, preview, notice, status.
- `SearchCoordinator`: active/background search request lifecycle.
- `IndexCoordinator`: index request lifecycle, inflight tracking, incremental state.
- `WorkerBus`: channels for non-index workers.
- `RuntimeUiState`: focus, scrolling, preview panel, drag state, UI flags.
- `CacheStateBundle`: preview, highlight, entry kind, and sort metadata caches.
- `TabSessionState`: persisted/background tabs and request-tab routing maps.
- `FeatureStateBundle`: root browser, FileList manager, update manager.
- `WorkerRuntime`: shutdown signal and join handles.

Rationale: the shell makes ownership explicit. Active-tab live state is separate from persisted/background tab snapshots, which prevents background worker responses from overwriting the visible tab.

### 6.7 Tab and Session Design

Responsibility: [app/tab_state.rs](../../rust/src/app/tab_state.rs), [app/tabs.rs](../../rust/src/app/tabs.rs), and [app/session.rs](../../rust/src/app/session.rs) own tab snapshots, tab lifecycle, and persisted UI state.

Persisted state includes:

- Last/default root.
- Preview visibility and panel sizes.
- Shared query history.
- Saved tabs and active tab index.
- Window geometry.
- Update skip/failure dialog preferences.

Rules:

- Runtime config `restore_tabs_enabled=true` gates tab restore.
- Runtime config `history_persist_disabled=true` disables query history load/save.
- Restored background tabs are refreshed lazily.
- Request routing maps bind preview/action/sort request IDs to tab IDs and are cleared when tabs close.
- Tab drag/reorder preserves active tab identity by tab ID, not by stale vector index.
- Tab accent is part of saved tab state and is rendered differently for active full-fill and inactive accent decoration.
- Background tabs may compact display caches, but retain enough base result/index state to restore without unnecessary reindexing.
- Closing a tab clears request routing for preview/action/sort so late worker responses cannot target a removed tab.

### 6.8 Rendering and Input

Responsibility: rendering modules collect UI intent and produce commands; owner modules apply state transitions.

Key design:

- [app/render.rs](../../rust/src/app/render.rs) owns the `run_ui_frame()` facade and `RenderCommand` dispatcher.
- [app/render_panels.rs](../../rust/src/app/render_panels.rs), [app/render_dialogs.rs](../../rust/src/app/render_dialogs.rs), [app/render_tabs.rs](../../rust/src/app/render_tabs.rs), [app/render_snapshot.rs](../../rust/src/app/render_snapshot.rs), and [app/render_theme.rs](../../rust/src/app/render_theme.rs) draw panels, dialogs, tabs, snapshots, theme colors, and result lists.
- `RenderCommand` boundaries prevent immediate complex state mutation from inside UI painting code.
- [app/input.rs](../../rust/src/app/input/mod.rs), [app/input_history.rs](../../rust/src/app/input_history.rs), and [app/query_state.rs](../../rust/src/app/query_state.rs) handle shortcuts, IME fallback, query editing, and shared history.

Rationale: egui UI code is easier to regress when it directly mutates cross-feature state. Command dispatch after drawing keeps rendering and behavior boundaries clearer.

### 6.9 Shell Support, Root Browser, and Window/IME Stability

Responsibility: [app/shell_support.rs](../../rust/src/app/shell_support.rs) owns process shutdown, egui font setup, window trace helpers, and shell-local support policy. [app/root_browser.rs](../../rust/src/app/root_browser.rs) owns root selector state and root change cleanup. [app/coordinator.rs](../../rust/src/app/coordinator.rs) owns status/notice helpers and root/path comparison helpers. [app/worker_support.rs](../../rust/src/app/worker_support.rs) keeps reusable worker routing and action target helpers out of worker spawn code.

Window and input stability rules:

- Windows configures System DPI awareness before native window creation to reduce monitor-crossing resize jitter.
- Saved window geometry is clamped against monitor dimensions before persistence and again during startup restore when monitor data exists.
- `FLISTWALKER_WINDOW_TRACE=1` is the opt-in GUI/session/input/update diagnostic channel and stays separate from worker-side `tracing`.
- IME fallback handles `CompositionEnd` text and space insertion gaps without forcing insertion at the query end; fallback text is inserted at the current cursor position.
- Root changes clear old-root current row, pinned paths, preview, pending FileList confirmations, pending use-walker confirmation, and deferred-after-index state.
- Root containment checks are kept out of FileList/walker indexing and are enforced at action dispatch.

Rationale: these helpers are intentionally not embedded in rendering or worker bodies. Keeping them as shell support boundaries prevents the app coordinator from regrowing broad platform and diagnostics responsibilities.

### 6.10 Worker Protocol and Runtime

Responsibility: [app/worker_protocol.rs](../../rust/src/app/worker_protocol.rs) centralizes request/response structs. [app/worker_bus.rs](../../rust/src/app/worker_bus.rs) groups channels. [app/worker_tasks.rs](../../rust/src/app/worker_tasks.rs) implements worker bodies.

Worker families:

- Search: latest queued request wins; uses `SearchPrefixCache`.
- Preview: latest queued request wins; builds preview text.
- Kind resolver: resolves delayed `EntryKind` and updates cache by epoch.
- FileList writer: writes `FileList.txt`, supports cancellation.
- Action: opens/executes selected paths.
- Sort metadata: resolves modified/created timestamps for current results.
- Update: checks/releases and stages update.

Failure mode handling:

- Most workers return response variants containing `error` or `notice` text.
- Receiver closure is traced and terminates the worker loop.
- Shutdown uses a shared atomic flag plus bounded join in [app/worker_runtime.rs](../../rust/src/app/worker_runtime.rs).

### 6.11 FileList Creation Lifecycle

Responsibility: [app/filelist.rs](../../rust/src/app/filelist/mod.rs), `FileListManager` in [app/state.rs](../../rust/src/app/state.rs), [app/worker_protocol.rs](../../rust/src/app/worker_protocol.rs), [app/worker_tasks.rs](../../rust/src/app/worker_tasks.rs), and [indexer/filelist_writer.rs](../../rust/src/indexer/filelist_writer.rs) own the Create File List workflow.

Lifecycle rules:

- The app separates overwrite confirmation, ancestor propagation confirmation, use-walker confirmation, deferred-after-index, in-flight request, and cancel state in `FileListWorkflowState`.
- FileList source tabs do not create a new tab for Create File List. They temporarily run Walker indexing for the same tab, create the FileList from that Walker snapshot, then restore normal FileList indexing for that tab.
- The worker request carries `request_id`, `tab_id`, requested `root`, candidate entries, propagation choice, and a shared cancel flag.
- Responses are correlated by `request_id` and requested root. A stale requested-root completion performs cleanup only and does not restore `use_filelist`, reindex the wrong tab, or update the visible notice as if it were current.
- Cancellation sets the cancel flag and expects a `Canceled` response; final root replacement and ancestor propagation must not start if cancellation has already been observed at the boundary.
- FileList writing uses an atomic/temporary write path where possible. Cross-device final placement falls back to copy so only the final destination is replaced.
- Ancestor propagation appends a child FileList reference without duplicates, restores the parent FileList mtime after append, and treats ancestor update failures as non-fatal to the root FileList creation result.
- Root changes destroy old-root pending confirmations and deferred FileList state to prevent accidental overwrite or propagation against a previous root.

Rationale: Create File List crosses UI confirmation, indexing, file writing, and tab routing. Keeping it in a manager and command boundary avoids direct `FlistWalkerApp` mutation and makes stale/cancel behavior testable.

### 6.12 Actions and OS Integration

Responsibility: [actions.rs](../../rust/src/actions.rs) chooses `Open` or `Execute` and calls platform-specific mechanisms.

Important rules:

- Direct execution avoids shell expansion by using `Command::new(path)`.
- Windows open uses `ShellExecuteW` with wide strings and normalized shell path.
- macOS uses `open`; Linux/Unix uses `xdg-open`.
- Windows `.ps1` is treated as open, not execute.
- GUI root containment checks happen immediately before action dispatch, not during indexing.

### 6.13 Self-update

Responsibility: [updater.rs](../../rust/src/updater.rs), [updater/](../../rust/src/updater/), [update_security.rs](../../rust/src/update_security.rs), [app/update.rs](../../rust/src/app/update.rs), and update worker code handle update discovery and application.

Flow:

- Fetch GitHub latest release metadata.
- Compare semver target with current version.
- Select platform asset, README, LICENSE, THIRD_PARTY_NOTICES, `SHA256SUMS`, and `SHA256SUMS.sig`.
- Verify detached signature before trusting checksums.
- Verify staged asset checksum.
- Build a private verified bundle only after signature/checksum verification and pass that bundle to the platform apply helper.
- Windows/Linux can auto-apply; macOS remains manual-only.

Operational guardrails:

- `FLISTWALKER_DISABLE_SELF_UPDATE` env var or sentinel file disables self-update.
- Builds without embedded update public key degrade to manual-only.
- Startup update failures are shown without blocking normal GUI work.

[[↑ Back to Top]](#top)

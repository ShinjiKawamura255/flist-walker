# EXECUTION PLAN: Slice A - contracts and shared core

## Control
- Date: 2026-07-26
- Owner: main agent; Terra implementation delegates
- Target Project: FlistWalker
- Plan Role: slice plan
- Execution Profile: strict
- Risk Tier: medium
- Required Safety Controls: recording backends for action tests; temporary roots for persistence/FileList tests; latest-read/merge lock tests; precomputed FileList write-plan and rollback evidence.
- Planning Depth: roadmap+slice
- Review Pattern: staged-subagents
- Review Requiredness: required-before-implementation-and-final
- Review Placement: before-and-final
- Execution Mode: none
- Execution Mode Policy: inherit roadmap
- Plan Readiness: ready-for-implementation
- Parent Plan: `docs/EXECUTION-PLAN-20260726-roadmap-cli-tui-parity.md`
- Child Plan(s): none
- Work Item Manifest: `docs/EXECUTION-WORK-ITEMS-20260726-cli-tui-parity.json`
- Scope Label: shared CLI/TUI contracts
- Related Docs/Tickets: FR-006, FR-010, FR-011, FR-025, FR-026; SP-001, SP-004, SP-006, SP-010, SP-013, SP-015, SP-016; DES-005, DES-006, DES-016, DES-017

### Temporary Ownership
- AGENTS.md Initial State: existing
- Temporary AGENTS.md Ownership: section-only
- Docs Directory Initial State: existing
- Temporary Docs Directory Ownership: existing-directory

## Goal
- Outcome: Durable SDD and failing tests define accepted CLI/TUI behavior, and reusable GUI-independent boundaries exist for later slices.
- Success Conditions: Contract tests cover flag/key semantics and shared APIs compile without GUI behavior regression.
- Closure Slice: not applicable

## Scope
### In
- SDD-first contract additions.
- Shared result-sort model/execution boundary, persisted root/history read boundary, authorized action request boundary, and FileList operation option model.
- Characterization tests for existing GUI behavior before extraction.

### Out
- User-facing batch/TUI implementation beyond minimal test seams.

## Constraints And Assumptions
- New shared modules MUST NOT depend on egui/crossterm.
- GUI adapters retain current labels, defaults, and request freshness.
- Session privacy setting applies equally when TUI history is enabled.

## Normative Contract Matrices

### Batch CLI

| Concern | Contract |
| --- | --- |
| Sort | `--sort MODE`, default `score`; modes are `score`, `name-asc`, `name-desc`, `modified-desc`, `modified-asc`, `created-desc`, `created-asc`, `size-desc`, `size-asc`; the full match set is sorted before `limit`; `limit=0` yields no targets. |
| Action | `--action print|open|reveal`, default `print`; `--action-all` is valid only for `open`/`reveal`; non-print action with more than one post-sort/post-limit target and no `--action-all` refuses before the first backend call. |
| Action output | `print` preserves existing path-only stdout framing; non-print actions write no result paths to stdout; progress/diagnostics/partial summaries use stderr. `--absolute`/`--print0` with non-print action are argument errors. |
| Action exit | Argument/combination error: 2; no match: existing 0 or 1 with `--fail-no-match`; authorization/executor/partial failure: 1; cancellation: 130; complete action success: 0. A preflight authorization failure makes zero backend calls. |
| Action target | Batch default target set is the single post-sort/post-limit result; `--action-all` uses the entire post-sort/post-limit result set. TUI action target is always current row only, regardless of pins. |
| Root selection | Exactly one of `--root PATH`, `--use-default-root`, `--saved-root INDEX` may be used; saved-root index is one-based in `--list-saved-roots` order; no root selector keeps current-directory behavior. Invalid/missing default/index is exit 2 before indexing. |
| Root listing | `--list-saved-roots` is an exclusive batch operation, supports newline or `--print0`, emits one-based index plus absolute stored path in human mode and path-only records when `--print0` is used, and performs no indexing/action/write. |
| FileList operation | `--create-filelist` is exclusive with query search/action/listing and rejects non-default search/filter/sort options. `--overwrite-filelist` and `--propagate-ancestors` require it. Root selectors remain valid. |
| FileList naming | Reuse the existing root FileList name selected by current detection precedence; create `FileList.txt` only when none exists; a fixture with both canonical names or case variants must produce a deterministic plan and must not overwrite the lower-priority file implicitly. |
| FileList consent | Existing root target without `--overwrite-filelist` refuses with zero writes. Ancestors are root-only by default; `--propagate-ancestors` authorizes only the precomputed ancestor target set. |
| FileList result | Build all contents and target metadata before commit; check cancellation immediately before every replacement; any I/O error is failure. On partial failure/cancel, attempt restoration of every committed target. Success is exit 0. Clean cancellation before commit or after complete rollback is exit 130. Any write/read/rollback failure is exit 1 even when cancellation triggered the rollback. All committed/failed/rolled-back/rollback-failed display paths go to stderr; stdout remains empty. Crash-level cross-file atomicity is not claimed. |

### TUI key and state precedence

| State | Enter | Esc / Ctrl+G | Ctrl-C | Other keys |
| --- | --- | --- | --- | --- |
| Normal | Restore terminal and output current/pins | Esc exits 130; `Ctrl+G` clears query and pins without exit | Cancel workers, restore, exit 130 | `Ctrl+O` open/execute current only; `Shift+Enter` reveal current only; `Tab` pin; `Ctrl+R` history; `Alt+P` preview; `F1` help; `F2` options; `F3` sort; `F4` roots; `F5` refresh; `F6` Create FileList |
| History | Apply highlighted history query and return to normal | Cancel history and restore draft | Exit entire TUI 130 | Up/Down/Page keys navigate; text edits history filter |
| Help | Close help | Close help | Exit entire TUI 130 | Navigation may scroll help; all side-effect keys disabled |
| Options / Sort / Root picker | Apply highlighted choice; root apply clears old selection and starts new index | Cancel overlay, preserve prior state | Exit entire TUI 130 | Up/Down/Page keys navigate; unrelated side-effect keys disabled |
| FileList confirmation | Confirm highlighted root-only/propagate choice, subject to overwrite confirmation | Cancel FileList request, stay in TUI | Cancel request and exit entire TUI 130 | No action/root/refresh dispatch while confirmation is active |
| Non-FileList worker busy | Normal navigation/query remains available; Enter still outputs current selection | Same as normal; exit cancels outstanding work | Cancel and exit 130 | New refresh/root/source request supersedes old request by identity; stale responses cannot mutate state |
| FileList active | Record `SelectOutput` intent, request cancel, settle first; do not output yet | Record sticky `CancelExit` intent, request cancel, settle first | Record sticky `CancelExit` intent, request cancel, settle first | Root choice records `SwitchRoot(path)` intent; other side-effect dispatch is disabled; navigation/help remain responsive |

- Preview is enabled by default when terminal width is at least 100 columns, collapses below that width, and can be toggled with `Alt+P`; preview I/O is worker-only.
- An external action request carries trusted root, current-row selection snapshot, request identity, and cancellation token. It performs whole-request preauthorization, then freshness/cancel check and reauthorization immediately before the single backend call. After root switch/exit cancellation is observed, no new backend call may start; an already-started OS action is irreversible.
- FileList/root/source transitions use the same request-identity rule and clear root-scoped current/pinned/preview state before accepting new results.
- `FileList active` spans accepted dispatch through terminal report synthesis and worker join. `Enter`, `Esc`, `Ctrl-C`, and root switch become one pending intent: request cancellation, keep the terminal event loop responsive in `Canceling/Settling FileList...`, and do not return/output/switch root until settlement. Priority is sticky `CancelExit` > `SwitchRoot(path)` > `SelectOutput`; `CancelExit` cannot be replaced, a later root choice replaces an earlier root choice, `SwitchRoot` replaces `SelectOutput`, and `SelectOutput` is accepted only when no higher intent exists. The generic 250ms detach path MUST NOT apply to FileList workers.
- FileList transaction execution is panic-contained. A panic attempts rollback from the worker-owned transaction report before returning failure. Channel disconnect or missing terminal response triggers worker join and synthesis of a failed settlement; panic/disconnect never resumes selection/root intents as success. A settled rollback/report failure keeps selection/root intents inside TUI for explicit recovery, while sticky `CancelExit` restores the terminal and exits 1 with the recovery report.
- Settlement success resumes the pending intent. Rollback failure is shown with recovery paths; selection/root-switch intents remain in TUI for explicit retry/exit, while cancel-exit restores the terminal and exits 1 rather than claiming clean cancellation. A delayed worker over 250ms, cancel after one commit, rollback failure, and zero writes after `run_cli_tui` return are mandatory tests. Force-kill/crash atomicity remains outside the guarantee.

### Persistence contract

- Shared persistence exposes read-only default/saved-root access separately from query-history mutation.
- History load/save is a no-op when `history_persist_disabled` is true, including no query/history diagnostic text.
- Callers submit ordered, trimmed, nonempty history deltas rather than full snapshots. Under the cross-process sidecar lock, the worker rereads the latest JSON and applies each delta by removing any exact duplicate, appending it as most recent, and trimming the front to 100 entries. Two serialized writers adding different queries therefore preserve both in commit order.
- GUI/TUI frame code only enqueues `UiStatePatch + history_delta`. Coalescing is lossless: non-history patch leaves merge last-write-wins per JSON leaf, while every history delta is concatenated in enqueue order and is never replaced by a later request. Patch application updates only named leaves, so unknown top-level fields and unknown fields nested inside known containers remain unchanged.
- The persistence worker assigns enqueue generations. Lock timeout/write failure retains the exact coalesced patch and ordered deltas for retry. A successful write clears only generations included in that commit; requests arriving during the write remain queued for the next commit. The worker performs bounded lock wait, latest-read merge, global history dedupe/recency/cap, and atomic write.
- Lock contention/timeouts never block frame dispatch. GUI retains dirty state and retries; TUI reports a stderr warning while preserving selection stdout/exit semantics. Graceful application shutdown requests a bounded flush outside frame rendering; crash-before-flush history loss is an explicitly documented residual risk.
- All UI-state writers migrate to the shared worker/lock before TUI history writing is enabled. Tests cover alternating two-process writers; lock-held same-process burst `A,B,A` producing recency order `B,A`; multiple patch leaves; top-level and nested unknown fields; cap 100; lock timeout retaining deltas across retry; generations arriving during commit; and frame dispatch latency independent of lock wait duration.

## Execution
1. Update SDD with the matrices above and add failing contract/unit tests for all accepted behavior.
2. Extract shared all-match sort from existing search/result implementation and characterize score/tie/metadata-None/folder-size/cancel behavior.
3. Extract asynchronous coalescing persistence worker plus locked latest-read/delta-merge APIs, and migrate existing UI-state writers without field loss or frame blocking.
4. Extract authorized action request and FileList write-plan/report/rollback boundaries.
5. Rewire GUI only where necessary and run GUI-focused regression tests.

## Validation
- Automated: focused shared-module tests, GUI sort/action/history/session tests, `cargo test --test cli_contract`, `cargo test cli_tui::tests`.
- Manual: none.
- Regression Focus: GUI behavior and public query/search APIs.
- Performance/Security: action tests use recording backend; metadata behavior remains bounded.

## Trace
- SDD Impact: required
- Related FR/NFR/CON: FR-006, FR-010, FR-011, FR-025, FR-026, NFR-006, NFR-011
- Related SP/DES/TC: SP-001/004/006/010/013/015/016, DES-005/006/016/017, TC-006A/050/052/054/057B/110/111/115/162
- Docs To Update Before Closure: all SDD topic and mapping files touched by the contract.

## Review
- Review Viewpoints: responsibility-boundary, ordering, validation, rollback, security
- Checkpoint(s): before-and-final; statuses inherited/recorded in roadmap checkpoint

## Temporary `AGENTS.md` Rule
- Plan Paths: roadmap then this slice.
- Overrides: none.

## Progress Log
- 2026-07-26 Planned.
- 2026-07-26 Main-agent preflight completed; implementation awaits workflow-level focused re-review.
- 2026-07-26 `slice-a.contracts` completed by Terra and integrated by Sol; SDD/AC/root excerpts/validation matrix synchronized, CLI contract 21 passed, TUI focused 18 passed, diff check passed. `slice-a.shared-sort` is ready.
- 2026-07-26 `slice-a.shared-sort` completed by Terra and integrated by Sol; shared mode/scope/result APIs now drive GUI search directly, focused shared-sort tests 5 passed, full `cargo test` passed. `slice-a.shared-persistence` is ready.
- 2026-07-26 `slice-a.shared-persistence` completed by Terra and integrated by Sol; GUI writes enqueue off-frame, history delta merge is cross-process safe and generation-aware, public read/enqueue/flush APIs are available, focused TC-167/168, fmt, clippy, and full tests passed. `slice-a.shared-action` is ready.
- 2026-07-26 `slice-a.shared-action` completed by Terra and integrated by Sol; shared preflight/reauthorization/cancel/freshness lifecycle now drives the GUI worker adapter and exposes safe/raw diagnostics, focused TC-164 and GUI action tests plus full regression passed. `slice-a.shared-filelist` is ready.

## Closure Capsule
- Verification Result: pending
- Review Result: pending
- Required Final Review Complete: no
- Findings Handled: no
- Required Revalidation Complete: no
- Durable Docs Updated: no
- Rollback Ready: yes, isolated shared-core commit
- Remaining Risks: extraction may expose GUI-private types
- Temporary Artifacts Removable Without Losing Open Work: no
- Close Decision: do not close

### Strict Extension
- Impacted Contracts/Compatibility: shared GUI/CLI semantics only; no default changes.
- Layer Or Responsibility Boundaries: domain types/helpers outside adapters; I/O remains worker-owned.
- Review Checkpoint / Findings Disposition Notes: workflow-level checkpoints.
- Additional Verification: characterization tests before extraction; both-name/lowercase/read-only/invalid-ancestor/symlink/partial-cancel/delayed-settlement/double-intent/panic/disconnect FileList fixtures; concurrent and burst history delta merge, nested unknown-field preservation, contention retry, generation handling, and frame-latency evidence.

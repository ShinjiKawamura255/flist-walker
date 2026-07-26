# EXECUTION PLAN: Slice D - TUI long-lived operations

## Control
- Date: 2026-07-26
- Owner: main agent; Terra implementation delegate
- Target Project: FlistWalker
- Plan Role: slice plan
- Execution Profile: strict
- Risk Tier: medium
- Required Safety Controls: asynchronous cancellable reindex/FileList workers; root change clears old selection/preview; temporary-root write tests.
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
- Scope Label: TUI runtime operations

### Temporary Ownership
- AGENTS.md Initial State: existing
- Temporary AGENTS.md Ownership: section-only
- Docs Directory Initial State: existing
- Temporary Docs Directory Ownership: existing-directory

## Goal
- Outcome: TUI can sort, change filters/root, refresh, and create FileList without restart or input blocking.
- Success Conditions: each operation is discoverable, cancellable where applicable, stale-safe, and covered by deterministic tests.
- Closure Slice: not applicable

## Scope
### In
- `F3` sort picker with status display and shared all-match sorting.
- `F2` options overlay for Files/Folders, Regex, case, Ignore, and source; reindex only when required.
- `F4` saved-root picker and root switching.
- `F5` refresh index and `F6` explicit FileList confirmation UI.

### Out
- Editing the saved-root list or changing default root from TUI; GUI remains the manager.
- TUI tab/session restoration.

## Constraints And Assumptions
- Root changes cancel old work and clear current/pinned/preview before accepting new results.
- Source/filter changes reuse current snapshot when possible and reindex only for kind/source requirements.
- FileList overwrite/ancestor decisions are explicit in the TUI confirmation state.
- Overlay/confirmation key precedence follows Slice A's TUI matrix.
- FileList transaction settlement gates selection output, exit, and root switching; the event loop remains active until rollback/report completion and joins the FileList worker without the generic timeout-detach path.
- Pending intent priority is sticky cancel-exit, then last selected root, then selection output; panic/disconnect synthesize failed settlement and never resume a success intent.
- All I/O work remains off the render/input loop.

## Execution
1. Add operation model/command palette or options overlay tests.
2. Add shared sort invocation and mode/status rendering.
3. Add runtime filter/source transitions and refresh.
4. Add saved-root picker/root lifecycle.
5. Add cancellable FileList confirmation/worker flow with pending intent, terminal settlement, join, and rollback-failure handling.
6. Run focused, full, performance, and terminal checks.

## Validation
- Automated: TUI state/worker tests, indexer/FileList tests, CLI contract tests, full suite, ignored indexing perf tests when applicable.
- Manual: root switch, refresh, toggles, cancellation, and FileList confirmation in isolated fixture.
- Regression Focus: stale responses, selection reset on root change, no UI blocking, no unintended write, no post-return FileList write, double-intent priority, panic/disconnect settlement.
- Performance/Security: debounce/throttle; root-scoped authorization; temp roots only.

## Trace
- SDD Impact: required
- Related FR/NFR/CON: FR-006/010/025/026, NFR-001/006/011
- Related SP/DES/TC: SP-001/006/010/013/015/016, DES-005/006/016/017, TC-016/052/057B/084/085/087/088/089/102/110/111/115/162
- Docs To Update Before Closure: README, SDD, manual validation matrix.

## Review
- Review Viewpoints: ordering, architecture, testing, rollback, operability
- Checkpoint(s): workflow-level before-and-final

## Temporary `AGENTS.md` Rule
- Plan Paths: roadmap then this slice.
- Overrides: none.

## Progress Log
- 2026-07-26 Planned.
- 2026-07-26 Main-agent preflight completed; implementation awaits workflow-level focused re-review.

## Closure Capsule
- Verification Result: pending
- Review Result: pending
- Required Final Review Complete: no
- Findings Handled: no
- Required Revalidation Complete: no
- Durable Docs Updated: no
- Rollback Ready: yes, TUI operations feature commit
- Remaining Risks: state-machine growth and cancellation races
- Temporary Artifacts Removable Without Losing Open Work: no
- Close Decision: do not close

### Strict Extension
- Impacted Contracts/Compatibility: startup flags remain valid; runtime changes are TUI-local.
- Layer Or Responsibility Boundaries: operation state separate from rendering and worker execution.
- Review Checkpoint / Findings Disposition Notes: workflow-level checkpoints.
- Additional Verification: perf gates when index/search materialization changes.

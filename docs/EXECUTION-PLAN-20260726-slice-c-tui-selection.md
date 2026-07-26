# EXECUTION PLAN: Slice C - TUI selection experience

## Control
- Date: 2026-07-26
- Owner: main agent; Terra implementation delegate
- Target Project: FlistWalker
- Plan Role: slice plan
- Execution Profile: strict
- Risk Tier: medium
- Required Safety Controls: asynchronous preview/action workers; recording backends; terminal guard restoration on every exit/error path.
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
- Scope Label: TUI selection usability

### Temporary Ownership
- AGENTS.md Initial State: existing
- Temporary AGENTS.md Ownership: section-only
- Docs Directory Initial State: existing
- Temporary Docs Directory Ownership: existing-directory

## Goal
- Outcome: TUI provides preview, safe open/reveal keys, history, clear actions, and discoverable help while preserving selection output.
- Success Conditions: input remains responsive, stale worker responses are ignored, and terminal/stdout contracts pass.
- Closure Slice: not applicable

## Scope
### In
- Toggleable preview pane (`Alt+P`) using shared preview text generation through a worker, with width-aware collapse.
- `Ctrl+O` opens/executes current row only and `Shift+Enter` reveals current row only; `Enter` remains selection output and pins never fan out side effects.
- Shared async delta-merged persisted history with `Ctrl+R`, respecting history persistence disablement and never blocking the TUI event loop on the persistence lock.
- `Ctrl+G` clears query and pins in normal mode; `F1` opens a context-aware help overlay.

### Out
- Mouse support and multi-tab.

## Constraints And Assumptions
- Preview/action responses carry request identity and root/path scope.
- State-specific Enter/Esc/Ctrl+G/Ctrl-C precedence follows Slice A's TUI matrix.
- Hidden pins retain output order.
- Help and status render only on stderr.

## Execution
1. Add failing TUI state/key/render tests.
2. Add preview worker/state/rendering and resize behavior.
3. Add current-only authorized action worker, cancellation/freshness boundary, and status/error reporting.
4. Add asynchronous delta-merge history search, contention warning/retry behavior, clear actions, and context-aware help overlay.
5. Record terminal evidence and update help docs.

## Validation
- Automated: `cargo test cli_tui::tests`, `cargo test --test cli_contract`, focused preview/action/history tests, full suite.
- Manual: Windows terminal resize/paste/preview/help/cancel/output redirection evidence.
- Regression Focus: cleanup-before-output, exit 130, hidden pins, Unicode width, request freshness.
- Performance/Security: no UI-loop filesystem/lock I/O; frame latency is independent of lock wait; recording backend only in automated action tests.

## Trace
- SDD Impact: required
- Related FR/NFR/CON: FR-006/011, NFR-001/011
- Related SP/DES/TC: SP-004/006/010/015/016, DES-005/006/016/017, TC-054/107/110/111/115/162
- Docs To Update Before Closure: README and TUI/manual test sections.

## Review
- Review Viewpoints: responsibility-boundary, testing, security, operability
- Checkpoint(s): workflow-level before-and-final

## Temporary `AGENTS.md` Rule
- Plan Paths: roadmap then this slice.
- Overrides: none.

## Progress Log
- 2026-07-26 Planned.
- 2026-07-26 Main-agent preflight completed; implementation awaits workflow-level focused re-review.
- 2026-07-26 `slice-c.preview-history-help` completed by Terra and integrated by Sol; async stale-safe preview, delta-merged history, context-specific clear/help overlays, bilingual key documentation, 32 focused TUI tests, 27 CLI contract tests, 697 full unit tests plus binary/integration/doc tests, check, clippy, fmt, and diff checks passed. `slice-c.action` is ready.
- 2026-07-26 `slice-c.action` completed by Terra and integrated by Sol; current-row-only open/reveal actions now use the shared authorization lifecycle on a cancellable freshness-scoped worker, recording-backend TC-164 coverage passed, full Rust regression passed, and Slice D is ready.

## Closure Capsule
- Verification Result: pending
- Review Result: pending
- Required Final Review Complete: no
- Findings Handled: no
- Required Revalidation Complete: no
- Durable Docs Updated: no
- Rollback Ready: yes, TUI selection feature commit
- Remaining Risks: pane layout and worker cleanup
- Temporary Artifacts Removable Without Losing Open Work: no
- Close Decision: do not close

### Strict Extension
- Impacted Contracts/Compatibility: Enter/Esc/stdout behavior frozen.
- Layer Or Responsibility Boundaries: state reducer, workers, renderer, and persistence adapter separated.
- Review Checkpoint / Findings Disposition Notes: workflow-level checkpoints.
- Additional Verification: partial setup/draw/read/unwind restoration.

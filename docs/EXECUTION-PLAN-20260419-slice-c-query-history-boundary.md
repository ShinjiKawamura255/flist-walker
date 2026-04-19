# EXECUTION PLAN: Slice C Query History Boundary

## Metadata
- Date: 2026-04-19
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice plan
- Execution Profile: standard
- Planning Depth: roadmap+slice
- Review Pattern: single-subagent
- Review Requiredness: required-before-implementation
- Execution Mode: none
- Execution Mode Policy: Follow parent roadmap. Keep this slice to query history state ownership; do not touch indexing, worker routing, render layout, or persisted session format.
- Parent Plan: docs/EXECUTION-PLAN-20260419-roadmap-quality-maturity-uplift.md
- Child Plan(s): none
- Scope Label: query-history-boundary
- Related Tickets/Issues: external multi-axis evaluation dated 2026-04-18
- Review Status: reviewed
- Review Notes:
  - 2026-04-19 main-agent review: feasible. `input_history.rs` currently mutates `QueryState` internals through repeated `shell.runtime.query_state` chains. Moving history-search transitions behind `QueryState` methods is a narrow owner-boundary improvement with existing query-history tests as coverage. `single-subagent` review is not executed because subagent spawning requires explicit user delegation.

## 1. Background
The external evaluation called out deep field chains and broad `pub(super)` state exposure as a maintainability bottleneck. The query history flow is a contained high-churn path with visible direct mutations, making it a good first Slice C target.

## 2. Goal
Reduce direct history-search field mutation outside `QueryState` while preserving all current query history behavior.

Observable success conditions:
- `input_history.rs` uses `QueryState` owner methods for history-search start, cancel, accept, result replacement, selection movement, and reset.
- No indexing, worker, render layout, or session schema behavior changes.
- Existing query history and shortcut tests still pass under `cargo test`.

## 3. Scope
### In Scope
- Add focused owner methods to `rust/src/app/query_state.rs`.
- Replace direct history-search state mutation in `rust/src/app/input_history.rs`.
- Make `FlistWalkerApp` reset wrappers delegate to `QueryState`.
- Update roadmap, task log, and temporary plan list.

### Out of Scope
- Making all `QueryState` fields private.
- Refactoring `tab_state.rs` snapshot layout or session persistence.
- Changing render history-search UI behavior.
- Indexing, worker, or FileList paths.

## 4. Constraints and Assumptions
- Behavior must remain unchanged.
- Rust changes require `cargo test`.
- VM-003 ignored perf tests are not required because this slice does not touch indexing paths.
- Tests may still inspect `pub(super)` fields until a later broader state encapsulation slice.

## 5. Current Risks
- Risk: Owner methods accidentally change history search cancel/accept semantics.
  - Impact: shortcuts can load the wrong query or fail to restore the original query.
  - Mitigation: rely on existing query history and shortcut tests that cover Ctrl+R, Enter/Ctrl+J/Ctrl+M accept, Escape/Ctrl+G cancel, tab/root reset, and persistence.
- Risk: This slice expands into a broad `QueryState` privacy migration.
  - Impact: large test and tab snapshot churn.
  - Mitigation: stop at owner methods for the active flow and leave field visibility unchanged.

## 6. Execution Strategy
1. Add query history owner methods
   - Files/modules/components: `rust/src/app/query_state.rs`.
   - Expected result: state transitions are named and reusable.
   - Verification: compile through `cargo test`.
2. Route history search flow through owner methods
   - Files/modules/components: `rust/src/app/input_history.rs`, `rust/src/app/input.rs`.
   - Expected result: fewer deep chains and no direct mutation of history-search fields in the flow owner.
   - Verification: existing query history/shortcut tests.
3. Update plan records
   - Files/modules/components: `AGENTS.md`, `docs/TASKS.md`, parent roadmap, this slice.
   - Expected result: active slice and validation result are recorded.
   - Verification: docs diff review.

## 7. Detailed Task Breakdown
- [x] Add `QueryState` history-search owner methods.
- [x] Update `input_history.rs` and `input.rs` to use the owner methods.
- [x] Run `cargo test`.
- [x] Run `cargo clippy --all-targets -- -D warnings`.
- [x] Update task/plan progress.

## 8. Validation Plan
- Automated tests: `cargo test`, `cargo clippy --all-targets -- -D warnings`
- Manual checks: not required for state-only behavior preservation.
- Performance or security checks: not applicable.
- Regression focus: Ctrl+R history search, accept/cancel shortcuts, root/tab reset, query history persistence.

## 9. Rollback Plan
- Revert `query_state.rs`, `input_history.rs`, and `input.rs` together.
- Docs-only plan updates can be reverted independently if the code rollback is required.

## 10. Temporary `AGENTS.md` Rule Draft
Handled by parent roadmap.

## 11. Progress Log
- 2026-04-19 Planned and reviewed Slice C.
- 2026-04-19 Added `QueryState` owner methods for history search lifecycle, result replacement, selection movement, and query history navigation reset.
- 2026-04-19 Routed `input_history.rs` and `input.rs` through those owner methods.
- 2026-04-19 Validation passed with `cargo test` and `cargo clippy --all-targets -- -D warnings`.

## 12. Communication Plan
- Return to user after validation or if existing tests expose a behavior mismatch.

## 13. Completion Checklist
- [x] Planned document created before implementation
- [x] Temporary `AGENTS.md` rule already present
- [x] Slice reviewed
- [x] Owner methods added
- [x] History-search flow routed through owner methods
- [x] Verification completed
- [x] Parent roadmap updated

## 14. Final Notes
This slice intentionally leaves `pub(super)` field visibility in place. The value is to introduce stable owner transitions first, then use later slices to tighten visibility where call sites have already moved behind methods.

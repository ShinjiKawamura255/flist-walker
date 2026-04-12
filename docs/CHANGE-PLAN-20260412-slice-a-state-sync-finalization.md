# CHANGE PLAN: Slice A - Final Ownership Sweep

## Metadata
- Date: 2026-04-12
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice plan
- Execution Mode: none
- Execution Mode Policy: none
- Parent Plan: [docs/CHANGE-PLAN-20260412-roadmap-state-sync-finalization.md](docs/CHANGE-PLAN-20260412-roadmap-state-sync-finalization.md)
- Child Plan(s): none
- Scope Label: final-ownership-sweep
- Related Tickets/Issues: none
- Review Status: 未レビュー
- Review Notes:
  - 実現性レビュー: feasible.
  - The task is intentionally narrow: tighten the remaining ownership seams without reopening the broader architecture work.

## 1. Background
- The prior pass reduced the largest snapshot/live sync hotspots, but some overlap remains.

## 2. Goal
- Reduce the remaining overlap between runtime, tab session, and UI transient state as far as is practical without widening scope.

## 3. Scope
### In Scope
- `rust/src/app/state.rs`
- `rust/src/app/tab_state.rs`
- `rust/src/app/tabs.rs`
- `rust/src/app/coordinator.rs`
- `rust/src/app/pipeline.rs`
- `rust/src/app/index_coordinator.rs`
- `rust/src/app/render.rs`
- `rust/src/app/input.rs`
- `rust/src/app/session.rs`
- `rust/src/app/tests/*`

### Out of Scope
- New features.
- Broad routing redesign.
- Shell boundary work that is already closed.

## 4. Constraints and Assumptions
- Evidence-based validation only.
- Keep the change small enough to review directly.

## 5. Current Risks
- Risk: changing ownership boundaries could accidentally restore a direct-mutation path.
  - Impact: predictability would regress.
  - Mitigation: keep the follow-up focused and verify with tests.

## 6. Execution Strategy
1. Sweep remaining ownership overlap.
2. Validate behavior with tests.

## 7. Detailed Task Breakdown
- [ ] Reduce avoidable duplication between live runtime and tab/session snapshots.
- [ ] Keep UI transient state on the runtime/UI side when it does not need to persist.

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test`

## 9. Rollback Plan
- Revert only the narrow ownership changes if they fail validation.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `state-sync-finalization`, read the roadmap and slice plans before starting implementation.
- Follow the roadmap `Execution Mode` and `Execution Mode Policy`.
- Execute Slice A first unless the plan is updated first.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-12 Planned.
- 2026-04-12 Completed: runtime-only `query_history_dirty_since` and per-tab `pending_restore_refresh_tabs` now own the remaining sync-sensitive state instead of duplicating it across snapshot and session layers.

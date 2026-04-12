# CHANGE PLAN: State Sync Finalization Roadmap

## Metadata
- Date: 2026-04-12
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: roadmap
- Execution Mode: standard
- Execution Mode Policy: Focus only on the remaining ownership/synchronization seams that keep runtime, tab session, and UI transient state from being fully single-sourced. Do not reopen shell-boundary work. Use the closure slice as the final decision point for whether this follow-up can be closed.
- Parent Plan: none
- Child Plan(s):
  - [docs/CHANGE-PLAN-20260412-slice-a-state-sync-finalization.md](docs/CHANGE-PLAN-20260412-slice-a-state-sync-finalization.md)
  - [docs/CHANGE-PLAN-20260412-slice-b-closure-validation.md](docs/CHANGE-PLAN-20260412-slice-b-closure-validation.md)
- Scope Label: state-sync-finalization
- Related Tickets/Issues: none
- Review Status: 未レビュー
- Review Notes:
  - 実現性レビュー: feasible.
  - The previous pass reduced the largest sync hotspots, but the remaining gap is now about final ownership placement and whether the tab/session snapshot model is still carrying too much UI transient state.

## 1. Background
- The previous ownership-consolidation pass removed the biggest direct-mutation hotspots and reduced snapshot churn.
- The remaining concern is whether the tab/session model still duplicates enough state to justify one more targeted pass.

## 2. Goal
- Push the remaining state ownership seams toward a stable live/snapshot split.
- Keep UI transient state out of persistence paths where possible.
- Confirm whether the architecture cleanup sequence can close after this pass.

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
- `docs/TESTPLAN.md`
- `docs/TASKS.md`

### Out of Scope
- New end-user features.
- Reintroducing shell-boundary cleanup.
- Broad routing or worker-protocol rewrites.

## 4. Constraints and Assumptions
- The work must remain evidence-driven.
- If the remaining gap is still material after the next pass, re-plan instead of forcing closure.

## 5. Current Risks
- Risk: the snapshot/live split may still be too permissive.
  - Impact: state transitions remain correct but harder to reason about.
  - Mitigation: keep the follow-up focused on the smallest remaining ownership overlap.
- Risk: closure could still be premature.
  - Impact: the project would still depend on manual sync discipline.
  - Mitigation: use the closure slice to decide whether the remaining overlap is acceptable.

## 6. Execution Strategy
1. Slice A: Final ownership sweep.
   - Expected result: the remaining overlap between runtime, tab session, and UI transient state is reduced to the minimum practical set.
   - Verification: `cargo test` and focused diff review.
2. Slice B: Closure validation and decision.
   - Expected result: a documented close/continue decision with clear evidence.
   - Verification: `cargo test`, docs review, and explicit closure note.

## 7. Detailed Task Breakdown
- [ ] Reduce remaining snapshot/live state overlap where it is still avoidable.
- [ ] Confirm that direct mutation seams are isolated to the intended owners.
- [ ] Decide whether the architecture cleanup can close after this pass.
- [ ] Update durable docs and temporary plan machinery according to that decision.

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test`
- Manual checks:
  - focused GUI smoke if the ownership split changes visible UI behavior
- Regression focus:
  - state ownership clarity
  - snapshot/live sync
  - closure correctness

## 9. Rollback Plan
- Keep the plan narrow enough that a slice-level revert remains understandable.
- If the closure slice shows the goal is not met, keep the temporary plan machinery in place and re-plan instead of forcing closure.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `state-sync-finalization`, read this roadmap and its slice plans before starting implementation.
- Follow the roadmap `Execution Mode` and `Execution Mode Policy`.
- Execute Slice A first, then the closure slice, unless the plan is updated first.
- Treat the closure slice as the final gating step before the roadmap is closed.
- If the closure slice shows the goal is still unmet, re-plan before continuing and do not remove the temporary rule.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-12 Planned.

## 12. Communication Plan
- Return the closure decision and any follow-up plan if the target is still not met.

## 13. Completion Checklist
- [ ] Planned document created before implementation
- [ ] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into durable docs as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 14. Final Notes
- This roadmap exists because the previous pass was good enough to reduce the major hotspots but not yet enough to fully close the architecture sequence.

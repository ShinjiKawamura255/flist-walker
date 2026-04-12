# CHANGE PLAN: Slice A - State Ownership and Event Routing Consolidation

## Metadata
- Date: 2026-04-12
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice plan
- Execution Mode: none
- Execution Mode Policy: none
- Parent Plan: [docs/CHANGE-PLAN-20260412-roadmap-app-state-ownership-consolidation.md](docs/CHANGE-PLAN-20260412-roadmap-app-state-ownership-consolidation.md)
- Child Plan(s): none
- Scope Label: state-ownership-and-event-routing
- Related Tickets/Issues: none
- Review Status: 未レビュー
- Review Notes:
  - 実現性レビュー: feasible.
  - The slice focuses on the remaining duplicated ownership and the event paths that still mutate state directly.

## 1. Background
- The current codebase still keeps some state in multiple places for capture/apply/restore convenience, which makes the true owner harder to identify.
- Some render/input/pipeline paths still update state inline instead of through a clearer command or reducer seam.

## 2. Goal
- Collapse the remaining duplicated ownership boundaries where they add sync risk without providing a meaningful isolation benefit.
- Make the remaining event paths easier to reason about by concentrating mutation behind owner helpers or reducer-like seams.

## 3. Scope
### In Scope
- `rust/src/app/state.rs`
- `rust/src/app/tab_state.rs`
- `rust/src/app/tabs.rs`
- `rust/src/app/render.rs`
- `rust/src/app/input.rs`
- `rust/src/app/pipeline.rs`
- `rust/src/app/pipeline_owner.rs`
- `rust/src/app/result_reducer.rs`
- `rust/src/app/result_flow.rs`
- `rust/src/app/response_flow.rs`
- `rust/src/app/coordinator.rs`
- `rust/src/app/session.rs`
- `rust/src/app/root_browser.rs`
- `rust/src/app/shell_support.rs`
- `rust/src/app/tests/*`

### Out of Scope
- New features.
- Search/index algorithm changes.
- Release or packaging changes.

## 4. Constraints and Assumptions
- Existing regression tests should continue to pass throughout the slice.
- The slice should prefer small ownership clarifications over broad architectural rewrites.

## 5. Current Risks
- Risk: ownership boundaries may be duplicated in a few places for practical reasons.
  - Impact: a purely mechanical cleanup could degrade maintainability if it ignores actual capture/apply needs.
  - Mitigation: keep live state and snapshot state distinct only where there is a concrete persistence or routing need.
- Risk: direct mutation may be deeply intertwined with rendering.
  - Impact: a too-aggressive rewrite could spread state changes across too many helpers.
  - Mitigation: favor clearer command collection and explicit owner helpers rather than expanding render complexity.

## 6. Phase Plan
1. Ownership inventory and boundary tightening.
   - Identify the state that truly belongs to live runtime, tab snapshot, UI transient, or feature-local ownership.
   - Remove or reduce redundant mirror fields where the same data is being maintained for convenience only.
   - Success condition: capture/apply paths have a single obvious owner for each remaining piece of state.
2. Event-routing consolidation.
   - Concentrate remaining direct mutations behind reducer or owner helpers where that makes the mutation path clearer.
   - Reduce inline mutation in render/input paths when the mutation is actually an event transition.
   - Success condition: the important state transitions are easier to trace from event to owner helper.
3. Test and docs alignment.
   - Adjust existing tests if ownership boundaries shift in a meaningful way.
   - Keep the validation story aligned with the new ownership map.
   - Success condition: `cargo test` stays green and the new ownership shape is reflected in docs/TASKS if necessary.

## 7. Detailed Task Breakdown
- [ ] Identify the remaining duplicated ownership boundaries.
- [ ] Narrow the snapshot/live-state sync surface.
- [ ] Consolidate the remaining direct mutation paths.
- [ ] Re-run validation and record any lasting boundary decisions.

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test`
- Manual checks:
  - targeted GUI smoke if render/input ownership changes noticeably
- Regression focus:
  - snapshot sync
  - event-routing clarity
  - stale response behavior

## 9. Rollback Plan
- Keep the changes grouped so a partial rollback does not reintroduce the old transparent ownership shape.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `architecture-score-80`, read the roadmap and this slice plan before starting implementation.
- Follow the roadmap `Execution Mode` and `Execution Mode Policy`.
- Execute Slice A before the closure slice unless the plan is updated first.
- If the slice reveals that the ownership cleanup needs a different boundary, update the plan before continuing.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-12 Planned.

## 12. Communication Plan
- Return a concise summary of the ownership changes and any remaining gap before closure.

## 13. Completion Checklist
- [ ] Planned document created before implementation
- [ ] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting notes moved into durable docs as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 14. Final Notes
- This slice is the only implementation slice in the roadmap.

# CHANGE PLAN: Slice B - Routing and Lifecycle Consolidation

## Metadata
- Date: 2026-04-12
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice plan
- Execution Mode: none
- Execution Mode Policy: none
- Parent Plan: [docs/CHANGE-PLAN-20260412-roadmap-architecture-score-uplift.md](docs/CHANGE-PLAN-20260412-roadmap-architecture-score-uplift.md)
- Child Plan(s): none
- Scope Label: routing-and-lifecycle-consolidation
- Related Tickets/Issues: none
- Review Status: 未レビュー
- Review Notes:
  - 実現性レビュー: feasible.
  - The slice can build on the existing owner modules and response flow seams already documented in the architecture.
  - No external dependency blocks the routing cleanup.
  - External review feedback highlights direct mutation, reducer blending, and imperative UI refreshes as the remaining routing/lifecycle gap.

## 1. Background
- The architecture already separated several owners, but routing and lifecycle behavior still spans multiple modules, so the remaining debt is about consolidating the path rather than inventing new structure.
- Background tab handling, stale response discard, and filelist/update flows are the most sensitive areas because they encode the latest-wins contract.
- The review also flagged that the system still feels procedural in places where a reducer-style message flow should be visible, especially for state writes and UI-derived updates.

## 2. Goal
- Ensure that each async concern has one obvious owner for request routing, response acceptance, and cleanup.
- Reduce the chance of stale-response or duplicated-transition regressions by making the lifecycle boundaries obvious and testable.
- Push the remaining state transitions toward explicit message/reducer boundaries and move trivial derived UI text out of imperative refresh calls.

## 3. Scope
### In Scope
- `rust/src/app/pipeline.rs`
- `rust/src/app/pipeline_owner.rs`
- `rust/src/app/search_coordinator.rs`
- `rust/src/app/index_coordinator.rs`
- `rust/src/app/response_flow.rs`
- `rust/src/app/result_reducer.rs`
- `rust/src/app/preview_flow.rs`
- `rust/src/app/filelist.rs`
- `rust/src/app/update.rs`
- `rust/src/app/tabs.rs`
- `rust/src/app/render.rs`
- `docs/TESTPLAN.md`

### Out of Scope
- Core search ranking semantics.
- Shell boundary work already handled by Slice A.
- Release/update packaging details beyond lifecycle ownership.

## 4. Constraints and Assumptions
- Latest-wins behavior based on `request_id` remains the acceptance rule.
- Response routing should stay non-blocking and UI-friendly.
- Tests should stay aligned with the owner-aligned modules already in the repo.

## 5. Current Risks
- Risk:
  - Impact: Consolidation could accidentally increase coupling between owner modules.
  - Mitigation: Keep routing surfaces thin and preserve clear ownership boundaries.
- Risk:
  - Impact: Stale response or cleanup regressions could slip through if only docs are updated.
  - Mitigation: Keep owner-focused regression tests in the validation step.
- Risk:
  - Impact: Tab/background handling could become fragile during consolidation.
  - Mitigation: Preserve the current background-tab semantics and validate them explicitly.

## 6. Execution Strategy
1. Consolidate search/index request routing.
   - Files/modules/components: `app/search_coordinator.rs`, `app/index_coordinator.rs`, `app/pipeline.rs`, `app/pipeline_owner.rs`.
   - Expected result: request routing and lifecycle tracking are clearly owned instead of being implied by call sites.
   - Verification: `cargo test` and owner-specific regression tests.
2. Consolidate response application and cleanup boundaries.
   - Files/modules/components: `app/response_flow.rs`, `app/result_reducer.rs`, `app/preview_flow.rs`, `app/filelist.rs`, `app/update.rs`, `app/tabs.rs`, `app/render.rs`.
   - Expected result: response handling, cleanup, and background-tab behavior follow one cleanup path per concern.
   - Verification: targeted lifecycle tests and focused GUI/manual checks.
3. Bring validation coverage up to the new routing story.
   - Files/modules/components: `docs/TESTPLAN.md`.
   - Expected result: the validation matrix reflects the final owner seams and closure criteria.
   - Verification: docs diff review and traceability check.

## 7. Detailed Task Breakdown
- [ ] Normalize request routing so search and index flows are owned explicitly.
- [ ] Align response application and cleanup with the owner modules.
- [ ] Update test coverage and validation text to match the final routing story.

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test`
- Manual checks:
  - GUI smoke for search, tab switch, background-tab restore, and filelist/update flows
- Performance or security checks:
  - Preserve latest-wins and non-blocking UI behavior
- Regression focus:
  - stale response discard
  - background tab routing
  - lifecycle cleanup ordering

## 9. Rollback Plan
- Routing changes should be revertible per owner module boundary.
- If the cleanup path becomes more complex, roll back the last consolidation step and keep the prior owner seam.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `architecture-score-80`, read the roadmap and this slice plan before starting implementation.
- Follow the roadmap `Execution Mode` and `Execution Mode Policy`.
- Execute the work in the documented order unless the plan is updated first.
- If scope, order, or risk changes, update the change plan before continuing.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-12 Planned.

## 12. Communication Plan
- Return to the closure slice when the routing and lifecycle story is stable enough to validate the goal.

## 13. Completion Checklist
- [ ] Planned document created before implementation
- [ ] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into the durable docs as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 14. Final Notes
- Keep this slice focused on routing and lifecycle ownership; do not expand it into general shell cleanup.

# CHANGE PLAN: App Shell and Use-Case Decoupling - Slice B

## Metadata
- Date: 2026-04-12
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice plan
- Execution Mode: none
- Execution Mode Policy: none
- Parent Plan: docs/CHANGE-PLAN-20260412-roadmap-app-shell-usecase-decoupling.md
- Child Plan(s): none
- Scope Label: app-shell-usecase-decoupling
- Related Tickets/Issues: none
- Review Status: reviewed
- Review Notes:
  - This slice focuses on the first behavior-neutral use-case/reducer extraction after the shell boundary foundation.
  - The slice should keep existing async routing semantics intact while moving the most stateful response handling behind explicit reducer helpers.
  - Review completed on 2026-04-12.
  - No blocking issues were found in the slice shape.

## 1. Background
- Slice A established an explicit shell/state ownership boundary.
- The next architectural pressure point is the repeated response-handling logic spread across `pipeline.rs`, `result_flow.rs`, and `preview_flow.rs`.
- These paths are still routed through the app shell, but the actual state transitions are already coherent enough to be grouped behind a reducer boundary.

## 2. Goal
- Extract a first reducer/use-case boundary for result, preview, and related response handling.
- Keep request routing, stale-response discard, and tab ownership behavior unchanged.
- Reduce the amount of direct transition logic that remains embedded in the shell-facing flow modules.

## 3. Scope
### In Scope
- Result/preview response reduction helpers.
- Search result snapshot refresh and sort application transitions where they belong to the same reducer boundary.
- Small shell-facing wrappers that keep current call sites stable while the behavior moves behind the new boundary.
- Doc and test updates needed to describe the new boundary.

### Out of Scope
- Full extraction of all pipeline, tab, filelist, and update use cases.
- Search/index algorithm changes.
- UI redesign or feature additions.
- Roadmap closure; that belongs to Slice C.

## 4. Constraints and Assumptions
- Behavior must remain stable for search, preview, sort, and stale-response discard.
- The existing `request_id` / tab ownership model remains the source of truth.
- The slice should stay small enough that `cargo test` is sufficient for validation.
- If the reducer boundary reveals a stronger seam than expected, the roadmap must be updated before widening scope.

## 5. Current Risks
- Risk:
  - Moving response handling without fully separating request orchestration.
  - Impact:
  - The shell may still look busy even after the reducer extraction.
  - Mitigation:
  - Limit this slice to the most stateful response transitions first, then re-evaluate the remaining gap in the roadmap.
- Risk:
  - Introducing a reducer helper that duplicates existing logic rather than consolidating it.
  - Impact:
  - More code, same coupling.
  - Mitigation:
  - Keep the reducer boundary mechanical and use the existing tests to confirm no behavior drift.
- Risk:
  - Accidentally widening the slice into filelist/update/index ownership at the same time.
  - Impact:
  - The slice becomes too large for safe review and rollback.
  - Mitigation:
  - Defer unrelated flows to later slices unless the roadmap is explicitly updated first.

## 6. Execution Strategy
1. Phase 1: Extract result/preview reducer helpers
   - Files/modules/components: `rust/src/app/result_flow.rs`, `rust/src/app/preview_flow.rs`, `rust/src/app/tabs.rs`, and a new reducer module such as `rust/src/app/result_reducer.rs` if needed.
   - Expected result: result snapshot refresh, preview response handling, and sort application transitions live behind a named reducer/use-case boundary rather than only inside the shell-facing flow modules.
   - Verification: compile and `cd rust && cargo test`.
2. Phase 2: Reconnect the remaining shell-facing entry points to the reducer boundary
   - Files/modules/components: `rust/src/app/pipeline.rs`, `rust/src/app/mod.rs`, and any small wrapper adjustments needed to keep call sites stable.
   - Expected result: the shell-facing code delegates to the reducer boundary for the extracted transitions, and the roadmap can judge whether another slice is needed for the remaining flows.
   - Verification: `cd rust && cargo test`, plus review of stale-response and preview/search regression coverage.
3. Phase 3: Update docs and record the next gap
   - Files/modules/components: `docs/ARCHITECTURE.md`, `docs/DESIGN.md`, `docs/TESTPLAN.md`, and `docs/TASKS.md` if the slice result is durable.
   - Expected result: the new reducer seam is documented, and the roadmap explicitly records what still remains in the shell.
   - Verification: doc diff review and `cargo test`.

## 7. Detailed Task Breakdown
- [ ] Define the first reducer boundary for result/preview transitions.
- [ ] Move the stateful transition logic behind that boundary without changing request ownership semantics.
- [ ] Keep shell-facing wrappers thin and behavior-neutral.
- [ ] Update architecture docs if the boundary is worth preserving.

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test`
  - Existing tests for search response routing, preview refresh, sort application, and stale response discard.
- Manual checks:
  - Basic GUI smoke for query editing, result movement, preview updates, and tab switching after the slice lands.
- Performance or security checks:
  - None expected unless the slice accidentally touches indexing or search execution paths.
- Regression focus:
  - Stale worker response discard.
  - Preview cache continuity.
  - Search-result selection and sort stability.
  - Tab ownership when responses arrive out of order.

## 9. Rollback Plan
- The reducer extraction should be revertible as a grouped unit if it proves to be only indirection.
- Any new module added solely for the reducer boundary can be removed together with the slice if a later slice chooses a different seam.
- If docs are updated as part of the slice, they should roll back with the corresponding code boundary.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `app-shell-usecase-decoupling`, read `docs/CHANGE-PLAN-20260412-roadmap-app-shell-usecase-decoupling.md`, `docs/CHANGE-PLAN-20260412-slice-a-app-shell-usecase-decoupling.md`, and `docs/CHANGE-PLAN-20260412-slice-b-app-shell-usecase-decoupling.md` before starting implementation.
- Follow the roadmap `Execution Mode` and `Execution Mode Policy`.
- Execute the work in the documented order unless the plan is updated first.
- If scope, order, or risk changes, update the change plan before continuing.
- If later slices are added under this roadmap, read them in upper-to-lower order before the relevant work.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-12 Planned.
- 2026-04-12 Implemented result/preview/sort reducer extraction and validated it with `cd rust && cargo test`.

## 12. Communication Plan
- Return to user when:
  - Slice B review is complete and reflected
  - Slice B phases are complete
  - the slice cannot continue without a blocking problem

## 13. Completion Checklist
- [x] Plan created before implementation
- [x] Temporary `AGENTS.md` rule added
- [x] Slice executed in documented order or the slice updated first
- [x] Verification completed
- [x] Lasting design/test updates moved into the durable docs
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Slice plan deleted after completion

## 14. Final Notes
- Slice B should make the reducer boundary visible, not finish the whole architecture migration.
- Anything that remains after this slice should be intentionally deferred to Slice C.

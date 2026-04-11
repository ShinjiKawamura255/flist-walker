# CHANGE PLAN: App Shell and Use-Case Decoupling - Slice A

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
  - Review completed on 2026-04-12.
  - No blocking issues found in the slice shape.
  - This slice is an appropriate foundation slice because it can establish the shell/model contract while keeping behavior stable.

## 1. Background
- The roadmap goal is to move the project toward a thin GUI shell and explicit use-case ownership.
- Slice A needs to create the seams that later extraction work can rely on, without forcing the first slice to absorb every architectural change at once.
- The safe starting point is to make ownership boundaries explicit and reduce the direct state surface of the coordinator before moving the higher-level flows.

## 2. Goal
- Establish the app shell and state-model contract that later slices can build on.
- Reduce the amount of implicit ownership living directly in `FlistWalkerApp` without changing observable behavior.
- Make it clear which data is shell-owned, tab-owned, feature-owned, and transient UI state.

## 3. Scope
### In Scope
- Shell/model boundary introduction for app state ownership.
- Direct field surface reduction in `rust/src/app/mod.rs` where it can be done behavior-neutrally.
- Explicit ownership grouping for runtime, tab/session, and feature/dialog state.
- Doc and test updates that lock the new boundary in place.

### Out of Scope
- Full extraction of all use cases and reducers.
- Search/index algorithm changes.
- UI redesign or feature additions.
- Roadmap closure; that belongs to Slice C.

## 4. Constraints and Assumptions
- The slice must not change the user-visible behavior of startup, tab switching, query input, search, filelist, preview, or update flows.
- Any new type or module introduced here should primarily clarify ownership, not add behavior.
- The validation baseline remains `cargo test`, plus focused regression tests that already protect the affected flows.
- If Slice A reveals that a boundary is too coarse, the plan should be updated before continuing to Slice B.

## 5. Current Risks
- Risk:
  - Introducing a model boundary that still leaves most transitions in `mod.rs`.
  - Impact:
  - The slice completes but does not materially reduce architectural pressure.
  - Mitigation:
  - Require the slice outcome to reduce the shell's direct state surface, not just add new wrapper types.
- Risk:
  - Moving ownership groups in a way that disturbs session restore or tab routing.
  - Impact:
  - High regression risk in the flows with the strongest user frequency.
  - Mitigation:
  - Keep the slice behavior-neutral and validate the startup/session/tab tests together.
- Risk:
  - The foundation slice expands into use-case extraction too early.
  - Impact:
  - The scope becomes too large for a safe first slice.
  - Mitigation:
  - Stop at the contract and ownership seam; leave extraction to Slice B.

## 6. Execution Strategy
1. Phase 1: Introduce the shell/model contract
   - Files/modules/components: `rust/src/app/mod.rs`, `rust/src/app/state.rs`, and any new shell/model module needed to make the contract explicit.
   - Expected result: The app shell has a clearly named boundary for command/event routing and state ownership.
   - Verification: Compile and run `cargo test` after the contract is introduced.
2. Phase 2: Re-home the explicit ownership groups
   - Files/modules/components: `rust/src/app/query_state.rs`, `rust/src/app/ui_state.rs`, `rust/src/app/tab_state.rs`, `rust/src/app/coordinator.rs`, and the owner-facing parts of `rust/src/app/mod.rs` / `rust/src/app/state.rs`.
   - Expected result: runtime, tab/session, and feature/dialog state are grouped more explicitly, and `mod.rs` has less direct ownership surface.
   - Verification: Focused regression tests for startup, tabs, session restore, and stale response handling, plus `cargo test`.
3. Phase 3: Sync docs and lock in the boundary
   - Files/modules/components: `docs/ARCHITECTURE.md`, `docs/DESIGN.md`, `docs/TESTPLAN.md`, and `docs/TASKS.md` if the slice outcome is durable.
   - Expected result: the new shell/state contract is documented and the roadmap has a clear handoff to Slice B.
   - Verification: Doc diff review and `cargo test`.

## 7. Detailed Task Breakdown
- [ ] Define the shell/model contract without changing behavior.
- [ ] Reduce direct coordinator ownership by moving explicit state groups behind the contract.
- [ ] Verify that the existing regression tests still describe the new boundary correctly.
- [ ] Update architecture docs if the new boundary is durable enough to keep.

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test`
  - Existing tests for startup/root selection, session restore, tabs, pipeline, and stale response handling.
- Manual checks:
  - Basic GUI smoke for startup, tab switching, query editing, and one update/filelist interaction after the slice lands.
- Performance or security checks:
  - None expected unless the slice accidentally touches indexing or search execution paths.
- Regression focus:
  - Startup root selection.
  - Tab switch/reorder/close behavior.
  - Session restore / lazy refresh behavior.
  - Stale response discard.

## 9. Rollback Plan
- The slice should be revertible as a grouped unit if the shell contract adds complexity without simplifying ownership.
- Any new module added solely for the contract can be removed together with the slice if later slices choose a different seam.
- If docs are updated as part of the slice, they should roll back with the corresponding code boundary.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `app-shell-usecase-decoupling`, read `docs/CHANGE-PLAN-20260412-roadmap-app-shell-usecase-decoupling.md` and `docs/CHANGE-PLAN-20260412-slice-a-app-shell-usecase-decoupling.md` before starting implementation.
- Follow the roadmap `Execution Mode` and `Execution Mode Policy`.
- Execute the work in the documented order unless the plan is updated first.
- If scope, order, or risk changes, update the change plan before continuing.
- If later slices are added under this roadmap, read them in upper-to-lower order before the relevant work.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-12 Planned.
- 2026-04-12 Implemented the shell contract via `AppShellState`, updated the shell-oriented docs, and validated the change with `cd rust && cargo test`.

## 12. Communication Plan
- Return to user when:
  - Slice A review is complete and reflected
  - Slice A phases are complete
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
- Slice A should finish with a cleaner shell/state boundary, not a completed architecture rewrite.
- Anything that remains after this slice should be intentionally deferred to Slice B or Slice C.

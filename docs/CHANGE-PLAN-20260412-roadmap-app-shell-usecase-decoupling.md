# CHANGE PLAN: App Shell and Use-Case Decoupling Roadmap

## Metadata
- Date: 2026-04-12
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: roadmap
- Execution Mode: standard
- Execution Mode Policy: Execute slices in order. Phase execution may be delegated to subagents when a phase touches multiple modules, while the main agent remains orchestrator and reviewer. Do not start later slices until the current slice has been reviewed and its outcome reflected in this roadmap. If the goal is still unmet after the validation slice, update the roadmap before adding any new slice.
- Parent Plan: none
- Child Plan(s):
  - docs/CHANGE-PLAN-20260412-slice-a-app-shell-usecase-decoupling.md
- Scope Label: app-shell-usecase-decoupling
- Related Tickets/Issues: none
- Review Status: reviewed
- Review Notes:
  - Review completed on 2026-04-12.
  - No blocking issues found in the roadmap shape.
  - `standard` execution is appropriate here because Slice A should be reviewed before later slices are started.
  - Phase execution may be delegated to subagents for multi-file work, but the main agent remains accountable for orchestration and review.

## 1. Background
- The current architecture already has meaningful module boundaries, but `FlistWalkerApp` still acts as the central place where too many ownership decisions, state transitions, and orchestration steps meet.
- That concentration is acceptable for a thin coordinator, but the current shape still makes future changes expensive because unrelated behavior tends to reconnect through the same shell surface.
- The next step is to keep the existing behavior and test coverage, while making the shell explicitly event-driven and moving use-case ownership away from the coordinator.

## 2. Goal
- The app shell should become a thin presentation/orchestration layer that routes events and commands, rather than the primary owner of business state transitions.
- State ownership should be explicit enough that tabs, query/search, preview, filelist, update, and session concerns can evolve without repeatedly expanding `app/mod.rs`.
- The end state should be observable in code structure, tests, and docs: use cases own transitions, the shell routes, and validation confirms that the coordinator no longer accretes new policy.
- This roadmap reserves the final slice for goal validation and the decision to close the roadmap or continue with a new slice if the goal is not yet met.

## 3. Scope
### In Scope
- Thin shell / use-case separation for GUI orchestration.
- Explicit command/event and reducer-oriented boundaries where they clarify ownership.
- State decomposition that makes app-global, tab-local, and feature-local ownership easier to see.
- Test and doc updates needed to keep the architecture contract synchronized.

### Out of Scope
- New user-visible features.
- Search algorithm redesign.
- Indexing performance tuning beyond what is needed to keep behavior unchanged.
- Broad UI redesign or layout restyling.

## 4. Constraints and Assumptions
- Existing behavior must remain intact unless a slice explicitly documents a behavioral change and its regression coverage.
- UI responsiveness remains a hard constraint; long-running work stays off the UI thread.
- Request routing must continue to honor `request_id` / tab ownership rules so stale worker responses cannot rewind UI state.
- The roadmap assumes the current test suite is the primary safety net and that `cargo test` remains the main automated validation gate for each slice.

## 5. Current Risks
- Risk:
  - Over-abstracting the shell too early and creating new indirection without actually reducing coupling.
  - Impact:
  - The code becomes harder to follow while still retaining most of the same coordination cost.
  - Mitigation:
  - Keep Slice A behavior-neutral and use it only to establish the contract and ownership seams.
- Risk:
  - Moving multiple orchestration paths at once and breaking tab/session or stale-response behavior.
  - Impact:
  - Regression in high-frequency GUI flows.
  - Mitigation:
  - Keep Slice B as the first real extraction slice after the foundation is validated, and keep rollback boundaries small.
- Risk:
  - The final validation slice may show that the shell is thinner but still not thin enough.
  - Impact:
  - The roadmap could end without reaching the target architecture.
  - Mitigation:
  - Reserve the final slice for an explicit close-or-continue decision, and add another slice only after updating the roadmap.

## 6. Execution Strategy
1. Slice A: Shell and State-Model Foundation
   - Files/modules/components: `rust/src/app/mod.rs`, `rust/src/app/state.rs`, `rust/src/app/query_state.rs`, `rust/src/app/ui_state.rs`, `rust/src/app/tab_state.rs`, `rust/src/app/coordinator.rs`, and any new shell/model boundary module needed to make ownership explicit.
   - Expected result: The app shell has an explicit model/command boundary, and direct ownership surfaces in `mod.rs` are reduced without changing behavior.
   - Verification: `cargo test`, plus targeted app regression tests that cover startup, tabs, session restore, and stale response handling.
2. Slice B: Use-Case and Reducer Extraction
   - Files/modules/components: `rust/src/app/pipeline.rs`, `rust/src/app/tabs.rs`, `rust/src/app/filelist.rs`, `rust/src/app/update.rs`, `rust/src/app/render.rs`, `rust/src/app/input.rs`, `rust/src/app/result_flow.rs`, `rust/src/app/preview_flow.rs`, `rust/src/app/search_coordinator.rs`, `rust/src/app/index_coordinator.rs`, and any dedicated reducer/use-case modules introduced for those flows.
   - Expected result: Search, index, tab, update, filelist, and preview transitions are owned by explicit use-case/reducer boundaries instead of the shell.
   - Verification: `cargo test`, plus focused regression coverage for routing, stale-response discard, and UI state continuity.
3. Slice C: Goal Validation and Roadmap Closure
   - Files/modules/components: `docs/ARCHITECTURE.md`, `docs/DESIGN.md`, `docs/TESTPLAN.md`, `docs/TASKS.md`, and `AGENTS.md` cleanup if the goal is reached.
   - Expected result: The thin shell target is either confirmed and the roadmap is closed, or the remaining gap is documented and a follow-up slice is defined.
   - Verification: `cargo test`, doc diff review, and GUI smoke checks if the extraction touched visible routing behavior.

## 7. Detailed Task Breakdown
- [ ] Establish a shell/model contract that separates orchestration from state ownership.
- [ ] Move the first state bundle or routing seam behind that contract without changing behavior.
- [ ] Extract the next use-case boundary only after the foundation is validated.
- [ ] Synchronize architecture docs and test expectations with the new shell boundary.
- [ ] Validate whether the thin-shell target is reached before closing the roadmap.

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test`
  - Existing app regression modules for startup, tabs, session restore, pipeline, and stale response behavior.
- Manual checks:
  - GUI smoke for startup, tab switching, query editing, and preview/update/filelist flows after each behavior-affecting slice.
- Performance or security checks:
  - None beyond the existing regression/perf gates unless a slice touches indexing or search execution paths.
- Regression focus:
  - Stale worker response discard.
  - Tab/session ownership after switch and restore.
  - Query/input continuity.
  - Update/filelist dialog routing.

## 9. Rollback Plan
- Slice A should remain behavior-neutral enough that it can be reverted as a unit if the new shell boundary proves too invasive.
- Slice B changes must stay grouped by use-case so reducer extraction can be backed out without touching unrelated state bundles.
- Docs should be updated in the same slice that changes the structure they describe, so rollback can restore both code and docs together.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `app-shell-usecase-decoupling`, read `docs/CHANGE-PLAN-20260412-roadmap-app-shell-usecase-decoupling.md` and the active slice plan `docs/CHANGE-PLAN-20260412-slice-a-app-shell-usecase-decoupling.md` before starting implementation.
- Follow the roadmap `Execution Mode` and `Execution Mode Policy`.
- Execute the work in the documented order unless the plan is updated first.
- If scope, order, or risk changes, update the change plan before continuing.
- If additional slices are added later under this roadmap, read them from upper to lower order before the relevant work.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-12 Planned.
- 2026-04-12 Slice A completed: `FlistWalkerApp` now dereferences through `AppShellState`, the app shell owns the runtime/ui/cache/feature bundles explicitly, and the full Rust test suite passed after the change.

## 12. Communication Plan
- Return to user when:
  - plan creation and review are complete
  - all phases are complete
  - a phase cannot continue without resolving a blocking problem
- If the project is under git control, commit when a completed phase forms an independent verification/rollback unit.

## 13. Completion Checklist
- [ ] Planned document created before implementation
- [ ] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] If the project is under git control, each commit corresponds to an independent verification/rollback unit, and grouped phases are documented in the plan
- [ ] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, and `TESTPLAN.md` as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 14. Final Notes
- Before deleting this plan, move any lasting decisions into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, or `TESTPLAN.md`.

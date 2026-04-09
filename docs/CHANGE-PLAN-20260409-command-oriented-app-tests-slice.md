# CHANGE PLAN: Command-Oriented App Tests Slice

## Metadata
- Date: 2026-04-09
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice
- Parent Plan: `docs/CHANGE-PLAN-20260408-app-architecture-roadmap.md`
- Child Plan(s): none
- Scope Label: command-oriented-app-tests-slice
- Related Tickets/Issues: none
- Review Status: reviewed
- Review Notes: Slice review completed on 2026-04-09 after the parent roadmap remained `feasible` following Slice C. The slice stays distinct from Slice C because it targets test boundaries rather than worker protocol ownership, and it stays distinct from Slice E because it focuses on command/owner-facing test structure before additional tracing/supportability changes land. Convergence review completed on 2026-04-09 after fixing the active-slice references in roadmap/TASKS/AGENTS and confirming that the work still fits a 2-level plan without a `subslice`.

## 1. Background
- Slice A-C established clearer app-layer seams around pipeline ownership, background tab result flow, and worker protocol boundaries.
- The remaining architectural pressure is concentrated in the test layer, where large files such as `rust/src/app/tests/app_core.rs` and `rust/src/app/tests/session_tabs.rs` still mix multiple concerns and rely heavily on broad `FlistWalkerApp` fixtures.
- This slice exists to reorganize app tests so they exercise owner/command boundaries more directly and make later tracing/supportability work easier to evolve safely.

## 2. Goal
- Reframe app-layer tests around command/owner boundaries instead of monolithic app fixtures where practical.
- Reduce cross-concern coupling inside large test files and align coverage with the owner seams added in earlier slices.
- Leave the project with smaller, easier-to-extend test modules before Slice E adds more tracing/supportability assertions.
- Slice exit criteria:
  - command/owner-oriented test seams are identified and reflected in the active test module layout
  - at least the highest-churn app test coverage is moved behind narrower support helpers or owner-aligned test modules
  - steady-state docs and roadmap/task tracking are synchronized before advancing

## 3. Scope
### In Scope
- `rust/src/app/tests/app_core.rs`
- `rust/src/app/tests/session_tabs.rs`
- `rust/src/app/tests/index_pipeline/*`
- `rust/src/app/tests/support.rs`
- owner/command surfaces that tests target, if lightweight extraction is needed to enable the test reshaping
- related docs/tracking updates

### Out of Scope
- user-visible behavior changes
- worker protocol redesign
- new tracing/supportability behavior beyond what is needed to keep tests readable
- search/query contract changes

## 4. Constraints and Assumptions
- Behavioral coverage must remain equivalent or better after the test reshaping.
- The slice should prefer moving test boundaries and support helpers over reopening app-layer architecture unless a tiny seam is necessary to make tests command-oriented.
- `cargo test` remains the baseline validation; GUI/manual checks are not expected unless the supporting app seams change behavior.

## 4.5 Slice Review Alignment
- Parent feasibility basis:
  - The roadmap already confirmed this program is feasible with the current app-layer seams and validation coverage, and Slice C improved the protocol boundaries that tests depend on.
- Why this slice remains the right active child plan:
  - The current test layout has multiple meaningful restructuring phases: mapping owner/command targets, relocating high-value tests, and syncing docs/tracking.
  - The work is bounded to test structure and helper seams, so it does not collapse into the completed protocol slice or the later tracing slice.
  - A `subslice` is not needed yet because the expected phases are still small enough to execute and verify with a single active slice plan.

## 5. Current Risks
- Risk:
  - Test reshaping devolves into file shuffling without improving the exercised boundary.
  - Impact:
    - The slice adds churn but leaves tests as broad app-fixture integration checks.
  - Mitigation:
    - Use owner/command alignment as the completion criterion, not file count.
- Risk:
  - Helper extraction leaks behavior changes into production code.
  - Impact:
    - The slice stops being test-focused and becomes an unplanned architecture refactor.
  - Mitigation:
    - Keep production changes narrowly scoped to exposing already-existing owner/command seams and reject broader behavior changes.

## 6. Execution Strategy
1. Phase 1: Map the current app test hotspots to owner/command seams
   - Files/modules/components: `rust/src/app/tests/app_core.rs`, `rust/src/app/tests/session_tabs.rs`, `rust/src/app/tests/index_pipeline/*`, relevant owner modules
   - Expected result: the highest-value tests are grouped by the owner or command surface they should target, and the minimal helper/seam changes are identified before edits begin.
   - Verification: targeted diff review plus `cd rust && cargo test`
2. Phase 2: Move high-churn tests toward narrower owner/command-oriented modules
   - Files/modules/components: app test modules and support helpers; owner/command modules only if small enabling seams are required
   - Expected result: large mixed-concern tests are split or rewritten so update/filelist/pipeline/tab behaviors target narrower command or owner boundaries with less fixture coupling.
   - Verification: `cd rust && cargo test`
3. Phase 3: Sync docs/tracking and close the slice
   - Files/modules/components: `docs/ARCHITECTURE.md`, `docs/DESIGN.md`, `docs/TASKS.md`, roadmap progress
   - Expected result: steady-state docs describe the new test boundary expectations and roadmap tracking advances cleanly.
   - Verification: docs diff review plus the same automated validation used for code changes.

## 7. Detailed Task Breakdown
- [ ] Map oversized test files and assign them to owner/command targets
- [ ] Introduce or refine support helpers needed for command-oriented tests
- [ ] Move the highest-churn tests away from broad app-fixture modules
- [ ] Sync architecture/design/task docs after the test layout settles

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test`
- Manual checks:
  - none expected unless a helper extraction changes visible behavior unexpectedly
- Performance or security checks:
  - preserve existing test coverage for request freshness, background routing, and worker shutdown behavior
- Regression focus:
  - update/filelist/pipeline owner behavior still covered after test moves
  - tab/background routing behavior remains covered without relying on unrelated fixtures
  - support helpers do not hide stale-response or request-id assertions

## 9. Rollback Plan
- Revert test-module reshaping independently if coverage becomes harder to read or production seams expand beyond the slice scope.
- Keep Slice A-C architecture changes intact unless rollback proves a hard dependency.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `app-architecture-roadmap`, read `docs/CHANGE-PLAN-20260408-app-architecture-roadmap.md` and then `docs/CHANGE-PLAN-20260409-command-oriented-app-tests-slice.md` before starting implementation.
- Execute the work in the documented order unless the roadmap or active slice is updated first.
- If scope, order, or risk changes, update the relevant change plan before continuing.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-09: Created as the active slice after Slice C completed.
- 2026-04-09: Review completed. Confirmed the parent-child handoff, active slice references, and the planned phases for command/owner-oriented test reshaping.

## 12. Completion Checklist
- [x] Planned document created before implementation
- [x] Temporary `AGENTS.md` rule updated to point at this active slice
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, and `TESTPLAN.md` as needed
- [ ] Temporary `AGENTS.md` rule removed after roadmap completion
- [ ] Change plan deleted after roadmap completion

## 13. Final Notes
- This slice assumes Slice A-C are stable. Structured tracing/supportability remains deferred to roadmap Slice E.

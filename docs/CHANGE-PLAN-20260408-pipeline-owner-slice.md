# CHANGE PLAN: Pipeline Owner Slice

## Metadata
- Date: 2026-04-08
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice plan
- Parent Plan: `docs/CHANGE-PLAN-20260408-app-architecture-roadmap.md`
- Child Plan(s): none
- Scope Label: pipeline-owner-slice
- Related Tickets/Issues: none
- Review Status: reviewed
- Review Notes: Local review completed on 2026-04-08. The slice is narrow enough to avoid scope drift and broad enough to justify its own document because it contains three meaningful phases and acts as the gate for later roadmap slices.

## 1. Background
- `IndexCoordinator` now owns request allocation and lifecycle helpers, but `pipeline.rs` is still fundamentally an `impl FlistWalkerApp` extension.
- The remaining architecture issue is not just duplication; it is ownership shape. Pipeline orchestration still reads as a large extension module instead of an independent owner.
- This slice is meant to create a narrower, clearer ownership seam before tackling background result flows, worker protocols, or test reshaping.

## 2. Goal
- Move pipeline orchestration toward a dedicated owner boundary.
- Reduce direct `FlistWalkerApp` field mutation inside `pipeline.rs`.
- Create a shape that later slices can build on instead of working around.
- Slice exit criteria:
  - pipeline orchestration no longer reads primarily as a broad `impl FlistWalkerApp` extension
  - a dedicated owner surface exists and is used in the main pipeline path
  - architecture/design/task docs are synchronized before the roadmap advances

## 3. Scope
### In Scope
- `rust/src/app/pipeline.rs`
- `rust/src/app/mod.rs`
- `rust/src/app/index_coordinator.rs`
- related app tests and architecture/design docs if the ownership boundary changes materially

### Out of Scope
- background tab specialization beyond what is necessary to support the new owner boundary
- worker protocol separation
- broad test-suite restructuring
- tracing redesign

## 4. Constraints and Assumptions
- The slice must keep behavior unchanged from the user perspective.
- Request freshness, cancellation, and UI responsiveness guarantees remain mandatory.
- This slice should produce at least two meaningful phases so the roadmap/slice split remains justified.

## 5. Current Risks
- Risk:
  - Ownership extraction becomes a rename/move exercise without reducing `FlistWalkerApp` coupling.
  - Impact:
    - The slice consumes time but does not improve the architecture.
  - Mitigation:
    - Treat field-touch reduction and API narrowing as completion criteria, not just file creation.
- Risk:
  - Pipeline owner extraction accidentally spills into background tab or worker protocol concerns.
  - Impact:
    - Scope drift and unstable slice boundaries.
  - Mitigation:
    - Keep this slice focused on index/search pipeline ownership only.

## 6. Execution Strategy
1. Phase 1: Identify and isolate pipeline owner seams
   - Files/modules/components: `rust/src/app/pipeline.rs`, `rust/src/app/mod.rs`
   - Expected result: clear grouping of pipeline-owned operations and data flow boundaries.
   - Verification: `cd rust && cargo test`
2. Phase 2: Introduce and route through a dedicated owner surface
   - Files/modules/components: `rust/src/app/pipeline.rs`, possibly a new owner type/module, `rust/src/app/mod.rs`, `rust/src/app/index_coordinator.rs`
   - Expected result: `FlistWalkerApp` calls into a narrower owner API instead of directly hosting most pipeline orchestration.
   - Verification: `cd rust && cargo test`; run perf guards if VM-003 applies.
3. Phase 3: Sync docs and close the slice
   - Files/modules/components: `docs/ARCHITECTURE.md`, `docs/DESIGN.md`, `docs/TASKS.md`
   - Expected result: steady-state docs describe the new ownership boundary, and the roadmap can advance to the next slice.
   - Verification: docs diff review plus the same automated validation used for the code changes.

## 7. Detailed Task Breakdown
- [ ] Map the current pipeline-owned behaviors that still live as broad `FlistWalkerApp` methods
- [ ] Introduce a dedicated owner surface for pipeline orchestration
- [ ] Reduce direct field mutation from `FlistWalkerApp` in the pipeline path
- [ ] Update tests as needed to lock the new ownership boundary
- [ ] Sync architecture/design/task docs after the code settles

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test`
  - VM-003 perf guards if indexing-path coverage applies
- Manual checks:
  - Only if visible search/index/tab behavior changes
- Performance or security checks:
  - Preserve the existing ignored perf budgets
  - Preserve request freshness and worker-only heavy work
- Regression focus:
  - active/background pipeline routing
  - stale response handling
  - incremental search/index refresh behavior

## 9. Rollback Plan
- Revert the owner extraction independently if it destabilizes app behavior.
- Keep already-merged lifecycle helper improvements unless the new owner surface depends on them in a breaking way.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `app-architecture-roadmap`, read `docs/CHANGE-PLAN-20260408-app-architecture-roadmap.md` and then `docs/CHANGE-PLAN-20260408-pipeline-owner-slice.md` before starting implementation.
- Execute the work in the documented order unless the roadmap or active slice is updated first.
- If scope, order, or risk changes, update the relevant change plan before continuing.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-08 00:00 Created as the active slice for the next architecture-improvement cycle.
- 2026-04-08 00:00 Local slice review completed. Confirmed that the slice boundary is valid and that later concerns remain deferred to roadmap slices B-E.
- 2026-04-08: Phase 1 completed. Active request cleanup now routes through `IndexCoordinator`, and search refresh request/response routing is grouped around `SearchCoordinator` lifecycle helpers plus pipeline-local response handlers. `cd rust && cargo test` passed after the seam extraction.

## 12. Completion Checklist
- [x] Planned document created before implementation
- [x] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, and `TESTPLAN.md` as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 13. Final Notes
- This slice is intentionally narrow. Background-result separation, worker protocols, test reshaping, and tracing are deferred to later roadmap slices.

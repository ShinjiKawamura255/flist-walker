# CHANGE PLAN: Worker Protocol Separation Slice

## Metadata
- Date: 2026-04-08
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice
- Parent Plan: `docs/CHANGE-PLAN-20260408-app-architecture-roadmap.md`
- Child Plan(s): none
- Scope Label: worker-protocol-separation-slice
- Related Tickets/Issues: none
- Review Status: reviewed
- Review Notes: Slice review completed on 2026-04-08 after the parent roadmap was judged `feasible`. The slice stays distinct from Slice B because it focuses on protocol/module boundaries rather than background tab result-flow ownership, and it stays distinct from Slice D because it does not pull command-oriented test restructuring into scope. Convergence review completed on 2026-04-09 after rechecking alignment with the updated `plan-driven-changes` skill.

## 1. Background
- Slice A extracted `pipeline_owner.rs` and Slice B separated background tab result/index application so that pipeline and tab orchestration now have clearer owner seams.
- The remaining structural hotspot is `rust/src/app/workers.rs`, where worker request/response protocol types still live alongside several worker implementations.
- This slice exists to separate protocol definitions from worker implementations before command-oriented test reshaping and tracing consolidation.

## 2. Goal
- Separate worker request/response protocols from worker implementation modules.
- Reduce cross-concern coupling inside `workers.rs`.
- Leave clearer import and ownership boundaries for later test and tracing slices.
- Slice exit criteria:
  - worker request/response protocol types are defined in narrower protocol-oriented modules than the current mixed `workers.rs`
  - worker implementation modules depend on protocols without co-owning all protocol definitions in one file
  - steady-state docs and roadmap/task tracking are synchronized before advancing

## 3. Scope
### In Scope
- `rust/src/app/workers.rs`
- new protocol-oriented module(s) under `rust/src/app/`
- `rust/src/app/mod.rs`
- related imports/tests/docs if module boundaries move materially

### Out of Scope
- new worker behavior changes
- broad app-layer command test restructuring
- tracing redesign
- search/query contract changes

## 4. Constraints and Assumptions
- Behavior must remain unchanged from the user perspective.
- Existing worker threading, request freshness, and shutdown semantics remain mandatory.
- This slice should separate protocol ownership, not reopen Slice A/B ownership work.

## 4.5 Slice Review Alignment
- Parent feasibility basis:
  - The roadmap already confirmed this work is technically feasible within existing app-layer seams and current validation coverage.
- Why this slice remains the right active child plan:
  - It still has multiple meaningful phases with separate extraction, import-routing, and doc-sync boundaries.
  - It is bounded to worker protocol ownership, so it does not collapse into the prior background-tab slice or the later command-test slice.
  - It does not require a third planning layer because the current phases are still small enough to execute and verify without introducing a `subslice`.

## 5. Current Risks
- Risk:
  - Protocol separation becomes a file shuffle without clarifying ownership.
  - Impact:
    - The slice adds files but leaves dependency boundaries muddy.
  - Mitigation:
    - Use protocol-vs-implementation boundaries as the completion criterion, not file count.
- Risk:
  - Moving protocol types breaks imports across app modules and tests in a noisy way.
  - Impact:
    - The slice causes broad mechanical churn and obscures reviewability.
  - Mitigation:
    - Move bounded protocol groups deliberately and keep behavior changes out of scope.

## 6. Execution Strategy
1. Phase 1: Isolate current worker protocol seams
   - Files/modules/components: `rust/src/app/workers.rs`, `rust/src/app/mod.rs`
   - Expected result: the current request/response protocol groups and their consumers are mapped and grouped for safe extraction.
   - Verification: `cd rust && cargo test`
2. Phase 2: Introduce protocol-oriented modules and route imports through them
   - Files/modules/components: `rust/src/app/workers.rs`, new protocol module(s), `rust/src/app/mod.rs`, related consumers/tests
   - Expected result: protocol types live outside broad implementation files and worker implementations import the narrower protocol surfaces.
   - Verification: `cd rust && cargo test`; run perf guards if VM-003 applies.
3. Phase 3: Sync docs and close the slice
   - Files/modules/components: `docs/ARCHITECTURE.md`, `docs/DESIGN.md`, `docs/TASKS.md`
   - Expected result: steady-state docs describe the new protocol/module boundary and roadmap tracking advances cleanly.
   - Verification: docs diff review plus the same automated validation used for code changes.

## 7. Detailed Task Breakdown
- [x] Map protocol groups and their consumers inside the current worker modules
- [x] Extract narrower protocol-oriented module(s)
- [x] Update imports and tests without changing worker behavior
- [x] Sync architecture/design/task docs after the code settles

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test`
  - VM-003 perf guards if indexing-path coverage applies
- Manual checks:
  - none expected unless visible worker behavior changes unexpectedly
- Performance or security checks:
  - Preserve existing perf guards and worker shutdown semantics
  - Preserve request freshness and async routing behavior
- Regression focus:
  - worker request/response routing
  - indexing-path worker coverage
  - import/module boundary correctness

## 9. Rollback Plan
- Revert protocol-module extraction independently if import churn or worker routing regressions appear.
- Keep Slice A/B ownership changes intact unless rollback proves a hard dependency.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `app-architecture-roadmap`, read `docs/CHANGE-PLAN-20260408-app-architecture-roadmap.md` and then `docs/CHANGE-PLAN-20260408-worker-protocol-separation-slice.md` before starting implementation.
- Execute the work in the documented order unless the roadmap or active slice is updated first.
- If scope, order, or risk changes, update the relevant change plan before continuing.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-08 00:00 Created as the active slice after Slice B completed.
- 2026-04-08 00:00 Review completed. Confirmed the parent-child handoff, active slice references, and phase boundaries are coherent after updating roadmap/AGENTS/TASKS to point at Slice C.
- 2026-04-09: Rechecked against the updated `plan-driven-changes` skill. Kept this as a 2-level slice with no `subslice`, because the three existing phases still provide sufficient execution and validation boundaries.
- 2026-04-09: Phase 1 mapped the remaining worker protocol seams and confirmed `workers.rs`, `index_worker.rs`, `worker_bus.rs`, and `mod.rs` as the bounded extraction consumers.
- 2026-04-09: Phase 2 introduced `rust/src/app/worker_protocol.rs` and rerouted worker protocol imports through the new module without changing worker behavior.
- 2026-04-09: Phase 3 synced `ARCHITECTURE.md`, `DESIGN.md`, and `TASKS.md`; `cargo test` passed after the module split.

## 12. Completion Checklist
- [x] Planned document created before implementation
- [x] Temporary `AGENTS.md` rule updated to point at this active slice
- [x] Work executed according to the plan or the plan updated first
- [x] Verification completed
- [x] Lasting requirements/spec/design/test updates moved into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, and `TESTPLAN.md` as needed
- [ ] Temporary `AGENTS.md` rule removed after roadmap completion
- [ ] Change plan deleted after roadmap completion

## 13. Final Notes
- This slice assumes Slice A/B are stable. Command-test reshaping and structured tracing remain deferred to roadmap slices D-E.

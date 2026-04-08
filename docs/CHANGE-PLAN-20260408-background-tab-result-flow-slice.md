# CHANGE PLAN: Background Tab Result-Flow Slice

## Metadata
- Date: 2026-04-08
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice plan
- Parent Plan: `docs/CHANGE-PLAN-20260408-app-architecture-roadmap.md`
- Child Plan(s): none
- Scope Label: background-tab-result-flow-slice
- Related Tickets/Issues: none
- Review Status: reviewed
- Review Notes: Review completed on 2026-04-08. The slice stays distinct from Slice A because it focuses on active/background result application and restore routing, and it stays distinct from Slice C because it does not pull worker protocol extraction into scope.

## 1. Background
- Slice A introduced `pipeline_owner.rs` and reduced the amount of search/result orchestration hosted directly in `pipeline.rs`.
- The remaining coupling is concentrated in background tab result application, where active-tab and background-tab response handling still share dispatch surfaces and tab-state mutation paths.
- This slice exists to separate background tab result-flow handling before worker protocol extraction and command-oriented test reshaping.

## 2. Goal
- Clarify active-tab vs background-tab response application paths.
- Reduce the amount of background tab mutation that still happens inline inside broad pipeline/tabs flows.
- Leave a cleaner boundary for the later worker protocol and testing slices.
- Slice exit criteria:
  - background tab result/index response application is routed through clearer owner/helper boundaries than the current mixed flow
  - active/background handling reads as intentionally separate paths rather than one broad dispatcher
  - steady-state docs and roadmap/task tracking are synchronized before advancing

## 3. Scope
### In Scope
- `rust/src/app/pipeline.rs`
- `rust/src/app/pipeline_owner.rs`
- `rust/src/app/tabs.rs`
- `rust/src/app/tab_state.rs`
- related app tests and architecture/design/task docs if result-flow boundaries move materially

### Out of Scope
- worker protocol/module extraction
- broad command-test restructuring
- tracing redesign
- search/query contract changes

## 4. Constraints and Assumptions
- User-visible behavior must remain unchanged.
- Request freshness, stale response discard, and UI responsiveness remain mandatory.
- This slice should stay focused on background tab result/index flow separation and not absorb Slice C/D concerns.

## 5. Current Risks
- Risk:
  - Background tab flow extraction collapses back into generic helpers that still mix active/background concerns.
  - Impact:
    - The slice moves code but does not materially improve ownership clarity.
  - Mitigation:
    - Treat dedicated background-flow boundaries and reduced mixed branching as completion criteria.
- Risk:
  - Result-flow separation touches tab restore/session behavior and causes subtle regressions.
  - Impact:
    - Background tabs may lose preview/result/index snapshots on activation or restoration.
  - Mitigation:
    - Keep session/tab regression tests in the validation set and add focused coverage where seams move.

## 6. Execution Strategy
1. Phase 1: Isolate current background tab result-flow seams
   - Files/modules/components: `rust/src/app/pipeline.rs`, `rust/src/app/tabs.rs`
   - Expected result: the current background-only response and restore paths are explicitly grouped and easier to route independently.
   - Verification: `cd rust && cargo test`
2. Phase 2: Introduce clearer background result/index owner helpers
   - Files/modules/components: `rust/src/app/pipeline.rs`, `rust/src/app/pipeline_owner.rs`, `rust/src/app/tabs.rs`, `rust/src/app/tab_state.rs`
   - Expected result: active/background result application is split across narrower helpers/owners with less inline tab mutation in broad dispatchers.
   - Verification: `cd rust && cargo test`; run perf guards if VM-003 applies.
3. Phase 3: Sync docs and close the slice
   - Files/modules/components: `docs/ARCHITECTURE.md`, `docs/DESIGN.md`, `docs/TASKS.md`
   - Expected result: steady-state docs describe the new background-flow boundary and roadmap tracking advances cleanly.
   - Verification: docs diff review plus the same automated validation used for code changes.

## 7. Detailed Task Breakdown
- [ ] Map the current background tab response and restore paths that still mix with active-tab orchestration
- [ ] Introduce narrower helpers/owners for background result/index application
- [ ] Reduce mixed active/background branching in the main pipeline dispatch path
- [ ] Update tests as needed to lock background tab restore/result behavior
- [ ] Sync architecture/design/task docs after the code settles

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test`
  - VM-003 perf guards if indexing-path coverage applies
- Manual checks:
  - Focused GUI smoke only if visible tab activation/restore behavior changes
- Performance or security checks:
  - Preserve existing perf guards and worker-only heavy work
  - Preserve request freshness and background tab compaction/restore behavior
- Regression focus:
  - background tab search/index response routing
  - background tab activation restore
  - stale response discard
  - preview/result snapshot retention for inactive tabs

## 9. Rollback Plan
- Revert the background-flow separation independently if tab activation or restore behavior regresses.
- Keep Slice A owner-surface extraction intact unless the rollback proves a hard dependency.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `app-architecture-roadmap`, read `docs/CHANGE-PLAN-20260408-app-architecture-roadmap.md` and then `docs/CHANGE-PLAN-20260408-background-tab-result-flow-slice.md` before starting implementation.
- Execute the work in the documented order unless the roadmap or active slice is updated first.
- If scope, order, or risk changes, update the relevant change plan before continuing.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-08 00:00 Created as the active slice after Slice A completed.
- 2026-04-08 00:00 Review completed. Confirmed the parent-child handoff, active slice references, and phase boundaries are coherent after clearing stale checklist/task items.

## 12. Completion Checklist
- [x] Planned document created before implementation
- [x] Temporary `AGENTS.md` rule updated to point at this active slice
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, and `TESTPLAN.md` as needed
- [ ] Temporary `AGENTS.md` rule removed after roadmap completion
- [ ] Change plan deleted after roadmap completion

## 13. Final Notes
- This slice assumes Slice A is stable. Worker protocol extraction, command-test reshaping, and tracing work remain deferred to roadmap slices C-E.

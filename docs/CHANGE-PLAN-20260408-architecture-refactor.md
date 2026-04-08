# CHANGE PLAN: Architecture Refactor Follow-up

## Metadata
- Date: 2026-04-08
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 1
- Plan Role: single plan
- Parent Plan: none
- Child Plan(s): none
- Scope Label: architecture-refactor
- Related Tickets/Issues: none
- Review Status: not reviewed
- Review Notes: User requested a plan-driven refactor program after an architecture assessment. This is intentionally a single-plan program because the current work is one coherent refactoring track, not multiple concurrently managed slices.

## 1. Background
- The project has already improved its module split and documentation, but the current GUI app layer still concentrates too much ownership in `FlistWalkerApp`.
- `rust/src/app/mod.rs` remains a large coordinator/state owner, and several feature files are still implemented as `impl FlistWalkerApp` extensions rather than independently owned application services.
- `rust/src/app/pipeline.rs` still contains duplicated active/background tab state transitions, and `rust/src/app/workers.rs` still mixes worker runtime management, worker protocols, and concrete worker implementations.
- This structure is workable and well-tested, but it keeps future changes expensive and increases the chance of partial updates across similar flows.

## 2. Goal
- Reduce architectural debt in the GUI app layer without destabilizing the existing user-facing behavior.
- Establish clearer ownership boundaries for pipeline orchestration and worker implementations.
- Make future changes cheaper by reducing duplicated state transitions and narrowing the blast radius of worker-related edits.
- Keep the existing behavior, validation rules, and release expectations intact while updating architecture documentation to match the new steady state.

## 3. Scope
### In Scope
- `rust/src/app/` architecture refactoring focused on pipeline orchestration and worker modularization.
- Removal of duplicated active/background tab pipeline reset and request setup logic.
- Extraction of app-layer ownership seams so `FlistWalkerApp` becomes thinner as coordinator.
- Worker module restructuring that separates runtime management, protocols, and concrete worker behavior.
- Required updates to `docs/ARCHITECTURE.md`, `docs/DESIGN.md`, `docs/TESTPLAN.md`, and `docs/TASKS.md`.

### Out of Scope
- Search algorithm changes in `rust/src/search.rs`.
- FileList contract changes in `rust/src/indexer.rs`.
- New user-facing features, UI redesign, or release workflow changes.
- Broad renames or directory moves that do not directly support the refactoring goal.

## 4. Constraints and Assumptions
- Existing GUI/CLI behavior and query contracts must remain backward compatible.
- Request ordering, request_id-based stale response rejection, and UI responsiveness rules in `AGENTS.md` remain mandatory.
- The plan should progress incrementally, with tests kept green at each stage.
- Validation must follow `docs/TESTPLAN.md`, especially VM-002 and VM-003 for app/indexing-path edits.
- Changes to indexing-path files must also run the ignored perf regression tests required by project policy.
- This program is managed as a single plan unless the work later proves to require separately managed slices.

## 5. Current Risks
- Risk: Pipeline reset logic stays duplicated across active and background tabs.
  - Impact: Future behavior changes can update one path and miss the other, causing subtle regressions.
  - Mitigation: Refactor pipeline pre-request state resets into shared helpers or owner-level APIs before broader restructuring.
- Risk: `FlistWalkerApp` remains the implicit owner of too many transitions.
  - Impact: Module split appears cleaner than the actual ownership structure, making future maintenance harder.
  - Mitigation: Introduce narrower owner boundaries and make `mod.rs` call them rather than directly owning every transition.
- Risk: `workers.rs` continues to mix runtime, protocol, and execution details.
  - Impact: Worker changes remain noisy, harder to review, and more likely to regress unrelated worker flows.
  - Mitigation: Split worker responsibilities by concern after the pipeline changes stabilize.
- Risk: Refactoring drifts away from the documented architecture.
  - Impact: Docs become aspirational instead of descriptive, reducing trust in design materials.
  - Mitigation: Update architecture/design/test docs in the same program and close the gap before removing the temporary rule.

## 6. Execution Strategy
1. Phase 1: Pipeline state-transition consolidation
   - Files/modules/components: `rust/src/app/pipeline.rs`, `rust/src/app/index_coordinator.rs`, `rust/src/app/tab_state.rs`, related app tests
   - Expected result: Active/background tab index refresh logic shares common reset/setup seams, reducing duplicated state mutation.
   - Verification: `cd rust && cargo test`; if indexing-path files are touched, run the two ignored perf tests required by `AGENTS.md`.
2. Phase 2: Pipeline ownership extraction
   - Files/modules/components: `rust/src/app/mod.rs`, `rust/src/app/pipeline.rs`, `rust/src/app/search_coordinator.rs`, related app tests and docs
   - Expected result: `FlistWalkerApp` delegates more orchestration behavior through narrower ownership boundaries, and `mod.rs` becomes thinner as coordinator.
   - Verification: `cd rust && cargo test`; targeted GUI smoke checks if input/index/search interaction semantics move.
3. Phase 3: Worker modularization and documentation closure
   - Files/modules/components: `rust/src/app/workers.rs`, `rust/src/app/worker_bus.rs`, `rust/src/app/worker_support.rs`, `docs/ARCHITECTURE.md`, `docs/DESIGN.md`, `docs/TESTPLAN.md`, `docs/TASKS.md`
   - Expected result: Worker runtime, protocols, and concrete worker implementations are split into clearer ownership units, and steady-state docs describe the final structure.
   - Verification: `cd rust && cargo test`; required ignored perf tests; docs diff review for architecture/testplan consistency.

## 7. Detailed Task Breakdown
- [ ] Create the temporary plan-driven rule and keep implementation aligned with this plan
- [ ] Consolidate duplicated pipeline reset/request setup flows for active/background tabs
- [ ] Extract narrower pipeline ownership seams so `FlistWalkerApp` loses direct orchestration responsibilities
- [ ] Split `workers.rs` by concern without changing public behavior
- [ ] Update architecture/design/test docs to reflect the final steady state
- [ ] Remove the temporary `AGENTS.md` rule and delete this plan after completion

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test`
  - When VM-003 applies: `cargo test perf_regression_filelist_stream_matches_v0123_reference_budget --lib -- --ignored --nocapture`
  - When VM-003 applies: `cargo test perf_walker_classification_is_faster_than_eager_metadata_resolution --lib -- --ignored --nocapture`
- Manual checks:
  - If render/input/index interaction changes, run the structural refactoring GUI smoke test items relevant to search, root switching, tabs, and Create File List.
- Performance or security checks:
  - Preserve the existing request_id freshness guarantees and background-worker-only heavy operations.
  - Watch for regressions in indexing responsiveness and worker shutdown behavior.
- Regression focus:
  - active vs background tab refresh parity
  - stale response rejection
  - FileList/Create File List workflows
  - worker shutdown/join behavior

## 9. Rollback Plan
- Phase 1, Phase 2, and Phase 3 should each remain independently revertible where practical.
- If Phase 2 ownership extraction destabilizes behavior, revert to the Phase 1-consolidated pipeline state rather than partially keeping broken ownership seams.
- If worker modularization causes instability, revert the worker split while keeping earlier pipeline improvements if they remain green.
- Do not roll back docs independently from the code state they describe; keep docs synchronized with the active implementation.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `architecture-refactor`, read `docs/CHANGE-PLAN-20260408-architecture-refactor.md` before starting implementation.
- Execute the work in the documented order unless the plan is updated first.
- If scope, order, or risk changes, update the change plan before continuing.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-08 00:00 Planned single-plan architecture refactor program after project-wide architecture review.
- 2026-04-08 00:00 Phase 1 started. Consolidated index request-id allocation and refresh-state reset helpers in `rust/src/app/pipeline.rs`, and added regression coverage for Create File List walker refresh state reset.
- 2026-04-08 00:00 Validation passed for the Phase 1 helper extraction: `cd rust && cargo test`, `cargo test perf_regression_filelist_stream_matches_v0123_reference_budget --lib -- --ignored --nocapture`, and `cargo test perf_walker_classification_is_faster_than_eager_metadata_resolution --lib -- --ignored --nocapture`.
- 2026-04-08 00:00 Phase 1 completed. `pipeline.rs` now shares request-id allocation, active/background refresh setup, and terminal cleanup helpers so the next step can focus on ownership extraction instead of repeated state-transition edits.

## 12. Completion Checklist
- [x] Planned document created before implementation
- [x] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, and `TESTPLAN.md` as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 13. Final Notes
- Before deleting this plan, move any lasting decisions into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, or `TESTPLAN.md`.

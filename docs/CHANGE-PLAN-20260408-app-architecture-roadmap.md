# CHANGE PLAN: App Architecture Improvement Roadmap

## Metadata
- Date: 2026-04-08
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: roadmap
- Parent Plan: none
- Child Plan(s): `docs/CHANGE-PLAN-20260409-command-oriented-app-tests-slice.md`
- Scope Label: app-architecture-roadmap
- Related Tickets/Issues: none
- Review Status: reviewed
- Review Notes: Feasibility review completed on 2026-04-08 and judged this program `feasible` because the remaining architecture work stays inside existing app-layer seams, can be validated with the current Rust test/perf coverage, and preserves per-slice rollback boundaries. Layered review then confirmed the roadmap/slice split is valid because the roadmap manages five distinct future slices and the active slice has multiple execution phases. Convergence review completed on 2026-04-08 after making slice status, activation order, and roadmap-closure expectations explicit.

## 1. Background
- The previous architecture refactor reduced duplication in pipeline state transitions, moved index request lifecycle ownership into `IndexCoordinator`, and split worker runtime/indexing concerns out of `workers.rs`.
- The current structure is materially better, but the next set of improvements spans multiple independent areas and should be tracked explicitly instead of being discovered incrementally.
- The requested next-step themes are:
  - pipeline owner extraction
  - background tab result-flow separation
  - worker protocol separation
  - command-oriented app-layer tests
  - stronger structured tracing/supportability

## 2. Goal
- Maintain a visible, ordered roadmap for the next app-architecture improvements.
- Execute each improvement as a separate slice with clear boundaries, validation, and closure criteria.
- Avoid reintroducing a large one-shot refactor; instead, move through bounded slices while keeping the project continuously releasable.

## 3. Scope
### In Scope
- Architecture and maintainability improvements in `rust/src/app/` and adjacent docs.
- Slice-by-slice planning and execution order for the next five improvements.
- Temporary workflow rules needed to keep implementation aligned with the roadmap and active slice.

### Out of Scope
- New user-facing features unrelated to architecture cleanup.
- Search/query contract changes unless a later slice explicitly requires them.
- Release workflow or updater changes unless a later slice explicitly requires them.

## 4. Constraints and Assumptions
- Each slice must remain independently verifiable and revertible where practical.
- The roadmap should track all five requested improvements, but only one active slice should drive implementation at a time.
- Validation must keep following `docs/TESTPLAN.md`, especially VM-002 and VM-003 when applicable.
- Temporary plan rules should be removed after the roadmap and active slice are fully closed.

## 4.5 Feasibility Review
- Status: `feasible`
- Checked assumptions:
  - The requested architecture work is confined to existing `rust/src/app/` ownership seams and does not require new external dependencies or release-process changes.
  - The current docs/test harness already provide a workable validation path for each slice via `cargo test` plus VM-003 perf guards when indexing paths move.
  - Per-slice rollback remains practical because each planned slice is scoped to a single architectural concern.
- Why this is feasible now:
  - Slice A/B already created stable owner seams, so Slice C-E can build on bounded app-layer modules instead of reopening the earlier refactor.
  - No unresolved migration or compatibility prerequisite blocks Slice C from starting.
  - The remaining risk is mainly scope drift, which is mitigated by keeping one concern per slice and updating the roadmap before any order/scope change.

## 5. Current Risks
- Risk:
  - A roadmap is created but the active slice is not specific enough to drive safe implementation.
  - Impact:
    - The roadmap becomes descriptive only and fails to constrain changes.
  - Mitigation:
    - Keep a dedicated active slice document with concrete phases, validation, and completion criteria.
- Risk:
  - Multiple architecture concerns are mixed in one slice.
  - Impact:
    - Scope drift and partial completion.
  - Mitigation:
    - Keep one concern per slice and update roadmap status as each slice closes.

## 6. Execution Strategy
1. Slice A: Pipeline Owner Extraction
   - Files/modules/components: `rust/src/app/pipeline.rs`, `rust/src/app/mod.rs`, `rust/src/app/index_coordinator.rs`, related tests
   - Expected result: `pipeline` logic moves toward a dedicated owner boundary instead of staying an `impl FlistWalkerApp` extension.
   - Verification: `cd rust && cargo test`; VM-002/VM-003 as applicable.
2. Slice B: Background Tab Result-Flow Separation
   - Files/modules/components: `rust/src/app/pipeline.rs`, `rust/src/app/tabs.rs`, `rust/src/app/tab_state.rs`, related tests
   - Expected result: active/background response application paths become clearer and less entangled.
   - Verification: `cd rust && cargo test`; GUI smoke checks if tab behavior changes.
3. Slice C: Worker Protocol Separation
   - Files/modules/components: `rust/src/app/workers.rs`, new protocol module(s), related imports/tests
   - Expected result: worker request/response protocols separate from worker implementations.
   - Verification: `cd rust && cargo test`; VM-003 perf guards when indexing-path coverage applies.
4. Slice D: Command-Oriented App Tests
   - Files/modules/components: `rust/src/app/tests/*`, owner/command surfaces introduced by earlier slices
   - Expected result: tests target command/owner boundaries more directly and depend less on large app fixtures.
   - Verification: `cd rust && cargo test`.
5. Slice E: Structured Tracing and Supportability
   - Files/modules/components: app owners, workers, diagnostics docs
   - Expected result: request_id-correlated tracing is more uniform, support/debug workflows get clearer event surfaces, and this final slice also records whether the roadmap goal is now satisfied or whether a follow-up slice must be added.
   - Verification: `cd rust && cargo test`; docs review for supportability notes.

### Slice Status Matrix
| Slice | Focus | Status | Activation / Exit Notes |
| --- | --- | --- | --- |
| Slice A | Pipeline Owner Extraction | DONE | Completed on 2026-04-08 after owner-surface extraction and steady-state doc sync. |
| Slice B | Background Tab Result-Flow Separation | DONE | Completed on 2026-04-08 after background search/index apply helpers and restore boundaries were separated. |
| Slice C | Worker Protocol Separation | DONE | Completed on 2026-04-09 after protocol types moved into `worker_protocol.rs` and steady-state docs were synchronized. |
| Slice D | Command-Oriented App Tests | DONE | Completed on 2026-04-09 after update/restore regressions moved into owner-aligned test modules and steady-state docs were synchronized. |
| Slice E | Structured Tracing and Supportability + Roadmap Closure Review | PLANNED | Next slice candidate after Slice D. Prefer after ownership, protocol, and test boundaries settle, so tracing lands on stable surfaces; this terminal slice must also decide whether the roadmap closes or continues with a newly added slice. |

## 7. Detailed Task Breakdown
- [x] Add roadmap and active slice documents before implementation resumes
- [x] Execute Slice A and update roadmap status
- [x] Execute Slice B and update roadmap status
- [x] Execute Slice C and update roadmap status
- [x] Execute Slice D and update roadmap status
- [ ] Execute Slice E and update roadmap status
- [ ] Record roadmap goal attainment and close or extend the roadmap at Slice E exit
- [ ] Remove temporary planning rules after roadmap closure

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test`
  - Run ignored perf guards whenever VM-003 applies
- Manual checks:
  - Run focused GUI smoke checks only for slices that affect visible tab/input/render behavior
- Performance or security checks:
  - Preserve UI responsiveness and request freshness guarantees
  - Preserve worker shutdown behavior and indexing performance budgets
- Regression focus:
  - active/background tab parity
  - index/search request lifecycle
  - worker modular boundaries
  - diagnostics consistency

## 9. Rollback Plan
- Roll back per slice rather than across the whole roadmap whenever possible.
- If a slice changes shared app boundaries but destabilizes tests, revert that slice and keep the roadmap active.
- Do not remove the roadmap or slice documents until the program is either completed or intentionally abandoned with closure notes.

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
- 2026-04-08 00:00 Planned roadmap for the next five app-architecture improvements and selected Slice A as the active starting point.
- 2026-04-08 00:00 Feasibility review judged the program feasible with current app-layer seams, validation coverage, and per-slice rollback boundaries.
- 2026-04-08 00:00 Layered roadmap review completed. Added explicit slice status/activation tracking and confirmed the 2-level plan split is justified.
- 2026-04-08: Slice A Phase 1 completed. The active request cleanup seam now routes through `IndexCoordinator`, and search refresh request/response routing is grouped around `SearchCoordinator` lifecycle helpers plus pipeline-local response handlers; later slice phases remain active.
- 2026-04-08: Slice A Phase 2 completed. `pipeline_owner.rs` now carries the dedicated owner surface for search/result refresh and entry-filter application, reducing direct pipeline orchestration inside `pipeline.rs`.
- 2026-04-08: Slice A Phase 3 completed. Steady-state architecture/design docs now describe `pipeline_owner.rs` as the search/result refresh owner surface and `pipeline.rs` as the thinner dispatcher layer.
- 2026-04-08: Activated Slice B and created `docs/CHANGE-PLAN-20260408-background-tab-result-flow-slice.md` as the new active child plan.
- 2026-04-08: Slice B handoff reviewed. Cleared stale roadmap task state and confirmed the new child plan/AGENTS/TASKS references are aligned.
- 2026-04-08: Slice B completed. Background search/index apply helpers now live in `tabs.rs`, active/background branching in pipeline paths is reduced, and steady-state docs describe the new boundary.
- 2026-04-08: Activated Slice C and created `docs/CHANGE-PLAN-20260408-worker-protocol-separation-slice.md` as the new active child plan.
- 2026-04-08: Slice C handoff reviewed. Confirmed the active child-plan references are aligned and the slice stays distinct from both Slice B and Slice D.
- 2026-04-09: Rechecked this roadmap against the updated `plan-driven-changes` skill. Kept the 2-level plan, documented the feasibility review explicitly, and marked Slice E as the terminal slice that must record roadmap closure or continuation.
- 2026-04-09: Slice C completed. Worker request/response protocols now live in `worker_protocol.rs`, the remaining worker modules import that narrower surface, and steady-state docs were updated before leaving the slice.
- 2026-04-09: Activated Slice D and created `docs/CHANGE-PLAN-20260409-command-oriented-app-tests-slice.md` as the new active child plan.
- 2026-04-09: Slice D handoff reviewed. Confirmed the active child-plan references are aligned and the slice stays distinct from both Slice C and Slice E.
- 2026-04-09: Slice D completed. Update command coverage now lives in `rust/src/app/tests/update_commands.rs`, session restore/startup-root coverage lives in `rust/src/app/tests/session_restore.rs`, and steady-state docs now describe the owner/command-oriented app test boundaries. During validation, an unrelated updater env race surfaced and was fixed by aligning the remaining self-update disable tests with the existing env lock.

## 12. Completion Checklist
- [x] Planned document created before implementation
- [x] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, and `TESTPLAN.md` as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 13. Final Notes
- The roadmap is the parent plan. The active slice carries the implementation-level phases and should be updated before code changes resume.

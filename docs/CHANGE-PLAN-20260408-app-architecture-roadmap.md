# CHANGE PLAN: App Architecture Improvement Roadmap

## Metadata
- Date: 2026-04-08
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: roadmap
- Parent Plan: none
- Child Plan(s): `docs/CHANGE-PLAN-20260408-pipeline-owner-slice.md`
- Scope Label: app-architecture-roadmap
- Related Tickets/Issues: none
- Review Status: reviewed
- Review Notes: Local review completed on 2026-04-08. The roadmap/slice split is valid because the roadmap manages five distinct future slices and the active slice has multiple execution phases. The roadmap was updated during review to make slice status and activation order explicit.

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
   - Expected result: request_id-correlated tracing is more uniform and support/debug workflows get clearer event surfaces.
   - Verification: `cd rust && cargo test`; docs review for supportability notes.

### Slice Status Matrix
| Slice | Focus | Status | Activation / Exit Notes |
| --- | --- | --- | --- |
| Slice A | Pipeline Owner Extraction | DONE | Completed on 2026-04-08 after owner-surface extraction and steady-state doc sync. |
| Slice B | Background Tab Result-Flow Separation | PLANNED | Activate after Slice A stabilizes the pipeline owner surface. |
| Slice C | Worker Protocol Separation | PLANNED | Activate after Slice B or earlier only if Slice A produces a protocol-friendly boundary without destabilizing tabs. |
| Slice D | Command-Oriented App Tests | PLANNED | Activate after ownership seams exist to test against. |
| Slice E | Structured Tracing and Supportability | PLANNED | Prefer after ownership and protocol boundaries settle, so tracing lands on stable surfaces. |

## 7. Detailed Task Breakdown
- [ ] Add roadmap and active slice documents before implementation resumes
- [ ] Execute Slice A and update roadmap status
- [ ] Execute Slice B and update roadmap status
- [ ] Execute Slice C and update roadmap status
- [ ] Execute Slice D and update roadmap status
- [ ] Execute Slice E and update roadmap status
- [ ] Close the roadmap and remove temporary planning rules

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
- For `app-architecture-roadmap`, read `docs/CHANGE-PLAN-20260408-app-architecture-roadmap.md` and then `docs/CHANGE-PLAN-20260408-pipeline-owner-slice.md` before starting implementation.
- Execute the work in the documented order unless the roadmap or active slice is updated first.
- If scope, order, or risk changes, update the relevant change plan before continuing.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-08 00:00 Planned roadmap for the next five app-architecture improvements and selected Slice A as the active starting point.
- 2026-04-08 00:00 Local roadmap review completed. Added explicit slice status/activation tracking and confirmed the 2-level plan split is justified.
- 2026-04-08: Slice A Phase 1 completed. The active request cleanup seam now routes through `IndexCoordinator`, and search refresh request/response routing is grouped around `SearchCoordinator` lifecycle helpers plus pipeline-local response handlers; later slice phases remain active.
- 2026-04-08: Slice A Phase 2 completed. `pipeline_owner.rs` now carries the dedicated owner surface for search/result refresh and entry-filter application, reducing direct pipeline orchestration inside `pipeline.rs`.
- 2026-04-08: Slice A Phase 3 completed. Steady-state architecture/design docs now describe `pipeline_owner.rs` as the search/result refresh owner surface and `pipeline.rs` as the thinner dispatcher layer.

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

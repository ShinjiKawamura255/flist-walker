# CHANGE PLAN: Worker and Domain Modularization Slice

## Metadata
- Date: 2026-04-08
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice plan
- Parent Plan: `docs/CHANGE-PLAN-20260408-improvement-roadmap.md`
- Child Plan(s): none
- Scope Label: improvement-worker-domain
- Related Tickets/Issues: none
- Review Status: reviewed
- Review Notes: Draft created after Slice A completion. Slice scope is narrowed to worker routing plus index/search boundary work; updater/OS hardening is deferred to later slices.

## 1. Background
- Slice A completed the coordinator compression work and left `app/mod.rs` in a cleaner orchestration role.
- The next structural bottleneck is the worker/domain layer: `workers.rs`, `indexer.rs`, and `search.rs` still concentrate multiple responsibilities and make boundary reasoning harder than it should be.
- `updater.rs` is intentionally deferred to the later OS integration hardening slice so this slice stays bounded.
- This slice exists to separate worker routing from domain logic so later OS integration hardening, perf gate tightening, and diagnostics work can be isolated cleanly.

## 2. Goal
- Establish clear module boundaries for worker orchestration and domain logic without changing user-visible behavior.
- Reduce the amount of cross-cutting logic living in `workers.rs` by moving domain-specific helpers behind stable module seams.
- Make the following outcomes observable:
  - worker request/response contracts are localized,
  - indexing and search domain helpers are easier to reason about independently,
  - updater-related worker glue remains out of scope for this slice and is handled later.

## 3. Scope
### In Scope
- Worker routing and request/response contract boundaries
- `workers.rs` responsibility reduction
- `indexer.rs` / `search.rs` domain seam clarification
- docs and validation matrix synchronization for the new module boundaries

### Out of Scope
- `updater.rs` worker glue separation
- OS integration hardening beyond the current worker/domain boundary work
- perf gate workflow redesign
- diagnostics/tracing enhancements
- docs information architecture overhaul

## 4. Constraints and Assumptions
- Slice A is complete and serves as the dependency base for this slice.
- UI responsiveness constraints remain unchanged; no synchronous work may be moved onto the UI thread.
- request_id semantics and stale-response suppression must remain intact while boundaries are moved.
- If this slice affects indexing paths, the perf regression checks in `docs/TESTPLAN.md` must be updated in the same change.
- Phase definitions must remain executable batches, not inventory-only steps.

## 5. Current Risks
- Risk:
  - Moving worker routing boundaries too aggressively could couple unrelated worker flows.
  - Impact:
    - Search, index, preview, update, or action responses could share the wrong lifecycle or invalidation rules.
  - Mitigation:
    - Keep each phase aligned to one boundary type and verify request routing after each boundary move.
- Risk:
  - Domain extraction could accidentally drag in coordinator or updater concerns again.
  - Impact:
    - The worker/domain split would be visually cleaner but still hard to maintain.
  - Mitigation:
    - Keep `app/mod.rs` out of the phase except for required callsite updates and regression sync.
- Risk:
  - The slice could expand into OS hardening or perf policy changes before the module seams stabilize.
  - Impact:
    - The roadmap order would blur and later slices would lose their dependency base.
  - Mitigation:
    - Defer OS-specific hardening, updater glue, and perf workflow changes to later slices.

## 6. Execution Strategy
1. Phase 1: Separate Worker Routing from Domain Helpers
   - Files/modules/components:
     - `rust/src/app/workers.rs`
     - `rust/src/app/worker_bus.rs`
     - `rust/src/app/pipeline.rs`
     - `rust/src/app/search_coordinator.rs`
     - `rust/src/app/index_coordinator.rs`
   - Expected result:
     - Worker routing and lifecycle glue are localized, and shared request/response contracts are easier to trace.
   - Verification:
     - `cargo test`
     - request routing regression review
2. Phase 2: Clarify Index/Search Domain Boundaries and Sync Docs
   - Files/modules/components:
     - `rust/src/indexer.rs`
     - `rust/src/search.rs`
     - `rust/src/query.rs`
     - `rust/src/entry.rs`
     - `docs/ARCHITECTURE.md`
     - `docs/DESIGN.md`
     - `docs/TESTPLAN.md`
     - `docs/TASKS.md`
   - Expected result:
     - Indexing and search helpers are separated enough that their responsibilities can be described without referencing worker glue.
   - Verification:
     - `cargo test`
     - indexing/search regression review

## 7. Detailed Task Breakdown
- [x] Define the worker routing boundary that should move out of `workers.rs`
- [x] Decide which request/response helpers belong with `worker_bus`, `pipeline`, or coordinator-owned modules
- [ ] Separate the index/search domain helpers from worker glue
- [ ] Sync architecture/design/test docs with the new module boundaries
- [ ] Reconfirm regression coverage for request routing and indexing/search flows

## 8. Validation Plan
- Automated tests:
  - `cargo test`
- Manual checks:
  - Worker/domain ownership is readable from the module layout
  - Routing changes do not regress request_id or stale-response handling
- Performance or security checks:
  - If indexing paths move, run the ignored perf tests required by the current validation matrix
- Regression focus:
  - request routing
  - stale response suppression
  - indexing/search separation
  - updater worker isolation is intentionally deferred to the next slice

## 9. Rollback Plan
- Boundary moves should stay small enough to revert independently.
- Docs changes must revert together with any corresponding module move.
- If a phase grows beyond the intended boundary, stop and update the roadmap before continuing.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `improvement-roadmap`, read `[docs/CHANGE-PLAN-20260408-improvement-roadmap.md]` and `[docs/CHANGE-PLAN-20260408-worker-domain-modularization-slice.md]` before starting implementation.
- Execute the Worker and Domain Modularization work in the documented phase order unless the roadmap or slice plan is updated first.
- If scope, order, or risk changes, update the relevant change plan before continuing.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-08 00:00 Drafted after Slice A completion.
- 2026-04-08 00:00 Intended to become the next active child slice of the improvement roadmap.
- 2026-04-08 00:00 Narrowed after review so updater/OS hardening stays out of scope.
- 2026-04-08 00:00 Phase 1 completed by extracting worker routing helpers into `app/worker_support.rs` and keeping `workers.rs` focused on worker thread orchestration.

## 12. Completion Checklist
- [x] Planned document created before implementation
- [x] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, and `TESTPLAN.md` as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 13. Final Notes
- This slice is limited to worker/domain modularization.
- Later slices should own updater/OS integration hardening, perf policy changes, and diagnostics work.
- Do not let helper extraction expand into a roadmap rewrite.

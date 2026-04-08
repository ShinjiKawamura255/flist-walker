# CHANGE PLAN: Architecture Debt Closure Slice

## Metadata
- Date: 2026-04-08
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 3
- Plan Role: slice plan
- Parent Plan: `docs/CHANGE-PLAN-20260408-architecture-debt-closure-roadmap.md`
- Child Plan(s): `docs/CHANGE-PLAN-20260408-architecture-debt-closure-updater-subslice.md`
- Scope Label: architecture-debt-updater
- Related Tickets/Issues: none
- Review Status: reviewed
- Review Notes: Initial review found no blockers. Updater-first ordering is acceptable, and the slice still has two meaningful phases that keep contract closure separate from docs sync.

## 1. Background
- `updater.rs` still concentrates release discovery, support classification, staged download, checksum/signature verification, and helper spawning in one place.
- `app/update.rs` acts as a command bridge, but the contract between updater decision logic and apply logic is still wider than ideal.
- This slice exists to make the updater boundary explicit before perf gate work or diagnostic work depends on its behavior.

## 2. Goal
- Split updater decision logic from staged apply logic so the self-update flow has a clear contract boundary.
- Preserve current user-visible behavior while making the failure surface smaller and better testable.
- Make the following outcomes observable:
  - update candidate selection is isolated from apply-time side effects,
  - updater/app command boundaries are explicit,
  - failure and disabled-update behavior remain stable under contract tests.

## 3. Scope
### In Scope
- `rust/src/updater.rs`
- `rust/src/app/update.rs`
- `rust/src/app/workers.rs`
- `rust/src/app/state.rs`
- update-related tests and docs sync

### Out of Scope
- Perf gate changes
- Diagnostics/supportability changes
- Docs restructuring outside the updater contract notes needed for this slice

## 4. Constraints and Assumptions
- This slice is the first slice of the debt roadmap and therefore sets the pattern for later slices.
- The current self-update behavior must remain functionally equivalent unless the roadmap is updated first.
- Any change in update test coverage must be reflected in `docs/TESTPLAN.md` in the same change.

## 5. Current Risks
- Risk:
  - update contract changes can accidentally hide platform-specific behavior.
  - Impact:
    - Windows/Linux/macOS update support may diverge.
  - Mitigation:
    - keep platform branches explicit and test the seams directly.
- Risk:
  - the updater slice can expand into perf, diagnostics, or release policy work.
  - Impact:
    - the debt roadmap loses ordering discipline.
  - Mitigation:
    - stop at contract boundaries and update the roadmap before expanding scope.

## 6. Execution Strategy
1. Phase 1: Separate updater decision and staging boundaries
   - Files/modules/components:
     - `rust/src/updater.rs`
     - `rust/src/app/workers.rs`
     - `rust/src/app/state.rs`
   - Expected result:
     - candidate selection, support classification, and staged apply steps are separable enough to test independently.
   - Verification:
     - `cargo test`
     - focused review of update contract regression coverage
2. Phase 2: Sync docs and lock the updater contract boundary
   - Files/modules/components:
     - `rust/src/app/update.rs`
     - `rust/src/app/tests/app_core.rs`
     - `docs/ARCHITECTURE.md`
     - `docs/DESIGN.md`
     - `docs/TESTPLAN.md`
     - `docs/TASKS.md`
   - Expected result:
     - updater/app command boundaries are explicit in code and docs, and the next slices can depend on them.
   - Verification:
     - `cargo test`
     - focused review of update dialog and start/install flow coverage

## 7. Detailed Task Breakdown
- [ ] Separate updater candidate selection from staged apply behavior
- [ ] Keep manual-only and disabled-update checks explicit
- [ ] Add or adjust updater contract tests
- [ ] Sync architecture/design/test docs with the updater boundary

## 8. Validation Plan
- Automated tests:
  - `cargo test`
- Manual checks:
  - startup update check behavior remains stable
  - manual-only and disabled-update paths still behave as expected
- Performance or security checks:
  - update signature and checksum verification behavior must not regress
- Regression focus:
  - self-update disable gating
  - release asset selection
  - staged apply behavior

## 9. Rollback Plan
- Updater helper extraction should be revertible without touching perf or diagnostics work.
- Docs changes must be reverted together with any corresponding updater contract changes.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `architecture-debt-closure`, read `[docs/CHANGE-PLAN-20260408-architecture-debt-closure-roadmap.md]`, `[docs/CHANGE-PLAN-20260408-architecture-debt-closure-slice.md]`, and `[docs/CHANGE-PLAN-20260408-architecture-debt-closure-updater-subslice.md]` before starting implementation.
- Execute the updater hardening work in the documented phase order unless the roadmap, slice, or subslice is updated first.
- If scope, order, or risk changes, update the relevant change plan before continuing.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-08 00:00 Drafted as the active slice for architecture debt closure.
- 2026-04-08: Phase 1 work began by splitting updater candidate resolution into release-asset selection and support-classification helpers.
- 2026-04-08 00:00 Phase 1 completed: updater candidate resolution now goes through a pure helper, and the contract stays testable without mixing staged apply logic.
- 2026-04-08 00:00 Phase 2 completed: updater command-surface comments now match the current manager/bridge boundary, and docs/testplan stay aligned with the updater contract.

## 12. Completion Checklist
- [x] Planned document created before implementation
- [x] Temporary `AGENTS.md` rule added
- [x] Work executed according to the plan or the plan updated first
- [x] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, and `TESTPLAN.md` as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 13. Final Notes
- Keep the slice focused on updater contract boundaries.
- Before deleting this plan, move any lasting decisions into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, or `TESTPLAN.md`.

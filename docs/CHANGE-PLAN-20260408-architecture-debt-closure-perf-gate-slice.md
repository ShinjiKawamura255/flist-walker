# CHANGE PLAN: Perf Gate Strengthening Slice

## Metadata
- Date: 2026-04-08
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice plan
- Parent Plan: `docs/CHANGE-PLAN-20260408-architecture-debt-closure-roadmap.md`
- Child Plan(s): none
- Scope Label: architecture-debt-perf-gate
- Related Tickets/Issues: none
- Review Status: not reviewed
- Review Notes: Draft created after Slice A closure. This slice is intended to keep the perf guard light, explicit, and closeable without a third layer.

## 1. Background
- The project already has ignored perf tests, but the most important regression budgets are not part of the default PR hot path.
- The current debt is not to add more perf experiments. It is to put one lightweight, enforceable guard in the right place and keep the heavier suite separate.
- This slice exists after the updater boundary is stabilized so perf gating can be defined against stable code paths rather than changing contracts.

## 2. Goal
- Add a lightweight perf guard to the PR validation path without pulling the full perf suite into the default workflow.
- Keep the gate cheap enough that it can run routinely, while still failing on the regressions that matter most.
- Make the following outcomes observable:
  - a single perf guard candidate and threshold are documented,
  - CI or validation matrix wiring is explicit,
  - the heavy perf suite remains available but clearly separated.

## 3. Scope
### In Scope
- perf regression workflow wiring
- validation matrix updates
- perf budget documentation and related release/process docs

### Out of Scope
- updater implementation changes
- diagnostics/supportability changes
- new feature work

## 4. Constraints and Assumptions
- The updater contract boundaries are already stable enough for perf work to measure meaningful behavior.
- The lightweight gate should be small enough to run in routine validation without becoming noisy.
- Any change to perf budget thresholds must be reflected in `docs/TESTPLAN.md` and the workflow docs in the same change.

## 5. Current Risks
- Risk:
  - the guard could be too noisy or too broad.
  - Impact:
    - PR validation becomes expensive or flaky.
  - Mitigation:
    - choose one gate first and keep the heavy suite separate.
- Risk:
  - the gate could be too weak to matter.
  - Impact:
    - perf regressions still slip through.
  - Mitigation:
    - tie the guard to the most regression-prone path and keep the budget explicit.

## 6. Execution Strategy
1. Phase 1: Define the lightweight perf gate candidate and budget
   - Files/modules/components:
     - `docs/TESTPLAN.md`
     - perf workflow documentation
     - related validation notes
   - Expected result:
     - one small perf check is selected with a concrete threshold and an explicit reason for being the PR gate.
   - Verification:
     - doc review and traceability check
2. Phase 2: Wire the guard into validation and keep the heavy suite separate
   - Files/modules/components:
     - workflow docs / CI config references
     - `docs/TASKS.md`
     - `docs/ARCHITECTURE.md` if needed for traceability
   - Expected result:
     - the lightweight guard is visible in normal validation, while the heavy perf suite remains an explicit follow-up.
   - Verification:
     - `cargo test`
     - targeted review of validation matrix / workflow references

## 7. Detailed Task Breakdown
- [ ] Pick the first lightweight perf gate candidate
- [ ] Define the budget and failure condition
- [ ] Update validation docs to include the gate
- [ ] Keep the heavy perf suite clearly separated

## 8. Validation Plan
- Automated tests:
  - `cargo test` when Rust-visible validation references change
- Manual checks:
  - validation matrix / workflow mapping
  - perf gate versus heavy suite separation
- Performance or security checks:
  - confirm the selected guard is light enough to stay routine
- Regression focus:
  - perf guard noise
  - perf guard blind spots

## 9. Rollback Plan
- The lightweight perf guard can be withdrawn without touching updater contract code.
- If the first gate proves too noisy, reduce it before broadening the scope.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `architecture-debt-closure`, read `[docs/CHANGE-PLAN-20260408-architecture-debt-closure-roadmap.md]` and `[docs/CHANGE-PLAN-20260408-architecture-debt-closure-perf-gate-slice.md]` before starting implementation.
- Execute the perf gate work in the documented phase order unless the roadmap or slice is updated first.
- If scope, order, or risk changes, update the relevant change plan before continuing.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-08 00:00 Drafted as the active perf gate slice after updater hardening.
- 2026-04-08 00:00 Phase 1 completed: the lightweight perf gate candidate and budget are defined in `docs/TESTPLAN.md`.
- 2026-04-08 00:00 Phase 2 completed: the lightweight perf gate is now wired into `ci-cross-platform.yml` on the linux-native job.
- 2026-04-08 00:00 Phase 1 started: FileList stream budget is the lightweight PR gate candidate, and the walker perf test remains the heavy suite.

## 12. Completion Checklist
- [ ] Planned document created before implementation
- [ ] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, and `TESTPLAN.md` as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 13. Final Notes
- Keep this slice focused on one lightweight gate.
- Before deleting this plan, move any lasting decisions into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, or `TESTPLAN.md`.

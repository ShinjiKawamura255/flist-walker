# CHANGE PLAN: Updater Hardening Subslice

## Metadata
- Date: 2026-04-08
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 3
- Plan Role: subslice plan
- Parent Plan: `docs/CHANGE-PLAN-20260408-architecture-debt-closure-slice.md`
- Child Plan(s): none
- Scope Label: architecture-debt-updater-helpers
- Related Tickets/Issues: none
- Review Status: reviewed
- Review Notes: Initial review found no blockers. The subslice is narrow enough to govern the first updater batch without collapsing into the slice-level docs phase.

## 1. Background
- The updater slice is still broad enough that the first helper split benefits from its own smaller control plane.
- Candidate selection, support classification, and staged apply behavior are all in `updater.rs`, but they do not need to move as a single indivisible step.
- This subslice exists so the first updater phase can be completed, reviewed, and validated before the slice moves to docs and contract closure.

## 2. Goal
- Isolate the candidate-selection side of the updater from the apply side first, then let the slice-level phase complete the docs and command-boundary closure.
- Keep the helper split hermetic so tests do not depend on network or real release state.
- Make the following outcomes observable:
  - selection/support logic is independently testable,
  - disabled/manual-only guards remain explicit,
  - the apply path can still be driven through the same public surface.

## 3. Scope
### In Scope
- `rust/src/updater.rs`
- updater-specific unit tests
- minimal callsite adjustments needed to keep the phase seam explicit

### Out of Scope
- perf gate work
- diagnostics/supportability work
- updater docs closure

## 4. Constraints and Assumptions
- This subslice is the first batch of the updater slice and should end with a clean handoff to the slice-level Phase 2.
- Behavior must stay functionally equivalent unless the slice is updated first.
- Tests should remain hermetic; avoid any dependency on live network state.

## 5. Current Risks
- Risk:
  - The helper split may accidentally mix selection and apply responsibilities.
  - Impact:
    - the seam becomes more complex instead of simpler.
  - Mitigation:
    - keep the selection-only logic side-effect free and explicitly separate.
- Risk:
  - test coverage can become brittle if the subslice relies on the live release feed.
  - Impact:
    - CI signal becomes noisy.
  - Mitigation:
    - use hermetic tests and existing environment-flagged paths only.

## 6. Execution Strategy
1. Batch 1: Separate update candidate selection and support classification
   - Files/modules/components:
     - `rust/src/updater.rs`
   - Expected result:
     - release discovery, version filtering, and support classification can be verified independently of staging and helper spawn logic.
   - Verification:
     - `cargo test`
2. Batch 2: Keep staged apply behavior reachable through the same contract surface
   - Files/modules/components:
     - `rust/src/updater.rs`
     - `rust/src/app/update.rs`
     - `rust/src/app/workers.rs`
     - `rust/src/app/tests/app_core.rs`
   - Expected result:
     - the apply path still works through the existing command flow while helper boundaries remain explicit.
   - Verification:
     - `cargo test`

## 7. Detailed Task Breakdown
- [ ] Separate candidate selection from apply-side side effects
- [ ] Keep manual-only and disabled-update guards explicit
- [ ] Add hermetic unit tests for selection and apply-path invariants

## 8. Validation Plan
- Automated tests:
  - `cargo test`
- Manual checks:
  - none required for this subslice unless the slice expands the user-visible update flow
- Performance or security checks:
  - checksum/signature behavior must remain intact
- Regression focus:
  - disabled-update gating
  - manual-only fallback
  - candidate selection behavior

## 9. Rollback Plan
- This subslice should be revertible without touching the slice’s docs phase.
- If the split proves too small or too large, update the slice before continuing.

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
- 2026-04-08 00:00 Drafted as the active updater helper subslice.
- 2026-04-08: Batch 1 completed with helper extraction for release-asset selection and support classification, plus direct contract tests.
- 2026-04-08 00:00 Batch 2 completed with command-surface comment cleanup and updater contract docs alignment.

## 12. Completion Checklist
- [ ] Planned document created before implementation
- [ ] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, and `TESTPLAN.md` as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 13. Final Notes
- Keep this batch focused on selection/support helpers.
- Before deleting this plan, move any lasting decisions into the slice or the stable docs.

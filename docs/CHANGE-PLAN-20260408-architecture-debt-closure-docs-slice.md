# CHANGE PLAN: Docs and Closure Restructuring Slice

## Metadata
- Date: 2026-04-08
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice plan
- Parent Plan: `docs/CHANGE-PLAN-20260408-architecture-debt-closure-roadmap.md`
- Child Plan(s): none
- Scope Label: architecture-debt-docs-closure
- Related Tickets/Issues: none
- Review Status: reviewed
- Review Notes: No blockers. The final slice remains valid as a two-phase docs/closure closeout.

## 1. Background
- Updater hardening, perf gate strengthening, and diagnostics/supportability are complete enough that the remaining work is documentation shape and program cleanup.
- The repo still mixes durable architecture/testing guidance with temporary debt-program tracking in a few places.
- This final slice exists to leave only steady-state docs and remove temporary change-plan control once the closure record has been transferred into stable docs.

## 2. Goal
- Separate durable engineering guidance from temporary debt-program scaffolding.
- Close the architecture debt roadmap cleanly without losing completion history.
- Make the following outcomes observable:
  - stable docs remain readable without plan-specific noise,
  - `TASKS.md` retains the durable closure record,
  - temporary `AGENTS.md` guidance and change-plan files are removed after completion.

## 3. Scope
### In Scope
- steady-state docs cleanup
- closure-history consolidation in `docs/TASKS.md`
- final roadmap/slice cleanup
- `AGENTS.md` temporary rule removal

### Out of Scope
- new feature work
- code changes
- release policy changes

## 4. Constraints and Assumptions
- Lasting architecture, design, and validation decisions must stay in `ARCHITECTURE.md`, `DESIGN.md`, and `TESTPLAN.md`.
- Program history that matters after cleanup must move into `TASKS.md` before deleting any change-plan files.
- This slice is docs-only and should validate with doc diff review plus reference-consistency checks.

## 5. Current Risks
- Risk:
  - cleanup could remove plan history before the durable record is preserved.
  - Impact:
    - the repo loses why and when the debt program was completed.
  - Mitigation:
    - move the closure summary into `TASKS.md` before removing plan documents.
- Risk:
  - durable docs could lose important validation or architecture notes during cleanup.
  - Impact:
    - future work regresses because the stable docs become thinner than the code reality.
  - Mitigation:
    - update stable docs first, then remove plan scaffolding second.

## 6. Execution Strategy
1. Phase 1: Restructure durable docs around steady-state guidance
   - Files/modules/components:
     - `docs/ARCHITECTURE.md`
     - `docs/DESIGN.md`
     - `docs/TESTPLAN.md`
     - `docs/TASKS.md`
   - Expected result:
     - durable docs keep the architecture, operations, and validation guidance without depending on roadmap/slice context.
   - Verification:
     - targeted docs review
     - `rg` reference consistency check
2. Phase 2: Close the debt program and remove temporary scaffolding
   - Files/modules/components:
     - `AGENTS.md`
     - `docs/CHANGE-PLAN-20260408-architecture-debt-closure-roadmap.md`
     - `docs/CHANGE-PLAN-20260408-architecture-debt-closure-diagnostics-slice.md`
     - `docs/CHANGE-PLAN-20260408-architecture-debt-closure-perf-gate-slice.md`
     - `docs/CHANGE-PLAN-20260408-architecture-debt-closure-slice.md`
     - `docs/CHANGE-PLAN-20260408-architecture-debt-closure-updater-subslice.md`
   - Expected result:
     - temporary rule and debt-program change plans are removed after the durable record is preserved.
   - Verification:
     - targeted docs review
     - `git diff --stat`
     - `rg` reference consistency check

## 7. Detailed Task Breakdown
- [ ] Move debt-program closure history into `docs/TASKS.md`
- [ ] Keep stable architecture/design/test guidance independent from change plans
- [ ] Remove the temporary `AGENTS.md` rule after closure is recorded
- [ ] Delete the debt-program change-plan files after closure is recorded

## 8. Validation Plan
- Automated tests:
  - none required beyond docs validation
- Manual checks:
  - doc diff review
  - `rg` checks for stale `CHANGE-PLAN-20260408-architecture-debt-closure` references
  - `git diff --stat` confirms docs-only cleanup
- Performance or security checks:
  - none
- Regression focus:
  - durable guidance survives
  - closure history remains discoverable

## 9. Rollback Plan
- If cleanup removes too much context, restore the deleted change-plan docs before attempting another closure pass.
- If stable docs prove incomplete, stop closure, restore the temporary rule, and update the docs before retrying cleanup.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `architecture-debt-closure`, read `[docs/CHANGE-PLAN-20260408-architecture-debt-closure-roadmap.md]` and `[docs/CHANGE-PLAN-20260408-architecture-debt-closure-docs-slice.md]` before starting implementation.
- Execute the docs/closure work in the documented phase order unless the roadmap or slice is updated first.
- If scope, order, or risk changes, update the relevant change plan before continuing.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-08 00:00 Drafted as the final docs/closure slice after diagnostics/supportability stabilization.

## 12. Completion Checklist
- [ ] Planned document created before implementation
- [ ] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, and `TESTPLAN.md` as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 13. Final Notes
- Prefer deleting temporary control docs over leaving stale references behind.
- Before deleting this plan, move any lasting closure notes into `docs/TASKS.md`.

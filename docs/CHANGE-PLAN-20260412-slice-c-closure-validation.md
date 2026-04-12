# CHANGE PLAN: Slice C - Closure Validation

## Metadata
- Date: 2026-04-12
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice plan
- Execution Mode: none
- Execution Mode Policy: none
- Parent Plan: [docs/CHANGE-PLAN-20260412-roadmap-architecture-score-uplift.md](docs/CHANGE-PLAN-20260412-roadmap-architecture-score-uplift.md)
- Child Plan(s): none
- Scope Label: closure-validation
- Related Tickets/Issues: none
- Review Status: 未レビュー
- Review Notes:
  - 実現性レビュー: feasible.
  - The closure slice is intentionally lightweight and relies on existing validation commands and docs.
  - The main open question is whether the score target is already satisfied or whether the roadmap needs another pass.

## 1. Background
- The roadmap needs an explicit closing step so the final decision is based on evidence rather than momentum.
- A closure slice is necessary because the architecture target is qualitative, and the project should not keep extending a plan without a documented reason.

## 2. Goal
- Validate whether the architecture has reached the intended 80-point level using concrete indicators.
- Decide one of two outcomes:
  - close the roadmap and remove the temporary plan machinery, or
  - keep the roadmap open and create a follow-up plan with explicit evidence.

## 3. Scope
### In Scope
- `rust` validation commands
- `docs/ARCHITECTURE.md`
- `docs/DESIGN.md`
- `docs/TESTPLAN.md`
- `docs/TASKS.md`
- `AGENTS.md`
- the completed slice outcomes

### Out of Scope
- New architecture refactoring work unrelated to validation or closure.
- Additional feature work.

## 4. Constraints and Assumptions
- Closure must be evidence-based and tied to the existing validation matrix.
- If the goal is not met, the roadmap must be re-planned explicitly instead of quietly extended.
- The temporary `AGENTS.md` rule should remain in place until the closure decision is recorded.

## 5. Current Risks
- Risk:
  - Impact: The closure decision could be made on feel rather than evidence.
  - Mitigation: Use the validation matrix and the documented architecture/doc trace as the basis for the decision.
- Risk:
  - Impact: A premature close could hide residual architectural debt.
  - Mitigation: Record the remaining gaps before closing and only close when they are acceptably small.

## 6. Execution Strategy
1. Run the relevant validation matrix entries.
   - Files/modules/components: `rust` test commands and the current docs.
   - Expected result: The architectural changes are confirmed against the expected regression and smoke gates.
   - Verification: `cargo test` and any required targeted checks from the matrix.
2. Compare the resulting shape against the goal.
   - Files/modules/components: roadmap, architecture docs, and task record.
   - Expected result: A clear close/continue decision is written down.
   - Verification: document review and explicit decision notes.
3. Apply closure updates.
   - Files/modules/components: `docs/TASKS.md`, `AGENTS.md`, and the change plan files if the roadmap closes.
   - Expected result: lasting notes stay in durable docs, temporary plan machinery is removed only if closing.
   - Verification: diff review and final cleanup check.

## 7. Detailed Task Breakdown
- [ ] Run the relevant validation commands from `docs/TESTPLAN.md`.
- [ ] Decide whether the architecture goal is met based on concrete evidence.
- [ ] Update durable docs with any lasting decisions.
- [ ] Remove the temporary plan machinery only if the roadmap is closed.

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test`
  - any additional matrix items required by the final slice results
- Manual checks:
  - focused GUI smoke if the routing/shell changes warrant it
- Performance or security checks:
  - preserve the existing performance and security regression guards
- Regression focus:
  - closure correctness
  - lingering architecture debt
  - docs/test traceability

## 9. Rollback Plan
- If the goal is not met, do not remove the temporary plan machinery.
- Keep the roadmap and slice docs in place until the follow-up plan is written or the road is closed.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `architecture-score-80`, read the roadmap and all slice plans before starting implementation.
- Follow the roadmap `Execution Mode` and `Execution Mode Policy`.
- Execute the work in the documented order unless the plan is updated first.
- Treat the `closure slice` as the final gating step before the roadmap is closed.
- If the closure slice shows the goal is unmet, re-plan before continuing and do not remove the temporary rule.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-12 Planned.

## 12. Communication Plan
- Return to the user with the closure decision, the evidence used, and any required follow-up plan.

## 13. Completion Checklist
- [ ] Planned document created before implementation
- [ ] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into the durable docs as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 14. Final Notes
- This slice is the only place where the roadmap may be closed; if the evidence is insufficient, leave everything in place and re-plan.

# CHANGE PLAN: Slice C - Closure Validation

## Metadata
- Date: 2026-04-13
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice plan
- Execution Mode: none
- Execution Mode Policy: none
- Parent Plan: [docs/CHANGE-PLAN-20260413-roadmap-architecture-score-72-to-80.md](docs/CHANGE-PLAN-20260413-roadmap-architecture-score-72-to-80.md)
- Child Plan(s): none
- Scope Label: closure-validation
- Related Tickets/Issues: none
- Review Status: 未レビュー
- Review Notes:
  - The slice is intentionally lightweight and depends on the evidence produced by the earlier slices.
  - The key question is whether the resulting architecture is now good enough to close the roadmap or whether another follow-up pass is still justified.

## 1. Background
- The architecture target is qualitative, so the final decision needs a dedicated evidence-based closing step.
- The project should not keep extending the plan without an explicit close/continue decision.

## 2. Goal
- Decide whether the follow-up cleanup is sufficient to close the architecture-score uplift sequence.
- If the debt remains material, record the gap clearly and keep the plan machinery in place for a follow-up roadmap.

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
- Additional implementation beyond validation and the close/continue decision.

## 4. Constraints and Assumptions
- Closure must be evidence-based.
- If the remaining architectural debt is still material, re-plan explicitly instead of forcing a close.

## 5. Current Risks
- Risk: the roadmap may be closed too early if the closure slice relies on intuition rather than evidence.
  - Impact: the remaining architectural debt would be understated.
  - Mitigation: base the decision on `cargo test`, code shape, and docs review.

## 6. Execution Strategy
1. Run the validation commands.
   - Expected result: the ownership and module-hygiene changes do not regress existing behavior.
   - Verification: `cargo test`.
2. Compare the resulting shape with the roadmap goal.
   - Expected result: a clear close/continue decision is written down.
   - Verification: review the code shape and docs alignment.
3. Apply closure updates.
   - Expected result: durable docs record the decision and temporary plan machinery is removed only if the roadmap closes.
   - Verification: diff review and final cleanup check.

## 7. Detailed Task Breakdown
- [ ] Run the relevant validation commands.
- [ ] Decide whether the architecture goal is met based on concrete evidence.
- [ ] Update durable docs with any lasting decisions.
- [ ] Remove the temporary plan machinery only if the roadmap is closed.

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test`
- Manual checks:
  - focused GUI smoke if the ownership or module-seam changes warrant it
- Regression focus:
  - closure correctness
  - lingering architectural debt
  - docs/test traceability

## 9. Rollback Plan
- If the goal is not met, do not remove the temporary plan machinery.
- Keep the roadmap and slice docs in place until the follow-up plan is written or the road is closed.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `architecture-score-72-to-80`, read the roadmap and slice plans before starting implementation.
- Follow the roadmap `Execution Mode` and `Execution Mode Policy`.
- Treat the closure slice as the final gating step before the roadmap is closed.
- If the closure slice shows the goal is still unmet, re-plan before continuing and do not remove the temporary rule.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-13 Planned.

## 12. Communication Plan
- Return the closure decision, the evidence used, and any follow-up plan if needed.

## 13. Completion Checklist
- [ ] Planned document created before implementation
- [ ] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting notes moved into durable docs as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 14. Final Notes
- This slice is the only place where the roadmap may be closed.

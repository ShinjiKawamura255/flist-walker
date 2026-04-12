# CHANGE PLAN: Slice B - Closure Validation

## Metadata
- Date: 2026-04-12
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice plan
- Execution Mode: none
- Execution Mode Policy: none
- Parent Plan: [docs/CHANGE-PLAN-20260412-roadmap-app-state-ownership-consolidation.md](docs/CHANGE-PLAN-20260412-roadmap-app-state-ownership-consolidation.md)
- Child Plan(s): none
- Scope Label: closure-validation
- Related Tickets/Issues: none
- Review Status: 未レビュー
- Review Notes:
  - 実現性レビュー: feasible.
  - This slice intentionally stays lightweight and depends on the evidence produced by Slice A.

## 1. Background
- A closing step is still needed so the roadmap does not silently end without an explicit close/continue decision.

## 2. Goal
- Decide whether the ownership consolidation is enough to close the architecture cleanup sequence.
- If not, record the remaining gap clearly and keep the plan machinery in place for a follow-up roadmap.

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
- If the remaining architecture debt is still material, re-plan explicitly instead of forcing a close.

## 5. Current Risks
- Risk: the roadmap may be closed too early if the closure slice relies on intuition rather than evidence.
  - Impact: the remaining state-ownership gap would be understated.
  - Mitigation: base the decision on `cargo test`, code shape, and docs review.

## 6. Execution Strategy
1. Run the validation commands.
   - Expected result: the ownership changes do not regress existing behavior.
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
  - focused GUI smoke if the ownership/routing changes warrant it
- Regression focus:
  - closure correctness
  - lingering ownership duplication
  - event-routing clarity

## 9. Rollback Plan
- If the goal is not met, do not remove the temporary plan machinery.
- Keep the roadmap and slice docs in place until the follow-up plan is written or the road is closed.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `architecture-score-80`, read the roadmap and slice plans before starting implementation.
- Follow the roadmap `Execution Mode` and `Execution Mode Policy`.
- Execute Slice A before the closure slice unless the plan is updated first.
- Treat the closure slice as the final gating step before the roadmap is closed.
- If the closure slice shows the goal is still unmet, re-plan before continuing and do not remove the temporary rule.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-12 Planned.
- 2026-04-12 Validation ran after the ownership/sync consolidation pass; result: continue. The remaining gap is still material enough to justify a follow-up roadmap rather than closing this one.

## 12. Communication Plan
- Return the closure decision, evidence used, and any follow-up plan if needed.

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
- Outcome on 2026-04-12: continue.

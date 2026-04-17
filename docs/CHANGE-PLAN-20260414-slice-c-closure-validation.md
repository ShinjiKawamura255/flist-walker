# CHANGE PLAN: Slice C - Closure Validation

## Metadata
- Date: 2026-04-14
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice plan
- Execution Mode: none
- Execution Mode Policy: none
- Parent Plan: [docs/CHANGE-PLAN-20260414-roadmap-architecture-score-80-follow-up.md](docs/CHANGE-PLAN-20260414-roadmap-architecture-score-80-follow-up.md)
- Child Plan(s): none
- Scope Label: closure-validation
- Related Tickets/Issues: none
- Review Status: レビュー済み
- Review Notes:
  - This slice exists only to determine whether the follow-up roadmap can be closed.
  - Review result: GO after fixing the close/continue rubric and the residual-debt record format.

## 1. Background
- The architecture target is qualitative, so the final decision needs a dedicated evidence-based closing step.

## 2. Goal
- Decide whether the follow-up cleanup is sufficient to close the architecture-score uplift sequence.
- If the remaining debt still requires a Tab-Shell data-model rewrite or a broader follow-up, record that explicitly instead of forcing closure.

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
- If the remaining debt is only a larger rewrite, note that as the next follow-up boundary rather than widening the current roadmap.
- The closure decision must be supported by a fixed evidence set: `cargo test` green, the Slice A/B contract and diff checks, and a doc review that confirms the roadmap goal and out-of-scope boundaries still match the implementation.
- Close only if all of the following are true:
  - `cargo test` is green.
  - Slice A's `DerefMut` removal check passes.
  - Slice A's tab-state contract test compiles with full field literals.
  - Slice B's import cleanup is confirmed by diff review.
  - The final implementation still matches the roadmap's stated goal and out-of-scope boundaries.
- Continue if any one of those checks fails.

## 5. Execution Strategy
1. Run the validation commands.
   - Verification: `cargo test`.
2. Compare the resulting shape with the roadmap goal.
   - Verification: review the code shape and docs alignment, including the Slice A contract-test evidence and the Slice B import-cleanup diff.
3. Apply closure updates.
   - Verification: diff review and final cleanup check, then record the result in a fixed format.

## 6. Detailed Task Breakdown
- [ ] Run the relevant validation commands.
- [ ] Decide whether the architecture goal is met based on the fixed close/continue rubric.
- [ ] Record the result using the fixed `Closed`, `Deferred`, and `Blocked` format.
- [ ] Update durable docs with any lasting decisions.
- [ ] Remove the temporary plan machinery only if the roadmap is closed.

## 7. Validation Plan
- Automated tests:
  - `cd rust && cargo test`
- Regression focus:
  - closure correctness
  - lingering architectural debt
  - docs/test traceability
- Evidence checklist:
  - Slice A `DerefMut` removal check passes.
  - Slice A tab-state contract test compiles with full field literals.
  - Slice B wildcard-import cleanup is confirmed by diff review.
  - roadmap goal/out-of-scope boundaries still match the final implementation.
- Record template:
  - `Closed:` one bullet per completed outcome, or `- none` if the roadmap is not closed.
  - `Deferred:` one bullet per residual item with `item | reason | next follow-up artifact`, or `- none`.
  - `Blocked:` one bullet per blocking issue with `item | blocker | required next step`, or `- none`.

## 8. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `architecture-score-80-follow-up`, read the follow-up roadmap and slice plans before starting implementation.
- Follow the roadmap `Execution Mode` and `Execution Mode Policy`.
- Treat the closure slice as the final gating step before the roadmap is closed.
- If the closure slice shows the goal is still unmet, re-plan before continuing and do not remove the temporary rule.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 9. Progress Log
- 2026-04-14 Planned.
- 2026-04-14 Reviewed GO.

## 10. Completion Checklist
- [ ] Planned document created before implementation
- [ ] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting notes moved into durable docs as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Follow-up roadmap and superseded roadmap documents deleted after completion

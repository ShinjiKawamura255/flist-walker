# CHANGE PLAN: Slice B - Import Hygiene and Closure

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
- Scope Label: import-hygiene-and-closure
- Related Tickets/Issues: none
- Review Status: レビュー済み
- Review Notes:
  - The slice is limited to the remaining wildcard-import hygiene, the known local clone hot path, and the final close/continue evidence record.
  - It should not broaden into a fresh refactor pass or a new abstraction effort.
  - Review result: GO. The slice stays mechanical and local, and the clone hot path remains bounded.

## 1. Background
- The previous roadmap already improved import hygiene on the densest modules, but the closure review showed that the broader app surface still relies on wildcard leakage.
- The follow-up needs to make the remaining dependencies visible enough to be maintainable.

## 2. Goal
- Remove wildcard-import leakage from the touched app surface and `app/mod.rs`.
- Remove the already-identified local clone hot path in `pipeline_owner.rs`.
- Produce the closing evidence that determines whether the roadmap can be removed or must remain open for another pass.

## 3. Scope
### In Scope
- `rust/src/app/mod.rs`
- `rust/src/app/result_reducer.rs`
- `rust/src/app/result_flow.rs`
- `rust/src/app/response_flow.rs`
- `rust/src/app/coordinator.rs`
- `rust/src/app/filelist.rs`
- `rust/src/app/update.rs`
- `rust/src/app/index_coordinator.rs`
- `rust/src/app/search_coordinator.rs`
- `rust/src/app/tests/*`
- `docs/ARCHITECTURE.md`
- `docs/DESIGN.md`
- `docs/TESTPLAN.md`

### Out of Scope
- Ownership refactors already assigned to Slice A.
- New helper-module splits.
- Any expansion into unrelated cleanup.
- Worker-bus trait abstraction, logging overhaul, theme consolidation, and other broad follow-up work not already localized to the touched surface.

## 4. Constraints and Assumptions
- The import cleanup should be mechanical and local.
- The localized cleanup should not become a new abstraction pass.
- Closure should be based on evidence, not the desire to finish the plan.

## 5. Current Risks
- Risk: import hygiene cleanup could become an open-ended sweep.
  - Impact: the slice would grow beyond the remaining debt.
  - Mitigation: restrict the slice to the touched app surface and the modules still carrying wildcard leakage.

## 6. Phase Plan
1. Replace wildcard imports with explicit imports on the touched app surface.
   - Success condition: the active surface no longer relies on `use super::*;`, and `app/mod.rs` no longer acts as an import-suppression sink for the active surface.
2. Validate the result and decide closure.
   - Success condition: the roadmap can be closed if the evidence supports it, or a narrower follow-up can be created if not.

## 7. Detailed Task Breakdown
- [ ] Remove wildcard-import leakage from the touched app surface.
- [ ] Remove the local clone hot path in `pipeline_owner.rs`.
- [ ] Re-run validation and record the close/continue decision.
- [ ] Update durable docs only with lasting conclusions.

## 8. Validation Plan
- `cd rust && cargo test`

## 9. Rollback Plan
- If closure is not justified, keep the roadmap and slice docs in place and re-plan explicitly.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `architecture-score-80-follow-up`, read the follow-up roadmap and slice plans before starting implementation.
- Follow the roadmap `Execution Mode` and `Execution Mode Policy`.
- Keep the import-hygiene work bounded to the documented slice.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-14 Planned.
- 2026-04-14 Reviewed GO.

## 12. Completion Checklist
- [ ] Planned document created before implementation
- [ ] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting notes moved into durable docs as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Follow-up roadmap and superseded roadmap documents deleted after completion

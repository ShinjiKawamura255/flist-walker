# CHANGE PLAN: Architecture Score 80 Follow-up

## Metadata
- Date: 2026-04-14
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: roadmap
- Execution Mode: standard
- Execution Mode Policy: Keep each slice narrowly tied to the remaining architectural debt, validate after each slice, and stop to re-plan instead of widening scope. If the closure slice shows that the remaining Tab-Shell dual-ownership debt still requires a data-model rewrite, do not absorb that rewrite into this roadmap; record it as a separate follow-up.
- Parent Plan: none
- Child Plan(s):
  - [docs/CHANGE-PLAN-20260414-slice-a-ownership-finalization.md](docs/CHANGE-PLAN-20260414-slice-a-ownership-finalization.md)
  - [docs/CHANGE-PLAN-20260414-slice-b-import-hygiene-and-closure.md](docs/CHANGE-PLAN-20260414-slice-b-import-hygiene-and-closure.md)
  - [docs/CHANGE-PLAN-20260414-slice-c-closure-validation.md](docs/CHANGE-PLAN-20260414-slice-c-closure-validation.md)
- Scope Label: architecture-score-80-follow-up
- Related Tickets/Issues: none
- Review Status: レビュー済み
- Review Notes:
  - The previous roadmap closed the broad shell-boundary work and the render/input/module hygiene split, but closure review still found the remaining debt material.
  - The remaining blockers are narrower: explicit ownership boundaries are still leaky in a few manager/state surfaces, and wildcard import hygiene is still pervasive outside the touched modules.
  - This follow-up is intentionally narrower than the previous roadmap and is meant to close only the debt that was still judged material on 2026-04-14.
  - The follow-up does **not** attempt a full Tab-Shell data-model rewrite in Slice A; that rewrite is explicitly out of scope for this pass and is treated as residual debt if the closure slice still says continue.
  - Review result: GO. Slice A and Slice B were both reviewed after hard-stop updates, and Slice C now uses a fixed close/continue rubric plus Closed/Deferred/Blocked output format.

## 1. Background
- The 72-to-80 roadmap improved the architecture materially, but the closure review found that the remaining debt still includes explicit ownership leakage, Tab-Shell dual-ownership friction, and import hygiene pollution.
- The purpose of this follow-up is to finish the last material structural gaps without reopening the broader refactor surface.

## 2. Goal
- Close the remaining architectural debt that prevented the previous roadmap from being closed.
- Remove the lingering `Deref/DerefMut` exposure patterns that still make state boundaries harder to reason about.
- Pin the current Tab-Shell transition contract with explicit tests and documentation, without widening this pass into a tab data-model rewrite.
- Remove the remaining wildcard-import pollution from the touched app surface so module dependencies are visible again.
- End with a clear close/continue decision and a durable record of what still remains, if anything.

## 3. Scope
### In Scope
- `rust/src/app/state.rs`
- `rust/src/app/tab_state.rs`
- `rust/src/app/tabs.rs`
- `rust/src/app/pipeline_owner.rs`
- `rust/src/app/result_reducer.rs`
- `rust/src/app/result_flow.rs`
- `rust/src/app/response_flow.rs`
- `rust/src/app/coordinator.rs`
- `rust/src/app/filelist.rs`
- `rust/src/app/update.rs`
- `rust/src/app/index_coordinator.rs`
- `rust/src/app/search_coordinator.rs`
- `rust/src/app/mod.rs`
- `rust/src/app/tests/*`
- `docs/ARCHITECTURE.md`
- `docs/DESIGN.md`
- `docs/TESTPLAN.md`
- `docs/TASKS.md`
- `AGENTS.md`

### Out of Scope
- New user-facing features.
- Render/input module splitting, worker-bus helper extraction, or other hygiene work already completed in the previous roadmap.
- Broad architectural rewrites beyond the specific remaining debt called out by the closure review.
- Any Tab-Shell data-model rewrite or new copy path creation in this pass.
- Worker-bus trait abstraction, observability redesign, theme-consolidation work, and other larger cleanups that are not already localized to the touched modules.

## 4. Constraints and Assumptions
- The follow-up should be judged by concrete evidence, not intuition.
- Slice A is bounded to mechanical ownership cleanup plus explicit pinning of the existing tab transition contract.
- If the remaining debt is still material after the planned slices, stop and re-plan rather than widening the current roadmap.
- If any change would require adding fields, removing fields, or introducing a new Tab-Shell copy path, it is out of scope for this roadmap.

## 5. Current Risks
- Risk: attempting to “finish everything” could turn the follow-up into another open-ended refactor.
  - Impact: the roadmap would become harder to complete and easier to drift.
  - Mitigation: keep the slices focused on the specific remaining blockers and use the closure slice to decide whether to stop.
- Risk: changing ownership surfaces without tests could regress tab/session behavior.
  - Impact: subtle state synchronization bugs could reappear.
  - Mitigation: keep the ownership slice tied to the existing regression tests and state-boundary behavior.
- Risk: Tab-Shell dual ownership may still remain after the mechanical cleanup.
  - Impact: the closure slice may need to continue the roadmap or spawn a separate follow-up.
  - Mitigation: do not hide that rewrite inside Slice A; document the residual boundary explicitly and let the closure slice decide.

## 6. Execution Strategy
1. Slice A: Ownership finalization.
   - Files/modules/components: `app/state.rs`, `app/tab_state.rs`, `app/tabs.rs`, `app/tests/session_tabs.rs`, `docs/ARCHITECTURE.md`, `docs/DESIGN.md`.
   - Phases:
     - Phase 1: remove the remaining transparent manager exposure by deleting `DerefMut` on `FileListManager` and `UpdateManager`, then adjust only the direct call sites required to compile.
     - Phase 2: pin the current Tab-Shell transition contract with tests and docs only. `from_shell` / `apply_shell` stay as the only copy points, but the tab data model itself is not rewritten in this pass.
   - Hard stop conditions:
     - Do not add or remove fields on `TabQueryState`, `TabIndexState`, `TabResultState`, or `AppTabState`.
     - Do not introduce a new Tab-Shell copy path or a new tab/session synchronization helper.
     - If fixing a compile error would require a Tab-Shell data-model rewrite, stop and re-plan.
   - Expected result: `DerefMut` is removed from the remaining manager/state surfaces, and the existing tab/session boundary is explicitly pinned rather than silently widened.
   - Verification: `rg -n "impl DerefMut for FileListManager|impl DerefMut for UpdateManager" rust/src/app/state.rs` returns no matches, `cargo test` passes, and the tab-switch / restore regression tests continue to pass.
2. Slice B: Import hygiene and localized cleanup.
   - Files/modules/components: `app/mod.rs`, `app/result_reducer.rs`, `app/result_flow.rs`, `app/response_flow.rs`, `app/coordinator.rs`, `app/filelist.rs`, `app/update.rs`, `app/index_coordinator.rs`, `app/search_coordinator.rs`, `app/pipeline_owner.rs`, `app/tests/*`.
   - Phases:
     - Phase 1: remove wildcard-import leakage from the touched app surface and remove the import-suppression sink in `app/mod.rs`.
     - Phase 2: apply only already-identified local mechanical cleanup in the touched surface, such as the remaining clone hot path in `pipeline_owner.rs`, without turning this into a broader abstraction pass.
   - Expected result: the active app surface no longer depends on `use super::*;` leakage, and the remaining localized cleanup is closed without introducing a new refactor axis.
   - Verification: `cargo test`, focused unit/regression tests for the touched flow boundaries, and explicit diff review for remaining wildcard imports.
3. Slice C: Closure validation and decision.
   - Files/modules/components: `docs/ARCHITECTURE.md`, `docs/DESIGN.md`, `docs/TESTPLAN.md`, `docs/TASKS.md`, `AGENTS.md`, and the completed slice outcomes.
   - Expected result: the follow-up roadmap is either safely closed or replaced with a narrower evidence-based follow-up plan.
   - Verification: `cargo test`, docs review, and an explicit close/continue decision.

## 7. Detailed Task Breakdown
- [ ] Finalize the remaining ownership boundary cleanup.
- [ ] Remove the lingering `Deref/DerefMut` and explicit ownership exposure that still increases sync risk.
- [ ] Pin the current Tab-Shell transition contract without rewriting the tab data model.
- [ ] Remove wildcard-import leakage from the touched app surface and `app/mod.rs`.
- [ ] Validate whether the resulting shape is sufficient to close the architecture follow-up.

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test`
- Manual checks:
  - focused review of state-boundary and import-boundary changes
- Regression focus:
  - ownership boundary correctness
  - tab/session state synchronization
  - wildcard import hygiene
  - closure correctness

## 9. Rollback Plan
- Keep the slices narrow enough that a partial revert is understandable and localized.
- If the closure slice shows the goal is still not met, keep the roadmap and slice docs in place and create a revised roadmap instead of forcing closure.
- If the remaining debt turns out to be a Tab-Shell rewrite, stop this roadmap and create a dedicated follow-up rather than widening Slice A.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `architecture-score-80-follow-up`, read the follow-up roadmap and slice plans before starting implementation.
- Follow the roadmap `Execution Mode` and `Execution Mode Policy`.
- Execute Slice A before Slice B, and treat the closure slice as the final gating step before the roadmap is closed.
- If a slice reveals that the ownership or import boundary needs a different shape, update the plan before continuing.
- If the closure slice shows the goal is still unmet, re-plan before continuing and do not remove the temporary rule.
- If the remaining debt would require a Tab-Shell data-model rewrite, do not absorb that rewrite into this roadmap.
- When the follow-up roadmap completes, remove this section and delete both the follow-up roadmap and the superseded 2026-04-13 roadmap/slice documents.
```

## 11. Progress Log
- 2026-04-14 Planned.
- 2026-04-14 Reviewed GO.
- 2026-04-17 Slice A implemented and validated with `cd rust && cargo test`. Slice B import hygiene remains the active follow-up.

## 12. Communication Plan
- Return the slice status after each implementation slice, and return with the close/continue decision after the closure slice.

## 13. Completion Checklist
- [ ] Planned document created before implementation
- [ ] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting notes moved into durable docs as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Follow-up roadmap and superseded roadmap documents deleted after completion

## 14. Final Notes
- This roadmap is intentionally narrower than the previous follow-up and targets only the blockers that still prevented closure.

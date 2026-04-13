# CHANGE PLAN: Architecture Score Uplift Follow-up from 72 to 80

## Metadata
- Date: 2026-04-13
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: roadmap
- Execution Mode: standard
- Execution Mode Policy: Execute one slice at a time, keep the slices narrowly tied to the remaining architectural debt, and re-evaluate after each slice instead of widening scope silently. Phase execution during implementation should be delegated to subagents by default when available; the main agent should act as orchestrator and reviewer. If the closure slice shows the goal is still unmet, stop and create a revised roadmap rather than silently expanding scope.
- Parent Plan: none
- Child Plan(s):
  - [docs/CHANGE-PLAN-20260413-slice-a-state-ownership-and-deref-removal.md](docs/CHANGE-PLAN-20260413-slice-a-state-ownership-and-deref-removal.md)
  - [docs/CHANGE-PLAN-20260413-slice-b-module-hygiene-and-testability.md](docs/CHANGE-PLAN-20260413-slice-b-module-hygiene-and-testability.md)
  - [docs/CHANGE-PLAN-20260413-slice-c-closure-validation.md](docs/CHANGE-PLAN-20260413-slice-c-closure-validation.md)
- Scope Label: architecture-score-72-to-80
- Related Tickets/Issues: none
- Review Status: レビュー済み
- Review Notes:
  - The external review on 2026-04-13 gives the current architecture a 72/100 score and identifies the remaining debt as structural half-finishedness rather than one more monolithic split.
  - The roadmap is intentionally split so that the highest-risk ownership issue is handled before broad hygiene and testability cleanup.
  - The Slice A review tightened the target shape to a tab-session owner boundary with transition-only snapshot helpers, so the plan is now specific enough to execute without widening scope into render/input.
  - The goal is to remove the remaining architectural debt without turning the roadmap into an open-ended polish project.

## 1. Background
- God Object decomposition succeeded, but the resulting architecture still has structural rough edges: explicit ownership boundaries are incomplete, the Tab-Shell 二重所有 pattern still exists, several modules are still too dense, and testability remains constrained by the current control-flow shape.
- The review indicates that the remaining work is not a single bug fix. It is a set of follow-up refactors that need to be ordered so the highest-risk ownership problems are solved first.
- The project already has strong docs and regression coverage, so the fastest path to an 80+ architecture score is to remove the half-finished structural patterns rather than add new features.

## 2. Goal
- Raise the architecture from 72/100 to a defensible 80+ by finishing the state-ownership boundary work and removing the remaining structural smells that still make the code harder to reason about.
- Make the remaining architecture easier to extend by resolving the Tab-Shell 二重所有 gap at the tab-session owner boundary, reducing boilerplate around worker APIs, render/input module density, and eliminating `use super::*;` pollution and other implicit imports.
- Keep the final decision evidence-based so the closure slice can justify either roadmap completion or a concrete follow-up roadmap.

## 3. Scope
### In Scope
- `rust/src/app/state.rs`
- `rust/src/app/tab_state.rs`
- `rust/src/app/tabs.rs`
- `rust/src/app/render.rs`
- `rust/src/app/input.rs`
- `rust/src/app/pipeline.rs`
- `rust/src/app/pipeline_owner.rs`
- `rust/src/app/result_reducer.rs`
- `rust/src/app/result_flow.rs`
- `rust/src/app/response_flow.rs`
- `rust/src/app/coordinator.rs`
- `rust/src/app/filelist.rs`
- `rust/src/app/update.rs`
- `rust/src/app/worker_bus.rs`
- `rust/src/app/workers.rs`
- `rust/src/app/index_coordinator.rs`
- `rust/src/app/search_coordinator.rs`
- `rust/src/app/tests/*`
- `docs/ARCHITECTURE.md`
- `docs/DESIGN.md`
- `docs/TESTPLAN.md`
- `docs/TASKS.md`
- `AGENTS.md`

### Out of Scope
- New user-facing features.
- Search or index algorithm redesign.
- Release packaging or installer work.
- Broad codebase rewrites unrelated to the remaining score gap.

## 4. Constraints and Assumptions
- UI responsiveness remains a hard constraint; no new blocking work should move onto the UI thread.
- Latest-wins async routing must continue to discard stale responses.
- The roadmap should not expand into unrelated refactors if the validation evidence is already sufficient to justify closure.

## 5. Current Risks
- Risk: state ownership work may still leave hidden direct mutation paths.
  - Impact: the code would look cleaner without actually becoming easier to reason about.
  - Mitigation: prioritize explicit owner APIs and remove the remaining transparent state exposure before broad cleanup.
- Risk: module splitting could become cosmetic if the split files still share the same uncontrolled dependency surface.
  - Impact: the project would remain difficult to navigate and test.
  - Mitigation: pair file splits with explicit imports, clear command seams, and test coverage that reflects the new boundaries.
- Risk: cleanup could overreach into performance or observability work that is not needed for closure.
  - Impact: the roadmap would become too broad to finish cleanly.
  - Mitigation: keep each slice tied to a concrete architectural question and let the closure slice decide whether the result is sufficient.

## 6. Execution Strategy
1. Slice A: State ownership, Tab-Shell 二重所有, and `Deref` removal.
   - Files/modules/components: `app/state.rs`, `app/tab_state.rs`, `app/tabs.rs`, `app/pipeline.rs`, `app/pipeline_owner.rs`, `app/response_flow.rs`, `app/result_reducer.rs`, `app/result_flow.rs`, `app/coordinator.rs`, `app/tests/*`.
   - Expected result: `TabSessionState` becomes the explicit owner boundary for tab transitions, the Tab-Shell 二重所有 gap is narrowed to transition-only snapshot helpers instead of transparent state access, and tab/shell ownership no longer depends on accidental field access.
   - Verification: `cargo test`, targeted ownership/regression tests, and docs diff review for the boundary model.
2. Slice B: Module hygiene, testability, and extensibility cleanup.
   - Files/modules/components: `app/mod.rs`, `app/render.rs`, `app/input.rs`, `app/worker_bus.rs`, `app/workers.rs`, `app/result_reducer.rs`, `app/pipeline_owner.rs`, `app/coordinator.rs`, `app/update.rs`, `app/filelist.rs`, `app/tests/*`, `docs/TESTPLAN.md`.
   - Expected result: `render.rs` and `input.rs` delegate the densest command/control-flow clusters to smaller free-function submodules, worker bus boilerplate is reduced through shared lifecycle helpers, `use super::*;` pollution is removed from the touched app modules and `app/mod.rs`, and one or two clearly bounded clone/observability hotspots are narrowed enough to support closure.
   - Verification: `cargo test`, focused unit/regression tests for input/render control flow, and docs/test-plan alignment review.
3. Slice C: Closure validation and decision.
   - Files/modules/components: `docs/ARCHITECTURE.md`, `docs/DESIGN.md`, `docs/TESTPLAN.md`, `docs/TASKS.md`, `AGENTS.md`, and the completed slice outcomes.
   - Expected result: the follow-up roadmap is either safely closed or replaced with a narrower, evidence-based follow-up plan.
   - Verification: `cargo test`, docs review, and an explicit close/continue decision.

## 7. Detailed Task Breakdown
- [ ] Remove the remaining transparent tab-session exposure and ownership ambiguity.
- [ ] Collapse the Tab-Shell 二重所有 gap where it still adds sync risk, while keeping snapshot helpers transition-only.
- [ ] Split the densest modules and remove `use super::*;` / boilerplate leakage.
- [ ] Improve testability, worker API shape, and the remaining observability/perf rough edges.
- [ ] Run closure validation and decide whether the architecture score target is reached.

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test`
- Manual checks:
  - focused GUI smoke if render/input control flow or module seams change materially
- Performance or security checks:
  - preserve the existing performance and security regression guards
- Regression focus:
  - tab-shell ownership
  - `Deref/DerefMut` exposure
  - `use super::*;` pollution
  - render/input command seams
  - worker lifecycle helpers
  - clone-heavy data flow
  - closure correctness

## 9. Rollback Plan
- Keep each slice narrow enough that a partial revert is understandable and localized.
- If the closure slice shows the goal is still not met, keep the plan machinery in place and create a revised roadmap instead of forcing closure.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `architecture-score-72-to-80`, read the roadmap and all slice plans before starting implementation.
- Follow the roadmap `Execution Mode` and `Execution Mode Policy`.
- Execute Slice A before Slice B, and keep the closure slice as the final gating step before the roadmap is closed.
- If a slice reveals that the ownership or module boundary needs a different shape, update the plan before continuing.
- If the closure slice shows the goal is still unmet, re-plan before continuing and do not remove the temporary rule.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-13 Planned.
- 2026-04-13 Reviewed and narrowed Slice A to a tab-session owner boundary, then executed the ownership cleanup and regression validation.
- 2026-04-13 `cd rust && cargo test` completed successfully after the Slice A ownership refactor.
- 2026-04-13 Slice B completed the render/input free-function split, explicit import hygiene on the touched surface, worker lifecycle helper extraction, filelist dialog None-guard tightening, history sync explicit iteration, and a bounded clone cleanup.
- 2026-04-13 `cd rust && cargo test` completed successfully after the Slice B cleanup.
- 2026-04-14 Slice C closure validation kept the roadmap open: `cargo test` is green, but Tab-Shell 二重所有, `Deref/DerefMut`, and wildcard-import hygiene remain material enough to warrant a follow-up pass.

## 12. Communication Plan
- Return to the user with the slice status after each implementation slice, and return with the close/continue decision after the closure slice.

## 13. Completion Checklist
- [ ] Planned document created before implementation
- [ ] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into durable docs as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 14. Final Notes
- This roadmap is intentionally narrower than the previous shell-boundary roadmap and focuses on the remaining architectural rough edges identified by the 2026-04-13 review.

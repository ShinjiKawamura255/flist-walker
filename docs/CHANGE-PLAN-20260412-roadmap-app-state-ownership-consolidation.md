# CHANGE PLAN: App State Ownership Consolidation Roadmap

## Metadata
- Date: 2026-04-12
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: roadmap
- Execution Mode: standard
- Execution Mode Policy: Keep the work bounded to the state-ownership and event-routing debt that remains after the shell-boundary cleanup. Execute Slice A first, validate the resulting ownership map and routing shape, then use the closure slice to decide whether the roadmap can be closed or needs a follow-up roadmap. Do not widen the scope mid-slice without updating the plan first.
- Parent Plan: none
- Child Plan(s):
  - [docs/CHANGE-PLAN-20260412-slice-a-state-ownership-and-event-routing.md](docs/CHANGE-PLAN-20260412-slice-a-state-ownership-and-event-routing.md)
  - [docs/CHANGE-PLAN-20260412-slice-b-closure-validation.md](docs/CHANGE-PLAN-20260412-slice-b-closure-validation.md)
- Scope Label: state-ownership-consolidation
- Related Tickets/Issues: none
- Review Status: 未レビュー
- Review Notes:
  - 実現性レビュー: feasible.
  - The remaining architecture debt is localized to duplicated ownership between runtime/tab/session state and the direct-mutation paths that still bypass reducer-style routing.
  - The plan is intentionally narrow enough to validate with `cargo test` and focused doc review.
  - Open question: whether the consolidation will be sufficient to close the architecture cleanup sequence without another follow-up roadmap.

## 1. Background
- The shell-boundary cleanup removed transparent shell exposure and moved the status line to render-time derivation, but the project still carries duplicated state ownership and multiple direct mutation paths.
- `AppRuntimeState`, `TabSessionState`, `AppTabState`, and UI transient state still require careful sync across capture/apply/restore boundaries.
- Some render/input/pipeline paths still mutate state directly instead of flowing through a clearer command or reducer seam.

## 2. Goal
- Make state ownership explicit enough that runtime, tab snapshot, and UI transient state each have a single obvious source of truth.
- Reduce the remaining direct-mutation paths that make event flow harder to reason about.
- Validate whether the resulting structure is strong enough to close the architecture-score uplift sequence.

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
- `rust/src/app/session.rs`
- `rust/src/app/root_browser.rs`
- `rust/src/app/shell_support.rs`
- `rust/src/app/tests/*`
- `docs/TESTPLAN.md`
- `docs/TASKS.md`

### Out of Scope
- New user-facing features.
- Search/index algorithm redesign.
- Release packaging or updater changes.
- Broad refactors outside the remaining ownership/routing cleanup.

## 4. Constraints and Assumptions
- UI responsiveness remains a hard constraint.
- Latest-wins async routing must continue to discard stale responses.
- The plan should not expand into unrelated architecture rewrites unless validation shows the remaining debt is still too large.

## 5. Current Risks
- Risk: state ownership is still duplicated after a mechanical rewrite.
  - Impact: the code would compile but remain hard to reason about.
  - Mitigation: define the live/snapshot boundary explicitly and confirm it with tests.
- Risk: event-routing cleanup could leave direct mutations in render/input paths.
  - Impact: the architecture would remain imperative in practice.
  - Mitigation: treat reducer/command seams as the preferred route for state transitions and keep the render path focused on observation and command collection.
- Risk: the remaining debt may still be too large to close in one pass.
  - Impact: closure would be premature.
  - Mitigation: use the closure slice to decide explicitly whether to close or continue.

## 6. Execution Strategy
1. Slice A: State ownership and event-routing consolidation.
   - Files/modules/components: `state.rs`, `tab_state.rs`, `tabs.rs`, `render.rs`, `input.rs`, `pipeline.rs`, `pipeline_owner.rs`, `result_reducer.rs`, `result_flow.rs`, `response_flow.rs`, `coordinator.rs`, `session.rs`, `root_browser.rs`, `shell_support.rs`, and the affected tests.
   - Expected result: live runtime state, tab snapshot state, and UI transient state have clearer ownership boundaries, and the remaining direct mutation paths are reduced or isolated behind a clearer command/reducer seam.
   - Verification: `cargo test` plus targeted review of state capture/apply and event-routing paths.
2. Slice B: Closure validation and decision.
   - Files/modules/components: `docs/ARCHITECTURE.md`, `docs/DESIGN.md`, `docs/TESTPLAN.md`, `docs/TASKS.md`, `AGENTS.md`, and the completed slice outcomes.
   - Expected result: a concrete close/continue decision based on the resulting code shape and validation evidence.
   - Verification: `cargo test`, docs review, and an explicit closure note.

## 7. Detailed Task Breakdown
- [ ] Reduce ownership duplication between runtime, tab snapshot, and UI transient state.
- [ ] Tighten the event-routing / reducer boundary for remaining direct mutations.
- [ ] Validate whether the architecture debt is now small enough to close.
- [ ] Update durable docs and temporary plan machinery according to the closure decision.

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test`
- Manual checks:
  - focused GUI smoke if the state ownership or render/event flow changes noticeably
- Regression focus:
  - snapshot/live-state sync
  - render/input direct mutation
  - stale response discard
  - closure correctness

## 9. Rollback Plan
- Keep the plan narrow enough that a slice-level revert remains understandable.
- If the closure slice shows the goal is not met, keep the temporary plan machinery in place and re-plan instead of forcing closure.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `architecture-score-80`, read this roadmap and its slice plans before starting implementation.
- Follow the roadmap `Execution Mode` and `Execution Mode Policy`.
- Execute Slice A first, then the closure slice, unless the plan is updated first.
- Treat the closure slice as the final gating step before the roadmap is closed.
- If the closure slice shows the goal is still unmet, re-plan before continuing and do not remove the temporary rule.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-12 Planned.

## 12. Communication Plan
- Return to the user with the closure decision and any follow-up plan if the target is still not met.

## 13. Completion Checklist
- [ ] Planned document created before implementation
- [ ] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into durable docs as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 14. Final Notes
- This roadmap is the next consolidation pass after the shell-boundary cleanup.

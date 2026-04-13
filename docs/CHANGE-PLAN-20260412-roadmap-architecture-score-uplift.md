# CHANGE PLAN: Architecture Score Uplift to 80

## Metadata
- Date: 2026-04-12
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: roadmap
- Execution Mode: standard
- Execution Mode Policy: Start from the reviewed roadmap, execute one slice at a time, and re-evaluate after each slice instead of auto-extending work. Phase execution during implementation should be delegated to subagents by default when available; the main agent should act as orchestrator and reviewer. If the goal still appears unmet at the closure slice, stop and create a revised roadmap rather than silently expanding scope.
- Parent Plan: none
- Child Plan(s):
  - [docs/CHANGE-PLAN-20260412-slice-a-shell-boundary-hardening.md](docs/CHANGE-PLAN-20260412-slice-a-shell-boundary-hardening.md)
  - [docs/CHANGE-PLAN-20260412-slice-b-routing-and-lifecycle-consolidation.md](docs/CHANGE-PLAN-20260412-slice-b-routing-and-lifecycle-consolidation.md)
  - [docs/CHANGE-PLAN-20260412-slice-c-closure-validation.md](docs/CHANGE-PLAN-20260412-slice-c-closure-validation.md)
- Scope Label: architecture-score-80
- Related Tickets/Issues: none
- Review Status: レビュー済み
- Review Notes:
  - 実現性レビュー: feasible.
  - Current architecture already exposes owner seams, split modules, and validation gates, so the remaining work is primarily boundary tightening and closure validation.
  - No external dependency or release blocker is required for this roadmap.
  - External architecture review feedback received on 2026-04-12: the current score is estimated at 73/100, with main gaps in shell transparency, direct mutation / side-effect mixing, state duplication across runtime vs persisted tab state, and imperative UI-derived updates.
  - This roadmap supersedes the earlier app-state-ownership consolidation sequence and is the current execution baseline for `architecture-score-80`.
  - Open question: the "80点" target is qualitative, so Slice C must decide closure by concrete indicators instead of intuition.
  - Roadmap review on 2026-04-13: feasible and complete enough to proceed; no scope or ordering change required before Slice A.

## 1. Background
- The current architecture has already moved far beyond a monolith, but the remaining score gap is concentrated in shell ownership, request-routing clarity, and closure discipline.
- `app/mod.rs` still acts as a broad coordinator across multiple feature areas, and several ownership boundaries are documented but not yet fully converged in implementation and validation.
- The project already has strong docs and test scaffolding, so the next score increase should come from making the remaining seams explicit instead of adding new features.
- A recent architecture review identified the remaining debt more sharply: the module split is real, but `Deref` chains and direct field mutation still make the structure behave like a disguised God Object.
- This roadmap is the active plan for closing the remaining gap to 80+ and should be treated as the authoritative execution baseline for the current workstream.
- The highest-value improvement is therefore not another file split, but a stricter one-way data flow: explicit access to state, explicit message handling, and more derived UI data.

## 2. Goal
- Raise the architecture to a level that is defensible as "80 points" by tightening the shell boundary, consolidating routing/lifecycle ownership, and proving the result through validation.
- The result should be visible in three ways:
  - the top-level shell remains thin and clearly bounded,
  - request routing and response application follow one owner path per concern,
  - direct state mutation is constrained behind explicit message/reducer boundaries,
  - the closure slice can justify either roadmap completion or a justified follow-up roadmap.
- This roadmap must end with a `closure slice` that validates the goal and decides whether to close or continue.

## 3. Scope
### In Scope
- Shell boundary tightening around `app/mod.rs`, `app/state.rs`, `app/tab_state.rs`, `app/ui_state.rs`, `app/query_state.rs`, and `app/shell_support.rs`.
- Routing and lifecycle consolidation around `app/pipeline.rs`, `app/pipeline_owner.rs`, `app/search_coordinator.rs`, `app/index_coordinator.rs`, `app/response_flow.rs`, `app/result_reducer.rs`, `app/preview_flow.rs`, `app/filelist.rs`, `app/update.rs`, `app/tabs.rs`, and `app/render.rs`.
- Explicit message/reducer pathways for the remaining direct-mutation hotspots, especially state updates that currently happen in response handlers and pipeline methods.
- Derived UI state treatment for strings and labels that can be computed at render time instead of being stored and manually refreshed.
- Documentation and validation sync in `docs/ARCHITECTURE.md`, `docs/DESIGN.md`, `docs/TESTPLAN.md`, and `docs/TASKS.md`.
- Temporary plan machinery, including the `AGENTS.md` temporary rule for this roadmap.

### Out of Scope
- New user-facing features.
- Search algorithm redesign.
- FileList/walker semantic changes unrelated to ownership boundaries.
- Release packaging or installer work.
- Prototype code under `prototype/python/`.

## 4. Constraints and Assumptions
- UI responsiveness must remain intact; no heavy I/O or blocking work may move back onto the UI thread.
- Latest-wins behavior based on `request_id` must remain the acceptance rule for async responses.
- Existing validation matrix in `docs/TESTPLAN.md` stays authoritative for verification.
- The roadmap assumes that the remaining architectural debt is localized enough to close in a small number of slices.

## 5. Current Risks
- Risk: Boundary work could spread across too many modules and become a diffuse cleanup.
  - Impact: The roadmap would drift from score uplift into unbounded refactoring.
  - Mitigation: Keep slice boundaries tied to one architectural question each and require the closure slice to decide whether to stop.
- Risk: A partial reducer rewrite could leave "old style" mutation pathways alive beside the new ones.
  - Impact: The architecture would look cleaner while still relying on hidden direct state writes.
  - Mitigation: Treat `Deref` removal, explicit state access, and reducer/message routing as a single architectural contract rather than optional polish.
- Risk: Validation could miss GUI regressions even if module boundaries look cleaner.
  - Impact: The score would improve on paper but not in actual use.
  - Mitigation: Keep targeted GUI/manual checks in the validation plan and preserve the existing regression guards.
- Risk: Documentation could lag behind implementation changes.
  - Impact: Future work would reintroduce ambiguity about ownership.
  - Mitigation: Update architecture, design, test plan, and task records in the same roadmap.

## 6. Execution Strategy
1. Slice A: Shell boundary hardening
   - Files/modules/components: `app/mod.rs`, `app/state.rs`, `app/tab_state.rs`, `app/ui_state.rs`, `app/query_state.rs`, `app/shell_support.rs`, `docs/ARCHITECTURE.md`, `docs/DESIGN.md`.
   - Expected result: `FlistWalkerApp` no longer behaves like a transparent shell over all runtime state; access to shell/runtime/tab state becomes explicit rather than leaked through chained `Deref`.
   - Verification: owner-focused unit tests, docs diff review, and `cargo test` for any Rust changes.
2. Slice B: Routing and lifecycle consolidation
   - Files/modules/components: `app/pipeline.rs`, `app/pipeline_owner.rs`, `app/search_coordinator.rs`, `app/index_coordinator.rs`, `app/response_flow.rs`, `app/result_reducer.rs`, `app/preview_flow.rs`, `app/filelist.rs`, `app/update.rs`, `app/tabs.rs`, `app/render.rs`, `docs/TESTPLAN.md`.
   - Expected result: Each async concern has one clear owner path for request routing, stale response handling, response application, and side-effect emission; status/label-style derived UI data is no longer refreshed imperatively at arbitrary call sites.
   - Verification: `cargo test`, targeted owner/regression tests, and a focused GUI smoke check for routing-sensitive flows.
3. Slice C: Closure validation and decision
   - Files/modules/components: `docs/ARCHITECTURE.md`, `docs/DESIGN.md`, `docs/TESTPLAN.md`, `docs/TASKS.md`, `AGENTS.md`, and the completed slice results.
   - Expected result: The roadmap is either safely closed or reopened with a concrete, evidence-based follow-up plan.
   - Verification: full relevant validation matrix, closure review, and a documented decision about continue vs close.

## 7. Detailed Task Breakdown
- [ ] Tighten the shell/state boundary and record the remaining ownership model.
- [ ] Consolidate routing and lifecycle ownership so each async concern has a single obvious owner.
- [ ] Update architecture/design/test docs to match the final implementation shape.
- [ ] Run the closure validation and decide whether the roadmap can close.

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test`
  - Any targeted owner tests needed by the changed slices
- Manual checks:
  - GUI smoke focused on search, tab switching, and routing-sensitive flows
- Performance or security checks:
  - Preserve existing regression guards; do not introduce new blocking work on the UI thread
- Regression focus:
  - stale response discard
  - background tab routing
  - shell ownership boundaries
  - direct mutation leakage
  - derived UI state refresh behavior
  - docs/test traceability

## 9. Rollback Plan
- Shell boundary changes should be revertible per slice because each slice is tied to a narrow ownership question.
- Routing/lifecycle consolidation should be reverted together with the tests and docs that explain the owner boundaries.
- If the closure slice shows the goal is not met, do not partially close the roadmap; reopen with a fresh plan instead.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `architecture-score-80`, read the relevant change plan documents before starting implementation.
- Read them from upper to lower order:
  - `[docs/CHANGE-PLAN-20260412-roadmap-architecture-score-uplift.md]`
  - `[docs/CHANGE-PLAN-20260412-slice-a-shell-boundary-hardening.md]`
  - `[docs/CHANGE-PLAN-20260412-slice-b-routing-and-lifecycle-consolidation.md]`
  - `[docs/CHANGE-PLAN-20260412-slice-c-closure-validation.md]`
- Follow the roadmap `Execution Mode` and `Execution Mode Policy`.
- Execute the work in the documented order unless the plan is updated first.
- If scope, order, or risk changes, update the change plan before continuing.
- Treat the `closure slice` as the final gating step before the roadmap is closed.
- Delegate phase execution to subagents by default when implementation begins; the main agent should act as orchestrator and reviewer.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-12 Planned.
- 2026-04-13 Refreshed as the active execution baseline and aligned with the current `architecture-score-80` temporary rule.
- 2026-04-13 Slice A implementation began with shell boundary hardening around `TabSessionState`.
- 2026-04-13 Slice A was further tightened with explicit owner API for active tab, tab id, pending restore refresh, and request routing, and the Rust test suite stayed green.
- 2026-04-13 Slice B routing consolidation started by moving index response routing into `IndexCoordinator::route_response`, trimming direct request-tab inspection in `pipeline.rs`, and consolidating worker lifecycle helpers for preview/action/sort/index flows.

## 12. Communication Plan
- Return to user when:
  - roadmap and slice plans are created and reviewed,
  - the closure slice is complete,
  - a blocking problem requires re-planning.

## 13. Completion Checklist
- [ ] Planned document created before implementation
- [ ] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] If the project is under git control, each commit corresponds to an independent verification/rollback unit, and grouped phases are documented in the plan
- [ ] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, and `TESTPLAN.md` as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 14. Final Notes
- Before deleting this plan, move any lasting decisions into `REQUIREMENTS.md`, `SPEC.md`, `DESIGN.md`, or `TESTPLAN.md`.

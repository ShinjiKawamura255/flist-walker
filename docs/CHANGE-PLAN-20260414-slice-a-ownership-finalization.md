# CHANGE PLAN: Slice A - Ownership Finalization

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
- Scope Label: ownership-finalization
- Related Tickets/Issues: none
- Review Status: レビュー済み
- Review Notes:
  - The slice is limited to the remaining ownership leakage and `Deref/DerefMut` exposure that still keep the architecture boundary partially transparent.
  - The intent is to close the last mechanical ownership gap without reopening the render/input or worker-bus work from the previous roadmap.
  - This slice is intentionally bounded to the manager/state surfaces and to test/docs pinning of the existing tab-session transition helpers; it is **not** a full Tab-Shell rewrite.
  - Review result: GO after adding explicit allowed/disallowed edits and a compile-time contract test for tab-state field drift.

## 1. Background
- The closure review found that the previous roadmap did not fully eliminate the ownership ambiguity it set out to remove.
- The remaining work is to make the owner/state boundary explicit enough that tab/session transitions and manager state are no longer semi-transparent.
- The tab-session data model itself is already in active use, so this slice must not widen into a rewrite of that model.

## 2. Goal
- Remove the remaining `Deref/DerefMut` exposure and other transparent ownership paths that still remain material.
- Tighten the state boundary so the code is easier to reason about.
- Pin the current tab/session transition contract with tests and docs so the remaining boundary is explicit, but do not rewrite the tab model in this slice.

## 3. Scope
### In Scope
- `rust/src/app/state.rs`
- `rust/src/app/tab_state.rs`
- `rust/src/app/tabs.rs`
- `rust/src/app/pipeline_owner.rs`
- `rust/src/app/tests/session_tabs.rs`
- `docs/ARCHITECTURE.md`
- `docs/DESIGN.md`

### Out of Scope
- Import-hygiene cleanup.
- Render/input or worker-bus work already addressed in the previous roadmap.
- New user-facing behavior.
- Any field additions/removals on `TabQueryState`, `TabIndexState`, `TabResultState`, or `AppTabState`.
- Any new Tab-Shell sync path, new helper layer, or tab data-model rewrite.

## 4. Constraints and Assumptions
- Keep the state-boundary changes mechanical and local.
- Preserve existing behavior and test coverage.
- Do not expand into a new Tab-Shell data-model rewrite; any tab-session changes must stay within the current explicit helper surface.
- If a required fix would need a new copy path or field-level model change, stop and re-plan.

## 5. Current Risks
- Risk: changing the owner boundary could reintroduce tab sync bugs.
  - Impact: regressions in tab switch or restore flows.
  - Mitigation: keep the changes tied to existing regression tests and update docs with the new boundary shape.
- Risk: the slice could drift into a hidden Tab-Shell rewrite.
  - Impact: the ownership cleanup would expand beyond the remaining mechanical debt.
  - Mitigation: only allow test/docs pinning for `tab_state.rs`; do not change the tab-session data model here.

## 6. Phase Plan
1. Remove the remaining transparent state exposure.
   - Remove `impl DerefMut for FileListManager` and `impl DerefMut for UpdateManager`.
   - Update only the call sites required to compile after those impls are removed. The allowed call-site-only files for this phase are `rust/src/app/filelist.rs`, `rust/src/app/update.rs`, `rust/src/app/coordinator.rs`, `rust/src/app/pipeline.rs`, `rust/src/app/render.rs`, `rust/src/app/worker_runtime.rs`, and any tests that fail because of the removal.
   - Do not change `TabSessionState`, `AppTabState`, or any `from_shell` / `apply_shell` bodies in this phase.
   - Success condition: `rg -n "impl DerefMut for FileListManager|impl DerefMut for UpdateManager" rust/src/app/state.rs` returns no matches, and `cargo test` still passes.
2. Tighten the tab-session ownership boundary.
   - Keep the existing `from_shell` / `apply_shell` transition path as the only copy point between shell state and tab snapshots.
   - Allowed edits in `tab_state.rs`: comments, boundary notes, and test-only assertions that document the existing contract. No field additions/removals, no helper signature changes, and no logic changes to the bodies of `from_shell` / `apply_shell`.
   - If `tabs.rs` or `pipeline_owner.rs` needs a follow-on touch because of phase 1, those edits are call-site-only and must not alter tab creation, switching, restore behavior, or lifecycle policy.
   - Pin the current tab-switch / restore behavior with `session_tabs.rs` tests, including one contract test that instantiates the tab-state structs with full field literals so field additions/removals fail at compile time.
   - Success condition: the tab/session boundary stays auditable, the helper surface is unchanged apart from tests/docs, the contract test compiles, and no new sync paths or helper layers are introduced.

## 7. Detailed Task Breakdown
- [ ] Remove `DerefMut` exposure from `FileListManager` and `UpdateManager`.
- [ ] Add a tab-state contract test that instantiates the relevant tab/session structs with full field literals and round-trips the existing transition helpers.
- [ ] Leave the tab/session snapshot data model unchanged while pinning its existing transition helpers with tests and comments only.
- [ ] Re-run validation and update the durable docs if the boundary model changes.

## 8. Validation Plan
- `cd rust && cargo test`
- `cd rust && cargo test session_tabs::tab_state_contract -- --nocapture` or the equivalent new contract test name added for this slice

## 9. Rollback Plan
- Keep each change localized so the ownership boundary can be reverted without undoing the entire follow-up roadmap.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `architecture-score-80-follow-up`, read the follow-up roadmap and this slice plan before starting implementation.
- Follow the roadmap `Execution Mode` and `Execution Mode Policy`.
- Keep the ownership cleanup bounded to the documented slice.
- Do not expand the slice into a Tab-Shell data-model rewrite.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-14 Planned.
- 2026-04-14 Reviewed GO.
- 2026-04-17 Implemented `DerefMut` removal and the tab-state contract regression; `cd rust && cargo test` passed.

## 12. Completion Checklist
- [ ] Planned document created before implementation
- [ ] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting notes moved into durable docs as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Follow-up roadmap and superseded roadmap documents deleted after completion

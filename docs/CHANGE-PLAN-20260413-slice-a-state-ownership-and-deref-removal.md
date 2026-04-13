# CHANGE PLAN: Slice A - State Ownership and `Deref` Removal

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
- Scope Label: state-ownership-and-deref-removal
- Related Tickets/Issues: none
- Review Status: レビュー済み
- Review Notes:
  - The slice targets the most structural problem in the review: the remaining semi-transparent state ownership.
  - The review narrowed the target shape to the tab-session owner boundary so the slice can finish cleanly without dragging render/input into scope.
  - The goal is to remove the unbounded internal state exposure rather than merely hide it behind more helper calls.

## 1. Background
- The project has already removed the most obvious God Object shape, but the tab/session boundary is still too permeable because tab ownership is split between shell runtime state and tab snapshots.
- `TabSessionState` still needs to become the explicit owner boundary for tab transitions so the live/snapshot split is only used as a transition helper, not as a general access path.

## 2. Goal
- Make the tab/session ownership boundary explicit enough that internal state is not exposed by default.
- Reduce the tab/shell duplicate ownership surface to the minimum needed for transition snapshots.
- Make the Tab-Shell 二重所有 gap visible, then narrow it to transition-only snapshot helpers instead of implicit sync conventions.

## 3. Scope
### In Scope
- `rust/src/app/state.rs`
- `rust/src/app/tab_state.rs`
- `rust/src/app/tabs.rs`
- `rust/src/app/pipeline.rs`
- `rust/src/app/pipeline_owner.rs`
- `rust/src/app/response_flow.rs`
- `rust/src/app/result_reducer.rs`
- `rust/src/app/result_flow.rs`
- `rust/src/app/coordinator.rs`
- `rust/src/app/tests/*`

### Out of Scope
- Module splitting unrelated to ownership.
- Worker abstraction, clone cleanup, or observability work unless it is needed to complete the ownership boundary.

## 4. Constraints and Assumptions
- Existing regression tests should continue to pass.
- The slice should prefer explicit APIs over further transparent state exposure, even if the snapshot helpers remain in place for transition boundaries.

## 5. Current Risks
- Risk: removing transparent tab-session access may reveal more call sites that depended on implicit slice semantics.
  - Impact: the slice could expand unexpectedly.
  - Mitigation: keep the API surface small and update only the callers that are directly part of the tab transition boundary.
- Risk: narrowing the Tab-Shell 二重所有 surface could expose more state duplication than initially expected.
  - Impact: the slice could reveal further sync hotspots in tab restore and routing.
  - Mitigation: keep the live/snapshot distinction only where it is concretely needed and document any remaining split as transition-only.
- Risk: collapsing duplicate ownership mechanically could make restore/snapshot logic more brittle.
  - Impact: sync bugs could be introduced.
  - Mitigation: preserve the live/snapshot distinction only where it is genuinely needed for restore or routing, and avoid changing unrelated render/input seams in this slice.

## 6. Phase Plan
1. Remove transparent internal-state exposure from manager/state wrappers.
   - Delete or replace the `TabSessionState` transparent slice exposure that makes tab ownership behave like a plain vector.
   - Add or use explicit API methods for the remaining operations that need to be available.
   - Success condition: callers no longer mutate tab-session internals through transparent slice access.
2. Tighten the tab/shell ownership boundary.
   - Keep `from_shell` / `apply_shell` as transition-only snapshot helpers instead of a general ownership bridge.
   - Make the Tab-Shell 二重所有 pattern explicit in the code and shrink the sync surface around active tab transitions, restore, and response routing.
   - Success condition: tab switch / restore / routing all pass through explicit owner methods, and the transition boundary is obvious at each call site.
3. Validate and align tests/docs.
   - Update or add tests that exercise tab switch, restore, and response routing after the ownership changes.
   - Sync durable docs only if the ownership model meaningfully changes.
   - Success condition: `cargo test` stays green and the ownership story is documented clearly.

## 7. Detailed Task Breakdown
- [ ] Remove the tab-session transparent slice exposure that exposes internals too broadly.
- [ ] Clarify the remaining live vs snapshot ownership boundaries.
- [ ] Resolve or explicitly bound the Tab-Shell 二重所有 gap as transition-only helpers.
- [ ] Re-run validation and record any lasting ownership decisions.

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test`
- Manual checks:
  - targeted GUI smoke if tab/session ownership behavior changes noticeably
- Regression focus:
  - transparent state exposure
  - tab-shell sync
  - Tab-Shell 二重所有
  - response routing after ownership changes

## 9. Rollback Plan
- Keep the changes grouped so a partial rollback does not restore the previous transparent ownership shape.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `architecture-score-72-to-80`, read the roadmap and this slice plan before starting implementation.
- Follow the roadmap `Execution Mode` and `Execution Mode Policy`.
- Execute Slice A before Slice B unless the plan is updated first.
- If the ownership boundary needs a different shape, update the plan before continuing.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-13 Planned.
- 2026-04-13 Reviewed, narrowed to the tab-session owner boundary, and implemented with regression validation.
- 2026-04-13 `cd rust && cargo test` passed after removing `TabSessionState`'s transparent slice exposure.

## 12. Communication Plan
- Return a concise summary of the ownership changes and any remaining gap before closure.

## 13. Completion Checklist
- [ ] Planned document created before implementation
- [ ] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting notes moved into durable docs as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 14. Final Notes
- This slice is the first implementation slice in the roadmap.

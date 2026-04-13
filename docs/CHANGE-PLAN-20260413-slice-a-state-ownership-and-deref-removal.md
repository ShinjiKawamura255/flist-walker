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
- Review Status: 未レビュー
- Review Notes:
  - The slice targets the most structural problem in the review: the remaining semi-transparent state ownership.
  - The goal is to remove the unbounded internal state exposure rather than merely hide it behind more helper calls.

## 1. Background
- The project has already removed the most obvious God Object shape, but `Deref/DerefMut` and the Tab-Shell 二重所有 pattern still make the remaining structure too permeable.
- The current state-management shape makes it too easy to reach across boundaries directly, which weakens the architectural value of the module split.

## 2. Goal
- Make the remaining ownership boundaries explicit enough that internal state is not exposed by default.
- Reduce the tab/shell duplicate ownership surface so the code is easier to reason about and harder to mutate accidentally.
- Make the Tab-Shell 二重所有 gap visible and then eliminate or clearly bound it rather than leaving it as an implicit sync convention.

## 3. Scope
### In Scope
- `rust/src/app/state.rs`
- `rust/src/app/tab_state.rs`
- `rust/src/app/tabs.rs`
- `rust/src/app/filelist.rs`
- `rust/src/app/update.rs`
- `rust/src/app/pipeline.rs`
- `rust/src/app/pipeline_owner.rs`
- `rust/src/app/response_flow.rs`
- `rust/src/app/result_reducer.rs`
- `rust/src/app/result_flow.rs`
- `rust/src/app/coordinator.rs`
- `rust/src/app/session.rs`
- `rust/src/app/root_browser.rs`
- `rust/src/app/tests/*`

### Out of Scope
- Module splitting unrelated to ownership.
- Worker abstraction, clone cleanup, or observability work unless it is needed to complete the ownership boundary.

## 4. Constraints and Assumptions
- Existing regression tests should continue to pass.
- The slice should prefer explicit APIs over further transparent state exposure.

## 5. Current Risks
- Risk: removing `Deref` may reveal more sites that depended on transparent state access.
  - Impact: the slice could expand unexpectedly.
  - Mitigation: keep the API surface small and update only the callers that are directly part of the ownership boundary.
- Risk: collapsing the Tab-Shell 二重所有 surface could expose more state duplication than initially expected.
  - Impact: the slice could reveal further sync hotspots in tab restore and routing.
  - Mitigation: keep the live/snapshot distinction only where it is concretely needed and document any remaining split.
- Risk: collapsing duplicate ownership could make restore/snapshot logic more brittle if it is done mechanically.
  - Impact: sync bugs could be introduced.
  - Mitigation: preserve the live/snapshot distinction only where it is genuinely needed for restore or routing.

## 6. Phase Plan
1. Remove transparent internal-state exposure from manager/state wrappers.
   - Delete or replace `Deref/DerefMut` where they make manager internals effectively public.
   - Add explicit API methods for the remaining operations that need to be available.
   - Success condition: callers no longer mutate manager internals through transparent deref access.
2. Tighten the tab/shell ownership boundary.
   - Reduce the remaining mirrored fields or make the sync path explicit enough to reason about.
   - Make the Tab-Shell 二重所有 pattern explicit in the code and then shrink it where the split is not actually buying isolation.
   - Keep tab/session and runtime ownership clear for active tab transitions, restore, and response routing.
   - Success condition: the state owner is obvious at each transition point.
3. Validate and align tests/docs.
   - Update or add tests that exercise the ownership boundary if the API changes materially.
   - Sync durable docs only if the ownership model meaningfully changes.
   - Success condition: `cargo test` stays green and the ownership story is documented clearly.

## 7. Detailed Task Breakdown
- [ ] Remove `Deref/DerefMut` from the state/manager wrappers that expose internals too broadly.
- [ ] Clarify the remaining live vs snapshot ownership boundaries.
- [ ] Resolve or explicitly bound the Tab-Shell 二重所有 gap.
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

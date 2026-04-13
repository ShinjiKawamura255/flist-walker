# CHANGE PLAN: Slice B - Module Hygiene and Testability Cleanup

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
- Scope Label: module-hygiene-and-testability
- Related Tickets/Issues: none
- Review Status: レビュー済み
- Review Notes:
  - The slice bundles the remaining non-ownership architectural debt: module density, wildcard imports, worker boilerplate, and testability seams.
  - The review also called out observability and data-flow rough edges, so the slice keeps those in view without broadening into feature work.
  - The slice is intentionally bounded to free-function submodules and helper extraction rather than introducing new `impl FlistWalkerApp` scattering.
  - The implementation later addressed the review findings by removing the `app/mod.rs` suppression blocks, deleting dead wrapper methods after the helper split, guarding `current_filelist_dialog_kind` against missing active tabs, making query-history tab sync explicit, and narrowing the incremental clone path.

## 1. Background
- The biggest ownership problem is separate from the biggest readability and extensibility problem.
- After ownership boundaries are explicit, the remaining issue is the amount of ceremony and hidden coupling that still makes the modules harder to evolve.
- `use super::*;` is one of the most visible forms of that hidden coupling, and it needs to be treated as a structural problem rather than a style nit.

## 2. Goal
- Make the densest modules more navigable and easier to test.
- Reduce boilerplate and hidden dependency leakage so the worker and UI seams are more maintainable.
- Replace `use super::*;` dependency leakage with explicit imports in the touched modules.
- Address the remaining clone-heavy and observability rough edges that still reduce the architecture score.

## 3. Scope
### In Scope
- `rust/src/app/mod.rs`
- `rust/src/app/render.rs`
- `rust/src/app/input.rs`
- `rust/src/app/worker_bus.rs`
- `rust/src/app/workers.rs`
- `rust/src/app/result_reducer.rs`
- `rust/src/app/pipeline_owner.rs`
- `rust/src/app/coordinator.rs`
- `rust/src/app/update.rs`
- `rust/src/app/filelist.rs`
- `rust/src/app/tests/*`
- `docs/ARCHITECTURE.md`
- `docs/DESIGN.md`
- `docs/TESTPLAN.md`

### Out of Scope
- New user-facing features.
- Any redesign that would be better handled by the ownership slice.

## 4. Constraints and Assumptions
- The slice should preserve current behavior while reducing structural friction.
- Module splits should improve clarity and testability, not just reduce file size.
- The render/input split should move command/control-flow clusters into free-function submodules instead of creating more `impl FlistWalkerApp` files.

## 5. Current Risks
- Risk: splitting modules without introducing clear seams would produce more files without reducing coupling.
  - Impact: the codebase would look more modular but remain just as hard to reason about.
  - Mitigation: pair file splits with explicit imports, command collection seams, and targeted tests, and keep the extracted code as free functions unless an `impl` is unavoidable.
- Risk: observability changes could become noisy if they are not tied to real failure paths.
  - Impact: logging would increase without making diagnosis better.
  - Mitigation: keep the focus on structured, actionable error paths rather than generic logging expansion.

## 6. Phase Plan
1. Split the densest UI/control modules into smaller seams.
   - Break up `render.rs` and `input.rs` along render/panel/dialog and shortcut/IME/control-flow seams.
   - Move the extracted clusters into new helper submodules that expose free functions, not additional `impl FlistWalkerApp` surfaces.
   - Preserve the command collection / dispatch boundaries so the GUI flow remains testable.
   - Success condition: the largest modules delegate clearly named clusters to submodules, and the control flow is easier to follow without adding more `impl` scatter.
2. Remove wildcard-import and boilerplate leakage.
   - Replace broad `use super::*` patterns in touched modules with explicit imports.
   - Update `app/mod.rs` so it no longer relies on wildcard leakage plus unused-import suppressions for the touched surface.
   - Introduce small shared helpers where worker bus boilerplate is repeated, keeping trait abstraction as a follow-up unless the helper shape is clearly insufficient.
   - Success condition: dependencies are visible in the module source, `use super::*;` pollution is materially reduced on the touched surface, and worker lifecycle boilerplate is reduced without a new generic abstraction layer.
3. Address the remaining data-flow / observability rough edges.
   - Trim the clearly avoidable clone-heavy path(s) identified during implementation.
   - Improve error/notice handling for the touched hotspots so the remaining observability story is less dependent on a single notice string path.
   - Success condition: the identified performance and operational rough spots are narrowed enough to support closure.
4. Validate testability improvements.
   - Add or adjust tests for the newly split seams and command/control routing.
   - Success condition: GUI/control-flow behavior is more directly exercised by tests than before.

## 7. Detailed Task Breakdown
- [ ] Split the densest render/input paths into smaller, testable seams.
- [ ] Remove `use super::*;` leakage from the touched modules and `app/mod.rs`.
- [ ] Reduce worker bus boilerplate and the identified clone-heavy data path(s).
- [ ] Improve the remaining observability rough edge(s) in the touched hotspots.
- [ ] Re-run validation and record the lasting testability implications.

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test`
- Manual checks:
  - focused GUI smoke if render/input module seams change materially
- Regression focus:
  - command collection
  - shortcut and dialog control flow
  - explicit imports instead of wildcard leakage
  - worker lifecycle helper behavior
  - clone-heavy paths
  - observability/error-path consistency
  - `app/mod.rs` import hygiene

## 9. Rollback Plan
- Keep each structural change localized enough that reverting one seam does not require reverting the entire slice.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `architecture-score-72-to-80`, read the roadmap and slice plans before starting implementation.
- Follow the roadmap `Execution Mode` and `Execution Mode Policy`.
- Execute Slice A before Slice B unless the plan is updated first.
- Keep the module hygiene and testability work bounded to the documented slice.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-13 Planned.
- 2026-04-13 Narrowed to free-function submodules, explicit imports on the touched surface, helper-based worker cleanup, and bounded hotspot fixes after review.
- 2026-04-13 Implemented the render/input splits, explicit import hygiene, worker bus lifecycle helper, `current_filelist_dialog_kind` guard, `sync_shared_query_history_to_tabs` explicit iteration, and the incremental clone cleanup.
- 2026-04-13 `cd rust && cargo test` completed successfully after the Slice B cleanup.

## 12. Communication Plan
- Return the outcome of the module/testability cleanup and note any leftover architectural debt before closure.

## 13. Completion Checklist
- [ ] Planned document created before implementation
- [ ] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting notes moved into durable docs as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 14. Final Notes
- This slice is the second implementation slice in the roadmap.

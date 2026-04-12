# CHANGE PLAN: Slice A - Shell Boundary Hardening

## Metadata
- Date: 2026-04-12
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice plan
- Execution Mode: none
- Execution Mode Policy: none
- Parent Plan: [docs/CHANGE-PLAN-20260412-roadmap-architecture-score-uplift.md](docs/CHANGE-PLAN-20260412-roadmap-architecture-score-uplift.md)
- Child Plan(s): none
- Scope Label: shell-boundary-hardening
- Related Tickets/Issues: none
- Review Status: 未レビュー
- Review Notes:
  - 実現性レビュー: feasible.
  - The slice stays within already documented shell/state modules, so the implementation path is clear.
  - No unresolved cross-repository dependency exists.
  - External review feedback emphasizes `Deref` transparency leaks as the main remaining shell-boundary issue.

## 1. Background
- The top-level shell already delegates many concerns, but it still needs a cleaner fixed point so the remaining coordination surface is easy to defend.
- State ownership is documented, yet the shell-facing inventory still benefits from tighter grouping and clearer ownership statements.
- The most important shell-side gap is not missing modules but overexposed access: `FlistWalkerApp` and its wrappers still behave too much like a flat surface.

## 2. Goal
- Make the shell boundary explicit enough that `app/mod.rs` reads like a thin coordinator rather than a broad feature owner, and remove the `Deref`-style transparency that makes the shell feel like a disguised God Object.
- Align the state bundle inventory with the current architecture story so later routing work can assume a stable shell boundary.

## 3. Scope
### In Scope
- `rust/src/app/mod.rs`
- `rust/src/app/state.rs`
- `rust/src/app/tab_state.rs`
- `rust/src/app/ui_state.rs`
- `rust/src/app/query_state.rs`
- `rust/src/app/shell_support.rs`
- `docs/ARCHITECTURE.md`
- `docs/DESIGN.md`

### Out of Scope
- Search semantics.
- Request routing and response application internals.
- FileList or updater behavior changes.

## 4. Constraints and Assumptions
- Do not reintroduce shell-local blocking work.
- Keep the owner modules intact; this slice is about clarifying the shell boundary, not renaming the world.
- Preserve existing tests and validation entry points.

## 5. Current Risks
- Risk:
  - Impact: The slice could become a cosmetic cleanup instead of a meaningful boundary hardening.
  - Mitigation: Tie every code or doc change to a specific ownership question.
- Risk:
  - Impact: Shell helper extraction may hide behavior if the doc trail is not updated.
  - Mitigation: Sync `ARCHITECTURE.md` and `DESIGN.md` in the same slice.

## 6. Execution Strategy
1. Reconcile the shell-facing state inventory.
   - Files/modules/components: `app/mod.rs`, `app/state.rs`, `app/tab_state.rs`, `app/ui_state.rs`, `app/query_state.rs`.
   - Expected result: The shell's state surface is easier to inspect and reason about.
   - Verification: targeted unit tests and code review of the ownership split.
2. Tighten shell-local helpers and policy seams.
   - Files/modules/components: `app/shell_support.rs`, `app/mod.rs`.
   - Expected result: shell-local policy reads as a narrow helper boundary.
   - Verification: `cargo test` and any existing shell-related regression tests.
3. Reflect the new shell story in docs.
   - Files/modules/components: `docs/ARCHITECTURE.md`, `docs/DESIGN.md`.
   - Expected result: the documented architecture matches the implementation shape.
   - Verification: docs diff review and traceability check.

## 7. Detailed Task Breakdown
- [ ] Reconcile the shell-facing field inventory with the current state bundles.
- [ ] Tighten the shell-local helper policy so `app/mod.rs` keeps only the top-level orchestration surface.
- [ ] Update architecture/design docs to match the refined shell boundary.

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test`
- Manual checks:
  - Read through the updated architecture docs for shell ownership clarity.
- Performance or security checks:
  - Keep UI-thread work out of shell coordination.
- Regression focus:
  - shell boundary confusion
  - state ownership ambiguity

## 9. Rollback Plan
- Revert shell helper extraction and docs together if the boundary story becomes harder to understand.
- Keep the shell-state split small enough that individual changes can be reverted without touching routing logic.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `architecture-score-80`, read the roadmap and this slice plan before starting implementation.
- Follow the roadmap `Execution Mode` and `Execution Mode Policy`.
- Execute the work in the documented order unless the plan is updated first.
- If scope, order, or risk changes, update the change plan before continuing.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-12 Planned.

## 12. Communication Plan
- Return to the roadmap review step when the shell boundary is stable enough to start routing consolidation.

## 13. Completion Checklist
- [ ] Planned document created before implementation
- [ ] Temporary `AGENTS.md` rule added
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into the durable docs as needed
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 14. Final Notes
- Keep the slice focused on shell boundary clarity; do not pull routing or filelist behavior into this workstream.

# EXECUTION PLAN: Quality and Maturity Uplift Roadmap

## Metadata
- Date: 2026-04-19
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: roadmap
- Execution Profile: standard
- Planning Depth: roadmap+slice
- Review Pattern: single-subagent
- Review Requiredness: required-before-implementation
- Execution Mode: standard
- Execution Mode Policy: This roadmap defines the improvement order. Execute only reviewed slice plans. Do not start structural refactoring while unrelated local changes are pending unless the affected files are verified and the plan is updated first.
- Parent Plan: none
- Child Plan(s):
  - docs/EXECUTION-PLAN-20260419-slice-a-quality-baseline-gates.md
- Scope Label: quality-maturity-uplift
- Related Tickets/Issues: external multi-axis evaluation dated 2026-04-18
- Review Status: reviewed
- Review Notes:
  - Initial feasibility review only. The current worktree already has unrelated local changes in CI, docs, AGENTS, and indexer files, so implementation must verify those changes before editing overlapping files.
  - 2026-04-19 main-agent review: roadmap scope is feasible if Slice A stays limited to CI/docs quality gates and does not start render/state refactoring. `single-subagent` review is recorded as not executed because this session only permits subagent spawning when the user explicitly asks for delegation.

## 1. Background
The 2026-04-18 evaluation scored the project at 72/100. The lowest areas were operational maturity, maintainability/extensibility, code quality, and GUI test coverage. Recent architecture work improved ownership boundaries, but the remaining risks are now less about one God Object and more about measurable quality gates, large presentation modules, and supportability gaps.

## 2. Goal
Raise the project toward an 80+ quality posture without breaking the existing SDD/TDD discipline or UI responsiveness policy.

Observable success conditions:
- CI fails on a meaningful coverage regression instead of only uploading `lcov.info`.
- `render.rs` and `ui_model.rs` have clearer ownership boundaries and smaller testable units.
- GUI rendering risk is covered by deterministic automated checks where possible and by explicit manual smoke steps where automation is not yet practical.
- Supportability improves without adding default telemetry or privacy-sensitive reporting.
- Closure slice records whether the project can reasonably claim improvement over the external 72/100 baseline.

The final slice is reserved as a closure slice for scoring, residual-risk review, and the decision to close or continue the roadmap.

## 3. Scope
### In Scope
- Coverage gate and related TESTPLAN/CI updates.
- Render/theme extraction and render-command or panel-level regression tests.
- `FlistWalkerApp` and state boundary tightening where it reduces deep field chains.
- Supportability improvements such as issue templates, log collection guidance, and opt-in diagnostics packaging.
- Documentation updates in `ARCHITECTURE.md`, `DESIGN.md`, `TESTPLAN.md`, and `TASKS.md`.

### Out of Scope
- Default telemetry, usage analytics, or automatic crash upload.
- Installer packaging, DMG creation, notarization completion, or package manager distribution.
- Large feature additions unrelated to the evaluation gaps.
- Rewriting the GUI framework or replacing egui.

## 4. Constraints and Assumptions
- UI freeze prevention remains the top priority.
- Rust implementation changes require at least `cd rust && cargo test`.
- CI/release changes must preserve existing Windows GNU cross-build and release asset rules.
- Existing local changes must not be reverted or overwritten.
- Privacy-sensitive operational features must be opt-in or user-initiated.

## 5. Current Risks
- Risk: Coverage gate may fail the current baseline unexpectedly.
  - Impact: CI becomes noisy and blocks unrelated work.
  - Mitigation: Measure current coverage first and set an initial threshold at or slightly below the verified baseline, with a documented ratchet path.
- Risk: Splitting `render.rs` can turn mechanical movement into behavior changes.
  - Impact: GUI regressions in tab rendering, dialogs, shortcuts, or command dispatch.
  - Mitigation: Add focused render-command and panel helper tests before moving behavior.
- Risk: Boundary tightening can expand into a broad refactor.
  - Impact: Hard-to-review churn and merge conflicts.
  - Mitigation: Use owner API increments and stop each slice at a verifiable reduction target.
- Risk: Supportability work can drift into telemetry.
  - Impact: Privacy and trust concerns.
  - Mitigation: Keep user-initiated log bundle / issue-template work separate from analytics.

## 6. Execution Strategy
1. Slice A: Quality Baseline Gates
   - Files/modules/components: `.github/workflows/ci-cross-platform.yml`, `docs/TESTPLAN.md`, `docs/TASKS.md`, optional coverage notes.
   - Expected result: coverage gate and validation wording are explicit, measurable, and safe for CI.
   - Verification: coverage command dry run or documented local limitation; `cargo test` is not required unless Rust files change.
2. Slice B: Render Boundary and Test Harness
   - Files/modules/components: `rust/src/app/render.rs`, new `rust/src/app/render_*` modules as needed, `rust/src/app/tests/render_tests.rs`.
   - Expected result: theme constants and at least one panel/dialog rendering concern move behind smaller helper surfaces with regression tests.
   - Verification: `cd rust && cargo test`; manual GUI smoke if visual layout behavior changes.
3. Slice C: App Boundary Tightening
   - Files/modules/components: `rust/src/app/state.rs`, `tab_state.rs`, owner modules that still require deep chains.
   - Expected result: reduce direct deep field access in one high-churn flow by introducing owner methods or narrower state views.
   - Verification: owner-aligned app tests plus `cd rust && cargo test`.
4. Slice D: Supportability Without Telemetry
   - Files/modules/components: `.github/ISSUE_TEMPLATE/*`, docs, optional CLI/log helper if scoped.
   - Expected result: users can report failures with useful logs and environment data without automatic upload.
   - Verification: docs review and template lint by inspection; Rust tests only if code changes.
5. Slice E: Closure Review
   - Files/modules/components: `docs/TASKS.md`, `docs/ARCHITECTURE.md`, `docs/TESTPLAN.md`.
   - Expected result: scoring delta, completed work, deferred items, and continue/close decision are recorded.
   - Verification: docs diff review and relevant command history.

## 7. Detailed Task Breakdown
- [x] Review and approve Slice A before implementation.
- [x] Measure current coverage behavior and define the initial gate.
- [x] Implement Slice A and update validation docs.
- [ ] Create/review Slice B based on the measured render-risk inventory.
- [ ] Execute Slice B with tests before larger movement.
- [ ] Create/review Slice C only after render risks are bounded.
- [ ] Execute supportability work without default telemetry.
- [ ] Run closure scoring and record the next decision.

## 8. Validation Plan
- Automated tests: `cd rust && cargo test`; CI coverage command for coverage-gate changes.
- Manual checks: GUI smoke for render/tab/dialog changes.
- Performance or security checks: run VM-003 ignored perf tests only if indexing paths are touched; security review if supportability adds file collection.
- Regression focus: render command queue, tab identity, nonblocking UI, FileList perf gate names already being updated in the current worktree.

## 9. Rollback Plan
- Coverage gate changes can be reverted independently from render refactors.
- Render extraction should be committed in small movement-plus-test units.
- Supportability templates/docs can be reverted independently unless a code-level log bundle helper is added.
- If a slice expands beyond its plan, stop and update the roadmap before continuing.

## 10. Temporary `AGENTS.md` Rule Draft
Add a temporary section to the project `AGENTS.md` with content equivalent to:

```md
## Temporary Change Plan Rule
- For `quality-maturity-uplift`, read the relevant change plan documents before starting implementation:
  - `docs/EXECUTION-PLAN-20260419-roadmap-quality-maturity-uplift.md`
  - `docs/EXECUTION-PLAN-20260419-slice-a-quality-baseline-gates.md`
- Follow the plan's `Execution Profile`, `Planning Depth`, `Review Pattern`, and `Review Requiredness`.
- The roadmap uses `Execution Mode: standard`; complete and review each slice before starting the next one.
- Do not close the roadmap until the closure slice has recorded the goal-validation result.
- Execute the work in the documented order unless the plan is updated first.
- If scope, order, or risk changes, update the change plan before continuing.
- Remove this section from `AGENTS.md` after the planned work is complete.
```

## 11. Progress Log
- 2026-04-19 Planned initial roadmap from external multi-axis evaluation.
- 2026-04-19 Slice A started. Coverage baseline was measured at 70.29% line coverage and the initial CI gate was set to `--fail-under-lines 70`.
- 2026-04-19 Slice A validation passed with `cargo llvm-cov --locked --workspace --lcov --output-path target/llvm-cov/lcov.info --fail-under-lines 70`.

## 12. Communication Plan
- Return to user when:
  - Slice A is reviewed and ready for implementation.
  - implementation is blocked by existing local changes.
  - a slice is complete and verified.

## 13. Completion Checklist
- [x] Planned document created before implementation
- [ ] Temporary `AGENTS.md` rule added
- [x] Slice A reviewed
- [ ] Work executed according to the plan or the plan updated first
- [ ] Verification completed
- [ ] Lasting docs updated
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted after completion

## 14. Final Notes
This roadmap deliberately starts with measurable gates rather than more architecture reshaping. The project already has substantial architecture cleanup history, so the next improvement should make regressions harder to miss before moving large presentation or state boundaries.

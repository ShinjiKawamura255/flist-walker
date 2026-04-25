# EXECUTION PLAN: Slice D Render Boundary Decomposition

## Metadata
- Date: 2026-04-25
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice plan
- Execution Profile: safety-critical
- Planning Depth: roadmap+slice
- Review Pattern: specialist-subagents
- Review Requiredness: required-before-and-after-revision
- Execution Mode: none
- Execution Mode Policy: Inherits the parent roadmap policy. This slice is behavior-preserving UI/render refactoring and must complete plan review, required revisions, convergence review, and Review Notes updates before implementation.
- Parent Plan: `docs/EXECUTION-PLAN-20260425-roadmap-quality-hardening-90.md`
- Child Plan(s): none
- Scope Label: quality-hardening-90 / slice-d-render-boundary
- Related Tickets/Issues: none
- Review Status: レビュー済み
- Review Notes:
  - 2026-04-25 initial staged review started with architecture and testing perspectives.
  - Architecture review found three blocking issues: slice metadata conflicted with the parent `safety-critical` / `specialist-subagents` requirement, "move or remove" was too ambiguous for duplicate render code, and delete/keep boundaries were not fixed.
  - Testing review found four blocking issues: GUI/manual regression gate was too weak for render changes, current headless/snapshot tests did not cover `run_ui_frame`, call-graph checks lacked pass/fail criteria, and touched-file fmt validation was ambiguous.
  - 2026-04-25 revised plan to align with parent `safety-critical` / `specialist-subagents`, forbid moving old duplicate `render.rs` drawing code into active modules, enumerate delete/keep boundaries, require `run_ui_frame` headless/manual smoke coverage, define negative/positive call-graph checks, and define touched-file fmt validation.
  - 2026-04-25 convergence review completed by architecture and testing reviewers.
  - Convergence result: all initial blockers were resolved in the plan; no material blockers remain.
  - Status changed to `レビュー済み`; implementation may start.

## 1. Background
`rust/src/app/render.rs` is still 1,353 lines even though `render_panels.rs`, `render_dialogs.rs`, `render_tabs.rs`, `render_snapshot.rs`, and `render_theme.rs` already exist. The current file contains command definitions and frame orchestration, but it also retains large duplicate panel/dialog/result-list implementations that overlap with the dedicated render modules.

The roadmap calls for render boundary decomposition after updater decomposition. This slice should reduce `render.rs` to the stable command/facade surface and leave panel/dialog/list drawing in the dedicated modules.

## 2. Goal
Make render ownership easier to review without changing visual behavior:

- Keep `RenderCommand` and dispatch ownership in `render.rs`.
- Keep `run_ui_frame`, `queue_render_command`, `dispatch_render_commands`, and lightweight wrapper/facade methods in `render.rs`.
- Do not move stale duplicate drawing code from `render.rs` into active modules. Active implementations in `render_panels.rs` / `render_dialogs.rs` must be preserved; old duplicate methods in `render.rs` may only be deleted or reduced to thin wrappers.
- Preserve `render_panels.rs`, `render_dialogs.rs`, `render_tabs.rs`, `render_snapshot.rs`, and `render_theme.rs` as the rendering concern owners.
- Preserve snapshot/headless render test behavior.
- Do not change input handling, queued command semantics, colors, layout constants, or GUI state transitions.

## 3. Scope
### In Scope
- `rust/src/app/render.rs`
- `rust/src/app/render_panels.rs`
- `rust/src/app/render_dialogs.rs`
- `rust/src/app/render_tabs.rs` only if wrapper visibility requires a small adjustment
- `rust/src/app/tests/render_tests.rs`
- Render-related docs and roadmap/TASKS progress updates

### Out of Scope
- Redesigning visual layout or colors.
- Changing `RenderCommand` variants or dispatch semantics unless tests show dead code.
- Changing FileList/update dialog behavior.
- Introducing new GUI automation frameworks.
- Touching search/indexer logic.

## 4. Constraints and Assumptions
- This is a behavior-preserving render boundary cleanup.
- `render.rs` should remain the command/facade module, not a catch-all drawing module.
- Dedicated render modules should own actual drawing for panels, dialogs, tabs, snapshots, and theme constants.
- Snapshot/headless tests are the primary automated guard for visual routing.
- Repository-wide `cargo fmt -- --check` still has known baseline failures outside this slice; touched/new render files must be formatted.
- Rust changes require `cargo test --locked` and `cargo clippy --all-targets -- -D warnings`.
- Because this slice touches render paths, it must include either a `run_ui_frame` headless test or a recorded manual GUI smoke covering result list, dialogs, preview, tabs, focus, and light/dark theme. Prefer adding the headless test first.

## 4.1 Delete/Keep Boundary
Delete candidates in `rust/src/app/render.rs` if call-graph checks confirm they are duplicate and unused:

- `render_results_and_preview`
- `render_results_list`
- `render_history_search_results`
- `render_result_row`
- `build_result_row_job`
- `render_top_panel`
- `render_status_panel`
- `render_filelist_dialogs`
- `render_update_dialog`

Keep candidates in `rust/src/app/render.rs` unless a later reviewed slice says otherwise:

- `filelist_use_walker_dialog_lines`
- `dialog_button`
- `top_action_labels`
- `top_action_command`
- `schedule_frame_repaint`
- `run_ui_frame`
- `render_tab_bar` thin wrapper
- `render_central_panel` thin wrapper
- `gui_surface_snapshot`
- `queue_render_command`
- `dispatch_render_commands`
- `RenderCommand` and command enums

## 5. Current Risks
- Risk: Removing duplicate methods breaks tests or private call sites.
  - Impact: compile failure or missing wrapper behavior.
  - Mitigation: use `rg` to confirm call sites before deletion; keep thin wrappers where callers still need `FlistWalkerApp` methods.
- Risk: Duplicate implementations are not perfectly equivalent.
  - Impact: subtle GUI behavior regression.
  - Mitigation: preserve the active module implementations already used by `run_ui_frame` and headless render tests. Do not replace them with old duplicate `render.rs` code.
- Risk: Render module privacy changes expose too much surface.
  - Impact: future coupling grows.
  - Mitigation: keep functions `pub(super)` only where existing app tests/modules require them.
- Risk: UI responsiveness regression.
  - Impact: event-loop stutter.
  - Mitigation: do not add synchronous I/O or worker polling to render code; this slice only moves/removes drawing code.

## 6. Execution Strategy
1. Confirm duplicate render call graph
   - Files/modules/components: `rust/src/app/render.rs`, `rust/src/app/render_panels.rs`, `rust/src/app/render_dialogs.rs`, render tests.
   - Expected result: identify which `FlistWalkerApp::render_*` methods in `render.rs` are unused duplicates and which are needed as thin wrappers.
   - Verification: `rg` call-site checks with explicit delete/keep boundary from section 4.1.
2. Remove stale duplicate panel/result-list methods from `render.rs`
   - Files/modules/components: `rust/src/app/render.rs`, possibly `rust/src/app/render_panels.rs`.
   - Expected result: results/list/history row rendering has a single implementation in `render_panels.rs`; `render.rs` no longer defines duplicate drawing logic.
   - Verification: render tests and compile.
3. Remove stale duplicate dialog methods from `render.rs`
   - Files/modules/components: `rust/src/app/render.rs`, `rust/src/app/render_dialogs.rs`.
   - Expected result: FileList/update dialog rendering has a single implementation in `render_dialogs.rs`.
   - Verification: render tests and update/filelist dialog tests.
4. Keep command and dispatcher surface stable
   - Files/modules/components: `rust/src/app/render.rs`, `rust/src/app/tests/render_tests.rs`.
   - Expected result: `RenderCommand` and dispatch tests remain intact; render command queues are still consumed after frame rendering.
   - Verification: `cargo test --locked render_tests`; `cargo test --locked dispatch_render_commands`; add or preserve a `run_ui_frame` headless test covering facade render order.
5. Synchronize docs and progress records
   - Files/modules/components: `docs/ARCHITECTURE.md`, `docs/DESIGN.md`, `docs/DETAILED_DESIGN.md`, `docs/TESTPLAN.md`, `docs/TASKS.md`, roadmap, this slice.
   - Expected result: docs describe `render.rs` as facade/command dispatcher and dedicated modules as drawing owners; fix stale wording that says `render.rs` owns update dialog drawing.
   - Verification: docs diff review.
6. Run validation and commit
   - Files/modules/components: all touched files.
   - Expected result: Slice D is one independent rollback unit.
   - Verification: `cargo test --locked`; `cargo clippy --all-targets -- -D warnings`; `cargo test --locked render_tests`; `cargo test --locked run_ui_frame`; `cargo test --locked render_panels_and_dialogs_execute_in_headless_frame`; `git diff --check`.

## 7. Detailed Task Breakdown
- [x] Review this slice plan with architecture/testing focus.
- [x] Confirm render duplicate call graph with `rg`.
- [x] Delete duplicate panel/result-list methods in `render.rs` without replacing active module implementations.
- [x] Delete duplicate dialog methods in `render.rs` without replacing active module implementations.
- [x] Keep `RenderCommand`/dispatch surface stable.
- [x] Add or preserve `run_ui_frame` headless coverage, or record manual GUI smoke if automated coverage is insufficient.
- [x] Update permanent docs for render boundary ownership.
- [x] Record touched-file formatting and known repository-wide fmt baseline if checked.
- [x] Run required validation.
- [x] Update roadmap/TASKS and mark Slice D complete.
- [x] Commit Slice D as an independent rollback unit.

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test --locked`
  - `cd rust && cargo clippy --all-targets -- -D warnings`
  - `cd rust && cargo test --locked render_tests`
  - `cd rust && cargo test --locked run_ui_frame`
  - `cd rust && cargo test --locked render_panels_and_dialogs_execute_in_headless_frame`
  - `git diff --check`
- Call graph checks:
  - Negative check after deletion: `rg -n "pub\\(super\\) fn render_results_and_preview|pub\\(super\\) fn render_results_list|pub\\(super\\) fn render_history_search_results|fn render_result_row|fn build_result_row_job|pub\\(super\\) fn render_top_panel|pub\\(super\\) fn render_status_panel|pub\\(super\\) fn render_filelist_dialogs|pub\\(super\\) fn render_update_dialog" rust/src/app/render.rs` should return no stale duplicate drawing methods.
  - Positive check after deletion: `rg -n "render_panels::render_top_panel|render_panels::render_status_panel|render_dialogs::render_filelist_dialogs|render_dialogs::render_update_dialog|render_panels::render_central_panel" rust/src/app/render.rs` should show frame/facade delegation.
  - Negative check after deletion: `rg -n "egui::Window::new|TopBottomPanel|CentralPanel|ScrollArea" rust/src/app/render.rs` should return no active panel/dialog drawing in `render.rs` except if a remaining wrapper is explicitly justified.
- Formatting:
  - format touched render files with `rustfmt`
  - exact touched-file check: `cd rust && rustfmt --check src/app/render.rs src/app/render_panels.rs src/app/render_dialogs.rs src/app/render_tabs.rs src/app/tests/render_tests.rs`
  - repository-wide `cargo fmt -- --check` is informative only until existing baseline failures are fixed; record the known baseline files if checked
- Manual checks:
  - If `run_ui_frame` headless coverage cannot be added, perform and record a manual GUI smoke covering result list, dialogs, preview, tabs, focus, and light/dark theme before completion.
  - If a visual/layout behavior change is unavoidable, stop and update this plan first.

## 9. Rollback Plan
- Revert `rust/src/app/render.rs` and any matching render module/test/doc changes together.
- Because this slice is behavior-preserving, rollback restores the previous duplicated render implementation without data migration.
- If duplicate implementations differ materially, stop and either narrow the slice or add explicit before/after tests before deleting code.

## 10. Temporary `AGENTS.md` Rule Draft
Use the parent roadmap rule already present in `AGENTS.md`.

## 11. Progress Log
- 2026-04-25 Planned.
- 2026-04-25 Implemented: removed stale duplicate panel/dialog/result-list drawing methods from `render.rs`, preserved module-owned active drawing implementations, and added `run_ui_frame_executes_render_facade_in_headless_frame`.
- 2026-04-25 Validation passed: `cargo test --locked`, `cargo clippy --all-targets -- -D warnings`, `cargo test --locked render_tests`, `cargo test --locked run_ui_frame`, `cargo test --locked render_panels_and_dialogs_execute_in_headless_frame`, call-graph deletion/delegation checks, negative drawing primitive check, touched-file `rustfmt --check`, and `git diff --check`.
- 2026-04-25 Informative repo-wide `cargo fmt -- --check` still fails only on existing baseline files: `rust/src/app/session.rs`, `rust/src/app/shell_support.rs`, `rust/src/runtime_config.rs`.
- 2026-04-25 Ready to commit as independent Slice D rollback unit.

## 12. Communication Plan
- Return to user if:
  - duplicate render implementations are not behaviorally equivalent
  - a layout/input behavior change becomes necessary
  - render tests are insufficient to cover a deletion
  - validation fails for unrelated baseline reasons that would make the slice unsafe to commit

## 13. Completion Checklist
- [x] Slice reviewed according to required-before-and-after-revision
- [x] `render.rs` command/facade boundary is preserved
- [x] Dedicated render modules own panel/dialog/list drawing
- [x] Required validation passed
- [x] Roadmap/TASKS updated
- [x] Slice committed

## 14. Final Notes
This slice should reduce render review surface before search/indexer boundary work. It should prefer deleting duplicate code over moving code when the dedicated module implementation is already the active path.

# EXECUTION PLAN: Slice B Render Theme Boundary

## Metadata
- Date: 2026-04-19
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice plan
- Execution Profile: standard
- Planning Depth: roadmap+slice
- Review Pattern: single-subagent
- Review Requiredness: required-before-implementation
- Execution Mode: none
- Execution Mode Policy: Follow parent roadmap. Keep this slice to low-risk render theme extraction and deterministic tests; do not move panel/dialog rendering bodies yet.
- Parent Plan: docs/EXECUTION-PLAN-20260419-roadmap-quality-maturity-uplift.md
- Child Plan(s): none
- Scope Label: render-theme-boundary
- Related Tickets/Issues: external multi-axis evaluation dated 2026-04-18
- Review Status: reviewed
- Review Notes:
  - 2026-04-19 main-agent review: feasible. The first render slice should extract repeated theme colors into a small helper module and add tests for those contracts before any larger `render.rs` panel split. `single-subagent` review is not executed because subagent spawning requires explicit user delegation.

## 1. Background
`render.rs` is still large, and repeated hard-coded colors make visual changes harder to audit. The next render improvement should create a small tested boundary before moving panel or dialog rendering into separate files.

## 2. Goal
Introduce a render theme helper module for repeated row/dialog/tab colors and lock it with unit tests. This reduces scattered magic numbers and gives later panel/dialog extraction a stable visual contract.

## 3. Scope
### In Scope
- Add `rust/src/app/render_theme.rs`.
- Move repeated selection background, entry kind label colors, and highlight color helpers into the module.
- Replace direct color literals in `render.rs` and `render_tabs.rs` where they match those theme contracts.
- Add deterministic render tests for the helper outputs.
- Update `TASKS.md` and this plan with validation results.

### Out of Scope
- Moving result panel, preview panel, filelist dialogs, or update dialog bodies.
- Changing visual colors or layout.
- Adding screenshot/snapshot infrastructure.
- Optimizing per-character `LayoutJob` construction.

## 4. Constraints and Assumptions
- This slice must preserve current light/dark colors exactly.
- Rust changes require `cd rust && cargo test`.
- GUI manual smoke is not required because the expected rendered colors are unchanged and covered by helper tests.

## 5. Current Risks
- Risk: A color helper changes an existing RGB value.
  - Impact: subtle visual regression.
  - Mitigation: tests assert exact RGB values for light/dark selection, kind colors, and highlight.
- Risk: The new module becomes another catch-all.
  - Impact: boundary loses value.
  - Mitigation: only theme/color helpers belong here; layout math stays in render modules.

## 6. Execution Strategy
1. Add theme helper
   - Files/modules/components: `rust/src/app/render_theme.rs`, `rust/src/app/mod.rs`.
   - Expected result: named functions expose existing color values.
   - Verification: compile and helper tests.
2. Replace duplicated literals
   - Files/modules/components: `rust/src/app/render.rs`, `rust/src/app/render_tabs.rs`.
   - Expected result: selection/highlight/kind colors use the helper.
   - Verification: `rg "Color32::from_rgb\\(48, 53, 62\\)|Color32::from_rgb\\(228, 232, 238\\)|Color32::from_rgb\\(245, 158, 11\\)" rust/src/app/render.rs rust/src/app/render_tabs.rs` returns no matches.
3. Add tests
   - Files/modules/components: `rust/src/app/tests/render_tests.rs`.
   - Expected result: RGB contracts are pinned.
   - Verification: `cd rust && cargo test`.

## 7. Detailed Task Breakdown
- [x] Add `render_theme.rs` and register it in `app/mod.rs`.
- [x] Replace repeated literals in render modules.
- [x] Add render theme tests.
- [x] Run `cd rust && cargo test`.
- [x] Update task/plan progress.

## 8. Validation Plan
- Automated tests: `cd rust && cargo test`
- Manual checks: not required for unchanged color contracts.
- Performance or security checks: not applicable.
- Regression focus: selected row/dialog/tab background colors, entry kind colors, highlight color.

## 9. Rollback Plan
- Revert `render_theme.rs`, module registration, and direct call-site replacements together.
- Since colors are unchanged, rollback has no data or migration concern.

## 10. Temporary `AGENTS.md` Rule Draft
Handled by parent roadmap.

## 11. Progress Log
- 2026-04-19 Planned and reviewed Slice B.
- 2026-04-19 Added `render_theme.rs`, moved selected fill / kind color / highlight color helpers behind it, and pinned RGB values in `render_tests.rs`.
- 2026-04-19 Validation passed with `cargo test`. Literal check for selected fill and highlight RGB values in `render.rs` / `render_tabs.rs` returned no matches.

## 12. Communication Plan
- Return to user after validation or if color-contract tests reveal an existing mismatch.

## 13. Completion Checklist
- [x] Planned document created before implementation
- [x] Temporary `AGENTS.md` rule already present
- [x] Slice reviewed
- [x] Theme helper added
- [x] Repeated literals replaced
- [x] Tests added
- [x] Verification completed
- [x] Parent roadmap updated

## 14. Final Notes
This is intentionally narrower than a full `render.rs` split. It creates a stable visual boundary and test harness so later panel/dialog extraction can be reviewed mechanically.

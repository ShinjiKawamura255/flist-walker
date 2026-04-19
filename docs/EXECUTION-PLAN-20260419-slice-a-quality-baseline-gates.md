# EXECUTION PLAN: Slice A Quality Baseline Gates

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
- Execution Mode Policy: Follow parent roadmap. Do not continue into implementation until this slice is reviewed and current local changes are accounted for.
- Parent Plan: docs/EXECUTION-PLAN-20260419-roadmap-quality-maturity-uplift.md
- Child Plan(s): none
- Scope Label: quality-baseline-gates
- Related Tickets/Issues: external multi-axis evaluation dated 2026-04-18
- Review Status: reviewed
- Review Notes:
  - Initial feasibility review only. `.github/workflows/ci-cross-platform.yml`, `AGENTS.md`, `docs/TESTPLAN.md`, and `rust/src/indexer/mod.rs` already have local modifications, so implementation must preserve those changes.
  - 2026-04-19 main-agent review: feasible. The committed FileList perf gate rename removed the earlier overlapping indexer/CI changes, leaving only plan documents and task tracking uncommitted. Scope remains CI/docs quality gate work. `single-subagent` review is not executed because subagent spawning requires explicit user delegation.

## 1. Background
The evaluation identified that coverage is collected but not gated, GUI/render validation is thin, and operational maturity lacks user-facing support channels. Before starting larger refactors, the project needs a measurable baseline and validation rules that can catch regression.

## 2. Goal
Create a low-risk baseline improvement that makes quality drift visible:
- Add or prepare a coverage threshold gate with a documented initial threshold.
- Extend `TESTPLAN.md` with a dedicated GUI/render automated-check strategy.
- Record the supportability path that avoids default telemetry.
- Leave the repository ready for the next render-boundary slice.

## 3. Scope
### In Scope
- CI coverage command and threshold behavior.
- `TESTPLAN.md` validation matrix wording for coverage and GUI/render risk.
- `TASKS.md` active roadmap status update.
- Optional short docs note if coverage baseline measurement needs a local record.

### Out of Scope
- Moving `render.rs` code.
- Adding screenshot infrastructure.
- Adding telemetry, crash upload, or installer work.
- Changing search/index behavior.

## 4. Constraints and Assumptions
- If only docs and workflow files change, Rust tests are optional unless the touched workflow command requires local verification.
- If the coverage command is changed, prefer a command that still writes `target/llvm-cov/lcov.info`.
- If local `cargo llvm-cov` is unavailable, document the limitation and leave CI command review as the verification.
- The current FileList perf gate rename in the worktree must be preserved.

## 5. Current Risks
- Risk: The initial threshold may be set above actual baseline.
  - Impact: CI fails immediately.
  - Mitigation: First run or inspect `cargo llvm-cov` locally; if not possible, set a conservative threshold and document the ratchet.
- Risk: The coverage gate command may stop producing `lcov.info`.
  - Impact: artifact upload loses value.
  - Mitigation: keep `--lcov --output-path target/llvm-cov/lcov.info` in the command.
- Risk: GUI test strategy becomes aspirational documentation only.
  - Impact: no practical improvement.
  - Mitigation: define the next slice entry criteria as at least one new deterministic render helper test.

## 6. Execution Strategy
1. Baseline measurement
   - Files/modules/components: local command output, optional docs note.
   - Expected result: current coverage behavior and threshold feasibility are known.
   - Verification: `cd rust && cargo llvm-cov --locked --workspace --lcov --output-path target/llvm-cov/lcov.info` or a documented reason it could not be run.
2. CI coverage gate
   - Files/modules/components: `.github/workflows/ci-cross-platform.yml`.
   - Expected result: coverage generation fails below the initial threshold while still uploading `lcov.info` when generated.
   - Verification: workflow diff review and local command when available.
3. Test plan update
   - Files/modules/components: `docs/TESTPLAN.md`.
   - Expected result: coverage gate, render test strategy, and next render slice criteria are traceable.
   - Verification: `rg` checks for coverage/render entries.
4. Task tracking update
   - Files/modules/components: `docs/TASKS.md`.
   - Expected result: active roadmap and Slice A status are visible.
   - Verification: docs diff review.

## 7. Detailed Task Breakdown
- [x] Check whether `cargo llvm-cov` is installed and can run locally.
- [x] Choose the initial coverage threshold from measured baseline or a conservative documented fallback.
- [x] Update CI coverage command without dropping the lcov artifact.
- [x] Update `TESTPLAN.md` with the new validation rule.
- [x] Update `TASKS.md` with active roadmap and Slice A progress.
- [x] Run applicable validation.

## 8. Validation Plan
- Automated tests:
  - Preferred: `cd rust && cargo llvm-cov --locked --workspace --lcov --output-path target/llvm-cov/lcov.info --fail-under-lines <threshold>`
  - If Rust code changes: `cd rust && cargo test`
- Manual checks:
  - Review workflow diff for artifact path stability.
- Performance or security checks:
  - Not required unless indexing or supportability code is touched.
- Regression focus:
  - Existing FileList perf gate command names in the current worktree remain intact.

## 9. Rollback Plan
- Revert the CI command change if the coverage gate is too noisy.
- Keep TESTPLAN/TASKS docs changes independent so they can be adjusted without touching code.

## 10. Temporary `AGENTS.md` Rule Draft
Handled by parent roadmap.

## 11. Progress Log
- 2026-04-19 Planned Slice A from external quality evaluation.
- 2026-04-19 Reviewed Slice A in-session. Local `cargo-llvm-cov 0.8.5` is available. Baseline run produced line coverage 70.29% (LH=9870 / LF=14042), so the initial CI gate is `--fail-under-lines 70`.
- 2026-04-19 Validation passed with `cargo llvm-cov --locked --workspace --lcov --output-path target/llvm-cov/lcov.info --fail-under-lines 70`.

## 12. Communication Plan
- Return to user if coverage baseline cannot be measured locally.
- Return to user after Slice A is verified or blocked by current local changes.

## 13. Completion Checklist
- [x] Planned document created before implementation
- [x] Temporary `AGENTS.md` rule added
- [x] Slice reviewed
- [x] Coverage baseline selected
- [x] CI/docs updates completed
- [x] Verification completed
- [x] Parent roadmap updated

## 14. Final Notes
This slice is intentionally narrow. It should make the next render and boundary slices safer by adding a visible quality gate and by fixing the validation language before large code movement begins.

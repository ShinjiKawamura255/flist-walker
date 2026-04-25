# EXECUTION PLAN: Slice F GUI Validation Uplift

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
- Execution Mode Policy: Inherits the parent roadmap policy. This slice strengthens GUI validation gates and must complete plan review, required revisions, convergence review, and Review Notes updates before implementation.
- Parent Plan: `docs/EXECUTION-PLAN-20260425-roadmap-quality-hardening-90.md`
- Child Plan(s): none
- Scope Label: quality-hardening-90 / slice-f-gui-validation-uplift
- Related Tickets/Issues: none
- Review Status: レビュー済み
- Review Notes:
  - 2026-04-25 initial plan created after Slice E commit `26a6fc0`.
  - 2026-04-26 specialist subagent review was attempted, but available subagents returned quota-limit errors and no review text. To avoid blocking a docs/script-only validation uplift indefinitely, main-agent fallback review was performed with testing/validation and release/operability checklists.
  - Fallback review finding 1: evidence path must be ignored/generated, not committed. Adopted `rust/target/gui-smoke/`, already covered by `.gitignore`.
  - Fallback review finding 2: the plan must explicitly connect GUI manual smoke to `TC-010`, `TC-011`, `TC-099`, `VM-002`, and `VM-006`. Adopted by updating `docs/TESTPLAN.md`.
  - Fallback review finding 3: report/checklist IDs must be stable and evidence-oriented. Adopted `GSM-001` through `GSM-010` in `docs/GUI-TESTPLAN.md` and `docs/GUI-TESTREPORT.md`.
  - Fallback convergence: scope remains docs/script-only; no product behavior, CI launch, release, or self-update behavior changes are introduced.
  - Status changed to `レビュー済み` under fallback review constraint; residual risk is absence of independent specialist review due quota exhaustion.

## 1. Background
The roadmap identifies GUI validation as a remaining quality gap. Current coverage is strong for app state, render snapshots, keyboard shortcuts, dialogs, tabs, and headless `run_ui_frame`, but the operational GUI regression gate is still mostly prose in `docs/TESTPLAN.md`.

For a native GUI app, fully automated E2E GUI is not yet reliable across Linux/macOS/Windows runners. This slice therefore raises the baseline by making the manual GUI gate repeatable, fixture-backed, evidence-oriented, and explicitly owned, while keeping CI changes out unless the plan review finds a low-risk automation seam.

## 2. Goal
Make GUI validation stronger and repeatable without introducing flaky CI:

- Add a dedicated GUI test plan and report template for FlistWalker's release-critical GUI flows.
- Add a deterministic GUI smoke fixture command that prepares data and evidence paths for manual runs.
- Update `docs/TESTPLAN.md` so VM-002 / VM-006 and relevant TC rows point to the new repeatable GUI gate.
- Define owner, frequency, evidence location, flake tolerance, and pass/fail criteria.
- Preserve the current automated headless/render/unit coverage and do not weaken existing CI.

## 3. Scope
### In Scope
- New `docs/GUI-TESTPLAN.md`.
- New `docs/GUI-TESTREPORT.md` or report template/checklist suitable for repeated use.
- New script under `scripts/` to prepare a deterministic GUI smoke fixture and evidence directory.
- Updates to `docs/TESTPLAN.md`, `docs/TASKS.md`, and the parent roadmap.
- Optional README/support doc pointer only if the review finds it necessary.

### Out of Scope
- Adding a heavyweight GUI automation framework.
- Making GUI launch mandatory in CI.
- Changing GUI behavior, rendering, input handling, update behavior, or search/index semantics.
- Changing release tag/publish flow.
- Adding screenshots or binary artifacts to the repository.

## 4. Constraints and Assumptions
- This slice should be docs/script only unless review identifies a minimal safe automated check.
- GUI smoke must not rely on network access.
- The fixture must avoid user-specific paths and secrets.
- Evidence files should live under `rust/target/gui-smoke/`, not in committed history.
- The script must be usable from WSL/Linux and should document Windows/macOS manual equivalents rather than pretending to automate every OS.
- Rust implementation is not expected to change; if it does, this plan must be updated before implementation.

## 5. Current Risks
- Risk: A manual gate remains too vague to improve quality.
  - Impact: Slice F fails the roadmap success condition.
  - Mitigation: require deterministic fixture creation, explicit checklist IDs, pass/fail criteria, evidence paths, and owner/frequency.
- Risk: CI GUI automation is flaky.
  - Impact: noisy failures or ignored checks.
  - Mitigation: do not add GUI launch to CI in this slice unless it is deterministic and review-approved.
- Risk: New script drifts from real GUI flows.
  - Impact: fixture proves little.
  - Mitigation: tie fixture files to the documented flows: search, preview, sorting, FileList, ignore list, tabs, dialogs, and action routing.
- Risk: Evidence templates accumulate stale local data.
  - Impact: misleading reports.
  - Mitigation: keep committed report as a template/current checklist and write run artifacts under `target/gui-smoke/`.

## 6. Execution Strategy
1. Confirm current GUI validation surface
   - Files/modules/components: `docs/TESTPLAN.md`, `docs/SPEC.md`, render/headless tests, scripts.
   - Expected result: identify manual and automated GUI coverage already present.
   - Verification: `rg` review and doc diff.
2. Add fixture-backed GUI smoke command
   - Files/modules/components: new `scripts/gui-smoke-fixture.sh`.
   - Expected result: command creates deterministic root data, FileList, ignore list, and evidence template under `target/gui-smoke/`.
   - Verification: run the script and inspect generated paths.
3. Add GUI test plan/report docs
   - Files/modules/components: `docs/GUI-TESTPLAN.md`, `docs/GUI-TESTREPORT.md`.
   - Expected result: release-critical GUI flows have IDs, steps, expected results, owner, frequency, evidence path, and flake policy.
   - Verification: doc diff review and ID/reference checks.
4. Synchronize permanent test plan and progress records
   - Files/modules/components: `docs/TESTPLAN.md`, `docs/TASKS.md`, roadmap, this slice.
   - Expected result: VM-002/VM-006 and TC-010/TC-011/TC-099 point to the stronger gate.
   - Verification: `rg` checks for GUI plan/report references.
5. Run validation and commit
   - Files/modules/components: all touched files.
   - Expected result: Slice F is one independent rollback unit.
   - Verification: script run, shell syntax check, docs reference checks, `git diff --check`.

## 7. Detailed Task Breakdown
- [x] Review this slice plan with testing/validation and release/operability focus.
- [x] Confirm existing GUI automated and manual validation coverage.
- [x] Add deterministic GUI smoke fixture command.
- [x] Add GUI test plan with prioritized critical flows and environment matrix.
- [x] Add GUI test report template/current checklist with evidence expectations.
- [x] Update `docs/TESTPLAN.md` validation matrix and relevant TC rows.
- [x] Update roadmap/TASKS and mark Slice F complete.
- [x] Commit Slice F as an independent rollback unit.

## 8. Validation Plan
- Automated/document checks:
  - `bash -n scripts/gui-smoke-fixture.sh`
  - `scripts/gui-smoke-fixture.sh`
  - `rg -n "GUI-TESTPLAN|GUI-TESTREPORT|gui-smoke-fixture|rust/target/gui-smoke|GSM-" docs/TESTPLAN.md docs/GUI-TESTPLAN.md docs/GUI-TESTREPORT.md`
  - `git diff --check`
- Optional Rust validation:
  - No Rust implementation change is expected. If Rust files are touched, run `cd rust && cargo test --locked`.
- Manual gate definition:
  - The new GUI plan must define at least startup/indexing, search/highlight, preview, selection/action, sorting, FileList creation/use, tabs, dialogs/cancel, light/dark theme, and responsiveness checks.
  - The report template must include OS, build/version, fixture path, evidence path, result status, and failure triage fields.
- CI decision:
  - Default decision for this slice: no GUI launch in CI.
  - Rationale: current risk is missing repeatable manual evidence, not lack of headless unit coverage; GUI E2E would likely be flaky without a dedicated harness.

## 9. Rollback Plan
- Revert the new GUI docs, fixture script, and TESTPLAN/TASKS/roadmap updates together.
- Since no application behavior is intended to change, rollback has no data migration or runtime compatibility impact.

## 10. Temporary `AGENTS.md` Rule Draft
Use the parent roadmap rule already present in `AGENTS.md`; update its active slice reference to this plan while Slice F is active.

## 11. Progress Log
- 2026-04-25 Planned.
- 2026-04-26 Implemented deterministic GUI smoke fixture command `scripts/gui-smoke-fixture.sh`.
- 2026-04-26 Added `docs/GUI-TESTPLAN.md` with stable `GSM-001` through `GSM-010` flow IDs, owner/frequency/evidence/pass-fail/flake policy, and environment matrix.
- 2026-04-26 Added `docs/GUI-TESTREPORT.md` as the committed report template and generated local report support under `rust/target/gui-smoke/evidence/GUI-TESTREPORT.local.md`.
- 2026-04-26 Updated `docs/TESTPLAN.md` so `TC-010`, `TC-011`, `TC-099`, `VM-002`, and `VM-006` reference the fixture-backed GUI gate.
- 2026-04-26 Validation passed:
  - `bash -n scripts/gui-smoke-fixture.sh`
  - `scripts/gui-smoke-fixture.sh`
  - `rg -n "GUI-TESTPLAN|GUI-TESTREPORT|gui-smoke-fixture|rust/target/gui-smoke|GSM-" docs/TESTPLAN.md docs/GUI-TESTPLAN.md docs/GUI-TESTREPORT.md`
  - `git diff --check`
- 2026-04-26 Fixture generation confirmed ignored output under `rust/target/`, including `rust/target/gui-smoke/root`, `rust/target/gui-smoke/evidence`, and `rust/target/debug/flistwalker.ignore.txt`.

## 12. Communication Plan
- Return to user if:
  - review requires CI GUI automation rather than a manual gate
  - fixture command cannot be made deterministic without Rust implementation changes
  - existing docs reveal a stronger GUI gate already exists and this slice should be reduced

## 13. Completion Checklist
- [ ] Slice reviewed according to required-before-and-after-revision
- [x] GUI fixture command exists and is validated
- [x] GUI test plan/report define owner, frequency, evidence, pass/fail, and flake policy
- [x] `docs/TESTPLAN.md` points to the repeatable GUI gate
- [x] Required validation passed
- [x] Roadmap/TASKS updated
- [x] Slice committed

## 14. Final Notes
This slice intentionally improves operational validation rather than product behavior. It should not introduce runtime changes or broad CI churn.

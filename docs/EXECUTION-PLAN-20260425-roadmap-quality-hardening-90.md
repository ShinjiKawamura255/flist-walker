# EXECUTION PLAN: Quality Hardening 90 Roadmap

## Metadata
- Date: 2026-04-25
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: roadmap
- Execution Profile: safety-critical
- Planning Depth: roadmap+slice
- Review Pattern: specialist-subagents
- Review Requiredness: required-before-and-after-revision
- Execution Mode: standard
- Execution Mode Policy: Use this roadmap as the upper plan for a later implementation pass. Do not execute phases until the roadmap and active slice have completed initial review, required revisions, convergence review, Review Notes updates, and a Temporary Change Plan Rule has been added to `AGENTS.md`. After each slice, update this roadmap with actual findings before creating or activating the next slice. Security-sensitive changes must receive security-focused review before implementation and after revision.
- Parent Plan: none
- Child Plan(s):
  - `docs/EXECUTION-PLAN-20260425-slice-a-traceability-and-plan-foundation.md` (completed 2026-04-25)
  - `docs/EXECUTION-PLAN-20260425-slice-b-self-update-staging-hardening.md` (completed 2026-04-25)
  - `docs/EXECUTION-PLAN-20260425-slice-c-updater-boundary-decomposition.md` (completed 2026-04-25)
  - `docs/EXECUTION-PLAN-20260425-slice-d-render-boundary-decomposition.md` (completed 2026-04-25)
  - `docs/EXECUTION-PLAN-20260425-slice-e-search-indexer-boundary-decomposition.md` (completed 2026-04-25)
  - `docs/EXECUTION-PLAN-20260425-slice-f-gui-validation-uplift.md` (completed 2026-04-26)
  - `docs/EXECUTION-PLAN-20260425-slice-g-dependency-audit-follow-up.md` (completed 2026-04-26)
  - Slice H plan to be created last: closure validation
- Scope Label: quality-hardening-90
- Related Tickets/Issues: none
- Review Status: レビュー済み
- Review Notes:
  - This roadmap is created from the 2026-04-25 candid 84/100 assessment.
  - 2026-04-25 initial specialist review completed with architecture/testing/security perspectives.
  - Architecture review: no critical findings; requested early audit decision point before render/GUI slices and dynamic duplicate-ID detection in Slice A.
  - Testing review: no critical findings; requested empty-output acceptance for duplicate check, meaning-by-meaning reference update policy, and docs-only `git diff --stat` confirmation.
  - Security review: critical finding that Slice B leaves a known self-update staging risk unmitigated until later; added release/tag/self-update stop condition and explicit Slice B security gates.
  - 2026-04-25 convergence review completed by two specialist reviewers.
  - Convergence result: all six required revisions were reflected; no blocking issues remain.
  - Status changed to `レビュー済み`; implementation may start after the Temporary Change Plan Rule is added to `AGENTS.md`.

## 1. Background
The 2026-04-25 project assessment scored FlistWalker at 84/100. The project has strong SDD documents, broad automated tests, cross-platform CI, clippy, coverage gate, and release/security awareness. The remaining issues are concentrated in long-term maintainability and security hardening:

- `docs/TESTPLAN.md` reuses several `TC-*` IDs, weakening traceability.
- Self-update staging uses a predictable time-derived temp directory and ordinary file creation.
- Large modules still carry high review and regression cost.
- GUI validation is still mostly unit/headless rather than end-to-end operational smoke.
- `cargo audit` reports an allowed unmaintained warning through the GUI stack.

## 2. Goal
Raise the project from the current 84/100 class to a defensible 90/100 class without weakening UI responsiveness, cross-platform release support, or existing search/index behavior.

Observable success conditions:

- All `TC-*` IDs in `docs/TESTPLAN.md` are unique and traceability remains coherent.
- Self-update staging uses non-predictable, exclusive temp resources and avoids symlink/clobber hazards.
- `updater.rs`, `render.rs`, `search/mod.rs`, and `indexer/mod.rs` each have a documented decomposition target and at least the highest-risk modules are split without behavior changes.
- GUI validation has a repeatable smoke path with clear CI/manual ownership.
- `cargo audit` warning posture is documented with an upgrade/remediation path.
- `cargo test --locked`, `cargo clippy --all-targets -- -D warnings`, coverage gate, and relevant targeted tests pass after each implementation slice.
- Final closure slice re-scores the project and records close/continue with evidence.

## 3. Scope
### In Scope
- Traceability cleanup in `docs/TESTPLAN.md` and related trace excerpts.
- Self-update temp/staging hardening in `rust/src/updater.rs`.
- Responsibility-preserving decomposition of updater/render/search/indexer modules.
- GUI validation strategy improvement with minimal automation or a stricter repeatable manual gate.
- Dependency/audit warning posture and upgrade plan.
- Docs synchronization for architecture, design, test plan, and release/security notes where behavior changes.

### Out of Scope
- New product features unrelated to the quality-hardening findings.
- Installer creation.
- macOS notarization completion.
- Network drive optimization.
- Rewrite of the GUI framework.
- Removing `prototype/python/` unless a later reviewed slice explicitly makes repository pruning in scope.

## 4. Constraints and Assumptions
- UI responsiveness is the top project constraint; no slice may move heavy I/O or metadata probing back to the UI thread.
- FileList and walker performance contracts must remain compatible with VM-003.
- Self-update changes must preserve Windows/Linux update behavior and macOS manual-only behavior.
- Public docs must not expose internal update override variables forbidden by `AGENTS.md`.
- Rust implementation changes require at least `cargo test`; targeted VM rules apply based on touched files.
- This roadmap is not yet executable until review is complete and `AGENTS.md` contains the temporary rule.
- Stop condition: until Slice B is completed and validated, do not create release tags, publish releases, or make unrelated self-update behavior changes. Any required update-path change found before Slice B must be folded into Slice B or explicitly added to the roadmap before continuing.

## 5. Current Risks
- Risk: Traceability cleanup accidentally changes test semantics rather than IDs.
  - Impact: docs become cleaner but less faithful to existing behavior.
  - Mitigation: Slice A is docs-first and uses `rg` checks for duplicate IDs and trace references.
- Risk: Self-update hardening changes platform behavior.
  - Impact: update install regressions on Windows/Linux.
  - Mitigation: isolate staging changes first, add unit tests around temp path/file creation behavior, then run update command tests.
- Risk: Module decomposition causes broad merge conflicts and hidden regressions.
  - Impact: high cost with little user-visible value.
  - Mitigation: split by seams with existing tests, keep each slice behavior-preserving, and stop if line movement outpaces test coverage.
- Risk: GUI validation becomes too expensive or flaky.
  - Impact: CI slowdown or ignored checks.
  - Mitigation: start with deterministic smoke boundaries and manual gate ownership before considering heavier automation.
- Risk: Dependency warning cannot be eliminated without framework upgrade.
  - Impact: warning remains.
  - Mitigation: document accepted risk, upstream path, and review cadence if no safe upgrade exists.

## 6. Execution Strategy
1. Slice A: Traceability and plan foundation
   - Files/modules/components: `docs/TESTPLAN.md`, trace excerpts in docs if needed, this roadmap.
   - Expected result: duplicate TC IDs are resolved and roadmap execution preconditions are clear.
   - Verification: `rg` duplicate check for `TC-*`; docs diff review; no Rust test required unless implementation files change.
2. Slice B: Self-update staging hardening
   - Files/modules/components: `rust/src/updater.rs`, `rust/src/update_security.rs` if needed, `docs/DESIGN.md`, `docs/TESTPLAN.md`.
   - Expected result: staging directory and staged files are created with exclusive/unpredictable semantics and covered by tests.
   - Verification: `cargo test --locked`; targeted updater/update command tests; `cargo clippy --all-targets -- -D warnings`; negative tests or equivalent proof for exclusive create, predictable path avoidance, symlink/clobber avoidance, permissions, cross-platform behavior, and forbidden update override variable exposure.
   - Required before Slice D: decide whether the current `cargo audit` warning is accepted temporarily or requires GUI framework upgrade work before render/GUI validation slices.
3. Slice C: Updater boundary decomposition
   - Files/modules/components: `rust/src/updater.rs`, possible new `rust/src/updater/*` or helper modules, architecture/design docs.
   - Expected result: update candidate resolution, download/verification, and platform apply helpers are separated without behavior change.
   - Verification: `cargo test --locked`; update-related unit tests; release docs review if public behavior changes.
4. Slice D: Render boundary decomposition
   - Files/modules/components: `rust/src/app/render.rs`, `render_panels.rs`, `render_dialogs.rs`, `render_tabs.rs`, render tests.
   - Expected result: `render.rs` loses large panel/dialog responsibilities and render command seams remain testable.
   - Verification: `cargo test --locked`; render snapshot/headless tests; GUI smoke if visual behavior changes.
5. Slice E: Search/indexer boundary decomposition
   - Files/modules/components: `rust/src/search/mod.rs`, `rust/src/indexer/mod.rs`, search/indexer tests, performance docs.
   - Expected result: query compile/evaluation/ranking and FileList override/walker orchestration seams are clearer.
   - Verification: `cargo test --locked`; VM-003 perf tests if indexing paths are touched; `cargo test perf_search_100k_candidates_reports_latency --lib -- --ignored --nocapture` if search hot path changes.
6. Slice F: GUI validation uplift
   - Files/modules/components: `docs/TESTPLAN.md`, `.github/workflows/*` if CI is changed, possible smoke script/test harness.
   - Expected result: repeatable GUI smoke validation is stronger than the current manual-only prose and has a defined owner/gate.
   - Verification: new smoke command or documented manual gate; CI workflow validation if changed; Slice F must decide CI vs manual gate, flake tolerance, owner, and evidence/log location before implementation.
7. Slice G: Dependency/audit and supply-chain follow-up
   - Files/modules/components: `rust/Cargo.toml`, `rust/Cargo.lock`, `docs/OSS_COMPLIANCE.md`, `docs/RELEASE.md`, `THIRD_PARTY_NOTICES.txt` if dependencies change.
   - Expected result: `cargo audit` warning is either eliminated or documented as accepted transitive risk with review cadence.
   - Verification: `cargo audit`; `cargo test --locked`; OSS compliance checklist if dependency changes.
8. Slice H: Closure validation
   - Files/modules/components: `docs/TASKS.md`, final roadmap update, maybe follow-up roadmap if score remains below target.
   - Expected result: close/continue decision with evidence and updated score.
   - Verification: full validation summary including tests, clippy, audit, coverage, relevant perf/manual checks.
   - Stop/continue rule: if the closure score remains below 90/100 or any security stop condition remains open, return to the user with a proposed follow-up slice instead of closing the roadmap.

## 7. Detailed Task Breakdown
- [x] Create and review Slice A.
- [x] Resolve duplicate `TC-*` IDs and update trace references.
- [x] Create, review, and implement Slice B security hardening.
- [x] Create, review, and implement Slice C updater decomposition.
- [x] Create, review, and implement Slice D render decomposition.
- [x] Create, review, and implement Slice E search/indexer decomposition.
- [x] Create, review, and implement Slice F GUI validation uplift.
- [x] Create, review, and implement Slice G dependency/audit follow-up.
- [ ] Run Slice H closure scoring and decide close/continue.

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test --locked`
  - `cd rust && cargo clippy --all-targets -- -D warnings`
  - `cd rust && cargo llvm-cov --locked --workspace --lcov --output-path target/llvm-cov/lcov.info --fail-under-lines 70`
  - `cd rust && cargo audit`
- Targeted tests:
  - update/updater tests after Slices B/C
  - render tests after Slice D
  - search/indexer tests after Slice E
  - VM-003 ignored perf tests when indexing hot paths change
- Manual checks:
  - GUI smoke for render/input/tab/FileList flows when UI behavior or validation docs change
  - Windows/Linux self-update manual procedure only when update apply behavior changes
- Regression focus:
  - UI remains responsive during index/search/update checks
  - FileList/walker performance gates remain valid
  - release docs do not expose forbidden update override variables

## 9. Rollback Plan
- Each slice should be an independent rollback unit.
- Slice A can be reverted as docs-only if ID remapping causes confusion.
- Slices B/C must keep update behavior isolated; revert code and matching tests/docs together.
- Slice B must define update-specific rollback before implementation for pre-helper staging failures: partially created staging files, existing binary preservation before helper spawn, and user-visible failure messages must fail safe. Post-helper Windows/Linux apply failure rollback is deferred to Slice C unless Slice B changes helper apply behavior.
- Slices D/E should avoid mixed behavior changes and movement-only changes in the same commit when possible.
- Slice F CI changes must be revertible without changing application behavior.
- Slice G dependency changes must include Cargo lockfile, notices, and docs in the same rollback unit.

## 10. Temporary `AGENTS.md` Rule Draft
Add this only after roadmap and active slice initial review, required revisions, convergence review, and Review Notes updates are complete:

```md
## Temporary Change Plan Rule
- For `quality-hardening-90`, read the relevant change plan document(s) before starting implementation.
- Read order:
  - `docs/EXECUTION-PLAN-20260425-roadmap-quality-hardening-90.md`
  - The currently active slice plan, starting with `docs/EXECUTION-PLAN-20260425-slice-a-traceability-and-plan-foundation.md`.
- Follow the plan's `Execution Profile: safety-critical`, `Planning Depth: roadmap+slice`, `Review Pattern: specialist-subagents`, and `Review Requiredness: required-before-and-after-revision`.
- This roadmap uses `Execution Mode: standard`; after each slice, update the roadmap and confirm the next active slice before continuing.
- Until Slice B is complete and validated, do not create release tags, publish releases, or make unrelated self-update behavior changes.
- Do not close the roadmap until the closure slice has recorded goal validation and close/continue decision.
- Execute work in documented order unless the roadmap is updated first.
- If scope, order, security posture, release behavior, or validation risk changes, update the plan before continuing.
- Remove this section from `AGENTS.md` after the roadmap is complete.
```

## 11. Progress Log
- 2026-04-25 Planned from project assessment result 84/100.
- 2026-04-25 Slice A completed: duplicate `TC-*` table-row IDs were removed, trace references were updated by meaning, and docs-only validation passed.
- 2026-04-25 Slice B plan created for self-update staging hardening.
- 2026-04-25 Slice B completed: self-update staging now uses 128-bit random exclusive temp directories with bounded retries, Unix private permissions, no-overwrite staged asset/helper creation, deterministic collision tests, and existing-file refusal tests. Validation passed with `cargo test --locked`, `cargo clippy --all-targets -- -D warnings`, and `cargo audit`. The existing allowed `paste` unmaintained warning remains accepted temporarily and is deferred to Slice G dependency/audit follow-up.
- 2026-04-25 Slice C plan created for updater boundary decomposition.
- 2026-04-25 Slice C completed: updater responsibilities were split behind the `crate::updater` facade into private release/staging/manifest/apply modules, with `VerifiedUpdateBundle` guarding apply spawn, no-overwrite helper script creation preserved, and updater boundary docs updated. Validation passed with `cargo test --locked`, `cargo clippy --all-targets -- -D warnings`, `cargo check --locked --target x86_64-pc-windows-gnu`, targeted updater/update command tests, and `git diff --check`.
- 2026-04-25 Slice D plan created for render boundary decomposition.
- 2026-04-25 Slice D completed: `render.rs` was reduced to the render command/facade/frame orchestration surface, stale duplicate panel/dialog/result-list implementations were deleted, active drawing ownership remained in `render_panels.rs` / `render_dialogs.rs` / `render_tabs.rs` / `render_snapshot.rs` / `render_theme.rs`, and `run_ui_frame` headless coverage was added. Validation passed with `cargo test --locked`, `cargo clippy --all-targets -- -D warnings`, render targeted tests, call-graph negative/positive checks, touched-file `rustfmt --check`, and `git diff --check`. Repository-wide `cargo fmt -- --check` remains blocked by the existing baseline in `app/session.rs`, `app/shell_support.rs`, and `runtime_config.rs`.
- 2026-04-25 Slice E plan created for search/indexer boundary decomposition.
- 2026-04-25 Slice E completed: search private matching/evaluation ownership moved to `search/match_eval.rs`, nested FileList hierarchy override ownership moved to `indexer/filelist_hierarchy.rs`, and the public `search` / `indexer` facades remained stable. Validation passed with full test/clippy/coverage gates, search/indexer targeted tests, boundary checks, touched-file `rustfmt --check`, `git diff --check`, search 100k perf (`29ms`, baseline `44ms`), and VM-003 perf guards. `cargo audit` passed with the existing allowed transitive `paste` unmaintained warning still deferred to Slice G.
- 2026-04-25 Slice F plan created for GUI validation uplift.
- 2026-04-26 Slice F completed: added a deterministic GUI smoke fixture script, dedicated `docs/GUI-TESTPLAN.md` and `docs/GUI-TESTREPORT.md`, stable `GSM-001` through `GSM-010` manual gate IDs, evidence location under ignored `rust/target/gui-smoke/`, and TESTPLAN links from `TC-010`, `TC-011`, `TC-099`, `VM-002`, and `VM-006`. Validation passed with shell syntax check, fixture generation, reference checks, and `git diff --check`. Independent specialist review could not be completed due subagent quota exhaustion; fallback main-agent review is recorded in the slice plan.
- 2026-04-26 Slice G plan created for dependency/audit and supply-chain follow-up.
- 2026-04-26 Slice G completed: `cargo audit` still reports the known allowed transitive `RUSTSEC-2024-0436` / `paste 1.0.15` unmaintained warning through the GUI stack, while `cargo tree -i paste` and `cargo tree --target all -i paste` print no active dependency path. The project now records this as accepted transitive risk in `docs/OSS_COMPLIANCE.md` with owner, release-candidate review cadence, re-evaluation triggers, and required evidence. No dependency, lockfile, audit-policy, or release behavior changes were made.

## 12. Communication Plan
- Return to user when:
  - roadmap and active slice are created
  - plan review is complete and implementation can start
  - a slice uncovers a blocking issue
  - closure scoring is complete
- For implementation, report validation commands and unresolved risks after each slice.

## 13. Completion Checklist
- [x] Roadmap document created before implementation
- [x] Roadmap reviewed according to specialist-subagents requirement
- [x] Active slice reviewed before implementation
- [x] Temporary `AGENTS.md` rule added
- [x] Work executed according to the plan or the plan updated first
- [x] Each completed slice forms an independent verification/rollback unit
- [ ] Verification completed
- [ ] Lasting requirements/spec/design/test updates moved into permanent docs
- [ ] Temporary `AGENTS.md` rule removed after completion
- [ ] Change plan deleted or archived according to project practice after completion

## 14. Final Notes
This roadmap intentionally starts with traceability cleanup before code. The project already has high automated test volume; the highest leverage first step is restoring the reliability of the SDD trace map, then using it to safely drive security and decomposition work.

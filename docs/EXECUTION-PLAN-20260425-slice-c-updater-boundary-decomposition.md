# EXECUTION PLAN: Slice C Updater Boundary Decomposition

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
- Execution Mode Policy: Inherits the parent roadmap policy. This slice changes a security-sensitive self-update module but is intended to be behavior-preserving. Implementation must not start until initial review, required revisions, convergence review, and Review Notes updates are complete.
- Parent Plan: `docs/EXECUTION-PLAN-20260425-roadmap-quality-hardening-90.md`
- Child Plan(s): none
- Scope Label: quality-hardening-90 / slice-c-updater-boundary
- Related Tickets/Issues: none
- Review Status: レビュー済み
- Review Notes:
  - 2026-04-25 initial specialist review started with architecture, security, and testing perspectives.
  - Architecture review found one blocking issue: non-current platform cfg validation was insufficient for moving platform-gated apply code.
  - Security review found three blocking issues: Windows target validation was missing, verification-before-apply was only a prose invariant, and Slice B no-overwrite helper script creation was not explicitly protected across the `staging.rs` / `apply.rs` split.
  - Testing review found two blocking issues: helper script generation lacked module-local validation, and `cargo fmt -- --check` baseline behavior was not defined despite existing repository-wide fmt failures.
  - 2026-04-25 revised plan to require Windows GNU target validation when available, a private verified bundle boundary before apply spawn, no-overwrite helper script creation through staging primitives, module-local apply script tests, forbidden overwrite-pattern checks, and touched-file formatting with existing fmt baseline recording.
  - 2026-04-25 convergence review completed by architecture, security/rollback, and testing reviewers.
  - Convergence result: all initial blockers were resolved in the plan; no material blockers remain.
  - Status changed to `レビュー済み`; implementation may start.

## 1. Background
`rust/src/updater.rs` is currently over 1,200 lines and owns release discovery, candidate resolution, staging/download, checksum/signature verification, platform helper generation, and tests. Slice B hardened staging primitives, but the module remains too broad for safe review. The roadmap calls for updater boundary decomposition before render/search/indexer decomposition.

## 2. Goal
Split updater responsibilities into private submodules while preserving the public updater contract and existing update behavior:

- Public API remains available through `crate::updater`.
- Windows/Linux auto-update and macOS manual-only behavior do not change.
- Signature/checksum verification order does not change.
- Slice B staging hardening remains intact.
- Tests remain at least as strong as before and move with their owning seams where practical.
- No release asset naming, feed, runtime config, or user-facing update UX changes are introduced.

## 3. Scope
### In Scope
- `rust/src/updater.rs` facade cleanup.
- New private modules under `rust/src/updater/`.
- Responsibility-preserving moves for:
  - release/candidate resolution
  - staging/download helpers
  - checksum/signature manifest verification
  - platform apply helper spawning
- Unit test relocation or module-local test updates required by the move.
- Architecture/design/testplan notes if the updater boundary description changes.
- Roadmap/TASKS progress updates for Slice C.

### Out of Scope
- Changing update network protocol or feed format.
- Changing release asset names.
- Changing helper apply semantics or post-helper rollback behavior.
- Dependency upgrades.
- GUI update dialog behavior.
- Removing or exposing internal dev/test update override variables.

## 4. Constraints and Assumptions
- This is a behavior-preserving decomposition. Any behavior change discovered as necessary must update this plan before implementation.
- `rust/src/lib.rs` should continue to expose `pub mod updater;`.
- A file module `rust/src/updater.rs` may declare sibling submodules in `rust/src/updater/*.rs`; a wholesale move to `rust/src/updater/mod.rs` is allowed only if it reduces risk and keeps imports stable.
- New submodules should stay private unless another crate boundary needs them.
- Do not expose `pub mod release`, `pub mod staging`, `pub mod manifest`, or `pub mod apply` from `crate::updater`. Use private modules and at most `pub(super)` for facade orchestration seams.
- The facade API allowlist is: `UpdateCandidate`, `UpdateSupport`, `current_version_string`, `self_update_disabled`, `forced_update_check_failure_message`, `check_for_update`, `prepare_and_start_update`, and `should_skip_update_prompt`.
- Avoid broad rename churn and avoid mixing movement-only changes with semantic changes.
- Public docs must not add new forbidden internal update override mentions.
- Rust changes require `cargo test --locked`, `cargo clippy --all-targets -- -D warnings`, and Windows GNU target compile validation when the target is installed.
- Repository-wide `cargo fmt -- --check` currently fails on pre-existing files outside Slice C (`rust/src/app/session.rs`, `rust/src/app/shell_support.rs`, `rust/src/runtime_config.rs`). Slice C must format all touched/new Rust files and record the existing fmt baseline instead of treating repository-wide fmt failure as a Slice C regression, unless the slice explicitly chooses to fix the baseline first.

## 5. Current Risks
- Risk: Moving private types breaks tests or app worker imports.
  - Impact: compile/test failure.
  - Mitigation: keep `UpdateCandidate`, `UpdateSupport`, `check_for_update`, `prepare_and_start_update`, `current_version_string`, `self_update_disabled`, `forced_update_check_failure_message`, and `should_skip_update_prompt` re-exported or defined in the facade.
- Risk: Decomposition accidentally changes verification/apply order.
  - Impact: self-update security regression.
  - Mitigation: keep `prepare_and_start_update` orchestration in the facade until the moved helpers are proven equivalent; introduce a private verified bundle boundary so `apply.rs` can only spawn from already verified staged paths; run existing update/security tests.
- Risk: Platform cfg movement breaks non-current OS builds.
  - Impact: Windows/macOS/Linux compile regression.
  - Mitigation: preserve existing cfg boundaries and keep platform helper tests/compilation guarded exactly as before.
- Risk: Moved tests lose access to private helpers.
  - Impact: weaker coverage or broad `pub` exposure.
  - Mitigation: place tests inside owning modules or use `pub(super)` only where needed.

## 6. Execution Strategy
1. Establish facade and module map
   - Files/modules/components: `rust/src/updater.rs`, new `rust/src/updater/*.rs`.
   - Expected result: public updater API remains in `updater.rs`; private module names and ownership are explicit.
   - Proposed module boundaries:
     - `release.rs`: release feed URL, semver comparison, platform target, asset selection, candidate resolution.
     - `staging.rs`: exclusive temp directory, no-overwrite file creation, downloads. This slice keeps download transport here as `staging/download`; a later slice may split `download.rs` if retry/transport behavior changes.
     - `manifest.rs`: checksum parsing, SHA-256 calculation, signature-backed manifest verification.
     - `apply.rs`: Windows/Linux/macOS helper spawning and helper script generation.
   - Public API rule: submodules stay private; app-facing imports must continue to use the `crate::updater` facade.
   - Verification: compile-oriented move; no behavior changes.
2. Move release/candidate resolution
   - Files/modules/components: `rust/src/updater.rs`, `rust/src/updater/release.rs`.
   - Expected result: `check_for_update` delegates release fetch and candidate resolution to a private release module; public candidate/support structs remain stable.
   - Verification: candidate resolution tests pass.
3. Move staging and manifest helpers
   - Files/modules/components: `rust/src/updater/staging.rs`, `rust/src/updater/manifest.rs`.
   - Expected result: Slice B hardening tests move with staging; checksum/signature tests move with manifest.
   - Security invariant: staged paths are not eligible for apply until `SHA256SUMS.sig` verifies and checksum verification passes for binary, README, LICENSE, and THIRD_PARTY_NOTICES.
   - Implementation boundary: introduce a private `VerifiedUpdateBundle` or equivalent type returned only after manifest signature and all checksum checks pass. `apply.rs` must accept that verified boundary rather than arbitrary unverified staged paths.
   - Verification: collision/no-overwrite tests and signature/checksum tests pass.
4. Move platform apply helpers
   - Files/modules/components: `rust/src/updater/apply.rs`.
   - Expected result: platform helper scripts and spawn logic are isolated behind one private `spawn_update_helper` boundary; helper contents and cfg behavior remain unchanged.
   - Dependency rule: `apply.rs` must call `staging::write_new_staged_file` or an equivalent no-overwrite helper for helper scripts; `fs::write`, `File::create`, and overwrite-capable `OpenOptions::create(true)` are forbidden in updater apply/staging production code unless explicitly justified in this plan first.
   - Test rule: helper script bodies should be generated by pure helper functions where possible, with module-local tests for command content, argument order, and no-overwrite helper file creation.
   - Verification: compile on current platform; Windows GNU target check when available; existing update command tests pass; apply module tests pass. No manual self-update test is required unless helper behavior changes.
5. Synchronize docs and progress records
   - Files/modules/components: `docs/ARCHITECTURE.md`, `docs/DETAILED_DESIGN.md`, `docs/DESIGN.md`, `docs/TESTPLAN.md`, `docs/TASKS.md`, roadmap, this slice.
   - Expected result: updater boundary documentation references the new module responsibilities without adding forbidden override exposure.
   - Verification: docs diff review and forbidden override baseline check.
6. Run validation and commit
   - Files/modules/components: all touched files.
   - Expected result: Slice C is one independent rollback unit.
   - Verification: `cargo test --locked`; `cargo clippy --all-targets -- -D warnings`; `cargo check --locked --target x86_64-pc-windows-gnu` if target is installed, otherwise record unavailability and rely on CI; `git diff --check`; forbidden override baseline check; forbidden updater write-pattern check.

- [x] Review this slice plan with architecture/security/testing focus.
- [x] Create private updater submodule files.
- [x] Keep public updater API stable through the facade.
- [x] Move release/candidate resolution without changing semantics.
- [x] Move staging/download helpers and preserve Slice B tests.
- [x] Move manifest verification helpers and preserve checksum/signature tests.
- [x] Move platform helper spawn logic with existing cfg behavior.
- [x] Add or preserve a private verified bundle boundary before apply spawn.
- [x] Add module-local apply helper script tests for content, argument order, and no-overwrite creation.
- [x] Update permanent docs for updater boundary if module paths change.
- [x] Confirm no new forbidden update override mentions are added to public docs.
- [x] Confirm no forbidden overwrite-capable write APIs are introduced in updater production modules.
- [x] Run Windows GNU target compile check when available, or record why it is unavailable.
- [x] Run required validation.
- [x] Update roadmap/TASKS and mark Slice C complete.
- [x] Commit Slice C as an independent rollback unit.

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test --locked`
  - `cd rust && cargo clippy --all-targets -- -D warnings`
  - `cd rust && cargo check --locked --target x86_64-pc-windows-gnu` when the target is installed; if unavailable, record the exact failure and rely on CI for Windows target coverage
  - `git diff --check`
- Formatting:
  - format touched/new Rust files with `cargo fmt -- <touched files>` or equivalent
  - repository-wide `cargo fmt -- --check` is informative only until the existing baseline failures are fixed; do not count the known `session.rs` / `shell_support.rs` / `runtime_config.rs` diffs as Slice C failures unless they are touched
- Targeted focus:
  - `cd rust && cargo test --locked resolve_update_candidate`
  - `cd rust && cargo test --locked unique_update_temp_dir_in`
  - `cd rust && cargo test --locked open_new_staged_file`
  - `cd rust && cargo test --locked write_new_staged_file`
  - `cd rust && cargo test --locked checksum_manifest_signature`
  - `cd rust && cargo test --locked update_commands`
  - module-local apply tests for helper script content, argument order, and no-overwrite script creation
- Security/docs checks:
  - signature/checksum verification still happens before helper spawn through a private verified bundle boundary or equivalent compile-time API shape
  - helper spawn code remains platform-gated as before
  - no public updater API removal
  - no new forbidden update override mentions in public docs beyond existing baseline
  - no `fs::write`, `File::create`, or overwrite-capable `OpenOptions::create(true)` is introduced in updater production modules; existing no-overwrite helpers remain the only staged/helper file creation path
  - macOS target validation is not expected locally; if not checked, record that platform coverage remains CI/release-workflow-owned
- Manual checks:
  - none required unless helper apply behavior changes

## 9. Rollback Plan
- Revert `rust/src/updater.rs`, any new `rust/src/updater/*.rs` modules, and matching docs/test updates together.
- Because this slice is intended to be behavior-preserving, rollback should restore the pre-split module layout without data migration.
- If validation finds a behavior difference, stop and either revise this plan or split a smaller follow-up slice before committing.

## 10. Temporary `AGENTS.md` Rule Draft
Use the parent roadmap rule already present in `AGENTS.md`.

## 11. Progress Log
- 2026-04-25 Planned.
- 2026-04-25 Reviewed and revised; convergence review found no remaining material blockers.
- 2026-04-25 Implemented private updater submodules: `release.rs`, `staging.rs`, `manifest.rs`, and `apply.rs`. `updater.rs` remains the public facade.
- 2026-04-25 Added private `VerifiedUpdateBundle` boundary so platform apply receives only paths that passed signature/checksum verification.
- 2026-04-25 Preserved Slice B no-overwrite behavior by keeping helper script creation on `staging::write_new_staged_file`; apply module tests cover Windows/Linux script content, argument order, and no-overwrite script creation.
- 2026-04-25 Validation passed: `cargo test --locked`; `cargo clippy --all-targets -- -D warnings`; `cargo check --locked --target x86_64-pc-windows-gnu`; targeted updater/update command tests; `git diff --check`.
- 2026-04-25 `cargo fmt -- --check` remains failing only on pre-existing baseline files `rust/src/app/session.rs`, `rust/src/app/shell_support.rs`, and `rust/src/runtime_config.rs`; touched/new updater files were formatted with `rustfmt`.
- 2026-04-25 Forbidden override check found existing baseline mentions plus moved code references in `rust/src/updater/release.rs`; no new public-doc override exposure was added. Forbidden overwrite-pattern check found only test-only `fs::write` uses in updater tests.

## 12. Communication Plan
- Return to user if:
  - the split requires public API changes
  - platform helper behavior must change
  - tests reveal an existing updater bug that is not movement-only
  - validation fails for unrelated baseline reasons that would make the slice unsafe to commit

## 13. Completion Checklist
- [x] Slice reviewed according to required-before-and-after-revision
- [x] Updater facade keeps public API stable
- [x] Release/staging/manifest/apply responsibilities split
- [x] Required validation passed
- [x] Roadmap/TASKS updated
- [x] Slice committed

## 14. Final Notes
This slice should reduce review surface before later render/search/indexer decomposition. It should not attempt to fix post-helper apply rollback; that requires a separate behavior-changing slice if still needed after decomposition.

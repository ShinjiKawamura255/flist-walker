# EXECUTION PLAN: Slice B Self-Update Staging Hardening

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
- Execution Mode Policy: Inherits the parent roadmap policy. This slice is security-sensitive and must complete initial review, required revisions, convergence review, and Review Notes updates before implementation.
- Parent Plan: `docs/EXECUTION-PLAN-20260425-roadmap-quality-hardening-90.md`
- Child Plan(s): none
- Scope Label: quality-hardening-90 / slice-b-self-update-staging
- Related Tickets/Issues: none
- Review Status: レビュー済み
- Review Notes:
  - 2026-04-25 initial specialist review completed with security and testing perspectives.
  - Security review found one blocking issue: roadmap-level rollback expectations included post-helper apply failure while this slice only defined pre-helper fail-safe.
  - Testing review requested explicit TDD seams for file creation and temp-dir collision/retry injection, fixed `cargo audit` decision recording, and baseline-aware forbidden override checks.
  - 2026-04-25 convergence review completed by two specialist reviewers.
  - Convergence result: post-helper rollback boundary, helper script no-overwrite, TDD seams, collision injection, entropy/retry/permission, cargo audit recording, and forbidden override baseline are reflected; no blocking issues remain.
  - Status changed to `レビュー済み`; implementation may start.

## 1. Background
The current self-update path stages downloaded assets under a temp directory named with `SystemTime::now().as_nanos()` and writes staged files with ordinary `File::create`. Although update assets are signature/checksum verified before apply, the staging primitives are weaker than necessary for a security-sensitive self-update path.

The roadmap requires hardening the staging boundary before any release/tag/self-update publication work continues.

## 2. Goal
Make self-update staging fail-safe against predictable path and accidental clobber hazards without changing the supported update UX:

- Windows/Linux auto-update behavior remains intact.
- macOS remains manual-only.
- Downloaded asset, sidecar files, checksum manifest, and signature are staged in a directory created with unpredictable/exclusive semantics.
- Staged file creation refuses to overwrite existing paths.
- Generated helper scripts are also written without overwriting existing paths, even though the private exclusive temp directory should make collisions unexpected.
- Unix staging directory permissions are private where supported.
- Windows relies on the per-user temp directory plus exclusive directory and no-overwrite file creation; this slice does not attempt Windows ACL customization.
- Failure before helper spawn does not modify the current executable.
- Post-helper apply failure rollback is explicitly out of this slice and must be addressed in Slice C if the helper apply flow is changed. Slice B preserves existing helper behavior and defines fail-safe only up to helper spawn.

## 3. Scope
### In Scope
- `rust/src/updater.rs` staging directory creation.
- `rust/src/updater.rs` staged download file creation.
- Unit tests for exclusive temp dir/file behavior and failure-safe assumptions.
- Targeted docs updates if the security contract or TESTPLAN needs clarification.
- Roadmap/TASKS progress updates for Slice B.

### Out of Scope
- Network fetch logic rewrite.
- Update asset naming changes.
- Release workflow changes.
- Windows/Linux helper apply behavior changes after helper spawn.
- Updater module decomposition; that remains Slice C.
- Dependency upgrades and `cargo audit` warning resolution; this slice only records the early decision point.

## 4. Constraints and Assumptions
- Avoid adding a new dependency unless necessary. Existing `rand_core` with `getrandom` can provide random bytes.
- No release tags or release publishing while this slice is incomplete.
- Public docs must not mention forbidden internal update override variables.
- Tests should not require network access.
- Any staging hardening must be compatible with Windows and Linux path semantics.
- Random suffix generation must use at least 128 bits of randomness with bounded retries.
- Temp directory creation must use single-level exclusive creation, not `create_dir_all`, so existing directories are never reused.
- Unix permission hardening should fail the update if private permissions cannot be set after directory creation.

## 5. Current Risks
- Risk: Random/exclusive temp creation loops could fail on unusual temp directories.
  - Impact: update download fails before modifying the executable.
  - Mitigation: bounded retry with clear error; fail before helper spawn.
- Risk: `create_new` may expose existing test assumptions that used overwrite semantics.
  - Impact: tests or manual flows fail if stale staging files are reused.
  - Mitigation: exclusive temp dir per update should make existing files unexpected; test this path explicitly.
- Risk: Unix permissions are not meaningful on every filesystem.
  - Impact: permission hardening may be best-effort on some mounts.
  - Mitigation: set `0o700` after exclusive create on Unix and verify where supported.
- Risk: Self-update behavior changes unintentionally.
  - Impact: user-visible update regression.
  - Mitigation: keep candidate selection, signature/checksum verification, and helper spawn contracts unchanged.

## 6. Execution Strategy
1. Add failing tests for staging primitives
   - Files/modules/components: `rust/src/updater.rs`
   - Expected result: tests capture exclusive directory creation, file/helper script creation refusing existing paths, and staged directory path uniqueness/non-determinism.
   - TDD seam: add small helpers such as `open_new_staged_file`, `write_new_staged_file`, and an injectable temp-dir constructor (`unique_update_temp_dir_in` or equivalent) so network-free tests can exercise overwrite and collision behavior.
   - Verification: targeted updater tests fail before implementation where applicable.
2. Harden temp directory creation
   - Files/modules/components: `rust/src/updater.rs`
   - Expected result: replace time-only temp dir name with 128-bit random suffix, bounded retry, and exclusive `create_dir`; set private permissions on Unix.
   - Collision testing: use the injectable constructor/random source seam to force at least one collision and verify the implementation retries or fails without reusing the existing directory.
   - Verification: targeted updater tests pass.
3. Harden staged file creation
   - Files/modules/components: `rust/src/updater.rs`
   - Expected result: `download_to_path` and helper script writes create files with no-overwrite semantics and never overwrite existing staged paths.
   - Verification: targeted updater tests pass.
4. Review update-specific rollback behavior
   - Files/modules/components: `rust/src/updater.rs`, this plan, roadmap/TASKS.
   - Expected result: failure before helper spawn leaves current executable untouched; partial staging files remain confined to private temp dir and are not applied. Existing post-helper apply behavior is unchanged and explicitly deferred to Slice C if further hardening is needed.
   - Verification: code review and test assertions around pre-helper paths.
5. Run validation and update progress docs
   - Files/modules/components: `docs/TASKS.md`, roadmap, this slice.
   - Expected result: Slice B progress and verification are recorded; next Slice C remains not created.
   - Verification: `cargo test --locked`; `cargo clippy --all-targets -- -D warnings`; targeted tests; `cargo audit` result and accepted-temporary/remediate decision recorded in roadmap/TASKS.

## 7. Detailed Task Breakdown
- [x] Review this slice plan with specialist security/testing focus.
- [x] Add tests for exclusive temp dir and no-overwrite staged file behavior.
- [x] Replace time-only temp dir naming with random exclusive creation.
- [x] Replace staged file `File::create` with no-overwrite creation.
- [x] Replace helper script `fs::write` with no-overwrite creation.
- [x] Add or use seams for network-free staged file tests and deterministic collision/retry tests.
- [x] Verify Unix private permission behavior where applicable.
- [x] Confirm this slice adds no new forbidden update override variable mentions to public docs; existing baseline mentions remain out of scope unless touched.
- [x] Record `cargo audit` result and whether the existing transitive warning is accepted temporarily or requires follow-up before Slice D.
- [x] Run required validation.
- [x] Update roadmap/TASKS and mark Slice B complete.
- [x] Commit Slice B as an independent rollback unit.

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test --locked`
  - `cd rust && cargo clippy --all-targets -- -D warnings`
  - `cd rust && cargo audit`
  - targeted updater tests by name after they are added
- Security checks:
  - exclusive temp directory creation uses at least 128 random bits and never reuses an existing directory
  - collision/retry behavior is covered by an injectable seam or equivalent deterministic test
  - staged download file creation refuses overwrite
  - helper script creation refuses overwrite
  - symlink/clobber risk is mitigated by private exclusive staging directory plus no-overwrite file creation
  - current executable is not touched until after signature/checksum verification and helper spawn
  - this slice adds no new forbidden update override variable mentions to public docs; pre-existing docs mentions are recorded as baseline and not expanded
- Audit decision:
  - run `cargo audit` during Slice B validation
  - record one of: `no warnings`, `accepted temporary transitive warning`, or `requires dependency/framework follow-up before Slice D`
  - if dependency changes are needed, stop and update the roadmap before proceeding
- Manual checks:
  - none required for this code-only staging primitive change unless helper spawn behavior changes

## 9. Rollback Plan
- Revert `rust/src/updater.rs` changes and matching tests together.
- If staging creation fails in production, update aborts before helper spawn and before current executable modification.
- Partial downloaded files remain in the update temp directory and are not applied.
- Existing binary and sidecar files are preserved because helper spawn is the first step that can apply staged files.
- Windows/Linux apply helper behavior is unchanged in this slice.
- Post-helper apply failure rollback is not claimed as solved by this slice. If post-helper fail-safe changes become necessary, create/update Slice C before implementing them.

## 10. Temporary `AGENTS.md` Rule Draft
Use the parent roadmap rule already present in `AGENTS.md`.

## 11. Progress Log
- 2026-04-25 Planned.
- 2026-04-25 Specialist security/testing review and convergence review completed; implementation proceeded after `Review Status: レビュー済み`.
- 2026-04-25 Implemented random 128-bit exclusive staging directory creation with bounded retries, Unix `0o700` permission hardening, no-overwrite staged download/helper file creation, deterministic collision tests, and existing-file refusal tests.
- 2026-04-25 Validation passed: `cd rust && cargo test --locked`; `cd rust && cargo clippy --all-targets -- -D warnings`; `cd rust && cargo audit`.
- 2026-04-25 `cargo audit` found the existing allowed unmaintained transitive warning `RUSTSEC-2024-0436` for `paste 1.0.15` via `eframe`/`wgpu`; accepted temporarily for this slice and left for Slice G dependency/audit follow-up.
- 2026-04-25 Forbidden override check found only the existing baseline mentions in `rust/src/runtime_config.rs`, `rust/src/updater.rs`, `docs/TESTPLAN.md`, `docs/DESIGN.md`, and `docs/RELEASE.md`; Slice B added no new public-doc override exposure.

## 12. Communication Plan
- Return to user if:
  - random/exclusive creation cannot be implemented without adding a dependency
  - Windows compatibility requires a helper behavior change
  - validation fails for reasons unrelated to this slice

## 13. Completion Checklist
- [x] Slice reviewed according to required-before-and-after-revision
- [x] Exclusive temp dir hardening implemented
- [x] No-overwrite staged file creation implemented
- [x] Security assumptions covered by tests or explicit proof
- [x] Required validation passed
- [x] Roadmap/TASKS updated
- [x] Slice committed

## 14. Final Notes
This slice intentionally avoids broad updater decomposition. It should change the staging safety properties first, then leave structural cleanup to Slice C.

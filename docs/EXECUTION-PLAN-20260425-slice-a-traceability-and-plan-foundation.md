# EXECUTION PLAN: Slice A Traceability and Plan Foundation

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
- Execution Mode Policy: Inherits the parent roadmap policy. This slice is a preparation gate and must be reviewed before implementation. Do not add the Temporary Change Plan Rule until review is complete.
- Parent Plan: `docs/EXECUTION-PLAN-20260425-roadmap-quality-hardening-90.md`
- Child Plan(s): none
- Scope Label: quality-hardening-90 / slice-a-traceability
- Related Tickets/Issues: none
- Review Status: レビュー済み
- Review Notes:
  - This slice exists to make later security and refactor slices traceable and reviewable.
  - 2026-04-25 initial specialist review completed with architecture/testing/security perspectives.
  - Architecture/testing review requested dynamic duplicate-ID detection rather than fixed known IDs, empty-output acceptance for duplicate checks, meaning-by-meaning reference update policy, and docs-only `git diff --stat` confirmation.
  - Security review accepted Slice A as docs-only preparation but requested plan-state consistency checks after completion.
  - 2026-04-25 convergence review completed by two specialist reviewers.
  - Convergence result: TC duplicate empty-output acceptance, meaning-by-meaning reference update, docs-only diff confirmation, and plan-state consistency checks are reflected; no blocking issues remain.
  - Status changed to `レビュー済み`; implementation may start after the Temporary Change Plan Rule is added to `AGENTS.md`.

## 1. Background
The assessment found duplicate `TC-*` IDs in `docs/TESTPLAN.md`. The affected IDs are currently reused for unrelated concerns, including tab accent behavior, regex/search behavior, self-update disablement, Windows `.ps1` action policy, checksum signature tamper detection, diagnostics trace smoke, and copy path notice normalization.

This weakens the SDD process because a later implementation slice cannot reliably map a requirement or regression guard to a single test case.

## 2. Goal
Make the quality-hardening roadmap executable by restoring unique test case IDs and documenting the active plan boundary.

Success conditions:

- Every table-row `TC-*` ID in `docs/TESTPLAN.md` is unique.
- The duplicate detection command must produce no output at completion.
- Renumbering preserves meaning and related SP references.
- Any trace excerpts that reference changed IDs are updated by test-case meaning, not by blind ID substitution, or explicitly left unchanged with a reason.
- Roadmap and slice metadata reflect the actual review state before implementation starts.
- No Rust behavior changes are made in this slice.

## 3. Scope
### In Scope
- `docs/TESTPLAN.md` duplicate `TC-*` cleanup.
- Reference checks for changed `TC-*` IDs across docs.
- `docs/TASKS.md` status update that records this roadmap as planned but not yet implemented.

### Out of Scope
- Rust source changes.
- Test implementation changes.
- CI workflow changes.
- Self-update hardening implementation.
- Module decomposition.

## 4. Constraints and Assumptions
- This is a docs-only preparation slice.
- Existing test names do not necessarily embed every `TC-*` ID; do not rename Rust tests unless a later reviewed slice requires it.
- Prefer assigning new IDs above the current maximum to avoid changing stable earlier references more than necessary.
- If duplicate rows are old regression entries, preserve their text and only change IDs.

## 5. Current Risks
- Risk: Renumbering breaks trace references.
  - Impact: requirements/spec/design/test linkage becomes harder to audit.
  - Mitigation: run `rg` for old and new IDs and update trace excerpts where direct references exist.
- Risk: New IDs collide with undocumented future IDs.
  - Impact: same problem reappears.
  - Mitigation: choose a contiguous reserved range and add a short note if needed.
- Risk: Docs-only slice appears to satisfy roadmap without improving code.
  - Impact: false progress.
  - Mitigation: completion requires creating the reviewed path for Slice B, not closing the roadmap.

## 6. Execution Strategy
1. Identify duplicate IDs
   - Files/modules/components: `docs/TESTPLAN.md`
   - Expected result: list duplicate table-row IDs and their line numbers.
   - Verification: `rg '^\\| TC-' docs/TESTPLAN.md | cut -d'|' -f2 | sed 's/^ *//; s/ *$//' | sort | uniq -d`
   - Acceptance: command output is empty after edits.
2. Assign replacement IDs
   - Files/modules/components: `docs/TESTPLAN.md`
   - Expected result: unrelated duplicate rows get unique IDs, preferably in a new high range.
   - Verification: manual diff review.
3. Update references
   - Files/modules/components: `docs/*.md`
   - Expected result: direct references to the changed test cases point to the new ID where appropriate.
   - Verification: use `rg` to inspect old and new IDs across `docs/`, then confirm each reference points to the same test-case meaning as before.
   - Known starting duplicates: `TC-093`, `TC-094`, `TC-095`, `TC-100`, `TC-101`.
4. Record roadmap status
   - Files/modules/components: `docs/TASKS.md`
   - Expected result: current active roadmap points to this unreviewed quality-hardening roadmap.
   - Verification: docs diff review.
5. Confirm docs-only scope and plan-state consistency
   - Files/modules/components: `docs/TASKS.md`, roadmap, slice plan.
   - Expected result: `git diff --stat` shows only docs changes for this slice, and roadmap/slice Review Status and Notes reflect actual review state.
   - Verification: `git diff --stat`; manual review of plan metadata.

## 7. Detailed Task Breakdown
- [x] Run duplicate ID check and record exact duplicates.
- [x] Allocate new IDs for the second and later uses of duplicated IDs.
- [x] Patch `docs/TESTPLAN.md`.
- [x] Patch trace references in docs if required.
- [x] Update `docs/TASKS.md` status snapshot.
- [x] Re-run duplicate ID check.
- [x] Confirm duplicate ID check output is empty.
- [x] Confirm `git diff --stat` is docs-only.
- [x] Confirm roadmap/slice Review Notes match actual review state.

## 8. Validation Plan
- Automated/docs checks:
  - `rg '^\\| TC-' docs/TESTPLAN.md | cut -d'|' -f2 | sed 's/^ *//; s/ *$//' | sort | uniq -d`
  - The duplicate check above must return no output.
  - `rg 'TC-[0-9]{3}[A-Z]?' docs/TESTPLAN.md docs/REQUIREMENTS.md docs/SPEC.md docs/DESIGN.md`
  - targeted `rg` checks for old and newly allocated IDs across `docs/`
- Manual checks:
  - Review changed rows for semantic preservation.
  - Confirm no Rust source files changed.
  - Confirm `git diff --stat` is docs-only.
  - Confirm roadmap and slice plan status fields reflect actual review state.
- Performance or security checks:
  - Not applicable for this docs-only slice.
- Regression focus:
  - SDD trace integrity.

## 9. Rollback Plan
- Revert docs changes in this slice only.
- No data, config, binary, or release artifact rollback is required.

## 10. Temporary `AGENTS.md` Rule Draft
Use the parent roadmap draft. This slice should not add a separate temporary rule before review.

## 11. Progress Log
- 2026-04-25 Planned.
- 2026-04-25 Implemented after specialist review and convergence review. Reassigned duplicate rows to `TC-117` through `TC-121`, updated meaning-preserving references, and confirmed the duplicate check returns no output.

## 12. Communication Plan
- Return to user after this slice is reviewed and ready to execute.
- If duplicate IDs are more extensive than expected, update the parent roadmap before continuing.

## 13. Completion Checklist
- [x] Planned document created before implementation
- [x] Slice reviewed according to required-before-and-after-revision
- [x] Duplicate TC IDs removed
- [x] Reference checks completed
- [x] `docs/TASKS.md` updated
- [x] Temporary `AGENTS.md` rule added only after review, if implementation continues

## 14. Final Notes
This slice deliberately treats traceability as a prerequisite rather than a cleanup footnote. Later security/refactor slices should not proceed while test IDs remain ambiguous.

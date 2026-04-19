# EXECUTION PLAN: Slice D Supportability Without Telemetry

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
- Execution Mode Policy: Follow parent roadmap. Keep this slice to user-initiated reporting docs and GitHub issue templates; do not add telemetry, crash upload, analytics, or runtime log collection code.
- Parent Plan: docs/EXECUTION-PLAN-20260419-roadmap-quality-maturity-uplift.md
- Child Plan(s): none
- Scope Label: supportability-without-telemetry
- Related Tickets/Issues: external multi-axis evaluation dated 2026-04-18
- Review Status: reviewed
- Review Notes:
  - 2026-04-19 main-agent review: feasible. The project currently has no `.github/ISSUE_TEMPLATE` files. Adding issue templates plus a support guide improves operational maturity without changing runtime behavior or collecting data automatically. `single-subagent` review is not executed because subagent spawning requires explicit user delegation.

## 1. Background
The external evaluation scored operational maturity at 40/100 and called out missing feedback, crash report, and log collection paths. The project policy excludes default telemetry and public documentation of internal test-only environment variables, so the safe first improvement is user-initiated support guidance.

## 2. Goal
Provide a clear failure-reporting path that helps maintainers reproduce problems without automatic upload or background data collection.

Observable success conditions:
- GitHub issue templates request OS, version, reproduction steps, expected/actual behavior, and redaction of private paths.
- A support guide explains how to collect basic version/build/test context and what not to attach.
- TESTPLAN and roadmap record the supportability validation boundary.

## 3. Scope
### In Scope
- Add `.github/ISSUE_TEMPLATE/bug_report.yml`.
- Add `.github/ISSUE_TEMPLATE/feature_request.yml`.
- Add `.github/ISSUE_TEMPLATE/config.yml`.
- Add `docs/SUPPORT.md`.
- Add a README support link.
- Update `docs/TESTPLAN.md`, `docs/TASKS.md`, parent roadmap, and this slice.

### Out of Scope
- Automatic crash reporting.
- Usage analytics or telemetry.
- Runtime log bundle generation code.
- Publicly documenting internal update override or window trace environment variables.
- Installer, notarization, or package-manager work.

## 4. Constraints and Assumptions
- Docs-only and GitHub-template changes use VM-001 validation.
- Rust tests are not required because no Rust files change.
- Support docs must avoid requesting secrets, tokens, or unredacted private paths.

## 5. Current Risks
- Risk: Support guidance asks users to paste sensitive file paths or logs.
  - Impact: privacy leakage in public issues.
  - Mitigation: templates and support docs explicitly require redaction and minimal reproduction.
- Risk: Support docs drift into telemetry promises.
  - Impact: conflicts with the no-default-telemetry scope.
  - Mitigation: state that FlistWalker does not automatically upload diagnostics in this slice.

## 6. Execution Strategy
1. Add GitHub issue templates
   - Files/modules/components: `.github/ISSUE_TEMPLATE/*.yml`.
   - Expected result: bug reports and feature requests have structured fields.
   - Verification: YAML shape and docs diff review.
2. Add support guide and README entry
   - Files/modules/components: `docs/SUPPORT.md`, `README.md`.
   - Expected result: user-initiated support path is discoverable without runtime telemetry.
   - Verification: docs diff review and forbidden internal env var search.
3. Update validation records
   - Files/modules/components: `docs/TESTPLAN.md`, `docs/TASKS.md`, parent roadmap, this slice.
   - Expected result: supportability template validation is recorded.
   - Verification: `rg` reference check.

## 7. Detailed Task Breakdown
- [x] Add issue templates.
- [x] Add support guide and README link.
- [x] Update TESTPLAN / TASKS / roadmap records.
- [x] Run docs reference checks.

## 8. Validation Plan
- Automated tests: YAML parse via Ruby Psych for all issue templates.
- Manual checks: inspect YAML and Markdown diffs.
- Performance or security checks: verify no automatic telemetry or internal update override names are introduced on the public support surfaces.
- Regression focus: support docs must not change runtime behavior or release rules.

## 9. Rollback Plan
- Revert `.github/ISSUE_TEMPLATE/*`, `docs/SUPPORT.md`, README link, and docs plan updates together.
- No data migration or runtime rollback is needed.

## 10. Temporary `AGENTS.md` Rule Draft
Handled by parent roadmap.

## 11. Progress Log
- 2026-04-19 Planned and reviewed Slice D.
- 2026-04-19 Added GitHub issue templates, `docs/SUPPORT.md`, README support link, and TESTPLAN supportability validation entry.
- 2026-04-19 Validation passed: Ruby Psych parsed all issue template YAML files, and `rg` found no internal update override names in README, `docs/SUPPORT.md`, or `.github/ISSUE_TEMPLATE`.

## 12. Communication Plan
- Return to user after docs/template validation or if policy constraints conflict with desired supportability scope.

## 13. Completion Checklist
- [x] Planned document created before implementation
- [x] Temporary `AGENTS.md` rule already present
- [x] Slice reviewed
- [x] Issue templates added
- [x] Support guide added
- [x] Validation completed
- [x] Parent roadmap updated

## 14. Final Notes
This slice is intentionally not a crash reporter. It improves the human reporting path first, which is the lowest-risk operational maturity gain under the project's privacy and no-default-telemetry constraints.

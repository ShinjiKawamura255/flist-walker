# v0.29.0 Review Notes

## Repository Health Review

- The bounded repository review covered product purpose, documentation ownership, architecture boundaries, release/update controls, CI policy, validation assets, and known native GUI evidence gaps.
- Release readiness initially depended on fresh current-version evidence rather than a repository-wide structural repair.
- Preparation, N-1 compatibility, CI stability, candidate identity, native GUI evidence, tagged bundle integrity, and publication were handled through protected PRs or immutable run/release evidence.

## Release Decisions

- The accepted candidate and immutable tag use exact source `ec4d9b8b12e0b92a5a441f45016f55621faac93e`.
- Windows residuals, macOS native checks, Color keyboard activation, and warning exceptions preserve their formal status and exact scope. They are not inherited by another run or release.
- The Color keyboard expectation remains a product contract. The closure documentation requires a reachable keyboard route before a future affected release can report the axis PASS.

## Process Findings

- Full native GUI certification and change-focused candidate validation were previously conflated, causing repeated checks and late prerequisite discoveries. The revised GUI plan separates deterministic evidence, change-focused native checks, periodic/platform certification, and exact release deviations.
- `NOT RUN` is an evidence status, while release eligibility is a separate decision. The revised report requires an explicit deviation record instead of relabeling an axis or treating all `NOT RUN` results identically.
- Candidate and tagged warnings remain independently scoped, but each full-log inventory is now dispositioned as one bounded decision rather than one prompt per matching log line.
- PR #150 preserved its release-process exception separately: run `36079919522` attempt 1 failed macOS TC-183 at seed `0x1838`, step 110, on head `4e03a598618d2909cafe5089884dcea4db1df468`. The user authorized one failed-jobs rerun on the unchanged head; attempt 2 passed. Diagnostic PR #151 and 50 local Windows seed replays did not reproduce the failure, so the original cause remains unknown. A focused independent evidence review passed before merge. Neither the rerun nor its exact-run external Action warning exception is reusable.
- Cross-process saved-tab restoration and in-process closed-tab/recent-inactive restoration are now named separately to prevent unsupported persistence expectations.
- Codex managed worktrees start detached while the existing new-change preflight requires checked-out `master`. This run used one explicitly approved, clean, exact-master bridge. The guard was not weakened in this closure.

## Final Review

- Independent read-only final review on 2026-09-27 reported Blocking 0, Major 0, Minor 0 after the missing PR #150 TC-183 retry record and PR #148 reference were added.
- The reviewer checked the exact closure diff, roadmap and publication slices, closure acceptance, public release API readback, tag and source identities, 26-commit classification, GUI process boundaries, SDD traceability, exception non-inheritance, links, and recorded VM-001/005/006/009 validation.
- Residual risks remain the disclosed Color keyboard FAIL, native `NOT RUN` axes, unnotarized macOS artifacts, and the unidentified original TC-183 failure. Protected PR checks and merge readback remain closure gates.

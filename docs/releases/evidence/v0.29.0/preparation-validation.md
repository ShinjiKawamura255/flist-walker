# v0.29.0 Local Release-Preparation Evidence

This point-in-time preparation record is retained with its original pending-gate statements. Final candidate, GUI, tagged workflow and publication outcomes are in `LOCAL-VALIDATION.md` and `../../v0.29.0.md`.

## Scope
- Reviewed source baseline: `6f4f19ae2ae3706754493894e2d270ffec683761`.
- Proposed package/tag: `0.29.0` / `v0.29.0`.
- Release-payload change type: version and changelog metadata only. Temporary plan, policy, and evidence files exist in this worktree but are excluded from the preparation PR; no Rust source, dependency, updater, workflow, build script, package asset, or OSS notice changes are in the release payload.
- TDD disposition: inapplicable to release metadata; the existing behavior suite and release validations were run without changing tests or expectations.
- Final tagged-range report must be extended after the preparation PR merge and exact tag creation to include every commit through `v0.29.0`, including metadata-only commits.

## Validation Planner
- Command: `python scripts/validate_change.py --base origin/master --plan`.
- Result: exit 0; selected VM-001, VM-005, and VM-009, with their required details and intent checklists read.
- Coverage: required PR CI Gate `lint-and-coverage` job owns the 75% line threshold; exact run evidence is pending the preparation PR.

## Local Command Results
| Command | Result |
| --- | --- |
| `cargo fmt --check` (from `rust/`) | PASS, exit 0 |
| `cargo test --locked` (from `rust/`) | PASS, exit 0; 1,490 passed, 15 ignored, 0 failed across the executed Rust targets. One macOS-only native-menu integration case reported its expected platform skip on Windows. |
| `cargo clippy --locked --all-targets -- -D warnings` (from `rust/`) | PASS, exit 0; no warnings. |
| `cargo llvm-cov --locked --workspace --lcov --output-path target/llvm-cov/lcov.info --fail-under-lines 75` (from `rust/`) | PASS, exit 0; local LCOV line coverage is 85.19% (40,222/47,212). The PR CI Gate's exact `lint-and-coverage` run remains required. |
| `cargo audit` (from `rust/`) | PASS, exit 0; refreshed advisory database, scanned 452 dependencies against 1,268 advisories, no findings reported. The first sandbox attempt could not lock the read-only user Cargo cache; the authorized elevated retry completed successfully. |
| `python -m unittest discover -s scripts/tests` | PASS, exit 0; 62 tests. |
| `python scripts/check_repo_contract.py` | PASS, exit 0. |
| `python scripts/check_ci_policy.py --guardian .` | PASS, exit 0. |
| `python scripts/test-updater-n-minus-one-compatibility.py` | PASS, exit 0. |
| `git diff --check` | PASS, exit 0. |
| Public forbidden update-override scan | PASS; no matches in README, Japanese README, release instructions, or release template. |
| Release asset/sidecar cross-document scan | PASS, exit 0; reviewed release docs, release template, tagged workflow, and packaging scripts. |
| SDD/TC reference and ID-order contract review | PASS; `check_repo_contract.py` passed and the release preflight scans were completed. |

## Local Environment Limits And External Gates
- A direct PyYAML parse of every workflow was attempted but not available: the bundled Python has no `yaml` module, no cached PyYAML wheel was present, and no local Node YAML parser was installed. No workflow file changed in this preparation. `gh workflow list --all` succeeded and GitHub reports the repository's nine workflows as active; this checks the server-recognized workflow set, not the uncommitted tree's syntax. The required PR CI Gate and CI Policy Guardian remain authoritative and must both pass on the exact PR head before merge.
- Candidate bundle build, candidate N-1 manifest check, TC-193, candidate GUI reports, exact PR coverage artifact, tag, tagged build, release draft, publication, and durable release evidence have not run yet.
- Windows/macOS release-candidate GUI reports remain required for GSM-001..013, each with Deterministic, Native interaction, and Liveness axes. A Windows scripted query probe cannot replace native manual checks. Missing macOS native evidence blocks tag/publication unless the user explicitly grants the one-time waiver recorded by the plan.

## Metadata Inspection
- `rust/Cargo.toml` package version and the `flist-walker` root package in `rust/Cargo.lock` both read `0.29.0`.
- `CHANGELOG.md` has the dated `0.29.0` section and its Unreleased/0.29.0 compare links are consistent with `v0.28.0` as predecessor.
- Commit-level dispositions for the final 26-commit tag range are in `commit-classification.md`; no dependency or notice update was indicated by the release payload.

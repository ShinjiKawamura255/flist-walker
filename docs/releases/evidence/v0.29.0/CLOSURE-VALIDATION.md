# v0.29.0 Closure Validation

## Selection

- `python scripts/validate_change.py --base origin/master --plan` selected VM-001, VM-005, VM-006, and VM-009.
- This closure changes documentation and release evidence only. It does not change product code, workflows, dependencies, packages, or published release assets.

## Repository And Policy Checks

- `python -m unittest discover -s scripts/tests`: PASS, 62 tests.
- `python scripts/check_repo_contract.py`: PASS.
- `python scripts/check_ci_policy.py --guardian .`: PASS.
- Required GUI contract scan: PASS, 90 matching lines.
- PowerShell parser for both GUI validation scripts: PASS.
- Bash parser for all three GUI validation scripts: PASS.
- PyYAML parse: NOT RUN because the bundled validation Python does not provide `yaml`; no workflow file changed, and the repository CI-policy checker passed.

## Rust Validation

- `cargo fmt --all -- --check`: PASS.
- `cargo clippy --locked --all-targets -- -D warnings`: PASS.
- `cargo test --locked`: PASS. The library suite passed 1,432 tests with 15 declared ignored measurements; all other Rust test binaries passed with zero failures.
- `cargo audit`: PASS after scanning 452 dependencies against 1,271 advisories.
- `cargo llvm-cov --locked --workspace --lcov --output-path target/llvm-cov/lcov.info --fail-under-lines 75`: PASS, 84.46% line coverage (41,140 / 48,709).

## GUI Validation Infrastructure

- Windows deterministic wrapper: PASS, all 14 canonical groups.
- Bash deterministic wrapper: PASS, all 14 canonical groups.
- Canonical fixture hashes and root/nested FileList counts: PASS.
- Existing local GUI report preservation: PASS.
- Corrupted fixture copy rejection: PASS.
- Deterministic TSV schema, 14-group count, `--lib`, minimum discovery, and unexpected-ignored guards: PASS.
- Headful-script static contracts for exact staged allowlist, updater-artifact absence, settings isolation, report paths, and the Windows PID-bound `WM_NULL` plus launch-query trace probe: PASS.
- No headful GUI was launched for this documentation-only closure. Native release evidence remains recorded in [LOCAL-VALIDATION.md](LOCAL-VALIDATION.md).

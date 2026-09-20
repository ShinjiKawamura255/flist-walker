# v0.28.0 Local Validation Evidence

Date: 2026-09-20

## PASS

- `scripts/test-updater-n-minus-one-compatibility.py`: PASS.
- `scripts/check_repo_contract.py`: PASS.
- `scripts/check_ci_policy.py --guardian .`: PASS.
- `python -m unittest discover -s scripts/tests`: PASS, 62 tests.
- `cargo fmt --check`: PASS.
- `cargo test --locked`: PASS, 1382 library tests, 8 GUI-launch tests, 47 CLI contract tests; 15 performance/manual tests ignored by their declared contracts.
- `cargo clippy --locked --all-targets -- -D warnings`: PASS.
- `cargo audit`: PASS in host context; 452 crate dependencies scanned against 1251 RustSec advisories.
- `cargo check --locked --target x86_64-pc-windows-gnu`: PASS.
- Ubuntu WSL host: `bash -n scripts/validate-release-bundle.sh scripts/test-validate-release-bundle.sh`: PASS.
- Ubuntu WSL host: `bash scripts/test-validate-release-bundle.sh`: PASS; expected 28-asset release inventory and regression cases passed.
- `git diff --check`: PASS.

## NOT RUN / pending external evidence

- Default-branch candidate workflow, exact merged SHA, validated candidate artifact digest/expiry, tagged workflow, native Linux/macOS/Windows release jobs, warning scans, and GitHub release read-back: pending protected PR and GitHub authentication.
- Interactive macOS GUI/manual evidence: not available in this Windows host; native CI tests and tagged macOS builds are the required substitute path.

The sandbox-local `cargo audit` and WSL/Bash attempts were not treated as product failures: sandbox advisory DB locking and WSL enumeration returned access denied, while the same read-only checks passed in host context.

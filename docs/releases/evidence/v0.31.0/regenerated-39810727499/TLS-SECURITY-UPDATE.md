# TLS dependency repair — 2026-10-23 JST

This follow-up repairs the source after failed candidate39810727499. **The original candidate remains FAIL with zero artifacts.** No replacement candidate, tag, release, GUI session or repository-setting change is part of this repair. The user authorized the dependency update, tests, OSS/evidence preservation, independent review, commit, PR and merge after successful CI.

## Change boundary

- Base `d1bf330fd599cc04a333fc903c64dec77fd50aee`; continue the task-owned `codex/v031-regenerated-candidate-evidence` branch created after the original clean synchronized-master new-change preflight. Remote master was freshly read back at the same SHA before repair. The only pre-existing working changes were this task's candidate packet and release index; the user explicitly requested preserving them. Other worktrees remain untouched.
- `cargo update -p rustls-webpki@0.103.15 --precise 0.103.16`, then `cargo update -p webpki-roots@1.0.7 --precise 1.0.10`. Only two registry package versions/checksums and the compatibility package's1.x reference change. Cargo.toml/features/toolchain/audit ignores are unchanged.
- Official [0382](https://rustsec.org/advisories/RUSTSEC-2026-0382) and [0381](https://rustsec.org/advisories/RUSTSEC-2026-0381) patched ranges match the selected stable versions. Rejecting distrusted intermediate keys/removing the affected root is the intended TLS trust change; updater Ed25519 keys, manifest parser, activation and action authorization code are untouched.
- [Metadata comparison](TLS-METADATA-COMPARISON.json) covers all four packaged target resolves. Package sets, edges, enabled/declared features and licenses are unchanged after normalizing the two versions. webpki-roots MSRV rises1.64→1.71; the fixed1.97.1 compiler satisfies it. Actual LICENSE bytes are unchanged for both packages. Notice version lines, OSS review and0.31.0 Security entry are updated together.
- Existing packaging copies the root LICENSE/notices into archives, standalone sidecars and macOS Resources; updater retains variant-specific local sidecars. Source routes were reviewed without altering layout. No claim is made about nonexistent new distribution contents.

## Validation

Dependency resolution is a mechanical upstream security update, so application-behavior TDD is inapplicable. A fresh audit provides the red/green check: before, exit1 with exactly the two advisories; after, exit0 with zero vulnerabilities/warnings. No test, skip, threshold or audit exception was weakened.

| Check | Result / scope |
| --- | --- |
| Fresh `cargo audit --json` | PASS after update:1349 advisories,452 dependencies, zero vulnerabilities, warnings `{}`. Before:2 vulnerabilities, RUSTSEC-2026-0382/0381. Same advisory DB commit640bf5a9f06e66d8721e8772766531fe77c8630d. Sanitized [audit evidence](TLS-AUDIT.json). |
| Four-target `cargo metadata --locked --format-version 1 --filter-platform <target>` | PASS before/after; normalized graph/license/MSRV findings above. No unrelated dependency updates. |
| `cd rust && cargo test --locked` | PASS on native macOS with pinned Rust1.97.1: library1626 passed/19 existing ignored; binary/integration/doc tests all passed (including48 CLI contract tests and updater security/staging/apply tests). Existing subprocess-abort probe result is separate, not a new ignored test. Host execution uses the previously confirmed sandbox restriction on Unix socket/loopback fixtures. |
| `cargo fmt --all -- --check` / `cargo clippy --locked --all-targets -- -D warnings` | PASS with updated dependencies and fixed compiler. |
| Change validation | VM-001/005/009 selected by `validate_change.py --base origin/master --plan`; owning details/checklists reviewed. Dependency/applicable cargo, OSS, contract/guardian checks apply. No workflow/checker/setting implementation changed, so no immutable rollout/proof PR or unrelated tooling/full native rerun is introduced. |
| `check_repo_contract.py` / `check_ci_policy.py --guardian .` | PASS; final reference/whitespace review also required before commit. |
| Hosted cross-platform CI / protected checks | Exact final head, run links and completed outcomes are owned by the PR; required CI Gate and CI Policy Guardian must both succeed without bypass before merge. No hosted PASS is claimed by this pre-PR record. |
| Independent read-only review | Separate implementation-independent reviewer inspected tracked diff plus all new evidence at base/HEADd1bf330. Snapshot bfa710dd7a4c195ce3947101023613be01c1475229d2107786219a9522f3011d was unchanged across review. No blocking/major findings; one minor finding fixed by explicitly dating VALIDATION.md's initial-phase scope and linking this follow-up. Final focused re-review and commit/head identity are retained in the PR. |

Full hosted CI, including fresh audit, Windows GNU updater producer/E2E and Linux coverage, must validate the final PR head. Tests using signed loopback updater fixtures cover distribution security separately from real public TLS delivery; no unapproved live updater/installed-app experiment was used.

## Residual release gates

The failed packet's warning findings, missing fresh28-asset bundle/signature/actual N-1/compiler inspection, incomplete GUI release-note coverage and selected native Windows/macOS/TC-193 checks remain open. Existing GUI/dependency impact requires reconciliation in a later candidate; mock/headless PASS is not native PASS. This security merge does not grant any external Action warning exception or release GO. Rollback unit is the security lockfile/notice/docs commit, but reverting it restores vulnerable dependencies and must never be used to promote a release. Historical failed-candidate evidence is retained separately.

# Evidence validation

This is the historical validation record for the initial candidate-generation phase, before the user authorized the dependency repair and PR. Its scope/results below are not claims about the expanded branch. The subsequent dependency delta, VM selection, tests and review are recorded in [TLS-SECURITY-UPDATE.md](TLS-SECURITY-UPDATE.md); candidate39810727499 itself remains failed.

Scope: base/source `d1bf330fd599cc04a333fc903c64dec77fd50aee`, branch `codex/v031-regenerated-candidate-evidence`, additive evidence and Windows preparation only. No Rust, helper, workflow, dependency, policy or test implementation changed. Product TDD is inapplicable; source tests/builds are read from exact hosted runs.

| Check | Result |
| --- | --- |
| `python3 scripts/validate_change.py --base d1bf330fd599cc04a333fc903c64dec77fd50aee --plan` | VM-001 for this evidence diff; complete release range separately selected VM-001/002/005/006/008/009 as reconciled in REPORT.md |
| `python3 scripts/test-updater-n-minus-one-compatibility.py` | PASS: `N-1 updater compatibility regression tests passed`. This is the checker self-test, not an absent candidate-manifest test. |
| `python3 -m unittest scripts.tests.test_windows_candidate_helper` | PASS:5 tests. Existing old-candidate helper is unchanged; timeout partial output and refusal to overwrite,102 synthetic raw observations, unchanged ratio/output guards and native-Windows restriction covered. No new-candidate helper has been issued. |
| Fresh predecessor signature | PASS: publicv0.30.2 binary matches its fresh manifest; Node builtin Ed25519 verifies raw64-byte signature under the unique64-hex key extracted from that binary; one-bit manifest tamper rejected. Exact hashes/key in PREDECESSOR-TRUST.json. No binary execution. |
| Hosted logs/annotations | All14 executed jobs across source CI and candidate retrieved; skipped jobs have no execution logs. Every job's full annotation list retained in CI-REVIEW.json. No candidate artifact or build/signature result is invented. |
| Docs/identity/JSON/reference review | Source/run/attempt kept separate from original packet; Windows NOT RUN, no new ZIP/binding, audit FAIL and release BLOCKED retained. JSON and local link checks apply to every new file. |
| Repository contract / whitespace | PASS: `python3 scripts/check_repo_contract.py`, `git diff --check`; all new evidence JSON parsed, all local Markdown targets resolved, and new files checked for trailing whitespace. |
| Independent release review | NOT RUN for this regenerated failed packet. Prior review of run39087986481 does not cover it. Required before promotion/reuse acceptance or any exact-run warning exception; no such acceptance/exception/GO is granted here. |

The previous public trust anchor was downloaded afresh with `gh release download v0.30.2` (manifest, signature, fw macOS arm64). Official latest-public readback remainedv0.30.2; remote master remainedd1bf330 and no v0.31.0 tag was present. Only one read-only candidate dispatch changed external state. No tag/release operation, setting change, GUI launch or OS appearance change was performed.

New files remain task-owned evidence in the working tree; no commit, push or PR was requested for this regeneration task. The failed candidate's security repair, fresh distribution and selected native checks remain open. The earlier Linux SIGSEGV remains unexplained; the successful source coverage run's synthetic/expected cores are not treated as a new real crash or a root-cause fix.

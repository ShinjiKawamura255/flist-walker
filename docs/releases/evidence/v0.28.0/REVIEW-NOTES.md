# v0.28.0 Independent Review Notes

## Before-implementation review

- Reviewer: independent subagent `01a0bdfe-1acb-71b3-ac55-6fb594ecdaca`
- Date: 2026-09-20
- Scope: release plan, previous public tag `v0.27.1`, exact base `b316c7e9a3a5cf21363b80737dd4b023a8ec2243`, release/updater/CI/worktree boundaries.
- Initial result: blocked on missing `v0.27.1` updater capability registration, explicit release-body application/read-back, broad publication evidence exceptions, and incomplete candidate evidence requirements.
- Disposition: added the `v0.27.1` shipped capability and exact `0.27.1 -> 0.28.0` inventory test; made reviewed release-body application and exact read-back mandatory; restricted `NOT RUN` to VM-approved optional/substitutable platform/manual surfaces; required candidate artifact ID, digest, retention expiry, run URL, warning scan, and N-1 result.

## Focused re-review

- Two focused read-only passes confirmed the remediation.
- Final result: no blocking or major finding remains before metadata implementation.
- Residual risk: GitHub authentication, candidate/tagged workflow evidence, native platform evidence, and final publication read-back remain pending and are publication blockers.

No files, branches, tags, releases, or external services were mutated by the reviewer.

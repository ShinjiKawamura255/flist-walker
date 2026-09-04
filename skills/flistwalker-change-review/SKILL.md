---
name: flistwalker-change-review
description: FlistWalker の並行処理、action認可、updater、release、CI/security、worktree/PR運用など高リスクな差分を、実装agentとは独立したread-only reviewerとしてmerge前またはplan-driven checkpointでレビューするときに使う。
---

# FlistWalker Change Review

## Sources
- `AGENTS.md`
- `docs/INDEX.md`
- `docs/AI_DEVELOPMENT.md`
- `docs/TESTPLAN.md` and selected VM detail
- The exact base/head diff, commit list, validation evidence, and related SDD documents

## Contract
Act only as an independent reviewer. Do not edit files, create commits, switch branches, push, merge, publish, rerun external actions, or change repository settings. The implementation owner remains responsible for findings disposition and closure.

Use this review for changes involving at least one of these surfaces:

- bounded workers, request IDs, cancellation, shutdown, cache/reclaimer ownership, or UI-thread responsiveness;
- path/action authorization, command execution, updater trust/transaction/recovery, or disclosure boundaries;
- CI trusted policy, dependency/audit posture, release assets, signing, tags, or publication;
- shared-worktree branch lifecycle, rebase replay, cleanup, or other Git safety boundaries;
- a `plan-driven-execution` checkpoint that explicitly requires independent review.

## Procedure
1. Fix the review identity: base ref/SHA, head ref/SHA, commit range, changed files, and whether the checkout is clean. Stop if the range is ambiguous.
2. Run `python3 scripts/validate_change.py --base <base> --plan` or inspect equivalent saved output. Read only the selected VM detail and affected SDD/operations documents.
3. Review behavior and boundaries before style:
   - required invariants and backward compatibility;
   - stale/cancel/failure/rollback paths;
   - tests that would fail for the reported defect or unsafe transition;
   - missing docs, trace, OSS, GUI, platform, or release evidence;
   - validation weakening, ignored failures, hidden external mutation, or evidence that is local/transient only.
4. Compare claimed evidence with actual commands/artifacts. Mark unavailable manual/platform checks `NOT RUN`; never infer PASS from an adjacent axis.
5. Return all findings in one pass. A focused re-review may cover only remediated findings and problems introduced directly by the remediation.

## Findings
Use `blocking`, `major`, or `minor` and include all fields:

```text
Severity:
Evidence: <file:line, diff hunk, command, or artifact>
Impact:
Recommended Fix:
Residual Risk:
```

If there are no findings, state the exact reviewed range, selected VM IDs, evidence inspected, and remaining `NOT RUN` surfaces. Do not write `GO` when the range or required evidence is incomplete.

## Stop Conditions
- Base/head identity changed during review.
- The requested review requires secrets, user data, external application launch, publication, or repository-setting mutation.
- A required artifact is missing and no safe local substitute exists.
- A blocking or major finding remains unresolved at a required checkpoint.

## Completion Report
Report review range, independence/prior involvement, selected VM IDs, findings with disposition status, evidence checked, `NOT RUN` items, and residual risk.

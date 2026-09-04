# AI Development Operations

## Purpose

This repository is maintained primarily through AI-driven changes. This document owns managed-worktree modes, durable evidence rules, validation routing, PR handoff, independent review triggers, and external-action boundaries. Product behavior and test intent remain in the SDD documents.

## Source-Of-Truth Order

1. The current user request, issue, or PR defines task scope.
2. `AGENTS.md` defines project invariants, required workflows, and prohibited actions.
3. `docs/INDEX.md` routes to the owning product, design, validation, release, or CI document.
4. Git/GitHub provides live branch, commit, PR, check, tag, and release state.
5. Historical documents provide context only and never reopen completed work.

When sources disagree, stop at the first conflict that could change scope, safety, compatibility, or external state. Do not repair shared Git state by assumption.

## Managed Worktree Modes

Run `python3 scripts/agent_worktree_preflight.py <mode> ...` before edits or branch mutation.

| Mode | Use | Required identity | Mutation boundary |
| --- | --- | --- | --- |
| `review` | Read-only diagnosis or review | Clean checkout; detached HEAD allowed | No branch, commit, push, merge, or cleanup operation |
| `new-change` | New independent work | `HEAD == origin/master`; unused target branch | Create and use the target branch only in the current worktree |
| `continue-pr` | Continue an existing PR | `HEAD == verified PR head`; local head branch owned by this worktree | Never commit detached or update a branch owned by another worktree |
| `stacked-change` | Work that intentionally depends on an open parent PR | `HEAD == verified parent head`; unused child branch; recorded old-parent SHA | After parent merge, replay only `old-parent..<task-head>` and revalidate |

The preflight is read-only and returns a stop reason instead of changing Git state. Fetching refs does not authorize switching, resetting, cleaning, deleting, or committing in another worktree.

### Stacked Rebase Contract

Before replaying a stacked change:

1. Require the parent PR to be `MERGED` into `master` and read back its exact base/head identity.
2. Fetch refs and verify parent patch equivalence on `origin/master`; rebase merge means old parent SHAs need not be ancestors.
3. Record the immutable old-parent SHA and pre-replay task head. Replay only `old-parent..<task-head>` with `rebase --onto` or an equivalent exact-range operation.
4. Capture the other worktree's path, branch, HEAD, and clean status before and after. They must remain unchanged.
5. Abort on conflict. Update the task plan before resolving a conflict or changing the replay boundary.
6. Rerun integration validation and independent final review against the post-replay diff.

## Validation Routing

Use one deterministic entrypoint before choosing commands manually:

```text
python3 scripts/validate_change.py --base origin/master --plan
python3 scripts/validate_change.py --base origin/master --quick
python3 scripts/validate_change.py --base origin/master --full
```

- `--plan` classifies committed and worktree changes and prints every selected intent checklist and VM detail.
- `--quick` runs repository-contract checks and agent-tooling unit tests when applicable.
- `--full` adds locally runnable format, Rust regression, and clippy checks for non-doc validation classes.
- Platform, GUI, release, security, and external evidence remain governed by the selected checklist and VM detail. A local command never upgrades `NOT RUN` evidence from another axis.
- Unknown non-document paths fail closed to the general application validation class.

`scripts/validation-rules.json` owns mechanical path-to-VM routing and the checklist/detail pointers emitted for each VM. The [Validation Matrix](testplan/validation-matrix.md#change-type-checklist) owns intent-dependent supplemental checks; `docs/testplan/validation/` owns the VM baseline, conditional/manual requirements, and evidence interpretation.

## Durable Evidence And Current State

- `docs/CURRENT_STATUS.md` does not claim that the current HEAD passed a historical run.
- Do not cite ignored `rust/target/`, `dist/`, temporary, or `.local.*` files as durable evidence.
- Store sanitized release evidence under `docs/releases/evidence/<version>/`; use the PR body or an exact Actions run URL for ordinary change evidence.
- Verify recorded commit SHAs resolve. Prefer PR, tag, and run identities across rebase boundaries.
- Include `PASS`, `FAIL`, or `NOT RUN` with the reason and approval/environment condition needed to run a missing check.

## PR Work Packet

Use `.github/pull_request_template.md`. A PR must preserve enough context for a fresh reviewer to reconstruct the task without chat history:

- objective, non-goals, and observable acceptance criteria;
- exact base/head or old-parent/task range;
- responsibility and source-of-truth documents;
- selected VM IDs and executed evidence;
- docs/trace/OSS/GUI/platform impact;
- independent review findings and dispositions when required;
- rollback boundary, residual risk, and every external-state mutation.

## Independent Review

Use `skills/flistwalker-change-review/` for plan-driven required checkpoints and changes to bounded concurrency, action authorization, updater trust/transactions, release, immutable CI policy, security, or worktree/rebase safety.

The reviewer is read-only and independent from the implementation owner. Blocking and major findings stop the checkpoint. Minor findings are fixed or accepted with a recorded reason. Review the final post-rebase diff, not a pre-replay branch snapshot.

## Authority Matrix

| Action | Authority |
| --- | --- |
| Read files, inspect diffs, run safe local tests | Allowed within the task |
| Edit task-scoped files, create a task branch, create required commits | Allowed for an implementation request after preflight |
| Push task branch, create PR, register the prescribed rebase auto-merge and server-side post-merge branch cleanup | Allowed only as part of the requested PR workflow and after identity/validation checks |
| Launch external applications, exercise native Open/Reveal, send sensitive data | Requires explicit approval for the exact action/session |
| Change repository settings or immutable trusted CI policy | Requires the controlled rollout and explicit approval in `docs/CI_OPERATIONS.md` |
| Create/push tags or publish/edit/delete a release | Requires an explicit release task and release skills/preflight |
| Reset, clean, force-push `master`, manually delete remote branches, admin-bypass gates | Prohibited |

## Immutable CI Activation Boundary

Repository-contract and validation tools can be developed and run locally in an ordinary PR. Adding them to `.github/workflows/` or changing `scripts/check_ci_policy.py` is an immutable trusted-policy structural change. Perform that activation only through the controlled rollout in [CI_OPERATIONS.md](CI_OPERATIONS.md), including snapshot, independent review, exact-head CI evidence, temporary required-check handling, immediate restoration, read-back, and protected-route proof.

---
name: flistwalker-pr-lifecycle
description: FlistWalker のmanaged worktreeを含むGitHub PR開始、継続、stacked変更、rebase auto-merge登録、merge確認、所有worktree内の安全な同期・整理を行うときに使う。
---

# FlistWalker PR Lifecycle

## Sources
- `AGENTS.md`
- `docs/AI_DEVELOPMENT.md`
- `docs/CI_OPERATIONS.md`
- `scripts/agent_worktree_preflight.py`

## Fix The Worktree Mode
Before editing or changing a branch, fetch the required refs and select exactly one mode. The preflight is read-only; a nonzero result is a stop, not permission to repair Git state automatically.

- Review only: `python3 scripts/agent_worktree_preflight.py review`
- New change: `python3 scripts/agent_worktree_preflight.py new-change --base-ref origin/master --target-branch codex/<topic>`
- Continue PR: `python3 scripts/agent_worktree_preflight.py continue-pr --head-ref origin/<head-branch>`
- Stacked change: `python3 scripts/agent_worktree_preflight.py stacked-change --parent-ref origin/<parent-branch> --target-branch codex/<topic>`

`review` permits a clean detached HEAD and never authorizes mutation. `new-change` requires a clean current `master` with exact `origin/master` identity. `continue-pr` rejects a head branch owned by another worktree. `stacked-change` permits the parent branch to be owned elsewhere, but requires a new unused child branch and a recorded immutable old-parent boundary.

## Start Or Continue Work
1. Record `git worktree list --porcelain`, current path, branch or detached state, HEAD, selected base/parent ref, and clean status.
2. Run the selected preflight. Do not switch, reset, clean, commit in, or delete a branch owned by another worktree.
3. For `new-change`, create the unused target branch in the current worktree before the first commit.
4. For `continue-pr`, proceed only when the current worktree owns the local head branch and its HEAD matches the verified PR head.
5. For `stacked-change`, record the exact old-parent SHA before creating the child branch. After the parent rebase-merges, replay only `<old-parent>..<task-head>` onto updated `origin/master`; verify parent patch equivalence, abort on conflict, revalidate, and review the post-replay diff.

## Create The PR
1. Use `.github/pull_request_template.md`; record objective, non-goals, acceptance, exact range, selected VM IDs, evidence, review, risks, and external mutations.
2. Commit and push only task-owned changes. Confirm the PR base is `master` and the head name is the recorded branch.
3. Register `gh pr merge <number> --auto --rebase --delete-branch` exactly once and read back the result. This prescribed flag and the repository auto-delete setting are the allowed server-side post-merge cleanup path; do not use manual `git push --delete`. Do not use merge, squash, admin bypass, or direct `master` push.

## Confirm Merge And Clean Up
1. Treat `gh pr view <number> --json state,mergedAt,baseRefName,headRefName,url` as authoritative. Require `MERGED`, a non-null `mergedAt`, base `master`, and exact head identity.
2. Fetch refs. Never mutate another worktree to synchronize local `master`; owner-worktree synchronization is a separate clean-state operation. GitHub merge completion does not depend on local cleanup.
3. Delete a local feature branch only when it is not checked out by any worktree and is fully merged or proven patch-equivalent to synchronized `origin/master`. Prefer normal deletion.
4. If rebase rewriting is the only reason normal deletion fails, use forced local deletion only after the exact PR, patch-equivalence, no-merge-commit, unused-worktree, non-`master`, and clean-state checks in `docs/CI_OPERATIONS.md`. Remote cleanup remains the prescribed PR flag/repository auto-delete's responsibility.

## Forbidden Actions
- `git reset --hard`, `git clean`, direct `master` push, `master` rebase/merge, manual remote branch deletion, admin bypass, or duplicate auto-merge registration.
- Detached commits for `continue-pr`, mutation of an occupied branch, or cleanup based only on matching commit subjects.
- Parent replay by `origin/master..<task-head>` when that range contains parent-PR commits; always use the recorded old-parent boundary.

## Completion Report
Report mode/preflight result, PR identity and merge state, selected validation, worktree ownership, task commit range, synchronization/cleanup performed or deferred, and any stop reason.

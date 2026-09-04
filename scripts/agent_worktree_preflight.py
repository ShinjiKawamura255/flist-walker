#!/usr/bin/env python3
"""Read-only preflight for AI work in shared Git worktrees."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


class RepositoryState:
    def __init__(
        self,
        *,
        root: str,
        current_worktree: str,
        head: str,
        branch: str | None,
        dirty: bool,
        refs: dict[str, str],
        worktrees: dict[str, str | None],
        local_branches: dict[str, str],
        remote_branches: dict[str, str],
    ) -> None:
        self.root = root
        self.current_worktree = current_worktree
        self.head = head
        self.branch = branch
        self.dirty = dirty
        self.refs = refs
        self.worktrees = worktrees
        self.local_branches = local_branches
        self.remote_branches = remote_branches


class Evaluation:
    def __init__(self, ok: bool, reasons: list[str], facts: dict[str, Any]) -> None:
        self.ok = ok
        self.reasons = reasons
        self.facts = facts


def short_branch(ref: str) -> str:
    for prefix in ("refs/heads/", "refs/remotes/origin/", "origin/"):
        if ref.startswith(prefix):
            return ref[len(prefix) :]
    return ref


def target_exists(state: RepositoryState, target: str) -> bool:
    return target in state.local_branches or f"origin/{target}" in state.remote_branches


def branch_owner_elsewhere(state: RepositoryState, branch: str) -> bool:
    current = str(Path(state.current_worktree).resolve())
    return any(
        str(Path(path).resolve()) != current and owner == branch
        for path, owner in state.worktrees.items()
    )


def evaluate(
    state: RepositoryState,
    *,
    mode: str,
    base_ref: str | None = None,
    head_ref: str | None = None,
    parent_ref: str | None = None,
    target_branch: str | None = None,
) -> Evaluation:
    reasons: list[str] = []
    if state.dirty:
        reasons.append("worktree_is_dirty")

    if mode == "new-change":
        if not base_ref or not target_branch:
            reasons.append("base_ref_and_target_branch_are_required")
        elif base_ref not in state.refs:
            reasons.append("base_ref_is_missing")
        else:
            if state.branch != short_branch(base_ref):
                reasons.append("current_branch_does_not_match_base")
            if state.head != state.refs[base_ref]:
                reasons.append("head_does_not_match_base")
            if target_exists(state, target_branch):
                reasons.append("target_branch_already_exists")
    elif mode == "continue-pr":
        if not head_ref:
            reasons.append("head_ref_is_required")
        elif head_ref not in state.refs:
            reasons.append("head_ref_is_missing")
        else:
            if state.head != state.refs[head_ref]:
                reasons.append("head_does_not_match_pr")
            branch = short_branch(head_ref)
            if state.branch != branch:
                reasons.append("current_branch_does_not_match_pr")
            if branch_owner_elsewhere(state, branch):
                reasons.append("head_branch_owned_by_other_worktree")
    elif mode == "stacked-change":
        if not parent_ref or not target_branch:
            reasons.append("parent_ref_and_target_branch_are_required")
        elif parent_ref not in state.refs:
            reasons.append("parent_ref_is_missing")
        else:
            if state.head != state.refs[parent_ref]:
                reasons.append("head_does_not_match_parent")
            if target_exists(state, target_branch):
                reasons.append("target_branch_already_exists")
    elif mode != "review":
        reasons.append("unsupported_mode")

    facts = {
        "root": state.root,
        "current_worktree": state.current_worktree,
        "head": state.head,
        "branch": state.branch,
        "dirty": state.dirty,
        "worktrees": state.worktrees,
    }
    return Evaluation(not reasons, reasons, facts)


def git(root: Path, arguments: list[str], *, allow_failure: bool = False) -> str:
    result = subprocess.run(
        ["git", *arguments],
        cwd=root,
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0 and not allow_failure:
        raise RuntimeError(result.stderr.strip() or "git command failed")
    return result.stdout.strip()


def branch_map(root: Path, prefix: str) -> dict[str, str]:
    output = git(
        root,
        ["for-each-ref", "--format=%(refname:short) %(objectname)", prefix],
    )
    result: dict[str, str] = {}
    for line in output.splitlines():
        if line:
            name, oid = line.split(maxsplit=1)
            result[name] = oid
    return result


def worktree_map(root: Path) -> dict[str, str | None]:
    output = git(root, ["worktree", "list", "--porcelain"])
    result: dict[str, str | None] = {}
    current: str | None = None
    for line in [*output.splitlines(), ""]:
        if line.startswith("worktree "):
            current = line.removeprefix("worktree ")
            result[current] = None
        elif line.startswith("branch ") and current:
            result[current] = short_branch(line.removeprefix("branch "))
        elif not line:
            current = None
    return result


def collect_state(root: Path, requested_refs: list[str]) -> RepositoryState:
    root = Path(git(root, ["rev-parse", "--show-toplevel"]))
    refs: dict[str, str] = {}
    for ref in sorted(set(filter(None, requested_refs))):
        oid = git(root, ["rev-parse", "--verify", ref], allow_failure=True)
        if oid:
            refs[ref] = oid.splitlines()[-1]
    branch = git(root, ["symbolic-ref", "--short", "-q", "HEAD"], allow_failure=True)
    return RepositoryState(
        root=str(root),
        current_worktree=str(root.resolve()),
        head=git(root, ["rev-parse", "HEAD"]),
        branch=branch or None,
        dirty=bool(git(root, ["status", "--porcelain"])),
        refs=refs,
        worktrees=worktree_map(root),
        local_branches=branch_map(root, "refs/heads"),
        remote_branches=branch_map(root, "refs/remotes/origin"),
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=".")
    parser.add_argument("--json", action="store_true")
    subparsers = parser.add_subparsers(dest="mode", required=True)
    subparsers.add_parser("review")
    new_change = subparsers.add_parser("new-change")
    new_change.add_argument("--base-ref", default="origin/master")
    new_change.add_argument("--target-branch", required=True)
    continue_pr = subparsers.add_parser("continue-pr")
    continue_pr.add_argument("--head-ref", required=True)
    stacked = subparsers.add_parser("stacked-change")
    stacked.add_argument("--parent-ref", required=True)
    stacked.add_argument("--target-branch", required=True)
    args = parser.parse_args()

    refs = [
        getattr(args, "base_ref", None),
        getattr(args, "head_ref", None),
        getattr(args, "parent_ref", None),
    ]
    try:
        state = collect_state(Path(args.root), refs)
        result = evaluate(
            state,
            mode=args.mode,
            base_ref=getattr(args, "base_ref", None),
            head_ref=getattr(args, "head_ref", None),
            parent_ref=getattr(args, "parent_ref", None),
            target_branch=getattr(args, "target_branch", None),
        )
    except (OSError, RuntimeError) as error:
        print(f"worktree preflight failed: {error}", file=sys.stderr)
        return 2

    payload = {"ok": result.ok, "mode": args.mode, "reasons": result.reasons, **result.facts}
    if args.json:
        print(json.dumps(payload, indent=2, sort_keys=True))
    else:
        print(f"mode: {args.mode}")
        print(f"result: {'PASS' if result.ok else 'STOP'}")
        print(f"head: {state.head}")
        print(f"branch: {state.branch or '(detached)'}")
        for reason in result.reasons:
            print(f"reason: {reason}")
    return 0 if result.ok else 1


if __name__ == "__main__":
    raise SystemExit(main())

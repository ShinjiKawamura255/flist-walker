#!/usr/bin/env python3
"""Select FlistWalker validation routes from a Git change set."""

from __future__ import annotations

import argparse
import fnmatch
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


def load_rules(path: Path) -> list[dict[str, Any]]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    if payload.get("schema") != "flistwalker.validation-routing.v1":
        raise ValueError(f"unsupported validation rule schema in {path}")
    rules = payload.get("rules")
    if not isinstance(rules, list) or not rules:
        raise ValueError(f"validation rules are missing in {path}")
    ids = [rule.get("id") for rule in rules]
    if len(ids) != len(set(ids)):
        raise ValueError(f"duplicate validation ids in {path}")
    for rule in rules:
        if not isinstance(rule.get("patterns"), list) or not rule["patterns"]:
            raise ValueError(f"validation rule {rule.get('id')} has no path patterns")
        if not isinstance(rule.get("detail"), str) or not rule["detail"]:
            raise ValueError(f"validation rule {rule.get('id')} has no detail document")
        if not isinstance(rule.get("checklists"), list) or not rule["checklists"]:
            raise ValueError(f"validation rule {rule.get('id')} has no intent checklist")
    return rules


def parse_name_status(text: str) -> list[str]:
    paths: list[str] = []
    for line in text.splitlines():
        fields = line.split("\t")
        if len(fields) < 2:
            continue
        status = fields[0]
        if status.startswith(("R", "C")) and len(fields) >= 3:
            paths.extend(fields[1:3])
        else:
            paths.append(fields[1])
    return paths


def rule_matches(path: str, patterns: list[str]) -> bool:
    return any(fnmatch.fnmatchcase(path, pattern) for pattern in patterns)


def classify_paths(paths: list[str], rules: list[dict[str, Any]]) -> list[dict[str, Any]]:
    selected: dict[str, dict[str, Any]] = {}
    for path in paths:
        matched = False
        for rule in rules:
            if rule_matches(path, rule["patterns"]):
                selected[rule["id"]] = rule
                matched = True
        if not matched:
            fallback = next(rule for rule in rules if rule["id"] == "VM-002")
            selected[fallback["id"]] = fallback
    return [selected[key] for key in sorted(selected)]


def build_plan(paths: list[str], rules: list[dict[str, Any]]) -> dict[str, Any]:
    changed = sorted(set(paths))
    validations = [
        {
            "id": rule["id"],
            "label": rule["label"],
            "detail": rule["detail"],
            "checklists": rule["checklists"],
        }
        for rule in classify_paths(changed, rules)
    ]
    return {
        "schema": "flistwalker.validation-plan.v1",
        "changed_paths": changed,
        "validations": validations,
        "notes": [
            "Unknown non-document paths fail closed to VM-002.",
            "Read every selected intent checklist and detail document; filenames cannot infer every required condition.",
        ],
    }


def git_output(root: Path, arguments: list[str]) -> str:
    result = subprocess.run(
        ["git", *arguments],
        cwd=root,
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or "git command failed")
    return result.stdout


def changed_paths(root: Path, base: str, include_worktree: bool) -> list[str]:
    paths = parse_name_status(git_output(root, ["diff", "--name-status", f"{base}...HEAD"]))
    if include_worktree:
        paths.extend(parse_name_status(git_output(root, ["diff", "--name-status"])))
        paths.extend(parse_name_status(git_output(root, ["diff", "--cached", "--name-status"])))
        paths.extend(
            line
            for line in git_output(root, ["ls-files", "--others", "--exclude-standard"]).splitlines()
            if line
        )
    return sorted(set(paths))


def command_set(root: Path, validations: list[dict[str, Any]], level: str) -> list[list[str]]:
    ids = {item["id"] for item in validations}
    commands: list[list[str]] = [
        [sys.executable, str(root / "scripts" / "check_repo_contract.py"), str(root)]
    ]
    if ids & {"VM-009"}:
        commands.append(
            [sys.executable, "-m", "unittest", "discover", "-s", "scripts/tests"]
        )
    if level == "full" and ids - {"VM-001", "VM-007"}:
        commands.extend(
            [
                ["cargo", "fmt", "--check"],
                ["cargo", "test", "--locked"],
                ["cargo", "clippy", "--locked", "--all-targets", "--", "-D", "warnings"],
            ]
        )
    return commands


def run_commands(root: Path, commands: list[list[str]]) -> int:
    for command in commands:
        cwd = root / "rust" if command[0] == "cargo" else root
        print(f"+ {' '.join(command)}", flush=True)
        result = subprocess.run(command, cwd=cwd, check=False)
        if result.returncode != 0:
            return result.returncode
    return 0


def render_text(plan: dict[str, Any]) -> str:
    lines = ["Changed paths:"]
    lines.extend(f"- {path}" for path in plan["changed_paths"])
    lines.append("Selected validation:")
    lines.extend(
        f"- {item['id']} {item['label']}\n"
        f"  detail: {item['detail']}\n"
        f"  checklists: {', '.join(item['checklists'])}"
        for item in plan["validations"]
    )
    lines.append("Notes:")
    lines.extend(f"- {note}" for note in plan["notes"])
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=".")
    parser.add_argument("--base", default="origin/master")
    parser.add_argument("--rules", default="scripts/validation-rules.json")
    parser.add_argument("--no-worktree", action="store_true")
    parser.add_argument("--json", action="store_true")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--plan", action="store_true")
    mode.add_argument("--quick", action="store_true")
    mode.add_argument("--full", action="store_true")
    args = parser.parse_args()

    root = Path(args.root).resolve()
    rules_path = Path(args.rules)
    if not rules_path.is_absolute():
        rules_path = root / rules_path
    try:
        rules = load_rules(rules_path)
        paths = changed_paths(root, args.base, not args.no_worktree)
    except (OSError, ValueError, RuntimeError) as error:
        print(f"validation planning failed: {error}", file=sys.stderr)
        return 2
    plan = build_plan(paths, rules)
    print(json.dumps(plan, indent=2) if args.json else render_text(plan))
    if args.quick or args.full:
        return run_commands(root, command_set(root, plan["validations"], "full" if args.full else "quick"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

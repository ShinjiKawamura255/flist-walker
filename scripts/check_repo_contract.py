#!/usr/bin/env python3
"""Validate durable repository documentation and project-local skill contracts."""

from __future__ import annotations

import argparse
import json
import re
import sys
import urllib.parse
from pathlib import Path


MARKDOWN_LINK_RE = re.compile(r"\[[^\]]+\]\(([^)]+)\)")
FRONTMATTER_NAME_RE = re.compile(r"(?m)^name:\s*([^\s]+)\s*$")
ENTRYPOINT_MAX_LINE = 500
SKILL_MAX_LINES = 500
RAW_COMMIT_SHA_RE = re.compile(r"(?<![0-9A-Fa-f])[0-9A-Fa-f]{7,40}(?![0-9A-Fa-f])")


def markdown_files(root: Path) -> list[Path]:
    candidates = [root / "AGENTS.md", root / "README.md", root / "README-ja.md"]
    candidates.extend((root / "docs").rglob("*.md") if (root / "docs").is_dir() else [])
    candidates.extend((root / "skills").rglob("SKILL.md") if (root / "skills").is_dir() else [])
    return sorted({path for path in candidates if path.is_file()})


def link_target(raw_target: str) -> str:
    target = raw_target.strip()
    if target.startswith("<") and ">" in target:
        target = target[1 : target.index(">")]
    else:
        target = target.split(maxsplit=1)[0]
    return urllib.parse.unquote(target.split("#", 1)[0])


def local_link_violations(root: Path, files: list[Path]) -> list[str]:
    violations: list[str] = []
    resolved_root = root.resolve()
    for path in files:
        text = path.read_text(encoding="utf-8-sig")
        for line_number, line in enumerate(text.splitlines(), 1):
            for raw in MARKDOWN_LINK_RE.findall(line):
                if raw.startswith(("#", "http://", "https://", "mailto:")):
                    continue
                target = link_target(raw)
                if not target:
                    continue
                resolved = (path.parent / target).resolve()
                try:
                    resolved.relative_to(resolved_root)
                except ValueError:
                    relative = path.relative_to(root).as_posix()
                    violations.append(
                        f"{relative}:{line_number}: local link resolves outside repository: {raw}"
                    )
                    continue
                if not resolved.exists():
                    relative = path.relative_to(root).as_posix()
                    violations.append(
                        f"{relative}:{line_number}: missing local link target: {raw}"
                    )
    return violations


def current_status_violations(root: Path) -> list[str]:
    path = root / "docs" / "CURRENT_STATUS.md"
    if not path.is_file():
        return []
    text = path.read_text(encoding="utf-8-sig")
    violations: list[str] = []
    if re.search(r"(?mi)^##+\s+Current HEAD Validation\s*$", text):
        violations.append(
            "docs/CURRENT_STATUS.md: Current HEAD Validation is volatile; use a durable last-validated baseline or external run record"
        )
    if re.search(r"rust/target/[^\s`)]*evidence", text):
        violations.append(
            "docs/CURRENT_STATUS.md: transient rust/target evidence cannot be a durable source of truth"
        )
    if RAW_COMMIT_SHA_RE.search(text):
        violations.append(
            "docs/CURRENT_STATUS.md: raw commit SHA is volatile; use the live Git source or a durable PR, tag, release, or run identity"
        )
    return violations


def skill_violations(root: Path) -> list[str]:
    skill_root = root / "skills"
    if not skill_root.is_dir():
        return []
    violations: list[str] = []
    for path in sorted(skill_root.glob("*/SKILL.md")):
        text = path.read_text(encoding="utf-8-sig")
        match = FRONTMATTER_NAME_RE.search(text)
        relative = path.relative_to(root).as_posix()
        if match is None:
            violations.append(f"{relative}: missing skill name frontmatter")
        elif match.group(1) != path.parent.name:
            violations.append(
                f"{relative}: skill name {match.group(1)!r} does not match folder {path.parent.name!r}"
            )
        line_count = len(text.splitlines())
        if line_count > SKILL_MAX_LINES:
            violations.append(
                f"{relative}: skill exceeds {SKILL_MAX_LINES} lines ({line_count})"
            )
    return violations


def entrypoint_line_violations(root: Path) -> list[str]:
    paths = [root / "AGENTS.md"]
    skill_root = root / "skills"
    if skill_root.is_dir():
        paths.extend(sorted(skill_root.glob("*/SKILL.md")))
    violations: list[str] = []
    for path in paths:
        if not path.is_file():
            continue
        for line_number, line in enumerate(
            path.read_text(encoding="utf-8-sig").splitlines(), 1
        ):
            if len(line) > ENTRYPOINT_MAX_LINE:
                relative = path.relative_to(root).as_posix()
                violations.append(
                    f"{relative}:{line_number}: line exceeds {ENTRYPOINT_MAX_LINE} characters ({len(line)})"
                )
    return violations


def collect_violations(root: Path) -> list[str]:
    root = root.resolve()
    files = markdown_files(root)
    violations = local_link_violations(root, files)
    violations.extend(current_status_violations(root))
    violations.extend(skill_violations(root))
    violations.extend(entrypoint_line_violations(root))
    return sorted(set(violations))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", nargs="?", default=".")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    root = Path(args.root)
    violations = collect_violations(root)
    if args.json:
        print(json.dumps({"ok": not violations, "violations": violations}, indent=2))
    elif violations:
        print("Repository contract violations:", file=sys.stderr)
        for violation in violations:
            print(f"- {violation}", file=sys.stderr)
    else:
        print("Repository contract OK")
    return 1 if violations else 0


if __name__ == "__main__":
    raise SystemExit(main())

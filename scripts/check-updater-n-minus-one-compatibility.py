#!/usr/bin/env python3
"""Validate the previous public updater's checksum-manifest grammar."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


CHECKSUM_ROW_RE = re.compile(r"^([0-9A-Fa-f]{64})  (.+)$", re.ASCII)
RELEASE_VERSION_RE = re.compile(r"^(\d+)\.(\d+)\.(\d+)$", re.ASCII)
SHIPPED_FAMILY_CAPABILITIES: dict[str, tuple[str, ...]] = {
    "0.24.3": ("FlistWalker-",),
    "0.24.4": ("FlistWalker-", "fw-"),
    "0.24.5": ("FlistWalker-", "fw-"),
    "0.25.0": ("FlistWalker-", "fw-"),
    "0.25.1": ("FlistWalker-", "fw-"),
}


def parse_release_version(value: str, label: str) -> tuple[int, int, int]:
    match = RELEASE_VERSION_RE.fullmatch(value)
    if match is None:
        raise ValueError(f"unsupported {label} release version: {value}")
    return tuple(int(part) for part in match.groups())


def accepted_families(previous_version: str) -> tuple[str, ...]:
    parse_release_version(previous_version, "previous")
    # Regression guard: inferring parser behavior from version ordering let an
    # unknown but well-formed release silently pass. Every shipped predecessor
    # capability must be registered and covered by an exact-inventory test.
    try:
        return SHIPPED_FAMILY_CAPABILITIES[previous_version]
    except KeyError as error:
        raise ValueError(
            f"unsupported previous release capability: {previous_version}"
        ) from error


def parse_legacy_manifest(path: Path) -> list[str]:
    try:
        text = path.read_bytes().decode("ascii")
    except UnicodeDecodeError as error:
        raise ValueError("checksum manifest must be ASCII") from error

    names: list[str] = []
    seen: set[str] = set()
    for line_number, raw in enumerate(text.splitlines(), 1):
        match = CHECKSUM_ROW_RE.fullmatch(raw)
        if not match:
            raise ValueError(f"invalid checksum row {line_number}")
        name = match.group(2)
        if (
            name in ("", ".", "..")
            or any(character.isspace() for character in name)
            or "/" in name
            or "\\" in name
            or not (
                (name.startswith("FlistWalker-") and len(name) > len("FlistWalker-"))
                or (name.startswith("fw-") and len(name) > len("fw-"))
            )
        ):
            raise ValueError(f"unsafe checksum asset basename on row {line_number}")
        if name in seen:
            raise ValueError(f"duplicate checksum asset basename on row {line_number}")
        seen.add(name)
        names.append(name)
    if not names:
        raise ValueError("checksum manifest must contain at least one entry")
    return names


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--previous-version", required=True)
    parser.add_argument("--candidate-version", required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    args = parser.parse_args()

    try:
        previous_version = parse_release_version(args.previous_version, "previous")
        families = accepted_families(args.previous_version)
        candidate_version = parse_release_version(args.candidate_version, "candidate")
        if candidate_version <= previous_version:
            raise ValueError(
                "candidate release version must be newer than previous release: "
                f"{args.candidate_version} <= {args.previous_version}"
            )
        names = parse_legacy_manifest(args.manifest)
    except (OSError, ValueError) as error:
        print(error, file=sys.stderr)
        return 2
    rejected = [
        (row, name)
        for row, name in enumerate(names, 1)
        if not name.startswith(families)
    ]

    if not rejected:
        print(f"N-1 manifest compatibility passed for v{args.previous_version}")
        return 0

    print(
        f"v{args.previous_version} rejects candidate manifest assets: "
        + ", ".join(f"row {row}: {name}" for row, name in rejected),
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())

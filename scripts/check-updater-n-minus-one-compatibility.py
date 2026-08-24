#!/usr/bin/env python3
"""Validate the previous public updater's checksum-manifest grammar."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


CHECKSUM_ROW_RE = re.compile(r"^([0-9A-Fa-f]{64})  (.+)$", re.ASCII)


def accepted_families(previous_version: str) -> tuple[str, ...]:
    # v0.24.3 is the only public release that shipped fw while its updater still
    # accepted only the universal family. Releases after the bridge carry the
    # mixed-family parser guarded by TC-194.
    if previous_version == "0.24.3":
        return ("FlistWalker-",)
    return ("FlistWalker-", "fw-")


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
    parser.add_argument("--acknowledge-v0243-manual-update", action="store_true")
    args = parser.parse_args()

    families = accepted_families(args.previous_version)
    try:
        names = parse_legacy_manifest(args.manifest)
    except (OSError, ValueError) as error:
        print(error, file=sys.stderr)
        return 2
    rejected = [name for name in names if not name.startswith(families)]

    if not rejected:
        print(f"N-1 manifest compatibility passed for v{args.previous_version}")
        return 0

    bridge_acknowledged = (
        args.previous_version == "0.24.3"
        and args.candidate_version == "0.24.4"
        and args.acknowledge_v0243_manual_update
        and all(name.startswith("fw-") for name in rejected)
    )
    if bridge_acknowledged:
        print(
            "v0.24.3 requires the documented one-time manual update to v0.24.4; "
            "the candidate manifest is intentionally not accepted by its old parser"
        )
        return 0

    print(
        f"v{args.previous_version} rejects candidate manifest assets: {', '.join(rejected)}",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())

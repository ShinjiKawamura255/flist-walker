#!/usr/bin/env python3
"""Deterministic regression tests for the N-1 updater compatibility gate."""

from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path


def run(script: Path, manifest: Path, previous: str, candidate: str, acknowledge: bool) -> int:
    command = [
        sys.executable,
        str(script),
        "--previous-version",
        previous,
        "--candidate-version",
        candidate,
        "--manifest",
        str(manifest),
    ]
    if acknowledge:
        command.append("--acknowledge-v0243-manual-update")
    return subprocess.run(command, check=False, capture_output=True, text=True).returncode


def main() -> int:
    script = Path(__file__).with_name("check-updater-n-minus-one-compatibility.py")
    digest = "a" * 64
    with tempfile.TemporaryDirectory() as temp:
        manifest = Path(temp) / "SHA256SUMS"
        manifest.write_text(
            f"{digest}  FlistWalker-0.24.4-windows-x86_64.exe\n"
            f"{digest}  fw-0.24.4-windows-x86_64.exe\n",
            encoding="ascii",
        )
        assert run(script, manifest, "0.24.3", "0.24.4", False) == 1
        assert run(script, manifest, "0.24.3", "0.24.4", True) == 0
        assert run(script, manifest, "0.24.3", "0.24.5", True) == 1
        assert run(script, manifest, "0.24.4", "0.24.5", False) == 0

        invalid_manifests = {
            "empty": "",
            "duplicate": (
                f"{digest}  FlistWalker-0.24.4-windows-x86_64.exe\n"
                f"{digest}  FlistWalker-0.24.4-windows-x86_64.exe\n"
            ),
            "forward separator": f"{digest}  FlistWalker-0.24.4/windows.exe\n",
            "back separator": f"{digest}  FlistWalker-0.24.4\\windows.exe\n",
            "near universal prefix": f"{digest}  FlistWalkerish-0.24.4.exe\n",
            "near fw prefix": f"{digest}  fwish-0.24.4.exe\n",
            "one space": f"{digest} FlistWalker-0.24.4.exe\n",
            "three spaces": f"{digest}   FlistWalker-0.24.4.exe\n",
            "tab separator": f"{digest}\tFlistWalker-0.24.4.exe\n",
            "binary marker": f"{digest}  *FlistWalker-0.24.4.exe\n",
            "short hash": f"{'a' * 63}  FlistWalker-0.24.4.exe\n",
            "nonhex hash": f"{'g' * 64}  FlistWalker-0.24.4.exe\n",
            "empty family suffix": f"{digest}  FlistWalker-\n",
            "whitespace basename": f"{digest}  FlistWalker-bad name.exe\n",
        }
        for label, content in invalid_manifests.items():
            manifest.write_text(content, encoding="ascii")
            result = run(script, manifest, "0.24.4", "0.24.5", False)
            assert result == 2, f"{label} must fail legacy grammar validation, got {result}"

        manifest.write_bytes(
            f"{digest}  FlistWalker-0.24.4-".encode("ascii") + "é.exe\n".encode("utf-8")
        )
        assert run(script, manifest, "0.24.4", "0.24.5", False) == 2
    print("N-1 updater compatibility regression tests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

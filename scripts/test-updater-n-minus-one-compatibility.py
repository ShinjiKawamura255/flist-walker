#!/usr/bin/env python3
"""Deterministic regression tests for the N-1 updater compatibility gate."""

from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path


def run(
    script: Path,
    manifest: Path,
    previous: str,
    candidate: str,
    acknowledge: bool = False,
) -> subprocess.CompletedProcess[str]:
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
    return subprocess.run(command, check=False, capture_output=True, text=True)


def release_inventory(version: str) -> list[str]:
    names = [
        f"FlistWalker-{version}-linux-x86_64",
        f"FlistWalker-{version}-linux-x86_64.tar.gz",
        f"FlistWalker-{version}-linux-x86_64.README.txt",
        f"FlistWalker-{version}-linux-x86_64.LICENSE.txt",
        f"FlistWalker-{version}-linux-x86_64.THIRD_PARTY_NOTICES.txt",
        f"fw-{version}-linux-x86_64",
        f"FlistWalker-{version}-windows-x86_64.exe",
        f"FlistWalker-{version}-windows-x86_64.zip",
        f"FlistWalker-{version}-windows-x86_64.README.txt",
        f"FlistWalker-{version}-windows-x86_64.LICENSE.txt",
        f"FlistWalker-{version}-windows-x86_64.THIRD_PARTY_NOTICES.txt",
        f"fw-{version}-windows-x86_64.exe",
    ]
    for arch in ("arm64", "x86_64"):
        names.extend(
            [
                f"FlistWalker-{version}-macos-{arch}",
                f"FlistWalker-{version}-macos-{arch}-app.zip",
                f"FlistWalker-{version}-macos-{arch}.tar.gz",
                f"FlistWalker-{version}-macos-{arch}.README.txt",
                f"FlistWalker-{version}-macos-{arch}.LICENSE.txt",
                f"FlistWalker-{version}-macos-{arch}.THIRD_PARTY_NOTICES.txt",
                f"fw-{version}-macos-{arch}",
            ]
        )
    # The release workflow runs `sha256sum *`: the published v0.24.4 manifest
    # contains every FlistWalker-* row before the four fw-* rows.
    names.sort(key=lambda name: (name.startswith("fw-"), name))
    assert len(names) == 26
    assert all(name.startswith("FlistWalker-") for name in names[:22])
    assert all(name.startswith("fw-") for name in names[22:])
    return names


def write_manifest(path: Path, names: list[str], digest: str) -> None:
    path.write_text(
        "".join(f"{digest}  {name}\n" for name in names), encoding="ascii"
    )


def test_regression_v0243_rejects_exact_v0244_inventory(
    script: Path, manifest: Path, digest: str
) -> None:
    names = release_inventory("0.24.4")
    write_manifest(manifest, names, digest)

    result = run(script, manifest, "0.24.3", "0.24.4")

    assert result.returncode == 1, result.stderr
    first_fw_row = names.index("fw-0.24.4-linux-x86_64") + 1
    assert first_fw_row == 23
    assert f"row {first_fw_row}" in result.stderr, result.stderr


def test_regression_v0243_bridge_acknowledgement_cannot_bypass_failure(
    script: Path, manifest: Path, digest: str
) -> None:
    write_manifest(manifest, release_inventory("0.24.4"), digest)

    result = run(script, manifest, "0.24.3", "0.24.4", acknowledge=True)

    assert result.returncode == 2, result.stderr


def test_regression_v0244_accepts_exact_v0245_inventory(
    script: Path, manifest: Path, digest: str
) -> None:
    write_manifest(manifest, release_inventory("0.24.5"), digest)

    result = run(script, manifest, "0.24.4", "0.24.5")

    assert result.returncode == 0, result.stderr


def test_regression_unknown_previous_capability_fails_closed(
    script: Path, manifest: Path, digest: str
) -> None:
    write_manifest(manifest, release_inventory("0.24.5"), digest)

    for previous in ("unknown", "0.24", "0.24.4-rc.1", "0.24.2", "9.9.9"):
        result = run(script, manifest, previous, "0.24.5")
        assert result.returncode == 2, (previous, result.stderr)


def main() -> int:
    script = Path(__file__).with_name("check-updater-n-minus-one-compatibility.py")
    digest = "a" * 64
    with tempfile.TemporaryDirectory() as temp:
        manifest = Path(temp) / "SHA256SUMS"
        test_regression_v0243_rejects_exact_v0244_inventory(script, manifest, digest)
        test_regression_v0243_bridge_acknowledgement_cannot_bypass_failure(
            script, manifest, digest
        )
        test_regression_v0244_accepts_exact_v0245_inventory(script, manifest, digest)
        test_regression_unknown_previous_capability_fails_closed(
            script, manifest, digest
        )

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
            result = run(script, manifest, "0.24.4", "0.24.5")
            assert result.returncode == 2, (
                f"{label} must fail legacy grammar validation, got "
                f"{result.returncode}: {result.stderr}"
            )

        manifest.write_bytes(
            f"{digest}  FlistWalker-0.24.4-".encode("ascii") + "é.exe\n".encode("utf-8")
        )
        assert run(script, manifest, "0.24.4", "0.24.5").returncode == 2
    print("N-1 updater compatibility regression tests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Prepare one fixed, isolated macOS development app; never launch it automatically."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import shlex
import shutil
import subprocess
import sys

APP_NAME = "FlistWalker Development"
BUNDLE_ID = "local.flistwalker.development"


def validate_output_paths(base: Path) -> None:
    app = base / f"{APP_NAME}.app"
    paths = [base, base / "root", base / "root/empty-dir", base / "evidence", base / "profile",
             base / "evidence/macos-development-app.local.json", app,
             app / "Contents", app / "Contents/Info.plist", app / "Contents/MacOS",
             app / "Contents/MacOS/flistwalker"]
    for path in paths:
        if path.is_symlink():
            raise ValueError(f"Refusing symlinked development path: {path}")


def validate_fixture_outputs(repo: Path, base: Path) -> None:
    # Check before the fixture subprocess can create or overwrite anything.
    validate_output_paths(base)
    for path in [repo / "rust", repo / "rust/target"]:
        if path.is_symlink():
            raise ValueError(f"Refusing symlinked development path: {path}")
    fixture = repo / "rust/tests/fixtures/gui-smoke"
    outputs = [base / "flistwalker.ignore.txt", base / "evidence/GUI-TESTREPORT.local.md"]
    outputs.extend(base / path.relative_to(fixture) for path in (fixture / "root").rglob("*"))
    for path in outputs:
        # Include every ancestor that the fixture copy may traverse.
        for target in [path, *path.parents]:
            if target == base:
                break
            if target.is_symlink():
                raise ValueError(f"Refusing symlinked fixture output: {target}")


def prepare_bundle(source: Path, base: Path) -> Path:
    app = base / f"{APP_NAME}.app"
    info_path = app / "Contents/Info.plist"
    binary = app / "Contents/MacOS/flistwalker"
    validate_output_paths(base)
    if app.exists():
        if not info_path.is_file() or plistlib.loads(info_path.read_bytes()).get("CFBundleIdentifier") != BUNDLE_ID:
            raise ValueError("Refusing to replace an unrecognized application bundle")
    info = {
        "CFBundleExecutable": "flistwalker",
        "CFBundleIdentifier": BUNDLE_ID,
        "CFBundleName": APP_NAME,
        "CFBundleDisplayName": APP_NAME,
        "CFBundlePackageType": "APPL",
        "NSHighResolutionCapable": True,
    }
    binary.parent.mkdir(parents=True, exist_ok=True)
    (base / "profile").mkdir(exist_ok=True)
    (base / "evidence").mkdir(exist_ok=True)
    # The dedicated copy keeps the source signature; no signing or TCC changes occur.
    temporary = binary.with_name("flistwalker.pending")
    if temporary.exists() or temporary.is_symlink():
        raise ValueError(f"Unsettled development copy exists: {temporary}")
    try:
        shutil.copy2(source, temporary)
        os.replace(temporary, binary)
    finally:
        temporary.unlink(missing_ok=True)
    info_path.write_bytes(plistlib.dumps(info))
    return app


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--no-build", action="store_true")
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("This preparation command is for macOS only")
    repo = Path(__file__).resolve().parents[1]
    base = repo / "rust/target/gui-smoke"
    if any(part in {"Documents", "Desktop", "Downloads"} for part in repo.parts):
        parser.error("Use the existing non-protected development checkout")
    validate_fixture_outputs(repo, base)
    subprocess.run([str(repo / "scripts/gui-smoke-fixture.sh")], check=True)
    if not args.no_build:
        subprocess.run(["cargo", "build", "--locked", "--bin", "flistwalker"], cwd=repo / "rust", check=True)
    source = repo / "rust/target/debug/flistwalker"
    if not source.is_file() or not os.access(source, os.X_OK):
        parser.error(f"Missing executable: {source}")
    app = prepare_bundle(source, base)
    binary = app / "Contents/MacOS/flistwalker"
    command = ["env", f"HOME={base / 'profile'}", "FLISTWALKER_DISABLE_SELF_UPDATE=1",
               "FLISTWALKER_DISABLE_HISTORY_PERSIST=1", "FLISTWALKER_RESTORE_TABS=0",
               str(binary), "--root", str(base / "root"), "--limit", "1000"]
    identity = {"app": str(app), "bundle_id": BUNDLE_ID,
                "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                "head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip(),
                "dirty": bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=repo)),
                "launch_command": shlex.join(command), "launched": False}
    (base / "evidence/macos-development-app.local.json").write_text(json.dumps(identity, indent=2) + "\n")
    print(json.dumps(identity, indent=2))
    print("Prepared only. Launch requires an authorized native session; stop on any new permission prompt.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path


def choose_action(path: Path) -> str:
    if path.is_dir():
        return "open"
    if os.access(path, os.X_OK):
        return "execute"
    return "open"


def _open_with_default(path: Path) -> None:
    if sys.platform.startswith("win"):
        os.startfile(str(path))  # type: ignore[attr-defined]
        return
    if sys.platform == "darwin":
        subprocess.Popen(["open", str(path)])
        return
    subprocess.Popen(["xdg-open", str(path)])


def execute_or_open(path: Path) -> None:
    action = choose_action(path)
    if action == "execute":
        subprocess.Popen([str(path)])
        return
    _open_with_default(path)

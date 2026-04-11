from pathlib import Path

import fast_file_finder.actions as actions_mod
from fast_file_finder.actions import choose_action


def test_choose_action_for_directory(tmp_path: Path) -> None:
    assert choose_action(tmp_path) == "open"


def test_choose_action_for_executable_file(tmp_path: Path) -> None:
    file_path = tmp_path / "run.sh"
    file_path.write_text("#!/bin/sh\necho hi", encoding="utf-8")
    file_path.chmod(0o755)

    assert choose_action(file_path) == "execute"


def test_choose_action_for_non_executable_file(tmp_path: Path) -> None:
    file_path = tmp_path / "readme.txt"
    file_path.write_text("hello", encoding="utf-8")

    assert choose_action(file_path) == "open"


def test_choose_action_windows_uses_extension_check(tmp_path: Path, monkeypatch) -> None:
    exe_file = tmp_path / "tool.exe"
    exe_file.write_text("binary", encoding="utf-8")
    txt_file = tmp_path / "notes.txt"
    txt_file.write_text("plain", encoding="utf-8")
    txt_file.chmod(0o755)

    monkeypatch.setattr(actions_mod.sys, "platform", "win32")

    assert choose_action(exe_file) == "execute"
    assert choose_action(txt_file) == "open"

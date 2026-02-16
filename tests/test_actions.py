from pathlib import Path

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

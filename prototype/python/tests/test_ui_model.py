from pathlib import Path

from fast_file_finder.ui_model import (
    build_preview_text,
    format_result_html,
    format_result_label,
    has_visible_match,
)


def test_format_result_label_uses_relative_path(tmp_path: Path) -> None:
    sample = tmp_path / "src" / "main.py"
    sample.parent.mkdir(parents=True)
    sample.write_text("print('x')\n", encoding="utf-8")

    label = format_result_label(sample, tmp_path)

    assert "src/main.py" in label
    assert "FILE" in label


def test_format_result_html_highlights_matched_chars(tmp_path: Path) -> None:
    sample = tmp_path / "src" / "main.py"
    sample.parent.mkdir(parents=True)
    sample.write_text("print('x')\n", encoding="utf-8")

    html = format_result_html(sample, tmp_path, "main")

    assert "font-weight:700" in html
    assert "main.py" not in html


def test_format_result_html_highlights_partial_match(tmp_path: Path) -> None:
    sample = tmp_path / "src" / "manifest.py"
    sample.parent.mkdir(parents=True)
    sample.write_text("print('x')\n", encoding="utf-8")

    html = format_result_html(sample, tmp_path, "mns")

    assert html.count("font-weight:700") >= 3


def test_format_result_html_ignores_exclusion_token_for_highlight(tmp_path: Path) -> None:
    sample = tmp_path / "src" / "main.py"
    sample.parent.mkdir(parents=True)
    sample.write_text("print('x')\n", encoding="utf-8")

    html = format_result_html(sample, tmp_path, "main !readme")

    assert html.count("font-weight:700") >= 4


def test_format_result_html_handles_exact_token_prefix(tmp_path: Path) -> None:
    sample = tmp_path / "src" / "main.py"
    sample.parent.mkdir(parents=True)
    sample.write_text("print('x')\n", encoding="utf-8")

    html = format_result_html(sample, tmp_path, "'main")

    assert html.count("font-weight:700") >= 4


def test_has_visible_match_false_when_term_not_in_visible_text(tmp_path: Path) -> None:
    sample = tmp_path / "src" / "main.py"
    sample.parent.mkdir(parents=True)
    sample.write_text("print('x')\n", encoding="utf-8")

    assert has_visible_match(sample, tmp_path, "zzzz") is False


def test_build_preview_text_for_directory(tmp_path: Path) -> None:
    (tmp_path / "a.txt").write_text("x", encoding="utf-8")

    preview = build_preview_text(tmp_path)

    assert "Directory:" in preview
    assert "Children:" in preview


def test_build_preview_text_for_file_contains_action_and_content(tmp_path: Path) -> None:
    file_path = tmp_path / "notes.txt"
    file_path.write_text("line1\nline2\n", encoding="utf-8")

    preview = build_preview_text(file_path)

    assert "File:" in preview
    assert "Action:" in preview
    assert "line1" in preview

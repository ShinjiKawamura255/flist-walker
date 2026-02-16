from pathlib import Path

from fast_file_finder.indexer import build_index, find_filelist, parse_filelist


def test_find_filelist_prefers_uppercase_name(tmp_path: Path) -> None:
    upper = tmp_path / "FileList.txt"
    lower = tmp_path / "filelist.txt"
    upper.write_text("a.txt\n", encoding="utf-8")
    lower.write_text("b.txt\n", encoding="utf-8")

    assert find_filelist(tmp_path) == upper


def test_find_filelist_accepts_lowercase_name(tmp_path: Path) -> None:
    lower = tmp_path / "filelist.txt"
    lower.write_text("a.txt\n", encoding="utf-8")

    assert find_filelist(tmp_path) == lower


def test_parse_filelist_resolves_relative_and_absolute_paths(tmp_path: Path) -> None:
    rel_file = tmp_path / "alpha.txt"
    rel_file.write_text("x", encoding="utf-8")

    abs_file = tmp_path / "beta.txt"
    abs_file.write_text("y", encoding="utf-8")

    filelist = tmp_path / "FileList.txt"
    filelist.write_text(f"# comment\nalpha.txt\n{abs_file}\nmissing.txt\n", encoding="utf-8")

    parsed = parse_filelist(filelist, tmp_path)

    assert rel_file.resolve() in parsed
    assert abs_file.resolve() in parsed
    assert len(parsed) == 2


def test_build_index_uses_filelist_when_present(tmp_path: Path) -> None:
    listed = tmp_path / "listed.txt"
    listed.write_text("ok", encoding="utf-8")
    hidden = tmp_path / "hidden.txt"
    hidden.write_text("no", encoding="utf-8")

    (tmp_path / "FileList.txt").write_text("listed.txt\n", encoding="utf-8")

    result = build_index(tmp_path)

    assert listed.resolve() in result
    assert hidden.resolve() not in result


def test_build_index_walks_when_filelist_missing(tmp_path: Path) -> None:
    nested = tmp_path / "dir"
    nested.mkdir()
    file_path = nested / "app.py"
    file_path.write_text("print('hi')", encoding="utf-8")

    result = build_index(tmp_path)

    assert file_path.resolve() in result
    assert nested.resolve() in result

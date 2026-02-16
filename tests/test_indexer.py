from pathlib import Path

from fast_file_finder.indexer import (
    build_filelist_text,
    build_index,
    build_index_with_metadata,
    find_filelist,
    parse_filelist,
    walk_dirs,
    walk_files,
    write_filelist,
)


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


def test_build_index_with_metadata_reports_filelist_source(tmp_path: Path) -> None:
    listed = tmp_path / "listed.txt"
    listed.write_text("ok", encoding="utf-8")
    (tmp_path / "filelist.txt").write_text("listed.txt\n", encoding="utf-8")

    result = build_index_with_metadata(tmp_path)

    assert result.source == "filelist"
    assert result.filelist_path is not None
    assert listed.resolve() in result.entries


def test_build_index_with_metadata_reports_walker_source(tmp_path: Path) -> None:
    (tmp_path / "sub").mkdir()

    result = build_index_with_metadata(tmp_path)

    assert result.source == "walker"
    assert result.filelist_path is None


def test_walkers_are_separated_for_files_and_dirs(tmp_path: Path) -> None:
    folder = tmp_path / "docs"
    folder.mkdir()
    file_path = folder / "a.txt"
    file_path.write_text("x", encoding="utf-8")

    files = walk_files(tmp_path)
    dirs = walk_dirs(tmp_path)

    assert file_path.resolve() in files
    assert folder.resolve() not in files
    assert folder.resolve() in dirs
    assert file_path.resolve() not in dirs


def test_build_index_can_disable_filelist(tmp_path: Path) -> None:
    listed = tmp_path / "listed.txt"
    listed.write_text("ok", encoding="utf-8")
    extra = tmp_path / "extra.txt"
    extra.write_text("ok", encoding="utf-8")
    (tmp_path / "FileList.txt").write_text("listed.txt\n", encoding="utf-8")

    result = build_index_with_metadata(tmp_path, use_filelist=False)

    assert result.source == "walker"
    assert listed.resolve() in result.entries
    assert extra.resolve() in result.entries


def test_build_filelist_text_uses_relative_paths_when_possible(tmp_path: Path) -> None:
    folder = tmp_path / "a"
    folder.mkdir()
    file_path = folder / "b.txt"
    file_path.write_text("x", encoding="utf-8")

    text = build_filelist_text([file_path.resolve(), folder.resolve()], tmp_path)

    assert "a/b.txt" in text
    assert "a\n" in text


def test_write_filelist_writes_file(tmp_path: Path) -> None:
    folder = tmp_path / "x"
    folder.mkdir()
    file_path = folder / "run.exe"
    file_path.write_text("bin", encoding="utf-8")

    output = write_filelist(tmp_path, [file_path.resolve(), folder.resolve()])

    assert output.exists()
    content = output.read_text(encoding="utf-8")
    assert "x/run.exe" in content

from pathlib import Path

from fast_file_finder.search import search_entries


def test_search_entries_orders_by_score_and_limit() -> None:
    entries = [
        Path("/tmp/src/main.py"),
        Path("/tmp/src/README.md"),
        Path("/tmp/docs/design.md"),
    ]

    result = search_entries("main", entries, limit=2)

    assert len(result) >= 1
    assert result[0][0].name == "main.py"
    if len(result) > 1:
        assert result[0][1] >= result[1][1]


def test_search_entries_empty_query_returns_empty() -> None:
    result = search_entries("", [Path("/tmp/a.txt")])
    assert result == []


def test_search_entries_prioritizes_exact_filename_match() -> None:
    entries = [
        Path("/tmp/src/main.py"),
        Path("/tmp/src/main.py.bak"),
        Path("/tmp/src/domain_main.py"),
    ]

    result = search_entries("main.py", entries, limit=3)

    assert result[0][0].name == "main.py"


def test_search_entries_hides_non_matching_results() -> None:
    entries = [
        Path("/tmp/src/main.py"),
        Path("/tmp/docs/readme.md"),
    ]

    result = search_entries("zzz", entries, limit=10)

    assert result == []


def test_search_entries_exact_token_with_quote() -> None:
    entries = [
        Path("/tmp/src/main.py"),
        Path("/tmp/src/main.py.bak"),
    ]

    result = search_entries("'main.py", entries, limit=10)

    assert len(result) >= 1
    assert result[0][0].name == "main.py"


def test_search_entries_exact_token_matches_literal_substring() -> None:
    entries = [
        Path("/tmp/src/main.py"),
        Path("/tmp/src/domain-main.rs"),
    ]

    result = search_entries("'main", entries, limit=10)

    assert len(result) == 2


def test_search_entries_exclude_token_with_bang() -> None:
    entries = [
        Path("/tmp/src/main.py"),
        Path("/tmp/src/readme.md"),
    ]

    result = search_entries("!readme", entries, limit=10)

    assert len(result) == 1
    assert result[0][0].name == "main.py"


def test_search_entries_supports_regex_when_enabled() -> None:
    entries = [
        Path("/tmp/src/main.py"),
        Path("/tmp/src/module.rs"),
    ]

    result = search_entries("ma.*py", entries, limit=10, use_regex=True)

    assert len(result) == 1
    assert result[0][0].name == "main.py"


def test_search_entries_supports_start_anchor_without_regex() -> None:
    entries = [
        Path("/tmp/src/main.py"),
        Path("/tmp/src/amain.py"),
    ]

    result = search_entries("^main", entries, limit=10, use_regex=False)

    assert len(result) == 1
    assert result[0][0].name == "main.py"


def test_search_entries_supports_end_anchor_without_regex() -> None:
    entries = [
        Path("/tmp/src/domain"),
        Path("/tmp/src/main.py"),
    ]

    result = search_entries("main$", entries, limit=10, use_regex=False)

    assert len(result) == 1
    assert result[0][0].name == "domain"

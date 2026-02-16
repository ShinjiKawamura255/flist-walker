from pathlib import Path

from fast_file_finder.search import search_entries


def test_search_entries_orders_by_score_and_limit() -> None:
    entries = [
        Path("/tmp/src/main.py"),
        Path("/tmp/src/README.md"),
        Path("/tmp/docs/design.md"),
    ]

    result = search_entries("main", entries, limit=2)

    assert len(result) == 2
    assert result[0][0].name == "main.py"
    assert result[0][1] >= result[1][1]


def test_search_entries_empty_query_returns_empty() -> None:
    result = search_entries("", [Path("/tmp/a.txt")])
    assert result == []

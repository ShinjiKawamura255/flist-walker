from __future__ import annotations

import argparse
import sys
from pathlib import Path

from fast_file_finder.actions import execute_or_open
from fast_file_finder.indexer import build_index
from fast_file_finder.search import search_entries


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Fast fuzzy file/folder finder")
    parser.add_argument("query", nargs="?", default="", help="initial query")
    parser.add_argument("--root", default=".", help="search root")
    parser.add_argument("--limit", type=int, default=20, help="max result count")
    parser.add_argument("--gui", action="store_true", help="launch Qt GUI prototype")
    return parser.parse_args(argv)


def _limit_was_specified(argv: list[str]) -> bool:
    return "--limit" in argv


def _pick_result(results: list[tuple[Path, float]]) -> Path | None:
    if not results:
        print("No matches")
        return None

    for idx, (path, score) in enumerate(results, start=1):
        print(f"{idx:2d}. [{score:5.1f}] {path}")

    raw = input("Select number (empty to cancel): ").strip()
    if not raw:
        return None
    if not raw.isdigit():
        print("Invalid selection", file=sys.stderr)
        return None

    pick = int(raw)
    if pick < 1 or pick > len(results):
        print("Out of range", file=sys.stderr)
        return None
    return results[pick - 1][0]


def run(argv: list[str]) -> int:
    args = parse_args(argv)
    if args.gui:
        try:
            from fast_file_finder.gui import GuiDependencyError, run_gui

            gui_args = ["--root", args.root, "--query", args.query]
            if _limit_was_specified(argv):
                gui_args.extend(["--limit", str(args.limit)])
            return run_gui(gui_args)
        except GuiDependencyError as exc:
            print(str(exc), file=sys.stderr)
            return 4
        except Exception as exc:
            print(f"GUI initialization failed: {exc}", file=sys.stderr)
            return 4

    root = Path(args.root).resolve()

    try:
        entries = build_index(root)
    except Exception as exc:
        print(f"Indexing failed: {exc}", file=sys.stderr)
        return 2

    if args.query.strip():
        query = args.query.strip()
    else:
        try:
            from prompt_toolkit import prompt

            query = prompt("Query> ").strip()
        except Exception:
            query = input("Query> ").strip()
    results = search_entries(query, entries, limit=args.limit)
    selected = _pick_result(results)
    if selected is None:
        return 0

    try:
        execute_or_open(selected)
    except Exception as exc:
        print(f"Action failed: {exc}", file=sys.stderr)
        return 3
    return 0


def main() -> None:
    raise SystemExit(run(sys.argv[1:]))

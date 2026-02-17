from __future__ import annotations

import html
from pathlib import Path

from fast_file_finder.actions import choose_action


def _display_path(path: Path, root: Path) -> str:
    try:
        rel = path.resolve().relative_to(root.resolve())
        return str(rel)
    except ValueError:
        return str(path)


def format_result_label(path: Path, root: Path) -> str:
    display = _display_path(path, root)
    kind = "DIR" if path.is_dir() else "FILE"
    return f"{kind:4s} {display}"


def _find_match_positions(text: str, query: str) -> set[int]:
    if not query:
        return set()

    lower_text = text.lower()
    lower_query = query.lower()

    start = lower_text.find(lower_query)
    if start >= 0:
        return set(range(start, start + len(lower_query)))

    positions: set[int] = set()
    qi = 0
    for ti, ch in enumerate(lower_text):
        if qi < len(lower_query) and ch == lower_query[qi]:
            positions.add(ti)
            qi += 1
    if qi == len(lower_query):
        return positions
    return set()


def _highlight_terms(query: str) -> list[str]:
    terms: list[str] = []
    for token in query.split():
        if token.startswith("!"):
            continue
        if token.startswith("'"):
            token = token[1:]
        if token.startswith("^"):
            token = token[1:]
        if token.endswith("$"):
            token = token[:-1]
        if token:
            terms.append(token)
    return terms


def has_visible_match(path: Path, root: Path, query: str) -> bool:
    terms = _highlight_terms(query)
    if not terms:
        return True

    display = _display_path(path, root)
    for term in terms:
        if _find_match_positions(path.name, term):
            return True
        if _find_match_positions(display, term):
            return True
    return False


def format_result_html(path: Path, root: Path, query: str) -> str:
    kind = "DIR" if path.is_dir() else "FILE"
    display = _display_path(path, root)
    positions: set[int] = set()
    terms = _highlight_terms(query)
    basename_start = max(0, len(display) - len(path.name))
    for term in terms:
        name_hits = _find_match_positions(path.name, term)
        if name_hits:
            positions.update({basename_start + p for p in name_hits})
            continue
        positions.update(_find_match_positions(display, term))

    chunks: list[str] = []
    for i, ch in enumerate(display):
        escaped = html.escape(ch)
        if i in positions:
            chunks.append(f"<span style='color:#f59e0b;font-weight:700;'>{escaped}</span>")
        else:
            chunks.append(f"<span style='color:#e5e7eb;'>{escaped}</span>")
    highlighted = "".join(chunks)
    kind_color = "#60a5fa" if kind == "FILE" else "#34d399"
    return (
        f"<span style='font-family:Consolas,monospace;color:{kind_color};'>{kind:4s}</span> "
        f"{highlighted}"
    )


def build_preview_text(path: Path) -> str:
    if path.is_dir():
        try:
            count = sum(1 for _ in path.iterdir())
        except OSError:
            count = -1
        if count >= 0:
            return f"Directory: {path}\nChildren: {count}"
        return f"Directory: {path}\nChildren: <unavailable>"

    action = choose_action(path)
    head = [f"File: {path}", f"Action: {action}"]
    try:
        text = path.read_text(encoding="utf-8")
        preview_lines = text.splitlines()[:20]
        if preview_lines:
            return "\n".join(head + ["", *preview_lines])
        return "\n".join(head + ["", "<empty file>"])
    except UnicodeDecodeError:
        return "\n".join(head + ["", "<binary or non-utf8 file>"])
    except OSError as exc:
        return "\n".join(head + ["", f"<preview unavailable: {exc}>"])

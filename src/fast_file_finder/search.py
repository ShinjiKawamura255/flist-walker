from __future__ import annotations

from difflib import SequenceMatcher
from pathlib import Path

try:
    from rapidfuzz import fuzz, process
except ImportError:  # pragma: no cover
    fuzz = None
    process = None


def _score(query: str, text: str) -> float:
    q = query.lower()
    t = text.lower()
    if q in t:
        return 100.0
    return SequenceMatcher(None, q, t).ratio() * 100.0


def search_entries(query: str, entries: list[Path], limit: int = 20) -> list[tuple[Path, float]]:
    query = query.strip()
    if not query:
        return []
    if limit <= 0:
        return []

    mapping = {str(entry): entry for entry in entries}
    if not mapping:
        return []

    if process and fuzz:
        extracted = process.extract(
            query,
            mapping.keys(),
            scorer=fuzz.WRatio,
            limit=limit,
        )
        return [(mapping[text], float(score)) for text, score, _ in extracted]

    scored = sorted(
        ((path, _score(query, text)) for text, path in mapping.items()),
        key=lambda x: x[1],
        reverse=True,
    )
    return scored[:limit]

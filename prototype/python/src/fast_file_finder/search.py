from __future__ import annotations

import re
from dataclasses import dataclass
from difflib import SequenceMatcher
from pathlib import Path

try:
    from rapidfuzz import fuzz, process
except ImportError:  # pragma: no cover
    fuzz = None
    process = None


@dataclass(frozen=True)
class QuerySpec:
    include_terms: list[str]
    exact_terms: list[str]
    exclude_terms: list[str]


def _parse_query(query: str) -> QuerySpec:
    include_terms: list[str] = []
    exact_terms: list[str] = []
    exclude_terms: list[str] = []
    for token in query.split():
        if token.startswith("'") and len(token) > 1:
            exact_terms.append(token[1:])
        elif token.startswith("!") and len(token) > 1:
            exclude_terms.append(token[1:])
        else:
            include_terms.append(token)
    return QuerySpec(
        include_terms=include_terms,
        exact_terms=exact_terms,
        exclude_terms=exclude_terms,
    )


def _is_subsequence(query: str, text: str) -> bool:
    qi = 0
    for ch in text:
        if qi < len(query) and ch == query[qi]:
            qi += 1
    return qi == len(query)


def _is_fuzzy_match(query: str, text: str) -> bool:
    q = query.lower()
    t = text.lower()
    return q in t or _is_subsequence(q, t)


def _matches_exact_term(term: str, name: str, full: str) -> bool:
    t = term.lower()
    return _matches_anchored_literal(t, name) or _matches_anchored_literal(t, full)


def _matches_exclusion_term(term: str, name: str, full: str) -> bool:
    t = term.lower()
    return _matches_anchored_literal(t, name) or _matches_anchored_literal(t, full)


def _matches_anchored_literal(term: str, text: str) -> bool:
    anchored_start = term.startswith("^")
    anchored_end = term.endswith("$")
    core = term
    if anchored_start:
        core = core[1:]
    if anchored_end:
        core = core[:-1]
    if not core:
        return False
    if anchored_start and anchored_end:
        return text == core
    if anchored_start:
        return text.startswith(core)
    if anchored_end:
        return text.endswith(core)
    return core in text


def _matches_include_term(term: str, name: str, full: str, use_regex: bool) -> bool:
    if use_regex:
        try:
            pattern = re.compile(term, re.IGNORECASE)
        except re.error:
            return False
        return bool(pattern.search(name) or pattern.search(full))
    t = term.lower()
    anchored_start = t.startswith("^")
    anchored_end = t.endswith("$")
    core = t
    if anchored_start:
        core = core[1:]
    if anchored_end:
        core = core[:-1]
    if not core:
        return False

    # In non-regex mode, '^'/'$' constrain only adjacent characters.
    if anchored_start:
        start_char = core[0]
        if not (name.startswith(start_char) or full.startswith(start_char)):
            return False
    if anchored_end:
        end_char = core[-1]
        if not (name.endswith(end_char) or full.endswith(end_char)):
            return False
    return _is_fuzzy_match(core, name) or _is_fuzzy_match(core, full)


def _matches_spec(spec: QuerySpec, path: Path, use_regex: bool) -> bool:
    name = path.name.lower()
    full = str(path).lower()

    for term in spec.exclude_terms:
        if _matches_exclusion_term(term, name, full):
            return False
    for term in spec.exact_terms:
        if not _matches_exact_term(term, name, full):
            return False
    for term in spec.include_terms:
        if not _matches_include_term(term, name, full, use_regex):
            return False
    return True


def _score(query: str, text: str) -> float:
    q = query.lower()
    t = text.lower()
    base = SequenceMatcher(None, q, t).ratio() * 100.0
    if q in t:
        base += 25.0
    if t.startswith(q):
        base += 30.0
    return base


def search_entries(
    query: str,
    entries: list[Path],
    limit: int = 20,
    *,
    use_regex: bool = False,
) -> list[tuple[Path, float]]:
    query = query.strip()
    if not query:
        return []
    if limit <= 0:
        return []

    spec = _parse_query(query)
    filtered = [path for path in entries if _matches_spec(spec, path, use_regex)]
    if not filtered:
        return []
    mapping = {str(entry): entry for entry in filtered}

    q = " ".join(spec.include_terms).lower()
    if not q and spec.exact_terms:
        q = spec.exact_terms[0].lower()
    scored: list[tuple[Path, float]] = []

    if process and fuzz and q:
        extracted = process.extract(q, mapping.keys(), scorer=fuzz.WRatio, limit=None)
        scored = [(mapping[text], float(score)) for text, score, _ in extracted]
    else:
        scored = [(path, _score(q or str(path), text)) for text, path in mapping.items()]

    boosted: list[tuple[Path, float]] = []
    for path, score in scored:
        name = path.name.lower()
        full = str(path).lower()
        adjusted = score
        if q and name == q:
            adjusted += 1000.0
        elif q and full == q:
            adjusted += 900.0
        for term in spec.exact_terms:
            if _matches_exact_term(term, name, full):
                adjusted += 800.0
        boosted.append((path, adjusted))

    boosted.sort(key=lambda x: x[1], reverse=True)
    scored = boosted
    return scored[:limit]

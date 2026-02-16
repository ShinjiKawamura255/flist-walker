from __future__ import annotations

from pathlib import Path


def find_filelist(root: Path) -> Path | None:
    upper = root / "FileList.txt"
    lower = root / "filelist.txt"
    if upper.exists() and upper.is_file():
        return upper
    if lower.exists() and lower.is_file():
        return lower

    for candidate in root.iterdir():
        if candidate.is_file() and candidate.name.lower() == "filelist.txt":
            return candidate
    return None


def parse_filelist(filelist_path: Path, root: Path) -> list[Path]:
    seen: set[Path] = set()
    parsed: list[Path] = []
    for raw in filelist_path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue

        path = Path(line)
        resolved = path if path.is_absolute() else (root / path)
        resolved = resolved.resolve()
        if resolved.exists() and resolved not in seen:
            seen.add(resolved)
            parsed.append(resolved)
    return parsed


def walk_entries(root: Path) -> list[Path]:
    items: list[Path] = []
    stack: list[Path] = [root.resolve()]

    while stack:
        current = stack.pop()
        try:
            for child in current.iterdir():
                resolved = child.resolve()
                items.append(resolved)
                if child.is_dir() and not child.is_symlink():
                    stack.append(resolved)
        except PermissionError:
            continue
        except NotADirectoryError:
            continue
    return items


def build_index(root: Path) -> list[Path]:
    root = root.resolve()
    filelist = find_filelist(root)
    if filelist:
        return parse_filelist(filelist, root)
    return walk_entries(root)

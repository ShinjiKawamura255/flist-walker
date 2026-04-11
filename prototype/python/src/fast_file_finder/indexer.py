from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class IndexBuildResult:
    entries: list[Path]
    source: str
    filelist_path: Path | None = None


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


def parse_filelist(
    filelist_path: Path,
    root: Path,
    *,
    include_files: bool = True,
    include_dirs: bool = True,
) -> list[Path]:
    seen: set[Path] = set()
    parsed: list[Path] = []
    for raw in filelist_path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue

        path = Path(line)
        resolved = path if path.is_absolute() else (root / path)
        resolved = resolved.resolve()
        if not resolved.exists():
            continue
        if resolved.is_file() and not include_files:
            continue
        if resolved.is_dir() and not include_dirs:
            continue
        if resolved not in seen:
            seen.add(resolved)
            parsed.append(resolved)
    return parsed


def _walk(root: Path) -> tuple[list[Path], list[Path]]:
    files: list[Path] = []
    dirs: list[Path] = []
    stack: list[Path] = [root.resolve()]

    while stack:
        current = stack.pop()
        try:
            for child in current.iterdir():
                resolved = child.resolve()
                if child.is_dir():
                    dirs.append(resolved)
                    if not child.is_symlink():
                        stack.append(resolved)
                else:
                    files.append(resolved)
        except PermissionError:
            continue
        except NotADirectoryError:
            continue
    return files, dirs


def walk_files(root: Path) -> list[Path]:
    files, _ = _walk(root)
    return files


def walk_dirs(root: Path) -> list[Path]:
    _, dirs = _walk(root)
    return dirs


def walk_entries(root: Path, *, include_files: bool = True, include_dirs: bool = True) -> list[Path]:
    files, dirs = _walk(root)
    entries: list[Path] = []
    if include_files:
        entries.extend(files)
    if include_dirs:
        entries.extend(dirs)
    return entries


def build_index(
    root: Path,
    *,
    use_filelist: bool = True,
    include_files: bool = True,
    include_dirs: bool = True,
) -> list[Path]:
    return build_index_with_metadata(
        root,
        use_filelist=use_filelist,
        include_files=include_files,
        include_dirs=include_dirs,
    ).entries


def build_index_with_metadata(
    root: Path,
    *,
    use_filelist: bool = True,
    include_files: bool = True,
    include_dirs: bool = True,
) -> IndexBuildResult:
    root = root.resolve()
    if not include_files and not include_dirs:
        return IndexBuildResult(entries=[], source="none")

    filelist = find_filelist(root) if use_filelist else None
    if filelist is not None:
        return IndexBuildResult(
            entries=parse_filelist(
                filelist,
                root,
                include_files=include_files,
                include_dirs=include_dirs,
            ),
            source="filelist",
            filelist_path=filelist,
        )
    return IndexBuildResult(
        entries=walk_entries(root, include_files=include_files, include_dirs=include_dirs),
        source="walker",
    )


def build_filelist_text(entries: list[Path], root: Path) -> str:
    lines: list[str] = []
    seen: set[str] = set()
    root = root.resolve()
    for entry in entries:
        resolved = entry.resolve()
        try:
            line = str(resolved.relative_to(root))
        except ValueError:
            line = str(resolved)
        if line not in seen:
            seen.add(line)
            lines.append(line)
    if not lines:
        return ""
    return "\n".join(lines) + "\n"


def write_filelist(root: Path, entries: list[Path], filename: str = "FileList.txt") -> Path:
    root = root.resolve()
    target = root / filename
    text = build_filelist_text(entries, root)
    target.write_text(text, encoding="utf-8")
    return target

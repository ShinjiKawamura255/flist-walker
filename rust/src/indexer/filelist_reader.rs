use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use super::{apply_nested_filelist_overrides, filelist_modified_time};

pub fn find_filelist(root: &Path) -> Option<PathBuf> {
    let upper = root.join("FileList.txt");
    if upper.is_file() {
        return Some(upper);
    }
    let lower = root.join("filelist.txt");
    if lower.is_file() {
        return Some(lower);
    }

    fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.is_file()
                && p.file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.eq_ignore_ascii_case("filelist.txt"))
                    == Some(true)
        })
}

pub fn find_filelist_in_first_level(root: &Path) -> Option<PathBuf> {
    find_filelist(root)
}

pub fn parse_filelist(
    filelist_path: &Path,
    root: &Path,
    include_files: bool,
    include_dirs: bool,
) -> Result<Vec<PathBuf>> {
    parse_filelist_collect(filelist_path, root, include_files, include_dirs, &|| false)
}

pub fn build_entries_from_filelist_hierarchy<C>(
    filelist_path: &Path,
    root: &Path,
    include_files: bool,
    include_dirs: bool,
    should_cancel: C,
) -> Result<Vec<PathBuf>>
where
    C: Fn() -> bool,
{
    let root_modified = filelist_modified_time(filelist_path);
    let mut entries = Vec::new();
    parse_filelist_stream(
        filelist_path,
        root,
        include_files,
        include_dirs,
        &should_cancel,
        |path, _is_dir| entries.push(path),
    )?;
    apply_nested_filelist_overrides(
        filelist_path,
        root,
        root_modified,
        &mut entries,
        include_files,
        include_dirs,
        &should_cancel,
    )?;
    Ok(entries)
}

pub fn apply_filelist_hierarchy_overrides<C>(
    filelist_path: &Path,
    root: &Path,
    entries: &mut Vec<PathBuf>,
    include_files: bool,
    include_dirs: bool,
    should_cancel: C,
) -> Result<bool>
where
    C: Fn() -> bool,
{
    let root_modified = filelist_modified_time(filelist_path);
    apply_nested_filelist_overrides(
        filelist_path,
        root,
        root_modified,
        entries,
        include_files,
        include_dirs,
        &should_cancel,
    )
}

pub(super) fn parse_filelist_collect<C>(
    filelist_path: &Path,
    root: &Path,
    include_files: bool,
    include_dirs: bool,
    should_cancel: &C,
) -> Result<Vec<PathBuf>>
where
    C: Fn() -> bool,
{
    let mut out = Vec::new();
    parse_filelist_stream(
        filelist_path,
        root,
        include_files,
        include_dirs,
        should_cancel,
        |path, _is_dir| out.push(path),
    )?;
    Ok(out)
}

pub fn parse_filelist_stream<F, C>(
    filelist_path: &Path,
    root: &Path,
    include_files: bool,
    include_dirs: bool,
    should_cancel: C,
    mut on_entry: F,
) -> Result<()>
where
    F: FnMut(PathBuf, Option<bool>),
    C: Fn() -> bool,
{
    let file = File::open(filelist_path)
        .with_context(|| format!("failed to read {}", filelist_path.display()))?;
    let reader = BufReader::new(file);
    let mut seen = HashSet::new();
    let filelist_base = filelist_path.parent().unwrap_or(root);
    for line_result in reader.lines() {
        if should_cancel() {
            anyhow::bail!("superseded");
        }
        let raw =
            line_result.with_context(|| format!("failed to read {}", filelist_path.display()))?;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let candidates = resolve_filelist_entry_candidates(line, filelist_base, root);
        if include_files && include_dirs {
            // Keep FileList indexing on the current control fast path: choose the
            // platform-preferred lexical candidate and avoid per-line existence probes
            // in the initial stream.
            if let Some(path) = candidates.into_iter().next() {
                if seen.insert(path.clone()) {
                    on_entry(path, None);
                }
            }
            continue;
        }

        for candidate in candidates {
            let Ok(meta) = candidate.metadata() else {
                continue;
            };
            let is_dir = meta.is_dir();
            let is_file = meta.is_file();
            if is_file && !include_files {
                continue;
            }
            if is_dir && !include_dirs {
                continue;
            }
            if !is_file && !is_dir {
                continue;
            }
            if seen.insert(candidate.clone()) {
                on_entry(candidate, Some(is_dir));
            }
            break;
        }
    }
    Ok(())
}

pub(crate) fn resolve_filelist_entry_candidates(
    line: &str,
    filelist_base: &Path,
    root: &Path,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    let raws = preferred_filelist_raw_candidates(strip_wrapping_quotes(line));

    for raw in raws {
        if raw.is_empty() {
            continue;
        }
        let p = PathBuf::from(&raw);
        if p.is_absolute() {
            push_unique_candidate(&mut candidates, &mut seen, p.clone());
        } else if looks_like_windows_absolute_path(&raw) {
            #[cfg(windows)]
            {
                push_unique_candidate(&mut candidates, &mut seen, PathBuf::from(&raw));
            }
            #[cfg(not(windows))]
            {
                if let Some(wsl) = windows_path_to_wsl(&raw) {
                    push_unique_candidate(&mut candidates, &mut seen, wsl);
                }
            }
        }

        if !looks_like_windows_absolute_path(&raw) {
            push_unique_candidate(&mut candidates, &mut seen, filelist_base.join(&p));
            if filelist_base != root {
                push_unique_candidate(&mut candidates, &mut seen, root.join(&p));
            }
        }
    }
    candidates
}

fn preferred_filelist_raw_candidates(raw: &str) -> Vec<String> {
    if !raw.contains('\\') {
        return vec![raw.to_string()];
    }

    let normalized = raw.replace('\\', "/");
    #[cfg(windows)]
    {
        if normalized == raw {
            vec![raw.to_string()]
        } else {
            vec![raw.to_string(), normalized]
        }
    }
    #[cfg(not(windows))]
    {
        if normalized == raw {
            vec![raw.to_string()]
        } else {
            vec![normalized, raw.to_string()]
        }
    }
}

fn push_unique_candidate(
    candidates: &mut Vec<PathBuf>,
    seen: &mut HashSet<PathBuf>,
    candidate: PathBuf,
) {
    if seen.insert(candidate.clone()) {
        candidates.push(candidate);
    }
}

pub(super) fn strip_wrapping_quotes(line: &str) -> &str {
    let bytes = line.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        &line[1..line.len() - 1]
    } else {
        line
    }
}

pub(super) fn looks_like_windows_absolute_path(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    if raw.starts_with(r"\\") {
        return true;
    }
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
}

#[cfg(not(windows))]
pub(crate) fn windows_path_to_wsl(raw: &str) -> Option<PathBuf> {
    let bytes = raw.as_bytes();
    if bytes.len() < 3
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || (bytes[2] != b'\\' && bytes[2] != b'/')
    {
        return None;
    }
    let drive = (bytes[0] as char).to_ascii_lowercase();
    let rest = raw[3..].replace('\\', "/");
    Some(PathBuf::from(format!("/mnt/{drive}/{rest}")))
}

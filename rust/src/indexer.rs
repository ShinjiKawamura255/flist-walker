use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexSource {
    FileList(PathBuf),
    Walker,
    None,
}

#[derive(Debug, Clone)]
pub struct IndexBuildResult {
    pub entries: Vec<PathBuf>,
    pub source: IndexSource,
}

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
        .find(|p| p.is_file() && p.file_name().and_then(|s| s.to_str()).map(|s| s.eq_ignore_ascii_case("filelist.txt")) == Some(true))
}

pub fn parse_filelist(
    filelist_path: &Path,
    root: &Path,
    include_files: bool,
    include_dirs: bool,
) -> Result<Vec<PathBuf>> {
    let text = fs::read_to_string(filelist_path)
        .with_context(|| format!("failed to read {}", filelist_path.display()))?;
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let p = PathBuf::from(line);
        let abs = if p.is_absolute() { p } else { root.join(p) };
        let abs = abs.canonicalize().unwrap_or(abs);
        if !abs.exists() {
            continue;
        }
        if abs.is_file() && !include_files {
            continue;
        }
        if abs.is_dir() && !include_dirs {
            continue;
        }
        if seen.insert(abs.clone()) {
            out.push(abs);
        }
    }
    Ok(out)
}

fn walk(root: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut files = Vec::new();
    let mut dirs = Vec::new();

    for entry in WalkDir::new(root).follow_links(false).min_depth(1).into_iter().flatten() {
        let path = entry.path().to_path_buf();
        if entry.file_type().is_dir() {
            dirs.push(path);
        } else {
            files.push(path);
        }
    }
    (files, dirs)
}

pub fn walk_files(root: &Path) -> Vec<PathBuf> {
    walk(root).0
}

pub fn walk_dirs(root: &Path) -> Vec<PathBuf> {
    walk(root).1
}

pub fn walk_entries(root: &Path, include_files: bool, include_dirs: bool) -> Vec<PathBuf> {
    let (files, dirs) = walk(root);
    let mut out = Vec::new();
    if include_files {
        out.extend(files);
    }
    if include_dirs {
        out.extend(dirs);
    }
    out
}

pub fn build_index_with_metadata(
    root: &Path,
    use_filelist: bool,
    include_files: bool,
    include_dirs: bool,
) -> Result<IndexBuildResult> {
    if !include_files && !include_dirs {
        return Ok(IndexBuildResult {
            entries: Vec::new(),
            source: IndexSource::None,
        });
    }

    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if use_filelist {
        if let Some(filelist) = find_filelist(&root) {
            let entries = parse_filelist(&filelist, &root, include_files, include_dirs)?;
            return Ok(IndexBuildResult {
                entries,
                source: IndexSource::FileList(filelist),
            });
        }
    }

    Ok(IndexBuildResult {
        entries: walk_entries(&root, include_files, include_dirs),
        source: IndexSource::Walker,
    })
}

pub fn build_index(
    root: &Path,
    use_filelist: bool,
    include_files: bool,
    include_dirs: bool,
) -> Result<Vec<PathBuf>> {
    Ok(build_index_with_metadata(root, use_filelist, include_files, include_dirs)?.entries)
}

pub fn build_filelist_text(entries: &[PathBuf], root: &Path) -> String {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut seen = HashSet::new();
    let mut lines = Vec::new();
    for entry in entries {
        let e = entry.canonicalize().unwrap_or_else(|_| entry.clone());
        let line = e
            .strip_prefix(&root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| e.to_string_lossy().to_string());
        if seen.insert(line.clone()) {
            lines.push(line);
        }
    }
    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}

pub fn write_filelist(root: &Path, entries: &[PathBuf], filename: &str) -> Result<PathBuf> {
    let out = root.join(filename);
    let text = build_filelist_text(entries, root);
    fs::write(&out, text).with_context(|| format!("failed to write {}", out.display()))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_write_filelist() {
        let root = std::env::temp_dir().join("fff-rs-indexer");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).expect("create dir");
        fs::write(root.join("src/main.rs"), "fn main(){}" ).expect("write");

        let out = write_filelist(&root, &[root.join("src/main.rs")], "FileList.txt").expect("write filelist");
        let parsed = parse_filelist(&out, &root, true, true).expect("parse filelist");
        assert_eq!(parsed.len(), 1);
    }
}

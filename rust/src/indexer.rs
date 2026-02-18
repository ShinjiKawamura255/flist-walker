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
    let text = fs::read_to_string(filelist_path)
        .with_context(|| format!("failed to read {}", filelist_path.display()))?;
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    let filelist_base = filelist_path.parent().unwrap_or(root);

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let p = PathBuf::from(line);
        let abs = if p.is_absolute() {
            p
        } else {
            let from_filelist = filelist_base.join(&p);
            if from_filelist.exists() {
                from_filelist
            } else {
                root.join(p)
            }
        };
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

    for entry in WalkDir::new(root)
        .follow_links(false)
        .min_depth(1)
        .into_iter()
        .flatten()
    {
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
        if let Some(filelist) = find_filelist_in_first_level(&root) {
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
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("fff-rs-{name}-{nonce}"))
    }

    #[test]
    fn find_filelist_prefers_uppercase_name() {
        let root = test_root("find-upper");
        fs::create_dir_all(&root).expect("create dir");
        fs::write(root.join("FileList.txt"), "a.txt\n").expect("write upper");
        fs::write(root.join("filelist.txt"), "b.txt\n").expect("write lower");

        let found = find_filelist(&root).expect("find filelist");
        assert_eq!(
            found.file_name().and_then(|s| s.to_str()),
            Some("FileList.txt")
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn find_filelist_accepts_lowercase_name() {
        let root = test_root("find-lower");
        fs::create_dir_all(&root).expect("create dir");
        fs::write(root.join("filelist.txt"), "a.txt\n").expect("write lower");

        let found = find_filelist(&root).expect("find filelist");
        assert_eq!(
            found.file_name().and_then(|s| s.to_str()),
            Some("filelist.txt")
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn parse_filelist_resolves_relative_and_absolute_paths() {
        let root = test_root("parse");
        fs::create_dir_all(&root).expect("create dir");
        let rel_file = root.join("alpha.txt");
        let abs_file = root.join("beta.txt");
        fs::write(&rel_file, "x").expect("write rel");
        fs::write(&abs_file, "y").expect("write abs");
        let filelist = root.join("FileList.txt");
        fs::write(
            &filelist,
            format!(
                "# comment\nalpha.txt\n{}\nmissing.txt\n",
                abs_file.display()
            ),
        )
        .expect("write filelist");

        let parsed = parse_filelist(&filelist, &root, true, true).expect("parse filelist");
        assert!(parsed.contains(&rel_file));
        assert!(parsed.contains(&abs_file));
        assert_eq!(parsed.len(), 2);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn build_index_uses_filelist_when_present() {
        let root = test_root("build-filelist");
        fs::create_dir_all(&root).expect("create dir");
        let listed = root.join("listed.txt");
        let hidden = root.join("hidden.txt");
        fs::write(&listed, "ok").expect("write listed");
        fs::write(&hidden, "no").expect("write hidden");
        fs::write(root.join("FileList.txt"), "listed.txt\n").expect("write filelist");

        let out = build_index(&root, true, true, true).expect("build index");
        assert!(out.contains(&listed));
        assert!(!out.contains(&hidden));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn build_index_walks_when_filelist_missing() {
        let root = test_root("build-walker");
        let nested = root.join("dir");
        fs::create_dir_all(&nested).expect("create nested dir");
        let file = nested.join("app.py");
        fs::write(&file, "print('hi')").expect("write file");

        let out = build_index(&root, true, true, true).expect("build index");
        assert!(out.contains(&file));
        assert!(out.contains(&nested));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn build_index_with_metadata_reports_filelist_source() {
        let root = test_root("source-filelist");
        fs::create_dir_all(&root).expect("create dir");
        let listed = root.join("listed.txt");
        fs::write(&listed, "ok").expect("write listed");
        fs::write(root.join("filelist.txt"), "listed.txt\n").expect("write filelist");

        let out = build_index_with_metadata(&root, true, true, true).expect("build index");
        assert!(matches!(out.source, IndexSource::FileList(_)));
        assert!(out.entries.contains(&listed));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn build_index_with_metadata_reports_walker_source() {
        let root = test_root("source-walker");
        fs::create_dir_all(root.join("sub")).expect("create sub");

        let out = build_index_with_metadata(&root, true, true, true).expect("build index");
        assert!(matches!(out.source, IndexSource::Walker));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn walkers_are_separated_for_files_and_dirs() {
        let root = test_root("walk-separate");
        let folder = root.join("docs");
        fs::create_dir_all(&folder).expect("create folder");
        let file = folder.join("a.txt");
        fs::write(&file, "x").expect("write file");

        let files = walk_files(&root);
        let dirs = walk_dirs(&root);
        assert!(files.contains(&file));
        assert!(!files.contains(&folder));
        assert!(dirs.contains(&folder));
        assert!(!dirs.contains(&file));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn build_index_can_disable_filelist() {
        let root = test_root("disable-filelist");
        fs::create_dir_all(&root).expect("create dir");
        let listed = root.join("listed.txt");
        let extra = root.join("extra.txt");
        fs::write(&listed, "ok").expect("write listed");
        fs::write(&extra, "ok").expect("write extra");
        fs::write(root.join("FileList.txt"), "listed.txt\n").expect("write filelist");

        let out = build_index_with_metadata(&root, false, true, true).expect("build index");
        assert!(matches!(out.source, IndexSource::Walker));
        assert!(out.entries.contains(&listed));
        assert!(out.entries.contains(&extra));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn build_filelist_text_uses_relative_paths_when_possible() {
        let root = test_root("filelist-text");
        let folder = root.join("a");
        fs::create_dir_all(&folder).expect("create folder");
        let file = folder.join("b.txt");
        fs::write(&file, "x").expect("write file");

        let text = build_filelist_text(&[file.clone(), folder.clone()], &root);
        assert!(text.contains("a/b.txt"));
        assert!(text.contains("a\n"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn write_filelist_writes_file() {
        let root = test_root("write-filelist");
        let folder = root.join("x");
        fs::create_dir_all(&folder).expect("create folder");
        let file = folder.join("run.exe");
        fs::write(&file, "bin").expect("write file");

        let out =
            write_filelist(&root, &[file.clone(), folder.clone()], "FileList.txt").expect("write");
        assert!(out.exists());
        let content = fs::read_to_string(&out).expect("read filelist");
        assert!(content.contains("x/run.exe"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn parse_and_write_filelist() {
        let root = test_root("parse-write");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).expect("create dir");
        fs::write(root.join("src/main.rs"), "fn main(){}").expect("write");

        let out = write_filelist(&root, &[root.join("src/main.rs")], "FileList.txt")
            .expect("write filelist");
        let parsed = parse_filelist(&out, &root, true, true).expect("parse filelist");
        assert_eq!(parsed.len(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn find_filelist_in_first_level_only_checks_root() {
        let root = test_root("find-first-level");
        let child = root.join("child");
        fs::create_dir_all(&child).expect("create child");
        fs::write(child.join("filelist.txt"), "a.txt\n").expect("write filelist");

        let found = find_filelist_in_first_level(&root);
        assert!(found.is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn build_index_ignores_nested_filelist_and_uses_walker() {
        let root = test_root("nested-filelist-ignored");
        let child = root.join("child");
        fs::create_dir_all(&child).expect("create child");
        let nested = child.join("nested.txt");
        fs::write(&nested, "x").expect("write nested");
        fs::write(child.join("filelist.txt"), "nested.txt\n").expect("write filelist");

        let out = build_index_with_metadata(&root, true, true, false).expect("build index");
        assert_eq!(out.source, IndexSource::Walker);
        assert!(out.entries.contains(&nested));
        let _ = fs::remove_dir_all(&root);
    }
}

mod filelist_reader;
mod filelist_writer;
mod walker;

use crate::entry::Entry;
use anyhow::Result;
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};
use tracing::info;

pub use filelist_reader::{
    apply_filelist_hierarchy_overrides, build_entries_from_filelist_hierarchy, find_filelist,
    find_filelist_in_first_level, parse_filelist, parse_filelist_stream,
};
pub use filelist_writer::{
    build_filelist_text, build_filelist_text_cancellable, has_ancestor_filelists, write_filelist,
    write_filelist_cancellable,
};
pub use walker::{walk_dirs, walk_entries, walk_files};

use filelist_reader::parse_filelist_collect;
use filelist_writer::filelist_modified_time;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexSource {
    FileList(PathBuf),
    Walker,
    None,
}

#[derive(Debug, Clone)]
pub struct IndexBuildResult {
    pub entries: Vec<Entry>,
    pub source: IndexSource,
}

pub fn build_index_with_metadata(
    root: &Path,
    use_filelist: bool,
    include_files: bool,
    include_dirs: bool,
) -> Result<IndexBuildResult> {
    let started_at = Instant::now();
    if !include_files && !include_dirs {
        return Ok(IndexBuildResult {
            entries: Vec::new(),
            source: IndexSource::None,
        });
    }

    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let result = if use_filelist {
        if let Some(filelist) = find_filelist_in_first_level(&root) {
            let entries = build_entries_from_filelist_hierarchy(
                &filelist,
                &root,
                include_files,
                include_dirs,
                || false,
            )?;
            IndexBuildResult {
                entries: entries.into_iter().map(Entry::from).collect(),
                source: IndexSource::FileList(filelist),
            }
        } else {
            IndexBuildResult {
                entries: walk_entries(&root, include_files, include_dirs)
                    .into_iter()
                    .map(Entry::from)
                    .collect(),
                source: IndexSource::Walker,
            }
        }
    } else {
        IndexBuildResult {
            entries: walk_entries(&root, include_files, include_dirs)
                .into_iter()
                .map(Entry::from)
                .collect(),
            source: IndexSource::Walker,
        }
    };
    info!(
        root = %root.display(),
        use_filelist,
        include_files,
        include_dirs,
        entry_count = result.entries.len(),
        source = ?result.source,
        elapsed_ms = started_at.elapsed().as_millis(),
        "index build completed"
    );
    Ok(result)
}

pub fn build_index(
    root: &Path,
    use_filelist: bool,
    include_files: bool,
    include_dirs: bool,
) -> Result<Vec<PathBuf>> {
    Ok(
        build_index_with_metadata(root, use_filelist, include_files, include_dirs)?
            .entries
            .into_iter()
            .map(|entry| entry.path)
            .collect(),
    )
}

fn apply_nested_filelist_overrides<C>(
    root_filelist: &Path,
    root: &Path,
    root_modified: Option<SystemTime>,
    entries: &mut Vec<PathBuf>,
    include_files: bool,
    include_dirs: bool,
    should_cancel: &C,
) -> Result<bool>
where
    C: Fn() -> bool,
{
    type PendingFileList = (Reverse<usize>, u64, PathBuf);

    let mut changed = false;
    let mut active_filelist_modified: HashMap<PathBuf, Option<SystemTime>> = HashMap::new();
    active_filelist_modified.insert(root.to_path_buf(), root_modified);

    let mut discovered = HashSet::new();
    let mut pending: std::collections::BinaryHeap<PendingFileList> =
        std::collections::BinaryHeap::new();
    let mut pending_seq = 0u64;
    enqueue_nested_filelists_from_entries(
        entries,
        root_filelist,
        root,
        &mut discovered,
        &mut pending,
        &mut pending_seq,
    );

    while let Some((_depth, _seq, child_filelist)) = pending.pop() {
        if should_cancel() {
            anyhow::bail!("superseded");
        }
        let Some(child_root) = child_filelist.parent().map(Path::to_path_buf) else {
            continue;
        };
        let active_modified =
            nearest_active_modified(&child_root, root, &active_filelist_modified).flatten();
        let child_modified = filelist_modified_time(&child_filelist);
        if !is_filelist_newer(child_modified, active_modified) {
            continue;
        }
        let child_entries = parse_filelist_collect(
            &child_filelist,
            root,
            include_files,
            include_dirs,
            should_cancel,
        )?;
        enqueue_nested_filelists_from_entries(
            &child_entries,
            root_filelist,
            root,
            &mut discovered,
            &mut pending,
            &mut pending_seq,
        );
        if replace_entries_in_subtree(entries, &child_root, child_entries) {
            changed = true;
        }
        active_filelist_modified.insert(child_root, child_modified);
    }
    Ok(changed)
}

fn enqueue_nested_filelists_from_entries(
    entries: &[PathBuf],
    root_filelist: &Path,
    root: &Path,
    discovered: &mut HashSet<PathBuf>,
    pending: &mut std::collections::BinaryHeap<(Reverse<usize>, u64, PathBuf)>,
    pending_seq: &mut u64,
) {
    for path in entries {
        if path == root_filelist {
            continue;
        }
        if !path.starts_with(root) {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if !name.eq_ignore_ascii_case("filelist.txt") {
            continue;
        }
        if !path.is_file() {
            continue;
        }
        if discovered.insert(path.clone()) {
            let depth = path_depth_from_root(path, root).unwrap_or(usize::MAX);
            pending.push((Reverse(depth), *pending_seq, path.clone()));
            *pending_seq = pending_seq.saturating_add(1);
        }
    }
}

fn path_depth_from_root(path: &Path, root: &Path) -> Option<usize> {
    path.parent()
        .and_then(|parent| parent.strip_prefix(root).ok())
        .map(|rel| rel.components().count())
}

fn nearest_active_modified(
    subtree_root: &Path,
    root: &Path,
    active_filelist_modified: &HashMap<PathBuf, Option<SystemTime>>,
) -> Option<Option<SystemTime>> {
    let mut current = Some(subtree_root);
    while let Some(path) = current {
        if let Some(found) = active_filelist_modified.get(path) {
            return Some(*found);
        }
        if path == root {
            break;
        }
        current = path.parent();
    }
    None
}

fn replace_entries_in_subtree(
    entries: &mut Vec<PathBuf>,
    subtree_root: &Path,
    replacements: Vec<PathBuf>,
) -> bool {
    let before_len = entries.len();
    entries.retain(|path| !is_path_in_subtree(path, subtree_root));
    let mut existing: HashSet<PathBuf> = entries.iter().cloned().collect();
    let mut inserted = 0usize;
    for replacement in replacements {
        if existing.insert(replacement.clone()) {
            entries.push(replacement);
            inserted = inserted.saturating_add(1);
        }
    }
    before_len != entries.len() || inserted > 0
}

fn is_path_in_subtree(path: &Path, subtree_root: &Path) -> bool {
    path == subtree_root || path.starts_with(subtree_root)
}

fn is_filelist_newer(candidate: Option<SystemTime>, baseline: Option<SystemTime>) -> bool {
    match (candidate, baseline) {
        (Some(lhs), Some(rhs)) => lhs > rhs,
        (Some(_), None) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::filelist_reader::resolve_filelist_entry_candidates;
    #[cfg(not(windows))]
    use super::filelist_reader::windows_path_to_wsl;
    use super::filelist_writer::{
        annotate_write_target_error, normalize_filelist_entry_for_text_compare,
        visit_ancestor_directories,
    };
    use super::*;
    use anyhow::Context;
    use std::fs;
    use std::fs::File;
    use std::io::{BufRead, BufReader};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("fff-rs-{name}-{nonce}"))
    }

    fn sleep_for_timestamp_tick() {
        std::thread::sleep(Duration::from_millis(1100));
    }

    fn canonical_or_original(path: &Path) -> PathBuf {
        path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
    }

    fn same_path(left: &Path, right: &Path) -> bool {
        if left == right {
            return true;
        }
        canonical_or_original(left) == canonical_or_original(right)
    }

    fn contains_path<T: AsRef<Path>>(entries: &[T], expected: &Path) -> bool {
        entries
            .iter()
            .any(|entry| same_path(entry.as_ref(), expected))
    }

    fn expected_backslash_relative_path(root: &Path) -> PathBuf {
        #[cfg(windows)]
        {
            root.join(r"nested\item.txt")
        }
        #[cfg(not(windows))]
        {
            root.join("nested/item.txt")
        }
    }

    fn assert_filelist_source_matches(source: &IndexSource, expected: &Path) {
        match source {
            IndexSource::FileList(actual) => {
                assert!(
                    same_path(actual, expected),
                    "unexpected FileList source: actual={} expected={}",
                    actual.display(),
                    expected.display()
                );
            }
            other => panic!("expected FileList source, got {other:?}"),
        }
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
        assert!(same_path(&found, &root.join("filelist.txt")));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn parse_filelist_resolves_relative_and_absolute_paths() {
        let root = test_root("parse");
        fs::create_dir_all(&root).expect("create dir");
        let rel_file = root.join("alpha.txt");
        let abs_file = root.join("beta.txt");
        let missing = root.join("missing.txt");
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
        assert!(parsed.contains(&missing));
        assert_eq!(parsed.len(), 3);
        let _ = fs::remove_dir_all(&root);
    }

    fn parse_filelist_stream_metadata_probe_count(
        filelist_path: &Path,
        root: &Path,
    ) -> Result<usize> {
        // This models the probe-heavy path we want to keep out of the fast stream.
        let file = File::open(filelist_path)
            .with_context(|| format!("failed to read {}", filelist_path.display()))?;
        let reader = BufReader::new(file);
        let filelist_base = filelist_path.parent().unwrap_or(root);
        let mut seen = HashSet::new();
        let mut count = 0usize;

        for line_result in reader.lines() {
            let raw = line_result
                .with_context(|| format!("failed to read {}", filelist_path.display()))?;
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let candidates = resolve_filelist_entry_candidates(line, filelist_base, root);
            for candidate in candidates {
                let Ok(meta) = candidate.metadata() else {
                    continue;
                };
                if !meta.is_file() && !meta.is_dir() {
                    continue;
                }
                if seen.insert(candidate) {
                    count = count.saturating_add(1);
                }
                break;
            }
        }
        Ok(count)
    }

    #[test]
    #[ignore = "perf measurement; run explicitly"]
    fn perf_filelist_stream_is_faster_than_metadata_probe_baseline() {
        let root = test_root("perf-filelist-metadata-probe-baseline");
        fs::create_dir_all(&root).expect("create dir");
        let filelist = root.join("FileList.txt");
        let data_dir = root.join("dataset");
        fs::create_dir_all(&data_dir).expect("create dataset dir");
        let lines = 30_000usize;
        let mut text = String::with_capacity(lines * 24);
        for i in 0..lines {
            let rel = format!("dataset\\f-{i}.txt");
            fs::write(root.join(&rel), "x").expect("write synthetic file");
            text.push_str(&rel);
            text.push('\n');
        }
        fs::write(&filelist, text).expect("write synthetic filelist");

        let iterations = 5usize;
        let mut probe_baseline_best = Duration::MAX;
        let mut current_best = Duration::MAX;
        let mut probe_baseline_count = 0usize;
        let mut current_count = 0usize;

        for _ in 0..iterations {
            let probe_baseline_start = Instant::now();
            probe_baseline_count = parse_filelist_stream_metadata_probe_count(&filelist, &root)
                .expect("metadata probe baseline parse");
            probe_baseline_best = probe_baseline_best.min(probe_baseline_start.elapsed());

            let current_start = Instant::now();
            current_count = 0usize;
            parse_filelist_stream(
                &filelist,
                &root,
                true,
                true,
                || false,
                |_path, _is_dir| {
                    current_count = current_count.saturating_add(1);
                },
            )
            .expect("current parse");
            current_best = current_best.min(current_start.elapsed());
        }

        let probe_baseline_ms = probe_baseline_best.as_secs_f64() * 1000.0;
        let current_ms = current_best.as_secs_f64() * 1000.0;
        let speedup = if current_ms > 0.0 {
            probe_baseline_ms / current_ms
        } else {
            f64::INFINITY
        };

        eprintln!(
            "FileList perf control_baseline lines={lines} metadata_probe_ms={probe_baseline_ms:.3} current_ms={current_ms:.3} speedup={speedup:.2}x metadata_probe_count={probe_baseline_count} current_count={current_count}"
        );

        assert_eq!(probe_baseline_count, lines);
        assert_eq!(current_count, lines);
        assert!(
            speedup >= 1.30,
            "line-only FileList parse did not beat the metadata-probe control baseline enough: {speedup:.2}x"
        );
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
        assert!(contains_path(&out, &listed));
        assert!(!contains_path(&out, &hidden));
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
        assert!(contains_path(&out, &file));
        assert!(contains_path(&out, &nested));
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
        assert_filelist_source_matches(&out.source, &root.join("filelist.txt"));
        assert!(contains_path(&out.entries, &listed));
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
        assert!(contains_path(&out.entries, &listed));
        assert!(contains_path(&out.entries, &extra));
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
        assert!(text.contains(&format!("a{}b.txt", std::path::MAIN_SEPARATOR)));
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

        let out = write_filelist(&root, &[file.clone(), folder.clone()], "FileList.txt", true)
            .expect("write");
        assert!(out.exists());
        let content = fs::read_to_string(&out).expect("read filelist");
        assert!(content.contains(&format!("x{}run.exe", std::path::MAIN_SEPARATOR)));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn write_filelist_appends_child_filelist_to_ancestor_filelists_without_touching_mtime() {
        let top = test_root("write-filelist-propagate");
        let parent = top.join("parent");
        let root = parent.join("child");
        let sibling = parent.join("keep.txt");
        fs::create_dir_all(&root).expect("create child root");
        fs::write(&sibling, "keep").expect("write sibling");
        let parent_filelist = parent.join("FileList.txt");
        fs::write(&parent_filelist, "keep.txt\n").expect("write parent filelist");
        sleep_for_timestamp_tick();
        let before_modified = filelist_modified_time(&parent_filelist).expect("mtime before");

        let child_entry = root.join("src/main.rs");
        fs::create_dir_all(child_entry.parent().expect("child parent")).expect("create src");
        fs::write(&child_entry, "fn main() {}").expect("write child entry");

        let out = write_filelist(&root, &[child_entry], "FileList.txt", true).expect("write child");

        let parent_content = fs::read_to_string(&parent_filelist).expect("read parent filelist");
        assert!(parent_content.contains("keep.txt"));
        assert!(parent_content.contains(&format!("child{}FileList.txt", std::path::MAIN_SEPARATOR)));
        let after_modified = filelist_modified_time(&parent_filelist).expect("mtime after");
        assert_eq!(after_modified, before_modified);

        let parsed_parent =
            parse_filelist(&parent_filelist, &parent, true, true).expect("parse parent filelist");
        assert!(contains_path(&parsed_parent, &out));
        let _ = fs::remove_dir_all(&top);
    }

    #[test]
    fn write_filelist_does_not_duplicate_existing_ancestor_child_reference() {
        let top = test_root("write-filelist-propagate-dedup");
        let parent = top.join("parent");
        let root = parent.join("child");
        fs::create_dir_all(&root).expect("create child root");
        let child_entry = root.join("src/main.rs");
        fs::create_dir_all(child_entry.parent().expect("child parent")).expect("create src");
        fs::write(&child_entry, "fn main() {}").expect("write child entry");
        let child_filelist = root.join("FileList.txt");
        let parent_filelist = parent.join("FileList.txt");
        fs::create_dir_all(&parent).expect("create parent");
        fs::write(
            &parent_filelist,
            format!("./child{}FileList.txt\n", std::path::MAIN_SEPARATOR),
        )
        .expect("write parent filelist");

        write_filelist(&root, &[child_entry], "FileList.txt", true).expect("write child");

        let parent_content = fs::read_to_string(&parent_filelist).expect("read parent filelist");
        assert_eq!(
            parent_content
                .lines()
                .filter(|line| line.contains("child") && line.contains("FileList.txt"))
                .count(),
            1
        );
        let parsed_parent =
            parse_filelist(&parent_filelist, &parent, true, true).expect("parse parent filelist");
        assert!(contains_path(&parsed_parent, &child_filelist));
        let _ = fs::remove_dir_all(&top);
    }

    #[test]
    fn normalize_filelist_entry_for_text_compare_collapses_relative_variants() {
        assert_eq!(
            normalize_filelist_entry_for_text_compare("./child\\FileList.txt"),
            Some("child/FileList.txt".to_string())
        );
        assert_eq!(
            normalize_filelist_entry_for_text_compare("\"child/FileList.txt\""),
            Some("child/FileList.txt".to_string())
        );
        assert_eq!(
            normalize_filelist_entry_for_text_compare("child/./nested/../FileList.txt"),
            Some("child/FileList.txt".to_string())
        );
    }

    #[test]
    fn visit_ancestor_directories_stops_when_callback_requests_break() {
        let path = PathBuf::from("/tmp/flistwalker/a/b/c");
        let mut visited = Vec::new();

        visit_ancestor_directories(path.as_path(), |ancestor| {
            visited.push(ancestor.to_path_buf());
            ancestor != Path::new("/tmp/flistwalker/a")
        });

        assert_eq!(
            visited,
            vec![
                PathBuf::from("/tmp/flistwalker/a/b"),
                PathBuf::from("/tmp/flistwalker/a")
            ]
        );
    }

    #[test]
    fn annotate_write_target_error_adds_permission_hint() {
        let path = PathBuf::from(r"C:\FileList.txt");
        let err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "os error 5");
        let msg = annotate_write_target_error(&path, err).to_string();
        assert!(msg.contains("permission denied while writing"));
        assert!(msg.contains(r"C:\FileList.txt"));
        assert!(msg.contains("UAC"));
    }

    #[test]
    fn build_filelist_text_keeps_lexical_relative_for_missing_entry() {
        let root = test_root("filelist-text-missing");
        fs::create_dir_all(&root).expect("create dir");
        let missing = root.join("missing.txt");

        let text = build_filelist_text(&[missing], &root);

        assert_eq!(text, "missing.txt\n");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn build_filelist_text_deduplicates_lexically_equivalent_relative_paths() {
        let root = test_root("filelist-text-lexical-dedup");
        fs::create_dir_all(root.join("a")).expect("create a dir");
        fs::write(root.join("b.txt"), "x").expect("write b");

        let p1 = root.join("a").join("..").join("b.txt");
        let p2 = root.join("b.txt");
        let text = build_filelist_text(&[p1, p2], &root);

        assert_eq!(text, "b.txt\n");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn parse_and_write_filelist() {
        let root = test_root("parse-write");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).expect("create dir");
        fs::write(root.join("src/main.rs"), "fn main(){}").expect("write");

        let out = write_filelist(&root, &[root.join("src/main.rs")], "FileList.txt", true)
            .expect("write filelist");
        let parsed = parse_filelist(&out, &root, true, true).expect("parse filelist");
        assert_eq!(parsed.len(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn write_filelist_cancellable_stops_before_replacing_output() {
        let root = test_root("write-filelist-cancel");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create dir");
        let existing = root.join("FileList.txt");
        fs::write(&existing, "old.txt\n").expect("write existing filelist");

        let err = write_filelist_cancellable(
            &root,
            &[root.join("src/main.rs")],
            "FileList.txt",
            false,
            &|| true,
        )
        .expect_err("canceled write should fail");

        assert!(err.to_string().contains("canceled"));
        let content = fs::read_to_string(&existing).expect("read existing filelist");
        assert_eq!(content, "old.txt\n");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn parse_filelist_stream_can_be_canceled() {
        let root = test_root("parse-stream-cancel");
        fs::create_dir_all(&root).expect("create dir");
        let file = root.join("a.txt");
        fs::write(&file, "x").expect("write file");
        let filelist = root.join("FileList.txt");
        fs::write(&filelist, "a.txt\n").expect("write filelist");

        let mut visited = 0usize;
        let err = parse_filelist_stream(
            &filelist,
            &root,
            true,
            true,
            || true,
            |_path, _is_dir| {
                visited = visited.saturating_add(1);
            },
        )
        .expect_err("canceled parse should fail");

        assert_eq!(visited, 0);
        assert!(err.to_string().contains("superseded"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn parse_filelist_accepts_backslash_relative_path() {
        let root = test_root("parse-backslash");
        let nested = root.join("nested");
        fs::create_dir_all(&nested).expect("create dir");
        let file = nested.join("item.txt");
        fs::write(&file, "x").expect("write file");
        let filelist = root.join("FileList.txt");
        fs::write(&filelist, "nested\\item.txt\n").expect("write filelist");

        let parsed = parse_filelist(&filelist, &root, true, false).expect("parse filelist");
        assert_eq!(parsed, vec![file]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn parse_filelist_accepts_quoted_path() {
        let root = test_root("parse-quoted");
        fs::create_dir_all(&root).expect("create dir");
        let file = root.join("quoted.txt");
        fs::write(&file, "x").expect("write file");
        let filelist = root.join("FileList.txt");
        fs::write(&filelist, "\"quoted.txt\"\n").expect("write filelist");

        let parsed = parse_filelist(&filelist, &root, true, false).expect("parse filelist");
        assert_eq!(parsed, vec![file]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn parse_filelist_stream_returns_unknown_kind_when_both_types_are_enabled() {
        let root = test_root("parse-stream-kind-unknown");
        fs::create_dir_all(&root).expect("create dir");
        let file = root.join("a.txt");
        fs::write(&file, "x").expect("write file");
        let filelist = root.join("FileList.txt");
        fs::write(&filelist, "a.txt\n").expect("write filelist");

        let mut kinds = Vec::new();
        parse_filelist_stream(
            &filelist,
            &root,
            true,
            true,
            || false,
            |_path, is_dir| {
                kinds.push(is_dir);
            },
        )
        .expect("parse filelist");

        assert_eq!(kinds, vec![None]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn regression_parse_filelist_stream_prefers_platform_candidate_without_exists_probe() {
        let root = test_root("parse-stream-platform-candidate");
        fs::create_dir_all(root.join("nested")).expect("create dir");
        let filelist = root.join("FileList.txt");
        fs::write(&filelist, "nested\\item.txt\n").expect("write filelist");

        let mut entries = Vec::new();
        parse_filelist_stream(
            &filelist,
            &root,
            true,
            true,
            || false,
            |path, is_dir| {
                entries.push((path, is_dir));
            },
        )
        .expect("parse filelist");

        assert_eq!(
            entries,
            vec![(expected_backslash_relative_path(&root), None)]
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn parse_filelist_stream_returns_known_kind_when_filter_requires_type() {
        let root = test_root("parse-stream-kind-known");
        fs::create_dir_all(root.join("d")).expect("create dir");
        let file = root.join("a.txt");
        fs::write(&file, "x").expect("write file");
        let filelist = root.join("FileList.txt");
        fs::write(&filelist, "a.txt\nd\n").expect("write filelist");

        let mut entries = Vec::new();
        parse_filelist_stream(
            &filelist,
            &root,
            false,
            true,
            || false,
            |path, is_dir| {
                entries.push((path, is_dir));
            },
        )
        .expect("parse filelist");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, root.join("d"));
        assert_eq!(entries[0].1, Some(true));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    #[cfg(not(windows))]
    fn windows_path_to_wsl_converts_drive_path() {
        let converted = windows_path_to_wsl(r"C:\Users\alice\work\file.txt");
        assert_eq!(
            converted,
            Some(PathBuf::from("/mnt/c/Users/alice/work/file.txt"))
        );
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
    fn build_index_overrides_subtree_with_newer_nested_filelist() {
        let root = test_root("nested-filelist-newer");
        let child = root.join("child");
        fs::create_dir_all(&child).expect("create child");
        let keep = root.join("keep.txt");
        let child_old = child.join("old.txt");
        let child_new = child.join("new.txt");
        fs::write(&keep, "x").expect("write keep");
        fs::write(&child_old, "x").expect("write old");
        fs::write(&child_new, "x").expect("write new");
        fs::write(
            root.join("FileList.txt"),
            "keep.txt\nchild\nchild/old.txt\nchild/filelist.txt\n",
        )
        .expect("write root filelist");
        sleep_for_timestamp_tick();
        fs::write(child.join("filelist.txt"), "new.txt\n").expect("write child filelist");

        let out = build_index_with_metadata(&root, true, true, true).expect("build index");
        assert_filelist_source_matches(&out.source, &root.join("FileList.txt"));
        assert!(contains_path(&out.entries, &keep));
        assert!(contains_path(&out.entries, &child_new));
        assert!(!contains_path(&out.entries, &child_old));
        assert!(!contains_path(&out.entries, &child));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn build_index_ignores_older_nested_filelist() {
        let root = test_root("nested-filelist-older");
        let child = root.join("child");
        fs::create_dir_all(&child).expect("create child");
        let child_old = child.join("old.txt");
        let child_new = child.join("new.txt");
        fs::write(&child_old, "x").expect("write old");
        fs::write(&child_new, "x").expect("write new");
        fs::write(child.join("filelist.txt"), "new.txt\n").expect("write child filelist");
        sleep_for_timestamp_tick();
        fs::write(root.join("FileList.txt"), "child/old.txt\n").expect("write root filelist");

        let out = build_index_with_metadata(&root, true, true, false).expect("build index");
        assert_filelist_source_matches(&out.source, &root.join("FileList.txt"));
        assert!(contains_path(&out.entries, &child_old));
        assert!(!contains_path(&out.entries, &child_new));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn build_index_applies_newest_filelist_per_depth() {
        let root = test_root("nested-filelist-depth");
        let child = root.join("child");
        let grand = child.join("grand");
        fs::create_dir_all(&grand).expect("create dirs");
        let top = root.join("top.txt");
        let root_only = child.join("child_from_root.txt");
        let child_only = child.join("child_from_child.txt");
        let grand_child = grand.join("grand_from_child.txt");
        let grand_only = grand.join("grand_from_grand.txt");
        for file in [&top, &root_only, &child_only, &grand_child, &grand_only] {
            fs::write(file, "x").expect("write file");
        }

        fs::write(
            root.join("FileList.txt"),
            "top.txt\nchild/child_from_root.txt\nchild/grand/grand_from_root.txt\nchild/filelist.txt\n",
        )
        .expect("write root filelist");
        sleep_for_timestamp_tick();
        fs::write(
            child.join("filelist.txt"),
            "child_from_child.txt\ngrand/grand_from_child.txt\ngrand/filelist.txt\n",
        )
        .expect("write child filelist");
        sleep_for_timestamp_tick();
        fs::write(grand.join("filelist.txt"), "grand_from_grand.txt\n")
            .expect("write grand filelist");

        let out = build_index_with_metadata(&root, true, true, false).expect("build index");
        assert!(contains_path(&out.entries, &top));
        assert!(contains_path(&out.entries, &child_only));
        assert!(contains_path(&out.entries, &grand_only));
        assert!(!contains_path(&out.entries, &root_only));
        assert!(!contains_path(&out.entries, &grand_child));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn apply_overrides_can_cancel_during_nested_filelist_parse() {
        let root = test_root("nested-filelist-cancel");
        let child = root.join("child");
        fs::create_dir_all(&child).expect("create child");
        fs::write(child.join("a.txt"), "x").expect("write child file");
        fs::write(child.join("filelist.txt"), "a.txt\n").expect("write child filelist");

        let mut entries = vec![child.join("filelist.txt")];
        let err = apply_filelist_hierarchy_overrides(
            &root.join("FileList.txt"),
            &root,
            &mut entries,
            true,
            true,
            || true,
        )
        .expect_err("override should be cancelable");

        assert!(err.to_string().contains("superseded"));
        let _ = fs::remove_dir_all(&root);
    }
}

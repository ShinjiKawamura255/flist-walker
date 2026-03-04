use anyhow::{Context, Result};
use jwalk::WalkDir;
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

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

fn parse_filelist_collect<C>(
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

fn resolve_filelist_entry_candidates(
    line: &str,
    filelist_base: &Path,
    root: &Path,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    let mut raws = vec![strip_wrapping_quotes(line).to_string()];
    if raws[0].contains('\\') {
        raws.push(raws[0].replace('\\', "/"));
    }

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

fn push_unique_candidate(
    candidates: &mut Vec<PathBuf>,
    seen: &mut HashSet<PathBuf>,
    candidate: PathBuf,
) {
    if seen.insert(candidate.clone()) {
        candidates.push(candidate);
    }
}

fn strip_wrapping_quotes(line: &str) -> &str {
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

fn looks_like_windows_absolute_path(raw: &str) -> bool {
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
fn windows_path_to_wsl(raw: &str) -> Option<PathBuf> {
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
            let entries = build_entries_from_filelist_hierarchy(
                &filelist,
                &root,
                include_files,
                include_dirs,
                || false,
            )?;
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
    let root_lexical = root.to_path_buf();
    let root_canonical = root.canonicalize().ok();
    let mut seen = HashSet::new();
    let mut lines = Vec::new();
    for entry in entries {
        let line = filelist_line_for_entry(entry, &root_lexical, root_canonical.as_deref());
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

fn filelist_line_for_entry(
    entry: &Path,
    root_lexical: &Path,
    root_canonical: Option<&Path>,
) -> String {
    // Fast path: lexical prefix stripping avoids filesystem calls for the common case.
    if let Ok(relative) = entry.strip_prefix(root_lexical) {
        return normalize_relative_lexically(relative)
            .to_string_lossy()
            .to_string();
    }
    if let Some(root) = root_canonical {
        if let Ok(relative) = entry.strip_prefix(root) {
            return normalize_relative_lexically(relative)
                .to_string_lossy()
                .to_string();
        }
    }

    // Compatibility path: canonicalize only when lexical checks failed.
    if let Some(root) = root_canonical {
        if let Ok(canonical_entry) = entry.canonicalize() {
            if let Ok(relative) = canonical_entry.strip_prefix(root) {
                return relative.to_string_lossy().to_string();
            }
            return canonical_entry.to_string_lossy().to_string();
        }
    }

    entry.to_string_lossy().to_string()
}

fn normalize_relative_lexically(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                let popped = out.pop();
                if !popped {
                    out.push(component.as_os_str());
                }
            }
            Component::Normal(segment) => out.push(segment),
            Component::RootDir | Component::Prefix(_) => out.push(component.as_os_str()),
        }
    }
    out
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

fn filelist_modified_time(path: &Path) -> Option<SystemTime> {
    fs::metadata(path)
        .ok()
        .and_then(|meta| meta.modified().ok())
}

fn is_filelist_newer(candidate: Option<SystemTime>, baseline: Option<SystemTime>) -> bool {
    match (candidate, baseline) {
        (Some(lhs), Some(rhs)) => lhs > rhs,
        (Some(_), None) => true,
        _ => false,
    }
}

fn build_temp_filelist_path(filename: &str) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let seq = TMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir()
        .join("flistwalker")
        .join(format!("{filename}.{pid}.{now}.{seq}.tmp"))
}

fn annotate_write_target_error(out: &Path, err: std::io::Error) -> anyhow::Error {
    if err.kind() == std::io::ErrorKind::PermissionDenied {
        return anyhow::anyhow!(
            "permission denied while writing {}. destination directory may be protected (for example C:\\ root/UAC), existing FileList.txt may be read-only, or another process may be locking the file. original error: {}",
            out.display(),
            err
        );
    }
    anyhow::Error::new(err)
}

pub fn write_filelist(root: &Path, entries: &[PathBuf], filename: &str) -> Result<PathBuf> {
    let out = root.join(filename);
    let text = build_filelist_text(entries, root);

    let tmp = build_temp_filelist_path(filename);
    if let Some(parent) = tmp.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create temp dir {}", parent.display()))?;
    }
    fs::write(&tmp, &text).with_context(|| format!("failed to write {}", tmp.display()))?;

    if out.exists() {
        fs::remove_file(&out)
            .map_err(|err| annotate_write_target_error(&out, err))
            .with_context(|| format!("failed to replace {}", out.display()))?;
    }
    if let Err(rename_err) = fs::rename(&tmp, &out) {
        fs::copy(&tmp, &out)
            .map_err(|err| annotate_write_target_error(&out, err))
            .with_context(|| {
                format!(
                    "failed to place {} from temp file {} ({})",
                    out.display(),
                    tmp.display(),
                    rename_err
                )
            })?;
        let _ = fs::remove_file(&tmp);
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn parse_filelist_stream_strict_exists_both_types(
        filelist_path: &Path,
        root: &Path,
    ) -> Result<usize> {
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
            let mut selected = None;
            for candidate in candidates {
                if candidate.try_exists().unwrap_or(false) {
                    selected = Some(candidate);
                    break;
                }
            }
            if let Some(path) = selected {
                if seen.insert(path) {
                    count = count.saturating_add(1);
                }
            }
        }
        Ok(count)
    }

    #[test]
    #[ignore]
    fn perf_parse_filelist_ab_strict_vs_fast() {
        let root = test_root("perf-parse-ab");
        fs::create_dir_all(&root).expect("create dir");
        let filelist = root.join("FileList.txt");
        let data_dir = root.join("dataset");
        fs::create_dir_all(&data_dir).expect("create dataset dir");
        let lines = 30_000usize;
        let mut text = String::with_capacity(lines * 24);
        for i in 0..lines {
            let rel = format!("dataset/f-{i}.txt");
            fs::write(root.join(&rel), "x").expect("write synthetic file");
            text.push_str(&rel);
            text.push('\n');
        }
        fs::write(&filelist, text).expect("write synthetic filelist");

        let strict_start = Instant::now();
        let strict_count =
            parse_filelist_stream_strict_exists_both_types(&filelist, &root).expect("strict parse");
        let strict_elapsed = strict_start.elapsed();

        let fast_start = Instant::now();
        let mut fast_count = 0usize;
        parse_filelist_stream(
            &filelist,
            &root,
            true,
            true,
            || false,
            |_path, _is_dir| {
                fast_count = fast_count.saturating_add(1);
            },
        )
        .expect("fast parse");
        let fast_elapsed = fast_start.elapsed();

        let strict_ms = strict_elapsed.as_secs_f64() * 1000.0;
        let fast_ms = fast_elapsed.as_secs_f64() * 1000.0;
        let speedup = if fast_ms > 0.0 {
            strict_ms / fast_ms
        } else {
            f64::INFINITY
        };

        eprintln!(
            "A/B parse_filelist (lines={lines}) strict_exists_ms={strict_ms:.3} fast_line_only_ms={fast_ms:.3} speedup={speedup:.2}x strict_count={strict_count} fast_count={fast_count}"
        );

        assert_eq!(strict_count, lines);
        assert_eq!(fast_count, lines);
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

        let out = write_filelist(&root, &[root.join("src/main.rs")], "FileList.txt")
            .expect("write filelist");
        let parsed = parse_filelist(&out, &root, true, true).expect("parse filelist");
        assert_eq!(parsed.len(), 1);
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
        assert_eq!(out.source, IndexSource::FileList(root.join("FileList.txt")));
        assert!(out.entries.contains(&keep));
        assert!(out.entries.contains(&child_new));
        assert!(!out.entries.contains(&child_old));
        assert!(!out.entries.contains(&child));
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
        assert_eq!(out.source, IndexSource::FileList(root.join("FileList.txt")));
        assert!(out.entries.contains(&child_old));
        assert!(!out.entries.contains(&child_new));
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
        assert!(out.entries.contains(&top));
        assert!(out.entries.contains(&child_only));
        assert!(out.entries.contains(&grand_only));
        assert!(!out.entries.contains(&root_only));
        assert!(!out.entries.contains(&grand_child));
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

use super::filelist_reader::parse_filelist_collect;
use super::filelist_writer::filelist_modified_time;
use anyhow::Result;
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub(super) fn apply_nested_filelist_overrides<C>(
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

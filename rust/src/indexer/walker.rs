use std::path::{Path, PathBuf};

use super::MaxDepth;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalkCancelled;

impl std::fmt::Display for WalkCancelled {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("walk cancelled")
    }
}

impl std::error::Error for WalkCancelled {}

pub fn walk_files(root: &Path) -> Vec<PathBuf> {
    walk_entries(root, true, false)
}

pub fn walk_dirs(root: &Path) -> Vec<PathBuf> {
    walk_entries(root, false, true)
}

pub fn walk_entries(root: &Path, include_files: bool, include_dirs: bool) -> Vec<PathBuf> {
    walk_entries_with_max_depth(root, include_files, include_dirs, MaxDepth::unlimited())
}

pub fn walk_entries_with_max_depth(
    root: &Path,
    include_files: bool,
    include_dirs: bool,
    max_depth: MaxDepth,
) -> Vec<PathBuf> {
    walk_entries_cancellable_with_max_depth(root, include_files, include_dirs, max_depth, || false)
        .expect("non-cancellable walker")
}

pub fn walk_entries_cancellable<C>(
    root: &Path,
    include_files: bool,
    include_dirs: bool,
    should_cancel: C,
) -> Result<Vec<PathBuf>, WalkCancelled>
where
    C: Fn() -> bool,
{
    walk_entries_cancellable_with_max_depth(
        root,
        include_files,
        include_dirs,
        MaxDepth::unlimited(),
        should_cancel,
    )
}

pub fn walk_entries_cancellable_with_max_depth<C>(
    root: &Path,
    include_files: bool,
    include_dirs: bool,
    max_depth: MaxDepth,
    should_cancel: C,
) -> Result<Vec<PathBuf>, WalkCancelled>
where
    C: Fn() -> bool,
{
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    visit(
        root,
        max_depth,
        include_files,
        include_dirs,
        &should_cancel,
        &mut |path, is_dir| {
            if is_dir {
                dirs.push(path);
            } else {
                files.push(path);
            }
        },
    )?;
    files.extend(dirs);
    Ok(files)
}

// Use the adaptive serial fast path for complete batch traversal. It shares
// Windows junction and special-file policy without adding thread startup cost
// or the interactive candidate cap to CLI output/FileList creation.
fn visit<F, C>(
    root: &Path,
    max_depth: MaxDepth,
    include_files: bool,
    include_dirs: bool,
    should_cancel: &C,
    on_entry: &mut F,
) -> Result<(), WalkCancelled>
where
    F: FnMut(PathBuf, bool),
    C: Fn() -> bool,
{
    let canceled = std::cell::Cell::new(false);
    let stop = || {
        if canceled.get() || should_cancel() {
            canceled.set(true);
            true
        } else {
            false
        }
    };
    crate::walker_runtime::walk_adaptive_with_max_depth(
        root,
        1,
        1,
        include_files,
        include_dirs,
        max_depth,
        |entry| {
            if stop() {
                return false;
            }
            if let Some((kind, _)) = crate::walker_runtime::classify_walker_entry(
                &entry.path,
                entry.file_type,
                include_files,
                include_dirs,
            ) {
                if stop() {
                    return false;
                }
                on_entry(entry.path, kind.is_dir == Some(true));
            }
            true
        },
        stop,
    );
    if stop() {
        Err(WalkCancelled)
    } else {
        Ok(())
    }
}

pub fn walk_entries_stream<F>(root: &Path, include_files: bool, include_dirs: bool, mut on_entry: F)
where
    F: FnMut(PathBuf),
{
    let result =
        walk_entries_stream_cancellable(root, include_files, include_dirs, || false, &mut on_entry);
    debug_assert_eq!(result, Ok(()));
}

pub fn walk_entries_stream_with_max_depth<F>(
    root: &Path,
    include_files: bool,
    include_dirs: bool,
    max_depth: MaxDepth,
    mut on_entry: F,
) where
    F: FnMut(PathBuf),
{
    let result = walk_entries_stream_cancellable_with_max_depth(
        root,
        include_files,
        include_dirs,
        max_depth,
        || false,
        &mut on_entry,
    );
    debug_assert_eq!(result, Ok(()));
}

pub fn walk_entries_stream_cancellable<F, C>(
    root: &Path,
    include_files: bool,
    include_dirs: bool,
    should_cancel: C,
    on_entry: F,
) -> Result<(), WalkCancelled>
where
    F: FnMut(PathBuf),
    C: Fn() -> bool,
{
    walk_entries_stream_cancellable_with_max_depth(
        root,
        include_files,
        include_dirs,
        MaxDepth::unlimited(),
        should_cancel,
        on_entry,
    )
}

pub fn walk_entries_stream_cancellable_with_max_depth<F, C>(
    root: &Path,
    include_files: bool,
    include_dirs: bool,
    max_depth: MaxDepth,
    should_cancel: C,
    mut on_entry: F,
) -> Result<(), WalkCancelled>
where
    F: FnMut(PathBuf),
    C: Fn() -> bool,
{
    visit(
        root,
        max_depth,
        include_files,
        include_dirs,
        &should_cancel,
        &mut |path, _| on_entry(path),
    )
}

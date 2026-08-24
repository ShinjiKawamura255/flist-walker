mod depth;
mod filelist_hierarchy;
mod filelist_reader;
mod filelist_writer;
mod walker;

use crate::entry::Entry;
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::info;

pub use depth::MaxDepth;
pub use filelist_reader::{
    apply_filelist_hierarchy_overrides, apply_filelist_hierarchy_overrides_with_max_depth,
    build_entries_from_filelist_hierarchy, build_entries_from_filelist_hierarchy_with_max_depth,
    find_filelist, find_filelist_in_first_level, find_filelist_in_first_level_cancellable,
    parse_filelist, parse_filelist_stream, parse_filelist_stream_with_max_depth,
    parse_filelist_with_max_depth, FileListDiscoveryCanceled,
};
pub use filelist_writer::{
    ancestor_filelist_propagation_needed, build_filelist_text, build_filelist_text_cancellable,
    execute_filelist_write_plan, has_ancestor_filelists, plan_filelist_write,
    plan_filelist_write_cancellable, write_filelist, write_filelist_cancellable,
    FileListWriteFailure, FileListWriteOptions, FileListWritePlan, FileListWriteReport,
    FileListWriteStatus, FileListWriteTarget, FileListWriteTargetKind,
};
pub use walker::{
    walk_dirs, walk_entries, walk_entries_cancellable, walk_entries_cancellable_with_max_depth,
    walk_entries_stream, walk_entries_stream_cancellable,
    walk_entries_stream_cancellable_with_max_depth, walk_entries_stream_with_max_depth,
    walk_entries_with_max_depth, walk_files, WalkCancelled,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexBuildCancelled;

impl std::fmt::Display for IndexBuildCancelled {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("index build cancelled")
    }
}

impl std::error::Error for IndexBuildCancelled {}

pub fn is_index_build_cancelled(error: &anyhow::Error) -> bool {
    error.downcast_ref::<IndexBuildCancelled>().is_some()
}

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
    build_index_with_metadata_and_max_depth(
        root,
        use_filelist,
        include_files,
        include_dirs,
        MaxDepth::unlimited(),
    )
}

pub fn build_index_with_metadata_and_max_depth(
    root: &Path,
    use_filelist: bool,
    include_files: bool,
    include_dirs: bool,
    max_depth: MaxDepth,
) -> Result<IndexBuildResult> {
    build_index_with_metadata_cancellable_and_max_depth(
        root,
        use_filelist,
        include_files,
        include_dirs,
        max_depth,
        || false,
    )
}

pub fn build_index_with_metadata_cancellable<C>(
    root: &Path,
    use_filelist: bool,
    include_files: bool,
    include_dirs: bool,
    should_cancel: C,
) -> Result<IndexBuildResult>
where
    C: Fn() -> bool,
{
    build_index_with_metadata_cancellable_and_max_depth(
        root,
        use_filelist,
        include_files,
        include_dirs,
        MaxDepth::unlimited(),
        should_cancel,
    )
}

pub fn build_index_with_metadata_cancellable_and_max_depth<C>(
    root: &Path,
    use_filelist: bool,
    include_files: bool,
    include_dirs: bool,
    max_depth: MaxDepth,
    should_cancel: C,
) -> Result<IndexBuildResult>
where
    C: Fn() -> bool,
{
    // Regression guard: an empty kind scope has no observable source and must
    // avoid even the cancellable FileList discovery wrapper.
    if !include_files && !include_dirs {
        return Ok(IndexBuildResult {
            entries: Vec::new(),
            source: IndexSource::None,
        });
    }
    let discovered_filelist = if use_filelist {
        find_filelist_in_first_level_cancellable(root, &should_cancel)
            .map_err(|_| anyhow::Error::new(IndexBuildCancelled))?
    } else {
        None
    };
    build_index_with_metadata_from_discovery_cancellable_and_max_depth(
        root,
        use_filelist,
        discovered_filelist,
        include_files,
        include_dirs,
        max_depth,
        should_cancel,
    )
}

/// Builds an index from a FileList discovery result owned by the caller.
///
/// Regression guard: callers that must inspect whether FileList is required
/// pass that same result here; rediscovering it can duplicate directory I/O and
/// let an obsolete TUI request continue after freshness changed.
pub fn build_index_with_metadata_from_discovery_cancellable_and_max_depth<C>(
    root: &Path,
    use_filelist: bool,
    discovered_filelist: Option<PathBuf>,
    include_files: bool,
    include_dirs: bool,
    max_depth: MaxDepth,
    should_cancel: C,
) -> Result<IndexBuildResult>
where
    C: Fn() -> bool,
{
    let started_at = Instant::now();
    if !include_files && !include_dirs {
        return Ok(IndexBuildResult {
            entries: Vec::new(),
            source: IndexSource::None,
        });
    }
    if should_cancel() {
        return Err(IndexBuildCancelled.into());
    }

    let requested_root = root;
    let root = requested_root
        .canonicalize()
        .unwrap_or_else(|_| requested_root.to_path_buf());
    let discovered_filelist = discovered_filelist.map(|filelist| {
        // Regression guard: discovery runs before root canonicalization so it can
        // be canceled once. Keep its result on the same canonical path basis as
        // root; mixed Windows verbatim/non-verbatim paths break nested overrides.
        match filelist.canonicalize() {
            Ok(canonical) => canonical,
            Err(_) => {
                if let Ok(relative) = filelist.strip_prefix(requested_root) {
                    root.join(relative)
                } else {
                    filelist
                }
            }
        }
    });
    let result = if use_filelist {
        if let Some(filelist) = discovered_filelist {
            let entries = build_entries_from_filelist_hierarchy_with_max_depth(
                &filelist,
                &root,
                include_files,
                include_dirs,
                max_depth,
                &should_cancel,
            )
            .map_err(|error| {
                if should_cancel() {
                    anyhow::Error::new(IndexBuildCancelled)
                } else {
                    error
                }
            })?;
            IndexBuildResult {
                entries: entries.into_iter().map(Entry::from).collect(),
                source: IndexSource::FileList(filelist),
            }
        } else {
            IndexBuildResult {
                entries: walk_entries_cancellable_with_max_depth(
                    &root,
                    include_files,
                    include_dirs,
                    max_depth,
                    &should_cancel,
                )
                .map_err(|_| anyhow::Error::new(IndexBuildCancelled))?
                .into_iter()
                .map(Entry::from)
                .collect(),
                source: IndexSource::Walker,
            }
        }
    } else {
        IndexBuildResult {
            entries: walk_entries_cancellable_with_max_depth(
                &root,
                include_files,
                include_dirs,
                max_depth,
                &should_cancel,
            )
            .map_err(|_| anyhow::Error::new(IndexBuildCancelled))?
            .into_iter()
            .map(Entry::from)
            .collect(),
            source: IndexSource::Walker,
        }
    };
    if should_cancel() {
        return Err(IndexBuildCancelled.into());
    }
    info!(
        root = %root.display(),
        use_filelist,
        include_files,
        include_dirs,
        max_depth = ?max_depth.value(),
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
    build_index_cancellable(root, use_filelist, include_files, include_dirs, || false)
}

pub fn build_index_with_max_depth(
    root: &Path,
    use_filelist: bool,
    include_files: bool,
    include_dirs: bool,
    max_depth: MaxDepth,
) -> Result<Vec<PathBuf>> {
    build_index_cancellable_with_max_depth(
        root,
        use_filelist,
        include_files,
        include_dirs,
        max_depth,
        || false,
    )
}

pub fn build_index_cancellable<C>(
    root: &Path,
    use_filelist: bool,
    include_files: bool,
    include_dirs: bool,
    should_cancel: C,
) -> Result<Vec<PathBuf>>
where
    C: Fn() -> bool,
{
    build_index_cancellable_with_max_depth(
        root,
        use_filelist,
        include_files,
        include_dirs,
        MaxDepth::unlimited(),
        should_cancel,
    )
}

pub fn build_index_cancellable_with_max_depth<C>(
    root: &Path,
    use_filelist: bool,
    include_files: bool,
    include_dirs: bool,
    max_depth: MaxDepth,
    should_cancel: C,
) -> Result<Vec<PathBuf>>
where
    C: Fn() -> bool,
{
    Ok(build_index_with_metadata_cancellable_and_max_depth(
        root,
        use_filelist,
        include_files,
        include_dirs,
        max_depth,
        should_cancel,
    )?
    .entries
    .into_iter()
    .map(|entry| entry.path)
    .collect())
}

#[cfg(test)]
mod tests;

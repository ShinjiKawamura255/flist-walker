mod filelist_hierarchy;
mod filelist_reader;
mod filelist_writer;
mod walker;

use crate::entry::Entry;
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::info;

pub use filelist_reader::{
    apply_filelist_hierarchy_overrides, build_entries_from_filelist_hierarchy, find_filelist,
    find_filelist_in_first_level, parse_filelist, parse_filelist_stream,
};
pub use filelist_writer::{
    ancestor_filelist_propagation_needed, build_filelist_text, build_filelist_text_cancellable,
    has_ancestor_filelists, write_filelist, write_filelist_cancellable,
};
pub use walker::{walk_dirs, walk_entries, walk_files};

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

#[cfg(test)]
mod tests;

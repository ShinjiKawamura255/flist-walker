use super::{ResultSortMode, SortMetadata};
use crate::entry::{Entry, EntryKind};
use crate::indexer::IndexSource;
use crate::updater::UpdateCandidate;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub(super) struct SearchRequest {
    pub(super) request_id: u64,
    pub(super) query: String,
    pub(super) entries: Arc<Vec<Entry>>,
    pub(super) limit: usize,
    pub(super) use_regex: bool,
    pub(super) ignore_case: bool,
    pub(super) root: PathBuf,
    pub(super) prefer_relative: bool,
}

pub(super) struct SearchResponse {
    pub(super) request_id: u64,
    pub(super) results: Vec<(PathBuf, f64)>,
    pub(super) error: Option<String>,
}

#[derive(Clone, Debug)]
pub(super) struct IndexEntry {
    pub(super) path: PathBuf,
    pub(super) kind: EntryKind,
    pub(super) kind_known: bool,
}

impl From<IndexEntry> for Entry {
    fn from(value: IndexEntry) -> Self {
        Self::new(value.path, value.kind_known.then_some(value.kind))
    }
}

pub(super) struct IndexRequest {
    pub(super) request_id: u64,
    pub(super) tab_id: u64,
    pub(super) root: PathBuf,
    pub(super) use_filelist: bool,
    pub(super) include_files: bool,
    pub(super) include_dirs: bool,
}

pub(super) enum IndexResponse {
    Started {
        request_id: u64,
        source: IndexSource,
    },
    Batch {
        request_id: u64,
        entries: Vec<IndexEntry>,
    },
    ReplaceAll {
        request_id: u64,
        entries: Vec<IndexEntry>,
    },
    Finished {
        request_id: u64,
        source: IndexSource,
    },
    Failed {
        request_id: u64,
        error: String,
    },
    Canceled {
        request_id: u64,
    },
    Truncated {
        request_id: u64,
        limit: usize,
    },
}

pub(super) struct PreviewRequest {
    pub(super) request_id: u64,
    pub(super) path: PathBuf,
    pub(super) is_dir: bool,
}

pub(super) struct PreviewResponse {
    pub(super) request_id: u64,
    pub(super) path: PathBuf,
    pub(super) preview: String,
}

pub(super) struct ActionRequest {
    pub(super) request_id: u64,
    pub(super) paths: Vec<PathBuf>,
    pub(super) open_parent_for_files: bool,
}

pub(super) struct ActionResponse {
    pub(super) request_id: u64,
    pub(super) notice: String,
}

pub(super) enum UpdateRequestKind {
    Check,
    DownloadAndApply {
        candidate: Box<UpdateCandidate>,
        current_exe: PathBuf,
    },
}

pub(super) struct UpdateRequest {
    pub(super) request_id: u64,
    pub(super) kind: UpdateRequestKind,
}

pub(super) enum UpdateResponse {
    UpToDate {
        request_id: u64,
    },
    CheckFailed {
        request_id: u64,
        error: String,
    },
    Available {
        request_id: u64,
        candidate: Box<UpdateCandidate>,
    },
    ApplyStarted {
        request_id: u64,
        target_version: String,
    },
    Failed {
        request_id: u64,
        error: String,
    },
}

pub(super) struct SortMetadataRequest {
    pub(super) request_id: u64,
    pub(super) paths: Vec<PathBuf>,
    pub(super) mode: ResultSortMode,
}

pub(super) struct SortMetadataResponse {
    pub(super) request_id: u64,
    pub(super) entries: Vec<(PathBuf, SortMetadata)>,
    pub(super) mode: ResultSortMode,
}

pub(super) struct KindResolveRequest {
    pub(super) epoch: u64,
    pub(super) path: PathBuf,
}

pub(super) struct KindResolveResponse {
    pub(super) epoch: u64,
    pub(super) path: PathBuf,
    pub(super) kind: Option<EntryKind>,
}

pub(super) struct FileListRequest {
    pub(super) request_id: u64,
    pub(super) tab_id: u64,
    pub(super) root: PathBuf,
    pub(super) entries: Vec<PathBuf>,
    pub(super) propagate_to_ancestors: bool,
    pub(super) cancel: Arc<AtomicBool>,
}

pub(super) enum FileListResponse {
    Finished {
        request_id: u64,
        root: PathBuf,
        path: PathBuf,
        count: usize,
    },
    Failed {
        request_id: u64,
        root: PathBuf,
        error: String,
    },
    Canceled {
        request_id: u64,
        root: PathBuf,
    },
}

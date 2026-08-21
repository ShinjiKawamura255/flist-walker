use super::{ResultSortMode, ResultSortScope, SortMetadata};
use crate::entry::{Entry, EntryKind};
use crate::indexer::IndexSource;
use crate::indexer::MaxDepth;
use crate::search_catalog::{SearchCatalog, SearchPreset};
use crate::updater::{UpdateCandidate, UpdateInstallControl};
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
    pub(super) sort_mode: ResultSortMode,
    pub(super) sort_scope: ResultSortScope,
    pub(super) cancel: Arc<AtomicBool>,
}

pub(super) struct SearchResponse {
    pub(super) request_id: u64,
    pub(super) results: Vec<(PathBuf, f64)>,
    pub(super) total_match_count: usize,
    pub(super) sort_mode: ResultSortMode,
    pub(super) sort_scope: ResultSortScope,
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
        let kind = if value.kind_known || value.kind.is_link() {
            Some(value.kind)
        } else {
            None
        };
        Self::new(value.path, kind)
    }
}

pub(super) struct IndexRequest {
    pub(super) request_id: u64,
    pub(super) tab_id: u64,
    pub(super) root: PathBuf,
    pub(super) use_filelist: bool,
    pub(super) include_files: bool,
    pub(super) include_dirs: bool,
    pub(super) max_depth: MaxDepth,
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
    pub(super) root: PathBuf,
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
    pub(super) control: Arc<UpdateInstallControl>,
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
    Canceled {
        request_id: u64,
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
    pub(super) tab_id: u64,
    pub(super) epoch: u64,
    pub(super) path: PathBuf,
}

pub(super) struct KindResolveResponse {
    pub(super) tab_id: u64,
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

pub(super) struct CatalogRequest {
    pub(super) request_id: u64,
    pub(super) kind: CatalogRequestKind,
}

pub(super) enum CatalogRequestKind {
    Load,
    AddNamedRoot {
        name: String,
        path: PathBuf,
    },
    ReplaceNamedRoot {
        original_name: String,
        name: String,
        path: PathBuf,
    },
    RemoveNamedRoot {
        name: String,
    },
    AddPreset {
        preset: SearchPreset,
    },
    ReplacePreset {
        original_name: String,
        preset: SearchPreset,
    },
    RemovePreset {
        name: String,
    },
}

pub(super) struct CatalogResponse {
    pub(super) request_id: u64,
    pub(super) result: Result<SearchCatalog, String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RootValidationIntent {
    Add,
    Edit { index: usize },
}

pub(super) struct RootValidationRequest {
    pub(super) request_id: u64,
    pub(super) dialog_generation: u64,
    pub(super) intent: RootValidationIntent,
    pub(super) input: String,
    pub(super) draft_roots: Vec<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ValidatedRoot {
    pub(super) path: PathBuf,
    pub(super) key: String,
}

pub(super) struct RootValidationResponse {
    pub(super) request_id: u64,
    pub(super) dialog_generation: u64,
    pub(super) intent: RootValidationIntent,
    pub(super) result: Result<ValidatedRoot, String>,
}

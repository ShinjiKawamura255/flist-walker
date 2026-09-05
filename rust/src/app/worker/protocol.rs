use crate::app::{ResultSortMode, ResultSortScope, SortMetadata};
use crate::entry::{Entry, EntryKind};
use crate::indexer::IndexSource;
use crate::indexer::MaxDepth;
use crate::search_catalog::{SearchCatalog, SearchPreset};
use crate::updater::{UpdateCandidate, UpdateInstallControl};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub(in crate::app) struct SearchRequest {
    pub(in crate::app) request_id: u64,
    pub(in crate::app) query: String,
    pub(in crate::app) entries: Arc<Vec<Entry>>,
    pub(in crate::app) limit: usize,
    pub(in crate::app) use_regex: bool,
    pub(in crate::app) ignore_case: bool,
    pub(in crate::app) root: PathBuf,
    pub(in crate::app) prefer_relative: bool,
    pub(in crate::app) sort_mode: ResultSortMode,
    pub(in crate::app) sort_scope: ResultSortScope,
    pub(in crate::app) cancel: Arc<AtomicBool>,
}

pub(in crate::app) struct SearchResponse {
    pub(in crate::app) request_id: u64,
    pub(in crate::app) results: Vec<(PathBuf, f64)>,
    pub(in crate::app) total_match_count: usize,
    pub(in crate::app) sort_mode: ResultSortMode,
    pub(in crate::app) sort_scope: ResultSortScope,
    pub(in crate::app) error: Option<String>,
}

#[derive(Clone, Debug)]
pub(in crate::app) struct IndexEntry {
    pub(in crate::app) path: PathBuf,
    pub(in crate::app) kind: EntryKind,
    pub(in crate::app) kind_known: bool,
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

pub(in crate::app) struct IndexRequest {
    pub(in crate::app) request_id: u64,
    pub(in crate::app) tab_id: u64,
    pub(in crate::app) root: PathBuf,
    pub(in crate::app) use_filelist: bool,
    pub(in crate::app) include_files: bool,
    pub(in crate::app) include_dirs: bool,
    pub(in crate::app) max_depth: MaxDepth,
    pub(in crate::app) follow_links: bool,
}

pub(in crate::app) enum IndexResponse {
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

pub(in crate::app) struct PreviewRequest {
    pub(in crate::app) request_id: u64,
    pub(in crate::app) path: PathBuf,
    pub(in crate::app) is_dir: bool,
}

pub(in crate::app) struct PreviewResponse {
    pub(in crate::app) canceled: bool,
    pub(in crate::app) request_id: u64,
    pub(in crate::app) path: PathBuf,
    pub(in crate::app) preview: String,
}

pub(in crate::app) struct ActionRequest {
    pub(in crate::app) request_id: u64,
    pub(in crate::app) root: PathBuf,
    pub(in crate::app) paths: Vec<PathBuf>,
    pub(in crate::app) open_parent_for_files: bool,
}

pub(in crate::app) struct ActionResponse {
    pub(in crate::app) request_id: u64,
    pub(in crate::app) notice: String,
}

pub(in crate::app) enum UpdateRequestKind {
    Check,
    DownloadAndApply {
        candidate: Box<UpdateCandidate>,
        current_exe: PathBuf,
    },
}

pub(in crate::app) struct UpdateRequest {
    pub(in crate::app) request_id: u64,
    pub(in crate::app) kind: UpdateRequestKind,
    pub(in crate::app) control: Arc<UpdateInstallControl>,
}

pub(in crate::app) enum UpdateResponse {
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

pub(in crate::app) struct SortMetadataRequest {
    pub(in crate::app) request_id: u64,
    pub(in crate::app) paths: Vec<PathBuf>,
    pub(in crate::app) mode: ResultSortMode,
}

pub(in crate::app) struct SortMetadataResponse {
    pub(in crate::app) request_id: u64,
    pub(in crate::app) entries: Vec<(PathBuf, SortMetadata)>,
    pub(in crate::app) mode: ResultSortMode,
}

pub(in crate::app) struct KindResolveRequest {
    pub(in crate::app) tab_id: u64,
    pub(in crate::app) epoch: u64,
    pub(in crate::app) path: PathBuf,
}

pub(in crate::app) struct KindResolveResponse {
    pub(in crate::app) tab_id: u64,
    pub(in crate::app) epoch: u64,
    pub(in crate::app) path: PathBuf,
    pub(in crate::app) kind: Option<EntryKind>,
}

pub(in crate::app) struct FileListRequest {
    pub(in crate::app) request_id: u64,
    pub(in crate::app) tab_id: u64,
    pub(in crate::app) root: PathBuf,
    pub(in crate::app) entries: Vec<PathBuf>,
    pub(in crate::app) propagate_to_ancestors: bool,
    pub(in crate::app) cancel: Arc<AtomicBool>,
}

pub(in crate::app) enum FileListResponse {
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

pub(in crate::app) struct CatalogRequest {
    pub(in crate::app) request_id: u64,
    pub(in crate::app) kind: CatalogRequestKind,
}

pub(in crate::app) enum CatalogRequestKind {
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

pub(in crate::app) struct CatalogResponse {
    pub(in crate::app) request_id: u64,
    pub(in crate::app) result: Result<SearchCatalog, String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum RootValidationIntent {
    Add,
    Edit { index: usize },
}

pub(in crate::app) struct RootValidationRequest {
    pub(in crate::app) request_id: u64,
    pub(in crate::app) dialog_generation: u64,
    pub(in crate::app) intent: RootValidationIntent,
    pub(in crate::app) input: String,
    pub(in crate::app) draft_roots: Vec<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::app) struct ValidatedRoot {
    pub(in crate::app) path: PathBuf,
    pub(in crate::app) key: String,
}

pub(in crate::app) struct RootValidationResponse {
    pub(in crate::app) request_id: u64,
    pub(in crate::app) dialog_generation: u64,
    pub(in crate::app) intent: RootValidationIntent,
    pub(in crate::app) result: Result<ValidatedRoot, String>,
}

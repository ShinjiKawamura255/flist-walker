use super::CliTuiOptions;
use crate::actions::{
    execute_or_open, AuthorizedActionBackend, AuthorizedActionGuard, AuthorizedActionMode,
    AuthorizedActionReport, AuthorizedActionRequest,
};
use crate::indexer::FileListWriteReport;
use crate::indexer::MaxDepth;
use crate::persistence::AsyncHistoryPersistence;
use crate::search::SearchSortMode;
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

pub(super) const EVENT_POLL: Duration = Duration::from_millis(50);
pub(super) const WORKER_JOIN_TIMEOUT: Duration = Duration::from_millis(250);

pub(super) enum WorkerResponse {
    IndexedSharedBatch {
        request_id: u64,
        root: PathBuf,
        batch: CandidateBatch,
    },
    IndexedBatch {
        request_id: u64,
        root: PathBuf,
        entries: Vec<PathBuf>,
    },
    IndexedFinished {
        request_id: u64,
        root: PathBuf,
        has_root_filelist: bool,
    },
    IndexTruncated {
        request_id: u64,
        root: PathBuf,
        limit: usize,
    },
    IndexFailed {
        request_id: u64,
        root: PathBuf,
        has_root_filelist: bool,
        error: String,
    },
    Searched {
        request_id: u64,
        root: PathBuf,
        query: String,
        options: SearchOptions,
        results: Arc<Vec<(PathBuf, f64)>>,
        error: Option<String>,
    },
    Previewed {
        request_id: u64,
        root: PathBuf,
        path: PathBuf,
        preview: String,
    },
    Actioned {
        request_id: u64,
        root: PathBuf,
        selected_path: PathBuf,
        report: AuthorizedActionReport,
    },
}

pub(super) enum FileListWorkerResult {
    DiscoveryFinished {
        request_id: u64,
        root: PathBuf,
        discovered: Option<PathBuf>,
        canceled: bool,
    },
    Finished {
        request_id: u64,
        root: PathBuf,
        report: FileListWriteReport,
    },
    Failed {
        request_id: u64,
        root: PathBuf,
        error: String,
    },
}

pub(super) struct SearchRequest {
    pub(super) request_id: u64,
    pub(super) query: String,
    pub(super) entries: Arc<Vec<CandidateBatch>>,
    pub(super) root: PathBuf,
    pub(super) limit: usize,
    pub(super) options: SearchOptions,
    pub(super) ignore_terms: Arc<Vec<String>>,
    pub(super) cancel: Arc<AtomicBool>,
}

pub(super) type IndexBatchOwner = Arc<Mutex<Vec<Arc<[PathBuf]>>>>;

// The owner field drops after entries. The recycler retains the owner while any
// response, UI snapshot or search request may still hold one of its batches.
#[derive(Clone, Debug)]
pub(super) struct CandidateBatch {
    pub(super) entries: Arc<[PathBuf]>,
    pub(super) _owner: Option<IndexBatchOwner>,
}
#[cfg(test)]
impl From<Vec<PathBuf>> for CandidateBatch {
    fn from(entries: Vec<PathBuf>) -> Self {
        Self {
            entries: Arc::from(entries),
            _owner: None,
        }
    }
}
impl std::ops::Deref for CandidateBatch {
    type Target = [PathBuf];
    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct CandidateBatches {
    pub(super) batches: Arc<Vec<CandidateBatch>>,
    pub(super) len: usize,
}

impl CandidateBatches {
    pub(super) fn push(&mut self, entries: Vec<PathBuf>) {
        if entries.is_empty() {
            return;
        }
        self.push_shared(CandidateBatch {
            entries: Arc::from(entries),
            _owner: None,
        });
    }

    pub(super) fn push_shared(&mut self, batch: CandidateBatch) {
        self.len = self.len.saturating_add(batch.len());
        Arc::make_mut(&mut self.batches).push(batch);
    }

    pub(super) fn clear(&mut self) {
        self.batches = Arc::new(Vec::new());
        self.len = 0;
    }

    pub(super) fn len(&self) -> usize {
        self.len
    }

    pub(super) fn snapshot(&self) -> Arc<Vec<CandidateBatch>> {
        Arc::clone(&self.batches)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SearchOptions {
    pub(super) regex: bool,
    pub(super) ignore_case: bool,
    pub(super) ignore_enabled: bool,
    pub(super) sort_mode: SearchSortMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TuiSource {
    Auto,
    FileList,
    Walker,
}

impl TuiSource {
    const ALL: [Self; 3] = [Self::Auto, Self::FileList, Self::Walker];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::FileList => "FileList",
            Self::Walker => "Walker",
        }
    }

    pub(super) fn next(self) -> Self {
        let index = Self::ALL.iter().position(|item| *item == self).unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TuiRuntimeOptions {
    pub(super) include_files: bool,
    pub(super) include_dirs: bool,
    pub(super) regex: bool,
    pub(super) ignore_case: bool,
    pub(super) ignore_enabled: bool,
    pub(super) source: TuiSource,
}

impl TuiRuntimeOptions {
    pub(super) fn from_startup(options: &CliTuiOptions) -> Self {
        let source = if options.require_filelist {
            TuiSource::FileList
        } else if options.use_filelist {
            TuiSource::Auto
        } else {
            TuiSource::Walker
        };
        Self {
            include_files: options.include_files,
            include_dirs: options.include_dirs,
            regex: options.regex,
            ignore_case: options.ignore_case,
            ignore_enabled: options.ignore_enabled,
            source,
        }
    }

    pub(super) fn search_options(self, sort_mode: SearchSortMode) -> SearchOptions {
        SearchOptions {
            regex: self.regex,
            ignore_case: self.ignore_case,
            ignore_enabled: self.ignore_enabled,
            sort_mode,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) enum FileListDiscoveryOwnership {
    WorkerOwned,
    Completed(Option<PathBuf>),
}

#[derive(Clone, Debug)]
pub(super) struct IndexRequest {
    pub(super) request_id: u64,
    pub(super) root: PathBuf,
    pub(super) include_files: bool,
    pub(super) include_dirs: bool,
    pub(super) source: TuiSource,
    pub(super) filelist_discovery: FileListDiscoveryOwnership,
    pub(super) max_depth: MaxDepth,
}

pub(super) struct TuiIndexFreshness {
    pub(super) current_request_id: AtomicU64,
}

impl TuiIndexFreshness {
    pub(super) fn new() -> Self {
        Self {
            current_request_id: AtomicU64::new(0),
        }
    }

    pub(super) fn activate(&self, request_id: u64) {
        self.current_request_id.store(request_id, Ordering::Release);
    }

    pub(super) fn is_current(&self, request_id: u64) -> bool {
        self.current_request_id.load(Ordering::Acquire) == request_id
    }
}

pub(super) struct PreviewRequest {
    pub(super) cancel: Arc<AtomicBool>,
    pub(super) request_id: u64,
    pub(super) root: PathBuf,
    pub(super) path: PathBuf,
}

pub(super) struct TuiActionRequest {
    pub(super) request: AuthorizedActionRequest,
    pub(super) selected_path: PathBuf,
}

pub(super) struct TuiFileListRequest {
    pub(super) request_id: u64,
    pub(super) root: PathBuf,
    pub(super) propagate_to_ancestors: bool,
    pub(super) allow_root_overwrite: bool,
    pub(super) cancel: Arc<AtomicBool>,
}

pub(super) struct TuiFileListDiscoveryRequest {
    pub(super) request_id: u64,
    pub(super) root: PathBuf,
    pub(super) cancel: Arc<AtomicBool>,
}

pub(super) struct TuiActionFreshness {
    pub(super) current_request_id: AtomicU64,
    pub(super) trusted_root: Mutex<PathBuf>,
}

impl TuiActionFreshness {
    pub(super) fn new() -> Self {
        Self {
            current_request_id: AtomicU64::new(0),
            trusted_root: Mutex::new(PathBuf::new()),
        }
    }

    pub(super) fn activate(&self, request_id: u64, root: &Path) {
        if let Ok(mut trusted_root) = self.trusted_root.lock() {
            *trusted_root = root.to_path_buf();
        }
        self.current_request_id.store(request_id, Ordering::Release);
    }
}

impl AuthorizedActionGuard for TuiActionFreshness {
    fn is_current(&self, request_id: u64, trusted_root: &Path) -> bool {
        self.current_request_id.load(Ordering::Acquire) == request_id
            && self
                .trusted_root
                .lock()
                .is_ok_and(|current_root| current_root.as_path() == trusted_root)
    }
}

pub(super) struct TuiActionBackend;

impl AuthorizedActionBackend for TuiActionBackend {
    fn execute_or_open(&self, path: &Path) -> Result<()> {
        execute_or_open(path)
    }

    fn reveal(&self, path: &Path) -> Result<()> {
        execute_or_open(path)
    }
}

pub(super) struct EventLoopContext<'a> {
    pub(super) preset_worker: &'a super::presets::PresetWorker,
    pub(super) index_tx: &'a mpsc::Sender<IndexRequest>,
    pub(super) index_freshness: Arc<TuiIndexFreshness>,
    pub(super) search_tx: &'a mpsc::Sender<SearchRequest>,
    pub(super) preview_tx: &'a mpsc::Sender<PreviewRequest>,
    pub(super) action_tx: &'a mpsc::Sender<TuiActionRequest>,
    pub(super) rx: &'a mpsc::Receiver<WorkerResponse>,
    pub(super) root: PathBuf,
    pub(super) initial_filelist_discovery: FileListDiscoveryOwnership,
    pub(super) saved_roots: Vec<PathBuf>,
    pub(super) options: &'a CliTuiOptions,
    pub(super) history_enabled: bool,
    pub(super) history_entries: Vec<String>,
    pub(super) history_persistence: Option<&'a AsyncHistoryPersistence>,
    pub(super) action_freshness: Arc<TuiActionFreshness>,
    pub(super) cancellation: Arc<AtomicBool>,
}

pub(super) enum TuiExit {
    Cancelled,
    Failed(String),
    Selected {
        paths: Vec<PathBuf>,
        query: String,
        root: PathBuf,
    },
}

pub(super) enum KeyAction {
    OpenPresets,
    PresetOperation(super::presets::PresetOperation),
    Continue,
    Cancel,
    Select,
    HistoryApplied,
    HistoryOpened(Option<String>),
    DispatchAction(AuthorizedActionMode),
    Reindex,
    Refresh,
    SwitchRoot(PathBuf),
    OpenFileList,
    StartFileList {
        propagate_to_ancestors: bool,

        allow_root_overwrite: bool,
    },
}

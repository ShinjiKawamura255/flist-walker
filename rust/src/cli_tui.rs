use crate::actions::{
    execute_authorized_action_request, execute_or_open, AuthorizedActionBackend,
    AuthorizedActionGuard, AuthorizedActionMode, AuthorizedActionOutcome, AuthorizedActionReport,
    AuthorizedActionRequest,
};
use crate::entry::Entry;
use crate::indexer::{
    build_index_cancellable, execute_filelist_write_plan, find_filelist_in_first_level,
    is_index_build_cancelled, plan_filelist_write_cancellable, FileListWriteOptions,
    FileListWriteReport, FileListWriteStatus,
};
#[cfg(test)]
use crate::path_utils::output_path_bytes;
use crate::persistence::{
    history_persistence_enabled, load_persisted_roots_and_history, AsyncHistoryPersistence,
};
use crate::query::{CompiledIgnoreTerms, CompiledQuery, QueryOptions, QueryScope};
use crate::runtime_config::{current_runtime_config, RuntimeConfig};
use crate::search::{rank_search_results, SearchPrefixCache, SearchSortMode, SearchSortScope};
use crate::ui_model::{build_preview_text_with_kind, display_path_with_mode};
#[cfg(not(test))]
use crate::updater::check_for_update;
use crate::updater::UpdateCandidate;
use crate::walker_runtime::{
    classify_walker_entry, walk_adaptive, walker_runtime_settings, walker_truncated_notice,
    AdaptiveWalkerEntry,
};
use anyhow::{Context, Result};
use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers,
};
use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor};
use crossterm::terminal::{
    self, BeginSynchronizedUpdate, Clear, ClearType, EndSynchronizedUpdate, EnterAlternateScreen,
    LeaveAlternateScreen,
};
use crossterm::{execute, queue};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::collections::HashSet;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use unicode_width::UnicodeWidthChar;

const INPUT_DEBOUNCE: Duration = Duration::from_millis(35);
const INDEX_REFRESH_THROTTLE: Duration = Duration::from_millis(100);
const EVENT_POLL: Duration = Duration::from_millis(50);
const WORKER_JOIN_TIMEOUT: Duration = Duration::from_millis(250);
const MAX_WORKER_RESPONSES_PER_TICK: usize = 64;
const PREVIEW_MIN_WIDTH: u16 = 100;
const PREVIEW_MIN_HEIGHT: u16 = 8;

fn format_tui_update_notice(target_version: &str) -> String {
    format!("Update available: v{target_version} — Run flistwalker --update after exiting")
}

#[cfg(not(test))]
fn spawn_tui_update_check() -> mpsc::Receiver<Option<UpdateCandidate>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let candidate = check_for_update().ok().flatten();
        let _ = tx.send(candidate);
    });
    rx
}

#[cfg(test)]
fn spawn_tui_update_check() -> mpsc::Receiver<Option<UpdateCandidate>> {
    let (_tx, rx) = mpsc::channel();
    rx
}

#[derive(Clone, Debug)]
pub struct CliTuiOptions {
    pub initial_query: String,
    pub limit: usize,
    pub absolute: bool,
    pub print0: bool,
    pub include_files: bool,
    pub include_dirs: bool,
    pub use_filelist: bool,
    pub require_filelist: bool,
    pub regex: bool,
    pub ignore_case: bool,
    pub ignore_enabled: bool,
    pub ignore_terms: Vec<String>,
    pub sort_mode: SearchSortMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CliTuiOutcome {
    Selected { paths: Vec<PathBuf>, root: PathBuf },
    Cancelled,
}

enum WorkerResponse {
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
        results: Vec<(PathBuf, f64)>,
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

enum FileListWorkerResult {
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

struct SearchRequest {
    request_id: u64,
    query: String,
    entries: Arc<Vec<Arc<[PathBuf]>>>,
    root: PathBuf,
    limit: usize,
    options: SearchOptions,
    ignore_terms: Arc<Vec<String>>,
}

#[derive(Clone, Debug, Default)]
struct CandidateBatches {
    batches: Arc<Vec<Arc<[PathBuf]>>>,
    len: usize,
}

impl CandidateBatches {
    fn push(&mut self, entries: Vec<PathBuf>) {
        if entries.is_empty() {
            return;
        }
        self.len = self.len.saturating_add(entries.len());
        Arc::make_mut(&mut self.batches).push(Arc::from(entries));
    }

    fn clear(&mut self) {
        self.batches = Arc::new(Vec::new());
        self.len = 0;
    }

    fn len(&self) -> usize {
        self.len
    }

    fn snapshot(&self) -> Arc<Vec<Arc<[PathBuf]>>> {
        Arc::clone(&self.batches)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SearchOptions {
    regex: bool,
    ignore_case: bool,
    ignore_enabled: bool,
    sort_mode: SearchSortMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TuiSource {
    Auto,
    FileList,
    Walker,
}

impl TuiSource {
    const ALL: [Self; 3] = [Self::Auto, Self::FileList, Self::Walker];

    fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::FileList => "FileList",
            Self::Walker => "Walker",
        }
    }

    fn next(self) -> Self {
        let index = Self::ALL.iter().position(|item| *item == self).unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TuiRuntimeOptions {
    include_files: bool,
    include_dirs: bool,
    regex: bool,
    ignore_case: bool,
    ignore_enabled: bool,
    source: TuiSource,
}

impl TuiRuntimeOptions {
    fn from_startup(options: &CliTuiOptions) -> Self {
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

    fn search_options(self, sort_mode: SearchSortMode) -> SearchOptions {
        SearchOptions {
            regex: self.regex,
            ignore_case: self.ignore_case,
            ignore_enabled: self.ignore_enabled,
            sort_mode,
        }
    }
}

#[derive(Clone, Debug)]
struct IndexRequest {
    request_id: u64,
    root: PathBuf,
    include_files: bool,
    include_dirs: bool,
    source: TuiSource,
}

struct TuiIndexFreshness {
    current_request_id: AtomicU64,
}

impl TuiIndexFreshness {
    fn new() -> Self {
        Self {
            current_request_id: AtomicU64::new(0),
        }
    }

    fn activate(&self, request_id: u64) {
        self.current_request_id.store(request_id, Ordering::Release);
    }

    fn is_current(&self, request_id: u64) -> bool {
        self.current_request_id.load(Ordering::Acquire) == request_id
    }
}

struct PreviewRequest {
    request_id: u64,
    root: PathBuf,
    path: PathBuf,
}

struct TuiActionRequest {
    request: AuthorizedActionRequest,
    selected_path: PathBuf,
}

struct TuiFileListRequest {
    request_id: u64,
    root: PathBuf,
    propagate_to_ancestors: bool,
    allow_root_overwrite: bool,
    cancel: Arc<AtomicBool>,
}

struct TuiActionFreshness {
    current_request_id: AtomicU64,
    trusted_root: Mutex<PathBuf>,
}

impl TuiActionFreshness {
    fn new() -> Self {
        Self {
            current_request_id: AtomicU64::new(0),
            trusted_root: Mutex::new(PathBuf::new()),
        }
    }

    fn activate(&self, request_id: u64, root: &Path) {
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

struct TuiActionBackend;

impl AuthorizedActionBackend for TuiActionBackend {
    fn execute_or_open(&self, path: &Path) -> Result<()> {
        execute_or_open(path)
    }

    fn reveal(&self, path: &Path) -> Result<()> {
        execute_or_open(path)
    }
}

struct EventLoopContext<'a> {
    index_tx: &'a mpsc::Sender<IndexRequest>,
    index_freshness: Arc<TuiIndexFreshness>,
    search_tx: &'a mpsc::Sender<SearchRequest>,
    preview_tx: &'a mpsc::Sender<PreviewRequest>,
    action_tx: &'a mpsc::Sender<TuiActionRequest>,
    rx: &'a mpsc::Receiver<WorkerResponse>,
    root: PathBuf,
    saved_roots: Vec<PathBuf>,
    options: &'a CliTuiOptions,
    history_enabled: bool,
    history_entries: Vec<String>,
    history_persistence: Option<&'a AsyncHistoryPersistence>,
    action_freshness: Arc<TuiActionFreshness>,
    cancellation: Arc<AtomicBool>,
}

enum TuiExit {
    Cancelled,
    Failed(String),
    Selected {
        paths: Vec<PathBuf>,
        query: String,
        root: PathBuf,
    },
}

enum KeyAction {
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

struct TuiState {
    query: String,
    query_cursor: usize,
    results: Vec<(PathBuf, f64)>,
    selected: usize,
    offset: usize,
    status: String,
    update_notice: Option<String>,
    dirty: bool,
    last_query_change: Option<Instant>,
    indexed: bool,
    root_filelist_known: bool,
    root_filelist_exists: bool,
    entries: CandidateBatches,
    root: PathBuf,
    saved_roots: Vec<PathBuf>,
    root_picker: Option<RootPicker>,
    runtime_options: TuiRuntimeOptions,
    ignore_terms: Arc<Vec<String>>,
    sort_mode: SearchSortMode,
    source_changed_on_apply: bool,
    next_index_request_id: u64,
    active_index_request: Option<(u64, PathBuf)>,
    index_truncated_limit: Option<usize>,
    pinned: Vec<PathBuf>,
    emacs_keybindings_enabled: bool,
    tab_pin_moves_to_next_row: bool,
    kill_buffer: String,
    viewport_rows: usize,
    next_search_request_id: u64,
    active_search_request_id: Option<u64>,
    last_incremental_search: Option<Instant>,
    preview_preferred: bool,
    preview_visible: bool,
    preview: String,
    next_preview_request_id: u64,
    active_preview_request: Option<PreviewRequestIdentity>,
    history_enabled: bool,
    history_entries: Vec<String>,
    history: Option<HistoryOverlay>,
    help: Option<HelpContext>,
    options_overlay: Option<OptionsOverlay>,
    sort_picker: Option<SortPicker>,
    filelist_confirmation: Option<FileListConfirmation>,
    next_filelist_request_id: u64,
    active_filelist: Option<ActiveFileList>,
    pending_filelist_intent: Option<PendingFileListIntent>,
    next_action_request_id: u64,
    active_action_request: Option<(u64, PathBuf)>,
}

#[derive(Clone, Debug)]
struct HistoryOverlay {
    draft_query: String,
    filter: String,
    filter_cursor: usize,
    results: Vec<String>,
    selected: usize,
    offset: usize,
}

#[derive(Clone, Debug)]
struct OptionsOverlay {
    draft: TuiRuntimeOptions,
    selected: usize,
}

#[derive(Clone, Debug)]
struct SortPicker {
    selected: usize,
}

#[derive(Clone, Debug)]
struct RootPicker {
    selected: usize,
}

#[derive(Clone, Debug)]
enum FileListConfirmation {
    Mode { propagate_to_ancestors: bool },
    Overwrite { propagate_to_ancestors: bool },
}

#[derive(Clone, Debug)]
struct ActiveFileList {
    request_id: u64,
    root: PathBuf,
    cancel: Arc<AtomicBool>,
}

struct ActiveFileListWorker {
    cancel: Arc<AtomicBool>,
    result: mpsc::Receiver<FileListWorkerResult>,
    done: mpsc::Receiver<()>,
    handle: Option<thread::JoinHandle<()>>,
}

impl ActiveFileListWorker {
    fn join(mut self) {
        let _ = self.done.recv();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }

    fn is_finished(&self) -> bool {
        self.handle
            .as_ref()
            .is_some_and(thread::JoinHandle::is_finished)
    }
}

impl Drop for ActiveFileListWorker {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        let _ = self.done.recv();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PendingFileListIntent {
    SelectOutput,
    SwitchRoot(PathBuf),
    CancelExit,
}

const SORT_MODES: [SearchSortMode; 9] = [
    SearchSortMode::Score,
    SearchSortMode::NameAsc,
    SearchSortMode::NameDesc,
    SearchSortMode::ModifiedDesc,
    SearchSortMode::ModifiedAsc,
    SearchSortMode::CreatedDesc,
    SearchSortMode::CreatedAsc,
    SearchSortMode::SizeDesc,
    SearchSortMode::SizeAsc,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HelpContext {
    Normal,
    History,
    FileList,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PreviewRequestIdentity {
    request_id: u64,
    root: PathBuf,
    path: PathBuf,
}

impl TuiState {
    fn new(query: &str) -> Self {
        Self {
            query: query.to_string(),
            query_cursor: query.chars().count(),
            results: Vec::new(),
            selected: 0,
            offset: 0,
            status: "Indexing...".to_string(),
            update_notice: None,
            dirty: true,
            last_query_change: Some(Instant::now()),
            indexed: false,
            root_filelist_known: false,
            root_filelist_exists: false,
            entries: CandidateBatches::default(),
            root: PathBuf::new(),
            saved_roots: Vec::new(),
            root_picker: None,
            runtime_options: TuiRuntimeOptions {
                include_files: true,
                include_dirs: true,
                regex: false,
                ignore_case: false,
                ignore_enabled: true,
                source: TuiSource::Auto,
            },
            ignore_terms: Arc::new(Vec::new()),
            sort_mode: SearchSortMode::Score,
            source_changed_on_apply: false,
            next_index_request_id: 0,
            active_index_request: None,
            index_truncated_limit: None,
            pinned: Vec::new(),
            emacs_keybindings_enabled: true,
            tab_pin_moves_to_next_row: false,
            kill_buffer: String::new(),
            viewport_rows: 1,
            next_search_request_id: 0,
            active_search_request_id: None,
            last_incremental_search: None,
            preview_preferred: true,
            preview_visible: false,
            preview: String::new(),
            next_preview_request_id: 0,
            active_preview_request: None,
            history_enabled: false,
            history_entries: Vec::new(),
            history: None,
            help: None,
            options_overlay: None,
            sort_picker: None,
            filelist_confirmation: None,
            next_filelist_request_id: 0,
            active_filelist: None,
            pending_filelist_intent: None,
            next_action_request_id: 0,
            active_action_request: None,
        }
    }

    fn status_line(&self) -> String {
        match &self.update_notice {
            Some(notice) => format!("{notice} | {}", self.status),
            None => self.status.clone(),
        }
    }

    fn set_results(&mut self, results: Vec<(PathBuf, f64)>, error: Option<String>) {
        let selected_path = self
            .results
            .get(self.selected)
            .map(|(path, _)| path.clone());
        self.results = results;
        self.selected = selected_path
            .as_ref()
            .and_then(|selected| self.results.iter().position(|(path, _)| path == selected))
            .unwrap_or(0);
        self.ensure_selection_visible();
        self.status = error.unwrap_or_else(|| {
            let result_status = format!(
                "{} result(s) | {}",
                self.results.len(),
                self.current_options_summary()
            );
            match self.index_truncated_limit {
                Some(limit) => format!("{} | {result_status}", walker_truncated_notice(limit)),
                None => result_status,
            }
        });
        self.dirty = true;
    }

    fn next_search_request(&mut self, root: PathBuf, limit: usize) -> SearchRequest {
        self.next_search_request_id = self.next_search_request_id.wrapping_add(1);
        let request_id = self.next_search_request_id;
        self.active_search_request_id = Some(request_id);
        SearchRequest {
            request_id,
            query: self.query.clone(),
            entries: self.entries.snapshot(),
            root,
            limit,
            options: self.runtime_options.search_options(self.sort_mode),
            ignore_terms: Arc::clone(&self.ignore_terms),
        }
    }

    fn next_index_request(&mut self, root: PathBuf) -> IndexRequest {
        self.next_index_request_id = self.next_index_request_id.wrapping_add(1);
        let request_id = self.next_index_request_id;
        self.active_index_request = Some((request_id, root.clone()));
        self.index_truncated_limit = None;
        self.indexed = false;
        self.root_filelist_known = false;
        self.root_filelist_exists = false;
        self.entries.clear();
        self.results.clear();
        self.selected = 0;
        self.offset = 0;
        self.clear_preview();
        self.active_search_request_id = None;
        self.status = "Indexing...".to_string();
        self.dirty = true;
        IndexRequest {
            request_id,
            root,
            include_files: self.runtime_options.include_files,
            include_dirs: self.runtime_options.include_dirs,
            source: self.runtime_options.source,
        }
    }

    fn ensure_selection_visible(&mut self) {
        if self.results.is_empty() {
            self.selected = 0;
            self.offset = 0;
            return;
        }
        self.selected = self.selected.min(self.results.len() - 1);
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + self.viewport_rows.max(1) {
            self.offset = self
                .selected
                .saturating_add(1)
                .saturating_sub(self.viewport_rows.max(1));
        }
        let max_offset = self.results.len().saturating_sub(self.viewport_rows.max(1));
        self.offset = self.offset.min(max_offset);
    }

    fn move_selection(&mut self, delta: isize) {
        if self.results.is_empty() {
            return;
        }
        self.selected = if delta.is_negative() {
            self.selected.saturating_sub(delta.unsigned_abs())
        } else {
            self.selected
                .saturating_add(delta as usize)
                .min(self.results.len() - 1)
        };
        self.ensure_selection_visible();
    }

    fn mark_query_changed(&mut self) {
        self.sort_mode = SearchSortMode::Score;
        self.last_query_change = Some(Instant::now());
    }

    fn current_options_summary(&self) -> String {
        format!(
            "Root: {} | Sort: {} | Source: {} | Files: {} | Folders: {} | Regex: {} | Ignore Case: {} | Ignore: {}",
            self.root.display(),
            self.sort_mode.label(),
            self.runtime_options.source.label(),
            if self.runtime_options.include_files { "on" } else { "off" },
            if self.runtime_options.include_dirs { "on" } else { "off" },
            if self.runtime_options.regex { "on" } else { "off" },
            if self.runtime_options.ignore_case { "on" } else { "off" },
            if self.runtime_options.ignore_enabled { "on" } else { "off" },
        )
    }

    fn open_options(&mut self) {
        self.options_overlay = Some(OptionsOverlay {
            draft: self.runtime_options,
            selected: 0,
        });
    }

    fn open_sort_picker(&mut self) {
        self.sort_picker = Some(SortPicker {
            selected: SORT_MODES
                .iter()
                .position(|mode| *mode == self.sort_mode)
                .unwrap_or(0),
        });
    }

    fn open_root_picker(&mut self) {
        self.root_picker = Some(RootPicker { selected: 0 });
    }

    fn open_filelist_confirmation(&mut self) {
        self.filelist_confirmation = Some(FileListConfirmation::Mode {
            propagate_to_ancestors: false,
        });
        self.dirty = true;
    }

    fn open_filelist_if_ready(&mut self) {
        if self.active_filelist.is_some() {
            return;
        }
        if self.root_filelist_known {
            self.open_filelist_confirmation();
        } else {
            self.status = "Wait for indexing to finish before creating FileList".to_string();
            self.dirty = true;
        }
    }

    fn next_filelist_request(
        &mut self,
        propagate_to_ancestors: bool,
        allow_root_overwrite: bool,
    ) -> TuiFileListRequest {
        self.next_filelist_request_id = self.next_filelist_request_id.wrapping_add(1);
        let request_id = self.next_filelist_request_id;
        let cancel = Arc::new(AtomicBool::new(false));
        self.active_filelist = Some(ActiveFileList {
            request_id,
            root: self.root.clone(),
            cancel: Arc::clone(&cancel),
        });
        self.status = "Creating FileList...".to_string();
        self.dirty = true;
        TuiFileListRequest {
            request_id,
            root: self.root.clone(),
            propagate_to_ancestors,
            allow_root_overwrite,
            cancel,
        }
    }

    fn cancel_active_filelist(&mut self) {
        if let Some(active) = self.active_filelist.as_ref() {
            active.cancel.store(true, Ordering::Release);
            self.status = "Canceling FileList creation...".to_string();
            self.dirty = true;
        }
    }

    fn record_filelist_intent(&mut self, intent: PendingFileListIntent) {
        let replace = match (&self.pending_filelist_intent, &intent) {
            (Some(PendingFileListIntent::CancelExit), _) => false,
            (_, PendingFileListIntent::CancelExit) => true,
            (_, PendingFileListIntent::SwitchRoot(_)) => true,
            (None, PendingFileListIntent::SelectOutput) => true,
            _ => false,
        };
        if replace {
            self.pending_filelist_intent = Some(intent);
        }
        self.cancel_active_filelist();
    }

    fn current_path(&self) -> Option<&PathBuf> {
        self.results.get(self.selected).map(|(path, _)| path)
    }

    fn clear_preview(&mut self) {
        self.preview.clear();
        self.active_preview_request = None;
    }

    fn next_preview_request(&mut self) -> Option<PreviewRequest> {
        if !self.preview_visible {
            self.clear_preview();
            return None;
        }
        let Some(path) = self.current_path().cloned() else {
            self.clear_preview();
            return None;
        };
        self.next_preview_request_id = self.next_preview_request_id.wrapping_add(1);
        let identity = PreviewRequestIdentity {
            request_id: self.next_preview_request_id,
            root: self.root.clone(),
            path: path.clone(),
        };
        self.active_preview_request = Some(identity.clone());
        self.preview = "Loading preview...".to_string();
        Some(PreviewRequest {
            request_id: identity.request_id,
            root: identity.root,
            path,
        })
    }

    fn begin_history(&mut self) {
        if !self.history_enabled || self.history.is_some() {
            return;
        }
        let mut history = HistoryOverlay {
            draft_query: self.query.clone(),
            filter: String::new(),
            filter_cursor: 0,
            results: Vec::new(),
            selected: 0,
            offset: 0,
        };
        refresh_history_results(&mut history, &self.history_entries);
        self.history = Some(history);
    }

    fn commit_query_to_history(&mut self) -> Option<String> {
        let query = self.query.trim().to_string();
        if query.is_empty() {
            return None;
        }
        self.history_entries.retain(|entry| entry != &query);
        self.history_entries.push(query.clone());
        while self.history_entries.len() > 100 {
            self.history_entries.remove(0);
        }
        Some(query)
    }

    fn cancel_history(&mut self) {
        if let Some(history) = self.history.take() {
            self.query = history.draft_query;
            self.query_cursor = self.query.chars().count();
        }
    }

    fn accept_history(&mut self) -> Option<String> {
        let history = self.history.take()?;
        let selected = history.results.get(history.selected)?.clone();
        self.query = selected.clone();
        self.query_cursor = self.query.chars().count();
        self.mark_query_changed();
        Some(selected)
    }

    fn open_help(&mut self) {
        self.help = Some(if self.active_filelist.is_some() {
            HelpContext::FileList
        } else if self.history.is_some() {
            HelpContext::History
        } else {
            HelpContext::Normal
        });
    }

    fn close_help(&mut self) {
        self.help = None;
    }

    fn next_action_request(
        &mut self,
        mode: AuthorizedActionMode,
        freshness: &TuiActionFreshness,
        cancellation: Arc<AtomicBool>,
    ) -> Option<TuiActionRequest> {
        let selected_path = self.current_path()?.clone();
        self.next_action_request_id = self.next_action_request_id.wrapping_add(1);
        let request_id = self.next_action_request_id;
        freshness.activate(request_id, &self.root);
        self.active_action_request = Some((request_id, selected_path.clone()));
        self.status = match mode {
            AuthorizedActionMode::ExecuteOrOpen => "Opening selected item...".to_string(),
            AuthorizedActionMode::Reveal => "Revealing selected item...".to_string(),
        };
        Some(TuiActionRequest {
            request: AuthorizedActionRequest::new_with_cancellation(
                request_id,
                self.root.clone(),
                vec![selected_path.clone()],
                mode,
                cancellation,
            ),
            selected_path,
        })
    }
}

trait TerminalOps {
    fn enable_raw_mode(&mut self) -> io::Result<()>;
    fn disable_raw_mode(&mut self) -> io::Result<()>;
    fn enter_alternate<W: Write>(&mut self, writer: &mut W) -> io::Result<()>;
    fn leave_alternate<W: Write>(&mut self, writer: &mut W) -> io::Result<()>;
    fn hide_cursor<W: Write>(&mut self, writer: &mut W) -> io::Result<()>;
    fn show_cursor<W: Write>(&mut self, writer: &mut W) -> io::Result<()>;
    fn enable_paste<W: Write>(&mut self, writer: &mut W) -> io::Result<()>;
    fn disable_paste<W: Write>(&mut self, writer: &mut W) -> io::Result<()>;
}

struct CrosstermOps;

impl TerminalOps for CrosstermOps {
    fn enable_raw_mode(&mut self) -> io::Result<()> {
        terminal::enable_raw_mode()
    }

    fn disable_raw_mode(&mut self) -> io::Result<()> {
        terminal::disable_raw_mode()
    }

    fn enter_alternate<W: Write>(&mut self, writer: &mut W) -> io::Result<()> {
        execute!(writer, EnterAlternateScreen)
    }

    fn leave_alternate<W: Write>(&mut self, writer: &mut W) -> io::Result<()> {
        execute!(writer, LeaveAlternateScreen)
    }

    fn hide_cursor<W: Write>(&mut self, writer: &mut W) -> io::Result<()> {
        execute!(writer, Hide)
    }

    fn show_cursor<W: Write>(&mut self, writer: &mut W) -> io::Result<()> {
        execute!(writer, Show)
    }

    fn enable_paste<W: Write>(&mut self, writer: &mut W) -> io::Result<()> {
        execute!(writer, EnableBracketedPaste)
    }

    fn disable_paste<W: Write>(&mut self, writer: &mut W) -> io::Result<()> {
        execute!(writer, DisableBracketedPaste)
    }
}

struct TerminalGuard<O: TerminalOps, W: Write> {
    ops: O,
    writer: W,
    raw_mode: bool,
    alternate_screen: bool,
    cursor_hidden: bool,
    bracketed_paste: bool,
}

impl<O: TerminalOps, W: Write> TerminalGuard<O, W> {
    fn start(ops: O, writer: W) -> Result<Self> {
        let mut guard = Self {
            ops,
            writer,
            raw_mode: false,
            alternate_screen: false,
            cursor_hidden: false,
            bracketed_paste: false,
        };
        guard
            .ops
            .enable_raw_mode()
            .context("failed to enable terminal raw mode")?;
        guard.raw_mode = true;
        guard
            .ops
            .enter_alternate(&mut guard.writer)
            .context("failed to enter alternate screen")?;
        guard.alternate_screen = true;
        guard
            .ops
            .hide_cursor(&mut guard.writer)
            .context("failed to hide terminal cursor")?;
        guard.cursor_hidden = true;
        guard
            .ops
            .enable_paste(&mut guard.writer)
            .context("failed to enable bracketed paste")?;
        guard.bracketed_paste = true;
        Ok(guard)
    }

    fn writer_mut(&mut self) -> &mut W {
        &mut self.writer
    }
}

impl<O: TerminalOps, W: Write> Drop for TerminalGuard<O, W> {
    fn drop(&mut self) {
        if self.bracketed_paste {
            let _ = self.ops.disable_paste(&mut self.writer);
            self.bracketed_paste = false;
        }
        if self.cursor_hidden {
            let _ = self.ops.show_cursor(&mut self.writer);
            self.cursor_hidden = false;
        }
        if self.alternate_screen {
            let _ = self.ops.leave_alternate(&mut self.writer);
            self.alternate_screen = false;
        }
        if self.raw_mode {
            let _ = self.ops.disable_raw_mode();
            self.raw_mode = false;
        }
    }
}

fn run_terminal_operation<O, W, T, F>(mut guard: TerminalGuard<O, W>, operation: F) -> Result<T>
where
    O: TerminalOps,
    W: Write,
    F: FnOnce(&mut W) -> Result<T>,
{
    let result = operation(guard.writer_mut());
    drop(guard);
    result
}

fn process_index_request<C, S>(request: IndexRequest, should_cancel: &C, send: S)
where
    C: Fn() -> bool,
    S: FnMut(WorkerResponse),
{
    let config = current_runtime_config();
    process_index_request_with_config(request, &config, should_cancel, send);
}

fn process_index_request_with_config<C, S>(
    request: IndexRequest,
    config: &RuntimeConfig,
    should_cancel: &C,
    mut send: S,
) where
    C: Fn() -> bool,
    S: FnMut(WorkerResponse),
{
    if should_cancel() {
        return;
    }
    match std::fs::metadata(&request.root) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => {
            send(WorkerResponse::IndexFailed {
                request_id: request.request_id,
                root: request.root,
                has_root_filelist: false,
                error: "selected root is not a directory".to_string(),
            });
            return;
        }
        Err(error) => {
            send(WorkerResponse::IndexFailed {
                request_id: request.request_id,
                root: request.root,
                has_root_filelist: false,
                error: format!("failed to read selected root: {error}"),
            });
            return;
        }
    }
    let has_filelist = find_filelist_in_first_level(&request.root).is_some();
    let use_filelist = match request.source {
        TuiSource::Auto => has_filelist,
        TuiSource::FileList => true,
        TuiSource::Walker => false,
    };
    if request.source == TuiSource::FileList && !has_filelist {
        if !should_cancel() {
            send(WorkerResponse::IndexFailed {
                request_id: request.request_id,
                root: request.root,
                has_root_filelist: false,
                error: "FileList source selected but no FileList was found".to_string(),
            });
        }
        return;
    }

    if use_filelist {
        match build_index_cancellable(
            &request.root,
            true,
            request.include_files,
            request.include_dirs,
            should_cancel,
        ) {
            Ok(paths) => {
                if !paths.is_empty() && !should_cancel() {
                    send(WorkerResponse::IndexedBatch {
                        request_id: request.request_id,
                        root: request.root.clone(),
                        entries: paths,
                    });
                }
            }
            Err(error) if is_index_build_cancelled(&error) => return,
            Err(error) => {
                if !should_cancel() {
                    send(WorkerResponse::IndexFailed {
                        request_id: request.request_id,
                        root: request.root,
                        has_root_filelist: has_filelist,
                        error: error.to_string(),
                    });
                }
                return;
            }
        }
    } else {
        let settings = walker_runtime_settings(config);
        let max_entries = settings.max_entries;
        let mut batch = Vec::with_capacity(256);
        let mut emitted_entries = 0usize;
        let mut truncated = false;
        walk_adaptive(
            &request.root,
            settings.adaptive_max_limit,
            settings.adaptive_initial_limit,
            |entry: AdaptiveWalkerEntry| {
                if should_cancel() {
                    return false;
                }
                if classify_walker_entry(
                    &entry.path,
                    entry.file_type,
                    request.include_files,
                    request.include_dirs,
                )
                .is_none()
                {
                    return true;
                }
                if emitted_entries >= max_entries {
                    truncated = true;
                    return false;
                }
                batch.push(entry.path);
                emitted_entries = emitted_entries.saturating_add(1);
                if batch.len() >= 256 && !should_cancel() {
                    send(WorkerResponse::IndexedBatch {
                        request_id: request.request_id,
                        root: request.root.clone(),
                        entries: std::mem::take(&mut batch),
                    });
                }
                true
            },
            should_cancel,
        );
        if should_cancel() {
            return;
        }
        if !batch.is_empty() {
            send(WorkerResponse::IndexedBatch {
                request_id: request.request_id,
                root: request.root.clone(),
                entries: batch,
            });
        }
        if truncated {
            send(WorkerResponse::IndexTruncated {
                request_id: request.request_id,
                root: request.root.clone(),
                limit: max_entries,
            });
        }
    }

    if !should_cancel() {
        send(WorkerResponse::IndexedFinished {
            request_id: request.request_id,
            root: request.root,
            has_root_filelist: has_filelist,
        });
    }
}

pub fn run_cli_tui(root: &Path, options: &CliTuiOptions) -> Result<CliTuiOutcome> {
    if !interactive_terminal_supported(io::stdin().is_terminal(), io::stderr().is_terminal()) {
        anyhow::bail!("--interactive requires terminal stdin and stderr");
    }
    if options.require_filelist && find_filelist_in_first_level(root).is_none() {
        anyhow::bail!(
            "FileList was required but none was found in {}",
            root.display()
        );
    }

    let persisted_roots_and_history = load_persisted_roots_and_history();
    let history_enabled = history_persistence_enabled();
    let history_entries = if history_enabled {
        persisted_roots_and_history.query_history
    } else {
        Vec::new()
    };
    let saved_roots = persisted_roots_and_history.saved_roots;
    let history_persistence = history_enabled
        .then(AsyncHistoryPersistence::new_default)
        .flatten();

    let guard = TerminalGuard::start(CrosstermOps, io::stderr())?;
    let root = root.to_path_buf();
    let cancelled = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    let (search_tx, search_rx) = mpsc::channel::<SearchRequest>();
    let (search_done_tx, search_done_rx) = mpsc::channel();
    let search_cancelled = Arc::clone(&cancelled);
    let response_tx = tx.clone();
    let search_handle = thread::Builder::new()
        .name("flistwalker-cli-search".to_string())
        .spawn(move || {
            let mut prefix_cache = SearchPrefixCache::default();
            while !search_cancelled.load(Ordering::Relaxed) {
                let mut request = match search_rx.recv_timeout(EVENT_POLL) {
                    Ok(request) => request,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                };
                while let Ok(newer) = search_rx.try_recv() {
                    request = newer;
                }
                let (results, error) = search(&request, &mut prefix_cache);
                if search_cancelled.load(Ordering::Relaxed)
                    || response_tx
                        .send(WorkerResponse::Searched {
                            request_id: request.request_id,
                            root: request.root,
                            query: request.query,
                            options: request.options,
                            results,
                            error,
                        })
                        .is_err()
                {
                    break;
                }
            }
            let _ = search_done_tx.send(());
        })
        .context("failed to start CLI search worker")?;

    let (preview_tx, preview_rx) = mpsc::channel::<PreviewRequest>();
    let (preview_done_tx, preview_done_rx) = mpsc::channel();
    let preview_cancelled = Arc::clone(&cancelled);
    let preview_response_tx = tx.clone();
    let preview_handle = match thread::Builder::new()
        .name("flistwalker-cli-preview".to_string())
        .spawn(move || {
            while !preview_cancelled.load(Ordering::Relaxed) {
                let mut request = match preview_rx.recv_timeout(EVENT_POLL) {
                    Ok(request) => request,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                };
                while let Ok(newer) = preview_rx.try_recv() {
                    request = newer;
                }
                let is_dir = request.path.is_dir();
                let preview = build_preview_text_with_kind(&request.path, is_dir);
                if preview_cancelled.load(Ordering::Relaxed)
                    || preview_response_tx
                        .send(WorkerResponse::Previewed {
                            request_id: request.request_id,
                            root: request.root,
                            path: request.path,
                            preview,
                        })
                        .is_err()
                {
                    break;
                }
            }
            let _ = preview_done_tx.send(());
        }) {
        Ok(handle) => handle,
        Err(error) => {
            cancelled.store(true, Ordering::Relaxed);
            drop(search_tx);
            finish_worker(search_handle, search_done_rx);
            return Err(error).context("failed to start CLI preview worker");
        }
    };

    let action_freshness = Arc::new(TuiActionFreshness::new());
    let (action_tx, action_rx) = mpsc::channel::<TuiActionRequest>();
    let (action_done_tx, action_done_rx) = mpsc::channel();
    let action_cancelled = Arc::clone(&cancelled);
    let action_response_tx = tx.clone();
    let action_worker_freshness = Arc::clone(&action_freshness);
    let action_handle = match thread::Builder::new()
        .name("flistwalker-cli-action".to_string())
        .spawn(move || {
            while !action_cancelled.load(Ordering::Acquire) {
                let mut action = match action_rx.recv_timeout(EVENT_POLL) {
                    Ok(action) => action,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                };
                while let Ok(newer) = action_rx.try_recv() {
                    action = newer;
                }
                let request_id = action.request.request_id;
                let root = action.request.trusted_root.clone();
                let selected_path = action.selected_path;
                let report = execute_authorized_action_request(
                    &action.request,
                    action_worker_freshness.as_ref(),
                    &TuiActionBackend,
                );
                if action_cancelled.load(Ordering::Acquire)
                    || action_response_tx
                        .send(WorkerResponse::Actioned {
                            request_id,
                            root,
                            selected_path,
                            report,
                        })
                        .is_err()
                {
                    break;
                }
            }
            let _ = action_done_tx.send(());
        }) {
        Ok(handle) => handle,
        Err(error) => {
            cancelled.store(true, Ordering::Release);
            drop(search_tx);
            drop(preview_tx);
            finish_worker(search_handle, search_done_rx);
            finish_worker(preview_handle, preview_done_rx);
            return Err(error).context("failed to start CLI action worker");
        }
    };

    let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
    let (index_done_tx, index_done_rx) = mpsc::channel();
    let worker_cancelled = Arc::clone(&cancelled);
    let worker_tx = tx.clone();
    let index_freshness = Arc::new(TuiIndexFreshness::new());
    let worker_index_freshness = Arc::clone(&index_freshness);
    let index_handle = match thread::Builder::new()
        .name("flistwalker-cli-index-search".to_string())
        .spawn(move || {
            while !worker_cancelled.load(Ordering::Relaxed) {
                let mut request = match index_rx.recv_timeout(EVENT_POLL) {
                    Ok(request) => request,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                };
                while let Ok(newer) = index_rx.try_recv() {
                    request = newer;
                }
                let request_id = request.request_id;
                let should_cancel = || {
                    worker_cancelled.load(Ordering::Relaxed)
                        || !worker_index_freshness.is_current(request_id)
                };
                process_index_request(request, &should_cancel, |response| {
                    let _ = worker_tx.send(response);
                });
            }
            let _ = index_done_tx.send(());
        }) {
        Ok(handle) => handle,
        Err(error) => {
            cancelled.store(true, Ordering::Relaxed);
            drop(search_tx);
            drop(preview_tx);
            drop(action_tx);
            finish_worker(search_handle, search_done_rx);
            finish_worker(preview_handle, preview_done_rx);
            finish_worker(action_handle, action_done_rx);
            return Err(error).context("failed to start CLI index worker");
        }
    };

    let result = run_terminal_operation(guard, |terminal_output| {
        run_event_loop(
            terminal_output,
            EventLoopContext {
                index_tx: &index_tx,
                index_freshness: Arc::clone(&index_freshness),
                search_tx: &search_tx,
                preview_tx: &preview_tx,
                action_tx: &action_tx,
                rx: &rx,
                root: root.clone(),
                saved_roots,
                options,
                history_enabled,
                history_entries,
                history_persistence: history_persistence.as_ref(),
                action_freshness: Arc::clone(&action_freshness),
                cancellation: Arc::clone(&cancelled),
            },
        )
    });
    cancelled.store(true, Ordering::Release);
    drop(search_tx);
    drop(index_tx);
    drop(preview_tx);
    drop(action_tx);
    finish_worker(search_handle, search_done_rx);
    finish_worker(preview_handle, preview_done_rx);
    finish_worker(action_handle, action_done_rx);
    finish_worker(index_handle, index_done_rx);

    if let Ok(TuiExit::Selected { query, .. }) = &result {
        if let Err(error) = enqueue_history_delta(history_persistence.as_ref(), query) {
            eprintln!("warning: failed to enqueue query history: {error}");
        }
    }
    if let Some(persistence) = history_persistence {
        if let Err(error) = persistence.shutdown(WORKER_JOIN_TIMEOUT) {
            eprintln!("warning: failed to persist query history: {error}");
        }
    }

    match result? {
        TuiExit::Cancelled => Ok(CliTuiOutcome::Cancelled),
        TuiExit::Failed(error) => anyhow::bail!(error),
        TuiExit::Selected { paths, root, .. } => Ok(CliTuiOutcome::Selected { paths, root }),
    }
}

fn finish_worker(handle: thread::JoinHandle<()>, done: mpsc::Receiver<()>) {
    if done.recv_timeout(WORKER_JOIN_TIMEOUT).is_ok() {
        let _ = handle.join();
    }
}

fn spawn_filelist_worker(request: TuiFileListRequest) -> Result<ActiveFileListWorker> {
    let cancel = Arc::clone(&request.cancel);
    let (done_tx, done) = mpsc::channel();
    let (result_tx, result) = mpsc::channel();
    let handle = thread::Builder::new()
        .name("flistwalker-cli-filelist".to_string())
        .spawn(move || {
            let request_id = request.request_id;
            let root = request.root.clone();
            let response = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let should_cancel = || request.cancel.load(Ordering::Acquire);
                let entries = match build_filelist_snapshot(&request.root, &should_cancel) {
                    Ok(entries) => entries,
                    Err(report) => {
                        return FileListWorkerResult::Finished {
                            request_id,
                            root: root.clone(),
                            report: *report,
                        };
                    }
                };
                let report = match plan_filelist_write_cancellable(
                    &request.root,
                    &entries,
                    FileListWriteOptions {
                        allow_root_overwrite: request.allow_root_overwrite,
                        propagate_to_ancestors: request.propagate_to_ancestors,
                    },
                    &should_cancel,
                ) {
                    Ok(plan) => execute_filelist_write_plan(&plan, &should_cancel),
                    Err(report) => *report,
                };
                FileListWorkerResult::Finished {
                    request_id,
                    root: root.clone(),
                    report,
                }
            }))
            .unwrap_or_else(|_| FileListWorkerResult::Failed {
                request_id,
                root,
                error: "FileList worker panicked".to_string(),
            });
            let _ = result_tx.send(response);
            let _ = done_tx.send(());
        })
        .context("failed to start CLI FileList worker")?;
    Ok(ActiveFileListWorker {
        cancel,
        result,
        done,
        handle: Some(handle),
    })
}

/// FileList creation must never inherit the TUI's currently displayed index: it
/// may be limited by the active source or file-kind filters.  Build the same
/// fresh, walker-only all-kinds snapshot used by the batch path instead.
fn build_filelist_snapshot<C>(
    root: &Path,
    should_cancel: &C,
) -> std::result::Result<Vec<PathBuf>, Box<FileListWriteReport>>
where
    C: Fn() -> bool,
{
    let entries = match build_index_cancellable(root, false, true, true, should_cancel) {
        Ok(entries) => entries,
        Err(error) if is_index_build_cancelled(&error) => {
            return Err(Box::new(canceled_filelist_report(root)));
        }
        Err(error) => {
            return Err(Box::new(FileListWriteReport {
                status: FileListWriteStatus::Failed,
                root_target: root.join("FileList.txt"),
                committed: Vec::new(),
                failed: vec![crate::indexer::FileListWriteFailure {
                    path: root.to_path_buf(),
                    error: error.to_string(),
                }],
                rolled_back: Vec::new(),
                rollback_failed: Vec::new(),
            }));
        }
    };
    Ok(entries
        .into_iter()
        .filter(|entry| !is_root_filelist_entry(root, entry))
        .collect())
}

fn canceled_filelist_report(root: &Path) -> FileListWriteReport {
    FileListWriteReport {
        status: FileListWriteStatus::Canceled,
        root_target: root.join("FileList.txt"),
        committed: Vec::new(),
        failed: Vec::new(),
        rolled_back: Vec::new(),
        rollback_failed: Vec::new(),
    }
}

fn is_root_filelist_entry(root: &Path, entry: &Path) -> bool {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let Ok(relative) = entry.strip_prefix(root) else {
        return false;
    };
    relative.components().count() == 1
        && relative
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("filelist.txt"))
}

fn run_event_loop<W: Write>(
    terminal_output: &mut W,
    context: EventLoopContext<'_>,
) -> Result<TuiExit> {
    let EventLoopContext {
        index_tx,
        index_freshness,
        search_tx,
        preview_tx,
        action_tx,
        rx,
        root,
        saved_roots,
        options,
        history_enabled,
        history_entries,
        history_persistence,
        action_freshness,
        cancellation,
    } = context;
    let mut state = TuiState::new(&options.initial_query);
    let runtime_config = current_runtime_config();
    state.emacs_keybindings_enabled = runtime_config.emacs_keybindings_enabled;
    state.tab_pin_moves_to_next_row = runtime_config.tab_pin_moves_to_next_row;
    state.root = root.clone();
    state.saved_roots = saved_roots;
    state.runtime_options = TuiRuntimeOptions::from_startup(options);
    state.sort_mode = options.sort_mode;
    state.ignore_terms = Arc::new(options.ignore_terms.clone());
    state.history_enabled = history_enabled;
    state.history_entries = history_entries;
    let update_rx = spawn_tui_update_check();
    if dispatch_current_index(&mut state, index_tx, index_freshness.as_ref()).is_err() {
        anyhow::bail!("index worker unavailable");
    }
    let mut filelist_worker: Option<ActiveFileListWorker> = None;
    loop {
        if let Ok(Some(candidate)) = update_rx.try_recv() {
            state.update_notice = Some(format_tui_update_notice(&candidate.target_version));
            state.dirty = true;
        }
        let filelist_result =
            filelist_worker
                .as_ref()
                .and_then(|worker| match worker.result.try_recv() {
                    Ok(result) => Some(Ok(result)),
                    Err(mpsc::TryRecvError::Disconnected) if worker.is_finished() => {
                        Some(Err("FileList worker disconnected".to_string()))
                    }
                    Err(mpsc::TryRecvError::Empty) if worker.is_finished() => {
                        Some(Err("FileList worker finished without a result".to_string()))
                    }
                    Err(_) => None,
                });
        if let Some(filelist_result) = filelist_result {
            if let Some(worker) = filelist_worker.take() {
                worker.join();
            }
            let settlement = match filelist_result {
                Ok(FileListWorkerResult::Finished {
                    request_id,
                    root,
                    report,
                }) => filelist_settlement_from_report(&mut state, request_id, &root, report),
                Ok(FileListWorkerResult::Failed {
                    request_id,
                    root,
                    error,
                }) => filelist_worker_failure(&mut state, request_id, &root, error),
                Err(error) => state
                    .active_filelist
                    .take()
                    .map(|_| FileListSettlement::Failed(error)),
            };
            if let Some(settlement) = settlement {
                if let Some(exit) = settle_filelist(
                    &mut state,
                    settlement,
                    index_tx,
                    index_freshness.as_ref(),
                    action_freshness.as_ref(),
                ) {
                    return Ok(exit);
                }
            }
        }
        let ready_responses = take_ready_responses(rx, MAX_WORKER_RESPONSES_PER_TICK);
        let worker_backlog = ready_responses.len() == MAX_WORKER_RESPONSES_PER_TICK;
        for response in ready_responses {
            let preview_path_before = state.current_path().cloned();
            apply_worker_response(&mut state, response)?;
            if preview_path_before != state.current_path().cloned() {
                request_preview_for_current(&mut state, preview_tx);
            }
        }

        let (width, height) = terminal::size()?;
        if update_preview_visibility(&mut state, width, height) {
            request_preview_for_current(&mut state, preview_tx);
        }

        if state.indexed
            && state
                .last_query_change
                .is_some_and(|at| at.elapsed() >= INPUT_DEBOUNCE)
        {
            state.last_query_change = None;
            state.status = "Searching...".to_string();
            state.dirty = true;
            let _ = search_tx.send(state.next_search_request(state.root.clone(), options.limit));
        }

        if state.dirty {
            draw(terminal_output, &mut state, options)?;
            state.dirty = false;
        }
        let poll_timeout = if worker_backlog {
            Duration::ZERO
        } else {
            EVENT_POLL
        };
        if event::poll(poll_timeout)? {
            match event::read()? {
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    let preview_path_before = state.current_path().cloned();
                    let preview_preferred_before = state.preview_preferred;
                    match handle_key(&mut state, key) {
                        KeyAction::Cancel => {
                            if state.active_filelist.is_some() {
                                state.record_filelist_intent(PendingFileListIntent::CancelExit);
                                continue;
                            }
                            cancellation.store(true, Ordering::Release);
                            return Ok(TuiExit::Cancelled);
                        }
                        KeyAction::Select => {
                            if state.active_filelist.is_some() {
                                state.record_filelist_intent(PendingFileListIntent::SelectOutput);
                                continue;
                            }
                            cancellation.store(true, Ordering::Release);
                            return Ok(TuiExit::Selected {
                                paths: selected_paths(&state),
                                query: state.query.clone(),
                                root: state.root.clone(),
                            });
                        }
                        KeyAction::HistoryApplied => {
                            if let Err(error) =
                                enqueue_history_delta(history_persistence, &state.query)
                            {
                                state.status = format!("History persistence unavailable: {error}");
                                state.dirty = true;
                            }
                        }
                        KeyAction::HistoryOpened(query) => {
                            if let Some(query) = query {
                                if let Err(error) =
                                    enqueue_history_delta(history_persistence, &query)
                                {
                                    state.status =
                                        format!("History persistence unavailable: {error}");
                                    state.dirty = true;
                                }
                            }
                        }
                        KeyAction::DispatchAction(mode) => {
                            if let Some(request) = state.next_action_request(
                                mode,
                                action_freshness.as_ref(),
                                Arc::clone(&cancellation),
                            ) {
                                if action_tx.send(request).is_err() {
                                    state.active_action_request = None;
                                    state.status = "Action worker unavailable".to_string();
                                }
                                state.dirty = true;
                            } else {
                                state.status = "No selection".to_string();
                                state.dirty = true;
                            }
                        }
                        KeyAction::Reindex => {
                            if state.source_changed_on_apply {
                                prepare_source_transition(
                                    &mut state,
                                    action_freshness.as_ref(),
                                    &root,
                                );
                            }
                            if dispatch_current_index(
                                &mut state,
                                index_tx,
                                index_freshness.as_ref(),
                            )
                            .is_err()
                            {
                                state.status = "Index worker unavailable".to_string();
                                state.dirty = true;
                            }
                        }
                        KeyAction::Refresh => {
                            if state.active_filelist.is_some() {
                                continue;
                            }
                            prepare_refresh(&mut state);
                            if dispatch_current_index(
                                &mut state,
                                index_tx,
                                index_freshness.as_ref(),
                            )
                            .is_err()
                            {
                                state.status = "Index worker unavailable".to_string();
                                state.dirty = true;
                            }
                        }
                        KeyAction::SwitchRoot(new_root) => {
                            if state.active_filelist.is_some() {
                                state.record_filelist_intent(PendingFileListIntent::SwitchRoot(
                                    new_root,
                                ));
                                continue;
                            }
                            prepare_root_switch(&mut state, action_freshness.as_ref(), new_root);
                            if dispatch_current_index(
                                &mut state,
                                index_tx,
                                index_freshness.as_ref(),
                            )
                            .is_err()
                            {
                                state.status = "Index worker unavailable".to_string();
                                state.dirty = true;
                            }
                        }
                        KeyAction::OpenFileList => {
                            state.open_filelist_if_ready();
                        }
                        KeyAction::StartFileList {
                            propagate_to_ancestors,
                            allow_root_overwrite,
                        } => {
                            let request = state.next_filelist_request(
                                propagate_to_ancestors,
                                allow_root_overwrite,
                            );
                            match spawn_filelist_worker(request) {
                                Ok(worker) => filelist_worker = Some(worker),
                                Err(error) => {
                                    state.active_filelist = None;
                                    state.status = format!("FileList worker unavailable: {error}");
                                    state.dirty = true;
                                }
                            }
                        }
                        KeyAction::Continue => {
                            if preview_path_before != state.current_path().cloned()
                                || preview_preferred_before != state.preview_preferred
                            {
                                request_preview_for_current(&mut state, preview_tx);
                            }
                        }
                    }
                }
                Event::Paste(text) => insert_paste(&mut state, &text),
                Event::Resize(_, _) => state.dirty = true,
                _ => {}
            }
        }
    }
}

fn dispatch_index_request(
    state: &mut TuiState,
    index_tx: &mpsc::Sender<IndexRequest>,
    freshness: &TuiIndexFreshness,
    root: PathBuf,
) -> Result<(), mpsc::SendError<IndexRequest>> {
    let request = state.next_index_request(root);
    freshness.activate(request.request_id);
    index_tx.send(request)
}

fn take_ready_responses<T>(rx: &mpsc::Receiver<T>, limit: usize) -> Vec<T> {
    rx.try_iter().take(limit).collect()
}

fn dispatch_current_index(
    state: &mut TuiState,
    index_tx: &mpsc::Sender<IndexRequest>,
    freshness: &TuiIndexFreshness,
) -> Result<(), mpsc::SendError<IndexRequest>> {
    dispatch_index_request(state, index_tx, freshness, state.root.clone())
}

fn prepare_source_transition(
    state: &mut TuiState,
    action_freshness: &TuiActionFreshness,
    root: &Path,
) {
    state.pinned.clear();
    state.clear_preview();
    state.active_action_request = None;
    action_freshness.activate(0, root);
    state.source_changed_on_apply = false;
}

fn prepare_root_switch(state: &mut TuiState, action_freshness: &TuiActionFreshness, root: PathBuf) {
    state.root = root.clone();
    state.pinned.clear();
    state.clear_preview();
    state.active_search_request_id = None;
    state.sort_mode = SearchSortMode::Score;
    state.active_action_request = None;
    action_freshness.activate(0, &root);
    state.status = format!("Switching root to {}...", root.display());
    state.dirty = true;
}

fn prepare_refresh(state: &mut TuiState) {
    state.sort_mode = SearchSortMode::Score;
    state.active_search_request_id = None;
    state.status = format!("Refreshing {}...", state.root.display());
    state.dirty = true;
}

enum FileListSettlement {
    Completed,
    Canceled,
    Failed(String),
}

fn filelist_settlement_from_report(
    state: &mut TuiState,
    request_id: u64,
    root: &Path,
    report: FileListWriteReport,
) -> Option<FileListSettlement> {
    let active = state.active_filelist.as_ref()?;
    if active.request_id != request_id || active.root.as_path() != root {
        return None;
    }
    state.active_filelist = None;
    Some(match report.status {
        FileListWriteStatus::Completed => FileListSettlement::Completed,
        FileListWriteStatus::Canceled if report.exit_code() == 130 => FileListSettlement::Canceled,
        FileListWriteStatus::Canceled | FileListWriteStatus::Failed => {
            FileListSettlement::Failed(report.summary())
        }
    })
}

fn filelist_worker_failure(
    state: &mut TuiState,
    request_id: u64,
    root: &Path,
    error: String,
) -> Option<FileListSettlement> {
    let active = state.active_filelist.as_ref()?;
    if active.request_id != request_id || active.root.as_path() != root {
        return None;
    }
    state.active_filelist = None;
    Some(FileListSettlement::Failed(error))
}

fn settle_filelist(
    state: &mut TuiState,
    settlement: FileListSettlement,
    index_tx: &mpsc::Sender<IndexRequest>,
    index_freshness: &TuiIndexFreshness,
    action_freshness: &TuiActionFreshness,
) -> Option<TuiExit> {
    let intent = state.pending_filelist_intent.take();
    match settlement {
        FileListSettlement::Failed(error) => {
            state.status = format!("FileList creation failed: {error}");
            state.dirty = true;
            if intent == Some(PendingFileListIntent::CancelExit) {
                return Some(TuiExit::Failed(error));
            }
        }
        FileListSettlement::Completed | FileListSettlement::Canceled => {
            let completed = matches!(settlement, FileListSettlement::Completed);
            state.status = if completed {
                "FileList created; refreshing...".to_string()
            } else {
                "FileList creation canceled".to_string()
            };
            state.dirty = true;
            match intent {
                Some(PendingFileListIntent::CancelExit) => return Some(TuiExit::Cancelled),
                Some(PendingFileListIntent::SelectOutput) => {
                    return Some(TuiExit::Selected {
                        paths: selected_paths(state),
                        query: state.query.clone(),
                        root: state.root.clone(),
                    });
                }
                Some(PendingFileListIntent::SwitchRoot(root)) => {
                    prepare_root_switch(state, action_freshness, root);
                    if dispatch_current_index(state, index_tx, index_freshness).is_err() {
                        state.status = "Index worker unavailable".to_string();
                        state.dirty = true;
                    }
                }
                None if completed => {
                    prepare_refresh(state);
                    if dispatch_current_index(state, index_tx, index_freshness).is_err() {
                        state.status = "Index worker unavailable".to_string();
                        state.dirty = true;
                    }
                }
                None => {}
            }
        }
    }
    None
}

fn apply_worker_response(state: &mut TuiState, response: WorkerResponse) -> Result<()> {
    match response {
        WorkerResponse::IndexedBatch {
            request_id,
            root,
            entries,
        } => {
            if state.active_index_request.as_ref() != Some(&(request_id, root)) {
                return Ok(());
            }
            state.entries.push(entries);
            state.indexed = true;
            let now = Instant::now();
            if state
                .last_incremental_search
                .is_none_or(|last| now.duration_since(last) >= INDEX_REFRESH_THROTTLE)
            {
                state.last_query_change = Some(now.checked_sub(INPUT_DEBOUNCE).unwrap_or(now));
                state.last_incremental_search = Some(now);
            }
            state.status = format!("Indexing... {} candidates", state.entries.len());
            state.dirty = true;
        }
        WorkerResponse::IndexedFinished {
            request_id,
            root,
            has_root_filelist,
        } => {
            if state.active_index_request.as_ref() != Some(&(request_id, root)) {
                return Ok(());
            }
            state.indexed = true;
            state.root_filelist_known = true;
            state.root_filelist_exists = has_root_filelist;
            state.status = state
                .index_truncated_limit
                .map(walker_truncated_notice)
                .unwrap_or_else(|| format!("Ready | {}", state.current_options_summary()));
            state.last_query_change = Some(
                Instant::now()
                    .checked_sub(INPUT_DEBOUNCE)
                    .unwrap_or_else(Instant::now),
            );
            state.dirty = true;
        }
        WorkerResponse::IndexTruncated {
            request_id,
            root,
            limit,
        } => {
            if state.active_index_request.as_ref() != Some(&(request_id, root)) {
                return Ok(());
            }
            state.index_truncated_limit = Some(limit);
            state.status = walker_truncated_notice(limit);
            state.dirty = true;
        }
        WorkerResponse::IndexFailed {
            request_id,
            root,
            has_root_filelist,
            error,
        } => {
            if state.active_index_request.as_ref() == Some(&(request_id, root)) {
                state.active_index_request = None;
                state.indexed = false;
                state.root_filelist_known = true;
                state.root_filelist_exists = has_root_filelist;
                state.status = format!("Indexing failed: {error}. Adjust options in F2 and retry.");
                state.dirty = true;
            }
        }
        WorkerResponse::Searched {
            request_id,
            root,
            query,
            options,
            results,
            error,
        } => apply_search_response(state, request_id, &root, &query, options, results, error),
        WorkerResponse::Previewed {
            request_id,
            root,
            path,
            preview,
        } => apply_preview_response(state, request_id, &root, &path, preview),
        WorkerResponse::Actioned {
            request_id,
            root,
            selected_path,
            report,
        } => apply_action_response(state, request_id, &root, &selected_path, &report),
    }
    Ok(())
}

fn apply_action_response(
    state: &mut TuiState,
    request_id: u64,
    root: &Path,
    selected_path: &Path,
    report: &AuthorizedActionReport,
) {
    if state
        .active_action_request
        .as_ref()
        .is_none_or(|(active_id, active_path)| {
            *active_id != request_id || active_path.as_path() != selected_path
        })
        || state.root.as_path() != root
    {
        return;
    }
    state.active_action_request = None;
    state.status = tui_action_status(report);
    state.dirty = true;
}

fn tui_action_status(report: &AuthorizedActionReport) -> String {
    match report.outcome {
        AuthorizedActionOutcome::Completed => "Action completed".to_string(),
        AuthorizedActionOutcome::Blocked => format!(
            "Action blocked: {}",
            report
                .diagnostic
                .as_deref()
                .unwrap_or("authorization failed")
        ),
        AuthorizedActionOutcome::Canceled | AuthorizedActionOutcome::Superseded => {
            "Action canceled".to_string()
        }
        AuthorizedActionOutcome::Failed | AuthorizedActionOutcome::PartialFailure => {
            "Action failed: executor failed".to_string()
        }
    }
}

fn interactive_terminal_supported(stdin_is_tty: bool, stderr_is_tty: bool) -> bool {
    stdin_is_tty && stderr_is_tty
}

fn apply_search_response(
    state: &mut TuiState,
    request_id: u64,
    root: &Path,
    query: &str,
    options: SearchOptions,
    results: Vec<(PathBuf, f64)>,
    error: Option<String>,
) {
    if state.active_search_request_id == Some(request_id)
        && state.root.as_path() == root
        && query == state.query
        && options == state.runtime_options.search_options(state.sort_mode)
    {
        state.set_results(results, error);
    }
}

fn preview_visible_for_size(preferred: bool, width: u16, height: u16) -> bool {
    preferred && width >= PREVIEW_MIN_WIDTH && height >= PREVIEW_MIN_HEIGHT
}

fn update_preview_visibility(state: &mut TuiState, width: u16, height: u16) -> bool {
    let visible = preview_visible_for_size(state.preview_preferred, width, height);
    if state.preview_visible == visible {
        return false;
    }
    state.preview_visible = visible;
    state.clear_preview();
    state.dirty = true;
    visible
}

fn request_preview_for_current(state: &mut TuiState, preview_tx: &mpsc::Sender<PreviewRequest>) {
    let Some(request) = state.next_preview_request() else {
        state.dirty = true;
        return;
    };
    if preview_tx.send(request).is_err() {
        state.preview = "<preview unavailable>".to_string();
        state.active_preview_request = None;
    }
    state.dirty = true;
}

fn apply_preview_response(
    state: &mut TuiState,
    request_id: u64,
    root: &Path,
    path: &Path,
    preview: String,
) {
    let expected = PreviewRequestIdentity {
        request_id,
        root: root.to_path_buf(),
        path: path.to_path_buf(),
    };
    if state.preview_visible
        && state.active_preview_request.as_ref() == Some(&expected)
        && state.root.as_path() == root
        && state.current_path().is_some_and(|current| current == path)
    {
        state.preview = preview;
        state.active_preview_request = None;
        state.dirty = true;
    }
}

fn history_search_score(query: &str, candidate: &str, recency_rank: usize) -> Option<i64> {
    if query.trim().is_empty() {
        return Some(recency_rank as i64);
    }
    let matcher = SkimMatcherV2::default();
    matcher.fuzzy_match(candidate, query).or_else(|| {
        let query_lower = query.to_ascii_lowercase();
        let candidate_lower = candidate.to_ascii_lowercase();
        candidate_lower
            .contains(&query_lower)
            .then_some((query_lower.len() as i64) * 100 + recency_rank as i64)
    })
}

fn refresh_history_results(history: &mut HistoryOverlay, entries: &[String]) {
    let mut scored = entries
        .iter()
        .rev()
        .enumerate()
        .filter_map(|(index, entry)| {
            history_search_score(
                history.filter.trim(),
                entry,
                entries.len().saturating_sub(index),
            )
            .map(|score| (entry.clone(), score, index))
        })
        .collect::<Vec<_>>();
    scored.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.2.cmp(&right.2)));
    history.results = scored.into_iter().map(|(entry, _, _)| entry).collect();
    history.selected = 0;
    history.offset = 0;
}

fn history_move_selection(history: &mut HistoryOverlay, delta: isize, viewport_rows: usize) {
    if history.results.is_empty() {
        history.selected = 0;
        history.offset = 0;
        return;
    }
    history.selected = if delta.is_negative() {
        history.selected.saturating_sub(delta.unsigned_abs())
    } else {
        history
            .selected
            .saturating_add(delta as usize)
            .min(history.results.len() - 1)
    };
    let viewport_rows = viewport_rows.max(1);
    if history.selected < history.offset {
        history.offset = history.selected;
    } else if history.selected >= history.offset + viewport_rows {
        history.offset = history.selected + 1 - viewport_rows;
    }
    history.offset = history
        .offset
        .min(history.results.len().saturating_sub(viewport_rows));
}

fn edit_history_filter(history: &mut HistoryOverlay, entries: &[String], key: KeyCode) -> bool {
    match key {
        KeyCode::Backspace if history.filter_cursor > 0 => {
            let start = char_to_byte_index(&history.filter, history.filter_cursor - 1);
            let end = char_to_byte_index(&history.filter, history.filter_cursor);
            history.filter.replace_range(start..end, "");
            history.filter_cursor -= 1;
        }
        KeyCode::Delete if history.filter_cursor < history.filter.chars().count() => {
            let start = char_to_byte_index(&history.filter, history.filter_cursor);
            let end = char_to_byte_index(&history.filter, history.filter_cursor + 1);
            history.filter.replace_range(start..end, "");
        }
        KeyCode::Left => history.filter_cursor = history.filter_cursor.saturating_sub(1),
        KeyCode::Right => {
            history.filter_cursor = (history.filter_cursor + 1).min(history.filter.chars().count())
        }
        KeyCode::Home => history.filter_cursor = 0,
        KeyCode::End => history.filter_cursor = history.filter.chars().count(),
        KeyCode::Char(ch) if !ch.is_control() => {
            let byte_index = char_to_byte_index(&history.filter, history.filter_cursor);
            history.filter.insert(byte_index, ch);
            history.filter_cursor += 1;
        }
        _ => return false,
    }
    refresh_history_results(history, entries);
    true
}

fn enqueue_history_delta(
    persistence: Option<&AsyncHistoryPersistence>,
    query: &str,
) -> Result<(), String> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(());
    }
    if let Some(persistence) = persistence {
        persistence.enqueue_history(vec![query.to_string()])?;
    }
    Ok(())
}

fn selected_paths(state: &TuiState) -> Vec<PathBuf> {
    if !state.pinned.is_empty() {
        return state.pinned.clone();
    }
    state
        .results
        .get(state.selected)
        .map(|(path, _)| vec![path.clone()])
        .unwrap_or_default()
}

fn move_overlay_selection(selected: &mut usize, delta: isize, len: usize) {
    if delta.is_negative() {
        *selected = selected.saturating_sub(delta.unsigned_abs());
    } else {
        *selected = selected
            .saturating_add(delta as usize)
            .min(len.saturating_sub(1));
    }
}

fn toggle_option(overlay: &mut OptionsOverlay) {
    match overlay.selected {
        0 if overlay.draft.include_dirs => {
            overlay.draft.include_files = !overlay.draft.include_files
        }
        1 if overlay.draft.include_files => {
            overlay.draft.include_dirs = !overlay.draft.include_dirs
        }
        2 => overlay.draft.regex = !overlay.draft.regex,
        3 => overlay.draft.ignore_case = !overlay.draft.ignore_case,
        4 => overlay.draft.ignore_enabled = !overlay.draft.ignore_enabled,
        5 => overlay.draft.source = overlay.draft.source.next(),
        _ => {}
    }
}

fn option_change_requires_reindex(before: TuiRuntimeOptions, after: TuiRuntimeOptions) -> bool {
    before.include_files != after.include_files
        || before.include_dirs != after.include_dirs
        || before.source != after.source
}

fn handle_options_key(state: &mut TuiState, key: KeyEvent) -> KeyAction {
    if matches!(
        (key.code, key.modifiers),
        (KeyCode::Char('c'), KeyModifiers::CONTROL)
    ) {
        return KeyAction::Cancel;
    }
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) | (KeyCode::Char('g'), KeyModifiers::CONTROL) => {
            state.options_overlay = None;
        }
        (KeyCode::Enter, _) => {
            let Some(overlay) = state.options_overlay.take() else {
                return KeyAction::Continue;
            };
            let previous = state.runtime_options;
            let changed = previous != overlay.draft;
            let reindex = option_change_requires_reindex(previous, overlay.draft);
            let source_changed = previous.source != overlay.draft.source;
            state.runtime_options = overlay.draft;
            if changed {
                state.sort_mode = SearchSortMode::Score;
                state.active_search_request_id = None;
                state.source_changed_on_apply = source_changed;
            }
            if reindex {
                state.status = "Reindexing...".to_string();
                state.dirty = true;
                return KeyAction::Reindex;
            }
            state.status = "Options applied".to_string();
            state.last_query_change = Some(Instant::now());
        }
        (KeyCode::Up, _) => {
            if let Some(overlay) = state.options_overlay.as_mut() {
                move_overlay_selection(&mut overlay.selected, -1, 6);
            }
        }
        (KeyCode::Down, _) => {
            if let Some(overlay) = state.options_overlay.as_mut() {
                move_overlay_selection(&mut overlay.selected, 1, 6);
            }
        }
        (KeyCode::Char(' '), _) | (KeyCode::Left, _) | (KeyCode::Right, _) => {
            if let Some(overlay) = state.options_overlay.as_mut() {
                toggle_option(overlay);
            }
        }
        _ => {}
    }
    state.dirty = true;
    KeyAction::Continue
}

fn handle_sort_key(state: &mut TuiState, key: KeyEvent) -> KeyAction {
    if matches!(
        (key.code, key.modifiers),
        (KeyCode::Char('c'), KeyModifiers::CONTROL)
    ) {
        return KeyAction::Cancel;
    }
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) | (KeyCode::Char('g'), KeyModifiers::CONTROL) => state.sort_picker = None,
        (KeyCode::Enter, _) => {
            if let Some(picker) = state.sort_picker.take() {
                state.sort_mode = SORT_MODES[picker.selected];
                state.status = format!("Sorting by {}...", state.sort_mode.label());
                state.last_query_change = Some(Instant::now());
            }
        }
        (KeyCode::Up, _) => {
            if let Some(picker) = state.sort_picker.as_mut() {
                move_overlay_selection(&mut picker.selected, -1, SORT_MODES.len());
            }
        }
        (KeyCode::Down, _) => {
            if let Some(picker) = state.sort_picker.as_mut() {
                move_overlay_selection(&mut picker.selected, 1, SORT_MODES.len());
            }
        }
        _ => {}
    }
    state.dirty = true;
    KeyAction::Continue
}

fn handle_root_picker_key(state: &mut TuiState, key: KeyEvent) -> KeyAction {
    if matches!(
        (key.code, key.modifiers),
        (KeyCode::Char('c'), KeyModifiers::CONTROL)
    ) {
        return KeyAction::Cancel;
    }
    let Some(picker) = state.root_picker.as_mut() else {
        return KeyAction::Continue;
    };
    if state.saved_roots.is_empty() {
        if matches!(
            (key.code, key.modifiers),
            (KeyCode::Enter, _) | (KeyCode::Esc, _) | (KeyCode::Char('g'), KeyModifiers::CONTROL)
        ) {
            state.root_picker = None;
        }
        state.dirty = true;
        return KeyAction::Continue;
    }
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) | (KeyCode::Char('g'), KeyModifiers::CONTROL) => state.root_picker = None,
        (KeyCode::Enter, _) => {
            let root = state.saved_roots[picker.selected].clone();
            state.root_picker = None;
            state.dirty = true;
            return KeyAction::SwitchRoot(root);
        }
        (KeyCode::Up, _) => {
            move_overlay_selection(&mut picker.selected, -1, state.saved_roots.len())
        }
        (KeyCode::Down, _) => {
            move_overlay_selection(&mut picker.selected, 1, state.saved_roots.len())
        }
        (KeyCode::PageUp, _) => move_overlay_selection(
            &mut picker.selected,
            -(state.viewport_rows.max(1) as isize),
            state.saved_roots.len(),
        ),
        (KeyCode::PageDown, _) => move_overlay_selection(
            &mut picker.selected,
            state.viewport_rows.max(1) as isize,
            state.saved_roots.len(),
        ),
        _ => {}
    }
    state.dirty = true;
    KeyAction::Continue
}

fn handle_filelist_confirmation_key(state: &mut TuiState, key: KeyEvent) -> KeyAction {
    let Some(confirmation) = state.filelist_confirmation.as_mut() else {
        return KeyAction::Continue;
    };
    if matches!(
        (key.code, key.modifiers),
        (KeyCode::Char('c'), KeyModifiers::CONTROL)
    ) {
        return KeyAction::Cancel;
    }
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) | (KeyCode::Char('g'), KeyModifiers::CONTROL) => {
            state.filelist_confirmation = None;
        }
        (KeyCode::Enter, _) => match confirmation {
            FileListConfirmation::Mode {
                propagate_to_ancestors,
            } if state.root_filelist_known && state.root_filelist_exists => {
                let propagate_to_ancestors = *propagate_to_ancestors;
                state.filelist_confirmation = Some(FileListConfirmation::Overwrite {
                    propagate_to_ancestors,
                });
            }
            FileListConfirmation::Mode {
                propagate_to_ancestors,
            } => {
                let propagate_to_ancestors = *propagate_to_ancestors;
                state.filelist_confirmation = None;
                state.dirty = true;
                return KeyAction::StartFileList {
                    propagate_to_ancestors,
                    allow_root_overwrite: false,
                };
            }
            FileListConfirmation::Overwrite {
                propagate_to_ancestors,
            } => {
                let propagate_to_ancestors = *propagate_to_ancestors;
                state.filelist_confirmation = None;
                state.dirty = true;
                return KeyAction::StartFileList {
                    propagate_to_ancestors,
                    allow_root_overwrite: true,
                };
            }
        },
        (KeyCode::Up, _) | (KeyCode::Down, _) | (KeyCode::Char(' '), _) => {
            if let FileListConfirmation::Mode {
                propagate_to_ancestors,
            } = confirmation
            {
                *propagate_to_ancestors = !*propagate_to_ancestors;
            }
        }
        _ => {}
    }
    state.dirty = true;
    KeyAction::Continue
}

fn handle_active_filelist_key(state: &mut TuiState, key: KeyEvent) -> KeyAction {
    match (key.code, key.modifiers) {
        (KeyCode::Char('c'), KeyModifiers::CONTROL)
        | (KeyCode::Char('g'), KeyModifiers::CONTROL)
        | (KeyCode::Esc, _) => KeyAction::Cancel,
        (KeyCode::F(1), _) => {
            state.open_help();
            state.dirty = true;
            KeyAction::Continue
        }
        (KeyCode::F(4), _) => {
            state.open_root_picker();
            state.dirty = true;
            KeyAction::Continue
        }
        (KeyCode::Enter, _) => KeyAction::Select,
        (KeyCode::Up, _) => {
            state.move_selection(-1);
            state.dirty = true;
            KeyAction::Continue
        }
        (KeyCode::Down, _) => {
            state.move_selection(1);
            state.dirty = true;
            KeyAction::Continue
        }
        (KeyCode::PageUp, _) => {
            state.move_selection(-(state.viewport_rows.max(1) as isize));
            state.dirty = true;
            KeyAction::Continue
        }
        (KeyCode::PageDown, _) => {
            state.move_selection(state.viewport_rows.max(1) as isize);
            state.dirty = true;
            KeyAction::Continue
        }
        _ => KeyAction::Continue,
    }
}

fn is_emacs_shortcut(key: KeyEvent) -> bool {
    match (key.code, key.modifiers) {
        (KeyCode::Char(ch), KeyModifiers::CONTROL) => matches!(
            ch.to_ascii_lowercase(),
            'a' | 'b'
                | 'd'
                | 'e'
                | 'f'
                | 'g'
                | 'h'
                | 'i'
                | 'j'
                | 'k'
                | 'm'
                | 'n'
                | 'p'
                | 'r'
                | 'u'
                | 'v'
                | 'w'
                | 'y'
        ),
        (KeyCode::Char(ch), KeyModifiers::ALT) => ch.eq_ignore_ascii_case(&'v'),
        _ => false,
    }
}

fn normalize_emacs_shortcut(key: KeyEvent) -> KeyEvent {
    let code = match (key.code, key.modifiers) {
        (KeyCode::Char(ch), KeyModifiers::CONTROL) => match ch.to_ascii_lowercase() {
            'n' => Some(KeyCode::Down),
            'p' => Some(KeyCode::Up),
            'v' => Some(KeyCode::PageDown),
            'i' => Some(KeyCode::Tab),
            'j' | 'm' => Some(KeyCode::Enter),
            _ => None,
        },
        (KeyCode::Char(ch), KeyModifiers::ALT) if ch.eq_ignore_ascii_case(&'v') => {
            Some(KeyCode::PageUp)
        }
        _ => None,
    };
    code.map_or(key, |code| KeyEvent::new(code, KeyModifiers::NONE))
}

fn apply_emacs_text_editing(
    text: &mut String,
    cursor: &mut usize,
    kill_buffer: &mut String,
    key: KeyEvent,
) -> Option<bool> {
    let (KeyCode::Char(ch), KeyModifiers::CONTROL) = (key.code, key.modifiers) else {
        return None;
    };
    let char_len = text.chars().count();
    let mut changed = false;
    match ch.to_ascii_lowercase() {
        'a' => *cursor = 0,
        'e' => *cursor = char_len,
        'b' => *cursor = cursor.saturating_sub(1),
        'f' => *cursor = (*cursor + 1).min(char_len),
        'h' if *cursor > 0 => {
            let start = char_to_byte_index(text, *cursor - 1);
            let end = char_to_byte_index(text, *cursor);
            text.replace_range(start..end, "");
            *cursor -= 1;
            changed = true;
        }
        'd' if *cursor < char_len => {
            let start = char_to_byte_index(text, *cursor);
            let end = char_to_byte_index(text, *cursor + 1);
            text.replace_range(start..end, "");
            changed = true;
        }
        'w' if *cursor > 0 => {
            let chars: Vec<char> = text.chars().collect();
            let mut start = *cursor;
            while start > 0 && chars[start - 1].is_whitespace() {
                start -= 1;
            }
            while start > 0 && !chars[start - 1].is_whitespace() {
                start -= 1;
            }
            let start_byte = char_to_byte_index(text, start);
            let end_byte = char_to_byte_index(text, *cursor);
            *kill_buffer = text[start_byte..end_byte].to_string();
            text.replace_range(start_byte..end_byte, "");
            *cursor = start;
            changed = true;
        }
        'k' if *cursor < char_len => {
            let start = char_to_byte_index(text, *cursor);
            *kill_buffer = text[start..].to_string();
            text.truncate(start);
            changed = true;
        }
        'y' if !kill_buffer.is_empty() => {
            let byte_index = char_to_byte_index(text, *cursor);
            text.insert_str(byte_index, kill_buffer);
            *cursor += kill_buffer.chars().count();
            changed = true;
        }
        'u' if *cursor > 0 => {
            let end = char_to_byte_index(text, *cursor);
            text.replace_range(..end, "");
            *cursor = 0;
            changed = true;
        }
        'd' | 'h' | 'k' | 'u' | 'w' | 'y' => {}
        _ => return None,
    }
    Some(changed)
}

fn apply_emacs_query_editing(state: &mut TuiState, key: KeyEvent) -> bool {
    let Some(changed) = apply_emacs_text_editing(
        &mut state.query,
        &mut state.query_cursor,
        &mut state.kill_buffer,
        key,
    ) else {
        return false;
    };
    if changed {
        state.mark_query_changed();
    }
    true
}

fn toggle_pin_current(state: &mut TuiState) {
    let Some(path) = state
        .results
        .get(state.selected)
        .map(|(path, _)| path.clone())
    else {
        return;
    };
    if let Some(index) = state.pinned.iter().position(|pinned| pinned == &path) {
        state.pinned.remove(index);
    } else {
        state.pinned.push(path);
    }
    if state.tab_pin_moves_to_next_row {
        state.move_selection(1);
    }
}

fn handle_key(state: &mut TuiState, key: KeyEvent) -> KeyAction {
    if !state.emacs_keybindings_enabled && is_emacs_shortcut(key) {
        return KeyAction::Continue;
    }
    let original_key = key;
    let key = normalize_emacs_shortcut(key);
    if state.help.is_some() {
        match (key.code, key.modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => return KeyAction::Cancel,
            (KeyCode::Enter, _)
            | (KeyCode::Esc, _)
            | (KeyCode::Char('g'), KeyModifiers::CONTROL) => state.close_help(),
            _ => {}
        }
        state.dirty = true;
        return KeyAction::Continue;
    }
    if state.options_overlay.is_some() {
        return handle_options_key(state, key);
    }
    if state.sort_picker.is_some() {
        return handle_sort_key(state, key);
    }
    if state.root_picker.is_some() {
        return handle_root_picker_key(state, key);
    }
    if state.filelist_confirmation.is_some() {
        return handle_filelist_confirmation_key(state, key);
    }
    if state.active_filelist.is_some() {
        return handle_active_filelist_key(state, key);
    }
    if matches!(key.code, KeyCode::F(1)) {
        state.open_help();
        state.dirty = true;
        return KeyAction::Continue;
    }
    if state.history.is_some() {
        if matches!(
            (key.code, key.modifiers),
            (KeyCode::Char('c'), KeyModifiers::CONTROL)
        ) {
            return KeyAction::Cancel;
        }
        let emacs_edit_handled = {
            let history = state.history.as_mut().expect("history overlay checked");
            match apply_emacs_text_editing(
                &mut history.filter,
                &mut history.filter_cursor,
                &mut state.kill_buffer,
                original_key,
            ) {
                Some(changed) => {
                    if changed {
                        refresh_history_results(history, &state.history_entries);
                    }
                    true
                }
                None => false,
            }
        };
        if emacs_edit_handled {
            state.dirty = true;
            return KeyAction::Continue;
        }
        let viewport_rows = state.viewport_rows;
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) | (KeyCode::Char('g'), KeyModifiers::CONTROL) => {
                state.cancel_history();
            }
            (KeyCode::Enter, _) if state.accept_history().is_some() => {
                state.dirty = true;
                return KeyAction::HistoryApplied;
            }
            (KeyCode::Up, _) => history_move_selection(
                state.history.as_mut().expect("history overlay checked"),
                -1,
                viewport_rows,
            ),
            (KeyCode::Down, _) => history_move_selection(
                state.history.as_mut().expect("history overlay checked"),
                1,
                viewport_rows,
            ),
            (KeyCode::PageUp, _) => history_move_selection(
                state.history.as_mut().expect("history overlay checked"),
                -(viewport_rows.max(1) as isize),
                viewport_rows,
            ),
            (KeyCode::PageDown, _) => history_move_selection(
                state.history.as_mut().expect("history overlay checked"),
                viewport_rows.max(1) as isize,
                viewport_rows,
            ),
            _ if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT => {
                let entries = state.history_entries.clone();
                let _ = edit_history_filter(
                    state.history.as_mut().expect("history overlay checked"),
                    &entries,
                    key.code,
                );
            }
            _ => {}
        }
        state.dirty = true;
        return KeyAction::Continue;
    }
    if matches!(key.code, KeyCode::F(2)) {
        state.open_options();
        state.dirty = true;
        return KeyAction::Continue;
    }
    if matches!(key.code, KeyCode::F(3)) {
        state.open_sort_picker();
        state.dirty = true;
        return KeyAction::Continue;
    }
    if matches!(key.code, KeyCode::F(4)) {
        state.open_root_picker();
        state.dirty = true;
        return KeyAction::Continue;
    }
    if matches!(key.code, KeyCode::F(5)) {
        return KeyAction::Refresh;
    }
    if matches!(key.code, KeyCode::F(6)) {
        return KeyAction::OpenFileList;
    }
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
            return KeyAction::Cancel;
        }
        (KeyCode::Char('g'), KeyModifiers::CONTROL) => {
            state.query.clear();
            state.query_cursor = 0;
            state.pinned.clear();
            state.status = "Query and pins cleared".to_string();
            state.mark_query_changed();
        }
        (KeyCode::Char('r'), KeyModifiers::CONTROL) => {
            if state.history_enabled {
                let query = state.commit_query_to_history();
                state.begin_history();
                state.dirty = true;
                return KeyAction::HistoryOpened(query);
            } else {
                return KeyAction::Continue;
            }
        }
        (KeyCode::Char('p'), KeyModifiers::ALT) | (KeyCode::Char('P'), KeyModifiers::ALT) => {
            state.preview_preferred = !state.preview_preferred;
            if !state.preview_preferred {
                state.preview_visible = false;
                state.clear_preview();
                state.status = "Preview hidden".to_string();
            } else {
                state.status = "Preview enabled".to_string();
            }
        }
        (KeyCode::Char('o'), KeyModifiers::CONTROL) => {
            return KeyAction::DispatchAction(AuthorizedActionMode::ExecuteOrOpen);
        }
        (KeyCode::Enter, KeyModifiers::SHIFT) => {
            return KeyAction::DispatchAction(AuthorizedActionMode::Reveal);
        }
        (KeyCode::Enter, _) => {
            if selected_paths(state).is_empty() {
                state.status = "No selection".to_string();
            } else {
                return KeyAction::Select;
            }
        }
        (KeyCode::Tab, _) | (KeyCode::BackTab, _) => toggle_pin_current(state),
        _ if apply_emacs_query_editing(state, original_key) => {}
        (KeyCode::Backspace, _) if state.query_cursor > 0 => {
            let start = char_to_byte_index(&state.query, state.query_cursor - 1);
            let end = char_to_byte_index(&state.query, state.query_cursor);
            state.query.replace_range(start..end, "");
            state.query_cursor -= 1;
            state.mark_query_changed();
        }
        (KeyCode::Delete, _) if state.query_cursor < state.query.chars().count() => {
            let start = char_to_byte_index(&state.query, state.query_cursor);
            let end = char_to_byte_index(&state.query, state.query_cursor + 1);
            state.query.replace_range(start..end, "");
            state.mark_query_changed();
        }
        (KeyCode::Left, _) => state.query_cursor = state.query_cursor.saturating_sub(1),
        (KeyCode::Right, _) => {
            state.query_cursor = (state.query_cursor + 1).min(state.query.chars().count())
        }
        (KeyCode::Home, _) => state.query_cursor = 0,
        (KeyCode::End, _) => state.query_cursor = state.query.chars().count(),
        (KeyCode::Char(ch), KeyModifiers::NONE) | (KeyCode::Char(ch), KeyModifiers::SHIFT) => {
            let byte_index = char_to_byte_index(&state.query, state.query_cursor);
            state.query.insert(byte_index, ch);
            state.query_cursor += 1;
            state.mark_query_changed();
        }
        (KeyCode::Up, _) => state.move_selection(-1),
        (KeyCode::Down, _) => state.move_selection(1),
        (KeyCode::PageUp, _) => state.move_selection(-(state.viewport_rows.max(1) as isize)),
        (KeyCode::PageDown, _) => state.move_selection(state.viewport_rows.max(1) as isize),
        _ => {}
    }
    state.dirty = true;
    KeyAction::Continue
}

fn char_to_byte_index(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

fn insert_paste(state: &mut TuiState, pasted: &str) {
    if pasted.is_empty()
        || state.help.is_some()
        || state.options_overlay.is_some()
        || state.sort_picker.is_some()
        || state.root_picker.is_some()
        || state.filelist_confirmation.is_some()
        || state.active_filelist.is_some()
    {
        return;
    }
    if let Some(history) = state.history.as_mut() {
        let byte_index = char_to_byte_index(&history.filter, history.filter_cursor);
        history.filter.insert_str(byte_index, pasted);
        history.filter_cursor += pasted.chars().count();
        refresh_history_results(history, &state.history_entries);
        state.dirty = true;
        return;
    }
    let byte_index = char_to_byte_index(&state.query, state.query_cursor);
    state.query.insert_str(byte_index, pasted);
    state.query_cursor += pasted.chars().count();
    state.mark_query_changed();
    state.dirty = true;
}

fn search(
    request: &SearchRequest,
    prefix_cache: &mut SearchPrefixCache,
) -> (Vec<(PathBuf, f64)>, Option<String>) {
    let compiled_ignore = request
        .options
        .ignore_enabled
        .then(|| CompiledIgnoreTerms::compile(&request.ignore_terms, request.options.ignore_case));
    let entries = Arc::new(
        request
            .entries
            .iter()
            .flat_map(|batch| batch.iter())
            .filter(|path| {
                compiled_ignore.as_ref().is_none_or(|compiled| {
                    !compiled.matches_path(
                        path,
                        QueryScope {
                            root: Some(&request.root),
                            prefer_relative: true,
                            ignore_case: request.options.ignore_case,
                        },
                    )
                })
            })
            .cloned()
            .map(Entry::from)
            .collect(),
    );
    let (result_set, error) = rank_search_results(
        &entries,
        &request.query,
        &request.root,
        request.limit,
        request.options.regex,
        request.options.ignore_case,
        true,
        prefix_cache,
        request.options.sort_mode,
        SearchSortScope::AllMatches,
    );
    (result_set.results, error)
}

fn draw<W: Write>(
    terminal_output: &mut W,
    state: &mut TuiState,
    options: &CliTuiOptions,
) -> Result<()> {
    let mut frame = Vec::new();
    render_frame(&mut frame, state, options)?;
    write_synchronized_frame(terminal_output, &frame)?;
    Ok(())
}

fn write_synchronized_frame<W: Write>(terminal_output: &mut W, frame: &[u8]) -> io::Result<()> {
    queue!(terminal_output, BeginSynchronizedUpdate)?;
    let frame_result = terminal_output.write_all(frame);
    let end_result = queue!(terminal_output, EndSynchronizedUpdate);
    let flush_result = terminal_output.flush();
    frame_result?;
    end_result?;
    flush_result
}

fn render_frame<W: Write>(
    terminal_output: &mut W,
    state: &mut TuiState,
    _options: &CliTuiOptions,
) -> Result<()> {
    let (width, height) = terminal::size()?;
    let preview_visible = preview_visible_for_size(state.preview_preferred, width, height);
    state.preview_visible = preview_visible;
    if !preview_visible {
        state.clear_preview();
    }
    let list_width = if preview_visible {
        width.saturating_mul(3).saturating_div(5).max(1)
    } else {
        width
    };
    let visible = if state.history.is_some() {
        height.saturating_sub(3) as usize
    } else {
        height.saturating_sub(4) as usize
    };
    state.viewport_rows = visible.max(1);
    state.ensure_selection_visible();
    let start = state.offset.min(state.results.len());
    execute!(
        terminal_output,
        Clear(ClearType::All),
        MoveTo(0, 0),
        SetAttribute(Attribute::Bold),
        Print(clip_to_width("FlistWalker CLI", list_width as usize)),
        SetAttribute(Attribute::Reset),
    )?;
    if height > 1 {
        execute!(
            terminal_output,
            MoveTo(0, 1),
            Print(query_line_for_width(state, list_width as usize))
        )?;
    }
    if height > 2 {
        execute!(
            terminal_output,
            MoveTo(0, 2),
            SetForegroundColor(Color::DarkGrey),
            Print(clip_to_width(&state.status_line(), list_width as usize)),
            ResetColor
        )?;
    }
    if height > 3 {
        execute!(
            terminal_output,
            MoveTo(0, 3),
            SetForegroundColor(Color::DarkGrey),
            Print(clip_to_width(
                &format!(
                    "Enter select | F2 options | F3 {} | Alt+P preview | Esc cancel",
                    state.sort_mode.label()
                ),
                list_width as usize,
            )),
            ResetColor
        )?;
    }
    let compiled = (!state.query.trim().is_empty()).then(|| {
        CompiledQuery::compile(
            &state.query,
            QueryOptions {
                use_regex: state.runtime_options.regex,
                ignore_case: state.runtime_options.ignore_case,
            },
        )
    });
    for (row, (path, _score)) in state.results.iter().skip(start).take(visible).enumerate() {
        let is_selected = start + row == state.selected;
        let is_pinned = state.pinned.contains(path);
        let marker = match (is_selected, is_pinned) {
            (true, true) => "*>",
            (true, false) => "> ",
            (false, true) => "* ",
            (false, false) => "  ",
        };
        let display = display_path_with_mode(path, &state.root, true);
        let positions = compiled
            .as_ref()
            .and_then(|query| query.as_ref().ok())
            .map(|query| {
                crate::ui_model::match_positions_for_path_with_compiled(
                    path,
                    &state.root,
                    query,
                    true,
                )
            })
            .unwrap_or_default();
        print_highlighted(
            terminal_output,
            (row + 4) as u16,
            marker,
            &display,
            &positions,
            list_width,
        )?;
    }
    if preview_visible {
        render_preview_pane(terminal_output, state, list_width, width, height)?;
    }
    if let Some(context) = state.help {
        render_help_overlay(
            terminal_output,
            context,
            state.emacs_keybindings_enabled,
            width,
            height,
        )?;
    } else if let Some(options_overlay) = state.options_overlay.as_ref() {
        render_options_overlay(
            terminal_output,
            options_overlay,
            state.emacs_keybindings_enabled,
            width,
            height,
        )?;
    } else if let Some(sort_picker) = state.sort_picker.as_ref() {
        render_sort_picker(
            terminal_output,
            sort_picker,
            state.emacs_keybindings_enabled,
            width,
            height,
        )?;
    } else if let Some(root_picker) = state.root_picker.as_ref() {
        render_root_picker(
            terminal_output,
            root_picker,
            &state.saved_roots,
            state.emacs_keybindings_enabled,
            width,
            height,
        )?;
    } else if let Some(confirmation) = state.filelist_confirmation.as_ref() {
        render_filelist_confirmation(
            terminal_output,
            confirmation,
            state.emacs_keybindings_enabled,
            width,
            height,
        )?;
    } else if let Some(history) = state.history.as_ref() {
        render_history_overlay(
            terminal_output,
            history,
            state.emacs_keybindings_enabled,
            width,
            height,
        )?;
    }
    Ok(())
}

fn render_filelist_confirmation<W: Write>(
    terminal_output: &mut W,
    confirmation: &FileListConfirmation,
    emacs_keybindings_enabled: bool,
    width: u16,
    height: u16,
) -> Result<()> {
    execute!(terminal_output, Clear(ClearType::All))?;
    let cancel_keys = overlay_cancel_keys(emacs_keybindings_enabled);
    let lines = match confirmation {
        FileListConfirmation::Mode {
            propagate_to_ancestors,
        } => vec![
            "Create FileList".to_string(),
            format!(
                "Up/Down/Space choose scope | Enter continue | {cancel_keys} cancel | Ctrl+C exit"
            ),
            format!(
                "> Scope: {}",
                if *propagate_to_ancestors {
                    "root and ancestors"
                } else {
                    "root only"
                }
            ),
            "No files are written until this confirmation is accepted.".to_string(),
        ],
        FileListConfirmation::Overwrite {
            propagate_to_ancestors,
        } => vec![
            "Overwrite existing root FileList?".to_string(),
            format!("Enter overwrite | {cancel_keys} cancel | Ctrl+C exit"),
            format!(
                "Scope: {}",
                if *propagate_to_ancestors {
                    "root and ancestors"
                } else {
                    "root only"
                }
            ),
            "This is the final write confirmation.".to_string(),
        ],
    };
    for (row, line) in lines.into_iter().take(height as usize).enumerate() {
        execute!(
            terminal_output,
            MoveTo(0, row as u16),
            Print(clip_to_width(&line, width as usize)),
        )?;
    }
    Ok(())
}

fn render_root_picker<W: Write>(
    terminal_output: &mut W,
    picker: &RootPicker,
    roots: &[PathBuf],
    emacs_keybindings_enabled: bool,
    width: u16,
    height: u16,
) -> Result<()> {
    execute!(terminal_output, Clear(ClearType::All))?;
    let cancel_keys = overlay_cancel_keys(emacs_keybindings_enabled);
    let lines = [
        "Saved roots".to_string(),
        format!("Enter switch | {cancel_keys} cancel | Ctrl+C exit | arrows/Page move"),
    ];
    for (row, line) in lines.iter().enumerate().take(height as usize) {
        execute!(
            terminal_output,
            MoveTo(0, row as u16),
            Print(clip_to_width(line, width as usize)),
        )?;
    }
    if roots.is_empty() {
        if height > 2 {
            execute!(
                terminal_output,
                MoveTo(0, 2),
                Print(clip_to_width(
                    "No saved roots are available.",
                    width as usize
                )),
            )?;
        }
        return Ok(());
    }
    let visible = height.saturating_sub(2) as usize;
    let start = overlay_window_start(picker.selected, roots.len(), visible);
    for (row, (index, root)) in roots
        .iter()
        .enumerate()
        .skip(start)
        .take(visible)
        .enumerate()
    {
        let marker = if index == picker.selected { "> " } else { "  " };
        execute!(
            terminal_output,
            MoveTo(0, (row + 2) as u16),
            Print(clip_to_width(
                &format!("{marker}{}", root.display()),
                width as usize
            )),
        )?;
    }
    Ok(())
}

fn render_options_overlay<W: Write>(
    terminal_output: &mut W,
    overlay: &OptionsOverlay,
    emacs_keybindings_enabled: bool,
    width: u16,
    height: u16,
) -> Result<()> {
    let on_off = |value| if value { "on" } else { "off" };
    let cancel_keys = overlay_cancel_keys(emacs_keybindings_enabled);
    let lines = [
        "Options".to_string(),
        format!("Enter apply | {cancel_keys} cancel | Ctrl+C exit | arrows + Space change"),
        format!("Files: {}", on_off(overlay.draft.include_files)),
        format!("Folders: {}", on_off(overlay.draft.include_dirs)),
        format!("Regex: {}", on_off(overlay.draft.regex)),
        format!("Ignore Case: {}", on_off(overlay.draft.ignore_case)),
        format!("Ignore: {}", on_off(overlay.draft.ignore_enabled)),
        format!("Source: {}", overlay.draft.source.label()),
    ];
    execute!(terminal_output, Clear(ClearType::All))?;
    for (row, line) in lines.iter().take(2).enumerate().take(height as usize) {
        execute!(
            terminal_output,
            MoveTo(0, row as u16),
            Print(clip_to_width(line, width as usize)),
        )?;
    }
    let option_rows = &lines[2..];
    let visible = height.saturating_sub(2) as usize;
    let start = overlay_window_start(overlay.selected, option_rows.len(), visible);
    for (row, (index, line)) in option_rows
        .iter()
        .enumerate()
        .skip(start)
        .take(visible)
        .enumerate()
    {
        let marker = if index == overlay.selected {
            "> "
        } else {
            "  "
        };
        execute!(
            terminal_output,
            MoveTo(0, (row + 2) as u16),
            Print(clip_to_width(&format!("{marker}{line}"), width as usize)),
        )?;
    }
    Ok(())
}

fn render_sort_picker<W: Write>(
    terminal_output: &mut W,
    picker: &SortPicker,
    emacs_keybindings_enabled: bool,
    width: u16,
    height: u16,
) -> Result<()> {
    execute!(terminal_output, Clear(ClearType::All))?;
    let cancel_keys = overlay_cancel_keys(emacs_keybindings_enabled);
    let heading = [
        "Sort results".to_string(),
        format!("Enter apply | {cancel_keys} cancel | Ctrl+C exit | arrows move"),
    ];
    for (row, line) in heading.into_iter().enumerate() {
        if row < height as usize {
            execute!(
                terminal_output,
                MoveTo(0, row as u16),
                Print(clip_to_width(&line, width as usize)),
            )?;
        }
    }
    let visible = height.saturating_sub(2) as usize;
    let start = overlay_window_start(picker.selected, SORT_MODES.len(), visible);
    for (row, (index, mode)) in SORT_MODES
        .iter()
        .enumerate()
        .skip(start)
        .take(visible)
        .enumerate()
    {
        let marker = if index == picker.selected { "> " } else { "  " };
        execute!(
            terminal_output,
            MoveTo(0, (row + 2) as u16),
            Print(clip_to_width(
                &format!("{marker}{}", mode.label()),
                width as usize
            )),
        )?;
    }
    Ok(())
}

fn overlay_window_start(selected: usize, total: usize, visible: usize) -> usize {
    if visible >= total {
        return 0;
    }
    let visible = visible.max(1);
    let before = visible / 2;
    selected
        .saturating_sub(before)
        .min(total.saturating_sub(visible))
}

fn render_history_overlay<W: Write>(
    terminal_output: &mut W,
    history: &HistoryOverlay,
    emacs_keybindings_enabled: bool,
    width: u16,
    height: u16,
) -> Result<()> {
    execute!(
        terminal_output,
        Clear(ClearType::All),
        MoveTo(0, 0),
        SetAttribute(Attribute::Bold),
        Print(clip_to_width("History", width as usize)),
        SetAttribute(Attribute::Reset),
    )?;
    if height > 1 {
        execute!(
            terminal_output,
            MoveTo(0, 1),
            Print(clip_to_width(
                &format!("Filter: {}", history.filter),
                width as usize,
            )),
        )?;
    }
    if height > 2 {
        execute!(
            terminal_output,
            MoveTo(0, 2),
            SetForegroundColor(Color::DarkGrey),
            Print(clip_to_width(
                &format!(
                    "Enter apply | {} cancel | Ctrl+C exit | arrows/Page move",
                    overlay_cancel_keys(emacs_keybindings_enabled)
                ),
                width as usize,
            )),
            ResetColor,
        )?;
    }
    let visible = height.saturating_sub(3) as usize;
    for (row, entry) in history
        .results
        .iter()
        .skip(history.offset)
        .take(visible)
        .enumerate()
    {
        let marker = if history.offset + row == history.selected {
            "> "
        } else {
            "  "
        };
        execute!(
            terminal_output,
            MoveTo(0, (row + 3) as u16),
            Print(clip_to_width(marker, width as usize)),
            Print(clip_to_width(entry, width.saturating_sub(2) as usize)),
        )?;
    }
    Ok(())
}

fn overlay_cancel_keys(emacs_keybindings_enabled: bool) -> &'static str {
    if emacs_keybindings_enabled {
        "Esc/Ctrl+G"
    } else {
        "Esc"
    }
}

fn render_help_overlay<W: Write>(
    terminal_output: &mut W,
    context: HelpContext,
    emacs_keybindings_enabled: bool,
    width: u16,
    height: u16,
) -> Result<()> {
    let close_help = if emacs_keybindings_enabled {
        "Enter / Esc / Ctrl+G close help | Ctrl+C exit"
    } else {
        "Enter / Esc close help | Ctrl+C exit"
    };
    let mut lines = vec!["Help".to_string(), close_help.to_string()];
    match context {
        HelpContext::Normal if emacs_keybindings_enabled => lines.extend([
            "Enter/Ctrl+J/Ctrl+M output | Tab/Shift+Tab/Ctrl+I pin".to_string(),
            "arrows/Ctrl+P/Ctrl+N move | PageUp/Alt+V and PageDown/Ctrl+V".to_string(),
            "Ctrl+O open current | Shift+Enter reveal current".to_string(),
            "Ctrl+G clear query and pins | Ctrl+R search history".to_string(),
            "F2 options | F3 sort | F4 roots | F5 refresh | F6 FileList | Alt+P preview | F1 help".to_string(),
        ]),
        HelpContext::Normal => lines.extend([
            "Enter output selection | Tab/Shift+Tab pin | arrows/Page move".to_string(),
            "Emacs shortcuts disabled by runtime config".to_string(),
            "Ctrl+O open current | Shift+Enter reveal current".to_string(),
            "F2 options | F3 sort | F4 roots | F5 refresh | F6 FileList | Alt+P preview | F1 help".to_string(),
        ]),
        HelpContext::History => lines.extend([
            "History search is paused while help is open.".to_string(),
            if emacs_keybindings_enabled {
                "Close help to use Enter, Esc/Ctrl+G, edit, or navigation."
            } else {
                "Close help to use Enter, Esc, edit, or navigation."
            }
            .to_string(),
        ]),
        HelpContext::FileList => lines.extend([
            "FileList creation is settling; no result is accepted before it finishes.".to_string(),
            if emacs_keybindings_enabled {
                "Enter selects after cancellation, F4 chooses a root, Esc/Ctrl+G/Ctrl+C exits after settlement."
            } else {
                "Enter selects after cancellation, F4 chooses a root, Esc/Ctrl+C exits after settlement."
            }
            .to_string(),
        ]),
    }
    execute!(terminal_output, Clear(ClearType::All))?;
    for (row, line) in lines.into_iter().take(height as usize).enumerate() {
        execute!(
            terminal_output,
            MoveTo(0, row as u16),
            Print(clip_to_width(&line, width as usize)),
        )?;
    }
    Ok(())
}

fn render_preview_pane<W: Write>(
    terminal_output: &mut W,
    state: &TuiState,
    list_width: u16,
    terminal_width: u16,
    terminal_height: u16,
) -> Result<()> {
    let x = list_width.saturating_add(1);
    let pane_width = terminal_width.saturating_sub(x);
    if pane_width == 0 {
        return Ok(());
    }
    execute!(
        terminal_output,
        MoveTo(x, 0),
        SetAttribute(Attribute::Bold),
        Print(clip_to_width("Preview", pane_width as usize)),
        SetAttribute(Attribute::Reset),
    )?;
    for (index, line) in state
        .preview
        .lines()
        .take(terminal_height.saturating_sub(1) as usize)
        .enumerate()
    {
        execute!(
            terminal_output,
            MoveTo(x, (index + 1) as u16),
            Print(clip_to_width(line, pane_width as usize)),
        )?;
    }
    Ok(())
}

fn print_highlighted<W: Write>(
    terminal_output: &mut W,
    row: u16,
    marker: &str,
    text: &str,
    positions: &HashSet<usize>,
    width: u16,
) -> Result<()> {
    execute!(
        terminal_output,
        MoveTo(0, row),
        Print(clip_to_width(marker, width as usize))
    )?;
    let mut highlighted = false;
    let mut chunk = String::new();
    let available = width.saturating_sub(2) as usize;
    let mut used = 0;
    for (index, ch) in text.chars().enumerate() {
        let display_char = terminal_safe_char(ch);
        let char_width = UnicodeWidthChar::width(display_char).unwrap_or(0);
        if used + char_width > available {
            break;
        }
        used += char_width;
        let next = positions.contains(&index);
        if next != highlighted {
            if !chunk.is_empty() {
                execute!(terminal_output, Print(std::mem::take(&mut chunk)))?;
            }
            if next {
                execute!(terminal_output, SetForegroundColor(Color::Yellow))?;
            } else {
                execute!(terminal_output, ResetColor)?;
            }
            highlighted = next;
        }
        chunk.push(display_char);
    }
    if !chunk.is_empty() {
        execute!(terminal_output, Print(chunk))?;
    }
    execute!(terminal_output, ResetColor)?;
    Ok(())
}

fn clip_to_width(text: &str, width: usize) -> String {
    let mut used = 0;
    text.chars()
        .map(terminal_safe_char)
        .take_while(|ch| {
            let char_width = UnicodeWidthChar::width(*ch).unwrap_or(0);
            if used + char_width > width {
                false
            } else {
                used += char_width;
                true
            }
        })
        .collect()
}

fn terminal_safe_char(ch: char) -> char {
    if ch.is_control() {
        '�'
    } else {
        ch
    }
}

fn query_line_for_width(state: &TuiState, width: usize) -> String {
    let prefix = clip_to_width("> ", width);
    let prefix_width = prefix
        .chars()
        .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
        .sum::<usize>();
    let available = width.saturating_sub(prefix_width);
    if available == 0 {
        return prefix;
    }

    let chars = state.query.chars().collect::<Vec<_>>();
    let cursor = state.query_cursor.min(chars.len());
    let left_budget = available.saturating_sub(1);
    let mut left = Vec::new();
    let mut left_width = 0;
    for ch in chars[..cursor].iter().rev().copied() {
        let safe = terminal_safe_char(ch);
        let char_width = UnicodeWidthChar::width(safe).unwrap_or(0);
        if left_width + char_width > left_budget {
            break;
        }
        left.push(safe);
        left_width += char_width;
    }
    left.reverse();

    let mut line = prefix;
    line.extend(left);
    line.push('│');
    let mut used = left_width + 1;
    for ch in chars[cursor..].iter().copied() {
        let safe = terminal_safe_char(ch);
        let char_width = UnicodeWidthChar::width(safe).unwrap_or(0);
        if used + char_width > available {
            break;
        }
        line.push(safe);
        used += char_width;
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tc_169_tui_update_notice_is_english_and_manual_only() {
        assert_eq!(
            format_tui_update_notice("0.20.0"),
            "Update available: v0.20.0 — Run flistwalker --update after exiting"
        );
    }
    use crate::runtime_config::{DeveloperRuntimeConfig, RuntimeConfig};
    use std::cell::RefCell;
    use std::fs;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::rc::Rc;

    struct TestTempDir {
        path: PathBuf,
    }

    impl TestTempDir {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "flistwalker-cli-tui-{label}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("system time")
                    .as_nanos()
            ));
            fs::create_dir_all(&path).expect("create temporary test directory");
            Self { path }
        }
    }

    impl Drop for TestTempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[derive(Clone)]
    struct FakeTerminalOps {
        calls: Rc<RefCell<Vec<&'static str>>>,
        fail_on: Option<&'static str>,
    }

    impl FakeTerminalOps {
        fn call(&self, name: &'static str) -> io::Result<()> {
            self.calls.borrow_mut().push(name);
            if self.fail_on == Some(name) {
                Err(io::Error::other(format!("failed at {name}")))
            } else {
                Ok(())
            }
        }
    }

    impl TerminalOps for FakeTerminalOps {
        fn enable_raw_mode(&mut self) -> io::Result<()> {
            self.call("enable_raw")
        }

        fn disable_raw_mode(&mut self) -> io::Result<()> {
            self.call("disable_raw")
        }

        fn enter_alternate<W: Write>(&mut self, _writer: &mut W) -> io::Result<()> {
            self.call("enter_alternate")
        }

        fn leave_alternate<W: Write>(&mut self, _writer: &mut W) -> io::Result<()> {
            self.call("leave_alternate")
        }

        fn hide_cursor<W: Write>(&mut self, _writer: &mut W) -> io::Result<()> {
            self.call("hide_cursor")
        }

        fn show_cursor<W: Write>(&mut self, _writer: &mut W) -> io::Result<()> {
            self.call("show_cursor")
        }

        fn enable_paste<W: Write>(&mut self, _writer: &mut W) -> io::Result<()> {
            self.call("enable_paste")
        }

        fn disable_paste<W: Write>(&mut self, _writer: &mut W) -> io::Result<()> {
            self.call("disable_paste")
        }
    }

    #[test]
    fn tc_006_interactive_query_editing_marks_search_dirty() {
        let mut state = TuiState::new("");
        state.dirty = false;

        assert!(matches!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)
            ),
            KeyAction::Continue
        ));
        assert_eq!(state.query, "a");
        assert!(state.last_query_change.is_some());
        assert!(state.dirty);
    }

    #[test]
    fn tc_006_interactive_enter_returns_selected_path() {
        let mut state = TuiState::new("");
        state.results = vec![(PathBuf::from("selected.txt"), 1.0)];

        assert!(matches!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
            ),
            KeyAction::Select
        ));
        assert_eq!(selected_paths(&state), vec![PathBuf::from("selected.txt")]);
    }

    #[test]
    fn tc_006_escape_cancels_without_selecting() {
        let mut state = TuiState::new("");
        assert!(matches!(
            handle_key(&mut state, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            KeyAction::Cancel
        ));
    }

    #[test]
    fn tc_006_tab_toggles_multiple_pins() {
        let mut state = TuiState::new("");
        state.results = vec![
            (PathBuf::from("one.txt"), 1.0),
            (PathBuf::from("two.txt"), 1.0),
        ];
        assert!(matches!(
            handle_key(&mut state, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            KeyAction::Continue
        ));
        state.selected = 1;
        handle_key(&mut state, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(
            selected_paths(&state),
            vec![PathBuf::from("one.txt"), PathBuf::from("two.txt")]
        );
    }

    #[test]
    fn tc_162_tui_emacs_navigation_pin_and_select_follow_runtime_toggle() {
        let mut enabled = TuiState::new("");
        enabled.results = (0..8)
            .map(|index| (PathBuf::from(format!("{index}.txt")), 1.0))
            .collect();
        enabled.viewport_rows = 3;

        handle_key(
            &mut enabled,
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
        );
        assert_eq!(enabled.selected, 1);
        handle_key(
            &mut enabled,
            KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
        );
        assert_eq!(enabled.selected, 0);
        handle_key(
            &mut enabled,
            KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL),
        );
        assert_eq!(enabled.selected, 3);
        handle_key(
            &mut enabled,
            KeyEvent::new(KeyCode::Char('v'), KeyModifiers::ALT),
        );
        assert_eq!(enabled.selected, 0);
        handle_key(
            &mut enabled,
            KeyEvent::new(KeyCode::Char('i'), KeyModifiers::CONTROL),
        );
        assert_eq!(enabled.pinned, vec![PathBuf::from("0.txt")]);
        assert!(matches!(
            handle_key(
                &mut enabled,
                KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL),
            ),
            KeyAction::Select
        ));

        let mut disabled = TuiState::new("");
        disabled.emacs_keybindings_enabled = false;
        disabled.results = enabled.results.clone();
        handle_key(
            &mut disabled,
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
        );
        handle_key(
            &mut disabled,
            KeyEvent::new(KeyCode::Char('i'), KeyModifiers::CONTROL),
        );
        assert_eq!(disabled.selected, 0);
        assert!(disabled.pinned.is_empty());
        assert!(matches!(
            handle_key(
                &mut disabled,
                KeyEvent::new(KeyCode::Char('m'), KeyModifiers::CONTROL),
            ),
            KeyAction::Continue
        ));
        disabled.query = "keep".to_string();
        disabled.query_cursor = disabled.query.chars().count();
        disabled.history_enabled = true;
        handle_key(
            &mut disabled,
            KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL),
        );
        handle_key(
            &mut disabled,
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
        );
        assert_eq!(disabled.query, "keep");
        assert!(disabled.history.is_none());
        handle_key(
            &mut disabled,
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
        );
        handle_key(
            &mut disabled,
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
        );
        assert_eq!(disabled.selected, 1);
        assert_eq!(disabled.pinned, vec![PathBuf::from("1.txt")]);
    }

    #[test]
    fn tc_162_tui_tab_pin_move_setting_applies_to_tab_backtab_and_ctrl_i() {
        let mut state = TuiState::new("");
        state.tab_pin_moves_to_next_row = true;
        state.results = (0..3)
            .map(|index| (PathBuf::from(format!("{index}.txt")), 1.0))
            .collect();

        handle_key(&mut state, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(state.pinned, vec![PathBuf::from("0.txt")]);
        assert_eq!(state.selected, 1);

        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
        );
        assert_eq!(
            state.pinned,
            vec![PathBuf::from("0.txt"), PathBuf::from("1.txt")]
        );
        assert_eq!(state.selected, 2);

        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('i'), KeyModifiers::CONTROL),
        );
        assert_eq!(state.pinned.last(), Some(&PathBuf::from("2.txt")));
        assert_eq!(state.selected, 2);
    }

    #[test]
    fn tc_162_tui_emacs_query_editing_uses_the_same_runtime_toggle() {
        let mut enabled = TuiState::new("alpha beta");
        enabled.query_cursor = 5;
        handle_key(
            &mut enabled,
            KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
        );
        assert_eq!(enabled.query, "alpha");
        handle_key(
            &mut enabled,
            KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL),
        );
        assert_eq!(enabled.query, "alpha beta");
        handle_key(
            &mut enabled,
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL),
        );
        assert_eq!(enabled.query_cursor, 0);
        handle_key(
            &mut enabled,
            KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL),
        );
        assert_eq!(enabled.query_cursor, enabled.query.chars().count());

        let mut disabled = TuiState::new("alpha beta");
        disabled.emacs_keybindings_enabled = false;
        disabled.query_cursor = 5;
        handle_key(
            &mut disabled,
            KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
        );
        assert_eq!(disabled.query, "alpha beta");
        assert_eq!(disabled.query_cursor, 5);
    }

    #[test]
    fn tc_162_result_refresh_preserves_the_selected_path() {
        let mut state = TuiState::new("");
        state.results = vec![
            (PathBuf::from("one.txt"), 1.0),
            (PathBuf::from("two.txt"), 0.5),
        ];
        state.selected = 1;

        state.set_results(
            vec![
                (PathBuf::from("zero.txt"), 2.0),
                (PathBuf::from("two.txt"), 1.5),
            ],
            None,
        );

        assert_eq!(state.selected, 1);
        assert_eq!(state.results[state.selected].0, PathBuf::from("two.txt"));
    }

    #[test]
    fn tc_162_hidden_pins_remain_part_of_the_final_selection() {
        let mut state = TuiState::new("");
        state.results = vec![(PathBuf::from("pinned.txt"), 1.0)];
        handle_key(&mut state, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        state.results = vec![(PathBuf::from("visible.txt"), 1.0)];
        state.selected = 0;

        assert_eq!(selected_paths(&state), vec![PathBuf::from("pinned.txt")]);
    }

    #[test]
    fn tc_162_enter_without_a_selection_does_not_exit() {
        let mut state = TuiState::new("");

        assert!(matches!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
            ),
            KeyAction::Continue
        ));
        assert_eq!(state.status, "No selection");
    }

    #[test]
    fn tc_162_query_editor_inserts_at_the_cursor() {
        let mut state = TuiState::new("ab");

        handle_key(&mut state, KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('X'), KeyModifiers::SHIFT),
        );

        assert_eq!(state.query, "aXb");
    }

    #[test]
    fn tc_162_stale_search_response_is_ignored_by_request_id() {
        let mut state = TuiState::new("new");
        state.root = PathBuf::from("root");
        state.active_search_request_id = Some(2);
        state.results = vec![(PathBuf::from("current.txt"), 1.0)];
        let search_options = state.runtime_options.search_options(state.sort_mode);

        apply_search_response(
            &mut state,
            1,
            Path::new("root"),
            "new",
            search_options,
            vec![(PathBuf::from("stale.txt"), 2.0)],
            None,
        );
        assert_eq!(state.results[0].0, PathBuf::from("current.txt"));

        apply_search_response(
            &mut state,
            2,
            Path::new("root"),
            "new",
            search_options,
            vec![(PathBuf::from("latest.txt"), 3.0)],
            None,
        );
        assert_eq!(state.results[0].0, PathBuf::from("latest.txt"));

        state.active_search_request_id = Some(3);
        apply_search_response(
            &mut state,
            3,
            Path::new("other-root"),
            "new",
            search_options,
            vec![(PathBuf::from("wrong-root.txt"), 4.0)],
            None,
        );
        assert_eq!(state.results[0].0, PathBuf::from("latest.txt"));
    }

    #[test]
    fn tc_162_index_failure_keeps_tui_recoverable() {
        let mut state = TuiState::new("");
        state.active_index_request = Some((1, PathBuf::from("root")));

        apply_worker_response(
            &mut state,
            WorkerResponse::IndexFailed {
                request_id: 1,
                root: PathBuf::from("root"),
                has_root_filelist: false,
                error: "broken FileList".to_string(),
            },
        )
        .expect("index failure is surfaced in status");

        assert!(state.status.contains("broken FileList"));
        assert!(state.active_index_request.is_none());
        assert!(state.root_filelist_known);
        assert!(!state.root_filelist_exists);
        assert!(matches!(
            handle_key(&mut state, KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE)),
            KeyAction::OpenFileList
        ));
    }

    #[test]
    fn tc_162_walker_failure_emits_index_failed_without_finished() {
        let missing_root = TestTempDir::new("walker-failure").path.join("missing");
        let request = IndexRequest {
            request_id: 7,
            root: missing_root,
            include_files: true,
            include_dirs: true,
            source: TuiSource::Walker,
        };
        let mut responses = Vec::new();

        process_index_request(request, &|| false, |response| responses.push(response));

        assert!(matches!(
            responses.as_slice(),
            [WorkerResponse::IndexFailed { request_id: 7, .. }]
        ));
    }

    #[test]
    fn tc_162_tui_walker_uses_runtime_adaptive_limits_and_reports_cap_before_finish() {
        let temp = TestTempDir::new("walker-runtime-limits");
        for name in ["one.txt", "two.txt", "three.txt"] {
            fs::write(temp.path.join(name), name).expect("write walker fixture");
        }
        let request = IndexRequest {
            request_id: 8,
            root: temp.path.clone(),
            include_files: true,
            include_dirs: false,
            source: TuiSource::Walker,
        };
        let config = RuntimeConfig {
            walker_max_entries: 1,
            developer: DeveloperRuntimeConfig {
                walker_adaptive_initial_limit: Some(1),
                walker_adaptive_max_limit: Some(1),
                ..DeveloperRuntimeConfig::default()
            },
            ..RuntimeConfig::default()
        };
        let mut responses = Vec::new();

        process_index_request_with_config(request, &config, &|| false, |response| {
            responses.push(response)
        });

        let emitted = responses
            .iter()
            .map(|response| match response {
                WorkerResponse::IndexedBatch { entries, .. } => entries.len(),
                _ => 0,
            })
            .sum::<usize>();
        let truncated = responses
            .iter()
            .position(|response| {
                matches!(
                    response,
                    WorkerResponse::IndexTruncated {
                        request_id: 8,
                        limit: 1,
                        ..
                    }
                )
            })
            .expect("truncation response");
        let finished = responses
            .iter()
            .position(|response| {
                matches!(
                    response,
                    WorkerResponse::IndexedFinished { request_id: 8, .. }
                )
            })
            .expect("finished response");

        assert_eq!(emitted, 1);
        assert!(truncated < finished);
    }

    #[test]
    fn tc_162_candidate_batches_append_without_cloning_existing_paths() {
        let mut candidates = CandidateBatches::default();
        candidates.push(vec![PathBuf::from("first.txt")]);
        let search_snapshot = candidates.snapshot();
        let first_batch = Arc::clone(&search_snapshot[0]);

        candidates.push(vec![PathBuf::from("second.txt")]);

        assert_eq!(candidates.len(), 2);
        assert!(Arc::ptr_eq(&first_batch, &candidates.snapshot()[0]));
    }

    #[test]
    fn tc_162_worker_response_drain_respects_per_tick_budget() {
        let (tx, rx) = mpsc::channel();
        for value in 0..=MAX_WORKER_RESPONSES_PER_TICK {
            tx.send(value).expect("queue response");
        }

        let drained = take_ready_responses(&rx, MAX_WORKER_RESPONSES_PER_TICK);

        assert_eq!(drained.len(), MAX_WORKER_RESPONSES_PER_TICK);
        assert_eq!(rx.try_recv(), Ok(MAX_WORKER_RESPONSES_PER_TICK));
    }

    #[test]
    fn tc_162_query_editor_supports_delete_home_end_and_unicode_paste() {
        let mut state = TuiState::new("ab");
        handle_key(&mut state, KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));
        insert_paste(&mut state, "界🙂");
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE),
        );
        handle_key(&mut state, KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
        );

        assert_eq!(state.query, "界🙂");
        assert_eq!(state.query_cursor, 2);
    }

    #[test]
    fn tc_162_page_navigation_uses_dynamic_viewport_rows() {
        let mut state = TuiState::new("");
        state.results = (0..20)
            .map(|index| (PathBuf::from(format!("{index}.txt")), 1.0))
            .collect();
        state.viewport_rows = 5;

        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
        );
        assert_eq!(state.selected, 5);
        assert_eq!(state.offset, 1);
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE),
        );
        assert_eq!(state.selected, 0);
        assert_eq!(state.offset, 0);
    }

    #[test]
    fn tc_162_unicode_clipping_uses_terminal_column_width() {
        assert_eq!(clip_to_width("a界b", 3), "a界");
        assert_eq!(clip_to_width("a界b", 2), "a");
        assert_eq!(clip_to_width("e\u{301}x", 1), "e\u{301}");
        assert_eq!(clip_to_width("a\u{1b}b", 3), "a�b");

        let mut state = TuiState::new("abcdefghijk");
        state.query_cursor = 10;
        let query_line = query_line_for_width(&state, 8);
        assert!(query_line.contains('│'));
        assert!(
            query_line
                .chars()
                .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
                .sum::<usize>()
                <= 8
        );
    }

    #[test]
    fn tc_162_preview_toggle_collapse_and_reexpansion_preserve_preference() {
        let mut state = TuiState::new("");
        state.results = vec![(PathBuf::from("selected.txt"), 1.0)];

        assert!(update_preview_visibility(
            &mut state,
            PREVIEW_MIN_WIDTH,
            PREVIEW_MIN_HEIGHT
        ));
        assert!(state.preview_visible);
        assert!(state.preview_preferred);

        let request = state
            .next_preview_request()
            .expect("visible preview request");
        assert_eq!(request.path, PathBuf::from("selected.txt"));
        assert_eq!(state.preview, "Loading preview...");

        assert!(!update_preview_visibility(
            &mut state,
            PREVIEW_MIN_WIDTH - 1,
            PREVIEW_MIN_HEIGHT
        ));
        assert!(!state.preview_visible);
        assert!(state.preview_preferred);
        assert!(state.preview.is_empty());

        assert!(update_preview_visibility(
            &mut state,
            PREVIEW_MIN_WIDTH,
            PREVIEW_MIN_HEIGHT
        ));
        assert!(state.preview_visible);
        assert!(state.preview_preferred);
        let expanded_request = state.next_preview_request().expect("re-expanded request");
        assert_ne!(expanded_request.request_id, request.request_id);

        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('p'), KeyModifiers::ALT),
        );
        assert!(!state.preview_preferred);
        assert!(!state.preview_visible);
        assert!(state.preview.is_empty());
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('p'), KeyModifiers::ALT),
        );
        assert!(state.preview_preferred);
        assert!(update_preview_visibility(
            &mut state,
            PREVIEW_MIN_WIDTH,
            PREVIEW_MIN_HEIGHT
        ));
        assert!(state.preview_visible);
        assert!(state.next_preview_request().is_some());
    }

    #[test]
    fn tc_162_preview_response_requires_matching_request_root_and_path() {
        let mut state = TuiState::new("");
        state.root = PathBuf::from("root-a");
        state.results = vec![
            (PathBuf::from("root-a/one.txt"), 1.0),
            (PathBuf::from("root-a/two.txt"), 1.0),
        ];
        update_preview_visibility(&mut state, PREVIEW_MIN_WIDTH, PREVIEW_MIN_HEIGHT);
        let request = state.next_preview_request().expect("preview request");

        apply_preview_response(
            &mut state,
            request.request_id,
            Path::new("root-b"),
            &request.path,
            "wrong root".to_string(),
        );
        assert_eq!(state.preview, "Loading preview...");

        apply_preview_response(
            &mut state,
            request.request_id.wrapping_add(1),
            &request.root,
            &request.path,
            "wrong id".to_string(),
        );
        assert_eq!(state.preview, "Loading preview...");

        state.move_selection(1);
        apply_preview_response(
            &mut state,
            request.request_id,
            &request.root,
            &request.path,
            "stale path".to_string(),
        );
        assert_eq!(state.preview, "Loading preview...");

        let request = state
            .next_preview_request()
            .expect("replacement preview request");
        assert_eq!(state.preview, "Loading preview...");
        assert_ne!(request.request_id, 1);
        apply_preview_response(
            &mut state,
            request.request_id,
            &request.root,
            &request.path,
            "fresh preview".to_string(),
        );
        assert_eq!(state.preview, "fresh preview");
    }

    #[test]
    fn tc_162_preview_request_clears_content_without_selection() {
        let mut state = TuiState::new("");
        update_preview_visibility(&mut state, PREVIEW_MIN_WIDTH, PREVIEW_MIN_HEIGHT);
        state.preview = "stale".to_string();
        state.active_preview_request = Some(PreviewRequestIdentity {
            request_id: 9,
            root: PathBuf::from("root"),
            path: PathBuf::from("root/old.txt"),
        });

        assert!(state.next_preview_request().is_none());
        assert!(state.preview.is_empty());
        assert!(state.active_preview_request.is_none());
    }

    #[test]
    fn tc_162_preview_uses_shared_text_builder_for_file_binary_and_error() {
        let root = std::env::temp_dir().join(format!(
            "flistwalker-cli-preview-{}-{}",
            std::process::id(),
            Instant::now().elapsed().as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("create preview fixture");
        let text = root.join("text.txt");
        let binary = root.join("binary.bin");
        std::fs::write(&text, "preview text").expect("write text fixture");
        std::fs::write(&binary, [0, 159, 146, 150]).expect("write binary fixture");

        assert!(build_preview_text_with_kind(&root, true).contains("Directory:"));
        assert!(build_preview_text_with_kind(&text, false).contains("preview text"));
        assert!(
            build_preview_text_with_kind(&binary, false).contains("<binary or unreadable file>")
        );
        assert!(
            build_preview_text_with_kind(&root.join("missing.txt"), false)
                .contains("<binary or unreadable file>")
        );

        std::fs::remove_dir_all(root).expect("remove preview fixture");
    }

    #[test]
    fn tc_162_preview_pane_clips_unicode_and_control_text() {
        let mut state = TuiState::new("");
        state.preview = "界界\u{1b}x\nsecond".to_string();
        let mut output = Vec::new();

        render_preview_pane(&mut output, &state, 60, 100, PREVIEW_MIN_HEIGHT)
            .expect("render preview pane");

        let rendered = String::from_utf8_lossy(&output);
        assert!(rendered.contains("Preview"));
        assert!(rendered.contains('�'));
        assert!(!rendered.contains("\u{1b}x"));
    }

    #[test]
    fn tc_162_delayed_preview_worker_cleanup_uses_the_bounded_wait() {
        let (done_tx, done_rx) = mpsc::channel();
        let handle = thread::Builder::new()
            .name("flistwalker-cli-preview-delayed-test".to_string())
            .spawn(move || {
                thread::sleep(Duration::from_millis(500));
                let _ = done_tx.send(());
            })
            .expect("start delayed preview worker");

        let started = Instant::now();
        finish_worker(handle, done_rx);
        assert!(
            started.elapsed() < Duration::from_millis(450),
            "preview cleanup exceeded the bounded wait: {:?}",
            started.elapsed()
        );
    }

    fn history_state(entries: &[&str], query: &str) -> TuiState {
        let mut state = TuiState::new(query);
        state.history_enabled = true;
        state.history_entries = entries.iter().map(|entry| (*entry).to_string()).collect();
        state.viewport_rows = 1;
        state
    }

    #[test]
    fn tc_162_history_overlay_orders_recent_entries_and_filters_fuzzily() {
        let mut state = history_state(&["old", "alpha", "beta"], "draft");
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
        );
        let history = state.history.as_ref().expect("history overlay");
        assert_eq!(history.draft_query, "draft");
        assert_eq!(history.results, vec!["draft", "beta", "alpha", "old"]);

        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE),
        );
        let history = state.history.as_ref().expect("filtered history overlay");
        assert_eq!(history.filter, "p");
        assert_eq!(history.results, vec!["alpha"]);
    }

    #[test]
    fn tc_162_history_filter_supports_enabled_emacs_editing_and_disabled_noop() {
        let mut enabled = history_state(&["alpha beta", "alpha"], "draft");
        handle_key(
            &mut enabled,
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
        );
        insert_paste(&mut enabled, "alpha beta");
        handle_key(
            &mut enabled,
            KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL),
        );
        assert_eq!(
            enabled.history.as_ref().expect("history overlay").filter,
            "alpha "
        );
        handle_key(
            &mut enabled,
            KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL),
        );
        assert_eq!(
            enabled.history.as_ref().expect("history overlay").filter,
            "alpha beta"
        );
        handle_key(
            &mut enabled,
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL),
        );
        assert_eq!(
            enabled
                .history
                .as_ref()
                .expect("history overlay")
                .filter_cursor,
            0
        );

        let mut disabled = history_state(&["alpha"], "draft");
        handle_key(
            &mut disabled,
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
        );
        insert_paste(&mut disabled, "alpha");
        disabled.emacs_keybindings_enabled = false;
        handle_key(
            &mut disabled,
            KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL),
        );
        assert_eq!(
            disabled.history.as_ref().expect("history overlay").filter,
            "alpha"
        );
    }

    #[test]
    fn tc_162_history_overlay_accept_cancel_navigation_and_paste_contract() {
        let mut state = history_state(&["one", "two", "three"], "draft");
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
        );
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
        );
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
        );
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('p'), KeyModifiers::ALT),
        );
        assert!(
            state
                .history
                .as_ref()
                .expect("history overlay")
                .filter
                .is_empty(),
            "side-effect chords must not edit the history filter"
        );
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
        );
        assert_eq!(state.history.as_ref().expect("history overlay").selected, 1);
        handle_key(&mut state, KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(state.history.as_ref().expect("history overlay").selected, 0);
        insert_paste(&mut state, "tw");
        assert_eq!(
            state.history.as_ref().expect("history overlay").results,
            vec!["two"]
        );
        assert!(matches!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
            ),
            KeyAction::HistoryApplied
        ));
        assert!(state.history.is_none());
        assert_eq!(state.query, "two");
        assert!(state.last_query_change.is_some());

        state.query = "draft again".to_string();
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
        );
        insert_paste(&mut state, "x");
        handle_key(&mut state, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(state.history.is_none());
        assert_eq!(state.query, "draft again");

        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
        );
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL),
        );
        assert!(state.history.is_none());
        assert_eq!(state.query, "draft again");
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
        );
        assert!(matches!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
            ),
            KeyAction::Cancel
        ));
    }

    #[test]
    fn tc_162_history_disabled_ctrl_r_is_a_silent_noop() {
        let mut state = TuiState::new("draft");
        state.status = "Ready".to_string();
        state.dirty = false;

        assert!(matches!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL)
            ),
            KeyAction::Continue
        ));
        assert!(state.history.is_none());
        assert_eq!(state.query, "draft");
        assert_eq!(state.status, "Ready");
        assert!(!state.dirty);
        assert!(enqueue_history_delta(None, " trimmed ").is_ok());
    }

    #[test]
    fn tc_162_history_open_commits_draft_as_the_most_recent_delta() {
        let mut state = history_state(&["first", "draft", "second"], " draft ");

        let action = handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
        );

        assert!(matches!(action, KeyAction::HistoryOpened(Some(ref query)) if query == "draft"));
        assert_eq!(state.history_entries, vec!["first", "second", "draft"]);
        assert_eq!(
            state
                .history
                .as_ref()
                .expect("history overlay")
                .results
                .first(),
            Some(&"draft".to_string())
        );
    }

    #[test]
    fn tc_162_help_overlay_has_precedence_and_ctrl_g_only_closes_it() {
        let mut state = history_state(&["prior"], "draft");
        state.pinned.push(PathBuf::from("pinned.txt"));
        state.preview_preferred = true;

        handle_key(&mut state, KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
        assert_eq!(state.help, Some(HelpContext::Normal));
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
        );
        handle_key(&mut state, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('p'), KeyModifiers::ALT),
        );
        assert_eq!(state.query, "draft");
        assert_eq!(state.pinned, vec![PathBuf::from("pinned.txt")]);
        assert!(state.preview_preferred);

        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL),
        );
        assert!(state.help.is_none());
        assert_eq!(state.query, "draft");
        assert_eq!(state.pinned, vec![PathBuf::from("pinned.txt")]);

        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL),
        );
        assert!(state.query.is_empty());
        assert!(state.pinned.is_empty());
    }

    #[test]
    fn tc_162_help_from_history_restores_history_and_ctrl_c_exits_tui() {
        let mut state = history_state(&["prior"], "draft");
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
        );
        insert_paste(&mut state, "pr");
        let filter = state
            .history
            .as_ref()
            .expect("history overlay")
            .filter
            .clone();

        handle_key(&mut state, KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
        assert_eq!(state.help, Some(HelpContext::History));
        insert_paste(&mut state, "ignored");
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert!(state.help.is_none());
        assert_eq!(
            state.history.as_ref().expect("history overlay").filter,
            filter
        );

        handle_key(&mut state, KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
        assert!(matches!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
            ),
            KeyAction::Cancel
        ));
    }

    #[test]
    fn tc_162_help_and_history_overlays_clear_the_full_terminal() {
        let history = HistoryOverlay {
            draft_query: String::new(),
            filter: String::new(),
            filter_cursor: 0,
            results: vec!["entry".to_string()],
            selected: 0,
            offset: 0,
        };
        let mut history_output = Vec::new();
        render_history_overlay(&mut history_output, &history, true, 40, 8).expect("render history");
        let mut help_output = Vec::new();
        render_help_overlay(&mut help_output, HelpContext::Normal, true, 40, 8)
            .expect("render help");

        for output in [&history_output, &help_output] {
            assert!(
                output.windows(4).any(|window| window == b"\x1b[2J"),
                "overlay must clear terminal before rendering"
            );
        }
    }

    #[test]
    fn tc_162_help_overlay_matches_emacs_runtime_config() {
        let mut enabled_output = Vec::new();
        render_help_overlay(&mut enabled_output, HelpContext::Normal, true, 100, 10)
            .expect("render enabled help");
        let enabled_text = String::from_utf8_lossy(&enabled_output);
        assert!(enabled_text.contains("Ctrl+N"));
        assert!(enabled_text.contains("Ctrl+G"));
        assert!(enabled_text.contains("Ctrl+R"));

        let mut disabled_output = Vec::new();
        render_help_overlay(&mut disabled_output, HelpContext::Normal, false, 100, 10)
            .expect("render disabled help");
        let disabled_text = String::from_utf8_lossy(&disabled_output);
        assert!(disabled_text.contains("Emacs shortcuts disabled"));
        assert!(!disabled_text.contains("Ctrl+N"));
        assert!(!disabled_text.contains("Ctrl+G"));
        assert!(!disabled_text.contains("Ctrl+R"));

        let options = OptionsOverlay {
            draft: TuiRuntimeOptions {
                include_files: true,
                include_dirs: true,
                regex: false,
                ignore_case: false,
                ignore_enabled: true,
                source: TuiSource::Walker,
            },
            selected: 0,
        };
        let history = HistoryOverlay {
            draft_query: String::new(),
            filter: String::new(),
            filter_cursor: 0,
            results: Vec::new(),
            selected: 0,
            offset: 0,
        };
        let mut overlay_outputs = vec![Vec::new(); 5];
        render_options_overlay(&mut overlay_outputs[0], &options, false, 120, 8)
            .expect("render disabled options");
        render_sort_picker(
            &mut overlay_outputs[1],
            &SortPicker { selected: 0 },
            false,
            120,
            8,
        )
        .expect("render disabled sort");
        render_root_picker(
            &mut overlay_outputs[2],
            &RootPicker { selected: 0 },
            &[PathBuf::from("root")],
            false,
            120,
            8,
        )
        .expect("render disabled roots");
        render_filelist_confirmation(
            &mut overlay_outputs[3],
            &FileListConfirmation::Mode {
                propagate_to_ancestors: false,
            },
            false,
            120,
            8,
        )
        .expect("render disabled FileList confirmation");
        render_history_overlay(&mut overlay_outputs[4], &history, false, 120, 8)
            .expect("render disabled history");
        for output in overlay_outputs {
            assert!(!String::from_utf8_lossy(&output).contains("Ctrl+G"));
        }
    }

    #[test]
    fn tc_162_f2_options_and_f3_sort_overlays_have_precedence_without_side_effects() {
        let mut state = TuiState::new("draft");
        state.results = vec![(PathBuf::from("selected.txt"), 1.0)];
        state.pinned.push(PathBuf::from("pinned.txt"));

        handle_key(&mut state, KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE));
        assert!(state.options_overlay.is_some());
        assert!(matches!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL)
            ),
            KeyAction::Continue
        ));
        assert_eq!(state.query, "draft");
        assert_eq!(state.pinned, vec![PathBuf::from("pinned.txt")]);
        handle_key(&mut state, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

        handle_key(&mut state, KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE));
        assert!(state.sort_picker.is_some());
        assert!(matches!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
            ),
            KeyAction::Continue
        ));
        assert_eq!(state.sort_mode, SearchSortMode::Score);
    }

    #[test]
    fn tc_162_tui_sort_picker_has_all_nine_shared_modes_and_query_resets_score() {
        assert_eq!(SORT_MODES.len(), 9);
        assert_eq!(SORT_MODES[0], SearchSortMode::Score);
        assert_eq!(SORT_MODES[8], SearchSortMode::SizeAsc);
        let mut state = TuiState::new("draft");
        state.sort_mode = SearchSortMode::SizeDesc;

        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
        );

        assert_eq!(state.sort_mode, SearchSortMode::Score);
    }

    #[test]
    fn tc_162_tui_options_reindex_only_for_scope_or_source_changes() {
        let base = TuiRuntimeOptions::from_startup(&CliTuiOptions {
            initial_query: String::new(),
            limit: 10,
            absolute: false,
            print0: false,
            include_files: true,
            include_dirs: true,
            use_filelist: true,
            require_filelist: false,
            regex: false,
            ignore_case: false,
            ignore_enabled: true,
            ignore_terms: vec!["ignored".to_string()],
            sort_mode: SearchSortMode::Score,
        });
        let mut search_only = base;
        search_only.regex = true;
        search_only.ignore_case = true;
        search_only.ignore_enabled = false;
        assert!(!option_change_requires_reindex(base, search_only));
        let mut reindex = base;
        reindex.include_files = false;
        assert!(option_change_requires_reindex(base, reindex));
        let mut source = base;
        source.source = TuiSource::Walker;
        assert!(option_change_requires_reindex(base, source));
    }

    #[test]
    fn tc_162_options_never_disable_both_files_and_folders() {
        let mut overlay = OptionsOverlay {
            draft: TuiRuntimeOptions {
                include_files: true,
                include_dirs: false,
                regex: false,
                ignore_case: false,
                ignore_enabled: true,
                source: TuiSource::Auto,
            },
            selected: 0,
        };
        toggle_option(&mut overlay);
        assert!(overlay.draft.include_files);
        overlay.draft.include_dirs = true;
        toggle_option(&mut overlay);
        assert!(!overlay.draft.include_files);
        overlay.selected = 1;
        toggle_option(&mut overlay);
        assert!(overlay.draft.include_dirs);
    }

    #[test]
    fn tc_162_stale_index_responses_are_discarded_by_identity() {
        let mut state = TuiState::new("");
        state.active_index_request = Some((2, PathBuf::from("root-b")));
        apply_worker_response(
            &mut state,
            WorkerResponse::IndexTruncated {
                request_id: 1,
                root: PathBuf::from("root-a"),
                limit: 3,
            },
        )
        .expect("stale truncation ignored");
        assert_eq!(state.index_truncated_limit, None);
        apply_worker_response(
            &mut state,
            WorkerResponse::IndexedBatch {
                request_id: 1,
                root: PathBuf::from("root-a"),
                entries: vec![PathBuf::from("stale.txt")],
            },
        )
        .expect("stale response ignored");
        assert_eq!(state.entries.len(), 0);
        apply_worker_response(
            &mut state,
            WorkerResponse::IndexedBatch {
                request_id: 2,
                root: PathBuf::from("root-b"),
                entries: vec![PathBuf::from("fresh.txt")],
            },
        )
        .expect("fresh response accepted");
        assert_eq!(state.entries.len(), 1);
        assert_eq!(
            state.entries.snapshot()[0].as_ref(),
            [PathBuf::from("fresh.txt")]
        );
        apply_worker_response(
            &mut state,
            WorkerResponse::IndexTruncated {
                request_id: 2,
                root: PathBuf::from("root-b"),
                limit: 5,
            },
        )
        .expect("fresh truncation accepted");
        apply_worker_response(
            &mut state,
            WorkerResponse::IndexedFinished {
                request_id: 2,
                root: PathBuf::from("root-b"),
                has_root_filelist: false,
            },
        )
        .expect("fresh finish accepted");
        assert!(state.status.contains("Walker capped at 5 entries"));
        assert_eq!(state.index_truncated_limit, Some(5));

        state.root = PathBuf::from("root-b");
        state.active_search_request_id = Some(9);
        let options = state.runtime_options.search_options(state.sort_mode);
        apply_worker_response(
            &mut state,
            WorkerResponse::Searched {
                request_id: 9,
                root: PathBuf::from("root-b"),
                query: String::new(),
                options,
                results: vec![(PathBuf::from("fresh.txt"), 1.0)],
                error: None,
            },
        )
        .expect("search response accepted");
        assert!(state.status.contains("Walker capped at 5 entries"));
    }

    #[test]
    fn tc_162_tui_search_applies_ignore_in_worker_snapshot_and_sorts_before_limit() {
        let entries = Arc::new(vec![Arc::from(vec![
            PathBuf::from("root/zeta.txt"),
            PathBuf::from("root/ignored.txt"),
            PathBuf::from("root/alpha.txt"),
        ])]);
        let mut cache = SearchPrefixCache::default();
        let request = SearchRequest {
            request_id: 1,
            query: String::new(),
            entries: Arc::clone(&entries),
            root: PathBuf::from("root"),
            limit: 2,
            options: SearchOptions {
                regex: false,
                ignore_case: false,
                ignore_enabled: true,
                sort_mode: SearchSortMode::NameAsc,
            },
            ignore_terms: Arc::new(vec!["ignored".to_string()]),
        };
        let (results, error) = search(&request, &mut cache);
        assert!(error.is_none());
        assert_eq!(
            results.iter().map(|(path, _)| path).collect::<Vec<_>>(),
            vec![
                &PathBuf::from("root/alpha.txt"),
                &PathBuf::from("root/zeta.txt")
            ],
            "Name sort must run over all non-ignored matches before limit"
        );

        let unignored = SearchRequest {
            options: SearchOptions {
                ignore_enabled: false,
                ..request.options
            },
            ..request
        };
        let (results, error) = search(&unignored, &mut cache);
        assert!(error.is_none());
        assert_eq!(results.len(), 2);
        assert!(results
            .iter()
            .any(|(path, _)| path.ends_with("ignored.txt")));
    }

    #[test]
    fn tc_163_disabled_startup_ignore_can_be_reenabled_without_reloading_terms() {
        let startup = CliTuiOptions {
            initial_query: String::new(),
            limit: 10,
            absolute: false,
            print0: false,
            include_files: true,
            include_dirs: true,
            use_filelist: true,
            require_filelist: false,
            regex: false,
            ignore_case: true,
            ignore_enabled: false,
            ignore_terms: vec!["ignored".to_string()],
            sort_mode: SearchSortMode::Score,
        };
        let mut runtime = TuiRuntimeOptions::from_startup(&startup);
        assert!(!runtime.ignore_enabled);

        runtime.ignore_enabled = true;
        let request = SearchRequest {
            request_id: 1,
            query: String::new(),
            entries: Arc::new(vec![Arc::from(vec![
                PathBuf::from("root/visible.txt"),
                PathBuf::from("root/ignored.txt"),
            ])]),
            root: PathBuf::from("root"),
            limit: 10,
            options: runtime.search_options(SearchSortMode::Score),
            ignore_terms: Arc::new(startup.ignore_terms),
        };

        let (results, error) = search(&request, &mut SearchPrefixCache::default());

        assert!(error.is_none());
        assert_eq!(results.len(), 1);
        assert!(results[0].0.ends_with("visible.txt"));
    }

    #[test]
    fn tc_162_newer_index_identity_supersedes_an_in_progress_request() {
        let freshness = TuiIndexFreshness::new();
        freshness.activate(1);
        assert!(freshness.is_current(1));

        freshness.activate(2);
        assert!(
            !freshness.is_current(1),
            "walker/FileList cancellation closure must stop the superseded request"
        );
        assert!(freshness.is_current(2));
    }

    #[test]
    fn tc_162_applied_options_reset_sort_only_when_the_draft_changes() {
        let mut state = TuiState::new("query");
        state.sort_mode = SearchSortMode::SizeDesc;
        state.options_overlay = Some(OptionsOverlay {
            draft: state.runtime_options,
            selected: 0,
        });
        handle_options_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert_eq!(state.sort_mode, SearchSortMode::SizeDesc);

        state.sort_mode = SearchSortMode::SizeDesc;
        let mut changed = state.runtime_options;
        changed.ignore_case = !changed.ignore_case;
        state.options_overlay = Some(OptionsOverlay {
            draft: changed,
            selected: 0,
        });
        handle_options_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert_eq!(state.sort_mode, SearchSortMode::Score);
        assert!(state.active_search_request_id.is_none());
    }

    #[test]
    fn tc_162_source_transition_clears_source_scoped_state_before_reindex() {
        let mut state = TuiState::new("");
        state.root = PathBuf::from("root");
        state.pinned.push(PathBuf::from("root/pinned.txt"));
        state.preview = "old preview".to_string();
        state.active_preview_request = Some(PreviewRequestIdentity {
            request_id: 3,
            root: state.root.clone(),
            path: PathBuf::from("root/old.txt"),
        });
        state.active_action_request = Some((4, PathBuf::from("root/old.txt")));
        state.source_changed_on_apply = true;
        let action_freshness = TuiActionFreshness::new();
        action_freshness.activate(4, &state.root);

        prepare_source_transition(&mut state, &action_freshness, Path::new("root"));

        assert!(state.pinned.is_empty());
        assert!(state.preview.is_empty());
        assert!(state.active_preview_request.is_none());
        assert!(state.active_action_request.is_none());
        assert!(!state.source_changed_on_apply);
        assert!(!action_freshness.is_current(4, Path::new("root")));
    }

    #[test]
    fn tc_162_every_reindex_clears_current_preview_and_pending_search_without_clearing_pins() {
        let mut state = TuiState::new("");
        state.root = PathBuf::from("root");
        state.results = vec![(PathBuf::from("root/current.txt"), 1.0)];
        state.pinned.push(PathBuf::from("root/pinned.txt"));
        state.preview = "stale preview".to_string();
        state.active_preview_request = Some(PreviewRequestIdentity {
            request_id: 1,
            root: state.root.clone(),
            path: PathBuf::from("root/current.txt"),
        });
        state.active_search_request_id = Some(2);

        state.next_index_request(state.root.clone());

        assert!(state.results.is_empty());
        assert!(state.preview.is_empty());
        assert!(state.active_preview_request.is_none());
        assert!(state.active_search_request_id.is_none());
        assert_eq!(state.pinned, vec![PathBuf::from("root/pinned.txt")]);
    }

    #[test]
    fn tc_162_root_picker_precedence_empty_state_and_small_viewport_are_safe() {
        let mut state = TuiState::new("query");
        state.results = vec![(PathBuf::from("current.txt"), 1.0)];
        handle_key(&mut state, KeyEvent::new(KeyCode::F(4), KeyModifiers::NONE));
        assert!(state.root_picker.is_some());
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
        );
        assert_eq!(state.query, "query");
        assert!(state.root_picker.is_some());
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert!(state.root_picker.is_none());

        let roots = (0..6)
            .map(|index| PathBuf::from(format!("root-{index}")))
            .collect::<Vec<_>>();
        let mut output = Vec::new();
        render_root_picker(
            &mut output,
            &RootPicker { selected: 5 },
            &roots,
            true,
            80,
            4,
        )
        .expect("render roots");
        assert!(String::from_utf8_lossy(&output).contains("> root-5"));
        let mut output = Vec::new();
        render_root_picker(&mut output, &RootPicker { selected: 0 }, &[], true, 80, 4)
            .expect("render empty roots");
        assert!(String::from_utf8_lossy(&output).contains("No saved roots"));
    }

    #[test]
    fn tc_162_root_switch_clears_old_scope_before_new_index_and_preserves_query_options_history() {
        let mut state = TuiState::new("keep query");
        state.root = PathBuf::from("old-root");
        state.history_enabled = true;
        state.history_entries = vec!["history".to_string()];
        state.runtime_options.regex = true;
        state.results = vec![(PathBuf::from("old-root/current.txt"), 1.0)];
        state.pinned.push(PathBuf::from("old-root/pinned.txt"));
        state.preview = "old preview".to_string();
        state.active_search_request_id = Some(5);
        let freshness = TuiActionFreshness::new();
        freshness.activate(7, Path::new("old-root"));
        state.active_action_request = Some((7, PathBuf::from("old-root/current.txt")));

        prepare_root_switch(&mut state, &freshness, PathBuf::from("new-root"));
        state.next_index_request(state.root.clone());

        assert_eq!(state.root, PathBuf::from("new-root"));
        assert!(state.results.is_empty());
        assert!(state.pinned.is_empty());
        assert!(state.preview.is_empty());
        assert!(state.active_search_request_id.is_none());
        assert!(state.active_action_request.is_none());
        assert_eq!(state.query, "keep query");
        assert!(state.runtime_options.regex);
        assert_eq!(state.history_entries, vec!["history"]);
        assert!(!freshness.is_current(7, Path::new("old-root")));
    }

    #[test]
    fn tc_162_root_picker_selects_the_highlighted_root_and_refresh_keeps_pins() {
        let mut state = TuiState::new("");
        state.root = PathBuf::from("old-root");
        state.saved_roots = vec![PathBuf::from("first"), PathBuf::from("second")];
        state.root_picker = Some(RootPicker { selected: 1 });
        assert!(matches!(
            handle_root_picker_key(&mut state, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            KeyAction::SwitchRoot(ref root) if root == Path::new("second")
        ));
        state.pinned.push(PathBuf::from("old-root/pinned.txt"));
        state.results = vec![(PathBuf::from("old-root/current.txt"), 1.0)];
        state.next_index_request(state.root.clone());
        assert_eq!(state.pinned, vec![PathBuf::from("old-root/pinned.txt")]);
        assert!(state.results.is_empty());
    }

    #[test]
    fn tc_162_options_overlay_keeps_headings_and_renders_items_below_them() {
        let overlay = OptionsOverlay {
            draft: TuiRuntimeOptions::from_startup(&CliTuiOptions {
                initial_query: String::new(),
                limit: 1,
                absolute: false,
                print0: false,
                include_files: true,
                include_dirs: true,
                use_filelist: true,
                require_filelist: false,
                regex: false,
                ignore_case: false,
                ignore_enabled: true,
                ignore_terms: Vec::new(),
                sort_mode: SearchSortMode::Score,
            }),
            selected: 0,
        };
        let mut output = Vec::new();
        render_options_overlay(&mut output, &overlay, true, 80, 5).expect("render options");
        let rendered = String::from_utf8_lossy(&output);
        assert!(rendered.contains("Options"));
        assert!(rendered.contains("Enter apply"));
        assert!(rendered.contains("\x1b[3;1H> Files:"), "{rendered:?}");
    }

    #[test]
    fn tc_162_paste_is_confined_to_history_and_never_leaks_through_modal_overlays() {
        let mut state = TuiState::new("query");
        state.options_overlay = Some(OptionsOverlay {
            draft: state.runtime_options,
            selected: 0,
        });
        insert_paste(&mut state, " leaked");
        assert_eq!(state.query, "query");
        state.options_overlay = None;
        state.sort_picker = Some(SortPicker { selected: 0 });
        insert_paste(&mut state, " leaked");
        assert_eq!(state.query, "query");
        state.sort_picker = None;
        state.root_picker = Some(RootPicker { selected: 0 });
        insert_paste(&mut state, " leaked");
        assert_eq!(state.query, "query");
        state.root_picker = None;

        state.history_enabled = true;
        state.history_entries = vec!["history".to_string()];
        state.begin_history();
        insert_paste(&mut state, "hi");
        assert_eq!(state.history.as_ref().expect("history").filter, "hi");
    }

    #[test]
    fn tc_162_root_switch_and_refresh_reset_sort_and_pending_search() {
        let freshness = TuiActionFreshness::new();
        let mut state = TuiState::new("");
        state.root = PathBuf::from("old-root");
        state.sort_mode = SearchSortMode::SizeDesc;
        state.active_search_request_id = Some(4);
        prepare_root_switch(&mut state, &freshness, PathBuf::from("new-root"));
        assert_eq!(state.sort_mode, SearchSortMode::Score);
        assert!(state.active_search_request_id.is_none());

        state.sort_mode = SearchSortMode::NameDesc;
        state.active_search_request_id = Some(5);
        prepare_refresh(&mut state);
        state.next_index_request(state.root.clone());
        assert_eq!(state.sort_mode, SearchSortMode::Score);
        assert!(state.active_search_request_id.is_none());
    }

    #[test]
    fn tc_162_active_root_relative_output_is_prepared_after_terminal_cleanup() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let guard = TerminalGuard::start(
            FakeTerminalOps {
                calls: Rc::clone(&calls),
                fail_on: None,
            },
            Vec::<u8>::new(),
        )
        .expect("terminal setup");
        let active_root = PathBuf::from("active-root");
        let path = active_root.join("selected.txt");
        let selected = run_terminal_operation(guard, |_writer| Ok((path, active_root.clone())))
            .expect("terminal operation");
        calls.borrow_mut().push("stdout_output");
        assert_eq!(
            output_path_bytes(&selected.0, &selected.1, true, false),
            b"selected.txt"
        );
        let disable_raw = calls
            .borrow()
            .iter()
            .position(|call| *call == "disable_raw")
            .expect("raw cleanup");
        let stdout_output = calls
            .borrow()
            .iter()
            .position(|call| *call == "stdout_output")
            .expect("stdout output");
        assert!(disable_raw < stdout_output);
    }

    #[test]
    fn tc_162_small_overlays_keep_source_and_size_selection_visible() {
        assert_eq!(overlay_window_start(5, 6, 2), 4);
        assert_eq!(overlay_window_start(8, 9, 2), 7);
        let options = OptionsOverlay {
            draft: TuiRuntimeOptions::from_startup(&CliTuiOptions {
                initial_query: String::new(),
                limit: 1,
                absolute: false,
                print0: false,
                include_files: true,
                include_dirs: true,
                use_filelist: true,
                require_filelist: false,
                regex: false,
                ignore_case: false,
                ignore_enabled: true,
                ignore_terms: Vec::new(),
                sort_mode: SearchSortMode::Score,
            }),
            selected: 5,
        };
        let mut output = Vec::new();
        render_options_overlay(&mut output, &options, true, 80, 4).expect("render options");
        assert!(String::from_utf8_lossy(&output).contains("> Source:"));
        let mut output = Vec::new();
        render_sort_picker(&mut output, &SortPicker { selected: 8 }, true, 80, 4)
            .expect("render sort");
        assert!(String::from_utf8_lossy(&output).contains("> Size (Small)"));
    }

    #[derive(Default)]
    struct RecordingTuiActionBackend {
        calls: Mutex<Vec<(AuthorizedActionMode, PathBuf)>>,
        fail: bool,
    }

    impl AuthorizedActionBackend for RecordingTuiActionBackend {
        fn execute_or_open(&self, path: &Path) -> Result<()> {
            self.calls
                .lock()
                .expect("record action")
                .push((AuthorizedActionMode::ExecuteOrOpen, path.to_path_buf()));
            if self.fail {
                anyhow::bail!("raw executor path and failure detail")
            }
            Ok(())
        }

        fn reveal(&self, path: &Path) -> Result<()> {
            self.calls
                .lock()
                .expect("record action")
                .push((AuthorizedActionMode::Reveal, path.to_path_buf()));
            if self.fail {
                anyhow::bail!("raw executor path and failure detail")
            }
            Ok(())
        }
    }

    fn action_fixture(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "flistwalker-tui-action-{name}-{}-{}",
            std::process::id(),
            Instant::now().elapsed().as_nanos()
        ));
        std::fs::create_dir_all(root.join("folder")).expect("create action fixture");
        let current = root.join("current.txt");
        let pinned = root.join("folder").join("pinned.txt");
        std::fs::write(&current, "current").expect("write current");
        std::fs::write(&pinned, "pinned").expect("write pinned");
        (root, current, pinned)
    }

    #[test]
    fn tc_164_tui_actions_snapshot_only_the_current_row_not_pins() {
        let (root, current, pinned) = action_fixture("current-only");
        let mut state = TuiState::new("");
        state.root = root.clone();
        state.results = vec![(current.clone(), 1.0)];
        state.pinned.push(pinned.clone());
        let freshness = TuiActionFreshness::new();
        let request = state
            .next_action_request(
                AuthorizedActionMode::ExecuteOrOpen,
                &freshness,
                Arc::new(AtomicBool::new(false)),
            )
            .expect("action request");
        assert_eq!(request.request.selected_targets, vec![current.clone()]);

        let backend = RecordingTuiActionBackend::default();
        let report = execute_authorized_action_request(&request.request, &freshness, &backend);
        assert_eq!(report.outcome, AuthorizedActionOutcome::Completed);
        let calls = backend.calls.lock().expect("calls");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, AuthorizedActionMode::ExecuteOrOpen);
        assert!(calls[0].1.ends_with("current.txt"));
        drop(calls);
        assert!(!backend
            .calls
            .lock()
            .expect("calls")
            .iter()
            .any(|(_, path)| path == &pinned));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn tc_164_tui_reveal_is_current_only_and_preauthorization_blocks_zero_calls() {
        let (root, current, _) = action_fixture("reveal-and-block");
        let freshness = TuiActionFreshness::new();
        freshness.activate(1, &root);
        let reveal = AuthorizedActionRequest::new_with_cancellation(
            1,
            root.clone(),
            vec![current.clone()],
            AuthorizedActionMode::Reveal,
            Arc::new(AtomicBool::new(false)),
        );
        let backend = RecordingTuiActionBackend::default();
        let report = execute_authorized_action_request(&reveal, &freshness, &backend);
        assert_eq!(report.outcome, AuthorizedActionOutcome::Completed);
        let calls = backend.calls.lock().expect("calls");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, AuthorizedActionMode::Reveal);
        assert!(calls[0]
            .1
            .ends_with(root.file_name().expect("fixture root name")));

        freshness.activate(2, &root);
        let outside = root
            .parent()
            .expect("fixture parent")
            .join("outside-action.txt");
        std::fs::write(&outside, "outside").expect("write outside");
        let blocked = AuthorizedActionRequest::new_with_cancellation(
            2,
            root.clone(),
            vec![outside.clone()],
            AuthorizedActionMode::ExecuteOrOpen,
            Arc::new(AtomicBool::new(false)),
        );
        let blocked_backend = RecordingTuiActionBackend::default();
        let report = execute_authorized_action_request(&blocked, &freshness, &blocked_backend);
        assert_eq!(report.outcome, AuthorizedActionOutcome::Blocked);
        assert!(blocked_backend.calls.lock().expect("calls").is_empty());
        let _ = std::fs::remove_file(outside);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn tc_164_tui_action_stale_cancel_and_executor_errors_are_safe() {
        let (root, current, _) = action_fixture("stale-cancel-error");
        let freshness = TuiActionFreshness::new();
        freshness.activate(1, &root);
        let cancellation = Arc::new(AtomicBool::new(false));
        let request = AuthorizedActionRequest::new_with_cancellation(
            1,
            root.clone(),
            vec![current.clone()],
            AuthorizedActionMode::ExecuteOrOpen,
            Arc::clone(&cancellation),
        );
        freshness.activate(2, &root);
        let backend = RecordingTuiActionBackend::default();
        let report = execute_authorized_action_request(&request, &freshness, &backend);
        assert_eq!(report.outcome, AuthorizedActionOutcome::Superseded);
        assert!(backend.calls.lock().expect("calls").is_empty());

        freshness.activate(3, &root);
        cancellation.store(true, Ordering::Release);
        let canceled = AuthorizedActionRequest::new_with_cancellation(
            3,
            root.clone(),
            vec![current.clone()],
            AuthorizedActionMode::ExecuteOrOpen,
            Arc::clone(&cancellation),
        );
        let report = execute_authorized_action_request(&canceled, &freshness, &backend);
        assert_eq!(report.outcome, AuthorizedActionOutcome::Canceled);
        assert!(backend.calls.lock().expect("calls").is_empty());

        let failing_backend = RecordingTuiActionBackend {
            calls: Mutex::default(),
            fail: true,
        };
        let active = AuthorizedActionRequest::new_with_cancellation(
            4,
            root.clone(),
            vec![current.clone()],
            AuthorizedActionMode::ExecuteOrOpen,
            Arc::new(AtomicBool::new(false)),
        );
        freshness.activate(4, &root);
        let report = execute_authorized_action_request(&active, &freshness, &failing_backend);
        assert_eq!(report.outcome, AuthorizedActionOutcome::Failed);
        assert_eq!(tui_action_status(&report), "Action failed: executor failed");
        assert!(!tui_action_status(&report).contains("raw executor"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn tc_162_tui_action_keys_are_current_only_and_disabled_in_overlays() {
        let mut state = TuiState::new("");
        state.results = vec![(PathBuf::from("current.txt"), 1.0)];
        state.pinned.push(PathBuf::from("pinned.txt"));
        assert!(matches!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL)
            ),
            KeyAction::DispatchAction(AuthorizedActionMode::ExecuteOrOpen)
        ));
        assert!(matches!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT)
            ),
            KeyAction::DispatchAction(AuthorizedActionMode::Reveal)
        ));

        state.history_enabled = true;
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
        );
        assert!(matches!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL)
            ),
            KeyAction::Continue
        ));
        state.open_help();
        assert!(matches!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT)
            ),
            KeyAction::Continue
        ));
    }

    #[test]
    fn tc_162_history_overlay_renderer_clips_control_text() {
        let mut history = HistoryOverlay {
            draft_query: String::new(),
            filter: "\u{1b}x".to_string(),
            filter_cursor: 2,
            results: vec!["界\u{1b}x".to_string()],
            selected: 0,
            offset: 0,
        };
        refresh_history_results(&mut history, &["界\u{1b}x".to_string()]);
        let mut output = Vec::new();
        render_history_overlay(&mut output, &history, true, 12, 6).expect("render history overlay");
        let rendered = String::from_utf8_lossy(&output);
        assert!(rendered.contains("History"));
        assert!(rendered.contains('�'));
        assert!(!rendered.contains("\u{1b}x"));
    }

    #[test]
    fn tc_162_tui_frame_is_wrapped_in_synchronized_terminal_update() {
        let mut output = Vec::new();

        write_synchronized_frame(&mut output, b"frame").expect("write synchronized frame");

        let rendered = String::from_utf8(output).expect("terminal output is UTF-8");
        let begin = rendered
            .find("\x1b[?2026h")
            .expect("begin synchronized update");
        let frame = rendered.find("frame").expect("frame payload");
        let end = rendered
            .find("\x1b[?2026l")
            .expect("end synchronized update");
        assert!(begin < frame && frame < end, "{rendered:?}");
    }

    #[test]
    fn tc_162_tty_policy_requires_stdin_and_stderr_only() {
        assert!(interactive_terminal_supported(true, true));
        assert!(!interactive_terminal_supported(false, true));
        assert!(!interactive_terminal_supported(true, false));
    }

    #[test]
    fn tc_162_terminal_guard_restores_only_successful_setup_steps() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let result = TerminalGuard::start(
            FakeTerminalOps {
                calls: Rc::clone(&calls),
                fail_on: Some("hide_cursor"),
            },
            Vec::<u8>::new(),
        );

        assert!(result.is_err());
        assert_eq!(
            calls.borrow().as_slice(),
            [
                "enable_raw",
                "enter_alternate",
                "hide_cursor",
                "leave_alternate",
                "disable_raw",
            ]
        );
    }

    #[test]
    fn tc_162_terminal_guard_restores_in_reverse_order_during_unwind() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let unwind_calls = Rc::clone(&calls);
        let result = catch_unwind(AssertUnwindSafe(move || {
            let _guard = TerminalGuard::start(
                FakeTerminalOps {
                    calls: unwind_calls,
                    fail_on: None,
                },
                Vec::<u8>::new(),
            )
            .expect("terminal setup");
            panic!("simulated runtime failure");
        }));

        assert!(result.is_err());
        assert_eq!(
            calls.borrow().as_slice(),
            [
                "enable_raw",
                "enter_alternate",
                "hide_cursor",
                "enable_paste",
                "disable_paste",
                "show_cursor",
                "leave_alternate",
                "disable_raw",
            ]
        );
    }

    #[test]
    fn tc_162_runtime_error_restores_terminal_before_propagation() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let guard = TerminalGuard::start(
            FakeTerminalOps {
                calls: Rc::clone(&calls),
                fail_on: None,
            },
            Vec::<u8>::new(),
        )
        .expect("terminal setup");

        let result: Result<()> =
            run_terminal_operation(guard, |_writer| anyhow::bail!("simulated draw/read error"));

        assert!(result.is_err());
        assert_eq!(
            calls.borrow().as_slice(),
            [
                "enable_raw",
                "enter_alternate",
                "hide_cursor",
                "enable_paste",
                "disable_paste",
                "show_cursor",
                "leave_alternate",
                "disable_raw",
            ]
        );
    }

    #[test]
    fn tc_162_selected_output_is_emitted_only_after_terminal_cleanup() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let guard = TerminalGuard::start(
            FakeTerminalOps {
                calls: Rc::clone(&calls),
                fail_on: None,
            },
            Vec::<u8>::new(),
        )
        .expect("terminal setup");

        let selected =
            run_terminal_operation(guard, |_writer| Ok(vec![PathBuf::from("selected.txt")]))
                .expect("terminal operation");
        calls.borrow_mut().push("stdout_output");

        assert_eq!(selected, vec![PathBuf::from("selected.txt")]);
        assert_eq!(calls.borrow().last(), Some(&"stdout_output"));
        let disable_raw = calls
            .borrow()
            .iter()
            .position(|call| *call == "disable_raw")
            .expect("raw cleanup");
        let stdout_output = calls
            .borrow()
            .iter()
            .position(|call| *call == "stdout_output")
            .expect("stdout output");
        assert!(disable_raw < stdout_output);
    }

    #[test]
    fn tc_166_filelist_confirmation_requires_explicit_scope_and_overwrite_consent() {
        let mut state = TuiState::new("draft");
        state.root = PathBuf::from("fixture-root");
        state.root_filelist_known = true;
        state.root_filelist_exists = true;

        assert!(matches!(
            handle_key(&mut state, KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE)),
            KeyAction::OpenFileList
        ));
        state.open_filelist_confirmation();
        insert_paste(&mut state, " must-not-leak");
        assert_eq!(state.query, "draft");
        assert!(matches!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
            ),
            KeyAction::Continue
        ));
        assert!(matches!(
            state.filelist_confirmation,
            Some(FileListConfirmation::Overwrite { .. })
        ));
        assert!(matches!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
            ),
            KeyAction::StartFileList {
                propagate_to_ancestors: false,
                allow_root_overwrite: true,
            }
        ));
        assert!(state.filelist_confirmation.is_none());
    }

    #[test]
    fn tc_166_filelist_uses_fresh_walker_snapshot_not_partial_tui_entries() {
        let temp = TestTempDir::new("filelist-fresh-walker");
        let nested = temp.path.join("nested");
        fs::create_dir_all(&nested).expect("create nested directory");
        fs::write(temp.path.join("visible.txt"), "visible").expect("write visible file");
        fs::write(nested.join("inside.txt"), "inside").expect("write nested file");
        fs::write(temp.path.join("FileList.txt"), "stale-entry\n").expect("write old FileList");

        let mut state = TuiState::new("");
        state.root = temp.path.clone();
        state.runtime_options.include_files = true;
        state.runtime_options.include_dirs = false;
        state.entries.push(vec![temp.path.join("visible.txt")]);
        let request = state.next_filelist_request(false, true);

        let entries =
            build_filelist_snapshot(&request.root, &|| false).expect("fresh walker snapshot");
        assert!(entries.iter().any(|entry| entry.ends_with("visible.txt")));
        assert!(entries.iter().any(|entry| entry.ends_with("nested")));
        assert!(entries.iter().any(|entry| entry.ends_with("inside.txt")));
        assert!(
            !entries.iter().any(|entry| entry.ends_with("FileList.txt")),
            "the root FileList must not list itself"
        );

        let plan = plan_filelist_write_cancellable(
            &request.root,
            &entries,
            FileListWriteOptions {
                allow_root_overwrite: request.allow_root_overwrite,
                propagate_to_ancestors: request.propagate_to_ancestors,
            },
            &|| false,
        )
        .expect("write plan");
        let report = execute_filelist_write_plan(&plan, &|| false);
        assert_eq!(report.status, FileListWriteStatus::Completed);
        let text = fs::read_to_string(temp.path.join("FileList.txt")).expect("read FileList");
        assert!(text.contains("visible.txt"));
        assert!(text.contains("nested"));
        assert!(text.contains("inside.txt"));
        assert!(!text.contains("FileList.txt"));
    }

    #[test]
    fn tc_166_filelist_fresh_walk_cancellation_is_a_clean_report() {
        let temp = TestTempDir::new("filelist-fresh-walk-cancel");
        fs::write(temp.path.join("candidate.txt"), "candidate").expect("write candidate");
        let cancelled = AtomicBool::new(true);

        let report = build_filelist_snapshot(&temp.path, &|| cancelled.load(Ordering::Acquire))
            .expect_err("cancelled walk must not reach planning");

        assert_eq!(report.status, FileListWriteStatus::Canceled);
        assert_eq!(report.exit_code(), 130);
        assert!(report.committed.is_empty());
        assert!(report.failed.is_empty());
        assert!(!temp.path.join("FileList.txt").exists());
    }

    #[test]
    fn tc_166_filelist_requires_completed_index_and_intent_priority_is_sticky() {
        let mut state = TuiState::new("");
        state.open_filelist_if_ready();
        assert!(state.filelist_confirmation.is_none());
        assert_eq!(
            state.status,
            "Wait for indexing to finish before creating FileList"
        );
        state.root_filelist_known = true;
        state.open_filelist_if_ready();
        assert!(state.filelist_confirmation.is_some());
        state.filelist_confirmation = None;
        let request = state.next_filelist_request(false, false);
        assert!(matches!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL)
            ),
            KeyAction::Cancel
        ));
        state.record_filelist_intent(PendingFileListIntent::SelectOutput);
        assert_eq!(
            state.pending_filelist_intent,
            Some(PendingFileListIntent::SelectOutput)
        );
        state.record_filelist_intent(PendingFileListIntent::SwitchRoot(PathBuf::from("first")));
        assert_eq!(
            state.pending_filelist_intent,
            Some(PendingFileListIntent::SwitchRoot(PathBuf::from("first")))
        );
        state.record_filelist_intent(PendingFileListIntent::SwitchRoot(PathBuf::from("latest")));
        assert_eq!(
            state.pending_filelist_intent,
            Some(PendingFileListIntent::SwitchRoot(PathBuf::from("latest")))
        );
        state.record_filelist_intent(PendingFileListIntent::CancelExit);
        assert_eq!(
            state.pending_filelist_intent,
            Some(PendingFileListIntent::CancelExit)
        );
        state.record_filelist_intent(PendingFileListIntent::SwitchRoot(PathBuf::from("ignored")));
        state.record_filelist_intent(PendingFileListIntent::SelectOutput);
        assert_eq!(
            state.pending_filelist_intent,
            Some(PendingFileListIntent::CancelExit)
        );
        assert!(request.cancel.load(Ordering::Acquire));
    }

    #[test]
    fn tc_166_filelist_failure_does_not_resume_select_or_root_but_cancel_exits_one() {
        let (index_tx, _index_rx) = mpsc::channel();
        let freshness = TuiIndexFreshness::new();
        let actions = TuiActionFreshness::new();
        let mut state = TuiState::new("");
        state.pending_filelist_intent = Some(PendingFileListIntent::SelectOutput);
        assert!(settle_filelist(
            &mut state,
            FileListSettlement::Failed("rollback failed".to_string()),
            &index_tx,
            &freshness,
            &actions,
        )
        .is_none());
        state.pending_filelist_intent = Some(PendingFileListIntent::CancelExit);
        assert!(matches!(
            settle_filelist(
                &mut state,
                FileListSettlement::Failed("rollback failed".to_string()),
                &index_tx,
                &freshness,
                &actions,
            ),
            Some(TuiExit::Failed(_))
        ));

        state.root = PathBuf::from("before");
        state.pending_filelist_intent =
            Some(PendingFileListIntent::SwitchRoot(PathBuf::from("after")));
        assert!(settle_filelist(
            &mut state,
            FileListSettlement::Failed("rollback failed".to_string()),
            &index_tx,
            &freshness,
            &actions,
        )
        .is_none());
        assert_eq!(state.root, PathBuf::from("before"));
    }

    #[test]
    fn tc_166_filelist_worker_join_never_detaches_a_delayed_transaction() {
        let temp = TestTempDir::new("filelist-join");
        let marker = temp.path.join("FileList.txt");
        let (result_tx, result_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let finished = Arc::new(AtomicBool::new(false));
        let worker_finished = Arc::clone(&finished);
        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(300));
            fs::write(&marker, "committed\n").expect("write delayed FileList marker");
            worker_finished.store(true, Ordering::Release);
            let _ = result_tx.send(FileListWorkerResult::Failed {
                request_id: 1,
                root: PathBuf::from("fixture"),
                error: "injected missing response path".to_string(),
            });
            let _ = done_tx.send(());
        });
        let worker = ActiveFileListWorker {
            cancel: Arc::new(AtomicBool::new(false)),
            result: result_rx,
            done: done_rx,
            handle: Some(handle),
        };
        let started = Instant::now();
        worker.join();
        assert!(
            started.elapsed() >= Duration::from_millis(250),
            "FileList worker must not use the generic bounded-detach cleanup"
        );
        assert!(finished.load(Ordering::Acquire));
        let bytes_at_return = fs::read(temp.path.join("FileList.txt")).expect("read marker");
        thread::sleep(Duration::from_millis(80));
        assert_eq!(
            fs::read(temp.path.join("FileList.txt")).expect("read marker after return"),
            bytes_at_return,
            "no FileList write may occur after the transaction worker has been joined"
        );
    }

    #[test]
    fn tc_166_filelist_missing_or_panicked_worker_never_resumes_success_intents() {
        let (result_tx, result_rx) = mpsc::channel::<FileListWorkerResult>();
        drop(result_tx);
        let (done_tx, done_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            drop(done_tx);
            panic!("injected FileList worker panic");
        });
        let worker = ActiveFileListWorker {
            cancel: Arc::new(AtomicBool::new(false)),
            result: result_rx,
            done: done_rx,
            handle: Some(handle),
        };
        while !worker.is_finished() {
            thread::yield_now();
        }
        assert!(matches!(
            worker.result.try_recv(),
            Err(mpsc::TryRecvError::Disconnected)
        ));
        worker.join();

        let (index_tx, _index_rx) = mpsc::channel();
        let freshness = TuiIndexFreshness::new();
        let actions = TuiActionFreshness::new();
        let mut state = TuiState::new("");
        state.pending_filelist_intent = Some(PendingFileListIntent::SelectOutput);
        assert!(settle_filelist(
            &mut state,
            FileListSettlement::Failed("FileList worker disconnected".to_string()),
            &index_tx,
            &freshness,
            &actions,
        )
        .is_none());
        assert!(state.status.contains("failed"));
    }
}

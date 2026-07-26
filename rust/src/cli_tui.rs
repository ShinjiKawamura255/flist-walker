use crate::actions::{
    execute_authorized_action_request, execute_or_open, AuthorizedActionBackend,
    AuthorizedActionGuard, AuthorizedActionMode, AuthorizedActionOutcome, AuthorizedActionReport,
    AuthorizedActionRequest,
};
use crate::indexer::{
    build_index_cancellable, find_filelist_in_first_level, is_index_build_cancelled,
    walk_entries_stream_cancellable,
};
use crate::path_utils::output_path_bytes;
use crate::persistence::{
    history_persistence_enabled, load_persisted_roots_and_history, AsyncHistoryPersistence,
};
use crate::query::{CompiledIgnoreTerms, CompiledQuery, QueryOptions, QueryScope};
use crate::search::try_search_entries_with_scope;
use crate::ui_model::{build_preview_text_with_kind, display_path_with_mode};
use anyhow::{Context, Result};
use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers,
};
use crossterm::execute;
use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor};
use crossterm::terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen};
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
const PREVIEW_MIN_WIDTH: u16 = 100;
const PREVIEW_MIN_HEIGHT: u16 = 8;

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
    pub ignore_terms: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CliTuiOutcome {
    Selected,
    Cancelled,
}

enum WorkerResponse {
    IndexedBatch(Vec<PathBuf>),
    IndexedFinished,
    IndexFailed(String),
    Searched {
        request_id: u64,
        query: String,
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

struct SearchRequest {
    request_id: u64,
    query: String,
    entries: Arc<Vec<PathBuf>>,
    root: PathBuf,
    limit: usize,
    regex: bool,
    ignore_case: bool,
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
    search_tx: &'a mpsc::Sender<SearchRequest>,
    preview_tx: &'a mpsc::Sender<PreviewRequest>,
    action_tx: &'a mpsc::Sender<TuiActionRequest>,
    rx: &'a mpsc::Receiver<WorkerResponse>,
    root: PathBuf,
    options: &'a CliTuiOptions,
    history_enabled: bool,
    history_entries: Vec<String>,
    history_persistence: Option<&'a AsyncHistoryPersistence>,
    action_freshness: Arc<TuiActionFreshness>,
    cancellation: Arc<AtomicBool>,
}

enum TuiExit {
    Cancelled,
    Selected { paths: Vec<PathBuf>, query: String },
}

enum KeyAction {
    Continue,
    Cancel,
    Select,
    HistoryApplied,
    HistoryOpened(Option<String>),
    DispatchAction(AuthorizedActionMode),
}

struct TuiState {
    query: String,
    query_cursor: usize,
    results: Vec<(PathBuf, f64)>,
    selected: usize,
    offset: usize,
    status: String,
    dirty: bool,
    last_query_change: Option<Instant>,
    indexed: bool,
    entries: Arc<Vec<PathBuf>>,
    root: PathBuf,
    pinned: Vec<PathBuf>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HelpContext {
    Normal,
    History,
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
            dirty: true,
            last_query_change: Some(Instant::now()),
            indexed: false,
            entries: Arc::new(Vec::new()),
            root: PathBuf::new(),
            pinned: Vec::new(),
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
            next_action_request_id: 0,
            active_action_request: None,
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
        self.status = error.unwrap_or_else(|| format!("{} result(s)", self.results.len()));
        self.dirty = true;
    }

    fn next_search_request(
        &mut self,
        root: PathBuf,
        limit: usize,
        regex: bool,
        ignore_case: bool,
    ) -> SearchRequest {
        self.next_search_request_id = self.next_search_request_id.wrapping_add(1);
        let request_id = self.next_search_request_id;
        self.active_search_request_id = Some(request_id);
        SearchRequest {
            request_id,
            query: self.query.clone(),
            entries: Arc::clone(&self.entries),
            root,
            limit,
            regex,
            ignore_case,
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
        self.last_query_change = Some(Instant::now());
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
        self.help = Some(if self.history.is_some() {
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

    let history_enabled = history_persistence_enabled();
    let history_entries = if history_enabled {
        load_persisted_roots_and_history().query_history
    } else {
        Vec::new()
    };
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
            while !search_cancelled.load(Ordering::Relaxed) {
                let mut request = match search_rx.recv_timeout(EVENT_POLL) {
                    Ok(request) => request,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                };
                while let Ok(newer) = search_rx.try_recv() {
                    request = newer;
                }
                let (results, error) = search(
                    &request.query,
                    &request.entries,
                    request.limit,
                    &request.root,
                    request.regex,
                    request.ignore_case,
                );
                if search_cancelled.load(Ordering::Relaxed)
                    || response_tx
                        .send(WorkerResponse::Searched {
                            request_id: request.request_id,
                            query: request.query,
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

    let (index_done_tx, index_done_rx) = mpsc::channel();
    let worker_cancelled = Arc::clone(&cancelled);
    let worker_tx = tx.clone();
    let worker_root = root.clone();
    let worker_options = options.clone();
    let index_handle = match thread::Builder::new()
        .name("flistwalker-cli-index-search".to_string())
        .spawn(move || {
            let compiled = CompiledIgnoreTerms::compile(
                &worker_options.ignore_terms,
                worker_options.ignore_case,
            );
            let send_batch = |paths: Vec<PathBuf>| {
                if worker_cancelled.load(Ordering::Relaxed) {
                    return;
                }
                let filtered = paths
                    .into_iter()
                    .filter(|path| {
                        !compiled.matches_path(
                            path,
                            QueryScope {
                                root: Some(&worker_root),
                                prefer_relative: true,
                                ignore_case: worker_options.ignore_case,
                            },
                        )
                    })
                    .collect::<Vec<_>>();
                if !filtered.is_empty() {
                    let _ = worker_tx.send(WorkerResponse::IndexedBatch(filtered));
                }
            };
            let has_filelist = find_filelist_in_first_level(&worker_root).is_some();
            if worker_options.use_filelist && has_filelist {
                match build_index_cancellable(
                    &worker_root,
                    true,
                    worker_options.include_files,
                    worker_options.include_dirs,
                    || worker_cancelled.load(Ordering::Relaxed),
                ) {
                    Ok(paths) => send_batch(paths),
                    Err(error) if is_index_build_cancelled(&error) => {}
                    Err(error) => {
                        let _ = worker_tx.send(WorkerResponse::IndexFailed(error.to_string()));
                    }
                }
            } else {
                let mut batch = Vec::with_capacity(256);
                let result = walk_entries_stream_cancellable(
                    &worker_root,
                    worker_options.include_files,
                    worker_options.include_dirs,
                    || worker_cancelled.load(Ordering::Relaxed),
                    |path| {
                        batch.push(path);
                        if batch.len() >= 256 {
                            send_batch(std::mem::take(&mut batch));
                        }
                    },
                );
                if result.is_ok() {
                    send_batch(batch);
                }
            }
            if !worker_cancelled.load(Ordering::Relaxed) {
                let _ = worker_tx.send(WorkerResponse::IndexedFinished);
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
                search_tx: &search_tx,
                preview_tx: &preview_tx,
                action_tx: &action_tx,
                rx: &rx,
                root: root.clone(),
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
        TuiExit::Selected { paths, .. } => {
            write_selected_paths(&paths, &root, options.absolute, options.print0)?;
            Ok(CliTuiOutcome::Selected)
        }
    }
}

fn finish_worker(handle: thread::JoinHandle<()>, done: mpsc::Receiver<()>) {
    if done.recv_timeout(WORKER_JOIN_TIMEOUT).is_ok() {
        let _ = handle.join();
    }
}

fn write_selected_paths(
    paths: &[PathBuf],
    root: &Path,
    absolute: bool,
    print0: bool,
) -> io::Result<()> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    for path in paths {
        output.write_all(&output_path_bytes(path, root, !absolute, print0))?;
        output.write_all(if print0 { b"\0" } else { b"\n" })?;
    }
    output.flush()
}

fn run_event_loop<W: Write>(
    terminal_output: &mut W,
    context: EventLoopContext<'_>,
) -> Result<TuiExit> {
    let EventLoopContext {
        search_tx,
        preview_tx,
        action_tx,
        rx,
        root,
        options,
        history_enabled,
        history_entries,
        history_persistence,
        action_freshness,
        cancellation,
    } = context;
    let mut state = TuiState::new(&options.initial_query);
    state.root = root.clone();
    state.history_enabled = history_enabled;
    state.history_entries = history_entries;
    loop {
        while let Ok(response) = rx.try_recv() {
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
            let _ = search_tx.send(state.next_search_request(
                root.clone(),
                options.limit,
                options.regex,
                options.ignore_case,
            ));
        }

        if state.dirty {
            draw(terminal_output, &mut state, options)?;
            state.dirty = false;
        }
        if event::poll(EVENT_POLL)? {
            match event::read()? {
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    let preview_path_before = state.current_path().cloned();
                    let preview_preferred_before = state.preview_preferred;
                    match handle_key(&mut state, key) {
                        KeyAction::Cancel => {
                            cancellation.store(true, Ordering::Release);
                            return Ok(TuiExit::Cancelled);
                        }
                        KeyAction::Select => {
                            cancellation.store(true, Ordering::Release);
                            return Ok(TuiExit::Selected {
                                paths: selected_paths(&state),
                                query: state.query.clone(),
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

fn apply_worker_response(state: &mut TuiState, response: WorkerResponse) -> Result<()> {
    match response {
        WorkerResponse::IndexedBatch(entries) => {
            Arc::make_mut(&mut state.entries).extend(entries);
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
        WorkerResponse::IndexedFinished => {
            state.indexed = true;
            state.status = "Ready".to_string();
            state.last_query_change = Some(
                Instant::now()
                    .checked_sub(INPUT_DEBOUNCE)
                    .unwrap_or_else(Instant::now),
            );
            state.dirty = true;
        }
        WorkerResponse::IndexFailed(error) => anyhow::bail!("indexing failed: {error}"),
        WorkerResponse::Searched {
            request_id,
            query,
            results,
            error,
        } => apply_search_response(state, request_id, &query, results, error),
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
    query: &str,
    results: Vec<(PathBuf, f64)>,
    error: Option<String>,
) {
    if state.active_search_request_id == Some(request_id) && query == state.query {
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

fn handle_key(state: &mut TuiState, key: KeyEvent) -> KeyAction {
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
        (KeyCode::Tab, _) => {
            if let Some((path, _)) = state.results.get(state.selected) {
                if let Some(index) = state.pinned.iter().position(|pinned| pinned == path) {
                    state.pinned.remove(index);
                } else {
                    state.pinned.push(path.clone());
                }
            }
        }
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
    if pasted.is_empty() || state.help.is_some() {
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
    query: &str,
    entries: &[PathBuf],
    limit: usize,
    root: &Path,
    regex: bool,
    ignore_case: bool,
) -> (Vec<(PathBuf, f64)>, Option<String>) {
    if query.trim().is_empty() {
        return (
            entries
                .iter()
                .take(limit)
                .cloned()
                .map(|path| (path, 0.0))
                .collect(),
            None,
        );
    }
    match try_search_entries_with_scope(query, entries, limit, regex, ignore_case, Some(root), true)
    {
        Ok(results) => (results, None),
        Err(error) => (Vec::new(), Some(error.to_string())),
    }
}

fn draw<W: Write>(
    terminal_output: &mut W,
    state: &mut TuiState,
    options: &CliTuiOptions,
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
            Print(clip_to_width(&state.status, list_width as usize)),
            ResetColor
        )?;
    }
    if height > 3 {
        execute!(
            terminal_output,
            MoveTo(0, 3),
            SetForegroundColor(Color::DarkGrey),
            Print(clip_to_width(
                "Enter select | Tab pin | Alt+P preview | Esc cancel",
                list_width as usize,
            )),
            ResetColor
        )?;
    }
    let compiled = (!state.query.trim().is_empty()).then(|| {
        CompiledQuery::compile(
            &state.query,
            QueryOptions {
                use_regex: options.regex,
                ignore_case: options.ignore_case,
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
        render_help_overlay(terminal_output, context, width, height)?;
    } else if let Some(history) = state.history.as_ref() {
        render_history_overlay(terminal_output, history, width, height)?;
    }
    terminal_output.flush()?;
    Ok(())
}

fn render_history_overlay<W: Write>(
    terminal_output: &mut W,
    history: &HistoryOverlay,
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
                "Enter apply | Esc/Ctrl+G cancel | Ctrl+C exit | arrows/Page move",
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

fn render_help_overlay<W: Write>(
    terminal_output: &mut W,
    context: HelpContext,
    width: u16,
    height: u16,
) -> Result<()> {
    let mut lines = vec![
        "Help".to_string(),
        "Enter / Esc / Ctrl+G close help | Ctrl+C exit".to_string(),
    ];
    match context {
        HelpContext::Normal => lines.extend([
            "Enter output selection | Tab pin | arrows/Page move".to_string(),
            "Ctrl+O open current | Shift+Enter reveal current".to_string(),
            "Ctrl+G clear query and pins | Ctrl+R search history".to_string(),
            "Alt+P toggle preview | F1 help".to_string(),
        ]),
        HelpContext::History => lines.extend([
            "History search is paused while help is open.".to_string(),
            "Close help to use Enter, Esc/Ctrl+G, edit, or navigation.".to_string(),
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
    use std::cell::RefCell;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::rc::Rc;

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
        state.active_search_request_id = Some(2);
        state.results = vec![(PathBuf::from("current.txt"), 1.0)];

        apply_search_response(
            &mut state,
            1,
            "new",
            vec![(PathBuf::from("stale.txt"), 2.0)],
            None,
        );
        assert_eq!(state.results[0].0, PathBuf::from("current.txt"));

        apply_search_response(
            &mut state,
            2,
            "new",
            vec![(PathBuf::from("latest.txt"), 3.0)],
            None,
        );
        assert_eq!(state.results[0].0, PathBuf::from("latest.txt"));
    }

    #[test]
    fn tc_162_index_failure_propagates_out_of_the_event_loop() {
        let mut state = TuiState::new("");

        let error = apply_worker_response(
            &mut state,
            WorkerResponse::IndexFailed("broken FileList".to_string()),
        )
        .expect_err("index failure must terminate the TUI");

        assert!(error.to_string().contains("broken FileList"));
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
        render_history_overlay(&mut history_output, &history, 40, 8).expect("render history");
        let mut help_output = Vec::new();
        render_help_overlay(&mut help_output, HelpContext::Normal, 40, 8).expect("render help");

        for output in [&history_output, &help_output] {
            assert!(
                output.windows(4).any(|window| window == b"\x1b[2J"),
                "overlay must clear terminal before rendering"
            );
        }
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
        render_history_overlay(&mut output, &history, 12, 6).expect("render history overlay");
        let rendered = String::from_utf8_lossy(&output);
        assert!(rendered.contains("History"));
        assert!(rendered.contains('�'));
        assert!(!rendered.contains("\u{1b}x"));
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
}

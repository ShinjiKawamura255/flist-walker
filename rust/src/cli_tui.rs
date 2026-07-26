use crate::indexer::{
    build_index_cancellable, find_filelist_in_first_level, is_index_build_cancelled,
    walk_entries_stream_cancellable,
};
use crate::path_utils::output_path_bytes;
use crate::query::{CompiledIgnoreTerms, CompiledQuery, QueryOptions, QueryScope};
use crate::search::try_search_entries_with_scope;
use crate::ui_model::display_path_with_mode;
use anyhow::{Context, Result};
use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers,
};
use crossterm::execute;
use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor};
use crossterm::terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen};
use std::collections::HashSet;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use unicode_width::UnicodeWidthChar;

const INPUT_DEBOUNCE: Duration = Duration::from_millis(35);
const INDEX_REFRESH_THROTTLE: Duration = Duration::from_millis(100);
const EVENT_POLL: Duration = Duration::from_millis(50);
const WORKER_JOIN_TIMEOUT: Duration = Duration::from_millis(250);

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

enum TuiExit {
    Cancelled,
    Selected(Vec<PathBuf>),
}

enum KeyAction {
    Continue,
    Cancel,
    Select,
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
            finish_worker(search_handle, search_done_rx);
            return Err(error).context("failed to start CLI index worker");
        }
    };

    let result = run_terminal_operation(guard, |terminal_output| {
        run_event_loop(terminal_output, &search_tx, &rx, root.clone(), options)
    });
    cancelled.store(true, Ordering::Relaxed);
    drop(search_tx);
    finish_worker(search_handle, search_done_rx);
    finish_worker(index_handle, index_done_rx);

    match result? {
        TuiExit::Cancelled => Ok(CliTuiOutcome::Cancelled),
        TuiExit::Selected(paths) => {
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
    search_tx: &mpsc::Sender<SearchRequest>,
    rx: &mpsc::Receiver<WorkerResponse>,
    root: PathBuf,
    options: &CliTuiOptions,
) -> Result<TuiExit> {
    let mut state = TuiState::new(&options.initial_query);
    state.root = root.clone();
    loop {
        while let Ok(response) = rx.try_recv() {
            apply_worker_response(&mut state, response)?;
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
                    match handle_key(&mut state, key) {
                        KeyAction::Cancel => return Ok(TuiExit::Cancelled),
                        KeyAction::Select => return Ok(TuiExit::Selected(selected_paths(&state))),
                        KeyAction::Continue => {}
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
    }
    Ok(())
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
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
            return KeyAction::Cancel;
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
    if pasted.is_empty() {
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
    let visible = height.saturating_sub(4) as usize;
    state.viewport_rows = visible.max(1);
    state.ensure_selection_visible();
    let start = state.offset.min(state.results.len());
    execute!(
        terminal_output,
        Clear(ClearType::All),
        MoveTo(0, 0),
        SetAttribute(Attribute::Bold),
        Print(clip_to_width("FlistWalker CLI", width as usize)),
        SetAttribute(Attribute::Reset),
    )?;
    if height > 1 {
        execute!(
            terminal_output,
            MoveTo(0, 1),
            Print(query_line_for_width(state, width as usize))
        )?;
    }
    if height > 2 {
        execute!(
            terminal_output,
            MoveTo(0, 2),
            SetForegroundColor(Color::DarkGrey),
            Print(clip_to_width(&state.status, width as usize)),
            ResetColor
        )?;
    }
    if height > 3 {
        execute!(
            terminal_output,
            MoveTo(0, 3),
            SetForegroundColor(Color::DarkGrey),
            Print(clip_to_width(
                "Enter select | Tab pin | Esc cancel | arrows/PageUp/PageDown move",
                width as usize,
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
            width,
        )?;
    }
    terminal_output.flush()?;
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

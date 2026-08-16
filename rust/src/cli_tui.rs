use crate::actions::{AuthorizedActionOutcome, AuthorizedActionReport};
use crate::indexer::find_filelist_in_first_level;
#[cfg(test)]
use crate::path_utils::output_path_bytes;
use crate::persistence::{
    history_persistence_enabled, load_persisted_roots_and_history, AsyncHistoryPersistence,
};
use crate::runtime_config::current_runtime_config;
use crate::search::SearchSortMode;
use crate::walker_runtime::walker_truncated_notice;
use anyhow::Result;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::style::Colored;
use crossterm::terminal;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

const INPUT_DEBOUNCE: Duration = Duration::from_millis(35);
const INDEX_REFRESH_THROTTLE: Duration = Duration::from_millis(100);
const MAX_WORKER_RESPONSES_PER_TICK: usize = 64;
const PREVIEW_MIN_WIDTH: u16 = 100;
const PREVIEW_MIN_HEIGHT: u16 = 8;

fn format_tui_update_notice(target_version: &str) -> String {
    format!("Update available: v{target_version} — Run flistwalker --update after exiting")
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
    pub color_enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CliTuiOutcome {
    Selected { paths: Vec<PathBuf>, root: PathBuf },
    Cancelled,
}

mod filelist;
mod input;
mod protocol;
mod render;
mod state;
#[path = "cli_tui/terminal.rs"]
mod terminal_io;
mod workers;

use filelist::*;
use input::*;
use protocol::*;
use render::*;
use state::*;
use terminal_io::*;
use workers::*;

// Regression guard: every TUI-owned user-facing path must cross this boundary so
// Windows extended prefixes never leak. Do not replace it with Path::display or
// to_string_lossy without updating the paired tc_177_regression_tui tests.
fn tui_path_label(path: &Path) -> String {
    crate::path_utils::normalize_path_for_display(path)
}

fn missing_required_filelist_message(root: &Path) -> String {
    format!(
        "FileList was required but none was found in {}",
        tui_path_label(root)
    )
}

// `--color always` must override NO_COLOR for the interactive renderer too.
// crossterm memoizes that environment setting separately from our CLI mode.
fn force_tui_color_output(color_enabled: bool) {
    if color_enabled {
        Colored::set_ansi_color_disabled(false);
    }
}

pub fn run_cli_tui(root: &Path, options: &CliTuiOptions) -> Result<CliTuiOutcome> {
    if !interactive_terminal_supported(io::stdin().is_terminal(), io::stderr().is_terminal()) {
        anyhow::bail!("--interactive requires terminal stdin and stderr");
    }
    if options.require_filelist && find_filelist_in_first_level(root).is_none() {
        anyhow::bail!(missing_required_filelist_message(root));
    }
    force_tui_color_output(options.color_enabled);

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
    let workers = TuiWorkerSet::start()?;
    let result = run_terminal_operation(guard, |terminal_output| {
        run_event_loop(
            terminal_output,
            EventLoopContext {
                index_tx: workers.index_tx(),
                index_freshness: workers.index_freshness(),
                search_tx: workers.search_tx(),
                preview_tx: workers.preview_tx(),
                action_tx: workers.action_tx(),
                rx: workers.response_rx(),
                root: root.clone(),
                saved_roots,
                options,
                history_enabled,
                history_entries,
                history_persistence: history_persistence.as_ref(),
                action_freshness: workers.action_freshness(),
                cancellation: workers.cancellation(),
            },
        )
    });
    workers.shutdown();

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
    if state
        .dispatch_current_index(index_tx, index_freshness.as_ref())
        .is_err()
    {
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
                            if state
                                .dispatch_current_index(index_tx, index_freshness.as_ref())
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
                            state.prepare_refresh();
                            if state
                                .dispatch_current_index(index_tx, index_freshness.as_ref())
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
                            state.prepare_root_switch(action_freshness.as_ref(), new_root);
                            if state
                                .dispatch_current_index(index_tx, index_freshness.as_ref())
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

fn take_ready_responses<T>(rx: &mpsc::Receiver<T>, limit: usize) -> Vec<T> {
    rx.try_iter().take(limit).collect()
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
        state.finish_search_request(request_id);
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

#[cfg(test)]
mod tests;

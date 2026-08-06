use super::protocol::{
    CandidateBatches, FileListWorkerResult, IndexRequest, PreviewRequest, SearchRequest,
    TuiActionFreshness, TuiActionRequest, TuiFileListRequest, TuiIndexFreshness, TuiRuntimeOptions,
    TuiSource,
};
use super::tui_path_label;
use crate::actions::{AuthorizedActionMode, AuthorizedActionRequest};
use crate::search::SearchSortMode;
use crate::walker_runtime::walker_truncated_notice;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Instant;

pub(super) struct TuiState {
    pub(super) query: String,
    pub(super) query_cursor: usize,
    pub(super) results: Vec<(PathBuf, f64)>,
    pub(super) selected: usize,
    pub(super) offset: usize,
    pub(super) status: String,
    pub(super) update_notice: Option<String>,
    pub(super) dirty: bool,
    pub(super) last_query_change: Option<Instant>,
    pub(super) indexed: bool,
    pub(super) root_filelist_known: bool,
    pub(super) root_filelist_exists: bool,
    pub(super) entries: CandidateBatches,
    pub(super) root: PathBuf,
    pub(super) saved_roots: Vec<PathBuf>,
    pub(super) root_picker: Option<RootPicker>,
    pub(super) runtime_options: TuiRuntimeOptions,
    pub(super) ignore_terms: Arc<Vec<String>>,
    pub(super) sort_mode: SearchSortMode,
    pub(super) source_changed_on_apply: bool,
    pub(super) next_index_request_id: u64,
    pub(super) active_index_request: Option<(u64, PathBuf)>,
    pub(super) index_truncated_limit: Option<usize>,
    pub(super) pinned: Vec<PathBuf>,
    pub(super) emacs_keybindings_enabled: bool,
    pub(super) tab_pin_moves_to_next_row: bool,
    pub(super) kill_buffer: String,
    pub(super) viewport_rows: usize,
    pub(super) next_search_request_id: u64,
    pub(super) active_search_request_id: Option<u64>,
    pub(super) last_incremental_search: Option<Instant>,
    pub(super) preview_preferred: bool,
    pub(super) preview_visible: bool,
    pub(super) preview: String,
    pub(super) next_preview_request_id: u64,
    pub(super) active_preview_request: Option<PreviewRequestIdentity>,
    pub(super) history_enabled: bool,
    pub(super) history_entries: Vec<String>,
    pub(super) history: Option<HistoryOverlay>,
    pub(super) help: Option<HelpContext>,
    pub(super) options_overlay: Option<OptionsOverlay>,
    pub(super) sort_picker: Option<SortPicker>,
    pub(super) filelist_confirmation: Option<FileListConfirmation>,
    pub(super) next_filelist_request_id: u64,
    pub(super) active_filelist: Option<ActiveFileList>,
    pub(super) pending_filelist_intent: Option<PendingFileListIntent>,
    pub(super) next_action_request_id: u64,
    pub(super) active_action_request: Option<(u64, PathBuf)>,
}

pub(super) fn history_search_score(
    query: &str,
    candidate: &str,
    recency_rank: usize,
) -> Option<i64> {
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

pub(super) fn refresh_history_results(history: &mut HistoryOverlay, entries: &[String]) {
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

#[derive(Clone, Debug)]
pub(super) struct HistoryOverlay {
    pub(super) draft_query: String,
    pub(super) filter: String,
    pub(super) filter_cursor: usize,
    pub(super) results: Vec<String>,
    pub(super) selected: usize,
    pub(super) offset: usize,
}

#[derive(Clone, Debug)]
pub(super) struct OptionsOverlay {
    pub(super) draft: TuiRuntimeOptions,
    pub(super) selected: usize,
}

#[derive(Clone, Debug)]
pub(super) struct SortPicker {
    pub(super) selected: usize,
}

#[derive(Clone, Debug)]
pub(super) struct RootPicker {
    pub(super) selected: usize,
}

#[derive(Clone, Debug)]
pub(super) enum FileListConfirmation {
    Mode { propagate_to_ancestors: bool },
    Overwrite { propagate_to_ancestors: bool },
}

#[derive(Clone, Debug)]
pub(super) struct ActiveFileList {
    pub(super) request_id: u64,
    pub(super) root: PathBuf,
    pub(super) cancel: Arc<AtomicBool>,
}

pub(super) struct ActiveFileListWorker {
    pub(super) cancel: Arc<AtomicBool>,
    pub(super) result: mpsc::Receiver<FileListWorkerResult>,
    pub(super) done: mpsc::Receiver<()>,
    pub(super) handle: Option<thread::JoinHandle<()>>,
}

impl ActiveFileListWorker {
    pub(super) fn join(mut self) {
        let _ = self.done.recv();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }

    pub(super) fn is_finished(&self) -> bool {
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
pub(super) enum PendingFileListIntent {
    SelectOutput,
    SwitchRoot(PathBuf),
    CancelExit,
}

pub(super) const SORT_MODES: [SearchSortMode; 9] = [
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
pub(super) enum HelpContext {
    Normal,
    History,
    FileList,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PreviewRequestIdentity {
    pub(super) request_id: u64,
    pub(super) root: PathBuf,
    pub(super) path: PathBuf,
}

impl TuiState {
    pub(super) fn dispatch_current_index(
        &mut self,
        index_tx: &mpsc::Sender<IndexRequest>,
        freshness: &TuiIndexFreshness,
    ) -> Result<(), mpsc::SendError<IndexRequest>> {
        let request = self.next_index_request(self.root.clone());
        freshness.activate(request.request_id);
        index_tx.send(request)
    }

    pub(super) fn prepare_root_switch(
        &mut self,
        action_freshness: &TuiActionFreshness,
        root: PathBuf,
    ) {
        self.root = root.clone();
        self.pinned.clear();
        self.clear_preview();
        self.active_search_request_id = None;
        self.sort_mode = SearchSortMode::Score;
        self.active_action_request = None;
        action_freshness.activate(0, &root);
        self.status = format!("Switching root to {}...", tui_path_label(&root));
        self.dirty = true;
    }

    pub(super) fn prepare_refresh(&mut self) {
        self.sort_mode = SearchSortMode::Score;
        self.active_search_request_id = None;
        self.status = format!("Refreshing {}...", tui_path_label(&self.root));
        self.dirty = true;
    }

    pub(super) fn new(query: &str) -> Self {
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

    pub(super) fn status_line(&self) -> String {
        match &self.update_notice {
            Some(notice) => format!("{notice} | {}", self.status),
            None => self.status.clone(),
        }
    }

    pub(super) fn set_results(&mut self, results: Vec<(PathBuf, f64)>, error: Option<String>) {
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

    pub(super) fn next_search_request(&mut self, root: PathBuf, limit: usize) -> SearchRequest {
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

    pub(super) fn next_index_request(&mut self, root: PathBuf) -> IndexRequest {
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

    pub(super) fn ensure_selection_visible(&mut self) {
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

    pub(super) fn move_selection(&mut self, delta: isize) {
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

    pub(super) fn mark_query_changed(&mut self) {
        self.sort_mode = SearchSortMode::Score;
        self.last_query_change = Some(Instant::now());
    }

    pub(super) fn current_options_summary(&self) -> String {
        format!(
            "Root: {} | Sort: {} | Source: {} | Files: {} | Folders: {} | Regex: {} | Ignore Case: {} | Ignore: {}",
            tui_path_label(&self.root),
            self.sort_mode.label(),
            self.runtime_options.source.label(),
            if self.runtime_options.include_files { "on" } else { "off" },
            if self.runtime_options.include_dirs { "on" } else { "off" },
            if self.runtime_options.regex { "on" } else { "off" },
            if self.runtime_options.ignore_case { "on" } else { "off" },
            if self.runtime_options.ignore_enabled { "on" } else { "off" },
        )
    }

    pub(super) fn open_options(&mut self) {
        self.options_overlay = Some(OptionsOverlay {
            draft: self.runtime_options,
            selected: 0,
        });
    }

    pub(super) fn open_sort_picker(&mut self) {
        self.sort_picker = Some(SortPicker {
            selected: SORT_MODES
                .iter()
                .position(|mode| *mode == self.sort_mode)
                .unwrap_or(0),
        });
    }

    pub(super) fn open_root_picker(&mut self) {
        self.root_picker = Some(RootPicker { selected: 0 });
    }

    pub(super) fn open_filelist_confirmation(&mut self) {
        self.filelist_confirmation = Some(FileListConfirmation::Mode {
            propagate_to_ancestors: false,
        });
        self.dirty = true;
    }

    pub(super) fn open_filelist_if_ready(&mut self) {
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

    pub(super) fn next_filelist_request(
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

    pub(super) fn cancel_active_filelist(&mut self) {
        if let Some(active) = self.active_filelist.as_ref() {
            active.cancel.store(true, Ordering::Release);
            self.status = "Canceling FileList creation...".to_string();
            self.dirty = true;
        }
    }

    pub(super) fn record_filelist_intent(&mut self, intent: PendingFileListIntent) {
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

    pub(super) fn current_path(&self) -> Option<&PathBuf> {
        self.results.get(self.selected).map(|(path, _)| path)
    }

    pub(super) fn clear_preview(&mut self) {
        self.preview.clear();
        self.active_preview_request = None;
    }

    pub(super) fn next_preview_request(&mut self) -> Option<PreviewRequest> {
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

    pub(super) fn begin_history(&mut self) {
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

    pub(super) fn commit_query_to_history(&mut self) -> Option<String> {
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

    pub(super) fn cancel_history(&mut self) {
        if let Some(history) = self.history.take() {
            self.query = history.draft_query;
            self.query_cursor = self.query.chars().count();
        }
    }

    pub(super) fn accept_history(&mut self) -> Option<String> {
        let history = self.history.take()?;
        let selected = history.results.get(history.selected)?.clone();
        self.query = selected.clone();
        self.query_cursor = self.query.chars().count();
        self.mark_query_changed();
        Some(selected)
    }

    pub(super) fn open_help(&mut self) {
        self.help = Some(if self.active_filelist.is_some() {
            HelpContext::FileList
        } else if self.history.is_some() {
            HelpContext::History
        } else {
            HelpContext::Normal
        });
    }

    pub(super) fn close_help(&mut self) {
        self.help = None;
    }

    pub(super) fn next_action_request(
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

use super::{
    normalize_windows_path_buf, EntryKindCacheState, FlistWalkerApp, PendingActiveIndexFinish,
    ResultSortMode, ResultSortScope, SavedTabState, TabAccentColor,
};
use crate::app::worker_protocol::IndexEntry;
use crate::entry::{Entry, EntryKind};
use crate::indexer::{IndexBuildResult, IndexSource};
use std::collections::{HashSet, VecDeque};
use std::mem;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone, Debug)]
pub(super) struct TabQueryState {
    pub(super) query: String,
    pub(super) query_history: VecDeque<String>,
    pub(super) query_history_cursor: Option<usize>,
    pub(super) query_history_draft: Option<String>,
    pub(super) history_search_active: bool,
    pub(super) history_search_query: String,
    pub(super) history_search_original_query: String,
    pub(super) history_search_results: Vec<String>,
    pub(super) history_search_current: Option<usize>,
}

#[derive(Clone, Debug)]
pub(super) struct TabIndexState {
    pub(super) index: IndexBuildResult,
    pub(super) all_entries: Arc<Vec<Entry>>,
    pub(super) entries: Arc<Vec<Entry>>,
    pub(super) pending_index_request_id: Option<u64>,
    pub(super) index_in_progress: bool,
    pub(super) pending_index_entries: VecDeque<IndexEntry>,
    pub(super) pending_index_entries_request_id: Option<u64>,
    pub(super) pending_index_finish: Option<PendingActiveIndexFinish>,
    pub(super) pending_kind_paths: VecDeque<PathBuf>,
    pub(super) pending_kind_paths_set: HashSet<PathBuf>,
    pub(super) in_flight_kind_paths: HashSet<PathBuf>,
    pub(super) resolved_kind_updates: Vec<(PathBuf, EntryKind)>,
    pub(super) kind_resolution_epoch: u64,
    pub(super) kind_resolution_in_progress: bool,
    pub(super) incremental_filtered_entries: Vec<Entry>,
    pub(super) last_incremental_results_refresh: Instant,
    pub(super) last_search_snapshot_len: usize,
    pub(super) search_resume_pending: bool,
    pub(super) search_rerun_pending: bool,
}

#[derive(Clone, Debug)]
pub(super) struct TabResultState {
    pub(super) base_results: Vec<(PathBuf, f64)>,
    pub(super) results: Vec<(PathBuf, f64)>,
    pub(super) result_sort_mode: ResultSortMode,
    pub(super) result_sort_scope: ResultSortScope,
    pub(super) total_match_count: usize,
    pub(super) pending_sort_request_id: Option<u64>,
    pub(super) sort_in_progress: bool,
    pub(super) pinned_paths: HashSet<PathBuf>,
    pub(super) current_row: Option<usize>,
    pub(super) preview: String,
    pub(super) results_compacted: bool,
}

#[derive(Debug)]
#[cfg_attr(test, derive(Clone))]
pub(crate) struct AppTabState {
    pub(super) id: u64,
    pub(super) root: PathBuf,
    pub(super) tab_accent: Option<TabAccentColor>,
    pub(super) use_filelist: bool,
    pub(super) use_regex: bool,
    pub(super) ignore_case: bool,
    pub(super) include_files: bool,
    pub(super) include_dirs: bool,
    pub(super) index_state: TabIndexState,
    pub(super) query_state: TabQueryState,
    pub(super) result_state: TabResultState,
    pub(super) entry_kind_cache: EntryKindCacheState,
    pub(super) notice: String,
    pub(super) pending_request_id: Option<u64>,
    pub(super) pending_preview_request_id: Option<u64>,
    pub(super) pending_action_request_id: Option<u64>,
    pub(super) search_in_progress: bool,
    pub(super) preview_in_progress: bool,
    pub(super) action_in_progress: bool,
}

impl TabIndexState {
    pub(super) fn begin_index_request(&mut self, request_id: u64) {
        self.pending_index_request_id = Some(request_id);
        self.index_in_progress = true;
    }

    pub(super) fn clear_index_request_state(&mut self) {
        self.pending_index_request_id = None;
        self.index_in_progress = false;
        self.pending_index_entries.clear();
        self.pending_index_entries_request_id = None;
        self.pending_index_finish = None;
        self.search_resume_pending = false;
        self.search_rerun_pending = false;
    }

    pub(super) fn clear_kind_resolution_state(&mut self) {
        self.pending_kind_paths.clear();
        self.pending_kind_paths_set.clear();
        self.in_flight_kind_paths.clear();
        self.resolved_kind_updates.clear();
        self.kind_resolution_in_progress = false;
    }

    pub(super) fn refresh_kind_resolution_progress(&mut self) {
        self.kind_resolution_in_progress =
            !self.pending_kind_paths.is_empty() || !self.in_flight_kind_paths.is_empty();
    }

    #[cfg(test)]
    pub(super) fn from_shell(shell: &FlistWalkerApp) -> Self {
        Self {
            index: shell.shell.runtime.index.clone(),
            all_entries: Arc::clone(&shell.shell.runtime.all_entries),
            entries: Arc::clone(&shell.shell.runtime.entries),
            pending_index_request_id: shell.shell.indexing.pending_request_id,
            index_in_progress: shell.shell.indexing.in_progress,
            pending_index_entries: shell.shell.indexing.pending_entries.clone(),
            pending_index_entries_request_id: shell.shell.indexing.pending_entries_request_id,
            pending_index_finish: shell.shell.indexing.pending_finish.clone(),
            pending_kind_paths: shell.shell.indexing.pending_kind_paths.clone(),
            pending_kind_paths_set: shell.shell.indexing.pending_kind_paths_set.clone(),
            in_flight_kind_paths: shell.shell.indexing.in_flight_kind_paths.clone(),
            resolved_kind_updates: shell.shell.indexing.resolved_kind_updates.clone(),
            kind_resolution_epoch: shell.shell.indexing.kind_resolution_epoch,
            kind_resolution_in_progress: shell.shell.indexing.kind_resolution_in_progress,
            incremental_filtered_entries: shell.shell.indexing.incremental_filtered_entries.clone(),
            last_incremental_results_refresh: shell.shell.indexing.last_incremental_results_refresh,
            last_search_snapshot_len: shell.shell.indexing.last_search_snapshot_len,
            search_resume_pending: shell.shell.indexing.search_resume_pending,
            search_rerun_pending: shell.shell.indexing.search_rerun_pending,
        }
    }

    #[cfg(test)]
    pub(super) fn apply_shell(&self, shell: &mut FlistWalkerApp) {
        shell.shell.runtime.index = self.index.clone();
        shell.shell.runtime.all_entries = Arc::clone(&self.all_entries);
        shell.shell.runtime.entries = Arc::clone(&self.entries);
        shell.shell.indexing.pending_request_id = self.pending_index_request_id;
        shell.shell.indexing.in_progress = self.index_in_progress;
        shell.shell.indexing.pending_entries = self.pending_index_entries.clone();
        shell.shell.indexing.pending_entries_request_id = self.pending_index_entries_request_id;
        shell.shell.indexing.pending_finish = self.pending_index_finish.clone();
        shell.shell.indexing.pending_kind_paths = self.pending_kind_paths.clone();
        shell.shell.indexing.pending_kind_paths_set = self.pending_kind_paths_set.clone();
        shell.shell.indexing.in_flight_kind_paths = self.in_flight_kind_paths.clone();
        shell.shell.indexing.resolved_kind_updates = self.resolved_kind_updates.clone();
        shell.shell.indexing.kind_resolution_epoch = self.kind_resolution_epoch;
        shell.shell.indexing.kind_resolution_in_progress = self.kind_resolution_in_progress;
        shell.shell.indexing.incremental_filtered_entries =
            self.incremental_filtered_entries.clone();
        shell.shell.indexing.last_incremental_results_refresh =
            self.last_incremental_results_refresh;
        shell.shell.indexing.last_search_snapshot_len = self.last_search_snapshot_len;
        shell.shell.indexing.search_resume_pending = self.search_resume_pending;
        shell.shell.indexing.search_rerun_pending = self.search_rerun_pending;
    }

    pub(super) fn swap_shell(&mut self, shell: &mut FlistWalkerApp) {
        mem::swap(&mut self.index, &mut shell.shell.runtime.index);
        mem::swap(&mut self.all_entries, &mut shell.shell.runtime.all_entries);
        mem::swap(&mut self.entries, &mut shell.shell.runtime.entries);
        mem::swap(
            &mut self.pending_index_request_id,
            &mut shell.shell.indexing.pending_request_id,
        );
        mem::swap(
            &mut self.index_in_progress,
            &mut shell.shell.indexing.in_progress,
        );
        mem::swap(
            &mut self.pending_index_entries,
            &mut shell.shell.indexing.pending_entries,
        );
        mem::swap(
            &mut self.pending_index_entries_request_id,
            &mut shell.shell.indexing.pending_entries_request_id,
        );
        mem::swap(
            &mut self.pending_index_finish,
            &mut shell.shell.indexing.pending_finish,
        );
        mem::swap(
            &mut self.pending_kind_paths,
            &mut shell.shell.indexing.pending_kind_paths,
        );
        mem::swap(
            &mut self.pending_kind_paths_set,
            &mut shell.shell.indexing.pending_kind_paths_set,
        );
        mem::swap(
            &mut self.in_flight_kind_paths,
            &mut shell.shell.indexing.in_flight_kind_paths,
        );
        mem::swap(
            &mut self.resolved_kind_updates,
            &mut shell.shell.indexing.resolved_kind_updates,
        );
        mem::swap(
            &mut self.kind_resolution_epoch,
            &mut shell.shell.indexing.kind_resolution_epoch,
        );
        mem::swap(
            &mut self.kind_resolution_in_progress,
            &mut shell.shell.indexing.kind_resolution_in_progress,
        );
        mem::swap(
            &mut self.incremental_filtered_entries,
            &mut shell.shell.indexing.incremental_filtered_entries,
        );
        mem::swap(
            &mut self.last_incremental_results_refresh,
            &mut shell.shell.indexing.last_incremental_results_refresh,
        );
        mem::swap(
            &mut self.last_search_snapshot_len,
            &mut shell.shell.indexing.last_search_snapshot_len,
        );
        mem::swap(
            &mut self.search_resume_pending,
            &mut shell.shell.indexing.search_resume_pending,
        );
        mem::swap(
            &mut self.search_rerun_pending,
            &mut shell.shell.indexing.search_rerun_pending,
        );
    }
}

impl TabQueryState {
    #[cfg(test)]
    pub(super) fn from_shell(shell: &FlistWalkerApp) -> Self {
        Self {
            query: shell.shell.runtime.query_state.query.clone(),
            query_history: shell.shell.runtime.query_state.query_history.clone(),
            query_history_cursor: shell.shell.runtime.query_state.query_history_cursor,
            query_history_draft: shell.shell.runtime.query_state.query_history_draft.clone(),
            history_search_active: shell.shell.runtime.query_state.history_search_active,
            history_search_query: shell.shell.runtime.query_state.history_search_query.clone(),
            history_search_original_query: shell
                .shell
                .runtime
                .query_state
                .history_search_original_query
                .clone(),
            history_search_results: shell
                .shell
                .runtime
                .query_state
                .history_search_results
                .clone(),
            history_search_current: shell.shell.runtime.query_state.history_search_current,
        }
    }

    #[cfg(test)]
    pub(super) fn apply_shell(&self, shell: &mut FlistWalkerApp) {
        shell.shell.runtime.query_state.query = self.query.clone();
        shell.shell.runtime.query_state.query_history = self.query_history.clone();
        shell.shell.runtime.query_state.query_history_cursor = self.query_history_cursor;
        shell.shell.runtime.query_state.query_history_draft = self.query_history_draft.clone();
        shell.shell.runtime.query_state.history_search_active = self.history_search_active;
        shell.shell.runtime.query_state.history_search_query = self.history_search_query.clone();
        shell
            .shell
            .runtime
            .query_state
            .history_search_original_query = self.history_search_original_query.clone();
        shell.shell.runtime.query_state.history_search_results =
            self.history_search_results.clone();
        shell.shell.runtime.query_state.history_search_current = self.history_search_current;
    }

    pub(super) fn swap_shell(&mut self, shell: &mut FlistWalkerApp) {
        let query_state = &mut shell.shell.runtime.query_state;
        mem::swap(&mut self.query, &mut query_state.query);
        mem::swap(&mut self.query_history, &mut query_state.query_history);
        mem::swap(
            &mut self.query_history_cursor,
            &mut query_state.query_history_cursor,
        );
        mem::swap(
            &mut self.query_history_draft,
            &mut query_state.query_history_draft,
        );
        mem::swap(
            &mut self.history_search_active,
            &mut query_state.history_search_active,
        );
        mem::swap(
            &mut self.history_search_query,
            &mut query_state.history_search_query,
        );
        mem::swap(
            &mut self.history_search_original_query,
            &mut query_state.history_search_original_query,
        );
        mem::swap(
            &mut self.history_search_results,
            &mut query_state.history_search_results,
        );
        mem::swap(
            &mut self.history_search_current,
            &mut query_state.history_search_current,
        );
    }
}

impl TabResultState {
    pub(super) fn clear_sort_request_state(&mut self) {
        self.pending_sort_request_id = None;
        self.sort_in_progress = false;
    }

    #[cfg(test)]
    pub(super) fn from_shell(shell: &FlistWalkerApp) -> Self {
        Self {
            base_results: shell.shell.runtime.base_results.clone(),
            results: shell.shell.runtime.results.clone(),
            result_sort_mode: shell.shell.runtime.result_sort_mode,
            result_sort_scope: shell.shell.runtime.result_sort_scope,
            total_match_count: shell.shell.runtime.total_match_count,
            pending_sort_request_id: shell.shell.worker_bus.sort.pending_request_id,
            sort_in_progress: shell.shell.worker_bus.sort.in_progress,
            pinned_paths: shell.shell.runtime.pinned_paths.clone(),
            current_row: shell.shell.runtime.current_row,
            preview: shell.shell.runtime.preview.clone(),
            results_compacted: false,
        }
    }

    #[cfg(test)]
    pub(super) fn apply_shell(&self, shell: &mut FlistWalkerApp) {
        shell.shell.runtime.base_results = self.base_results.clone();
        shell.shell.runtime.results = self.results.clone();
        shell.shell.runtime.result_sort_mode = self.result_sort_mode;
        shell.shell.runtime.result_sort_scope = self.result_sort_scope;
        shell.shell.runtime.total_match_count = self.total_match_count;
        shell.shell.worker_bus.sort.pending_request_id = self.pending_sort_request_id;
        shell.shell.worker_bus.sort.in_progress = self.sort_in_progress;
        shell.shell.runtime.pinned_paths = self.pinned_paths.clone();
        shell.shell.runtime.current_row = self.current_row;
        shell.shell.runtime.preview = self.preview.clone();
    }

    pub(super) fn swap_shell(&mut self, shell: &mut FlistWalkerApp) {
        mem::swap(
            &mut self.base_results,
            &mut shell.shell.runtime.base_results,
        );
        mem::swap(&mut self.results, &mut shell.shell.runtime.results);
        mem::swap(
            &mut self.result_sort_mode,
            &mut shell.shell.runtime.result_sort_mode,
        );
        mem::swap(
            &mut self.result_sort_scope,
            &mut shell.shell.runtime.result_sort_scope,
        );
        mem::swap(
            &mut self.total_match_count,
            &mut shell.shell.runtime.total_match_count,
        );
        mem::swap(
            &mut self.pending_sort_request_id,
            &mut shell.shell.worker_bus.sort.pending_request_id,
        );
        mem::swap(
            &mut self.sort_in_progress,
            &mut shell.shell.worker_bus.sort.in_progress,
        );
        mem::swap(
            &mut self.pinned_paths,
            &mut shell.shell.runtime.pinned_paths,
        );
        mem::swap(&mut self.current_row, &mut shell.shell.runtime.current_row);
        mem::swap(&mut self.preview, &mut shell.shell.runtime.preview);
    }
}

impl AppTabState {
    pub(super) fn begin_search_request(&mut self, request_id: u64) {
        self.pending_request_id = Some(request_id);
        self.search_in_progress = true;
    }

    pub(super) fn clear_search_request_state(&mut self) {
        self.pending_request_id = None;
        self.search_in_progress = false;
    }

    pub(super) fn clear_preview_request_state(&mut self) {
        self.pending_preview_request_id = None;
        self.preview_in_progress = false;
    }

    pub(super) fn clear_action_request_state(&mut self) {
        self.pending_action_request_id = None;
        self.action_in_progress = false;
    }

    #[cfg(test)]
    pub(super) fn from_shell(shell: &FlistWalkerApp, id: u64) -> Self {
        Self {
            id,
            root: shell.shell.runtime.root.clone(),
            tab_accent: shell
                .shell
                .tabs
                .get(shell.shell.tabs.active_tab_index())
                .and_then(|tab| tab.tab_accent),
            use_filelist: shell.shell.runtime.use_filelist,
            use_regex: shell.shell.runtime.use_regex,
            ignore_case: shell.shell.runtime.ignore_case,
            include_files: shell.shell.runtime.include_files,
            include_dirs: shell.shell.runtime.include_dirs,
            index_state: TabIndexState::from_shell(shell),
            query_state: TabQueryState::from_shell(shell),
            result_state: TabResultState::from_shell(shell),
            entry_kind_cache: shell.shell.cache.entry_kind.clone(),
            notice: shell.shell.runtime.notice.clone(),
            pending_request_id: shell.shell.search.pending_request_id(),
            pending_preview_request_id: shell.shell.worker_bus.preview.pending_request_id,
            pending_action_request_id: shell.shell.worker_bus.action.pending_request_id,
            search_in_progress: shell.shell.search.in_progress(),
            preview_in_progress: shell.shell.worker_bus.preview.in_progress,
            action_in_progress: shell.shell.worker_bus.action.in_progress,
        }
    }

    pub(super) fn from_saved(shell: &FlistWalkerApp, id: u64, saved: &SavedTabState) -> Self {
        Self {
            id,
            root: normalize_windows_path_buf(PathBuf::from(&saved.root)),
            tab_accent: saved.tab_accent,
            use_filelist: saved.use_filelist,
            use_regex: saved.use_regex,
            ignore_case: saved.ignore_case,
            include_files: saved.include_files,
            include_dirs: saved.include_dirs,
            index_state: TabIndexState {
                index: IndexBuildResult {
                    entries: Vec::new(),
                    source: IndexSource::None,
                },
                all_entries: Arc::new(Vec::new()),
                entries: Arc::new(Vec::new()),
                pending_index_request_id: None,
                index_in_progress: false,
                pending_index_entries: VecDeque::new(),
                pending_index_entries_request_id: None,
                pending_index_finish: None,
                pending_kind_paths: VecDeque::new(),
                pending_kind_paths_set: HashSet::new(),
                in_flight_kind_paths: HashSet::new(),
                resolved_kind_updates: Vec::new(),
                kind_resolution_epoch: 1,
                kind_resolution_in_progress: false,
                incremental_filtered_entries: Vec::new(),
                last_incremental_results_refresh: Instant::now(),
                last_search_snapshot_len: 0,
                search_resume_pending: false,
                search_rerun_pending: false,
            },
            query_state: TabQueryState {
                query: saved.query.clone(),
                query_history: shell.shell.runtime.query_state.query_history.clone(),
                query_history_cursor: None,
                query_history_draft: None,
                history_search_active: false,
                history_search_query: String::new(),
                history_search_original_query: String::new(),
                history_search_results: Vec::new(),
                history_search_current: None,
            },
            result_state: TabResultState {
                base_results: Vec::new(),
                results: Vec::new(),
                result_sort_mode: ResultSortMode::Score,
                result_sort_scope: ResultSortScope::ShownResults,
                total_match_count: 0,
                pending_sort_request_id: None,
                sort_in_progress: false,
                pinned_paths: HashSet::new(),
                current_row: Some(0),
                preview: String::new(),
                results_compacted: false,
            },
            entry_kind_cache: EntryKindCacheState::default(),
            notice: "Restored tab".to_string(),
            pending_request_id: None,
            pending_preview_request_id: None,
            pending_action_request_id: None,
            search_in_progress: false,
            preview_in_progress: false,
            action_in_progress: false,
        }
    }

    pub(super) fn scratch_from_shell(shell: &FlistWalkerApp, id: u64) -> Self {
        let saved = Self::saved_from_shell(shell, false);
        Self::from_saved(shell, id, &saved)
    }

    pub(super) fn new_tab_from_shell(shell: &FlistWalkerApp, id: u64) -> Self {
        let base_results = shell
            .shell
            .runtime
            .entries
            .iter()
            .take(shell.shell.runtime.limit)
            .cloned()
            .map(|entry| (entry.path, 0.0))
            .collect::<Vec<_>>();
        Self {
            id,
            root: shell.shell.runtime.root.clone(),
            tab_accent: None,
            use_filelist: true,
            use_regex: shell.shell.runtime.use_regex,
            ignore_case: shell.shell.runtime.ignore_case,
            include_files: shell.shell.runtime.include_files,
            include_dirs: shell.shell.runtime.include_dirs,
            index_state: TabIndexState {
                index: IndexBuildResult {
                    entries: Vec::new(),
                    source: shell.shell.runtime.index.source.clone(),
                },
                all_entries: Arc::clone(&shell.shell.runtime.all_entries),
                entries: Arc::clone(&shell.shell.runtime.entries),
                pending_index_request_id: None,
                index_in_progress: false,
                pending_index_entries: VecDeque::new(),
                pending_index_entries_request_id: None,
                pending_index_finish: None,
                pending_kind_paths: VecDeque::new(),
                pending_kind_paths_set: HashSet::new(),
                in_flight_kind_paths: HashSet::new(),
                resolved_kind_updates: Vec::new(),
                kind_resolution_epoch: 1,
                kind_resolution_in_progress: false,
                incremental_filtered_entries: Vec::new(),
                last_incremental_results_refresh: Instant::now(),
                last_search_snapshot_len: shell.shell.runtime.entries.len(),
                search_resume_pending: false,
                search_rerun_pending: false,
            },
            query_state: TabQueryState {
                query: String::new(),
                query_history: shell.shell.runtime.query_state.query_history.clone(),
                query_history_cursor: None,
                query_history_draft: None,
                history_search_active: false,
                history_search_query: String::new(),
                history_search_original_query: String::new(),
                history_search_results: Vec::new(),
                history_search_current: None,
            },
            result_state: TabResultState {
                results: base_results.clone(),
                base_results,
                result_sort_mode: ResultSortMode::Score,
                result_sort_scope: shell.shell.runtime.result_sort_scope,
                total_match_count: shell.shell.runtime.entries.len(),
                pending_sort_request_id: None,
                sort_in_progress: false,
                pinned_paths: HashSet::new(),
                current_row: None,
                preview: String::new(),
                results_compacted: false,
            },
            entry_kind_cache: EntryKindCacheState::default(),
            notice: "Opened new tab".to_string(),
            pending_request_id: None,
            pending_preview_request_id: None,
            pending_action_request_id: None,
            search_in_progress: false,
            preview_in_progress: false,
            action_in_progress: false,
        }
    }

    pub(super) fn sync_small_fields_from_shell(&mut self, shell: &FlistWalkerApp) {
        self.root.clone_from(&shell.shell.runtime.root);
        self.use_filelist = shell.shell.runtime.use_filelist;
        self.use_regex = shell.shell.runtime.use_regex;
        self.ignore_case = shell.shell.runtime.ignore_case;
        self.include_files = shell.shell.runtime.include_files;
        self.include_dirs = shell.shell.runtime.include_dirs;
    }

    pub(super) fn apply_small_fields_to_shell(&self, shell: &mut FlistWalkerApp) {
        shell.shell.runtime.root.clone_from(&self.root);
        shell.shell.runtime.use_filelist = self.use_filelist;
        shell.shell.runtime.use_regex = self.use_regex;
        shell.shell.runtime.ignore_case = self.ignore_case;
        shell.shell.runtime.include_files = self.include_files;
        shell.shell.runtime.include_dirs = self.include_dirs;
    }

    pub(super) fn swap_payload_with_shell(&mut self, shell: &mut FlistWalkerApp) {
        self.index_state.swap_shell(shell);
        self.query_state.swap_shell(shell);
        self.result_state.swap_shell(shell);
        mem::swap(
            &mut self.entry_kind_cache,
            &mut shell.shell.cache.entry_kind,
        );
        mem::swap(&mut self.notice, &mut shell.shell.runtime.notice);

        let shell_search_request_id = shell.shell.search.pending_request_id();
        shell
            .shell
            .search
            .set_pending_request_id(self.pending_request_id);
        self.pending_request_id = shell_search_request_id;
        mem::swap(
            &mut self.pending_preview_request_id,
            &mut shell.shell.worker_bus.preview.pending_request_id,
        );
        mem::swap(
            &mut self.pending_action_request_id,
            &mut shell.shell.worker_bus.action.pending_request_id,
        );

        let shell_search_in_progress = shell.shell.search.in_progress();
        shell.shell.search.set_in_progress(self.search_in_progress);
        self.search_in_progress = shell_search_in_progress;
        mem::swap(
            &mut self.preview_in_progress,
            &mut shell.shell.worker_bus.preview.in_progress,
        );
        mem::swap(
            &mut self.action_in_progress,
            &mut shell.shell.worker_bus.action.in_progress,
        );
    }

    #[cfg(test)]
    pub(super) fn apply_shell(&self, shell: &mut FlistWalkerApp) {
        shell.shell.runtime.root = self.root.clone();
        shell.shell.runtime.use_filelist = self.use_filelist;
        shell.shell.runtime.use_regex = self.use_regex;
        shell.shell.runtime.ignore_case = self.ignore_case;
        shell.shell.runtime.include_files = self.include_files;
        shell.shell.runtime.include_dirs = self.include_dirs;
        self.index_state.apply_shell(shell);
        self.query_state.apply_shell(shell);
        self.result_state.apply_shell(shell);
        shell.shell.cache.entry_kind = self.entry_kind_cache.clone();
        shell.shell.runtime.notice = self.notice.clone();
        shell
            .shell
            .search
            .set_pending_request_id(self.pending_request_id);
        shell.shell.worker_bus.preview.pending_request_id = self.pending_preview_request_id;
        shell.shell.worker_bus.action.pending_request_id = self.pending_action_request_id;
        shell.shell.search.set_in_progress(self.search_in_progress);
        shell.shell.worker_bus.preview.in_progress = self.preview_in_progress;
        shell.shell.worker_bus.action.in_progress = self.action_in_progress;
    }

    pub(super) fn to_saved(&self, history_persist_disabled: bool) -> SavedTabState {
        SavedTabState {
            root: self.root.to_string_lossy().to_string(),
            use_filelist: self.use_filelist,
            use_regex: self.use_regex,
            ignore_case: self.ignore_case,
            include_files: self.include_files,
            include_dirs: self.include_dirs,
            query: self.query_state.query.clone(),
            query_history: if history_persist_disabled {
                Vec::new()
            } else {
                self.query_state.query_history.iter().cloned().collect()
            },
            tab_accent: self.tab_accent,
        }
    }

    pub(super) fn saved_from_shell(
        shell: &FlistWalkerApp,
        history_persist_disabled: bool,
    ) -> SavedTabState {
        SavedTabState {
            root: shell.shell.runtime.root.to_string_lossy().to_string(),
            use_filelist: shell.shell.runtime.use_filelist,
            use_regex: shell.shell.runtime.use_regex,
            ignore_case: shell.shell.runtime.ignore_case,
            include_files: shell.shell.runtime.include_files,
            include_dirs: shell.shell.runtime.include_dirs,
            query: shell.shell.runtime.query_state.query.clone(),
            query_history: if history_persist_disabled {
                Vec::new()
            } else {
                shell
                    .shell
                    .runtime
                    .query_state
                    .query_history
                    .iter()
                    .cloned()
                    .collect()
            },
            tab_accent: shell
                .shell
                .tabs
                .get(shell.shell.tabs.active_tab_index())
                .and_then(|tab| tab.tab_accent),
        }
    }
}

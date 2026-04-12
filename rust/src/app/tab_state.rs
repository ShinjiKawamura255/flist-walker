use super::*;

#[derive(Clone, Debug)]
pub(super) struct TabQueryState {
    pub(super) query: String,
    pub(super) query_history: VecDeque<String>,
    pub(super) query_history_cursor: Option<usize>,
    pub(super) query_history_draft: Option<String>,
    pub(super) query_history_dirty_since: Option<Instant>,
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
    pub(super) pending_kind_paths: VecDeque<PathBuf>,
    pub(super) pending_kind_paths_set: HashSet<PathBuf>,
    pub(super) in_flight_kind_paths: HashSet<PathBuf>,
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
    pub(super) pending_sort_request_id: Option<u64>,
    pub(super) sort_in_progress: bool,
    pub(super) pinned_paths: HashSet<PathBuf>,
    pub(super) current_row: Option<usize>,
    pub(super) preview: String,
    pub(super) results_compacted: bool,
}

#[derive(Clone, Debug)]
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
    pub(super) pending_restore_refresh: bool,
    pub(super) result_state: TabResultState,
    pub(super) notice: String,
    pub(super) pending_request_id: Option<u64>,
    pub(super) pending_preview_request_id: Option<u64>,
    pub(super) pending_action_request_id: Option<u64>,
    pub(super) search_in_progress: bool,
    pub(super) preview_in_progress: bool,
    pub(super) action_in_progress: bool,
    pub(super) scroll_to_current: bool,
    pub(super) focus_query_requested: bool,
    pub(super) unfocus_query_requested: bool,
}

impl TabIndexState {
    pub(super) fn from_shell(shell: &FlistWalkerApp) -> Self {
        Self {
            index: shell.runtime.index.clone(),
            all_entries: Arc::clone(&shell.runtime.all_entries),
            entries: Arc::clone(&shell.runtime.entries),
            pending_index_request_id: shell.indexing.pending_request_id,
            index_in_progress: shell.indexing.in_progress,
            pending_index_entries: shell.indexing.pending_entries.clone(),
            pending_index_entries_request_id: shell.indexing.pending_entries_request_id,
            pending_kind_paths: shell.indexing.pending_kind_paths.clone(),
            pending_kind_paths_set: shell.indexing.pending_kind_paths_set.clone(),
            in_flight_kind_paths: shell.indexing.in_flight_kind_paths.clone(),
            kind_resolution_epoch: shell.indexing.kind_resolution_epoch,
            kind_resolution_in_progress: shell.indexing.kind_resolution_in_progress,
            incremental_filtered_entries: shell.indexing.incremental_filtered_entries.clone(),
            last_incremental_results_refresh: shell.indexing.last_incremental_results_refresh,
            last_search_snapshot_len: shell.indexing.last_search_snapshot_len,
            search_resume_pending: shell.indexing.search_resume_pending,
            search_rerun_pending: shell.indexing.search_rerun_pending,
        }
    }

    pub(super) fn apply_shell(&self, shell: &mut FlistWalkerApp) {
        shell.runtime.index = self.index.clone();
        shell.runtime.all_entries = Arc::clone(&self.all_entries);
        shell.runtime.entries = Arc::clone(&self.entries);
        shell.indexing.pending_request_id = self.pending_index_request_id;
        shell.indexing.in_progress = self.index_in_progress;
        shell.indexing.pending_entries = self.pending_index_entries.clone();
        shell.indexing.pending_entries_request_id = self.pending_index_entries_request_id;
        shell.indexing.pending_kind_paths = self.pending_kind_paths.clone();
        shell.indexing.pending_kind_paths_set = self.pending_kind_paths_set.clone();
        shell.indexing.in_flight_kind_paths = self.in_flight_kind_paths.clone();
        shell.indexing.kind_resolution_epoch = self.kind_resolution_epoch;
        shell.indexing.kind_resolution_in_progress = self.kind_resolution_in_progress;
        shell.indexing.incremental_filtered_entries = self.incremental_filtered_entries.clone();
        shell.indexing.last_incremental_results_refresh = self.last_incremental_results_refresh;
        shell.indexing.last_search_snapshot_len = self.last_search_snapshot_len;
        shell.indexing.search_resume_pending = self.search_resume_pending;
        shell.indexing.search_rerun_pending = self.search_rerun_pending;
    }
}

impl TabQueryState {
    pub(super) fn from_shell(shell: &FlistWalkerApp) -> Self {
        Self {
            query: shell.runtime.query_state.query.clone(),
            query_history: shell.runtime.query_state.query_history.clone(),
            query_history_cursor: shell.runtime.query_state.query_history_cursor,
            query_history_draft: shell.runtime.query_state.query_history_draft.clone(),
            query_history_dirty_since: shell.runtime.query_state.query_history_dirty_since,
            history_search_active: shell.runtime.query_state.history_search_active,
            history_search_query: shell.runtime.query_state.history_search_query.clone(),
            history_search_original_query: shell.runtime.query_state.history_search_original_query.clone(),
            history_search_results: shell.runtime.query_state.history_search_results.clone(),
            history_search_current: shell.runtime.query_state.history_search_current,
        }
    }

    pub(super) fn apply_shell(&self, shell: &mut FlistWalkerApp) {
        shell.runtime.query_state.query = self.query.clone();
        shell.runtime.query_state.query_history = self.query_history.clone();
        shell.runtime.query_state.query_history_cursor = self.query_history_cursor;
        shell.runtime.query_state.query_history_draft = self.query_history_draft.clone();
        shell.runtime.query_state.query_history_dirty_since = self.query_history_dirty_since;
        shell.runtime.query_state.history_search_active = self.history_search_active;
        shell.runtime.query_state.history_search_query = self.history_search_query.clone();
        shell.runtime.query_state.history_search_original_query =
            self.history_search_original_query.clone();
        shell.runtime.query_state.history_search_results = self.history_search_results.clone();
        shell.runtime.query_state.history_search_current = self.history_search_current;
    }
}

impl TabResultState {
    pub(super) fn from_shell(shell: &FlistWalkerApp) -> Self {
        Self {
            base_results: shell.runtime.base_results.clone(),
            results: shell.runtime.results.clone(),
            result_sort_mode: shell.runtime.result_sort_mode,
            pending_sort_request_id: shell.worker_bus.sort.pending_request_id,
            sort_in_progress: shell.worker_bus.sort.in_progress,
            pinned_paths: shell.runtime.pinned_paths.clone(),
            current_row: shell.runtime.current_row,
            preview: shell.runtime.preview.clone(),
            results_compacted: false,
        }
    }

    pub(super) fn apply_shell(&self, shell: &mut FlistWalkerApp) {
        shell.runtime.base_results = self.base_results.clone();
        shell.runtime.results = self.results.clone();
        shell.runtime.result_sort_mode = self.result_sort_mode;
        shell.worker_bus.sort.pending_request_id = self.pending_sort_request_id;
        shell.worker_bus.sort.in_progress = self.sort_in_progress;
        shell.runtime.pinned_paths = self.pinned_paths.clone();
        shell.runtime.current_row = self.current_row;
        shell.runtime.preview = self.preview.clone();
    }
}

impl AppTabState {
    pub(super) fn from_shell(shell: &FlistWalkerApp, id: u64) -> Self {
        Self {
            id,
            root: shell.runtime.root.clone(),
            tab_accent: shell
                .tabs
                .get(shell.tabs.active_tab)
                .and_then(|tab| tab.tab_accent),
            use_filelist: shell.runtime.use_filelist,
            use_regex: shell.runtime.use_regex,
            ignore_case: shell.runtime.ignore_case,
            include_files: shell.runtime.include_files,
            include_dirs: shell.runtime.include_dirs,
            index_state: TabIndexState::from_shell(shell),
            query_state: TabQueryState::from_shell(shell),
            pending_restore_refresh: shell.tabs.pending_restore_refresh,
            result_state: TabResultState::from_shell(shell),
            notice: shell.runtime.notice.clone(),
            pending_request_id: shell.search.pending_request_id(),
            pending_preview_request_id: shell.worker_bus.preview.pending_request_id,
            pending_action_request_id: shell.worker_bus.action.pending_request_id,
            search_in_progress: shell.search.in_progress(),
            preview_in_progress: shell.worker_bus.preview.in_progress,
            action_in_progress: shell.worker_bus.action.in_progress,
            scroll_to_current: shell.ui.scroll_to_current,
            focus_query_requested: shell.ui.focus_query_requested,
            unfocus_query_requested: shell.ui.unfocus_query_requested,
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
                pending_kind_paths: VecDeque::new(),
                pending_kind_paths_set: HashSet::new(),
                in_flight_kind_paths: HashSet::new(),
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
            query_history: shell.runtime.query_state.query_history.clone(),
                query_history_cursor: None,
                query_history_draft: None,
                query_history_dirty_since: None,
                history_search_active: false,
                history_search_query: String::new(),
                history_search_original_query: String::new(),
                history_search_results: Vec::new(),
                history_search_current: None,
            },
            pending_restore_refresh: true,
            result_state: TabResultState {
                base_results: Vec::new(),
                results: Vec::new(),
                result_sort_mode: ResultSortMode::Score,
                pending_sort_request_id: None,
                sort_in_progress: false,
                pinned_paths: HashSet::new(),
                current_row: Some(0),
                preview: String::new(),
                results_compacted: false,
            },
            notice: "Restored tab".to_string(),
            pending_request_id: None,
            pending_preview_request_id: None,
            pending_action_request_id: None,
            search_in_progress: false,
            preview_in_progress: false,
            action_in_progress: false,
            scroll_to_current: true,
            focus_query_requested: false,
            unfocus_query_requested: false,
        }
    }

    pub(super) fn apply_shell(&self, shell: &mut FlistWalkerApp) {
        shell.runtime.root = self.root.clone();
        shell.runtime.use_filelist = self.use_filelist;
        shell.runtime.use_regex = self.use_regex;
        shell.runtime.ignore_case = self.ignore_case;
        shell.runtime.include_files = self.include_files;
        shell.runtime.include_dirs = self.include_dirs;
        self.index_state.apply_shell(shell);
        self.query_state.apply_shell(shell);
        shell.tabs.pending_restore_refresh = self.pending_restore_refresh;
        self.result_state.apply_shell(shell);
        shell.runtime.notice = self.notice.clone();
        shell.search.set_pending_request_id(self.pending_request_id);
        shell.worker_bus.preview.pending_request_id = self.pending_preview_request_id;
        shell.worker_bus.action.pending_request_id = self.pending_action_request_id;
        shell.search.set_in_progress(self.search_in_progress);
        shell.worker_bus.preview.in_progress = self.preview_in_progress;
        shell.worker_bus.action.in_progress = self.action_in_progress;
        shell.ui.scroll_to_current = self.scroll_to_current;
        shell.ui.focus_query_requested = self.focus_query_requested;
        shell.ui.unfocus_query_requested = self.unfocus_query_requested;
    }

    pub(super) fn into_saved(self, history_persist_disabled: bool) -> SavedTabState {
        SavedTabState {
            root: self.root.to_string_lossy().to_string(),
            use_filelist: self.use_filelist,
            use_regex: self.use_regex,
            ignore_case: self.ignore_case,
            include_files: self.include_files,
            include_dirs: self.include_dirs,
            query: self.query_state.query,
            query_history: if history_persist_disabled {
                Vec::new()
            } else {
                self.query_state.query_history.into_iter().collect()
            },
            tab_accent: self.tab_accent,
        }
    }
}

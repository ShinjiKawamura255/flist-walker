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
pub(super) struct AppTabState {
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

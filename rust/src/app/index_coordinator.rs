use super::*;

pub(super) struct IndexCoordinator {
    pub(super) tx: Sender<IndexRequest>,
    pub(super) rx: Receiver<IndexResponse>,
    pub(super) next_request_id: u64,
    pub(super) pending_request_id: Option<u64>,
    pub(super) latest_request_ids: Arc<Mutex<HashMap<u64, u64>>>,
    pub(super) pending_queue: VecDeque<IndexRequest>,
    pub(super) inflight_requests: HashSet<u64>,
    pub(super) in_progress: bool,
    pub(super) incremental_filtered_entries: Vec<Entry>,
    pub(super) pending_entries: VecDeque<IndexEntry>,
    pub(super) pending_entries_request_id: Option<u64>,
    pub(super) pending_kind_paths: VecDeque<PathBuf>,
    pub(super) pending_kind_paths_set: HashSet<PathBuf>,
    pub(super) in_flight_kind_paths: HashSet<PathBuf>,
    pub(super) kind_resolution_epoch: u64,
    pub(super) kind_resolution_in_progress: bool,
    pub(super) last_incremental_results_refresh: Instant,
    pub(super) last_search_snapshot_len: usize,
    pub(super) search_resume_pending: bool,
    pub(super) search_rerun_pending: bool,
    pub(super) request_tabs: HashMap<u64, u64>,
    pub(super) background_states: HashMap<u64, BackgroundIndexState>,
}

impl IndexCoordinator {
    pub(super) fn new(
        tx: Sender<IndexRequest>,
        rx: Receiver<IndexResponse>,
        latest_request_ids: Arc<Mutex<HashMap<u64, u64>>>,
    ) -> Self {
        Self {
            tx,
            rx,
            next_request_id: 1,
            pending_request_id: None,
            latest_request_ids,
            pending_queue: VecDeque::new(),
            inflight_requests: HashSet::new(),
            in_progress: false,
            incremental_filtered_entries: Vec::new(),
            pending_entries: VecDeque::new(),
            pending_entries_request_id: None,
            pending_kind_paths: VecDeque::new(),
            pending_kind_paths_set: HashSet::new(),
            in_flight_kind_paths: HashSet::new(),
            kind_resolution_epoch: 1,
            kind_resolution_in_progress: false,
            last_incremental_results_refresh: Instant::now(),
            last_search_snapshot_len: 0,
            search_resume_pending: false,
            search_rerun_pending: false,
            request_tabs: HashMap::new(),
            background_states: HashMap::new(),
        }
    }

    pub(super) fn clear_for_tab(&mut self, tab_id: u64) {
        self.request_tabs.retain(|_, id| *id != tab_id);
        self.pending_queue.retain(|req| req.tab_id != tab_id);
        if let Ok(mut latest) = self.latest_request_ids.lock() {
            latest.remove(&tab_id);
        }
        self.background_states
            .retain(|request_id, _| self.request_tabs.contains_key(request_id));
    }

    pub(super) fn allocate_request_id(&mut self, tab_id: Option<u64>) -> u64 {
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1);
        if let Some(tab_id) = tab_id {
            self.request_tabs.insert(request_id, tab_id);
            if let Ok(mut latest) = self.latest_request_ids.lock() {
                latest.insert(tab_id, request_id);
            }
        }
        request_id
    }

    pub(super) fn begin_active_refresh(&mut self, request_id: u64, query_non_empty: bool) {
        self.pending_request_id = Some(request_id);
        self.in_progress = true;
        self.search_resume_pending = query_non_empty;
        self.search_rerun_pending = false;
    }

    pub(super) fn begin_active_refresh_with_inflight(
        &mut self,
        request_id: u64,
        query_non_empty: bool,
    ) {
        self.begin_active_refresh(request_id, query_non_empty);
        self.inflight_requests.insert(request_id);
    }

    pub(super) fn begin_background_refresh(
        &mut self,
        tab: &mut AppTabState,
        request_id: u64,
        notice: &str,
    ) {
        tab.index_state.pending_index_request_id = Some(request_id);
        tab.index_state.index_in_progress = true;
        tab.pending_restore_refresh = false;
        tab.pending_request_id = None;
        tab.search_in_progress = false;
        tab.index_state.search_resume_pending = !tab.query_state.query.trim().is_empty();
        tab.index_state.search_rerun_pending = false;
        tab.index_state.index.entries.clear();
        tab.index_state.index.source = IndexSource::None;
        tab.index_state.pending_index_entries.clear();
        tab.index_state.pending_index_entries_request_id = None;
        tab.index_state.pending_kind_paths.clear();
        tab.index_state.pending_kind_paths_set.clear();
        tab.index_state.in_flight_kind_paths.clear();
        tab.index_state.kind_resolution_in_progress = false;
        tab.index_state.kind_resolution_epoch =
            tab.index_state.kind_resolution_epoch.saturating_add(1);
        tab.pending_preview_request_id = None;
        tab.preview_in_progress = false;
        tab.index_state.last_incremental_results_refresh = Instant::now();
        tab.index_state.last_search_snapshot_len = 0;
        tab.notice = notice.to_string();
    }

    pub(super) fn cleanup_request(&mut self, request_id: u64) {
        self.request_tabs.remove(&request_id);
        self.background_states.remove(&request_id);
        self.inflight_requests.remove(&request_id);
    }

    pub(super) fn settle_active_terminal_state(&mut self) {
        self.in_progress = false;
        self.pending_request_id = None;
        self.search_resume_pending = false;
        self.search_rerun_pending = false;
        self.pending_entries.clear();
        self.pending_entries_request_id = None;
    }
}

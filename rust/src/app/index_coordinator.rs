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
}

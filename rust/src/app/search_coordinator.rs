use super::*;

pub(super) struct SearchCoordinator {
    pub(super) tx: Sender<SearchRequest>,
    pub(super) rx: Receiver<SearchResponse>,
    pub(super) next_request_id: u64,
    pub(super) pending_request_id: Option<u64>,
    pub(super) in_progress: bool,
    pub(super) request_tabs: HashMap<u64, u64>,
}

impl SearchCoordinator {
    pub(super) fn new(tx: Sender<SearchRequest>, rx: Receiver<SearchResponse>) -> Self {
        Self {
            tx,
            rx,
            next_request_id: 1,
            pending_request_id: None,
            in_progress: false,
            request_tabs: HashMap::new(),
        }
    }
}

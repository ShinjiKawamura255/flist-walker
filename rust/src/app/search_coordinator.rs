use super::*;

pub(super) enum SearchResponseRoute {
    Active,
    Background(u64),
    Stale,
}

pub(super) struct SearchCoordinator {
    pub(super) tx: Sender<SearchRequest>,
    pub(super) rx: Receiver<SearchResponse>,
    next_request_id: u64,
    pending_request_id: Option<u64>,
    in_progress: bool,
    request_tabs: HashMap<u64, u64>,
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

    pub(super) fn allocate_request_id(&mut self) -> u64 {
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1);
        request_id
    }

    pub(super) fn begin_active_request(&mut self, tab_id: Option<u64>) -> u64 {
        let request_id = self.allocate_request_id();
        self.pending_request_id = Some(request_id);
        self.in_progress = true;
        if let Some(tab_id) = tab_id {
            self.bind_request_tab(request_id, tab_id);
        }
        request_id
    }

    pub(super) fn begin_tab_request(&mut self, tab: &mut AppTabState) -> u64 {
        let request_id = self.allocate_request_id();
        tab.begin_search_request(request_id);
        self.bind_request_tab(request_id, tab.id);
        request_id
    }

    pub(super) fn pending_request_id(&self) -> Option<u64> {
        self.pending_request_id
    }

    pub(super) fn set_pending_request_id(&mut self, request_id: Option<u64>) {
        self.pending_request_id = request_id;
    }

    pub(super) fn in_progress(&self) -> bool {
        self.in_progress
    }

    pub(super) fn set_in_progress(&mut self, in_progress: bool) {
        self.in_progress = in_progress;
    }

    pub(super) fn clear_active_request_state(&mut self) {
        self.pending_request_id = None;
        self.in_progress = false;
    }

    pub(super) fn bind_request_tab(&mut self, request_id: u64, tab_id: u64) {
        self.request_tabs.insert(request_id, tab_id);
    }

    pub(super) fn route_response(&mut self, request_id: u64) -> SearchResponseRoute {
        if Some(request_id) == self.pending_request_id {
            return SearchResponseRoute::Active;
        }
        match self.take_request_tab(request_id) {
            Some(tab_id) => SearchResponseRoute::Background(tab_id),
            None => SearchResponseRoute::Stale,
        }
    }

    pub(super) fn take_request_tab(&mut self, request_id: u64) -> Option<u64> {
        self.request_tabs.remove(&request_id)
    }

    pub(super) fn clear_for_tab(&mut self, tab_id: u64) {
        self.request_tabs.retain(|_, id| *id != tab_id);
    }
}

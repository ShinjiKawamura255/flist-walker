use super::{AppTabState, SearchRequest, SearchResponse};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

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
    request_cancellations: HashMap<u64, Arc<AtomicBool>>,
    latest_tab_requests: HashMap<u64, u64>,
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
            request_cancellations: HashMap::new(),
            latest_tab_requests: HashMap::new(),
        }
    }

    pub(super) fn allocate_request_id(&mut self) -> u64 {
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1);
        request_id
    }

    pub(super) fn begin_active_request(&mut self, tab_id: Option<u64>) -> (u64, Arc<AtomicBool>) {
        let request_id = self.allocate_request_id();
        let cancel = self.register_request(request_id, tab_id);
        self.pending_request_id = Some(request_id);
        self.in_progress = true;
        (request_id, cancel)
    }

    pub(super) fn begin_tab_request(&mut self, tab: &mut AppTabState) -> (u64, Arc<AtomicBool>) {
        let request_id = self.allocate_request_id();
        let cancel = self.register_request(request_id, Some(tab.id));
        tab.begin_search_request(request_id);
        (request_id, cancel)
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
        if let Some(request_id) = self.pending_request_id.take() {
            let tab_id = self.take_request_tab(request_id);
            self.cancel_request(request_id);
            self.finish_request(request_id, tab_id);
        }
        self.in_progress = false;
    }

    pub(super) fn bind_request_tab(&mut self, request_id: u64, tab_id: u64) {
        self.request_tabs.insert(request_id, tab_id);
    }

    pub(super) fn route_response(&mut self, request_id: u64) -> SearchResponseRoute {
        let is_active = Some(request_id) == self.pending_request_id;
        let tab_id = self.take_request_tab(request_id);
        self.finish_request(request_id, tab_id);
        if is_active {
            return SearchResponseRoute::Active;
        }
        match tab_id {
            Some(tab_id) => SearchResponseRoute::Background(tab_id),
            None => SearchResponseRoute::Stale,
        }
    }

    pub(super) fn take_request_tab(&mut self, request_id: u64) -> Option<u64> {
        self.request_tabs.remove(&request_id)
    }

    pub(super) fn clear_for_tab(&mut self, tab_id: u64) {
        let request_ids = self
            .request_tabs
            .iter()
            .filter_map(|(request_id, id)| (*id == tab_id).then_some(*request_id))
            .collect::<Vec<_>>();
        for request_id in request_ids {
            self.cancel_request(request_id);
            self.request_tabs.remove(&request_id);
        }
        self.latest_tab_requests.remove(&tab_id);
    }

    #[cfg(test)]
    pub(super) fn request_routes_for_test(&self) -> Vec<(u64, u64)> {
        let mut routes = self
            .request_tabs
            .iter()
            .map(|(request_id, tab_id)| (*request_id, *tab_id))
            .collect::<Vec<_>>();
        routes.sort_unstable();
        routes
    }

    fn register_request(&mut self, request_id: u64, tab_id: Option<u64>) -> Arc<AtomicBool> {
        if let Some(tab_id) = tab_id {
            if let Some(previous) = self.latest_tab_requests.insert(tab_id, request_id) {
                self.cancel_request(previous);
                self.request_tabs.remove(&previous);
            }
            self.bind_request_tab(request_id, tab_id);
        } else if let Some(previous) = self.pending_request_id {
            self.cancel_request(previous);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.request_cancellations
            .insert(request_id, Arc::clone(&cancel));
        cancel
    }

    fn cancel_request(&mut self, request_id: u64) {
        if let Some(cancel) = self.request_cancellations.remove(&request_id) {
            cancel.store(true, Ordering::Release);
        }
    }

    fn finish_request(&mut self, request_id: u64, tab_id: Option<u64>) {
        self.request_cancellations.remove(&request_id);
        if let Some(tab_id) = tab_id {
            if self.latest_tab_requests.get(&tab_id) == Some(&request_id) {
                self.latest_tab_requests.remove(&tab_id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SearchCoordinator;
    use crate::app::{SearchRequest, SearchResponse};
    use std::sync::atomic::Ordering;
    use std::sync::mpsc;

    #[test]
    fn newer_active_search_cancels_the_superseded_request() {
        let (request_tx, _request_rx) = mpsc::channel::<SearchRequest>();
        let (_response_tx, response_rx) = mpsc::channel::<SearchResponse>();
        let mut coordinator = SearchCoordinator::new(request_tx, response_rx);

        let (_first_id, first_cancel) = coordinator.begin_active_request(None);
        let (_second_id, second_cancel) = coordinator.begin_active_request(None);

        assert!(first_cancel.load(Ordering::Acquire));
        assert!(!second_cancel.load(Ordering::Acquire));
    }

    #[test]
    fn tab_scoped_search_cancels_only_the_same_tab() {
        let (request_tx, _request_rx) = mpsc::channel::<SearchRequest>();
        let (_response_tx, response_rx) = mpsc::channel::<SearchResponse>();
        let mut coordinator = SearchCoordinator::new(request_tx, response_rx);

        let (_tab_one_first_id, tab_one_first) = coordinator.begin_active_request(Some(1));
        let (_tab_two_id, tab_two) = coordinator.begin_active_request(Some(2));
        assert!(!tab_one_first.load(Ordering::Acquire));
        assert!(!tab_two.load(Ordering::Acquire));

        let (_tab_one_second_id, tab_one_second) = coordinator.begin_active_request(Some(1));
        assert!(tab_one_first.load(Ordering::Acquire));
        assert!(!tab_one_second.load(Ordering::Acquire));
        assert!(!tab_two.load(Ordering::Acquire));
    }
}

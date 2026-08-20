use super::events::{Event, TerminalOutcome};
use super::invariants::{validate, SemanticSnapshot};
use crate::app::tests::*;
use crate::app::worker_channel::BoundedReceiver;

pub(super) struct StatefulHarness {
    app: FlistWalkerApp,
    base: PathBuf,
    roots: Vec<PathBuf>,
    index_requests: BoundedReceiver<IndexRequest>,
    index_responses: mpsc::Sender<IndexResponse>,
    pending_indexes: VecDeque<IndexRequest>,
    search_requests: mpsc::Receiver<SearchRequest>,
    search_responses: mpsc::Sender<SearchResponse>,
    pending_searches: VecDeque<SearchRequest>,
    next_stale_request_id: u64,
}

impl StatefulHarness {
    pub(super) fn new(label: &str) -> Self {
        let base = test_root(label);
        let roots = (0..3)
            .map(|index| base.join(format!("root-{index}")))
            .collect::<Vec<_>>();
        for (root_index, root) in roots.iter().enumerate() {
            fs::create_dir_all(root).expect("create endurance root");
            for file_index in 0..3 {
                fs::write(
                    root.join(format!("root-{root_index}-file-{file_index}.txt")),
                    format!("fixture {root_index}/{file_index}"),
                )
                .expect("write endurance fixture");
            }
        }

        let mut app = FlistWalkerApp::new(roots[0].clone(), 20, String::new());
        reset_index_request_state_for_test(&mut app);

        let (index_tx, index_requests) = bounded_request_channel::<IndexRequest>(8);
        let (index_responses, index_rx) = mpsc::channel::<IndexResponse>();
        app.shell.indexing.tx = index_tx;
        app.shell.indexing.rx = index_rx;

        let (search_tx, search_requests) = mpsc::channel::<SearchRequest>();
        let (search_responses, search_rx) = mpsc::channel::<SearchResponse>();
        app.shell.search = SearchCoordinator::new(search_tx, search_rx);
        app.sync_active_tab_state();

        Self {
            app,
            base,
            roots,
            index_requests,
            index_responses,
            pending_indexes: VecDeque::new(),
            search_requests,
            search_responses,
            pending_searches: VecDeque::new(),
            next_stale_request_id: 1_000_000,
        }
    }

    pub(super) fn run(&mut self, seed: u64, events: &[Event]) {
        self.capture_requests();
        self.assert_invariants(seed, 0, &Event::RefreshIndex);
        for (step, event) in events.iter().enumerate() {
            self.apply(event);
            self.capture_requests();
            self.assert_invariants(seed, step + 1, event);
        }
    }

    pub(super) fn quiesce(&mut self, seed: u64) {
        for step in 0..128 {
            self.capture_requests();
            if let Some(request) = self.pending_indexes.pop_front() {
                self.respond_to_index(request, TerminalOutcome::Canceled);
            } else if let Some(request) = self.pending_searches.pop_front() {
                self.respond_to_search(request);
            } else {
                self.app
                    .poll_index_response_with_budget_for_test(Duration::from_millis(20));
                self.app.poll_search_response();
                self.app.dispatch_index_queue();
                self.capture_requests();
                if self.is_quiescent() {
                    self.assert_invariants(seed, step, &Event::DeliverStaleIndex);
                    return;
                }
            }
        }

        panic!(
            "stateful endurance did not quiesce: seed={seed:#x}; state={}",
            self.snapshot().digest()
        );
    }

    pub(super) fn cleanup(self) {
        let Self { app, base, .. } = self;
        drop(app);
        let _ = fs::remove_dir_all(base);
    }

    fn apply(&mut self, event: &Event) {
        match *event {
            Event::CreateTab => {
                if self.app.shell.tabs.len() < 5 {
                    self.app.create_new_tab();
                }
            }
            Event::CloseTab(index) => {
                let len = self.app.shell.tabs.len();
                self.app.close_tab_index(index % len.max(1));
            }
            Event::RestoreTab => self.app.restore_recently_closed_tab(),
            Event::SwitchTab(index) => {
                let len = self.app.shell.tabs.len();
                self.app.switch_to_tab_index(index % len.max(1));
            }
            Event::ReorderTab { from, to } => {
                let len = self.app.shell.tabs.len();
                self.app.move_tab(from % len.max(1), to % len.max(1));
            }
            Event::ChangeQuery(query) => {
                self.app.shell.runtime.query_state.query = match query % 6 {
                    0 => String::new(),
                    1 => "file".to_string(),
                    2 => "root".to_string(),
                    3 => "^root".to_string(),
                    4 => "!missing".to_string(),
                    _ => "'fixture".to_string(),
                };
                self.app.update_results();
                self.app.sync_active_tab_state();
            }
            Event::ChangeRoot(index) => {
                self.app
                    .apply_root_change_direct(self.roots[index % self.roots.len()].clone());
            }
            Event::RefreshIndex => self.app.request_index_refresh(),
            Event::CompleteOldestIndex(outcome) => {
                if let Some(request) = self.pending_indexes.pop_front() {
                    self.respond_to_index(request, outcome);
                }
            }
            Event::CompleteNewestIndex(outcome) => {
                if let Some(request) = self.pending_indexes.pop_back() {
                    self.respond_to_index(request, outcome);
                }
            }
            Event::CompleteOldestSearch => {
                if let Some(request) = self.pending_searches.pop_front() {
                    self.respond_to_search(request);
                }
            }
            Event::DeliverStaleIndex => self.deliver_stale_index(),
            Event::DeliverStaleSearch => self.deliver_stale_search(),
        }
    }

    fn capture_requests(&mut self) {
        while let Ok(request) = self.index_requests.try_recv() {
            self.pending_indexes.push_back(request);
        }
        while let Ok(request) = self.search_requests.try_recv() {
            self.pending_searches.push_back(request);
        }
    }

    fn respond_to_index(&mut self, request: IndexRequest, outcome: TerminalOutcome) {
        let request_id = request.request_id;
        match outcome {
            TerminalOutcome::Finished | TerminalOutcome::Replaced => {
                let entry = IndexEntry {
                    path: request.root.join(format!("request-{request_id}.txt")),
                    kind: EntryKind::file(),
                    kind_known: true,
                };
                let response = if outcome == TerminalOutcome::Replaced {
                    IndexResponse::ReplaceAll {
                        request_id,
                        entries: vec![entry],
                    }
                } else {
                    IndexResponse::Batch {
                        request_id,
                        entries: vec![entry],
                    }
                };
                self.index_responses
                    .send(response)
                    .expect("send index data");
                self.index_responses
                    .send(IndexResponse::Finished {
                        request_id,
                        source: IndexSource::Walker,
                    })
                    .expect("send index finish");
            }
            TerminalOutcome::Failed => self
                .index_responses
                .send(IndexResponse::Failed {
                    request_id,
                    error: "injected endurance failure".to_string(),
                })
                .expect("send index failure"),
            TerminalOutcome::Canceled => self
                .index_responses
                .send(IndexResponse::Canceled { request_id })
                .expect("send index cancel"),
        }
        self.app
            .poll_index_response_with_budget_for_test(Duration::from_millis(20));
        self.app.poll_search_response();
        self.app.dispatch_index_queue();
    }

    fn respond_to_search(&mut self, request: SearchRequest) {
        let SearchRequest {
            request_id,
            entries,
            limit,
            sort_mode,
            sort_scope,
            ..
        } = request;
        let results = entries
            .iter()
            .take(limit.min(2))
            .enumerate()
            .map(|(index, entry)| (entry.path.clone(), 10.0 - index as f64))
            .collect::<Vec<_>>();
        let total_match_count = results.len();
        self.search_responses
            .send(SearchResponse {
                request_id,
                results,
                total_match_count,
                sort_mode,
                sort_scope,
                error: None,
            })
            .expect("send search response");
        self.app.poll_search_response();
    }

    fn deliver_stale_index(&mut self) {
        let before = self.snapshot();
        let request_id = self.take_stale_request_id();
        self.index_responses
            .send(IndexResponse::Finished {
                request_id,
                source: IndexSource::Walker,
            })
            .expect("send stale index");
        self.app
            .poll_index_response_with_budget_for_test(Duration::from_millis(20));
        assert_eq!(
            before,
            self.snapshot(),
            "stale index changed semantic state"
        );
    }

    fn deliver_stale_search(&mut self) {
        let before = self.snapshot();
        let request_id = self.take_stale_request_id();
        self.search_responses
            .send(SearchResponse {
                request_id,
                results: Vec::new(),
                total_match_count: 0,
                sort_mode: ResultSortMode::Score,
                sort_scope: ResultSortScope::ShownResults,
                error: None,
            })
            .expect("send stale search");
        self.app.poll_search_response();
        assert_eq!(
            before,
            self.snapshot(),
            "stale search changed semantic state"
        );
    }

    fn take_stale_request_id(&mut self) -> u64 {
        let request_id = self.next_stale_request_id;
        self.next_stale_request_id = self.next_stale_request_id.saturating_add(1);
        request_id
    }

    fn snapshot(&self) -> SemanticSnapshot {
        snapshot_for_app(&self.app, &self.roots)
    }

    fn assert_invariants(&self, seed: u64, step: usize, event: &Event) {
        let snapshot = self.snapshot();
        if let Err(error) = validate(&snapshot) {
            panic!(
                "stateful endurance invariant failed: {error}; seed={seed:#x}; step={step}; event={event:?}; state={}; replay=cargo test --locked stateful_endurance_replay --lib -- --ignored --nocapture",
                snapshot.digest()
            );
        }
    }

    fn is_quiescent(&self) -> bool {
        self.pending_indexes.is_empty()
            && self.pending_searches.is_empty()
            && self.app.shell.indexing.pending_queue.is_empty()
            && self.app.shell.indexing.inflight_requests.is_empty()
            && self.app.shell.indexing.pending_request_id.is_none()
            && !self.app.shell.indexing.in_progress
            && self.app.shell.indexing.pending_entries.is_empty()
            && self.app.shell.indexing.pending_finish.is_none()
            && self.app.shell.search.pending_request_id().is_none()
            && !self.app.shell.search.in_progress()
            && self.app.shell.search.request_routes_for_test().is_empty()
    }
}

pub(super) fn snapshot_for_app(app: &FlistWalkerApp, roots: &[PathBuf]) -> SemanticSnapshot {
    let tab_ids = app.shell.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>();
    let mut routed_tab_ids = app
        .shell
        .search
        .request_routes_for_test()
        .into_iter()
        .map(|(_, tab_id)| tab_id)
        .chain(app.shell.indexing.request_tabs.values().copied())
        .chain(app.shell.tabs.routed_tab_ids_for_test())
        .collect::<Vec<_>>();
    routed_tab_ids.sort_unstable();

    let active_root = roots
        .iter()
        .position(|root| path_key(root) == path_key(&app.shell.runtime.root))
        .unwrap_or(usize::MAX);

    SemanticSnapshot {
        tab_ids,
        active_tab: app.shell.tabs.active_tab_index(),
        active_root,
        active_query: app.shell.runtime.query_state.query.clone(),
        results_len: app.shell.runtime.results.len(),
        total_match_count: app.shell.runtime.total_match_count,
        current_row: app.shell.runtime.current_row,
        index_pending: app.shell.indexing.pending_queue.len(),
        index_inflight: app.shell.indexing.inflight_requests.len(),
        routed_tab_ids,
        active_index_pending: app.shell.indexing.pending_request_id.is_some()
            || app.shell.indexing.in_progress,
        active_search_pending: app.shell.search.pending_request_id().is_some()
            || app.shell.search.in_progress(),
    }
}

use super::events::{Event, IndexData, TerminalOutcome, WorkerOutcome};
use super::invariants::{validate, SemanticSnapshot, TabSemanticSnapshot};
use crate::app::tests::*;
use crate::app::worker_channel::BoundedReceiver;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

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
    preview_requests: mpsc::Receiver<PreviewRequest>,
    preview_responses: mpsc::Sender<PreviewResponse>,
    pending_previews: VecDeque<PreviewRequest>,
    action_requests: BoundedReceiver<ActionRequest>,
    action_responses: mpsc::Sender<ActionResponse>,
    pending_actions: VecDeque<ActionRequest>,
    sort_requests: mpsc::Receiver<SortMetadataRequest>,
    sort_responses: mpsc::Sender<SortMetadataResponse>,
    pending_sorts: VecDeque<SortMetadataRequest>,
    filelist_requests: mpsc::Receiver<FileListRequest>,
    filelist_responses: mpsc::Sender<FileListResponse>,
    pending_filelists: VecDeque<FileListRequest>,
    next_stale_request_id: u64,
    next_index_entry_id: u64,
    replay_steps: usize,
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
            fs::write(root.join("fixture.txt"), format!("fixture {root_index}"))
                .expect("write shared endurance fixture");
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

        let (preview_tx, preview_requests) = mpsc::channel::<PreviewRequest>();
        let (preview_responses, preview_rx) = mpsc::channel::<PreviewResponse>();
        app.shell.worker_bus.preview.tx = preview_tx;
        app.shell.worker_bus.preview.rx = preview_rx;
        app.shell.worker_bus.preview.clear_request();

        let (action_tx, action_requests) = bounded_request_channel::<ActionRequest>(8);
        let (action_responses, action_rx) = mpsc::channel::<ActionResponse>();
        app.shell.worker_bus.action.tx = action_tx;
        app.shell.worker_bus.action.rx = action_rx;
        app.shell.worker_bus.action.clear_request();

        let (sort_tx, sort_requests) = mpsc::channel::<SortMetadataRequest>();
        let (sort_responses, sort_rx) = mpsc::channel::<SortMetadataResponse>();
        app.shell.worker_bus.sort.tx = sort_tx;
        app.shell.worker_bus.sort.rx = sort_rx;
        app.shell.worker_bus.sort.clear_request();

        let (filelist_tx, filelist_requests) = mpsc::channel::<FileListRequest>();
        let (filelist_responses, filelist_rx) = mpsc::channel::<FileListResponse>();
        app.shell.worker_bus.filelist.tx = filelist_tx;
        app.shell.worker_bus.filelist.rx = filelist_rx;
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
            preview_requests,
            preview_responses,
            pending_previews: VecDeque::new(),
            action_requests,
            action_responses,
            pending_actions: VecDeque::new(),
            sort_requests,
            sort_responses,
            pending_sorts: VecDeque::new(),
            filelist_requests,
            filelist_responses,
            pending_filelists: VecDeque::new(),
            next_stale_request_id: 1_000_000,
            next_index_entry_id: 1,
            replay_steps: 0,
        }
    }

    pub(super) fn run(&mut self, seed: u64, events: &[Event]) {
        self.replay_steps = events.len();
        self.capture_requests();
        self.assert_invariants(seed, 0, &Event::RefreshIndex);
        for (step, event) in events.iter().enumerate() {
            let before = self.snapshot();
            let response_owner = self.response_owner(event);
            self.apply(event);
            self.capture_requests();
            if let Some(owner) = response_owner {
                self.assert_other_tab_content_unchanged(
                    &before,
                    &self.snapshot(),
                    owner,
                    seed,
                    step + 1,
                    event,
                );
            }
            self.assert_invariants(seed, step + 1, event);
        }
    }

    pub(super) fn quiesce(&mut self, seed: u64) {
        let max_steps = self.replay_steps.saturating_mul(3).saturating_add(256);
        for step in 0..max_steps {
            self.capture_requests();
            if let Some(request) = self.pending_indexes.pop_front() {
                self.respond_to_index(request, TerminalOutcome::Canceled);
            } else if let Some(request) = self.pending_searches.pop_front() {
                self.respond_to_search(request);
            } else if let Some(request) = self.pending_previews.pop_front() {
                self.respond_to_preview(request, WorkerOutcome::Finished);
            } else if let Some(request) = self.pending_actions.pop_front() {
                self.respond_to_action(request, WorkerOutcome::Finished);
            } else if let Some(request) = self.pending_sorts.pop_front() {
                self.respond_to_sort(request, WorkerOutcome::Finished);
            } else if let Some(request) = self.pending_filelists.pop_front() {
                self.respond_to_filelist(request, TerminalOutcome::Canceled);
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
            "stateful endurance did not quiesce: seed={seed:#x}; phase=quiescence; max_steps={max_steps}; index_routes={:?}; search_routes={:?}; response_routes={:?}; filelist_route={:?}; state={}; replay={}",
            self.app.shell.indexing.request_tabs,
            self.app.shell.search.request_routes_for_test(),
            self.app.shell.tabs.routed_tab_ids_for_test(),
            self.app.shell.features.filelist.workflow.pending_request_tab_id,
            self.snapshot().digest(),
            self.replay_command(seed),
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
            Event::DeliverOldestIndexData(data) => {
                if let Some((request_id, root)) = self
                    .pending_indexes
                    .front()
                    .map(|request| (request.request_id, request.root.clone()))
                {
                    self.respond_with_index_data(request_id, &root, data);
                }
            }
            Event::DeliverNewestIndexData(data) => {
                if let Some((request_id, root)) = self
                    .pending_indexes
                    .back()
                    .map(|request| (request.request_id, request.root.clone()))
                {
                    self.respond_with_index_data(request_id, &root, data);
                }
            }
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
            Event::RequestPreview => self.request_preview(),
            Event::CompleteOldestPreview(outcome) => {
                if let Some(request) = self.pending_previews.pop_front() {
                    self.respond_to_preview(request, outcome);
                }
            }
            Event::RequestAction => self.request_action(),
            Event::CompleteOldestAction(outcome) => {
                if let Some(request) = self.pending_actions.pop_front() {
                    self.respond_to_action(request, outcome);
                }
            }
            Event::RequestSort => self.request_sort(),
            Event::CompleteOldestSort(outcome) => {
                if let Some(request) = self.pending_sorts.pop_front() {
                    self.respond_to_sort(request, outcome);
                }
            }
            Event::RequestFileList => self.request_filelist(),
            Event::CompleteOldestFileList(outcome) => {
                if let Some(request) = self.pending_filelists.pop_front() {
                    self.respond_to_filelist(request, outcome);
                }
            }
            Event::DeliverStaleIndex => self.deliver_stale_index(),
            Event::DeliverStaleSearch => self.deliver_stale_search(),
        }
    }

    fn response_owner(&self, event: &Event) -> Option<Option<u64>> {
        let owner = match event {
            Event::DeliverOldestIndexData(_) | Event::CompleteOldestIndex(_) => {
                self.pending_indexes.front().and_then(|request| {
                    self.app
                        .shell
                        .indexing
                        .request_tabs
                        .get(&request.request_id)
                        .copied()
                })
            }
            Event::DeliverNewestIndexData(_) | Event::CompleteNewestIndex(_) => {
                self.pending_indexes.back().and_then(|request| {
                    self.app
                        .shell
                        .indexing
                        .request_tabs
                        .get(&request.request_id)
                        .copied()
                })
            }
            Event::CompleteOldestSearch => self.pending_searches.front().and_then(|request| {
                self.app
                    .shell
                    .search
                    .request_routes_for_test()
                    .into_iter()
                    .find_map(|(request_id, tab_id)| {
                        (request_id == request.request_id).then_some(tab_id)
                    })
            }),
            Event::CompleteOldestPreview(_) => self
                .pending_previews
                .front()
                .and_then(|request| self.app.preview_request_tab(request.request_id)),
            Event::CompleteOldestAction(_) => self
                .pending_actions
                .front()
                .and_then(|request| self.app.action_request_tab(request.request_id)),
            Event::CompleteOldestSort(_) => self
                .pending_sorts
                .front()
                .and_then(|request| self.app.sort_request_tab(request.request_id)),
            Event::CompleteOldestFileList(_) => {
                self.pending_filelists.front().and_then(|request| {
                    let workflow = &self.app.shell.features.filelist.workflow;
                    (workflow.pending_request_id == Some(request.request_id))
                        .then_some(request.tab_id)
                })
            }
            Event::DeliverStaleIndex | Event::DeliverStaleSearch => None,
            _ => return None,
        };
        Some(owner)
    }

    fn assert_other_tab_content_unchanged(
        &self,
        before: &SemanticSnapshot,
        after: &SemanticSnapshot,
        owner: Option<u64>,
        seed: u64,
        step: usize,
        event: &Event,
    ) {
        for before_tab in before.tabs.iter().filter(|tab| Some(tab.id) != owner) {
            let Some(after_tab) = after.tabs.iter().find(|tab| tab.id == before_tab.id) else {
                panic!(
                    "stateful response removed unrelated tab {}; {}",
                    before_tab.id,
                    self.failure_context(seed, step, event, after)
                );
            };
            let content_unchanged = before_tab.root == after_tab.root
                && before_tab.query == after_tab.query
                && before_tab.results_len == after_tab.results_len
                && before_tab.total_match_count == after_tab.total_match_count
                && before_tab.current_row == after_tab.current_row
                && before_tab.results_digest == after_tab.results_digest
                && before_tab.notice == after_tab.notice;
            assert!(
                content_unchanged,
                "stateful response changed unrelated tab {}; before={before_tab:?}; after={after_tab:?}; {}",
                before_tab.id,
                self.failure_context(seed, step, event, after)
            );
        }
    }

    fn capture_requests(&mut self) {
        while let Ok(request) = self.index_requests.try_recv() {
            self.pending_indexes.push_back(request);
        }
        while let Ok(request) = self.search_requests.try_recv() {
            self.pending_searches.push_back(request);
        }
        while let Ok(request) = self.preview_requests.try_recv() {
            self.pending_previews.push_back(request);
        }
        while let Ok(request) = self.action_requests.try_recv() {
            self.pending_actions.push_back(request);
        }
        while let Ok(request) = self.sort_requests.try_recv() {
            self.pending_sorts.push_back(request);
        }
        while let Ok(request) = self.filelist_requests.try_recv() {
            self.pending_filelists.push_back(request);
        }
    }

    fn respond_to_index(&mut self, request: IndexRequest, outcome: TerminalOutcome) {
        let request_id = request.request_id;
        match outcome {
            TerminalOutcome::Finished => {
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

    fn respond_with_index_data(&mut self, request_id: u64, root: &Path, data: IndexData) {
        let entry_path = root.join(format!(
            "request-{request_id}-entry-{}.txt",
            self.next_index_entry_id
        ));
        self.next_index_entry_id = self.next_index_entry_id.saturating_add(1);
        fs::write(&entry_path, "controlled endurance index entry")
            .expect("write controlled index entry");
        let entry = IndexEntry {
            path: entry_path,
            kind: EntryKind::file(),
            kind_known: true,
        };
        let response = match data {
            IndexData::Batch => IndexResponse::Batch {
                request_id,
                entries: vec![entry],
            },
            IndexData::ReplaceAll => IndexResponse::ReplaceAll {
                request_id,
                entries: vec![entry],
            },
        };
        self.index_responses
            .send(response)
            .expect("send index data");
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

    fn prepare_current_result(&mut self) -> Option<PathBuf> {
        let path = self.app.shell.runtime.results.first()?.0.clone();
        self.app.shell.runtime.current_row = Some(0);
        self.app.set_entry_kind(&path, EntryKind::file());
        Some(path)
    }

    fn request_preview(&mut self) {
        if self.prepare_current_result().is_none() {
            return;
        }
        self.app.clear_preview_cache();
        self.app.request_preview_for_current();
    }

    fn respond_to_preview(&mut self, request: PreviewRequest, outcome: WorkerOutcome) {
        let preview = match outcome {
            WorkerOutcome::Finished => "controlled endurance preview",
            WorkerOutcome::Failed => "Preview failed: controlled endurance failure",
        };
        self.preview_responses
            .send(PreviewResponse {
                request_id: request.request_id,
                path: request.path,
                preview: preview.to_string(),
            })
            .expect("send preview response");
        self.app.poll_preview_response();
    }

    fn request_action(&mut self) {
        if self.prepare_current_result().is_none() {
            return;
        }
        self.app.execute_selected();
    }

    fn respond_to_action(&mut self, request: ActionRequest, outcome: WorkerOutcome) {
        let notice = match outcome {
            WorkerOutcome::Finished => "controlled endurance action",
            WorkerOutcome::Failed => "Action failed: controlled endurance failure",
        };
        self.action_responses
            .send(ActionResponse {
                request_id: request.request_id,
                notice: notice.to_string(),
            })
            .expect("send action response");
        self.app.poll_action_response();
    }

    fn request_sort(&mut self) {
        if self.prepare_current_result().is_none() {
            return;
        }
        self.app.clear_sort_metadata_cache();
        self.app.set_result_sort_mode(ResultSortMode::SizeDesc);
    }

    fn respond_to_sort(&mut self, request: SortMetadataRequest, outcome: WorkerOutcome) {
        let entries = request
            .paths
            .into_iter()
            .map(|path| {
                let metadata = match outcome {
                    WorkerOutcome::Finished => SortMetadata {
                        size_bytes: Some(1),
                        ..SortMetadata::default()
                    },
                    WorkerOutcome::Failed => SortMetadata::default(),
                };
                (path, metadata)
            })
            .collect();
        self.sort_responses
            .send(SortMetadataResponse {
                request_id: request.request_id,
                entries,
                mode: request.mode,
            })
            .expect("send sort response");
        self.app.poll_sort_response();
    }

    fn request_filelist(&mut self) {
        if self.app.shell.features.filelist.workflow.in_progress {
            return;
        }
        let tab_id = self.app.current_tab_id().expect("active endurance tab");
        let root = self.app.shell.runtime.root.clone();
        self.app.start_filelist_creation(
            tab_id,
            root.clone(),
            vec![root.join("fixture.txt")],
            false,
        );
    }

    fn respond_to_filelist(&mut self, request: FileListRequest, outcome: TerminalOutcome) {
        let response = match outcome {
            TerminalOutcome::Finished => FileListResponse::Finished {
                request_id: request.request_id,
                path: request.root.join("FileList.txt"),
                root: request.root,
                count: request.entries.len(),
            },
            TerminalOutcome::Failed => FileListResponse::Failed {
                request_id: request.request_id,
                root: request.root,
                error: "controlled endurance failure".to_string(),
            },
            TerminalOutcome::Canceled => FileListResponse::Canceled {
                request_id: request.request_id,
                root: request.root,
            },
        };
        self.filelist_responses
            .send(response)
            .expect("send filelist response");
        self.app.poll_filelist_response();
    }

    fn deliver_stale_index(&mut self) {
        let request_id = self.take_stale_request_id();
        self.index_responses
            .send(IndexResponse::Finished {
                request_id,
                source: IndexSource::Walker,
            })
            .expect("send stale index");
        self.app
            .poll_index_response_with_budget_for_test(Duration::from_millis(20));
    }

    fn deliver_stale_search(&mut self) {
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
                "stateful endurance invariant failed: {error}; {}",
                self.failure_context(seed, step, event, &snapshot)
            );
        }
    }

    fn replay_command(&self, seed: u64) -> String {
        format!(
            "FLISTWALKER_ENDURANCE_SEED={seed:#x} FLISTWALKER_ENDURANCE_STEPS={} cargo test --locked stateful_endurance_replay --lib -- --ignored --nocapture",
            self.replay_steps
        )
    }

    fn failure_context(
        &self,
        seed: u64,
        step: usize,
        event: &Event,
        snapshot: &SemanticSnapshot,
    ) -> String {
        format!(
            "seed={seed:#x}; step={step}; event={event:?}; state={}; replay={}",
            snapshot.digest(),
            self.replay_command(seed)
        )
    }

    fn is_quiescent(&self) -> bool {
        let snapshot = self.snapshot();
        self.pending_indexes.is_empty()
            && self.pending_searches.is_empty()
            && self.pending_previews.is_empty()
            && self.pending_actions.is_empty()
            && self.pending_sorts.is_empty()
            && self.pending_filelists.is_empty()
            && self.app.shell.indexing.pending_queue.is_empty()
            && self.app.shell.indexing.inflight_requests.is_empty()
            && self.app.shell.indexing.pending_request_id.is_none()
            && !self.app.shell.indexing.in_progress
            && self.app.shell.indexing.pending_entries.is_empty()
            && self.app.shell.indexing.pending_finish.is_none()
            && self.app.shell.search.pending_request_id().is_none()
            && !self.app.shell.search.in_progress()
            && snapshot.routed_tab_ids.is_empty()
            && !snapshot.preview_pending
            && !snapshot.action_pending
            && !snapshot.sort_pending
            && !snapshot.filelist_pending
            && snapshot.tabs.iter().all(|tab| {
                !tab.index_pending
                    && !tab.search_pending
                    && !tab.preview_pending
                    && !tab.action_pending
                    && !tab.sort_pending
            })
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
        .chain(app.shell.features.filelist.workflow.pending_request_tab_id)
        .collect::<Vec<_>>();
    routed_tab_ids.sort_unstable();

    let root_index = |candidate: &PathBuf| {
        roots
            .iter()
            .position(|root| path_key(root) == path_key(candidate))
            .unwrap_or(usize::MAX)
    };
    let active_tab = app.shell.tabs.active_tab_index();
    let active_root = root_index(&app.shell.runtime.root);
    let results_digest = |results: &[(PathBuf, f64)]| {
        let mut hasher = DefaultHasher::new();
        for (path, score) in results {
            path.hash(&mut hasher);
            score.to_bits().hash(&mut hasher);
        }
        hasher.finish()
    };
    let tabs = app
        .shell
        .tabs
        .iter()
        .enumerate()
        .map(|(index, tab)| {
            if index == active_tab {
                TabSemanticSnapshot {
                    id: tab.id,
                    root: active_root,
                    query: app.shell.runtime.query_state.query.clone(),
                    results_len: app.shell.runtime.results.len(),
                    total_match_count: app.shell.runtime.total_match_count,
                    current_row: app.shell.runtime.current_row,
                    results_digest: results_digest(&app.shell.runtime.results),
                    notice: app.shell.runtime.notice.clone(),
                    index_pending: app.shell.indexing.pending_request_id.is_some()
                        || app.shell.indexing.in_progress,
                    search_pending: app.shell.search.pending_request_id().is_some()
                        || app.shell.search.in_progress(),
                    preview_pending: app.shell.worker_bus.preview.pending_request_id.is_some()
                        || app.shell.worker_bus.preview.in_progress,
                    action_pending: app.shell.worker_bus.action.pending_request_id.is_some()
                        || app.shell.worker_bus.action.in_progress,
                    sort_pending: app.shell.worker_bus.sort.pending_request_id.is_some()
                        || app.shell.worker_bus.sort.in_progress,
                }
            } else {
                TabSemanticSnapshot {
                    id: tab.id,
                    root: root_index(&tab.root),
                    query: tab.query_state.query.clone(),
                    results_len: tab.result_state.results.len(),
                    total_match_count: tab.result_state.total_match_count,
                    current_row: tab.result_state.current_row,
                    results_digest: results_digest(&tab.result_state.results),
                    notice: tab.notice.clone(),
                    index_pending: tab.index_state.pending_index_request_id.is_some()
                        || tab.index_state.index_in_progress,
                    search_pending: tab.pending_request_id.is_some() || tab.search_in_progress,
                    preview_pending: tab.pending_preview_request_id.is_some()
                        || tab.preview_in_progress,
                    action_pending: tab.pending_action_request_id.is_some()
                        || tab.action_in_progress,
                    sort_pending: tab.result_state.pending_sort_request_id.is_some()
                        || tab.result_state.sort_in_progress,
                }
            }
        })
        .collect();

    SemanticSnapshot {
        tab_ids,
        tabs,
        active_tab,
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
        preview_pending: app.shell.worker_bus.preview.pending_request_id.is_some()
            || app.shell.worker_bus.preview.in_progress,
        action_pending: app.shell.worker_bus.action.pending_request_id.is_some()
            || app.shell.worker_bus.action.in_progress,
        sort_pending: app.shell.worker_bus.sort.pending_request_id.is_some()
            || app.shell.worker_bus.sort.in_progress,
        filelist_pending: app
            .shell
            .features
            .filelist
            .workflow
            .pending_request_id
            .is_some()
            || app
                .shell
                .features
                .filelist
                .workflow
                .pending_request_tab_id
                .is_some()
            || app.shell.features.filelist.workflow.pending_root.is_some()
            || app
                .shell
                .features
                .filelist
                .workflow
                .pending_cancel
                .is_some()
            || app.shell.features.filelist.workflow.cancel_requested
            || app
                .shell
                .features
                .filelist
                .workflow
                .pending_after_index
                .is_some()
            || app.shell.features.filelist.workflow.in_progress,
    }
}

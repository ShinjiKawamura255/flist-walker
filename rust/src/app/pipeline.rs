use super::{
    AppTabState, Entry, FlistWalkerApp, IndexCoordinator, IndexEntry, IndexRequest, IndexResponse,
    IndexSource, PendingActiveIndexFinish, PipelineOwner, ResultSortMode,
};
use crate::app::index_coordinator::IndexResponseRoute;
use crate::app::tabs::BackgroundIndexResponseEffect;
use crate::path_utils::path_key;
use crate::walker_runtime::walker_truncated_notice;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

const MAX_PENDING_INDEX_ENTRIES: usize = 32_768;

impl FlistWalkerApp {
    fn pipeline_owner(&mut self) -> PipelineOwner<'_> {
        PipelineOwner::new(self)
    }

    fn cancel_stale_pending_after_index_for_active_root(&mut self) {
        let current_tab_id = self.current_tab_id().unwrap_or_default();
        if self
            .shell
            .features
            .filelist
            .workflow
            .pending_after_index
            .as_ref()
            .is_some_and(|pending| {
                pending.tab_id == current_tab_id
                    && path_key(&pending.root) != path_key(&self.shell.runtime.root)
            })
        {
            self.shell.features.filelist.workflow.pending_after_index = None;
            self.set_notice("Deferred Create File List canceled because root changed");
        }
    }

    fn reset_active_index_refresh_state(&mut self, reset_kind_resolution: bool) {
        self.shell.runtime.index.entries.clear();
        self.shell.runtime.index.source = IndexSource::None;
        self.clear_preview_cache();
        self.clear_highlight_cache();
        self.shell.cache.entry_kind.clear();
        self.shell.indexing.resolved_kind_updates.clear();
        self.shell.indexing.incremental_filtered_entries.clear();
        self.shell.indexing.pending_entries.clear();
        self.shell.indexing.pending_entries_request_id = None;
        self.shell.indexing.pending_finish = None;
        if reset_kind_resolution {
            self.reset_kind_resolution_state();
        } else {
            self.shell.indexing.pending_kind_paths.clear();
            self.shell.indexing.pending_kind_paths_set.clear();
            self.shell.indexing.in_flight_kind_paths.clear();
            self.shell.indexing.resolved_kind_updates.clear();
            self.shell.indexing.kind_resolution_in_progress = false;
            self.shell.indexing.kind_resolution_epoch =
                self.shell.indexing.kind_resolution_epoch.saturating_add(1);
        }
        self.shell.worker_bus.preview.pending_request_id = None;
        self.shell.worker_bus.preview.in_progress = false;
        self.shell.indexing.last_incremental_results_refresh = Instant::now();
        self.shell.indexing.last_search_snapshot_len = 0;
    }

    fn prepare_active_index_refresh_request(
        &mut self,
        request_id: u64,
        reset_kind_resolution: bool,
    ) {
        let query_non_empty = !self.shell.runtime.query_state.query.trim().is_empty();
        self.shell
            .indexing
            .begin_active_refresh(request_id, query_non_empty);
        self.shell.search.set_pending_request_id(None);
        self.shell.search.set_in_progress(false);
        self.reset_active_index_refresh_state(reset_kind_resolution);
    }

    pub(super) fn request_index_refresh(&mut self) {
        self.ensure_entry_filters();
        self.invalidate_result_sort(true);
        self.clear_sort_metadata_cache();
        self.clear_pending_activation_refresh();
        self.cancel_stale_pending_filelist_confirmations_for_active_root();
        self.cancel_stale_pending_after_index_for_active_root();
        let tab_id = self.current_tab_id();
        let request_id = self.shell.indexing.allocate_request_id(tab_id);
        self.prepare_active_index_refresh_request(request_id, false);
        self.refresh_status_line();

        let req = IndexRequest {
            request_id,
            tab_id: tab_id.unwrap_or_default(),
            root: self.shell.runtime.root.clone(),
            use_filelist: self.shell.runtime.use_filelist,
            include_files: self.shell.runtime.include_files,
            include_dirs: self.shell.runtime.include_dirs,
            max_depth: self.shell.runtime.max_depth,
        };
        self.enqueue_index_request(req);
        self.dispatch_index_queue();
    }

    pub(super) fn request_create_filelist_walker_refresh(&mut self) {
        self.cancel_stale_pending_filelist_confirmations_for_active_root();
        self.cancel_stale_pending_after_index_for_active_root();
        let tab_id = self.current_tab_id();
        let request_id = self.shell.indexing.allocate_request_id(tab_id);
        self.prepare_active_index_refresh_request(request_id, true);
        self.refresh_status_line();

        let req = IndexRequest {
            request_id,
            tab_id: tab_id.unwrap_or_default(),
            root: self.shell.runtime.root.clone(),
            use_filelist: false,
            include_files: self.shell.runtime.include_files,
            include_dirs: self.shell.runtime.include_dirs,
            max_depth: crate::indexer::MaxDepth::unlimited(),
        };
        self.enqueue_index_request(req);
        self.dispatch_index_queue();
    }

    pub(super) fn request_background_index_refresh_for_tab(&mut self, tab_index: usize) {
        let shell = &mut self.shell;
        let (tabs, indexing) = (&mut shell.tabs, &mut shell.indexing);
        let Some(tab_id) = tabs.get(tab_index).map(|tab| tab.id) else {
            return;
        };
        let request_id = indexing.allocate_request_id(Some(tab_id));

        let Some(tab) = tabs.get_mut(tab_index) else {
            indexing.request_tabs.remove(&request_id);
            return;
        };
        indexing.begin_background_refresh(tab, request_id, "Refreshing from created FileList");

        let req = IndexRequest {
            request_id,
            tab_id,
            root: tab.root.clone(),
            use_filelist: tab.use_filelist,
            include_files: tab.include_files,
            include_dirs: tab.include_dirs,
            max_depth: tab.max_depth,
        };
        self.enqueue_index_request(req);
        self.dispatch_index_queue();
    }

    fn clear_tab_index_request_state(tab: &mut AppTabState) {
        tab.index_state.pending_index_request_id = None;
        tab.index_state.index_in_progress = false;
        tab.index_state.pending_index_entries.clear();
        tab.index_state.pending_index_entries_request_id = None;
        tab.index_state.search_resume_pending = false;
        tab.index_state.search_rerun_pending = false;
    }

    fn handle_index_worker_unavailable(&mut self) {
        let notice = "Index worker is unavailable".to_string();
        {
            let shell = &mut self.shell;
            let (tabs, indexing, features) =
                (&mut shell.tabs, &mut shell.indexing, &mut shell.features);
            let affected_tab_ids: HashSet<u64> = indexing.request_tabs.values().copied().collect();

            features.filelist.workflow.pending_after_index = None;
            features
                .filelist
                .workflow
                .pending_index_completion_notices
                .clear();
            indexing.pending_queue.clear();
            indexing.background_states.clear();
            indexing.inflight_requests.clear();
            indexing.request_tabs.clear();

            indexing.clear_active_request_state(tabs);

            for tab in tabs {
                if affected_tab_ids.contains(&tab.id)
                    || tab.index_state.pending_index_request_id.is_some()
                {
                    Self::clear_tab_index_request_state(tab);
                    tab.notice = notice.clone();
                }
            }
        }
        self.set_notice(notice.clone());
    }

    pub(super) fn maybe_reindex_from_filter_toggles(
        &mut self,
        use_filelist_changed: bool,
        files_changed: bool,
        dirs_changed: bool,
    ) {
        let mut reindex = use_filelist_changed;
        reindex |= files_changed || dirs_changed;
        if self.use_filelist_requires_locked_filters()
            && (!self.shell.runtime.include_files || !self.shell.runtime.include_dirs)
        {
            self.shell.runtime.include_files = true;
            self.shell.runtime.include_dirs = true;
            reindex = true;
        }
        reindex |= self.ensure_entry_filters();
        if reindex {
            self.request_index_refresh();
        }
    }

    pub(super) fn enqueue_index_request(&mut self, req: IndexRequest) {
        self.shell
            .features
            .filelist
            .workflow
            .pending_index_completion_notices
            .retain(|_, pending| pending.tab_id != req.tab_id);
        let active_tab_id = self.current_tab_id().unwrap_or_default();
        // Requests accepted by the worker channel remain physically queued or running until
        // their terminal response arrives. Keep them in coordinator accounting even after a
        // replacement becomes latest; otherwise later active-tab work can be accepted behind
        // an invisible FIFO backlog and appear not to start for seconds.
        let stale_queued = self
            .shell
            .indexing
            .pending_queue
            .iter()
            .filter_map(|queued| (queued.tab_id == req.tab_id).then_some(queued.request_id))
            .collect::<Vec<_>>();
        self.shell
            .indexing
            .pending_queue
            .retain(|queued| queued.tab_id != req.tab_id);
        for request_id in stale_queued {
            self.discard_filelist_index_completion_notice(request_id);
            self.shell.indexing.cleanup_request(request_id);
        }
        self.shell.indexing.pending_queue.push_back(req);

        while self.shell.indexing.pending_queue.len() > Self::INDEX_MAX_QUEUE {
            let drop_idx = self
                .shell
                .indexing
                .pending_queue
                .iter()
                .position(|queued| queued.tab_id != active_tab_id)
                .unwrap_or(0);
            let dropped = self.shell.indexing.pending_queue.remove(drop_idx);
            if let Some(dropped) = dropped {
                self.discard_filelist_index_completion_notice(dropped.request_id);
                let dropped_is_latest = self
                    .shell
                    .indexing
                    .latest_request_ids
                    .lock()
                    .map(|latest| latest.get(&dropped.tab_id).copied() == Some(dropped.request_id))
                    .unwrap_or(true);
                let has_queued_replacement = self
                    .shell
                    .indexing
                    .pending_queue
                    .iter()
                    .any(|queued| queued.tab_id == dropped.tab_id);
                if let Some(tab_index) = self.find_tab_index_by_id(dropped.tab_id) {
                    if let Some(tab) = self.shell.tabs.get_mut(tab_index) {
                        if tab.index_state.pending_index_request_id == Some(dropped.request_id) {
                            tab.index_state.pending_index_request_id = None;
                            tab.index_state.index_in_progress = false;
                            tab.notice = "Index request dropped due to queue limit".to_string();
                        }
                    }
                }
                if dropped.tab_id != active_tab_id && dropped_is_latest && !has_queued_replacement {
                    self.mark_pending_activation_refresh_for_tab(dropped.tab_id);
                }
                self.shell.indexing.request_tabs.remove(&dropped.request_id);
                self.shell
                    .indexing
                    .background_states
                    .remove(&dropped.request_id);
            }
        }
    }

    pub(super) fn queued_request_for_tab_exists(&self, tab_id: u64) -> bool {
        self.shell
            .indexing
            .pending_queue
            .iter()
            .any(|req| req.tab_id == tab_id)
    }

    pub(super) fn has_inflight_for_tab(&self, tab_id: u64) -> bool {
        self.shell
            .indexing
            .inflight_requests
            .iter()
            .any(|request_id| {
                self.shell
                    .indexing
                    .request_tabs
                    .get(request_id)
                    .is_some_and(|rid_tab_id| *rid_tab_id == tab_id)
            })
    }

    pub(super) fn pop_next_index_request(&mut self) -> Option<IndexRequest> {
        let active_tab_id = self.current_tab_id()?;
        if let Some(pos) =
            self.shell.indexing.pending_queue.iter().position(|req| {
                req.tab_id == active_tab_id && !self.has_inflight_for_tab(req.tab_id)
            })
        {
            return self.shell.indexing.pending_queue.remove(pos);
        }
        if let Some(pos) = self
            .shell
            .indexing
            .pending_queue
            .iter()
            .position(|req| !self.has_inflight_for_tab(req.tab_id))
        {
            return self.shell.indexing.pending_queue.remove(pos);
        }
        None
    }

    pub(super) fn preempt_background_for_active_request(&mut self) -> bool {
        let Some(active_tab_id) = self.current_tab_id() else {
            return false;
        };
        if !self.queued_request_for_tab_exists(active_tab_id) {
            return false;
        }
        if self.shell.indexing.inflight_requests.len() < Self::INDEX_MAX_CONCURRENT {
            return false;
        }

        let victims: Vec<(u64, u64)> = self
            .shell
            .indexing
            .inflight_requests
            .iter()
            .filter_map(|request_id| {
                let tab_id = self.shell.indexing.request_tabs.get(request_id).copied()?;
                if tab_id == active_tab_id {
                    return None;
                }
                let replacement_request_id = self
                    .shell
                    .indexing
                    .pending_queue
                    .iter()
                    .rev()
                    .find(|req| req.tab_id == tab_id)
                    .map(|req| req.request_id)
                    .unwrap_or(0);
                Some((tab_id, replacement_request_id))
            })
            .collect();

        let Ok(mut latest) = self.shell.indexing.latest_request_ids.lock() else {
            return false;
        };
        let mut preempted = false;
        let mut activation_refresh_tabs = Vec::new();
        for (tab_id, replacement_request_id) in victims {
            if latest.get(&tab_id).copied() == Some(replacement_request_id) {
                continue;
            }
            latest.insert(tab_id, replacement_request_id);
            if replacement_request_id == 0 {
                activation_refresh_tabs.push(tab_id);
            }
            preempted = true;
        }
        drop(latest);
        for tab_id in activation_refresh_tabs {
            self.mark_pending_activation_refresh_for_tab(tab_id);
        }
        preempted
    }

    pub(super) fn dispatch_index_queue(&mut self) {
        loop {
            if self.shell.indexing.inflight_requests.len() >= Self::INDEX_MAX_CONCURRENT {
                let _ = self.preempt_background_for_active_request();
                break;
            }
            let Some(req) = self.pop_next_index_request() else {
                break;
            };
            let req_id = req.request_id;
            let req_tab_id = req.tab_id;
            match self.shell.indexing.tx.try_send(req) {
                Ok(()) => {
                    super::worker::channel::trace_worker_load(
                        &self.shell.indexing.tx,
                        "index",
                        "accepted",
                        super::worker::channel::WorkerTraceContext {
                            worker_id: "ui-dispatch",
                            request_id: Some(req_id),
                            tab_id: Some(req_tab_id),
                            epoch: None,
                            outcome: "accepted",
                        },
                    );
                    self.shell.indexing.inflight_requests.insert(req_id);
                }
                Err(std::sync::mpsc::TrySendError::Full(req)) => {
                    super::worker::channel::trace_worker_load(
                        &self.shell.indexing.tx,
                        "index",
                        "full",
                        super::worker::channel::WorkerTraceContext {
                            worker_id: "ui-dispatch",
                            request_id: Some(req_id),
                            tab_id: Some(req_tab_id),
                            epoch: None,
                            outcome: "full",
                        },
                    );
                    self.shell.indexing.pending_queue.push_front(req);
                    break;
                }
                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                    super::worker::channel::trace_worker_load(
                        &self.shell.indexing.tx,
                        "index",
                        "disconnected",
                        super::worker::channel::WorkerTraceContext {
                            worker_id: "ui-dispatch",
                            request_id: Some(req_id),
                            tab_id: Some(req_tab_id),
                            epoch: None,
                            outcome: "disconnected",
                        },
                    );
                    self.handle_index_worker_unavailable();
                    break;
                }
            }
        }
    }

    fn enqueue_search_request_for_tab_index(&mut self, tab_index: usize) {
        self.pipeline_owner()
            .enqueue_search_request_for_tab_index(tab_index);
    }

    fn handle_background_index_response(&mut self, tab_index: usize, msg: IndexResponse) {
        let request_id = IndexCoordinator::response_request_id(&msg);
        let completion_notice_can_apply = self
            .shell
            .tabs
            .get(tab_index)
            .is_some_and(|tab| tab.index_state.pending_index_request_id == Some(request_id));
        let restore_completion_notice = matches!(
            &msg,
            IndexResponse::Finished { .. } | IndexResponse::Canceled { .. }
        );
        let terminal = IndexCoordinator::is_terminal_response(&msg);
        let BackgroundIndexResponseEffect {
            trigger_search,
            cleanup_request_id,
            deferred_filelist,
        } = self.apply_background_index_response(tab_index, msg);

        if terminal {
            let owner = self
                .shell
                .tabs
                .get(tab_index)
                .map(|tab| (tab.id, tab.root.clone()));
            if completion_notice_can_apply && restore_completion_notice {
                if let Some((tab_id, root)) = owner {
                    if let Some(notice) =
                        self.take_filelist_index_completion_notice(request_id, tab_id, &root)
                    {
                        if let Some(tab) = self.shell.tabs.get_mut(tab_index) {
                            tab.notice = notice;
                        }
                    }
                }
            } else {
                self.discard_filelist_index_completion_notice(request_id);
            }
        }

        if let Some((tab_id, root, entries)) = deferred_filelist {
            self.request_filelist_creation(tab_id, root, entries);
        }
        if trigger_search {
            self.enqueue_search_request_for_tab_index(tab_index);
        }
        if let Some(request_id) = cleanup_request_id {
            self.shell.indexing.cleanup_request(request_id);
        }
        self.dispatch_index_queue();
    }

    pub(super) fn poll_index_response(&mut self) {
        self.poll_index_response_with_budget(Duration::from_millis(4));
    }

    #[cfg(test)]
    pub(super) fn poll_index_response_with_budget_for_test(&mut self, budget: Duration) {
        self.poll_index_response_with_budget(budget);
    }

    fn poll_index_response_with_budget(&mut self, frame_budget: Duration) {
        const MAX_MESSAGES_PER_FRAME: usize = 64;
        const MAX_PRIORITY_ROUTE_MESSAGES_PER_FRAME: usize = 4_096;
        // Large capped roots can leave hundreds of thousands of entries queued at
        // the terminal point. While the worker is still indexing, allow larger
        // chunks; after Finished, prioritize input responsiveness over tail speed.
        const MAX_INDEX_ENTRIES_PER_FRAME: usize = 32_768;
        const MAX_POST_FINISH_INDEX_ENTRIES_PER_FRAME: usize = 2_048;

        let frame_start = Instant::now();
        let mut processed = 0usize;
        let mut priority_routed = 0usize;
        let mut has_index_progress = false;
        let mut finished_current_request = false;
        loop {
            let (msg, from_shared_response_queue) =
                if let Some(msg) = self.shell.indexing.deferred_response.take() {
                    (msg, false)
                } else if let Ok(msg) = self.shell.indexing.rx.try_recv() {
                    (msg, true)
                } else if let Some(msg) = self
                    .shell
                    .indexing
                    .deferred_non_active_responses
                    .pop_front()
                {
                    (msg, false)
                } else {
                    break;
                };
            let request_id = IndexCoordinator::response_request_id(&msg);
            let route = self.shell.indexing.route_response(request_id);
            let stale_payload = matches!(route, IndexResponseRoute::Stale)
                && matches!(
                    &msg,
                    IndexResponse::Batch { .. } | IndexResponse::ReplaceAll { .. }
                );
            if from_shared_response_queue
                && self.shell.indexing.pending_request_id.is_some()
                && (matches!(route, IndexResponseRoute::Background(_)) || stale_payload)
            {
                // Regression guard: an activated restored tab's first response must not
                // sit behind bulk batches already emitted for an older/background tab.
                // Route those messages by ownership only; apply or discard them later
                // under the normal frame budget without copying their entry payloads.
                self.shell
                    .indexing
                    .deferred_non_active_responses
                    .push_back(msg);
                priority_routed = priority_routed.saturating_add(1);
                if priority_routed >= MAX_PRIORITY_ROUTE_MESSAGES_PER_FRAME
                    || frame_start.elapsed() >= frame_budget
                {
                    break;
                }
                continue;
            }
            // The response channel already owns complete batches. Do not copy another active
            // batch into the UI-owned VecDeque while its existing backlog is at the frame-sized
            // high-water mark: growing a very large VecDeque can relocate every queued PathBuf
            // in one uninterruptible allocation and defeat the wall-clock budget below. Hold one
            // batch by ownership so control/terminal messages still retain their normal path.
            if matches!(route, IndexResponseRoute::Active)
                && matches!(
                    &msg,
                    IndexResponse::Batch { .. } | IndexResponse::ReplaceAll { .. }
                )
                && self.shell.indexing.pending_entries_request_id == Some(request_id)
                && self.shell.indexing.pending_entries.len() >= MAX_PENDING_INDEX_ENTRIES
            {
                self.shell.indexing.deferred_response = Some(msg);
                break;
            }
            match route {
                IndexResponseRoute::Background(tab_id) => {
                    if let Some(tab_index) = self.find_tab_index_by_id(tab_id) {
                        self.handle_background_index_response(tab_index, msg);
                    } else {
                        self.discard_filelist_index_completion_notice(request_id);
                        self.shell.indexing.cleanup_request(request_id);
                    }
                    processed = processed.saturating_add(1);
                    if processed >= MAX_MESSAGES_PER_FRAME || frame_start.elapsed() >= frame_budget
                    {
                        break;
                    }
                    continue;
                }
                IndexResponseRoute::Stale => {
                    self.discard_filelist_index_completion_notice(request_id);
                    if IndexCoordinator::is_terminal_response(&msg) {
                        let stale_tab_id =
                            self.shell.indexing.request_tabs.get(&request_id).copied();
                        if let Some(tab_index) =
                            stale_tab_id.and_then(|tab_id| self.find_tab_index_by_id(tab_id))
                        {
                            if let Some(tab) = self.shell.tabs.get_mut(tab_index) {
                                if tab.index_state.pending_index_request_id == Some(request_id) {
                                    // A preempted background request remains owned by its tab
                                    // until the terminal response arrives. Settle that ownership
                                    // without applying superseded entries or waking the tab.
                                    Self::clear_tab_index_request_state(tab);
                                }
                            }
                        }
                        self.shell
                            .indexing
                            .cleanup_stale_terminal_response(request_id);
                    }
                    processed = processed.saturating_add(1);
                    if processed >= MAX_MESSAGES_PER_FRAME || frame_start.elapsed() >= frame_budget
                    {
                        break;
                    }
                    continue;
                }
                IndexResponseRoute::Active => {}
            }

            match msg {
                IndexResponse::Started { source, .. } => {
                    self.shell.runtime.index.source = source;
                    self.refresh_status_line();
                }
                IndexResponse::Batch {
                    request_id,
                    entries,
                } => {
                    self.queue_index_batch(request_id, entries);
                    has_index_progress = true;
                }
                IndexResponse::ReplaceAll {
                    request_id,
                    entries,
                } => {
                    self.shell.indexing.pending_entries.clear();
                    self.shell.indexing.pending_entries_request_id = None;
                    self.shell.runtime.index.entries.clear();
                    self.shell.indexing.incremental_filtered_entries.clear();
                    self.queue_index_batch(request_id, entries);
                    has_index_progress = true;
                }
                IndexResponse::Finished { request_id, source } => {
                    self.shell.indexing.pending_finish =
                        Some(PendingActiveIndexFinish { request_id, source });
                    self.shell.indexing.in_progress = false;
                    break;
                }
                IndexResponse::Failed { request_id, error } => {
                    self.shell.features.filelist.workflow.pending_after_index = None;
                    self.discard_filelist_index_completion_notice(request_id);
                    self.set_notice(format!("Indexing failed: {}", error));
                    self.shell.indexing.complete_active_request(request_id);
                }
                IndexResponse::Canceled { request_id } => {
                    if let Some((tab_id, root)) = self
                        .current_tab_id()
                        .map(|tab_id| (tab_id, self.shell.runtime.root.clone()))
                    {
                        if let Some(notice) =
                            self.take_filelist_index_completion_notice(request_id, tab_id, &root)
                        {
                            self.set_notice(notice);
                        }
                    }
                    self.shell.indexing.complete_active_request(request_id);
                }
                IndexResponse::Truncated { limit, .. } => {
                    self.set_notice(walker_truncated_notice(limit));
                }
            }

            processed = processed.saturating_add(1);
            if processed >= MAX_MESSAGES_PER_FRAME || frame_start.elapsed() >= frame_budget {
                break;
            }
        }

        if let Some(request_id) = self.shell.indexing.pending_request_id {
            let remaining_budget = frame_budget.saturating_sub(frame_start.elapsed());
            let consumed = if remaining_budget.is_zero() {
                self.drain_queued_index_entries(request_id, 32)
            } else {
                let max_entries = if self.shell.indexing.pending_finish.is_some() {
                    MAX_POST_FINISH_INDEX_ENTRIES_PER_FRAME
                } else {
                    MAX_INDEX_ENTRIES_PER_FRAME
                };
                self.drain_queued_index_entries_with_budget(
                    request_id,
                    Instant::now(),
                    remaining_budget,
                    max_entries,
                )
            };
            has_index_progress |= consumed;
        }

        if !finished_current_request {
            finished_current_request = self.try_finish_active_index_after_pending_drain();
        }

        if finished_current_request {
            self.dispatch_index_queue();
            return;
        }

        if self.shell.indexing.pending_finish.is_some() {
            self.dispatch_index_queue();
            return;
        }

        if !has_index_progress {
            self.dispatch_index_queue();
            return;
        }

        if self.shell.runtime.query_state.query.trim().is_empty() {
            self.apply_incremental_empty_query_results();
        } else {
            self.maybe_refresh_incremental_search();
        }
        self.dispatch_index_queue();
    }

    fn ensure_entry_filters(&mut self) -> bool {
        if !self.shell.runtime.include_files && !self.shell.runtime.include_dirs {
            self.shell.runtime.include_files = true;
            return true;
        }
        false
    }

    pub(super) fn apply_results_with_selection_policy(
        &mut self,
        results: Vec<(PathBuf, f64)>,
        keep_scroll_position: bool,
        preserve_selected_path: bool,
    ) {
        self.pipeline_owner().apply_results_with_selection_policy(
            results,
            keep_scroll_position,
            preserve_selected_path,
        );
    }

    pub(super) fn enqueue_search_request(&mut self) {
        self.pipeline_owner().enqueue_search_request();
    }

    pub(super) fn poll_search_response(&mut self) {
        self.pipeline_owner().poll_search_response();
    }

    pub(super) fn update_results(&mut self) {
        self.pipeline_owner().update_results();
    }

    fn queue_index_batch(&mut self, request_id: u64, entries: Vec<IndexEntry>) {
        if self.shell.indexing.pending_entries_request_id != Some(request_id) {
            self.shell.indexing.pending_entries.clear();
            self.shell.indexing.pending_entries_request_id = Some(request_id);
        }
        self.shell.indexing.pending_entries.extend(entries);
    }

    fn ingest_index_entry(&mut self, entry: IndexEntry) {
        let entry: Entry = entry.into();
        if let Some(kind) = entry.kind {
            self.shell.cache.entry_kind.set(entry.path.clone(), kind);
        }
        if entry.kind.is_none_or(|kind| kind.needs_resolution())
            && self.kind_resolution_needed_for_filters()
        {
            self.queue_kind_resolution(entry.path.clone());
        }
        let compiled_ignore_terms = self.compiled_ignore_terms();
        if self.should_track_incremental_filtered_entries()
            && self.is_entry_visible_for_current_filter(&entry, compiled_ignore_terms.as_deref())
        {
            self.shell
                .indexing
                .incremental_filtered_entries
                .push(entry.clone());
        }
        self.shell.runtime.index.entries.push(entry);
    }

    pub(super) fn drain_queued_index_entries(
        &mut self,
        request_id: u64,
        max_entries: usize,
    ) -> bool {
        if self.shell.indexing.pending_entries_request_id != Some(request_id) {
            return false;
        }
        let mut processed = 0usize;
        while processed < max_entries {
            let Some(entry) = self.shell.indexing.pending_entries.pop_front() else {
                break;
            };
            self.ingest_index_entry(entry);
            processed = processed.saturating_add(1);
        }
        if self.shell.indexing.pending_entries.is_empty() {
            self.shell.indexing.pending_entries_request_id = None;
        }
        processed > 0
    }

    fn drain_queued_index_entries_with_budget(
        &mut self,
        request_id: u64,
        frame_start: Instant,
        budget: Duration,
        max_entries: usize,
    ) -> bool {
        if self.shell.indexing.pending_entries_request_id != Some(request_id) {
            return false;
        }
        let mut processed = 0usize;
        while processed < max_entries && frame_start.elapsed() < budget {
            let Some(entry) = self.shell.indexing.pending_entries.pop_front() else {
                break;
            };
            self.ingest_index_entry(entry);
            processed = processed.saturating_add(1);
        }
        if self.shell.indexing.pending_entries.is_empty() {
            self.shell.indexing.pending_entries_request_id = None;
        }
        processed > 0
    }

    fn should_track_incremental_filtered_entries(&self) -> bool {
        !self.shell.runtime.query_state.query.trim().is_empty()
            || !self.shell.runtime.include_files
            || !self.shell.runtime.include_dirs
            || (self.shell.ui.ignore_list_enabled
                && !self.shell.runtime.ignore_list_terms.is_empty())
    }

    fn try_finish_active_index_after_pending_drain(&mut self) -> bool {
        let Some(pending_finish) = self.shell.indexing.pending_finish.clone() else {
            return false;
        };
        if self.shell.indexing.pending_entries_request_id == Some(pending_finish.request_id)
            && !self.shell.indexing.pending_entries.is_empty()
        {
            return false;
        }
        self.shell.indexing.pending_finish = None;
        self.finish_active_index_request(pending_finish);
        true
    }

    fn finish_active_index_request(&mut self, pending_finish: PendingActiveIndexFinish) {
        let request_id = pending_finish.request_id;
        self.shell.runtime.index.source = pending_finish.source;
        self.shell.runtime.all_entries =
            Arc::new(std::mem::take(&mut self.shell.runtime.index.entries));

        let needs_filtering = !self.shell.runtime.include_files
            || !self.shell.runtime.include_dirs
            || (self.shell.ui.ignore_list_enabled
                && !self.shell.runtime.ignore_list_terms.is_empty());
        let has_incremental_filter_snapshot = needs_filtering
            && (!self.shell.indexing.incremental_filtered_entries.is_empty()
                || !self.shell.runtime.index.entries.is_empty());
        self.shell.indexing.settle_active_terminal_state();
        if needs_filtering {
            if has_incremental_filter_snapshot {
                self.shell.runtime.entries = Arc::new(std::mem::take(
                    &mut self.shell.indexing.incremental_filtered_entries,
                ));
                self.shell.indexing.last_search_snapshot_len = self.shell.runtime.entries.len();
                self.shell.indexing.search_rerun_pending = false;
                if self.shell.runtime.query_state.query.trim().is_empty() {
                    self.shell.search.clear_active_request_state();
                    // The incremental snapshot replaces the previous result set at
                    // index completion, so its denominator must replace any count
                    // left by the search that was active before the refresh.
                    self.shell.runtime.total_match_count = self.shell.runtime.entries.len();
                    let results = self
                        .shell
                        .runtime
                        .entries
                        .iter()
                        .take(self.shell.runtime.limit)
                        .cloned()
                        .map(|entry| (entry.path, 0.0))
                        .collect();
                    self.replace_results_snapshot(results, true);
                } else {
                    self.update_results();
                }
            } else {
                self.apply_entry_filters(true);
            }
        } else {
            self.shell.runtime.entries = Arc::clone(&self.shell.runtime.all_entries);
            self.shell.indexing.incremental_filtered_entries.clear();
            self.shell.indexing.last_search_snapshot_len = self.shell.runtime.entries.len();
            self.shell.indexing.search_rerun_pending = false;
            if self.shell.runtime.query_state.query.trim().is_empty() {
                self.shell.search.clear_active_request_state();
                // The index refresh supersedes the previous result snapshot;
                // keep the Results denominator aligned with the final index.
                self.shell.runtime.total_match_count = self.shell.runtime.entries.len();
                let results = self
                    .shell
                    .runtime
                    .entries
                    .iter()
                    .take(self.shell.runtime.limit)
                    .cloned()
                    .map(|entry| (entry.path, 0.0))
                    .collect();
                self.replace_results_snapshot(results, true);
            } else {
                self.update_results();
            }
        }

        if self.shell.runtime.query_state.query.trim().is_empty()
            && self.shell.runtime.result_sort_mode != ResultSortMode::Score
            && !self.shell.search.in_progress()
            && !self.shell.worker_bus.sort.in_progress
        {
            // A closed-tab restore may restart interrupted index and result
            // work together. The final index snapshot supersedes the retained
            // snapshot, so reapply the preserved empty-query sort after the
            // replacement index settles.
            self.apply_result_sort(false);
        }

        if matches!(self.shell.runtime.index.source, IndexSource::Walker) {
            // Regression guard: keep Walker kind resolution bounded to what is
            // actually visible. Resolving the entire tree eagerly can keep the
            // process hot on huge on-demand roots even after search/index settles.
            self.queue_unknown_kind_paths_for_visible_results();
        } else {
            self.reset_kind_resolution_state();
        }
        let current_tab_id = self.current_tab_id().unwrap_or_default();
        let current_root = self.shell.runtime.root.clone();
        if let Some(notice) =
            self.take_filelist_index_completion_notice(request_id, current_tab_id, &current_root)
        {
            self.set_notice(notice);
        } else {
            self.clear_notice();
        }
        if self
            .shell
            .features
            .filelist
            .workflow
            .pending_after_index
            .as_ref()
            .is_some_and(|pending| {
                pending.tab_id == current_tab_id
                    && path_key(&pending.root) == path_key(&self.shell.runtime.root)
            })
        {
            let root = self.shell.runtime.root.clone();
            let entries = self.filelist_entries_snapshot();
            self.shell.features.filelist.workflow.pending_after_index = None;
            self.request_filelist_creation(current_tab_id, root, entries);
        }
        self.shell.indexing.complete_active_request(request_id);
    }

    fn take_filelist_index_completion_notice(
        &mut self,
        request_id: u64,
        tab_id: u64,
        root: &Path,
    ) -> Option<String> {
        self.shell
            .features
            .filelist
            .workflow
            .pending_index_completion_notices
            .remove(&request_id)
            .filter(|pending| pending.tab_id == tab_id && path_key(&pending.root) == path_key(root))
            .map(|pending| pending.notice)
    }

    fn discard_filelist_index_completion_notice(&mut self, request_id: u64) {
        self.shell
            .features
            .filelist
            .workflow
            .pending_index_completion_notices
            .remove(&request_id);
    }

    fn apply_incremental_empty_query_results(&mut self) {
        self.pipeline_owner()
            .apply_incremental_empty_query_results();
    }

    fn maybe_refresh_incremental_search(&mut self) {
        self.pipeline_owner().maybe_refresh_incremental_search();
    }

    pub(super) fn should_refresh_incremental_search(&self) -> bool {
        let current_len = self.shell.indexing.incremental_filtered_entries.len();
        let delta = current_len.saturating_sub(self.shell.indexing.last_search_snapshot_len);
        if delta == 0 {
            return false;
        }
        if self.shell.indexing.in_progress {
            if delta < Self::INCREMENTAL_SEARCH_MIN_DELTA_DURING_INDEX {
                return false;
            }
            return self
                .shell
                .indexing
                .last_incremental_results_refresh
                .elapsed()
                >= Self::INCREMENTAL_SEARCH_REFRESH_INTERVAL_DURING_INDEX;
        }
        self.shell
            .indexing
            .last_incremental_results_refresh
            .elapsed()
            >= Self::INCREMENTAL_SEARCH_REFRESH_INTERVAL
    }

    pub(super) fn apply_entry_filters(&mut self, keep_scroll_position: bool) {
        self.pipeline_owner()
            .apply_entry_filters(keep_scroll_position);
    }
}

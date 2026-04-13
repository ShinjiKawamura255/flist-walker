use super::*;
use crate::app::index_coordinator::IndexResponseRoute;
use crate::app::tabs::BackgroundIndexResponseEffect;
use crate::path_utils::path_key;

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
            .pending_after_index
            .as_ref()
            .is_some_and(|pending| {
                pending.tab_id == current_tab_id
                    && path_key(&pending.root) != path_key(&self.shell.runtime.root)
            })
        {
            self.shell.features.filelist.pending_after_index = None;
            self.set_notice("Deferred Create File List canceled because root changed");
        }
    }

    fn reset_active_index_refresh_state(&mut self, reset_kind_resolution: bool) {
        self.shell.runtime.index.entries.clear();
        self.shell.runtime.index.source = IndexSource::None;
        self.clear_preview_cache();
        self.clear_highlight_cache();
        self.shell.cache.entry_kind.clear();
        self.shell.indexing.incremental_filtered_entries.clear();
        self.shell.indexing.pending_entries.clear();
        self.shell.indexing.pending_entries_request_id = None;
        if reset_kind_resolution {
            self.reset_kind_resolution_state();
        } else {
            self.shell.indexing.pending_kind_paths.clear();
            self.shell.indexing.pending_kind_paths_set.clear();
            self.shell.indexing.in_flight_kind_paths.clear();
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
        mark_inflight: bool,
    ) {
        let query_non_empty = !self.shell.runtime.query_state.query.trim().is_empty();
        if mark_inflight {
            self.shell
                .indexing
                .begin_active_refresh_with_inflight(request_id, query_non_empty);
        } else {
            self.shell
                .indexing
                .begin_active_refresh(request_id, query_non_empty);
        }
        self.shell.search.set_pending_request_id(None);
        self.shell.search.set_in_progress(false);
        self.reset_active_index_refresh_state(reset_kind_resolution);
    }

    pub(super) fn request_index_refresh(&mut self) {
        self.ensure_entry_filters();
        self.invalidate_result_sort(true);
        self.clear_sort_metadata_cache();
        self.clear_pending_restore_refresh();
        self.cancel_stale_pending_filelist_confirmations_for_active_root();
        self.cancel_stale_pending_after_index_for_active_root();
        let tab_id = self.current_tab_id();
        let request_id = self.shell.indexing.allocate_request_id(tab_id);
        self.prepare_active_index_refresh_request(request_id, false, false);
        self.refresh_status_line();

        let req = IndexRequest {
            request_id,
            tab_id: tab_id.unwrap_or_default(),
            root: self.shell.runtime.root.clone(),
            use_filelist: self.shell.runtime.use_filelist,
            include_files: self.shell.runtime.include_files,
            include_dirs: self.shell.runtime.include_dirs,
        };
        self.enqueue_index_request(req);
        self.dispatch_index_queue();
    }

    pub(super) fn request_create_filelist_walker_refresh(&mut self) {
        self.cancel_stale_pending_filelist_confirmations_for_active_root();
        self.cancel_stale_pending_after_index_for_active_root();
        let tab_id = self.current_tab_id();
        let request_id = self.shell.indexing.allocate_request_id(tab_id);
        self.prepare_active_index_refresh_request(request_id, true, true);
        self.refresh_status_line();

        let req = IndexRequest {
            request_id,
            tab_id: tab_id.unwrap_or_default(),
            root: self.shell.runtime.root.clone(),
            use_filelist: false,
            include_files: self.shell.runtime.include_files,
            include_dirs: self.shell.runtime.include_dirs,
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

            features.filelist.pending_after_index = None;
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

    fn enqueue_index_request(&mut self, req: IndexRequest) {
        let active_tab_id = self.current_tab_id().unwrap_or_default();
        let stale_inflight: Vec<u64> = self
            .shell
            .indexing
            .inflight_requests
            .iter()
            .copied()
            .filter(|request_id| {
                self.shell
                    .indexing
                    .request_tabs
                    .get(request_id)
                    .is_some_and(|tab_id| *tab_id == req.tab_id)
            })
            .collect();
        for request_id in stale_inflight {
            self.shell.indexing.inflight_requests.remove(&request_id);
            self.shell.indexing.request_tabs.remove(&request_id);
            self.shell.indexing.background_states.remove(&request_id);
        }
        self.shell
            .indexing
            .pending_queue
            .retain(|queued| queued.tab_id != req.tab_id);
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
                if let Some(tab_index) = self.find_tab_index_by_id(dropped.tab_id) {
                    if let Some(tab) = self.shell.tabs.get_mut(tab_index) {
                        if tab.index_state.pending_index_request_id == Some(dropped.request_id) {
                            tab.index_state.pending_index_request_id = None;
                            tab.index_state.index_in_progress = false;
                            tab.notice = "Index request dropped due to queue limit".to_string();
                        }
                    }
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

        let victim_request_id =
            self.shell
                .indexing
                .inflight_requests
                .iter()
                .copied()
                .find(|request_id| {
                    self.shell
                        .indexing
                        .request_tabs
                        .get(request_id)
                        .is_some_and(|tab_id| *tab_id != active_tab_id)
                });
        let Some(victim_request_id) = victim_request_id else {
            return false;
        };
        let Some(victim_tab_id) = self
            .shell
            .indexing
            .request_tabs
            .get(&victim_request_id)
            .copied()
        else {
            return false;
        };
        let replacement_request_id = self
            .shell
            .indexing
            .pending_queue
            .iter()
            .rev()
            .find(|req| req.tab_id == victim_tab_id)
            .map(|req| req.request_id)
            .unwrap_or(0);

        if let Ok(mut latest) = self.shell.indexing.latest_request_ids.lock() {
            if latest.get(&victim_tab_id).copied() == Some(replacement_request_id) {
                return false;
            }
            latest.insert(victim_tab_id, replacement_request_id);
            return true;
        }
        false
    }

    fn dispatch_index_queue(&mut self) {
        loop {
            if self.shell.indexing.inflight_requests.len() >= Self::INDEX_MAX_CONCURRENT {
                let _ = self.preempt_background_for_active_request();
                break;
            }
            let Some(req) = self.pop_next_index_request() else {
                break;
            };
            let req_id = req.request_id;
            if self.shell.indexing.tx.send(req).is_err() {
                self.handle_index_worker_unavailable();
                break;
            } else {
                self.shell.indexing.inflight_requests.insert(req_id);
            }
        }
    }

    fn enqueue_search_request_for_tab_index(&mut self, tab_index: usize) {
        self.pipeline_owner()
            .enqueue_search_request_for_tab_index(tab_index);
    }

    fn handle_background_index_response(&mut self, tab_index: usize, msg: IndexResponse) {
        let BackgroundIndexResponseEffect {
            trigger_search,
            cleanup_request_id,
            deferred_filelist,
        } = self.apply_background_index_response(tab_index, msg);

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
        const MAX_MESSAGES_PER_FRAME: usize = 12;
        const FRAME_BUDGET: Duration = Duration::from_millis(4);
        const MAX_INDEX_ENTRIES_PER_FRAME: usize = 256;

        let frame_start = Instant::now();
        let mut processed = 0usize;
        let mut has_index_progress = false;
        let mut finished_current_request = false;
        while let Ok(msg) = self.shell.indexing.rx.try_recv() {
            let request_id = IndexCoordinator::response_request_id(&msg);
            match self.shell.indexing.route_response(request_id) {
                IndexResponseRoute::Background(tab_id) => {
                    if let Some(tab_index) = self.find_tab_index_by_id(tab_id) {
                        self.handle_background_index_response(tab_index, msg);
                    } else {
                        self.shell.indexing.cleanup_request(request_id);
                    }
                    processed = processed.saturating_add(1);
                    if processed >= MAX_MESSAGES_PER_FRAME || frame_start.elapsed() >= FRAME_BUDGET
                    {
                        break;
                    }
                    continue;
                }
                IndexResponseRoute::Stale => {
                    self.shell.indexing.cleanup_stale_terminal_response(request_id);
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
                    self.drain_queued_index_entries(request_id, usize::MAX);
                    self.shell.runtime.index.source = source;
                    self.shell.runtime.all_entries =
                        Arc::new(std::mem::take(&mut self.shell.runtime.index.entries));
                    self.shell.indexing.last_search_snapshot_len =
                        self.shell.runtime.all_entries.len();
                    self.shell.indexing.incremental_filtered_entries.clear();
                    self.shell.indexing.settle_active_terminal_state();
                    self.apply_entry_filters(true);
                    self.rebuild_entry_kind_cache();
                    if matches!(self.shell.runtime.index.source, IndexSource::Walker) {
                        self.queue_unknown_kind_paths_for_completed_walker_entries();
                    } else {
                        self.reset_kind_resolution_state();
                    }
                    self.clear_notice();
                    let current_tab_id = self.current_tab_id().unwrap_or_default();
                    if self
                        .shell
                        .features
                        .filelist
                        .pending_after_index
                        .as_ref()
                        .is_some_and(|pending| {
                            pending.tab_id == current_tab_id
                                && path_key(&pending.root) == path_key(&self.shell.runtime.root)
                        })
                    {
                        let root = self.shell.runtime.root.clone();
                        let entries = self.filelist_entries_snapshot();
                        self.shell.features.filelist.pending_after_index = None;
                        self.request_filelist_creation(current_tab_id, root, entries);
                    }
                    self.shrink_checkpoint_buffers();
                    self.shell.indexing.complete_active_request(request_id);
                    finished_current_request = true;
                    break;
                }
                IndexResponse::Failed { request_id, error } => {
                    self.shell.features.filelist.pending_after_index = None;
                    self.shrink_checkpoint_buffers();
                    self.set_notice(format!("Indexing failed: {}", error));
                    self.shell.indexing.complete_active_request(request_id);
                }
                IndexResponse::Canceled { request_id } => {
                    self.shrink_checkpoint_buffers();
                    self.shell.indexing.complete_active_request(request_id);
                }
                IndexResponse::Truncated { limit, .. } => {
                    self.set_notice(format!(
                        "Walker capped at {} entries (set FLISTWALKER_WALKER_MAX_ENTRIES to adjust)",
                        limit
                    ));
                }
            }

            processed = processed.saturating_add(1);
            if processed >= MAX_MESSAGES_PER_FRAME || frame_start.elapsed() >= FRAME_BUDGET {
                break;
            }
        }

        if finished_current_request {
            self.dispatch_index_queue();
            return;
        }

        if let Some(request_id) = self.shell.indexing.pending_request_id {
            let remaining_budget = FRAME_BUDGET.saturating_sub(frame_start.elapsed());
            let consumed = if remaining_budget.is_zero() {
                self.drain_queued_index_entries(request_id, 32)
            } else {
                self.drain_queued_index_entries_with_budget(
                    request_id,
                    Instant::now(),
                    remaining_budget,
                    MAX_INDEX_ENTRIES_PER_FRAME,
                )
            };
            has_index_progress |= consumed;
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

    #[cfg_attr(not(test), allow(dead_code))]
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
        if entry.kind.is_none() && self.kind_resolution_needed_for_filters() {
            self.queue_kind_resolution(entry.path.clone());
        }
        if self.is_entry_visible_for_current_filter(&entry) {
            self.shell
                .indexing
                .incremental_filtered_entries
                .push(entry.clone());
        }
        self.shell.runtime.index.entries.push(entry);
    }

    fn drain_queued_index_entries(&mut self, request_id: u64, max_entries: usize) -> bool {
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

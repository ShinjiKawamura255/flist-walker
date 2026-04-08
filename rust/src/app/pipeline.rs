use super::*;

impl FlistWalkerApp {
    fn cancel_stale_pending_after_index_for_active_root(&mut self) {
        let current_tab_id = self.current_tab_id().unwrap_or_default();
        if self
            .filelist_state
            .pending_after_index
            .as_ref()
            .is_some_and(|pending| {
                pending.tab_id == current_tab_id
                    && Self::path_key(&pending.root) != Self::path_key(&self.root)
            })
        {
            self.filelist_state.pending_after_index = None;
            self.set_notice("Deferred Create File List canceled because root changed");
        }
    }

    fn reset_active_index_refresh_state(&mut self, reset_kind_resolution: bool) {
        self.index.entries.clear();
        self.index.source = IndexSource::None;
        self.clear_preview_cache();
        self.clear_highlight_cache();
        self.cache.entry_kind.clear();
        self.indexing.incremental_filtered_entries.clear();
        self.indexing.pending_entries.clear();
        self.indexing.pending_entries_request_id = None;
        if reset_kind_resolution {
            self.reset_kind_resolution_state();
        } else {
            self.indexing.pending_kind_paths.clear();
            self.indexing.pending_kind_paths_set.clear();
            self.indexing.in_flight_kind_paths.clear();
            self.indexing.kind_resolution_in_progress = false;
            self.indexing.kind_resolution_epoch =
                self.indexing.kind_resolution_epoch.saturating_add(1);
        }
        self.worker_bus.preview.pending_request_id = None;
        self.worker_bus.preview.in_progress = false;
        self.indexing.last_incremental_results_refresh = Instant::now();
        self.indexing.last_search_snapshot_len = 0;
    }

    fn prepare_active_index_refresh_request(
        &mut self,
        request_id: u64,
        reset_kind_resolution: bool,
        mark_inflight: bool,
    ) {
        if mark_inflight {
            self.indexing.begin_active_refresh_with_inflight(
                request_id,
                !self.query_state.query.trim().is_empty(),
            );
        } else {
            self.indexing
                .begin_active_refresh(request_id, !self.query_state.query.trim().is_empty());
        }
        self.search.set_pending_request_id(None);
        self.search.set_in_progress(false);
        self.reset_active_index_refresh_state(reset_kind_resolution);
    }

    fn settle_background_index_failure(tab: &mut AppTabState, notice: Option<String>) {
        tab.index_state.index_in_progress = false;
        tab.index_state.pending_index_request_id = None;
        tab.index_state.search_resume_pending = false;
        tab.index_state.search_rerun_pending = false;
        tab.index_state.pending_index_entries.clear();
        tab.index_state.pending_index_entries_request_id = None;
        Self::shrink_tab_checkpoint_buffers(tab);
        if let Some(notice) = notice {
            tab.notice = notice;
        }
    }

    fn overwrite_entries_arc(target: &mut Arc<Vec<Entry>>, source: &[Entry]) {
        if let Some(entries) = Arc::get_mut(target) {
            entries.clear();
            entries.extend(source.iter().cloned());
        } else {
            *target = Arc::new(source.to_vec());
        }
    }

    fn overwrite_entries_vec(target: &mut Vec<Entry>, source: &[Entry]) {
        target.clear();
        target.extend(source.iter().cloned());
    }

    pub(super) fn request_index_refresh(&mut self) {
        self.ensure_entry_filters();
        self.invalidate_result_sort(true);
        self.clear_sort_metadata_cache();
        self.pending_restore_refresh = false;
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.pending_restore_refresh = false;
        }
        self.cancel_stale_pending_filelist_confirmation();
        self.cancel_stale_pending_filelist_ancestor_confirmation();
        self.cancel_stale_pending_filelist_use_walker_confirmation();
        self.cancel_stale_pending_after_index_for_active_root();
        let tab_id = self.current_tab_id();
        let request_id = self.indexing.allocate_request_id(tab_id);
        self.prepare_active_index_refresh_request(request_id, false, false);
        self.refresh_status_line();

        let req = IndexRequest {
            request_id,
            tab_id: tab_id.unwrap_or_default(),
            root: self.root.clone(),
            use_filelist: self.use_filelist,
            include_files: self.include_files,
            include_dirs: self.include_dirs,
        };
        self.enqueue_index_request(req);
        self.dispatch_index_queue();
    }

    pub(super) fn request_create_filelist_walker_refresh(&mut self) {
        self.cancel_stale_pending_filelist_confirmation();
        self.cancel_stale_pending_filelist_ancestor_confirmation();
        self.cancel_stale_pending_filelist_use_walker_confirmation();
        self.cancel_stale_pending_after_index_for_active_root();
        let tab_id = self.current_tab_id();
        let request_id = self.indexing.allocate_request_id(tab_id);
        self.prepare_active_index_refresh_request(request_id, true, true);
        self.refresh_status_line();

        let req = IndexRequest {
            request_id,
            tab_id: tab_id.unwrap_or_default(),
            root: self.root.clone(),
            use_filelist: false,
            include_files: self.include_files,
            include_dirs: self.include_dirs,
        };
        self.enqueue_index_request(req);
        self.dispatch_index_queue();
    }

    pub(super) fn request_background_index_refresh_for_tab(&mut self, tab_index: usize) {
        let Some(tab_id) = self.tabs.get(tab_index).map(|tab| tab.id) else {
            return;
        };
        let request_id = self.indexing.allocate_request_id(Some(tab_id));

        let Some(tab) = self.tabs.get_mut(tab_index) else {
            self.indexing.request_tabs.remove(&request_id);
            return;
        };
        self.indexing
            .begin_background_refresh(tab, request_id, "Refreshing from created FileList");

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

    fn clear_active_index_request_state(&mut self) {
        self.indexing.pending_request_id = None;
        self.indexing.in_progress = false;
        self.indexing.pending_entries.clear();
        self.indexing.pending_entries_request_id = None;
        self.indexing.search_resume_pending = false;
        self.indexing.search_rerun_pending = false;
        self.pending_restore_refresh = false;
    }

    fn clear_tab_index_request_state(tab: &mut AppTabState) {
        tab.index_state.pending_index_request_id = None;
        tab.index_state.index_in_progress = false;
        tab.index_state.pending_index_entries.clear();
        tab.index_state.pending_index_entries_request_id = None;
        tab.index_state.search_resume_pending = false;
        tab.index_state.search_rerun_pending = false;
        tab.pending_restore_refresh = false;
    }

    fn handle_index_worker_unavailable(&mut self) {
        let affected_tab_ids: HashSet<u64> = self.indexing.request_tabs.values().copied().collect();
        let notice = "Index worker is unavailable".to_string();

        self.filelist_state.pending_after_index = None;
        self.indexing.pending_queue.clear();
        self.indexing.background_states.clear();
        self.indexing.inflight_requests.clear();
        self.indexing.request_tabs.clear();

        self.clear_active_index_request_state();
        self.set_notice(notice.clone());

        for tab in &mut self.tabs {
            if affected_tab_ids.contains(&tab.id)
                || tab.index_state.pending_index_request_id.is_some()
            {
                Self::clear_tab_index_request_state(tab);
                tab.notice = notice.clone();
            }
        }
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
            && (!self.include_files || !self.include_dirs)
        {
            self.include_files = true;
            self.include_dirs = true;
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
            .indexing
            .inflight_requests
            .iter()
            .copied()
            .filter(|request_id| {
                self.indexing
                    .request_tabs
                    .get(request_id)
                    .is_some_and(|tab_id| *tab_id == req.tab_id)
            })
            .collect();
        for request_id in stale_inflight {
            self.indexing.inflight_requests.remove(&request_id);
            self.indexing.request_tabs.remove(&request_id);
            self.indexing.background_states.remove(&request_id);
        }
        self.indexing
            .pending_queue
            .retain(|queued| queued.tab_id != req.tab_id);
        self.indexing.pending_queue.push_back(req);

        while self.indexing.pending_queue.len() > Self::INDEX_MAX_QUEUE {
            let drop_idx = self
                .indexing
                .pending_queue
                .iter()
                .position(|queued| queued.tab_id != active_tab_id)
                .unwrap_or(0);
            let dropped = self.indexing.pending_queue.remove(drop_idx);
            if let Some(dropped) = dropped {
                if let Some(tab_index) = self.find_tab_index_by_id(dropped.tab_id) {
                    if let Some(tab) = self.tabs.get_mut(tab_index) {
                        if tab.index_state.pending_index_request_id == Some(dropped.request_id) {
                            tab.index_state.pending_index_request_id = None;
                            tab.index_state.index_in_progress = false;
                            tab.notice = "Index request dropped due to queue limit".to_string();
                        }
                    }
                }
                self.indexing.request_tabs.remove(&dropped.request_id);
                self.indexing.background_states.remove(&dropped.request_id);
            }
        }
    }

    pub(super) fn queued_request_for_tab_exists(&self, tab_id: u64) -> bool {
        self.indexing
            .pending_queue
            .iter()
            .any(|req| req.tab_id == tab_id)
    }

    pub(super) fn has_inflight_for_tab(&self, tab_id: u64) -> bool {
        self.indexing.inflight_requests.iter().any(|request_id| {
            self.indexing
                .request_tabs
                .get(request_id)
                .is_some_and(|rid_tab_id| *rid_tab_id == tab_id)
        })
    }

    pub(super) fn pop_next_index_request(&mut self) -> Option<IndexRequest> {
        let active_tab_id = self.current_tab_id()?;
        if let Some(pos) =
            self.indexing.pending_queue.iter().position(|req| {
                req.tab_id == active_tab_id && !self.has_inflight_for_tab(req.tab_id)
            })
        {
            return self.indexing.pending_queue.remove(pos);
        }
        if let Some(pos) = self
            .indexing
            .pending_queue
            .iter()
            .position(|req| !self.has_inflight_for_tab(req.tab_id))
        {
            return self.indexing.pending_queue.remove(pos);
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
        if self.indexing.inflight_requests.len() < Self::INDEX_MAX_CONCURRENT {
            return false;
        }

        let victim_request_id =
            self.indexing
                .inflight_requests
                .iter()
                .copied()
                .find(|request_id| {
                    self.indexing
                        .request_tabs
                        .get(request_id)
                        .is_some_and(|tab_id| *tab_id != active_tab_id)
                });
        let Some(victim_request_id) = victim_request_id else {
            return false;
        };
        let Some(victim_tab_id) = self.indexing.request_tabs.get(&victim_request_id).copied()
        else {
            return false;
        };
        let replacement_request_id = self
            .indexing
            .pending_queue
            .iter()
            .rev()
            .find(|req| req.tab_id == victim_tab_id)
            .map(|req| req.request_id)
            .unwrap_or(0);

        if let Ok(mut latest) = self.indexing.latest_request_ids.lock() {
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
            if self.indexing.inflight_requests.len() >= Self::INDEX_MAX_CONCURRENT {
                let _ = self.preempt_background_for_active_request();
                break;
            }
            let Some(req) = self.pop_next_index_request() else {
                break;
            };
            let req_id = req.request_id;
            if self.indexing.tx.send(req).is_err() {
                self.handle_index_worker_unavailable();
                break;
            } else {
                self.indexing.inflight_requests.insert(req_id);
            }
        }
    }

    fn enqueue_search_request_for_tab_index(&mut self, tab_index: usize) {
        let Some(tab) = self.tabs.get_mut(tab_index) else {
            return;
        };
        let request_id = self.search.allocate_request_id();
        tab.pending_request_id = Some(request_id);
        tab.search_in_progress = true;
        self.search.bind_request_tab(request_id, tab.id);

        let req = SearchRequest {
            request_id,
            query: tab.query_state.query.clone(),
            entries: Arc::clone(&tab.index_state.entries),
            limit: self.limit,
            use_regex: tab.use_regex,
            ignore_case: tab.ignore_case,
            root: tab.root.clone(),
            prefer_relative: Self::prefer_relative_display_for(&tab.index_state.index.source),
        };
        if self.search.tx.send(req).is_err() {
            tab.pending_request_id = None;
            tab.search_in_progress = false;
            tab.notice = "Search worker is unavailable".to_string();
        }
    }

    fn handle_background_index_response(&mut self, tab_index: usize, msg: IndexResponse) {
        let mut trigger_search = false;
        let mut cleanup_request_id: Option<u64> = None;
        let mut deferred_filelist: Option<(u64, PathBuf, Vec<PathBuf>)> = None;

        {
            let Some(tab) = self.tabs.get_mut(tab_index) else {
                return;
            };
            match msg {
                IndexResponse::Started { request_id, source } => {
                    if tab.index_state.pending_index_request_id != Some(request_id) {
                        return;
                    }
                    tab.index_state.index.source = source.clone();
                    self.indexing
                        .background_states
                        .entry(request_id)
                        .or_default()
                        .source = Some(source);
                }
                IndexResponse::Batch {
                    request_id,
                    entries,
                } => {
                    if tab.index_state.pending_index_request_id != Some(request_id) {
                        return;
                    }
                    let state = self
                        .indexing
                        .background_states
                        .entry(request_id)
                        .or_default();
                    for entry in entries {
                        state.entries.push(entry.into());
                    }
                }
                IndexResponse::ReplaceAll {
                    request_id,
                    entries,
                } => {
                    if tab.index_state.pending_index_request_id != Some(request_id) {
                        return;
                    }
                    let state = self
                        .indexing
                        .background_states
                        .entry(request_id)
                        .or_default();
                    state.entries.clear();
                    for entry in entries {
                        state.entries.push(entry.into());
                    }
                }
                IndexResponse::Finished { request_id, source } => {
                    if tab.index_state.pending_index_request_id != Some(request_id) {
                        cleanup_request_id = Some(request_id);
                    } else {
                        let state = self
                            .indexing
                            .background_states
                            .remove(&request_id)
                            .unwrap_or_default();
                        tab.index_state.index.source = state.source.unwrap_or(source);
                        tab.index_state.index.entries.clear();
                        tab.index_state.all_entries = Arc::new(state.entries);
                        if tab.include_files && tab.include_dirs {
                            tab.index_state.entries = Arc::clone(&tab.index_state.all_entries);
                        } else {
                            let filtered: Vec<Entry> = tab
                                .index_state
                                .all_entries
                                .iter()
                                .filter(|entry| {
                                    Self::is_entry_visible_for_flags(
                                        entry,
                                        tab.include_files,
                                        tab.include_dirs,
                                    )
                                })
                                .cloned()
                                .collect();
                            tab.index_state.entries = Arc::new(filtered);
                        }
                        tab.index_state.pending_index_request_id = None;
                        tab.index_state.index_in_progress = false;
                        tab.index_state.pending_index_entries.clear();
                        tab.index_state.pending_index_entries_request_id = None;
                        tab.index_state.search_resume_pending = false;
                        tab.index_state.search_rerun_pending = false;
                        tab.index_state.last_search_snapshot_len = tab.index_state.entries.len();
                        tab.index_state.last_incremental_results_refresh = Instant::now();
                        if matches!(tab.index_state.index.source, IndexSource::Walker) {
                            for entry in tab.index_state.all_entries.iter() {
                                if entry.kind.is_none()
                                    && !tab.index_state.pending_kind_paths_set.contains(&entry.path)
                                    && !tab.index_state.in_flight_kind_paths.contains(&entry.path)
                                {
                                    tab.index_state
                                        .pending_kind_paths_set
                                        .insert(entry.path.clone());
                                    tab.index_state
                                        .pending_kind_paths
                                        .push_back(entry.path.clone());
                                }
                            }
                            tab.index_state.kind_resolution_in_progress =
                                !tab.index_state.pending_kind_paths.is_empty()
                                    || !tab.index_state.in_flight_kind_paths.is_empty();
                        } else {
                            tab.index_state.pending_kind_paths.clear();
                            tab.index_state.pending_kind_paths_set.clear();
                            tab.index_state.in_flight_kind_paths.clear();
                            tab.index_state.kind_resolution_in_progress = false;
                        }
                        if self
                            .filelist_state
                            .pending_after_index
                            .as_ref()
                            .is_some_and(|pending| {
                                pending.tab_id == tab.id
                                    && Self::path_key(&pending.root) == Self::path_key(&tab.root)
                            })
                        {
                            deferred_filelist = Some((
                                tab.id,
                                tab.root.clone(),
                                tab.index_state
                                    .all_entries
                                    .iter()
                                    .map(|entry| entry.path.clone())
                                    .collect(),
                            ));
                            self.filelist_state.pending_after_index = None;
                        }

                        if tab.query_state.query.trim().is_empty() {
                            tab.result_state.results = tab
                                .index_state
                                .entries
                                .iter()
                                .take(self.limit)
                                .cloned()
                                .map(|entry| (entry.path, 0.0))
                                .collect();
                            if tab.result_state.results.is_empty() {
                                tab.result_state.current_row = None;
                                tab.result_state.preview.clear();
                                tab.pending_preview_request_id = None;
                                tab.preview_in_progress = false;
                            } else {
                                let max_index = tab.result_state.results.len().saturating_sub(1);
                                tab.result_state.current_row =
                                    Some(tab.result_state.current_row.unwrap_or(0).min(max_index));
                            }
                        } else {
                            trigger_search = true;
                        }
                        Self::shrink_tab_checkpoint_buffers(tab);
                        cleanup_request_id = Some(request_id);
                    }
                }
                IndexResponse::Failed { request_id, error } => {
                    if tab.index_state.pending_index_request_id != Some(request_id) {
                        cleanup_request_id = Some(request_id);
                    } else {
                        Self::settle_background_index_failure(
                            tab,
                            Some(format!("Indexing failed: {}", error)),
                        );
                        cleanup_request_id = Some(request_id);
                    }
                }
                IndexResponse::Canceled { request_id } => {
                    if tab.index_state.pending_index_request_id == Some(request_id) {
                        Self::settle_background_index_failure(tab, None);
                    }
                    cleanup_request_id = Some(request_id);
                }
                IndexResponse::Truncated { request_id, limit } => {
                    if tab.index_state.pending_index_request_id == Some(request_id) {
                        tab.notice = format!(
                            "Walker capped at {} entries (set FLISTWALKER_WALKER_MAX_ENTRIES to adjust)",
                            limit
                        );
                    }
                }
            }
        }

        if let Some((tab_id, root, entries)) = deferred_filelist {
            self.request_filelist_creation(tab_id, root, entries);
        }
        if trigger_search {
            self.enqueue_search_request_for_tab_index(tab_index);
        }
        if let Some(request_id) = cleanup_request_id {
            self.indexing.cleanup_request(request_id);
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
        while let Ok(msg) = self.indexing.rx.try_recv() {
            let request_id = match &msg {
                IndexResponse::Started { request_id, .. }
                | IndexResponse::Batch { request_id, .. }
                | IndexResponse::ReplaceAll { request_id, .. }
                | IndexResponse::Finished { request_id, .. }
                | IndexResponse::Failed { request_id, .. }
                | IndexResponse::Canceled { request_id }
                | IndexResponse::Truncated { request_id, .. } => *request_id,
            };
            let target_tab_id = self.indexing.request_tabs.get(&request_id).copied();
            let current_tab_id = self.current_tab_id();
            if let Some(tab_id) = target_tab_id {
                if Some(tab_id) != current_tab_id {
                    if let Some(tab_index) = self.find_tab_index_by_id(tab_id) {
                        self.handle_background_index_response(tab_index, msg);
                    } else {
                        self.indexing.cleanup_request(request_id);
                    }
                    processed = processed.saturating_add(1);
                    if processed >= MAX_MESSAGES_PER_FRAME || frame_start.elapsed() >= FRAME_BUDGET
                    {
                        break;
                    }
                    continue;
                }
            }

            let terminal_request_id = match &msg {
                IndexResponse::Finished { request_id, .. }
                | IndexResponse::Failed { request_id, .. }
                | IndexResponse::Canceled { request_id } => Some(*request_id),
                _ => None,
            };

            match msg {
                IndexResponse::Started { request_id, source } => {
                    if Some(request_id) != self.indexing.pending_request_id {
                        continue;
                    }
                    self.index.source = source;
                    self.refresh_status_line();
                }
                IndexResponse::Batch {
                    request_id,
                    entries,
                } => {
                    if Some(request_id) != self.indexing.pending_request_id {
                        continue;
                    }
                    self.queue_index_batch(request_id, entries);
                    has_index_progress = true;
                }
                IndexResponse::ReplaceAll {
                    request_id,
                    entries,
                } => {
                    if Some(request_id) != self.indexing.pending_request_id {
                        continue;
                    }
                    self.indexing.pending_entries.clear();
                    self.indexing.pending_entries_request_id = None;
                    self.index.entries.clear();
                    self.indexing.incremental_filtered_entries.clear();
                    self.queue_index_batch(request_id, entries);
                    has_index_progress = true;
                }
                IndexResponse::Finished { request_id, source } => {
                    if Some(request_id) != self.indexing.pending_request_id {
                        self.indexing.cleanup_request(request_id);
                        continue;
                    }
                    self.drain_queued_index_entries(request_id, usize::MAX);
                    self.index.source = source;
                    self.all_entries = Arc::new(std::mem::take(&mut self.index.entries));
                    self.indexing.last_search_snapshot_len = self.all_entries.len();
                    self.indexing.incremental_filtered_entries.clear();
                    self.indexing.settle_active_terminal_state();
                    self.apply_entry_filters(true);
                    self.rebuild_entry_kind_cache();
                    if matches!(self.index.source, IndexSource::Walker) {
                        self.queue_unknown_kind_paths_for_completed_walker_entries();
                    } else {
                        self.reset_kind_resolution_state();
                    }
                    self.clear_notice();
                    let current_tab_id = self.current_tab_id().unwrap_or_default();
                    if self
                        .filelist_state
                        .pending_after_index
                        .as_ref()
                        .is_some_and(|pending| {
                            pending.tab_id == current_tab_id
                                && Self::path_key(&pending.root) == Self::path_key(&self.root)
                        })
                    {
                        let root = self.root.clone();
                        let entries = self.filelist_entries_snapshot();
                        self.filelist_state.pending_after_index = None;
                        self.request_filelist_creation(current_tab_id, root, entries);
                    }
                    self.shrink_checkpoint_buffers();
                    self.indexing.cleanup_request(request_id);
                    finished_current_request = true;
                    break;
                }
                IndexResponse::Failed { request_id, error } => {
                    if Some(request_id) != self.indexing.pending_request_id {
                        self.indexing.cleanup_request(request_id);
                        continue;
                    }
                    self.indexing.settle_active_terminal_state();
                    self.filelist_state.pending_after_index = None;
                    self.shrink_checkpoint_buffers();
                    self.set_notice(format!("Indexing failed: {}", error));
                    self.indexing.cleanup_request(request_id);
                }
                IndexResponse::Canceled { request_id } => {
                    if Some(request_id) == self.indexing.pending_request_id {
                        self.indexing.settle_active_terminal_state();
                        self.shrink_checkpoint_buffers();
                    }
                    self.indexing.cleanup_request(request_id);
                }
                IndexResponse::Truncated { request_id, limit } => {
                    if Some(request_id) == self.indexing.pending_request_id {
                        self.set_notice(format!(
                            "Walker capped at {} entries (set FLISTWALKER_WALKER_MAX_ENTRIES to adjust)",
                            limit
                        ));
                    }
                }
            }
            if let Some(request_id) = terminal_request_id {
                self.indexing.inflight_requests.remove(&request_id);
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

        if let Some(request_id) = self.indexing.pending_request_id {
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

        if self.query_state.query.trim().is_empty() {
            self.apply_incremental_empty_query_results();
        } else {
            self.maybe_refresh_incremental_search();
        }
        self.dispatch_index_queue();
    }

    fn ensure_entry_filters(&mut self) -> bool {
        if !self.include_files && !self.include_dirs {
            self.include_files = true;
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
        fn clamp_row(current_row: Option<usize>, results_len: usize) -> Option<usize> {
            current_row.map(|row| row.min(results_len.saturating_sub(1)))
        }

        let selected_path = preserve_selected_path
            .then(|| {
                self.current_row
                    .and_then(|row| self.results.get(row).map(|(path, _)| path.clone()))
            })
            .flatten();
        let previous_row = self.current_row;
        self.results = results;
        if self.results.is_empty() {
            self.current_row = None;
            self.preview.clear();
            self.worker_bus.preview.in_progress = false;
            self.worker_bus.preview.pending_request_id = None;
        } else {
            let previous_row = clamp_row(previous_row, self.results.len());
            self.current_row = selected_path
                .and_then(|selected| self.results.iter().position(|(path, _)| *path == selected))
                .or(previous_row);
            self.request_preview_for_current();
            if !keep_scroll_position {
                self.ui.scroll_to_current = true;
            }
        }
        self.refresh_status_line();
    }

    pub(super) fn enqueue_search_request(&mut self) {
        self.commit_query_history_if_needed(false);
        let request_id = self.search.allocate_request_id();
        self.search.set_pending_request_id(Some(request_id));
        if let Some(tab_id) = self.current_tab_id() {
            self.search.bind_request_tab(request_id, tab_id);
        }
        self.search.set_in_progress(true);
        self.refresh_status_line();

        let req = SearchRequest {
            request_id,
            query: self.query_state.query.clone(),
            entries: Arc::clone(&self.entries),
            limit: self.limit,
            use_regex: self.use_regex,
            ignore_case: self.ignore_case,
            root: self.root.clone(),
            prefer_relative: self.prefer_relative_display(),
        };

        if self.search.tx.send(req).is_err() {
            self.search.set_pending_request_id(None);
            self.search.set_in_progress(false);
            self.set_notice("Search worker is unavailable");
        }
    }

    pub(super) fn poll_search_response(&mut self) {
        while let Ok(response) = self.search.rx.try_recv() {
            let target_tab_id = self.search.take_request_tab(response.request_id);
            if Some(response.request_id) == self.search.pending_request_id() {
                self.search.set_pending_request_id(None);
                self.search.set_in_progress(false);
                if let Some(error) = response.error {
                    self.set_notice(format!("Search failed: {error}"));
                } else {
                    self.clear_notice();
                }
                self.replace_results_snapshot(response.results, false);
                if self.indexing.search_rerun_pending
                    && !self.query_state.query.trim().is_empty()
                    && self.indexing.in_progress
                    && self.should_refresh_incremental_search()
                {
                    self.indexing.search_rerun_pending = false;
                    self.indexing.search_resume_pending = false;
                    self.sync_entries_from_incremental();
                    self.indexing.last_search_snapshot_len = self.entries.len();
                    self.indexing.last_incremental_results_refresh = Instant::now();
                    self.update_results();
                }
                continue;
            }

            let Some(tab_id) = target_tab_id else {
                continue;
            };
            let Some(tab_index) = self.find_tab_index_by_id(tab_id) else {
                continue;
            };
            let Some(tab) = self.tabs.get_mut(tab_index) else {
                continue;
            };
            tab.pending_request_id = None;
            tab.search_in_progress = false;
            tab.notice = response
                .error
                .map(|error| format!("Search failed: {error}"))
                .unwrap_or_default();
            tab.result_state.base_results = response.results.clone();
            tab.result_state.results = response.results;
            tab.result_state.results_compacted = false;
            tab.result_state.result_sort_mode = ResultSortMode::Score;
            tab.result_state.pending_sort_request_id = None;
            tab.result_state.sort_in_progress = false;
            if tab.result_state.results.is_empty() {
                tab.result_state.current_row = None;
                tab.result_state.preview.clear();
                tab.pending_preview_request_id = None;
                tab.preview_in_progress = false;
            } else {
                let max_index = tab.result_state.results.len().saturating_sub(1);
                tab.result_state.current_row =
                    tab.result_state.current_row.map(|row| row.min(max_index));
            }
            Self::compact_inactive_tab_state(tab);
        }
    }

    pub(super) fn update_results(&mut self) {
        if self.query_state.query.trim().is_empty() {
            self.search.set_pending_request_id(None);
            self.search.set_in_progress(false);
            let results = self
                .entries
                .iter()
                .take(self.limit)
                .cloned()
                .map(|entry| (entry.path, 0.0))
                .collect();
            self.replace_results_snapshot(results, false);
            return;
        }
        self.enqueue_search_request();
    }

    fn queue_index_batch(&mut self, request_id: u64, entries: Vec<IndexEntry>) {
        if self.indexing.pending_entries_request_id != Some(request_id) {
            self.indexing.pending_entries.clear();
            self.indexing.pending_entries_request_id = Some(request_id);
        }
        self.indexing.pending_entries.extend(entries);
    }

    fn ingest_index_entry(&mut self, entry: IndexEntry) {
        let entry: Entry = entry.into();
        if let Some(kind) = entry.kind {
            self.cache.entry_kind.set(entry.path.clone(), kind);
        }
        if entry.kind.is_none() && self.kind_resolution_needed_for_filters() {
            self.queue_kind_resolution(entry.path.clone());
        }
        if self.is_entry_visible_for_current_filter(&entry) {
            self.indexing
                .incremental_filtered_entries
                .push(entry.clone());
        }
        self.index.entries.push(entry);
    }

    fn drain_queued_index_entries(&mut self, request_id: u64, max_entries: usize) -> bool {
        if self.indexing.pending_entries_request_id != Some(request_id) {
            return false;
        }
        let mut processed = 0usize;
        while processed < max_entries {
            let Some(entry) = self.indexing.pending_entries.pop_front() else {
                break;
            };
            self.ingest_index_entry(entry);
            processed = processed.saturating_add(1);
        }
        if self.indexing.pending_entries.is_empty() {
            self.indexing.pending_entries_request_id = None;
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
        if self.indexing.pending_entries_request_id != Some(request_id) {
            return false;
        }
        let mut processed = 0usize;
        while processed < max_entries && frame_start.elapsed() < budget {
            let Some(entry) = self.indexing.pending_entries.pop_front() else {
                break;
            };
            self.ingest_index_entry(entry);
            processed = processed.saturating_add(1);
        }
        if self.indexing.pending_entries.is_empty() {
            self.indexing.pending_entries_request_id = None;
        }
        processed > 0
    }

    fn sync_entries_from_incremental(&mut self) {
        Self::overwrite_entries_arc(
            &mut self.entries,
            &self.indexing.incremental_filtered_entries,
        );
    }

    fn apply_incremental_empty_query_results(&mut self) {
        self.sync_entries_from_incremental();
        self.search.set_pending_request_id(None);
        self.search.set_in_progress(false);
        let results = self
            .entries
            .iter()
            .take(self.limit)
            .cloned()
            .map(|entry| (entry.path, 0.0))
            .collect();
        self.replace_results_snapshot(results, true);
    }

    fn maybe_refresh_incremental_search(&mut self) {
        if self.query_state.query.trim().is_empty() {
            return;
        }

        if self.indexing.search_resume_pending {
            if self.search.in_progress() {
                self.indexing.search_rerun_pending = true;
                return;
            }
            self.sync_entries_from_incremental();
            self.indexing.last_search_snapshot_len = self.entries.len();
            self.indexing.last_incremental_results_refresh = Instant::now();
            self.update_results();
            self.indexing.search_resume_pending = false;
            return;
        }

        let current_len = self.indexing.incremental_filtered_entries.len();
        if self.should_refresh_incremental_search() {
            if self.search.in_progress() {
                self.indexing.search_rerun_pending = true;
                return;
            }
            self.sync_entries_from_incremental();
            self.indexing.last_search_snapshot_len = current_len;
            self.indexing.last_incremental_results_refresh = Instant::now();
            self.update_results();
        }
    }

    pub(super) fn should_refresh_incremental_search(&self) -> bool {
        let current_len = self.indexing.incremental_filtered_entries.len();
        let delta = current_len.saturating_sub(self.indexing.last_search_snapshot_len);
        if delta == 0 {
            return false;
        }
        if self.indexing.in_progress {
            if delta < Self::INCREMENTAL_SEARCH_MIN_DELTA_DURING_INDEX {
                return false;
            }
            return self.indexing.last_incremental_results_refresh.elapsed()
                >= Self::INCREMENTAL_SEARCH_REFRESH_INTERVAL_DURING_INDEX;
        }
        self.indexing.last_incremental_results_refresh.elapsed()
            >= Self::INCREMENTAL_SEARCH_REFRESH_INTERVAL
    }

    fn filtered_entries(&self, source: &[Entry]) -> Vec<Entry> {
        source
            .iter()
            .filter(|entry| self.is_entry_visible_for_current_filter(entry))
            .cloned()
            .collect()
    }

    pub(super) fn apply_entry_filters(&mut self, keep_scroll_position: bool) {
        if self.kind_resolution_needed_for_filters() {
            self.queue_unknown_kind_paths_for_active_entries();
        } else if !self.indexing.pending_kind_paths.is_empty()
            || !self.indexing.in_flight_kind_paths.is_empty()
        {
            self.reset_kind_resolution_state();
        }

        let source_is_all_entries = !self.indexing.in_progress || self.index.entries.is_empty();
        let base = if !source_is_all_entries {
            &self.index.entries
        } else {
            self.all_entries.as_ref()
        };
        if source_is_all_entries && self.include_files && self.include_dirs {
            self.entries = Arc::clone(&self.all_entries);
        } else {
            self.entries = Arc::new(self.filtered_entries(base));
        }
        if self.indexing.in_progress {
            Self::overwrite_entries_vec(
                &mut self.indexing.incremental_filtered_entries,
                self.entries.as_ref(),
            );
        } else {
            self.indexing.incremental_filtered_entries.clear();
        }
        self.indexing.last_search_snapshot_len = self.entries.len();
        self.indexing.search_rerun_pending = false;

        if self.query_state.query.trim().is_empty() {
            self.search.set_pending_request_id(None);
            self.search.set_in_progress(false);
            let results = self
                .entries
                .iter()
                .take(self.limit)
                .cloned()
                .map(|entry| (entry.path, 0.0))
                .collect();
            self.replace_results_snapshot(results, keep_scroll_position);
        } else {
            self.update_results();
        }
    }
}

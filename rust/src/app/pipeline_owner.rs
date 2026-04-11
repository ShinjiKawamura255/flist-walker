use super::*;
use crate::app::search_coordinator::SearchResponseRoute;

pub(super) struct PipelineOwner<'a> {
    app: &'a mut FlistWalkerApp,
}

impl<'a> PipelineOwner<'a> {
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

    pub(super) fn new(app: &'a mut FlistWalkerApp) -> Self {
        Self { app }
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
                self.app
                    .current_row
                    .and_then(|row| self.app.results.get(row).map(|(path, _)| path.clone()))
            })
            .flatten();
        let previous_row = self.app.current_row;
        self.app.results = results;
        if self.app.results.is_empty() {
            self.app.current_row = None;
            self.app.preview.clear();
            self.app.worker_bus.preview.in_progress = false;
            self.app.worker_bus.preview.pending_request_id = None;
        } else {
            let previous_row = clamp_row(previous_row, self.app.results.len());
            self.app.current_row = selected_path
                .and_then(|selected| {
                    self.app
                        .results
                        .iter()
                        .position(|(path, _)| *path == selected)
                })
                .or(previous_row);
            self.app.request_preview_for_current();
            if !keep_scroll_position {
                self.app.ui.scroll_to_current = true;
            }
        }
        self.app.refresh_status_line();
    }

    pub(super) fn enqueue_search_request(&mut self) {
        self.app.commit_query_history_if_needed(false);
        let request_id = self
            .app
            .search
            .begin_active_request(self.app.current_tab_id());
        self.app.refresh_status_line();

        let req = self.build_active_search_request(request_id);
        if self.app.search.tx.send(req).is_err() {
            self.app.search.clear_active_request_state();
            self.app.set_notice("Search worker is unavailable");
        }
    }

    pub(super) fn poll_search_response(&mut self) {
        while let Ok(response) = self.app.search.rx.try_recv() {
            match self.app.search.route_response(response.request_id) {
                SearchResponseRoute::Active => self.handle_active_search_response(response),
                SearchResponseRoute::Background(tab_id) => {
                    self.app.apply_background_search_response(tab_id, response);
                }
                SearchResponseRoute::Stale => continue,
            }
        }
    }

    pub(super) fn update_results(&mut self) {
        if self.app.query_state.query.trim().is_empty() {
            self.app.search.clear_active_request_state();
            let results = self
                .app
                .entries
                .iter()
                .take(self.app.limit)
                .cloned()
                .map(|entry| (entry.path, 0.0))
                .collect();
            self.app.replace_results_snapshot(results, false);
            return;
        }
        self.enqueue_search_request();
    }

    pub(super) fn apply_entry_filters(&mut self, keep_scroll_position: bool) {
        if self.app.kind_resolution_needed_for_filters() {
            self.app.queue_unknown_kind_paths_for_active_entries();
        } else if !self.app.indexing.pending_kind_paths.is_empty()
            || !self.app.indexing.in_flight_kind_paths.is_empty()
        {
            self.app.reset_kind_resolution_state();
        }

        let source_is_all_entries =
            !self.app.indexing.in_progress || self.app.index.entries.is_empty();
        let base = if !source_is_all_entries {
            &self.app.index.entries
        } else {
            self.app.all_entries.as_ref()
        };
        if source_is_all_entries && self.app.include_files && self.app.include_dirs {
            self.app.entries = Arc::clone(&self.app.all_entries);
        } else {
            self.app.entries = Arc::new(self.filtered_entries(base));
        }
        if self.app.indexing.in_progress {
            Self::overwrite_entries_vec(
                &mut self.app.indexing.incremental_filtered_entries,
                self.app.entries.as_ref(),
            );
        } else {
            self.app.indexing.incremental_filtered_entries.clear();
        }
        self.app.indexing.last_search_snapshot_len = self.app.entries.len();
        self.app.indexing.search_rerun_pending = false;

        if self.app.query_state.query.trim().is_empty() {
            self.app.search.clear_active_request_state();
            let results = self
                .app
                .entries
                .iter()
                .take(self.app.limit)
                .cloned()
                .map(|entry| (entry.path, 0.0))
                .collect();
            self.app
                .replace_results_snapshot(results, keep_scroll_position);
        } else {
            self.update_results();
        }
    }

    pub(super) fn apply_incremental_empty_query_results(&mut self) {
        self.sync_entries_from_incremental();
        self.app.search.clear_active_request_state();
        let results = self
            .app
            .entries
            .iter()
            .take(self.app.limit)
            .cloned()
            .map(|entry| (entry.path, 0.0))
            .collect();
        self.app.replace_results_snapshot(results, true);
    }

    pub(super) fn maybe_refresh_incremental_search(&mut self) {
        if self.app.query_state.query.trim().is_empty() {
            return;
        }

        if self.app.indexing.search_resume_pending {
            if self.app.search.in_progress() {
                self.app.indexing.search_rerun_pending = true;
                return;
            }
            self.sync_entries_from_incremental();
            self.app.indexing.last_search_snapshot_len = self.app.entries.len();
            self.app.indexing.last_incremental_results_refresh = Instant::now();
            self.update_results();
            self.app.indexing.search_resume_pending = false;
            return;
        }

        let current_len = self.app.indexing.incremental_filtered_entries.len();
        if self.app.should_refresh_incremental_search() {
            if self.app.search.in_progress() {
                self.app.indexing.search_rerun_pending = true;
                return;
            }
            self.sync_entries_from_incremental();
            self.app.indexing.last_search_snapshot_len = current_len;
            self.app.indexing.last_incremental_results_refresh = Instant::now();
            self.update_results();
        }
    }

    fn build_search_request_for_tab(
        tab: &AppTabState,
        request_id: u64,
        limit: usize,
    ) -> SearchRequest {
        SearchRequest {
            request_id,
            query: tab.query_state.query.clone(),
            entries: Arc::clone(&tab.index_state.entries),
            limit,
            use_regex: tab.use_regex,
            ignore_case: tab.ignore_case,
            root: tab.root.clone(),
            prefer_relative: FlistWalkerApp::prefer_relative_display_for(
                &tab.index_state.index.source,
            ),
        }
    }

    fn build_active_search_request(&self, request_id: u64) -> SearchRequest {
        SearchRequest {
            request_id,
            query: self.app.query_state.query.clone(),
            entries: Arc::clone(&self.app.entries),
            limit: self.app.limit,
            use_regex: self.app.use_regex,
            ignore_case: self.app.ignore_case,
            root: self.app.root.clone(),
            prefer_relative: self.app.prefer_relative_display(),
        }
    }

    fn handle_active_search_response(&mut self, response: SearchResponse) {
        self.app.search.clear_active_request_state();
        if let Some(error) = response.error {
            self.app.set_notice(format!("Search failed: {error}"));
        } else {
            self.app.clear_notice();
        }
        self.app.replace_results_snapshot(response.results, false);
        if self.app.indexing.search_rerun_pending
            && !self.app.query_state.query.trim().is_empty()
            && self.app.indexing.in_progress
            && self.app.should_refresh_incremental_search()
        {
            self.app.indexing.search_rerun_pending = false;
            self.app.indexing.search_resume_pending = false;
            self.sync_entries_from_incremental();
            self.app.indexing.last_search_snapshot_len = self.app.entries.len();
            self.app.indexing.last_incremental_results_refresh = Instant::now();
            self.update_results();
        }
    }
    fn filtered_entries(&self, source: &[Entry]) -> Vec<Entry> {
        source
            .iter()
            .filter(|entry| self.app.is_entry_visible_for_current_filter(entry))
            .cloned()
            .collect()
    }

    fn sync_entries_from_incremental(&mut self) {
        Self::overwrite_entries_arc(
            &mut self.app.entries,
            &self.app.indexing.incremental_filtered_entries,
        );
    }

    pub(super) fn enqueue_search_request_for_tab_index(&mut self, tab_index: usize) {
        let Some(tab) = self.app.tabs.get_mut(tab_index) else {
            return;
        };
        let request_id = self.app.search.begin_tab_request(tab);
        let req = Self::build_search_request_for_tab(tab, request_id, self.app.limit);
        if self.app.search.tx.send(req).is_err() {
            tab.pending_request_id = None;
            tab.search_in_progress = false;
            tab.notice = "Search worker is unavailable".to_string();
        }
    }
}

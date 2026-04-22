use super::{result_reducer, AppTabState, Entry, FlistWalkerApp, SearchRequest};
use crate::app::search_coordinator::SearchResponseRoute;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

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
        result_reducer::apply_results_with_selection_policy(
            self.app,
            results,
            keep_scroll_position,
            preserve_selected_path,
        );
    }

    pub(super) fn enqueue_search_request(&mut self) {
        self.app.commit_query_history_if_needed(false);
        let current_tab_id = self.app.current_tab_id();
        let request_id = self.app.shell.search.begin_active_request(current_tab_id);
        self.app.refresh_status_line();

        let req = self.build_active_search_request(request_id);
        if self.app.shell.search.tx.send(req).is_err() {
            self.app.shell.search.clear_active_request_state();
            self.app.set_notice("Search worker is unavailable");
        }
    }

    pub(super) fn poll_search_response(&mut self) {
        while let Ok(response) = self.app.shell.search.rx.try_recv() {
            match self.app.shell.search.route_response(response.request_id) {
                SearchResponseRoute::Active => {
                    result_reducer::apply_active_search_response(self.app, &response);
                }
                SearchResponseRoute::Background(tab_id) => {
                    self.app.apply_background_search_response(tab_id, response);
                }
                SearchResponseRoute::Stale => continue,
            }
        }
    }

    pub(super) fn update_results(&mut self) {
        if self.app.shell.runtime.query_state.query.trim().is_empty() {
            self.app.shell.search.clear_active_request_state();
            let results = self
                .app
                .shell
                .runtime
                .entries
                .iter()
                .take(self.app.shell.runtime.limit)
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
        } else if !self.app.shell.indexing.pending_kind_paths.is_empty()
            || !self.app.shell.indexing.in_flight_kind_paths.is_empty()
        {
            self.app.reset_kind_resolution_state();
        }

        let source_is_all_entries =
            !self.app.shell.indexing.in_progress || self.app.shell.runtime.index.entries.is_empty();
        let base = if !source_is_all_entries {
            &self.app.shell.runtime.index.entries
        } else {
            self.app.shell.runtime.all_entries.as_ref()
        };
        let needs_filtering = !self.app.shell.runtime.include_files
            || !self.app.shell.runtime.include_dirs
            || self.ignore_list_filter_active();
        // Keep the zero-copy path only when no per-entry filter needs evaluation.
        // Ignore List must stay in the filtered path even when files/folders are both enabled,
        // otherwise the default all-entries snapshot leaks ignored paths back into the UI.
        if needs_filtering {
            self.app.shell.runtime.entries = Arc::new(self.filtered_entries(base));
        } else if source_is_all_entries {
            self.app.shell.runtime.entries = Arc::clone(&self.app.shell.runtime.all_entries);
        } else {
            self.app.shell.runtime.entries = Arc::new(base.clone());
        }
        if self.app.shell.indexing.in_progress {
            let entries = Arc::clone(&self.app.shell.runtime.entries);
            let source_entries = entries.as_ref().to_vec();
            Self::overwrite_entries_vec(
                &mut self.app.shell.indexing.incremental_filtered_entries,
                &source_entries,
            );
        } else {
            self.app.shell.indexing.incremental_filtered_entries.clear();
        }
        self.app.shell.indexing.last_search_snapshot_len = self.app.shell.runtime.entries.len();
        self.app.shell.indexing.search_rerun_pending = false;

        if self.app.shell.runtime.query_state.query.trim().is_empty() {
            self.app.shell.search.clear_active_request_state();
            let results = self
                .app
                .shell
                .runtime
                .entries
                .iter()
                .take(self.app.shell.runtime.limit)
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
        self.app.shell.search.clear_active_request_state();
        let results = self
            .app
            .shell
            .runtime
            .entries
            .iter()
            .take(self.app.shell.runtime.limit)
            .cloned()
            .map(|entry| (entry.path, 0.0))
            .collect();
        self.app.replace_results_snapshot(results, true);
    }

    pub(super) fn maybe_refresh_incremental_search(&mut self) {
        if self.app.shell.runtime.query_state.query.trim().is_empty() {
            return;
        }

        if self.app.shell.indexing.search_resume_pending {
            if self.app.shell.search.in_progress() {
                self.app.shell.indexing.search_rerun_pending = true;
                return;
            }
            self.sync_entries_from_incremental();
            self.app.shell.indexing.last_search_snapshot_len = self.app.shell.runtime.entries.len();
            self.app.shell.indexing.last_incremental_results_refresh = Instant::now();
            self.update_results();
            self.app.shell.indexing.search_resume_pending = false;
            return;
        }

        let current_len = self.app.shell.indexing.incremental_filtered_entries.len();
        if self.app.should_refresh_incremental_search() {
            if self.app.shell.search.in_progress() {
                self.app.shell.indexing.search_rerun_pending = true;
                return;
            }
            self.sync_entries_from_incremental();
            self.app.shell.indexing.last_search_snapshot_len = current_len;
            self.app.shell.indexing.last_incremental_results_refresh = Instant::now();
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
            query: self.app.shell.runtime.query_state.query.clone(),
            entries: Arc::clone(&self.app.shell.runtime.entries),
            limit: self.app.shell.runtime.limit,
            use_regex: self.app.shell.runtime.use_regex,
            ignore_case: self.app.shell.runtime.ignore_case,
            root: self.app.shell.runtime.root.clone(),
            prefer_relative: self.app.prefer_relative_display(),
        }
    }

    fn filtered_entries(&self, source: &[Entry]) -> Vec<Entry> {
        source
            .iter()
            .filter(|entry| self.app.is_entry_visible_for_current_filter(entry))
            .cloned()
            .collect()
    }

    fn ignore_list_filter_active(&self) -> bool {
        self.app.shell.ui.ignore_list_enabled
            && !self.app.shell.runtime.ignore_list_terms.is_empty()
    }

    fn sync_entries_from_incremental(&mut self) {
        let incremental_entries = self.app.shell.indexing.incremental_filtered_entries.clone();
        Self::overwrite_entries_arc(&mut self.app.shell.runtime.entries, &incremental_entries);
    }

    pub(super) fn enqueue_search_request_for_tab_index(&mut self, tab_index: usize) {
        let limit = self.app.shell.runtime.limit;
        let (request_id, req) = {
            let shell = &mut self.app.shell;
            let (tabs, search) = (&mut shell.tabs, &mut shell.search);
            let Some(tab) = tabs.get_mut(tab_index) else {
                return;
            };
            let request_id = search.begin_tab_request(tab);
            let req = Self::build_search_request_for_tab(tab, request_id, limit);
            (request_id, req)
        };
        if self.app.shell.search.tx.send(req).is_err() {
            let Some(tab) = self.app.shell.tabs.get_mut(tab_index) else {
                return;
            };
            if Some(request_id) == tab.pending_request_id {
                tab.pending_request_id = None;
            }
            tab.search_in_progress = false;
            tab.notice = "Search worker is unavailable".to_string();
        }
    }
}

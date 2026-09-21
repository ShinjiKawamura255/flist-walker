use super::{result_reducer, AppTabState, FlistWalkerApp, ResultSortMode, SearchRequest};
use crate::app::search_coordinator::SearchResponseRoute;
use std::path::PathBuf;
use std::sync::mpsc::TryRecvError;
use std::sync::Arc;
use std::time::Instant;

pub(super) struct PipelineOwner<'a> {
    app: &'a mut FlistWalkerApp,
}

impl<'a> PipelineOwner<'a> {
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
        // The current query is retained in runtime; preparation completion
        // submits it with the authoritative candidate snapshot exactly once.
        if self.app.active_entry_filter_pending() {
            return;
        }
        self.app.shell.runtime.query_state.search_error = None;
        self.app.commit_query_history_if_needed(false);
        let current_tab_id = self.app.current_tab_id();
        let (request_id, cancel) = self.app.shell.search.begin_active_request(current_tab_id);
        self.app.refresh_status_line();

        let req = self.build_active_search_request(request_id, cancel);
        if self.app.shell.search.worker_unavailable() || self.app.shell.search.tx.send(req).is_err()
        {
            self.poll_search_response();
            self.fail_search_worker();
        }
    }

    fn fail_search_worker(&mut self) {
        let active_pending = self.app.shell.search.in_progress()
            || self.app.shell.search.pending_request_id().is_some();
        self.app.shell.search.mark_worker_unavailable();
        if active_pending {
            self.app.shell.runtime.query_state.search_error = Some((
                self.app.shell.runtime.query_state.query.clone(),
                "Search worker is unavailable".into(),
            ));
            self.app.set_notice("Search worker is unavailable");
        }
        let active_index = self.app.shell.tabs.active_tab_index();
        for (index, tab) in self.app.shell.tabs.iter_mut().enumerate() {
            if index != active_index && (tab.search_in_progress || tab.pending_request_id.is_some())
            {
                tab.query_state.search_error = Some((
                    tab.query_state.query.clone(),
                    "Search worker is unavailable".into(),
                ));
                tab.notice = "Search worker is unavailable".into();
            }
            // Clear the active tab's saved state too: it may contain an older
            // request snapshot and must not resurrect it during a later swap.
            tab.clear_search_request_state();
        }
        self.app.refresh_status_line();
    }

    pub(super) fn poll_search_response(&mut self) {
        if self.app.shell.search.worker_unavailable() {
            return;
        }
        loop {
            let response = match self.app.shell.search.rx.try_recv() {
                Ok(response) => response,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.fail_search_worker();
                    break;
                }
            };
            match self.app.shell.search.route_response(response.request_id) {
                SearchResponseRoute::Active => {
                    result_reducer::apply_active_search_response(self.app, response);
                }
                SearchResponseRoute::Background(tab_id) => {
                    self.app.apply_background_search_response(tab_id, response);
                }
                SearchResponseRoute::Stale => continue,
            }
        }
    }

    pub(super) fn update_results(&mut self) {
        if self.app.active_entry_filter_pending() {
            return;
        }
        if !super::result_policy::needs_search_worker(
            &self.app.shell.runtime.query_state.query,
            self.app.shell.runtime.result_sort_mode,
            self.app.shell.runtime.result_sort_scope,
        ) {
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
            let count = self.app.shell.runtime.entries.len();
            self.app.shell.runtime.set_total_match_count(count);
            self.app.replace_results_snapshot(results, false);
            return;
        }
        self.enqueue_search_request();
    }

    pub(super) fn apply_entry_filters(&mut self, keep_scroll_position: bool) {
        self.app.request_active_entry_filter(keep_scroll_position);
    }

    pub(super) fn apply_incremental_empty_query_results(&mut self) {
        if self.app.active_entry_filter_pending() {
            return;
        }
        if self.app.shell.indexing.in_progress
            && self.app.shell.runtime.result_sort_mode != ResultSortMode::Score
        {
            // A preserve-sort refresh keeps the sorted last-good snapshot visible.
            // The terminal snapshot is installed only when its selected sort can
            // be applied synchronously or handed to the bounded sort worker.
            return;
        }
        let needs_filtering = !self.app.shell.runtime.include_files
            || !self.app.shell.runtime.include_dirs
            || self.ignore_list_filter_active();
        if self.app.shell.indexing.in_progress && !needs_filtering {
            self.app.shell.search.clear_active_request_state();
            let source = self.app.shell.indexing.build.index.entries.as_slice();
            let results = source
                .iter()
                .take(self.app.shell.runtime.limit)
                .cloned()
                .map(|entry| (entry.path, 0.0))
                .collect();
            self.app.shell.runtime.set_total_match_count(source.len());
            self.app.shell.indexing.last_search_snapshot_len = source.len();
            self.app.shell.indexing.last_incremental_results_refresh = Instant::now();
            self.app.replace_results_snapshot(results, true);
            return;
        }
        self.app.request_active_entry_filter(true);
    }

    pub(super) fn maybe_refresh_incremental_search(&mut self) {
        if self.app.active_entry_filter_pending() {
            return;
        }
        if self.app.shell.runtime.query_state.query.trim().is_empty() {
            return;
        }

        if self.app.shell.indexing.search_resume_pending {
            if self.app.shell.search.in_progress() {
                self.app.shell.indexing.search_rerun_pending = true;
                return;
            }
            self.app.request_active_entry_filter(true);
            return;
        }

        if self.app.should_refresh_incremental_search() {
            if self.app.shell.search.in_progress() {
                self.app.shell.indexing.search_rerun_pending = true;
                return;
            }
            self.app.request_active_entry_filter(true);
        }
    }

    fn build_search_request_for_tab(
        tab: &AppTabState,
        request_id: u64,
        limit: usize,
        cancel: Arc<std::sync::atomic::AtomicBool>,
    ) -> SearchRequest {
        SearchRequest {
            request_id,
            query: tab.query_state.query.clone(),
            entries: Arc::clone(&tab.result_state.committed.entries),
            limit,
            use_regex: tab.use_regex,
            ignore_case: tab.ignore_case,
            root: tab.root.clone(),
            prefer_relative: FlistWalkerApp::prefer_relative_display_for(
                &tab.index_state.build.index.source,
            ),
            sort_mode: tab.result_state.result_sort_mode,
            sort_scope: tab.result_state.result_sort_scope,
            cancel,
        }
    }

    fn build_active_search_request(
        &self,
        request_id: u64,
        cancel: Arc<std::sync::atomic::AtomicBool>,
    ) -> SearchRequest {
        SearchRequest {
            request_id,
            query: self.app.shell.runtime.query_state.query.clone(),
            entries: Arc::clone(&self.app.shell.runtime.entries),
            limit: self.app.shell.runtime.limit,
            use_regex: self.app.shell.runtime.use_regex,
            ignore_case: self.app.shell.runtime.ignore_case,
            root: self.app.shell.runtime.root.clone(),
            prefer_relative: self.app.prefer_relative_display(),
            sort_mode: self.app.shell.runtime.result_sort_mode,
            sort_scope: self.app.shell.runtime.result_sort_scope,
            cancel,
        }
    }

    fn ignore_list_filter_active(&self) -> bool {
        self.app.shell.ui.ignore_list_enabled
            && !self.app.shell.runtime.ignore_list_terms.is_empty()
    }

    pub(super) fn enqueue_search_request_for_tab_index(&mut self, tab_index: usize) {
        let limit = self.app.shell.runtime.limit;
        let req = {
            let shell = &mut self.app.shell;
            let (tabs, search) = (&mut shell.tabs, &mut shell.search);
            let Some(tab) = tabs.get_mut(tab_index) else {
                return;
            };
            let (request_id, cancel) = search.begin_tab_request(tab);
            tab.query_state.search_error = None;
            Self::build_search_request_for_tab(tab, request_id, limit, cancel)
        };
        if self.app.shell.search.worker_unavailable() || self.app.shell.search.tx.send(req).is_err()
        {
            self.poll_search_response();
            self.fail_search_worker();
        }
    }
}

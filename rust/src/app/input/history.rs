use super::super::FlistWalkerApp;
use crate::query_history::{history_matches, history_with_query};
use std::time::Instant;

fn sync_shared_query_history_to_tabs(app: &mut FlistWalkerApp) {
    let history = app.shell.runtime.query_state.query_history.clone();
    let active_tab = app.shell.tabs.active_tab_index();
    for (index, tab) in app.shell.tabs.iter_mut().enumerate() {
        if index != active_tab {
            tab.query_state.query_history = history.clone();
        }
    }
}

impl FlistWalkerApp {
    pub(in crate::app) fn reset_query_history_navigation(&mut self) {
        self.shell
            .runtime
            .query_state
            .reset_query_history_navigation();
    }

    pub(in crate::app) fn reset_history_search_state(&mut self) {
        self.shell.runtime.query_state.reset_history_search();
    }

    pub(in crate::app) fn refresh_history_search_results(&mut self) {
        if !self.shell.runtime.query_state.is_history_search_active() {
            self.shell
                .runtime
                .query_state
                .clear_history_search_results();
            self.refresh_status_line();
            return;
        }

        let results = {
            let query_state = &self.shell.runtime.query_state;
            history_matches(
                query_state.history_search_query(),
                query_state.query_history().iter(),
            )
        };
        self.shell
            .runtime
            .query_state
            .replace_history_search_results(results);
        self.refresh_status_line();
    }

    pub(in crate::app) fn start_history_search(&mut self) {
        self.commit_query_history_if_needed(true);
        self.shell.runtime.query_state.begin_history_search();
        self.refresh_history_search_results();
        self.request_focus_query();
        self.clear_unfocus_query_request();
    }

    pub(in crate::app) fn cancel_history_search(&mut self) {
        if !self
            .shell
            .runtime
            .query_state
            .restore_original_history_search_query()
        {
            return;
        }
        self.update_results();
        self.ensure_results_cursor_visible();
        self.finish_programmatic_query_replacement();
        self.set_notice("Canceled history search");
    }

    pub(in crate::app) fn accept_history_search(&mut self) {
        if self
            .shell
            .runtime
            .query_state
            .accept_history_search_selection()
            .is_none()
        {
            return;
        };
        self.update_results();
        self.ensure_results_cursor_visible();
        self.finish_programmatic_query_replacement();
        self.set_notice("Loaded query from history");
    }

    pub(in crate::app) fn move_history_search_selection(&mut self, delta: isize) {
        let before = self.shell.runtime.query_state.history_search_current;
        self.shell
            .runtime
            .query_state
            .move_history_search_selection(delta);
        if self.shell.runtime.query_state.history_search_current != before {
            self.shell.tabs.mark_active_tab_meaningfully_engaged();
        }
    }

    pub(in crate::app) fn select_history_search_result(&mut self, index: usize) {
        if index >= self.shell.runtime.query_state.history_search_results.len()
            || self.shell.runtime.query_state.history_search_current == Some(index)
        {
            return;
        }
        self.shell.runtime.query_state.history_search_current = Some(index);
        self.shell.tabs.mark_active_tab_meaningfully_engaged();
    }

    pub(in crate::app) fn mark_query_edited(&mut self) {
        self.shell.tabs.mark_active_tab_meaningfully_engaged();
        self.reset_query_history_navigation();
        self.set_query_history_dirty_since(Some(Instant::now()));
        self.invalidate_result_sort(true);
    }

    pub(in crate::app) fn commit_query_history_if_needed(&mut self, force: bool) {
        if self.shell.ui.ime_composition_active {
            return;
        }
        let should_commit = self
            .shell
            .runtime
            .query_state
            .query_history_dirty_since
            .is_some_and(|since| {
                force || since.elapsed() >= FlistWalkerApp::QUERY_HISTORY_IDLE_DELAY
            });
        if !should_commit
            || self
                .shell
                .runtime
                .query_state
                .query_history_cursor
                .is_some()
        {
            return;
        }
        let query = self.shell.runtime.query_state.query.clone();
        self.set_query_history_dirty_since(None);
        let Some(updated) =
            history_with_query(self.shell.runtime.query_state.query_history.iter(), &query)
        else {
            return;
        };
        if self
            .shell
            .runtime
            .query_state
            .query_history
            .iter()
            .ne(updated.iter())
        {
            self.shell.runtime.query_state.query_history = updated.into();
            sync_shared_query_history_to_tabs(self);
            self.mark_ui_state_dirty();
        }
    }
}

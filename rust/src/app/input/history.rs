use super::super::{input_history, FlistWalkerApp};

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
        input_history::refresh_history_search_results(self);
    }

    pub(in crate::app) fn start_history_search(&mut self) {
        input_history::start_history_search(self);
    }

    pub(in crate::app) fn cancel_history_search(&mut self) {
        input_history::cancel_history_search(self);
    }

    pub(in crate::app) fn accept_history_search(&mut self) {
        input_history::accept_history_search(self);
    }

    pub(in crate::app) fn move_history_search_selection(&mut self, delta: isize) {
        input_history::move_history_search_selection(self, delta);
    }

    pub(in crate::app) fn mark_query_edited(&mut self) {
        input_history::mark_query_edited(self);
    }

    pub(in crate::app) fn commit_query_history_if_needed(&mut self, force: bool) {
        input_history::commit_query_history_if_needed(self, force);
    }
}

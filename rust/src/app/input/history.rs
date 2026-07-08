use super::super::FlistWalkerApp;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::collections::VecDeque;
use std::time::Instant;

fn history_search_score(query: &str, candidate: &str, recency_rank: usize) -> Option<i64> {
    if query.trim().is_empty() {
        return Some(recency_rank as i64);
    }

    let matcher = SkimMatcherV2::default();
    matcher.fuzzy_match(candidate, query).or_else(|| {
        let query_lower = query.to_ascii_lowercase();
        let candidate_lower = candidate.to_ascii_lowercase();
        if candidate_lower.contains(&query_lower) {
            Some((query_lower.len() as i64) * 100 + recency_rank as i64)
        } else {
            None
        }
    })
}

fn push_query_history(history: &mut VecDeque<String>, query: &str) {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return;
    }
    if history.back().is_some_and(|entry| entry == trimmed) {
        return;
    }
    history.push_back(trimmed.to_string());
    while history.len() > FlistWalkerApp::QUERY_HISTORY_MAX {
        history.pop_front();
    }
}

fn sync_shared_query_history_to_tabs(app: &mut FlistWalkerApp) {
    let history = app.shell.runtime.query_state.query_history.clone();
    for tab in app.shell.tabs.iter_mut() {
        tab.query_state.query_history = history.clone();
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

        let mut scored = {
            let query_state = &self.shell.runtime.query_state;
            let query = query_state.history_search_query().trim();
            query_state
                .query_history()
                .iter()
                .rev()
                .enumerate()
                .filter_map(|(idx, entry)| {
                    history_search_score(query, entry, FlistWalkerApp::QUERY_HISTORY_MAX - idx)
                        .map(|score| (entry.clone(), score, idx))
                })
                .collect::<Vec<_>>()
        };
        scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.2.cmp(&b.2)));
        self.shell
            .runtime
            .query_state
            .replace_history_search_results(
                scored.into_iter().map(|(entry, _, _)| entry).collect(),
            );
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
        self.request_focus_query();
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
        self.request_focus_query();
        self.set_notice("Loaded query from history");
    }

    pub(in crate::app) fn move_history_search_selection(&mut self, delta: isize) {
        self.shell
            .runtime
            .query_state
            .move_history_search_selection(delta);
    }

    pub(in crate::app) fn mark_query_edited(&mut self) {
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
        let before_len = self.shell.runtime.query_state.query_history.len();
        let query = self.shell.runtime.query_state.query.clone();
        push_query_history(&mut self.shell.runtime.query_state.query_history, &query);
        self.set_query_history_dirty_since(None);
        if self.shell.runtime.query_state.query_history.len() != before_len
            || self
                .shell
                .runtime
                .query_state
                .query_history
                .back()
                .is_some_and(|entry| entry == query.trim())
        {
            sync_shared_query_history_to_tabs(self);
            self.mark_ui_state_dirty();
        }
    }
}

use std::collections::VecDeque;
use std::time::Instant;

#[derive(Clone, Debug)]
pub(super) struct QueryState {
    pub(super) query: String,
    pub(super) query_history: VecDeque<String>,
    pub(super) query_history_cursor: Option<usize>,
    pub(super) query_history_draft: Option<String>,
    pub(super) query_history_dirty_since: Option<Instant>,
    pub(super) history_search_active: bool,
    pub(super) history_search_query: String,
    pub(super) history_search_original_query: String,
    pub(super) history_search_results: Vec<String>,
    pub(super) history_search_current: Option<usize>,
    pub(super) kill_buffer: String,
}

impl QueryState {
    pub(super) fn new(query: String, query_history: VecDeque<String>) -> Self {
        Self {
            query,
            query_history,
            query_history_cursor: None,
            query_history_draft: None,
            query_history_dirty_since: None,
            history_search_active: false,
            history_search_query: String::new(),
            history_search_original_query: String::new(),
            history_search_results: Vec::new(),
            history_search_current: None,
            kill_buffer: String::new(),
        }
    }

    pub(super) fn reset_query_history_navigation(&mut self) {
        self.query_history_cursor = None;
        self.query_history_draft = None;
    }

    pub(super) fn is_history_search_active(&self) -> bool {
        self.history_search_active
    }

    pub(super) fn history_search_query(&self) -> &str {
        &self.history_search_query
    }

    pub(super) fn query_history(&self) -> &VecDeque<String> {
        &self.query_history
    }

    pub(super) fn begin_history_search(&mut self) {
        self.history_search_active = true;
        self.history_search_query.clear();
        self.history_search_original_query = self.query.clone();
    }

    pub(super) fn reset_history_search(&mut self) {
        self.history_search_active = false;
        self.history_search_query.clear();
        self.history_search_original_query.clear();
        self.history_search_results.clear();
        self.history_search_current = None;
    }

    pub(super) fn restore_original_history_search_query(&mut self) -> bool {
        if !self.history_search_active {
            return false;
        }
        self.query = self.history_search_original_query.clone();
        self.reset_history_search();
        true
    }

    pub(super) fn accept_history_search_selection(&mut self) -> Option<String> {
        if !self.history_search_active {
            return None;
        }
        let selected = self
            .history_search_current
            .and_then(|index| self.history_search_results.get(index))
            .cloned()?;
        self.query = selected.clone();
        self.reset_query_history_navigation();
        self.query_history_dirty_since = None;
        self.reset_history_search();
        Some(selected)
    }

    pub(super) fn replace_history_search_results(&mut self, results: Vec<String>) {
        self.history_search_results = results;
        self.history_search_current = (!self.history_search_results.is_empty()).then_some(0);
    }

    pub(super) fn clear_history_search_results(&mut self) {
        self.history_search_results.clear();
        self.history_search_current = None;
    }

    pub(super) fn move_history_search_selection(&mut self, delta: isize) {
        if !self.history_search_active || self.history_search_results.is_empty() {
            return;
        }
        let current = self.history_search_current.unwrap_or(0) as isize;
        let next = (current + delta).clamp(0, self.history_search_results.len() as isize - 1);
        self.history_search_current = Some(next as usize);
    }
}

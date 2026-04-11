use super::*;

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
}

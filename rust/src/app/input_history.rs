use super::FlistWalkerApp;
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

pub(super) fn refresh_history_search_results(app: &mut FlistWalkerApp) {
    if !app.shell.runtime.query_state.history_search_active {
        app.shell
            .runtime
            .query_state
            .history_search_results
            .clear();
        app.shell.runtime.query_state.history_search_current = None;
        app.refresh_status_line();
        return;
    }

    let query = app.shell.runtime.query_state.history_search_query.trim();
    let mut scored = app
        .shell
        .runtime
        .query_state
        .query_history
        .iter()
        .rev()
        .enumerate()
        .filter_map(|(idx, entry)| {
            history_search_score(query, entry, FlistWalkerApp::QUERY_HISTORY_MAX - idx)
                .map(|score| (entry.clone(), score, idx))
        })
        .collect::<Vec<_>>();
    scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.2.cmp(&b.2)));
    app.shell.runtime.query_state.history_search_results =
        scored.into_iter().map(|(entry, _, _)| entry).collect();
    app.shell.runtime.query_state.history_search_current = (!app
        .shell
        .runtime
        .query_state
        .history_search_results
        .is_empty())
    .then_some(0);
    app.refresh_status_line();
}

pub(super) fn start_history_search(app: &mut FlistWalkerApp) {
    app.commit_query_history_if_needed(true);
    app.shell.runtime.query_state.history_search_active = true;
    app.shell.runtime.query_state.history_search_query.clear();
    app.shell.runtime.query_state.history_search_original_query =
        app.shell.runtime.query_state.query.clone();
    refresh_history_search_results(app);
    app.request_focus_query();
    app.clear_unfocus_query_request();
}

pub(super) fn cancel_history_search(app: &mut FlistWalkerApp) {
    if !app.shell.runtime.query_state.history_search_active {
        return;
    }
    app.shell.runtime.query_state.query = app
        .shell
        .runtime
        .query_state
        .history_search_original_query
        .clone();
    app.reset_history_search_state();
    app.update_results();
    app.request_focus_query();
    app.set_notice("Canceled history search");
}

pub(super) fn accept_history_search(app: &mut FlistWalkerApp) {
    if !app.shell.runtime.query_state.history_search_active {
        return;
    }
    let Some(index) = app.shell.runtime.query_state.history_search_current else {
        return;
    };
    let Some(selected) = app
        .shell
        .runtime
        .query_state
        .history_search_results
        .get(index)
        .cloned()
    else {
        return;
    };
    app.shell.runtime.query_state.query = selected;
    app.reset_query_history_navigation();
    app.set_query_history_dirty_since(None);
    app.reset_history_search_state();
    app.update_results();
    app.request_focus_query();
    app.set_notice("Loaded query from history");
}

pub(super) fn move_history_search_selection(app: &mut FlistWalkerApp, delta: isize) {
    if !app.shell.runtime.query_state.history_search_active
        || app
            .shell
            .runtime
            .query_state
            .history_search_results
            .is_empty()
    {
        return;
    }
    let current = app.shell.runtime.query_state.history_search_current.unwrap_or(0) as isize;
    let next = (current + delta).clamp(
        0,
        app.shell.runtime.query_state.history_search_results.len() as isize - 1,
    );
    app.shell.runtime.query_state.history_search_current = Some(next as usize);
}

pub(super) fn mark_query_edited(app: &mut FlistWalkerApp) {
    app.reset_query_history_navigation();
    app.set_query_history_dirty_since(Some(Instant::now()));
    app.invalidate_result_sort(true);
}

pub(super) fn push_query_history(history: &mut VecDeque<String>, query: &str) {
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

pub(super) fn sync_shared_query_history_to_tabs(app: &mut FlistWalkerApp) {
    let history = app.shell.runtime.query_state.query_history.clone();
    for tab in app.shell.tabs.iter_mut() {
        tab.query_state.query_history = history.clone();
    }
}

pub(super) fn commit_query_history_if_needed(app: &mut FlistWalkerApp, force: bool) {
    if app.shell.ui.ime_composition_active {
        return;
    }
    let should_commit = app
        .shell
        .runtime
        .query_state
        .query_history_dirty_since
        .is_some_and(|since| force || since.elapsed() >= FlistWalkerApp::QUERY_HISTORY_IDLE_DELAY);
    if !should_commit || app.shell.runtime.query_state.query_history_cursor.is_some() {
        return;
    }
    let before_len = app.shell.runtime.query_state.query_history.len();
    let query = app.shell.runtime.query_state.query.clone();
    push_query_history(&mut app.shell.runtime.query_state.query_history, &query);
    app.set_query_history_dirty_since(None);
    if app.shell.runtime.query_state.query_history.len() != before_len
        || app
            .shell
            .runtime
            .query_state
            .query_history
            .back()
            .is_some_and(|entry| entry == query.trim())
    {
        sync_shared_query_history_to_tabs(app);
        app.mark_ui_state_dirty();
    }
}

use super::{
    AppTabState, FlistWalkerApp, PreviewResponse, ResultSortMode, SearchResponse,
    SortMetadataRequest, SortMetadataResponse,
};
use crate::indexer::IndexSource;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

fn clear_tab_result_selection(tab: &mut AppTabState) {
    tab.result_state.committed.current_row = None;
    tab.result_state.committed.preview.clear();
    tab.clear_preview_request_state();
}

fn normalized_result_row(current_row: Option<usize>, results_len: usize) -> Option<usize> {
    if results_len == 0 {
        return None;
    }
    // Regression guard: a visible non-empty Results snapshot must never lose its
    // cursor; programmatic replacement selects row zero when no prior row exists.
    Some(current_row.unwrap_or(0).min(results_len - 1))
}

fn clamp_tab_result_selection(tab: &mut AppTabState) {
    if tab.result_state.committed.results.is_empty() {
        clear_tab_result_selection(tab);
        return;
    }
    tab.result_state.committed.current_row = normalized_result_row(
        tab.result_state.committed.current_row,
        tab.result_state.committed.results.len(),
    );
}

fn selected_tab_path(tab: &AppTabState) -> Option<&PathBuf> {
    let results = if tab.result_state.results_compacted {
        &tab.result_state.committed.base_results
    } else {
        &tab.result_state.committed.results
    };
    tab.result_state
        .committed
        .current_row
        .and_then(|row| results.get(row).map(|(path, _)| path))
}

fn invalidate_background_preview_if_selection_changed(
    tab: &mut AppTabState,
    previous_path: Option<&PathBuf>,
) -> bool {
    if selected_tab_path(tab) == previous_path {
        return false;
    }
    // Regression guard: an inactive tab may receive search/sort after its preview.
    // Never carry that old path's preview or request ownership into activation.
    tab.result_state.committed.preview.clear();
    tab.clear_preview_request_state();
    tab.mark_preview_reload_pending();
    true
}

pub(super) fn apply_results_with_selection_policy(
    app: &mut FlistWalkerApp,
    results: Vec<(PathBuf, f64)>,
    keep_scroll_position: bool,
    preserve_selected_path: bool,
) {
    let evicted_selected_path = app.shell.runtime.evicted_selected_path.clone();
    let selected_path = evicted_selected_path.clone().or_else(|| {
        preserve_selected_path
            .then(|| {
                app.shell.runtime.current_row.and_then(|row| {
                    app.shell
                        .runtime
                        .results
                        .get(row)
                        .map(|(path, _)| path.clone())
                })
            })
            .flatten()
    });
    let previous_row = app.shell.runtime.current_row;
    app.shell.runtime.results = results;
    if app.shell.runtime.results.is_empty() {
        app.set_current_row(None);
        app.shell.runtime.preview.clear();
        app.shell.worker_bus.preview.clear_request();
    } else {
        let previous_row = normalized_result_row(previous_row, app.shell.runtime.results.len());
        let selected_row = selected_path.as_ref().and_then(|selected| {
            app.shell
                .runtime
                .results
                .iter()
                .position(|(path, _)| path == selected)
        });
        if evicted_selected_path.is_some() && selected_row.is_some() {
            app.shell.runtime.evicted_selected_path = None;
        }
        app.set_current_row(selected_row.or(previous_row));
        app.request_preview_for_current();
        if !keep_scroll_position {
            app.request_scroll_to_current();
        }
    }
    app.refresh_status_line();
}

pub(super) fn clear_unrestored_evicted_selection(app: &mut FlistWalkerApp) {
    // Incremental index/search snapshots may not contain the restored path yet.
    // Discard the intent only after a successful authoritative snapshot has missed it.
    app.shell.runtime.evicted_selected_path = None;
}

pub(super) fn apply_background_search_response(
    app: &mut FlistWalkerApp,
    tab_id: u64,
    response: SearchResponse,
) {
    let Some(tab_index) = app.find_tab_index_by_id(tab_id) else {
        return;
    };
    if tab_index == app.shell.tabs.active_tab_index() {
        return;
    }
    let Some(tab) = app.shell.tabs.get_mut(tab_index) else {
        return;
    };
    let previous_path = selected_tab_path(tab).cloned();
    tab.clear_search_request_state();
    let response_failed = response.error.is_some();
    tab.notice = response
        .error
        .map(|error| format!("Search failed: {error}"))
        .unwrap_or_default();
    tab.result_state.committed.base_results_are_score_ranked = !response
        .sort_scope
        .sorts_all_matches_before_limit(response.sort_mode);
    tab.result_state.committed.base_results = response.results.clone();
    tab.result_state.committed.results = response.results;
    tab.result_state.committed.total_match_count = response.total_match_count;
    tab.result_state.results_compacted = false;
    tab.result_state.result_sort_mode = response.sort_mode;
    tab.result_state.result_sort_scope = response.sort_scope;
    tab.result_state.clear_sort_request_state();
    let local_sort = response.sort_scope == super::ResultSortScope::ShownResults
        && response.sort_mode != ResultSortMode::Score;
    let missing_paths = if local_sort && response.sort_mode.uses_metadata() {
        tab.result_state
            .committed
            .base_results
            .iter()
            .filter(|(path, _)| !app.shell.cache.sort_metadata.contains(path))
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    if local_sort && missing_paths.is_empty() {
        tab.result_state.committed.results = FlistWalkerApp::build_sorted_results_from(
            &tab.result_state.committed.base_results,
            response.sort_mode,
            app.shell.cache.sort_metadata.get_map(),
        );
    }
    if let Some(selected) = tab.result_state.evicted_selected_path.clone() {
        let selected_row = tab
            .result_state
            .committed
            .results
            .iter()
            .position(|(path, _)| *path == selected);
        if selected_row.is_some() || (!tab.index_state.index_in_progress && !response_failed) {
            tab.result_state.evicted_selected_path = None;
        }
        tab.result_state.committed.current_row = selected_row
            .or_else(|| normalized_result_row(None, tab.result_state.committed.results.len()));
        if tab.result_state.committed.results.is_empty() {
            clear_tab_result_selection(tab);
        }
    } else {
        clamp_tab_result_selection(tab);
    }
    let preview_invalidated =
        invalidate_background_preview_if_selection_changed(tab, previous_path.as_ref());
    FlistWalkerApp::trim_inactive_tab_preview(tab);
    if preview_invalidated {
        app.shell
            .tabs
            .clear_preview_response_routing_for_tab(tab_id);
    }
    if !missing_paths.is_empty() {
        request_background_sort_metadata(app, tab_id, response.sort_mode, missing_paths);
    }
}

pub(super) fn apply_active_search_response(
    app: &mut FlistWalkerApp,
    response: SearchResponse,
) -> bool {
    if Some(response.request_id) != app.shell.search.pending_request_id() {
        return false;
    }
    app.shell.search.clear_active_request_state();
    let response_failed = response.error.is_some();
    if let Some(error) = response.error {
        app.set_notice(format!("Search failed: {error}"));
    } else {
        app.clear_notice();
    }
    app.shell.runtime.total_match_count = response.total_match_count;
    app.shell.runtime.result_sort_mode = response.sort_mode;
    app.shell.runtime.result_sort_scope = response.sort_scope;
    app.replace_results_snapshot(response.results, false);
    app.shell.runtime.base_results_are_score_ranked = !response
        .sort_scope
        .sorts_all_matches_before_limit(response.sort_mode);
    if response.sort_scope == super::ResultSortScope::ShownResults
        && response.sort_mode != ResultSortMode::Score
    {
        apply_result_sort(app, false);
    }
    if !app.shell.indexing.in_progress && !response_failed {
        clear_unrestored_evicted_selection(app);
    }
    if matches!(app.shell.indexing.build.index.source, IndexSource::Walker) {
        // Search results can arrive after index completion. Queue kind resolution
        // from the newly installed result snapshot so deferred LINK entries are
        // not lost when the finish-time snapshot was stale.
        app.queue_unknown_kind_paths_for_visible_results();
    }
    if app.shell.indexing.search_rerun_pending
        && !app.shell.runtime.query_state.query.trim().is_empty()
        && app.shell.indexing.in_progress
        && app.should_refresh_incremental_search()
    {
        app.shell.indexing.search_rerun_pending = false;
        app.shell.indexing.search_resume_pending = false;
        app.shell.runtime.entries = Arc::new(
            app.shell
                .indexing
                .build
                .incremental_filtered_entries
                .clone(),
        );
        app.shell.indexing.last_search_snapshot_len = app.shell.runtime.entries.len();
        app.shell.indexing.last_incremental_results_refresh = Instant::now();
        app.enqueue_search_request();
    }
    true
}

pub(super) fn replace_results_snapshot(
    app: &mut FlistWalkerApp,
    results: Vec<(PathBuf, f64)>,
    keep_scroll_position: bool,
) {
    app.shell.worker_bus.sort.clear_request();
    app.shell.runtime.base_results = results.clone();
    app.shell.runtime.base_results_are_score_ranked = true;
    // Regression guard: search refreshes must keep the cursor on the same row number.
    // Following the previous path here makes the highlight jump when the query changes.
    apply_results_with_selection_policy(app, results, keep_scroll_position, false);
}

pub(super) fn invalidate_result_sort(app: &mut FlistWalkerApp, keep_scroll_position: bool) {
    let had_non_score_sort = app.shell.runtime.result_sort_mode != ResultSortMode::Score;
    app.shell.worker_bus.sort.clear_request();
    app.shell.runtime.result_sort_mode = ResultSortMode::Score;
    app.shell.runtime.result_sort_scope = super::ResultSortScope::ShownResults;
    if had_non_score_sort
        && !app.shell.runtime.base_results.is_empty()
        && app.shell.runtime.results != app.shell.runtime.base_results
    {
        apply_results_with_selection_policy(
            app,
            app.shell.runtime.base_results.clone(),
            keep_scroll_position,
            true,
        );
    } else {
        app.refresh_status_line();
    }
}

fn request_sort_metadata(
    app: &mut FlistWalkerApp,
    mode: ResultSortMode,
    missing_paths: Vec<PathBuf>,
) {
    let request_id = app.shell.worker_bus.sort.begin_request();
    app.bind_sort_request_to_current_tab(request_id);
    app.refresh_status_line();
    if app
        .shell
        .worker_bus
        .sort
        .tx
        .send(SortMetadataRequest {
            request_id,
            paths: missing_paths,
            mode,
        })
        .is_err()
    {
        app.shell.worker_bus.sort.clear_request();
        app.set_notice("Sort worker is unavailable");
    }
}

fn request_background_sort_metadata(
    app: &mut FlistWalkerApp,
    tab_id: u64,
    mode: ResultSortMode,
    paths: Vec<PathBuf>,
) {
    let request_id = app.shell.worker_bus.sort.next_request_id;
    app.shell.worker_bus.sort.next_request_id = request_id.saturating_add(1);
    let Some(index) = app.find_tab_index_by_id(tab_id) else {
        return;
    };
    if app
        .shell
        .worker_bus
        .sort
        .tx
        .send(SortMetadataRequest {
            request_id,
            paths,
            mode,
        })
        .is_err()
    {
        if let Some(tab) = app.shell.tabs.get_mut(index) {
            tab.notice = "Sort worker is unavailable".into();
        }
        return;
    }
    // Background completion must not borrow the active tab's pending sort slot.
    let tab = app.shell.tabs.get_mut(index).expect("live background tab");
    tab.result_state.pending_sort_request_id = Some(request_id);
    tab.result_state.sort_in_progress = true;
    app.bind_sort_request_to_tab(request_id, tab_id);
}

pub(super) fn apply_result_sort(app: &mut FlistWalkerApp, keep_scroll_position: bool) {
    if app.shell.runtime.result_sort_mode == ResultSortMode::Score
        && !app.shell.runtime.base_results_are_score_ranked
    {
        // AllMatches may have removed Score's best candidates before applying limit.
        // Sorting that subset cannot restore them; rebuild from current entries.
        app.shell.worker_bus.sort.clear_request();
        app.update_results();
        return;
    }
    if app.shell.runtime.result_sort_scope == super::ResultSortScope::AllMatches
        && app.shell.runtime.result_sort_mode != ResultSortMode::Score
    {
        app.shell.worker_bus.sort.clear_request();
        app.enqueue_search_request();
        return;
    }
    if app.shell.runtime.base_results.is_empty() {
        app.shell.worker_bus.sort.clear_request();
        app.refresh_status_line();
        return;
    }
    if !app.shell.runtime.result_sort_mode.uses_metadata() {
        let sorted = app.build_sorted_results(app.shell.runtime.result_sort_mode);
        app.shell.worker_bus.sort.clear_request();
        apply_results_with_selection_policy(app, sorted, keep_scroll_position, false);
        return;
    }

    let missing_paths = app
        .shell
        .runtime
        .base_results
        .iter()
        .map(|(path, _)| path.clone())
        .filter(|path| !app.shell.cache.sort_metadata.contains(path))
        .collect::<Vec<_>>();
    if missing_paths.is_empty() {
        let sorted = app.build_sorted_results(app.shell.runtime.result_sort_mode);
        app.shell.worker_bus.sort.clear_request();
        apply_results_with_selection_policy(app, sorted, keep_scroll_position, false);
        return;
    }

    request_sort_metadata(app, app.shell.runtime.result_sort_mode, missing_paths);
}

fn apply_changed_sort(app: &mut FlistWalkerApp, pending_search: bool) {
    if pending_search {
        // Preserve a pending query change while replacing its sort request identity.
        app.update_results();
        if app.shell.search.in_progress() {
            return;
        }
        // Empty-query Shown results are rebuilt without a worker response.
    }
    apply_result_sort(app, false);
}

pub(super) fn set_result_sort_mode(app: &mut FlistWalkerApp, mode: ResultSortMode) {
    if app.shell.runtime.result_sort_mode == mode {
        return;
    }
    app.shell.tabs.mark_active_tab_meaningfully_engaged();
    let pending_search = app.shell.search.in_progress();
    app.shell.search.clear_active_request_state();
    app.shell.worker_bus.sort.clear_request();
    app.shell.runtime.result_sort_mode = mode;
    apply_changed_sort(app, pending_search);
}

pub(super) fn set_result_sort_scope(app: &mut FlistWalkerApp, scope: super::ResultSortScope) {
    if app.shell.runtime.result_sort_scope == scope {
        return;
    }
    app.shell.tabs.mark_active_tab_meaningfully_engaged();
    let pending_search = app.shell.search.in_progress();
    app.shell.search.clear_active_request_state();
    app.shell.worker_bus.sort.clear_request();
    app.shell.runtime.result_sort_scope = scope;
    apply_changed_sort(app, pending_search);
}

pub(super) fn apply_background_preview_response(
    app: &mut FlistWalkerApp,
    response: PreviewResponse,
) {
    let Some(tab_id) = app.take_preview_request_tab(response.request_id) else {
        return;
    };
    let Some(tab_index) = app.find_tab_index_by_id(tab_id) else {
        return;
    };
    if tab_index == app.shell.tabs.active_tab_index() {
        return;
    }
    app.cache_preview(response.path.clone(), response.preview.clone());
    if let Some(tab) = app.shell.tabs.get_mut(tab_index) {
        tab.clear_preview_request_state();
        let current_path = if tab.result_state.results_compacted {
            tab.result_state.committed.current_row.and_then(|row| {
                tab.result_state
                    .committed
                    .base_results
                    .get(row)
                    .map(|(path, _)| path)
            })
        } else {
            tab.result_state.committed.current_row.and_then(|row| {
                tab.result_state
                    .committed
                    .results
                    .get(row)
                    .map(|(path, _)| path)
            })
        };
        if current_path.is_some_and(|current_path| *current_path == response.path) {
            tab.result_state.committed.preview = response.preview;
        }
    }
}

pub(super) fn apply_active_preview_response(
    app: &mut FlistWalkerApp,
    response: &PreviewResponse,
) -> bool {
    if Some(response.request_id) != app.shell.worker_bus.preview.pending_request_id {
        return false;
    }
    app.take_preview_request_tab(response.request_id);
    app.shell.worker_bus.preview.clear_request();
    app.cache_preview(response.path.clone(), response.preview.clone());
    if let Some(row) = app.shell.runtime.current_row {
        if let Some((current_path, _)) = app.shell.runtime.results.get(row) {
            if *current_path == response.path {
                app.shell.runtime.preview = response.preview.clone();
            }
        }
    }
    true
}

pub(super) fn apply_background_sort_response(
    app: &mut FlistWalkerApp,
    response: SortMetadataResponse,
) {
    let Some(tab_id) = app.take_sort_request_tab(response.request_id) else {
        return;
    };
    let Some(tab_index) = app.find_tab_index_by_id(tab_id) else {
        return;
    };
    if tab_index == app.shell.tabs.active_tab_index() {
        return;
    }
    let sort_metadata = app.shell.cache.sort_metadata.get_map().clone();
    let Some(tab) = app.shell.tabs.get_mut(tab_index) else {
        return;
    };
    if Some(response.request_id) != tab.result_state.pending_sort_request_id {
        return;
    }
    tab.result_state.clear_sort_request_state();
    if response.mode == tab.result_state.result_sort_mode {
        let previous_path = selected_tab_path(tab).cloned();
        tab.result_state.committed.results = FlistWalkerApp::build_sorted_results_from(
            &tab.result_state.committed.base_results,
            tab.result_state.result_sort_mode,
            &sort_metadata,
        );
        tab.result_state.results_compacted = false;
        if tab.result_state.committed.results.is_empty() {
            tab.result_state.committed.current_row = None;
            tab.result_state.committed.preview.clear();
            tab.clear_preview_request_state();
        } else {
            tab.result_state.committed.current_row = normalized_result_row(
                tab.result_state.committed.current_row,
                tab.result_state.committed.results.len(),
            );
        }
        let preview_invalidated =
            invalidate_background_preview_if_selection_changed(tab, previous_path.as_ref());
        FlistWalkerApp::trim_inactive_tab_preview(tab);
        if preview_invalidated {
            app.shell
                .tabs
                .clear_preview_response_routing_for_tab(tab_id);
        }
    }
}

pub(super) fn apply_active_sort_response(
    app: &mut FlistWalkerApp,
    response: &SortMetadataResponse,
) -> bool {
    if Some(response.request_id) != app.shell.worker_bus.sort.pending_request_id {
        return false;
    }
    app.take_sort_request_tab(response.request_id);
    app.shell.worker_bus.sort.clear_request();
    if response.mode == app.shell.runtime.result_sort_mode {
        apply_result_sort(app, false);
    } else {
        app.refresh_status_line();
    }
    true
}

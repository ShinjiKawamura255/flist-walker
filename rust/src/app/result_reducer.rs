use super::{
    AppTabState, FlistWalkerApp, PreviewResponse, ResultSortMode, SearchResponse,
    SortMetadata, SortMetadataRequest, SortMetadataResponse,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

fn clear_tab_result_selection(tab: &mut AppTabState) {
    tab.result_state.current_row = None;
    tab.result_state.preview.clear();
    tab.clear_preview_request_state();
}

fn clamp_tab_result_selection(tab: &mut AppTabState) {
    if tab.result_state.results.is_empty() {
        clear_tab_result_selection(tab);
        return;
    }
    let max_index = tab.result_state.results.len().saturating_sub(1);
    tab.result_state.current_row = tab
        .result_state
        .current_row
        .map(|row: usize| row.min(max_index));
}

pub(super) fn apply_results_with_selection_policy(
    app: &mut FlistWalkerApp,
    results: Vec<(PathBuf, f64)>,
    keep_scroll_position: bool,
    preserve_selected_path: bool,
) {
    fn clamp_row(current_row: Option<usize>, results_len: usize) -> Option<usize> {
        current_row.map(|row| row.min(results_len.saturating_sub(1)))
    }

    let selected_path = preserve_selected_path
        .then(|| {
            app.shell.runtime.current_row.and_then(|row| {
                app.shell
                    .runtime
                    .results
                    .get(row)
                    .map(|(path, _)| path.clone())
            })
        })
        .flatten();
    let previous_row = app.shell.runtime.current_row;
    app.shell.runtime.results = results;
    if app.shell.runtime.results.is_empty() {
        app.set_current_row(None);
        app.shell.runtime.preview.clear();
        app.shell.worker_bus.preview.clear_request();
    } else {
        let previous_row = clamp_row(previous_row, app.shell.runtime.results.len());
        app.set_current_row(
            selected_path
                .and_then(|selected| {
                    app.shell
                        .runtime
                        .results
                        .iter()
                        .position(|(path, _)| *path == selected)
                })
                .or(previous_row),
        );
        app.request_preview_for_current();
        if !keep_scroll_position {
            app.request_scroll_to_current();
        }
    }
    app.refresh_status_line();
}

pub(super) fn apply_background_search_response(
    app: &mut FlistWalkerApp,
    tab_id: u64,
    response: SearchResponse,
) {
    let Some(tab_index) = app.find_tab_index_by_id(tab_id) else {
        return;
    };
    let Some(tab) = app.shell.tabs.get_mut(tab_index) else {
        return;
    };
    tab.clear_search_request_state();
    tab.notice = response
        .error
        .map(|error| format!("Search failed: {error}"))
        .unwrap_or_default();
    tab.result_state.base_results = response.results.clone();
    tab.result_state.results = response.results;
    tab.result_state.results_compacted = false;
    tab.result_state.result_sort_mode = ResultSortMode::Score;
    tab.result_state.clear_sort_request_state();
    clamp_tab_result_selection(tab);
    FlistWalkerApp::compact_inactive_tab_state(tab);
}

pub(super) fn apply_active_search_response(
    app: &mut FlistWalkerApp,
    response: &SearchResponse,
) -> bool {
    if Some(response.request_id) != app.shell.search.pending_request_id() {
        return false;
    }
    app.shell.search.clear_active_request_state();
    if let Some(error) = &response.error {
        app.set_notice(format!("Search failed: {error}"));
    } else {
        app.clear_notice();
    }
    app.replace_results_snapshot(response.results.clone(), false);
    if app.shell.indexing.search_rerun_pending
        && !app.shell.runtime.query_state.query.trim().is_empty()
        && app.shell.indexing.in_progress
        && app.should_refresh_incremental_search()
    {
        app.shell.indexing.search_rerun_pending = false;
        app.shell.indexing.search_resume_pending = false;
        app.shell.runtime.entries =
            Arc::new(app.shell.indexing.incremental_filtered_entries.clone());
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
    app.shell.runtime.result_sort_mode = ResultSortMode::Score;
    app.shell.runtime.base_results = results.clone();
    // Regression guard: search refreshes must keep the cursor on the same row number.
    // Following the previous path here makes the highlight jump when the query changes.
    apply_results_with_selection_policy(app, results, keep_scroll_position, false);
}

pub(super) fn invalidate_result_sort(app: &mut FlistWalkerApp, keep_scroll_position: bool) {
    let had_non_score_sort = app.shell.runtime.result_sort_mode != ResultSortMode::Score;
    app.shell.worker_bus.sort.clear_request();
    app.shell.runtime.result_sort_mode = ResultSortMode::Score;
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

pub(super) fn apply_result_sort(app: &mut FlistWalkerApp, keep_scroll_position: bool) {
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

pub(super) fn set_result_sort_mode(app: &mut FlistWalkerApp, mode: ResultSortMode) {
    app.shell.runtime.result_sort_mode = mode;
    apply_result_sort(app, false);
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
    app.cache_preview(response.path.clone(), response.preview.clone());
    if let Some(tab) = app.shell.tabs.get_mut(tab_index) {
        tab.clear_preview_request_state();
        let current_path = if tab.result_state.results_compacted {
            tab.result_state
                .current_row
                .and_then(|row| tab.result_state.base_results.get(row).map(|(path, _)| path))
        } else {
            tab.result_state
                .current_row
                .and_then(|row| tab.result_state.results.get(row).map(|(path, _)| path))
        };
        if current_path.is_some_and(|current_path| *current_path == response.path) {
            tab.result_state.preview = response.preview;
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
    let sort_metadata = app.shell.cache.sort_metadata.get_map().clone();
    let Some(tab) = app.shell.tabs.get_mut(tab_index) else {
        return;
    };
    if Some(response.request_id) != tab.result_state.pending_sort_request_id {
        return;
    }
    tab.result_state.clear_sort_request_state();
    if response.mode == tab.result_state.result_sort_mode {
        tab.result_state.results = FlistWalkerApp::build_sorted_results_from(
            &tab.result_state.base_results,
            tab.result_state.result_sort_mode,
            &sort_metadata,
        );
        tab.result_state.results_compacted = false;
        if tab.result_state.results.is_empty() {
            tab.result_state.current_row = None;
            tab.result_state.preview.clear();
            tab.clear_preview_request_state();
        } else {
            let max_index = tab.result_state.results.len().saturating_sub(1);
            tab.result_state.current_row = tab
                .result_state
                .current_row
                .map(|row: usize| row.min(max_index));
        }
        FlistWalkerApp::compact_inactive_tab_state(tab);
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

use super::*;
use crate::app::tab_state::AppTabState;

const PAYLOAD_LEN: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PayloadAllocations {
    index_entries: (*const Entry, usize),
    pending_index_entries: (*const IndexEntry, usize),
    pending_kind_paths: (*const PathBuf, usize),
    resolved_kind_updates: (*const (PathBuf, EntryKind), usize),
    incremental_filtered_entries: (*const Entry, usize),
    base_results: (*const (PathBuf, f64), usize),
    results: (*const (PathBuf, f64), usize),
    entry_kind_cache: (*const PathBuf, usize),
}

fn entries(prefix: &str) -> Vec<Entry> {
    (0..PAYLOAD_LEN)
        .map(|index| file_entry(PathBuf::from(format!("{prefix}-{index}.txt"))))
        .collect()
}

fn pending_entries(prefix: &str) -> VecDeque<IndexEntry> {
    (0..PAYLOAD_LEN)
        .map(|index| IndexEntry {
            path: PathBuf::from(format!("{prefix}-pending-{index}.txt")),
            kind: EntryKind::file(),
            kind_known: true,
        })
        .collect()
}

fn kind_paths(prefix: &str) -> VecDeque<PathBuf> {
    (0..PAYLOAD_LEN)
        .map(|index| PathBuf::from(format!("{prefix}-kind-{index}.txt")))
        .collect()
}

fn kind_updates(prefix: &str) -> Vec<(PathBuf, EntryKind)> {
    (0..PAYLOAD_LEN)
        .map(|index| {
            (
                PathBuf::from(format!("{prefix}-resolved-{index}.txt")),
                EntryKind::file(),
            )
        })
        .collect()
}

fn results(prefix: &str) -> Vec<(PathBuf, f64)> {
    (0..PAYLOAD_LEN)
        .map(|index| {
            (
                PathBuf::from(format!("{prefix}-result-{index}.txt")),
                index as f64,
            )
        })
        .collect()
}

fn seed_live_payload(app: &mut FlistWalkerApp, prefix: &str, request_id: u64) {
    app.shell.runtime.index.entries = entries(prefix);
    app.shell.indexing.pending_entries = pending_entries(prefix);
    app.shell.indexing.pending_kind_paths = kind_paths(prefix);
    app.shell.indexing.resolved_kind_updates = kind_updates(prefix);
    app.shell.indexing.incremental_filtered_entries = entries(&format!("{prefix}-incremental"));
    app.shell.runtime.base_results = results(&format!("{prefix}-base"));
    app.shell.runtime.results = results(prefix);
    app.shell.cache.entry_kind.clear();
    for index in 0..PAYLOAD_LEN {
        app.shell.cache.entry_kind.set(
            PathBuf::from(format!("{prefix}-cached-{index}.txt")),
            EntryKind::file(),
        );
    }
    app.shell.search.set_pending_request_id(Some(request_id));
    app.shell.search.set_in_progress(true);
}

fn seed_tab_payload(tab: &mut AppTabState, prefix: &str, request_id: u64) {
    tab.index_state.index.entries = entries(prefix);
    tab.index_state.pending_index_entries = pending_entries(prefix);
    tab.index_state.pending_kind_paths = kind_paths(prefix);
    tab.index_state.resolved_kind_updates = kind_updates(prefix);
    tab.index_state.incremental_filtered_entries = entries(&format!("{prefix}-incremental"));
    tab.result_state.base_results = results(&format!("{prefix}-base"));
    tab.result_state.results = results(prefix);
    tab.entry_kind_cache.clear();
    for index in 0..PAYLOAD_LEN {
        tab.entry_kind_cache.set(
            PathBuf::from(format!("{prefix}-cached-{index}.txt")),
            EntryKind::file(),
        );
    }
    tab.pending_request_id = Some(request_id);
    tab.search_in_progress = true;
}

fn deque_allocation<T>(deque: &VecDeque<T>) -> (*const T, usize) {
    let (head, tail) = deque.as_slices();
    assert!(tail.is_empty(), "fixture deque must remain contiguous");
    (head.as_ptr(), deque.capacity())
}

fn live_allocations(app: &FlistWalkerApp) -> PayloadAllocations {
    PayloadAllocations {
        index_entries: (
            app.shell.runtime.index.entries.as_ptr(),
            app.shell.runtime.index.entries.capacity(),
        ),
        pending_index_entries: deque_allocation(&app.shell.indexing.pending_entries),
        pending_kind_paths: deque_allocation(&app.shell.indexing.pending_kind_paths),
        resolved_kind_updates: (
            app.shell.indexing.resolved_kind_updates.as_ptr(),
            app.shell.indexing.resolved_kind_updates.capacity(),
        ),
        incremental_filtered_entries: (
            app.shell.indexing.incremental_filtered_entries.as_ptr(),
            app.shell.indexing.incremental_filtered_entries.capacity(),
        ),
        base_results: (
            app.shell.runtime.base_results.as_ptr(),
            app.shell.runtime.base_results.capacity(),
        ),
        results: (
            app.shell.runtime.results.as_ptr(),
            app.shell.runtime.results.capacity(),
        ),
        entry_kind_cache: (
            app.shell
                .cache
                .entry_kind
                .entries
                .keys()
                .next()
                .expect("live cache key") as *const PathBuf,
            app.shell.cache.entry_kind.entries.capacity(),
        ),
    }
}

fn tab_allocations(tab: &AppTabState) -> PayloadAllocations {
    PayloadAllocations {
        index_entries: (
            tab.index_state.index.entries.as_ptr(),
            tab.index_state.index.entries.capacity(),
        ),
        pending_index_entries: deque_allocation(&tab.index_state.pending_index_entries),
        pending_kind_paths: deque_allocation(&tab.index_state.pending_kind_paths),
        resolved_kind_updates: (
            tab.index_state.resolved_kind_updates.as_ptr(),
            tab.index_state.resolved_kind_updates.capacity(),
        ),
        incremental_filtered_entries: (
            tab.index_state.incremental_filtered_entries.as_ptr(),
            tab.index_state.incremental_filtered_entries.capacity(),
        ),
        base_results: (
            tab.result_state.base_results.as_ptr(),
            tab.result_state.base_results.capacity(),
        ),
        results: (
            tab.result_state.results.as_ptr(),
            tab.result_state.results.capacity(),
        ),
        entry_kind_cache: (
            tab.entry_kind_cache
                .entries
                .keys()
                .next()
                .expect("tab cache key") as *const PathBuf,
            tab.entry_kind_cache.entries.capacity(),
        ),
    }
}

#[test]
fn tc_154_tab_switch_transfers_large_payload_allocations_in_both_directions() {
    let root = test_root("tc-154-tab-payload-transfer");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), PAYLOAD_LEN, String::new());
    app.create_new_tab();

    seed_live_payload(&mut app, "active", 1541);
    seed_tab_payload(
        app.shell.tabs.get_mut(0).expect("inactive tab"),
        "inactive",
        1540,
    );
    let active_allocations = live_allocations(&app);
    let inactive_allocations = tab_allocations(app.shell.tabs.get(0).expect("inactive tab"));

    app.switch_to_tab_index(0);

    assert_eq!(live_allocations(&app), inactive_allocations);
    assert_eq!(
        tab_allocations(app.shell.tabs.get(1).expect("outgoing tab")),
        active_allocations
    );

    app.switch_to_tab_index(1);

    assert_eq!(live_allocations(&app), active_allocations);
    assert_eq!(
        tab_allocations(app.shell.tabs.get(0).expect("outgoing tab")),
        inactive_allocations
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_154_active_request_state_moves_to_background_slot_and_back() {
    let root = test_root("tc-154-active-request-transfer");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    let active_id = app.current_tab_id().expect("active tab id");

    app.shell.search.set_pending_request_id(Some(1541));
    app.shell.search.set_in_progress(true);
    app.shell.indexing.pending_request_id = Some(1542);
    app.shell.indexing.in_progress = true;
    app.shell.worker_bus.preview.pending_request_id = Some(1543);
    app.shell.worker_bus.preview.in_progress = true;
    app.shell.worker_bus.action.pending_request_id = Some(1544);
    app.shell.worker_bus.action.in_progress = true;
    app.shell.worker_bus.sort.pending_request_id = Some(1545);
    app.shell.worker_bus.sort.in_progress = true;
    app.bind_preview_request_to_tab(1543, active_id);
    app.bind_action_request_to_tab(1544, active_id);
    app.bind_sort_request_to_tab(1545, active_id);

    assert_eq!(
        app.shell
            .tabs
            .get(1)
            .expect("active scratch")
            .pending_action_request_id,
        None
    );
    app.switch_to_tab_index(0);

    let background = app.shell.tabs.get(1).expect("background tab");
    assert_eq!(background.pending_request_id, Some(1541));
    assert!(background.search_in_progress);
    assert_eq!(background.index_state.pending_index_request_id, Some(1542));
    assert!(background.index_state.index_in_progress);
    assert_eq!(background.pending_preview_request_id, Some(1543));
    assert!(background.preview_in_progress);
    assert_eq!(background.pending_action_request_id, Some(1544));
    assert!(background.action_in_progress);
    assert_eq!(background.result_state.pending_sort_request_id, Some(1545));
    assert!(background.result_state.sort_in_progress);

    app.switch_to_tab_index(1);

    assert_eq!(app.shell.search.pending_request_id(), Some(1541));
    assert!(app.shell.search.in_progress());
    assert_eq!(app.shell.indexing.pending_request_id, Some(1542));
    assert!(app.shell.indexing.in_progress);
    assert_eq!(app.shell.worker_bus.preview.pending_request_id, Some(1543));
    assert!(app.shell.worker_bus.preview.in_progress);
    assert_eq!(app.shell.worker_bus.action.pending_request_id, Some(1544));
    assert!(app.shell.worker_bus.action.in_progress);
    assert_eq!(app.shell.worker_bus.sort.pending_request_id, Some(1545));
    assert!(app.shell.worker_bus.sort.in_progress);
    assert_eq!(app.preview_request_tab(1543), Some(active_id));
    assert_eq!(app.action_request_tab(1544), Some(active_id));
    assert_eq!(app.sort_request_tab(1545), Some(active_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_154_stale_background_routes_never_mutate_active_scratch() {
    let root = test_root("tc-154-stale-active-owner-guard");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    let active_index = app.shell.tabs.active_tab_index();
    let active_id = app.current_tab_id().expect("active tab id");
    let scratch_path = root.join("scratch.txt");
    let stale_path = root.join("stale.txt");

    {
        let scratch = app
            .shell
            .tabs
            .get_mut(active_index)
            .expect("active scratch");
        scratch.index_state.index.entries = vec![file_entry(scratch_path.clone())];
        scratch.result_state.base_results = vec![(scratch_path.clone(), 1.0)];
        scratch.result_state.results = scratch.result_state.base_results.clone();
        scratch.result_state.preview = "scratch preview".to_string();
        scratch.notice = "scratch notice".to_string();
        scratch.pending_request_id = Some(2001);
        scratch.pending_preview_request_id = Some(2002);
        scratch.pending_action_request_id = Some(2003);
        scratch.result_state.pending_sort_request_id = Some(2004);
    }

    for response in [
        IndexResponse::Batch {
            request_id: 2101,
            entries: vec![IndexEntry {
                path: stale_path.clone(),
                kind: EntryKind::file(),
                kind_known: true,
            }],
        },
        IndexResponse::ReplaceAll {
            request_id: 2102,
            entries: vec![IndexEntry {
                path: stale_path.clone(),
                kind: EntryKind::file(),
                kind_known: true,
            }],
        },
        IndexResponse::Finished {
            request_id: 2103,
            source: IndexSource::Walker,
        },
    ] {
        let request_id = match &response {
            IndexResponse::Batch { request_id, .. }
            | IndexResponse::ReplaceAll { request_id, .. }
            | IndexResponse::Finished { request_id, .. } => *request_id,
            _ => unreachable!("fixture only uses batch/replace-all/finished"),
        };
        let effect = app.apply_background_index_response(active_index, response);
        assert_eq!(effect.cleanup_request_id, Some(request_id));
    }

    app.apply_background_search_response(
        active_id,
        SearchResponse {
            request_id: 2201,
            results: vec![(stale_path.clone(), 9.0)],
            total_match_count: 1,
            sort_mode: ResultSortMode::Score,
            sort_scope: ResultSortScope::ShownResults,
            error: None,
        },
    );

    app.bind_preview_request_to_tab(2202, active_id);
    app.apply_background_preview_response(PreviewResponse {
        request_id: 2202,
        path: stale_path.clone(),
        preview: "stale preview".to_string(),
    });

    app.bind_action_request_to_tab(2203, active_id);
    app.apply_background_action_response(ActionResponse {
        request_id: 2203,
        notice: "stale notice".to_string(),
    });

    app.bind_sort_request_to_tab(2204, active_id);
    app.apply_background_sort_response(SortMetadataResponse {
        request_id: 2204,
        entries: Vec::new(),
        mode: ResultSortMode::NameAsc,
    });

    let scratch = app.shell.tabs.get(active_index).expect("active scratch");
    assert_eq!(scratch.index_state.index.entries[0], scratch_path);
    assert_eq!(scratch.result_state.base_results[0].0, scratch_path);
    assert_eq!(scratch.result_state.results[0].0, scratch_path);
    assert_eq!(scratch.result_state.preview, "scratch preview");
    assert_eq!(scratch.notice, "scratch notice");
    assert_eq!(scratch.pending_request_id, Some(2001));
    assert_eq!(scratch.pending_preview_request_id, Some(2002));
    assert_eq!(scratch.pending_action_request_id, Some(2003));
    assert_eq!(scratch.result_state.pending_sort_request_id, Some(2004));
    let _ = fs::remove_dir_all(&root);
}

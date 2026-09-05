use super::*;

fn collect_drop_threads_until(
    drop_rx: &mpsc::Receiver<String>,
    expected: &str,
    timeout: Duration,
) -> Vec<String> {
    let deadline = Instant::now() + timeout;
    let mut threads = drop_rx.try_iter().collect::<Vec<_>>();
    while !threads.iter().any(|name| name == expected) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match drop_rx.recv_timeout(remaining) {
            Ok(name) => threads.push(name),
            Err(_) => break,
        }
    }
    threads
}

const BACKGROUND_FINALIZATION_FRAME_LIMIT: usize = 2_000;
const BACKGROUND_FINALIZATION_STALL_LIMIT: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BackgroundFinalizationProgress {
    initial_remaining: usize,
    pending_remaining: usize,
    continuation_remaining: usize,
    completed: usize,
    filter_cursor: usize,
    filtered: usize,
    kind_cursor: usize,
    unresolved_kinds: usize,
    scratch_reclaimed: bool,
}

#[derive(Clone, Debug)]
struct BackgroundFinalizationFrame {
    elapsed: Duration,
    before: Option<BackgroundFinalizationProgress>,
    after: Option<BackgroundFinalizationProgress>,
}

#[derive(Clone, Copy)]
enum BackgroundFinalizationTarget {
    Removed,
    FilterCursorAtLeast(usize),
    TabFinishSettled,
}

fn background_finalization_progress(
    app: &FlistWalkerApp,
    request_id: u64,
) -> Option<BackgroundFinalizationProgress> {
    app.shell
        .indexing
        .background_finalizations
        .get(&request_id)
        .map(|state| BackgroundFinalizationProgress {
            initial_remaining: state.initial_entries.len(),
            pending_remaining: state.pending_entries.len(),
            continuation_remaining: state.continuation_entries.len(),
            completed: state.completed_entries.len(),
            filter_cursor: state.filter_cursor,
            filtered: state.filtered_entries.as_ref().map_or(0, Vec::len),
            kind_cursor: state.kind_cursor,
            unresolved_kinds: state.unresolved_kind_paths.len(),
            scratch_reclaimed: state.scratch_reclaimed,
        })
}

fn background_finalization_target_reached(
    app: &FlistWalkerApp,
    tab_index: usize,
    request_id: u64,
    target: BackgroundFinalizationTarget,
) -> bool {
    match target {
        BackgroundFinalizationTarget::Removed => !app
            .shell
            .indexing
            .background_finalizations
            .contains_key(&request_id),
        BackgroundFinalizationTarget::FilterCursorAtLeast(minimum) => app
            .shell
            .indexing
            .background_finalizations
            .get(&request_id)
            .is_some_and(|state| state.filter_cursor >= minimum),
        BackgroundFinalizationTarget::TabFinishSettled => app
            .shell
            .tabs
            .get(tab_index)
            .is_some_and(|tab| tab.index_state.pending_index_finish.is_none()),
    }
}

fn assert_background_finalization_progress(
    request_id: u64,
    frame: usize,
    consecutive_stalled_frames: usize,
    before: Option<BackgroundFinalizationProgress>,
) {
    assert!(
        consecutive_stalled_frames <= BACKGROUND_FINALIZATION_STALL_LIMIT,
        "background finalization stalled: request_id={request_id}, frame={frame}, consecutive_stalled_frames={consecutive_stalled_frames}, progress={before:?}"
    );
}

fn drive_background_finalization_with_frame_budget(
    app: &mut FlistWalkerApp,
    tab_index: usize,
    request_id: u64,
    source: IndexSource,
    target: BackgroundFinalizationTarget,
) -> Vec<BackgroundFinalizationFrame> {
    let mut frames = Vec::new();
    let mut consecutive_stalled_frames = 0usize;
    for frame in 0..BACKGROUND_FINALIZATION_FRAME_LIMIT {
        if background_finalization_target_reached(app, tab_index, request_id, target) {
            return frames;
        }
        let before = background_finalization_progress(app, request_id);
        let started = Instant::now();
        app.handle_background_index_response(
            tab_index,
            IndexResponse::Finished {
                request_id,
                source: source.clone(),
            },
        );
        let elapsed = started.elapsed();
        let after = background_finalization_progress(app, request_id);
        if !background_finalization_target_reached(app, tab_index, request_id, target) {
            consecutive_stalled_frames = if before == after {
                consecutive_stalled_frames.saturating_add(1)
            } else {
                0
            };
            assert_background_finalization_progress(
                request_id,
                frame,
                consecutive_stalled_frames,
                before,
            );
        }
        frames.push(BackgroundFinalizationFrame {
            elapsed,
            before,
            after,
        });
    }
    panic!(
        "background finalization exceeded frame bound: request_id={request_id}, tab_index={tab_index}, frames={BACKGROUND_FINALIZATION_FRAME_LIMIT}, progress={:?}",
        background_finalization_progress(app, request_id)
    );
}

fn settle_background_finish_with_frame_budget(
    app: &mut FlistWalkerApp,
    tab_index: usize,
    request_id: u64,
    source: IndexSource,
) -> Vec<Duration> {
    drive_background_finalization_with_frame_budget(
        app,
        tab_index,
        request_id,
        source,
        BackgroundFinalizationTarget::TabFinishSettled,
    )
    .into_iter()
    .map(|frame| frame.elapsed)
    .collect()
}

#[test]
fn tc_207_stalled_background_finalization_guard_fails_deterministically_regression() {
    let stalled = BackgroundFinalizationProgress {
        initial_remaining: 10,
        pending_remaining: 0,
        continuation_remaining: 0,
        completed: 0,
        filter_cursor: 0,
        filtered: 0,
        kind_cursor: 0,
        unresolved_kinds: 0,
        scratch_reclaimed: false,
    };
    let failure = std::panic::catch_unwind(|| {
        assert_background_finalization_progress(
            207,
            BACKGROUND_FINALIZATION_STALL_LIMIT,
            BACKGROUND_FINALIZATION_STALL_LIMIT + 1,
            Some(stalled),
        );
    });
    assert!(failure.is_err());
}

#[test]
fn background_tab_search_and_preview_responses_are_retained() {
    let root = test_root("background-tab-search-preview");
    fs::create_dir_all(&root).expect("create dir");
    let selected = root.join("picked.txt");
    fs::write(&selected, "hello").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "picked".to_string());
    app.shell.indexing.in_progress = false;
    app.shell.indexing.pending_request_id = None;
    app.shell.runtime.entries = Arc::new(vec![file_entry(selected.clone())]);
    app.shell.runtime.results = vec![(selected.clone(), 0.0)];
    app.shell.runtime.current_row = Some(0);
    app.set_entry_kind(&selected, EntryKind::file());

    let (search_tx_req, _search_rx_req) = mpsc::channel::<SearchRequest>();
    let (search_tx_res, search_rx_res) = mpsc::channel::<SearchResponse>();
    app.shell.search.tx = search_tx_req;
    app.shell.search.rx = search_rx_res;
    app.enqueue_search_request();
    let search_request_id = app
        .shell
        .search
        .pending_request_id()
        .expect("search request id");
    let first_tab_id = app.shell.tabs.get(0).expect("tab 0").id;

    let (preview_tx_req, _preview_rx_req) = mpsc::channel::<PreviewRequest>();
    let (preview_tx_res, preview_rx_res) = mpsc::channel::<PreviewResponse>();
    app.shell.worker_bus.preview.tx = preview_tx_req;
    app.shell.worker_bus.preview.rx = preview_rx_res;
    app.request_preview_for_current();
    let preview_request_id = app
        .shell
        .worker_bus
        .preview
        .pending_request_id
        .expect("preview request id");

    app.create_new_tab();
    assert_eq!(app.shell.tabs.active_tab, 1);

    search_tx_res
        .send(SearchResponse {
            request_id: search_request_id,
            results: vec![(selected.clone(), 9.0)],
            total_match_count: 1,
            sort_mode: ResultSortMode::Score,
            sort_scope: ResultSortScope::ShownResults,
            error: None,
        })
        .expect("send search response");
    preview_tx_res
        .send(PreviewResponse {
            canceled: false,
            request_id: preview_request_id,
            path: selected.clone(),
            preview: "preview-body".to_string(),
        })
        .expect("send preview response");
    app.poll_search_response();
    app.poll_preview_response();

    let first_tab = app
        .shell
        .tabs
        .iter()
        .find(|tab| tab.id == first_tab_id)
        .expect("first tab");
    assert_eq!(first_tab.result_state.committed.results.len(), 1);
    assert!(!first_tab.result_state.results_compacted);
    assert_eq!(first_tab.result_state.committed.base_results.len(), 1);
    assert_eq!(first_tab.result_state.committed.base_results[0].0, selected);
    assert_eq!(first_tab.result_state.committed.preview, "preview-body");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn background_search_selection_change_invalidates_old_preview_and_reloads_on_activation_regression()
{
    let root = test_root("background-search-preview-ownership");
    fs::create_dir_all(&root).expect("create dir");
    let old_path = root.join("old.txt");
    let new_path = root.join("new.txt");
    fs::write(&old_path, "old").expect("write old");
    fs::write(&new_path, "new").expect("write new");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "new".to_string());
    app.shell.indexing.in_progress = false;
    app.shell.indexing.pending_request_id = None;
    app.shell.runtime.entries = Arc::new(vec![
        file_entry(old_path.clone()),
        file_entry(new_path.clone()),
    ]);
    app.shell.runtime.results = vec![(old_path.clone(), 1.0)];
    app.shell.runtime.base_results = app.shell.runtime.results.clone();
    app.shell.runtime.current_row = Some(0);
    app.set_entry_kind(&old_path, EntryKind::file());
    app.set_entry_kind(&new_path, EntryKind::file());
    let (preview_tx, preview_rx) = mpsc::channel::<PreviewRequest>();
    app.shell.worker_bus.preview.tx = preview_tx;
    app.request_preview_for_current();
    let old_request = preview_rx.try_recv().expect("old preview request");
    let background_tab_id = app.current_tab_id().expect("background tab id");

    app.create_new_tab();
    while preview_rx.try_recv().is_ok() {}
    app.apply_background_search_response(
        background_tab_id,
        SearchResponse {
            request_id: 91,
            results: vec![(new_path.clone(), 9.0)],
            total_match_count: 1,
            sort_mode: ResultSortMode::Score,
            sort_scope: ResultSortScope::ShownResults,
            error: None,
        },
    );

    let background = app.shell.tabs.get(0).expect("background tab");
    assert!(background.result_state.committed.preview.is_empty());
    assert!(background.pending_preview_request_id.is_none());
    assert!(background.preview_reload_pending);
    assert_eq!(app.preview_request_tab(old_request.request_id), None);
    app.apply_background_preview_response(PreviewResponse {
        canceled: false,
        request_id: old_request.request_id,
        path: old_path,
        preview: "late old preview".to_string(),
    });
    app.shell
        .tabs
        .get_mut(0)
        .expect("background tab")
        .index_state
        .build
        .entry_kind_cache
        .set(new_path.clone(), EntryKind::file());

    app.switch_to_tab_index(0);
    let activated_requests = preview_rx.try_iter().collect::<Vec<_>>();
    assert!(
        activated_requests
            .iter()
            .any(|request| request.path == new_path),
        "activation must request the background tab's new selected path; results={:?}, preview={}",
        app.shell.runtime.results,
        app.shell.runtime.preview
    );
    assert_eq!(app.shell.runtime.preview, "Loading preview...");
    assert!(
        !app.shell
            .tabs
            .get(0)
            .expect("activated tab slot")
            .preview_reload_pending
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tab_activation_without_background_selection_change_does_not_request_preview_regression() {
    let root = test_root("tab-activation-preview-noop");
    fs::create_dir_all(&root).expect("create dir");
    let selected = root.join("selected.txt");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.indexing.in_progress = false;
    app.shell.indexing.pending_request_id = None;
    app.shell.runtime.entries = Arc::new(vec![file_entry(selected.clone())]);
    app.shell.runtime.results = vec![(selected.clone(), 1.0)];
    app.shell.runtime.base_results = app.shell.runtime.results.clone();
    app.shell.runtime.current_row = Some(0);
    app.set_entry_kind(&selected, EntryKind::file());
    // Keep this fixture non-compacting so it isolates activation from the
    // separate result-restoration path, which intentionally refreshes preview.
    app.shell.search.set_pending_request_id(Some(801));
    app.shell.search.set_in_progress(true);
    let (preview_tx, preview_rx) = mpsc::channel::<PreviewRequest>();
    app.shell.worker_bus.preview.tx = preview_tx;

    app.create_new_tab();
    let create_requests = preview_rx
        .try_iter()
        .map(|request| request.path)
        .collect::<Vec<_>>();
    assert!(
        create_requests.is_empty(),
        "ordinary tab creation must not request preview: {create_requests:?}"
    );
    assert!(
        !app.shell
            .tabs
            .get(0)
            .expect("ordinary background tab")
            .preview_reload_pending
    );
    app.switch_to_tab_index(0);

    let unexpected = preview_rx
        .try_iter()
        .map(|request| request.path)
        .collect::<Vec<_>>();
    assert!(
        unexpected.is_empty(),
        "ordinary activation must not request preview: {unexpected:?}"
    );
    assert!(app.shell.runtime.preview.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn sizedesc_inactive_completed_preview_roundtrips_via_explicit_reload_regression() {
    let root = test_root("sizedesc-preview-roundtrip");
    fs::create_dir_all(&root).expect("create dir");
    let selected = root.join("selected.txt");
    fs::write(&selected, "selected").expect("write fixture");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.indexing.in_progress = false;
    app.shell.indexing.pending_request_id = None;
    app.shell.runtime.entries = Arc::new(vec![file_entry(selected.clone())]);
    app.shell.runtime.base_results = vec![(selected.clone(), 1.0)];
    app.shell.runtime.results = app.shell.runtime.base_results.clone();
    app.shell.runtime.current_row = Some(0);
    app.shell.runtime.result_sort_mode = ResultSortMode::SizeDesc;
    app.shell.runtime.preview = "completed preview".to_string();
    app.set_entry_kind(&selected, EntryKind::file());
    let (preview_tx, preview_rx) = mpsc::channel::<PreviewRequest>();
    app.shell.worker_bus.preview.tx = preview_tx;

    app.create_new_tab();
    let background = app.shell.tabs.get(0).expect("background tab");
    assert!(!background.result_state.results_compacted);
    assert!(background.result_state.committed.preview.is_empty());
    assert!(background.preview_reload_pending);
    while preview_rx.try_recv().is_ok() {}

    app.switch_to_tab_index(0);
    assert_eq!(
        preview_rx
            .try_recv()
            .expect("activation preview reload")
            .path,
        selected
    );
    assert!(
        !app.shell
            .tabs
            .get(0)
            .expect("active tab")
            .preview_reload_pending
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn background_none_to_some_selection_rejects_late_preview_and_reloads_regression() {
    let root = test_root("background-none-some-preview");
    fs::create_dir_all(&root).expect("create dir");
    let selected = root.join("selected.txt");
    fs::write(&selected, "selected").expect("write fixture");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "selected".to_string());
    app.shell.indexing.in_progress = false;
    app.shell.indexing.pending_request_id = None;
    app.shell.runtime.entries = Arc::new(vec![file_entry(selected.clone())]);
    app.shell.runtime.results.clear();
    app.shell.runtime.base_results.clear();
    app.shell.runtime.current_row = None;
    let background_tab_id = app.current_tab_id().expect("background tab id");
    app.shell.worker_bus.preview.pending_request_id = Some(711);
    app.shell.worker_bus.preview.in_progress = true;
    app.bind_preview_request_to_tab(711, background_tab_id);

    app.create_new_tab();
    app.apply_background_search_response(
        background_tab_id,
        SearchResponse {
            request_id: 712,
            results: vec![(selected.clone(), 9.0)],
            total_match_count: 1,
            sort_mode: ResultSortMode::Score,
            sort_scope: ResultSortScope::ShownResults,
            error: None,
        },
    );
    let background = app.shell.tabs.get(0).expect("background tab");
    assert!(background.preview_reload_pending);
    assert!(background.pending_preview_request_id.is_none());
    assert_eq!(app.preview_request_tab(711), None);
    app.apply_background_preview_response(PreviewResponse {
        canceled: false,
        request_id: 711,
        path: selected.clone(),
        preview: "late preview".to_string(),
    });
    assert!(app
        .shell
        .tabs
        .get(0)
        .expect("background tab")
        .result_state
        .committed
        .preview
        .is_empty());

    app.shell
        .tabs
        .get_mut(0)
        .expect("background tab")
        .index_state
        .build
        .entry_kind_cache
        .set(selected.clone(), EntryKind::file());
    let (preview_tx, preview_rx) = mpsc::channel::<PreviewRequest>();
    app.shell.worker_bus.preview.tx = preview_tx;
    app.switch_to_tab_index(0);
    assert_eq!(
        preview_rx
            .try_recv()
            .expect("activation preview request")
            .path,
        selected
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn background_sort_reorder_invalidates_old_preview_request_regression() {
    let root = test_root("background-sort-preview-ownership");
    fs::create_dir_all(&root).expect("create dir");
    let old_path = root.join("old.txt");
    let new_path = root.join("new.txt");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "item".to_string());
    app.shell.indexing.in_progress = false;
    app.shell.indexing.pending_request_id = None;
    app.shell.runtime.base_results = vec![(old_path.clone(), 2.0), (new_path.clone(), 1.0)];
    app.shell.runtime.results = app.shell.runtime.base_results.clone();
    app.shell.runtime.current_row = Some(0);
    app.shell.runtime.preview = "old preview".to_string();
    app.shell.runtime.result_sort_mode = ResultSortMode::SizeDesc;
    app.shell.worker_bus.sort.pending_request_id = Some(93);
    app.shell.worker_bus.sort.in_progress = true;
    app.shell.worker_bus.preview.pending_request_id = Some(94);
    app.shell.worker_bus.preview.in_progress = true;
    let background_tab_id = app.current_tab_id().expect("background tab id");
    app.bind_sort_request_to_tab(93, background_tab_id);
    app.bind_preview_request_to_tab(94, background_tab_id);
    app.cache_sort_metadata(
        old_path.clone(),
        SortMetadata {
            size_bytes: Some(1),
            ..SortMetadata::default()
        },
    );
    app.cache_sort_metadata(
        new_path.clone(),
        SortMetadata {
            size_bytes: Some(2),
            ..SortMetadata::default()
        },
    );

    app.create_new_tab();
    app.apply_background_sort_response(SortMetadataResponse {
        request_id: 93,
        entries: Vec::new(),
        mode: ResultSortMode::SizeDesc,
    });

    let background = app.shell.tabs.get(0).expect("background tab");
    assert_eq!(
        background.result_state.committed.base_results[0].0,
        old_path
    );
    assert_eq!(background.result_state.committed.results[0].0, new_path);
    assert!(background.result_state.committed.preview.is_empty());
    assert!(background.pending_preview_request_id.is_none());
    assert!(background.preview_reload_pending);
    assert_eq!(app.preview_request_tab(94), None);

    app.shell
        .tabs
        .get_mut(0)
        .expect("background tab")
        .index_state
        .build
        .entry_kind_cache
        .set(new_path.clone(), EntryKind::file());
    let (preview_tx, preview_rx) = mpsc::channel::<PreviewRequest>();
    app.shell.worker_bus.preview.tx = preview_tx;
    app.switch_to_tab_index(0);
    assert_eq!(
        preview_rx.try_recv().expect("sort activation preview").path,
        new_path
    );
    assert!(
        !app.shell
            .tabs
            .get(0)
            .expect("activated tab slot")
            .preview_reload_pending
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn background_tab_switch_does_not_stop_indexing_progress() {
    let root = test_root("background-tab-indexing-progress");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.indexing.in_progress = true;
    app.create_new_tab();

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: tab_switch_shortcut_modifiers(true),
        }],
    );

    assert!(app.shell.indexing.in_progress);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn background_tab_index_batches_do_not_override_active_tab_entries() {
    let root = test_root("background-tab-index-isolation");
    fs::create_dir_all(&root).expect("create dir");
    let active_file = root.join("active.txt");
    let indexed_file = root.join("indexed.txt");
    fs::write(&active_file, "a").expect("write active");
    fs::write(&indexed_file, "b").expect("write indexed");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (index_req_tx, index_req_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = index_req_tx;
    let (index_res_tx, index_res_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = index_res_rx;
    reset_index_request_state_for_test(&mut app);

    app.request_index_refresh();
    let index_req = index_req_rx.try_recv().expect("index request");
    app.shell.runtime.entries = Arc::new(vec![unknown_entry(active_file.clone())]);
    app.shell.runtime.all_entries = Arc::new(vec![unknown_entry(active_file.clone())]);
    app.sync_active_tab_state();

    app.create_new_tab();
    assert_eq!(app.shell.tabs.active_tab, 1);
    app.shell.runtime.entries = Arc::new(vec![unknown_entry(active_file.clone())]);
    app.shell.runtime.all_entries = Arc::new(vec![unknown_entry(active_file.clone())]);
    app.sync_active_tab_state();

    index_res_tx
        .send(IndexResponse::Batch {
            request_id: index_req.request_id,
            entries: vec![IndexEntry {
                path: indexed_file.clone(),
                kind: EntryKind::file(),
                kind_known: true,
            }],
        })
        .expect("send batch");
    index_res_tx
        .send(IndexResponse::Finished {
            request_id: index_req.request_id,
            source: IndexSource::Walker,
        })
        .expect("send finished");

    app.poll_index_response();

    assert_eq!(app.shell.runtime.entries.len(), 1);
    assert_eq!(app.shell.runtime.entries[0], active_file);

    app.switch_to_tab_index(0);
    assert_eq!(app.shell.runtime.entries.len(), 1);
    assert_eq!(app.shell.runtime.entries[0], indexed_file);
    assert!(!app.shell.indexing.in_progress);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn background_index_finish_invalidates_older_sort_snapshot() {
    let root = test_root("background-index-invalidates-sort");
    fs::create_dir_all(&root).expect("create dir");
    let stale = root.join("stale.txt");
    let current = root.join("current.txt");
    fs::write(&stale, "stale").expect("write stale");
    fs::write(&current, "current").expect("write current");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (index_req_tx, index_req_rx) = bounded_request_channel::<IndexRequest>(2);
    let (index_res_tx, index_res_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.tx = index_req_tx;
    app.shell.indexing.rx = index_res_rx;
    reset_index_request_state_for_test(&mut app);
    app.request_index_refresh();
    let index_req = index_req_rx.try_recv().expect("index request");

    app.shell.runtime.entries = Arc::new(vec![file_entry(stale.clone())]);
    app.shell.runtime.all_entries = Arc::clone(&app.shell.runtime.entries);
    app.shell.runtime.base_results = vec![(stale.clone(), 1.0)];
    app.shell.runtime.results = app.shell.runtime.base_results.clone();
    app.shell.runtime.total_match_count = 1;
    app.shell.runtime.current_row = Some(0);
    let (sort_req_tx, sort_req_rx) = mpsc::channel::<SortMetadataRequest>();
    let (sort_res_tx, sort_res_rx) = mpsc::channel::<SortMetadataResponse>();
    app.shell.worker_bus.sort.tx = sort_req_tx;
    app.shell.worker_bus.sort.rx = sort_res_rx;
    app.set_result_sort_mode(ResultSortMode::SizeDesc);
    let sort_req = sort_req_rx.try_recv().expect("sort request");

    app.create_new_tab();
    index_res_tx
        .send(IndexResponse::ReplaceAll {
            request_id: index_req.request_id,
            entries: vec![IndexEntry {
                path: current.clone(),
                kind: EntryKind::file(),
                kind_known: true,
            }],
        })
        .expect("send replace");
    index_res_tx
        .send(IndexResponse::Finished {
            request_id: index_req.request_id,
            source: IndexSource::Walker,
        })
        .expect("send finish");
    app.poll_index_response();

    sort_res_tx
        .send(SortMetadataResponse {
            request_id: sort_req.request_id,
            entries: vec![(
                stale,
                SortMetadata {
                    size_bytes: Some(5),
                    ..SortMetadata::default()
                },
            )],
            mode: ResultSortMode::SizeDesc,
        })
        .expect("send stale sort");
    app.poll_sort_response();

    let background = app.shell.tabs.get(0).expect("background tab");
    assert_eq!(
        background.result_state.result_sort_mode,
        ResultSortMode::Score
    );
    assert_eq!(background.result_state.committed.total_match_count, 1);
    assert_eq!(
        background.result_state.committed.base_results,
        vec![(current.clone(), 0.0)]
    );
    assert_eq!(
        background.result_state.committed.results,
        vec![(current, 0.0)]
    );
    assert!(background.result_state.pending_sort_request_id.is_none());
    assert!(!background.result_state.sort_in_progress);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn active_index_progress_before_tab_switch_is_preserved_on_background_finish() {
    let root = test_root("active-index-progress-before-tab-switch");
    fs::create_dir_all(&root).expect("create dir");
    let first_file = root.join("first.txt");
    let second_file = root.join("second.txt");
    fs::write(&first_file, "first").expect("write first");
    fs::write(&second_file, "second").expect("write second");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (index_req_tx, index_req_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = index_req_tx;
    let (index_res_tx, index_res_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = index_res_rx;
    reset_index_request_state_for_test(&mut app);

    app.request_index_refresh();
    let index_req = index_req_rx.try_recv().expect("index request");

    index_res_tx
        .send(IndexResponse::Batch {
            request_id: index_req.request_id,
            entries: vec![IndexEntry {
                path: first_file.clone(),
                kind: EntryKind::file(),
                kind_known: true,
            }],
        })
        .expect("send active batch");
    app.poll_index_response();
    assert_eq!(app.shell.indexing.build.index.entries.len(), 1);

    app.create_new_tab();
    assert_eq!(app.shell.tabs.active_tab, 1);

    index_res_tx
        .send(IndexResponse::Batch {
            request_id: index_req.request_id,
            entries: vec![IndexEntry {
                path: second_file.clone(),
                kind: EntryKind::file(),
                kind_known: true,
            }],
        })
        .expect("send background batch");
    index_res_tx
        .send(IndexResponse::Finished {
            request_id: index_req.request_id,
            source: IndexSource::Walker,
        })
        .expect("send background finished");

    app.poll_index_response();
    app.switch_to_tab_index(0);

    assert_eq!(app.shell.runtime.entries.len(), 2);
    assert!(app
        .shell
        .runtime
        .entries
        .iter()
        .any(|entry| entry.path == first_file));
    assert!(app
        .shell
        .runtime
        .entries
        .iter()
        .any(|entry| entry.path == second_file));
    assert!(!app.shell.indexing.in_progress);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn active_index_handoff_preserves_pending_and_background_batches() {
    let root = test_root("active-index-handoff-pending-background");
    fs::create_dir_all(&root).expect("create dir");
    let drained_file = root.join("drained.txt");
    let pending_file = root.join("pending.txt");
    let background_file = root.join("background.txt");
    fs::write(&drained_file, "drained").expect("write drained");
    fs::write(&pending_file, "pending").expect("write pending");
    fs::write(&background_file, "background").expect("write background");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (index_req_tx, index_req_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = index_req_tx;
    let (index_res_tx, index_res_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = index_res_rx;
    reset_index_request_state_for_test(&mut app);

    app.request_index_refresh();
    let index_req = index_req_rx.try_recv().expect("index request");
    app.shell.indexing.build.index.entries = vec![file_entry(drained_file.clone())];
    app.shell.indexing.pending_entries_request_id = Some(index_req.request_id);
    app.shell
        .indexing
        .build
        .pending_entries
        .push_back(IndexEntry {
            path: pending_file.clone(),
            kind: EntryKind::file(),
            kind_known: true,
        });

    app.create_new_tab();
    assert_eq!(app.shell.tabs.active_tab, 1);

    index_res_tx
        .send(IndexResponse::Batch {
            request_id: index_req.request_id,
            entries: vec![IndexEntry {
                path: background_file.clone(),
                kind: EntryKind::file(),
                kind_known: true,
            }],
        })
        .expect("send background batch");
    index_res_tx
        .send(IndexResponse::Finished {
            request_id: index_req.request_id,
            source: IndexSource::Walker,
        })
        .expect("send background finished");

    app.poll_index_response();
    app.switch_to_tab_index(0);

    assert_eq!(app.shell.runtime.entries.len(), 3);
    assert!(app
        .shell
        .runtime
        .entries
        .iter()
        .any(|entry| entry.path == drained_file));
    assert!(app
        .shell
        .runtime
        .entries
        .iter()
        .any(|entry| entry.path == pending_file));
    assert!(app
        .shell
        .runtime
        .entries
        .iter()
        .any(|entry| entry.path == background_file));
    assert!(!app.shell.indexing.in_progress);
    assert!(app.shell.indexing.build.pending_entries.is_empty());

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn background_replace_all_after_active_handoff_discards_prior_partial_index() {
    let root = test_root("background-replace-all-discards-partial");
    fs::create_dir_all(&root).expect("create dir");
    let stale_file = root.join("stale.txt");
    let replacement_file = root.join("replacement.txt");
    fs::write(&stale_file, "stale").expect("write stale");
    fs::write(&replacement_file, "replacement").expect("write replacement");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (index_req_tx, index_req_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = index_req_tx;
    let (index_res_tx, index_res_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = index_res_rx;
    reset_index_request_state_for_test(&mut app);

    app.request_index_refresh();
    let index_req = index_req_rx.try_recv().expect("index request");
    app.shell.indexing.build.index.entries = vec![file_entry(stale_file.clone())];
    app.shell.indexing.pending_entries_request_id = Some(index_req.request_id);
    app.shell
        .indexing
        .build
        .pending_entries
        .push_back(IndexEntry {
            path: stale_file.clone(),
            kind: EntryKind::file(),
            kind_known: true,
        });

    app.create_new_tab();

    index_res_tx
        .send(IndexResponse::ReplaceAll {
            request_id: index_req.request_id,
            entries: vec![IndexEntry {
                path: replacement_file.clone(),
                kind: EntryKind::file(),
                kind_known: true,
            }],
        })
        .expect("send replace all");
    index_res_tx
        .send(IndexResponse::Finished {
            request_id: index_req.request_id,
            source: IndexSource::FileList(root.join("FileList.txt")),
        })
        .expect("send finished");

    app.poll_index_response();
    app.switch_to_tab_index(0);

    assert_eq!(app.shell.runtime.entries.len(), 1);
    assert_eq!(app.shell.runtime.entries[0], replacement_file);
    assert!(!app
        .shell
        .runtime
        .entries
        .iter()
        .any(|entry| entry.path == stale_file));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn background_empty_query_index_finish_updates_total_match_count() {
    let root = test_root("background-empty-query-total-count");
    fs::create_dir_all(&root).expect("create dir");
    let active_file = root.join("active.txt");
    let indexed_a = root.join("indexed-a.txt");
    let indexed_b = root.join("indexed-b.txt");
    fs::write(&active_file, "a").expect("write active");
    fs::write(&indexed_a, "a").expect("write indexed a");
    fs::write(&indexed_b, "b").expect("write indexed b");

    let mut app = FlistWalkerApp::new(root.clone(), 1, String::new());
    let (index_res_tx, index_res_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = index_res_rx;
    app.shell.runtime.entries = Arc::new(vec![file_entry(active_file.clone())]);
    app.shell.runtime.all_entries = Arc::new(vec![file_entry(active_file.clone())]);
    app.shell.runtime.results = vec![(active_file.clone(), 0.0)];
    app.shell.runtime.base_results = app.shell.runtime.results.clone();
    app.shell.runtime.total_match_count = 99;
    app.sync_active_tab_state();

    app.create_new_tab();
    let background_tab_id = app.shell.tabs.get(0).expect("tab 0").id;
    app.shell
        .indexing
        .request_tabs
        .insert(77, background_tab_id);
    app.shell
        .indexing
        .latest_request_ids
        .lock()
        .expect("latest index requests")
        .insert(background_tab_id, 77);
    app.shell
        .tabs
        .get_mut(0)
        .expect("tab 0")
        .index_state
        .pending_index_request_id = Some(77);
    app.shell
        .tabs
        .get_mut(0)
        .expect("tab 0")
        .index_state
        .index_in_progress = true;

    index_res_tx
        .send(IndexResponse::Batch {
            request_id: 77,
            entries: vec![
                IndexEntry {
                    path: indexed_a.clone(),
                    kind: EntryKind::file(),
                    kind_known: true,
                },
                IndexEntry {
                    path: indexed_b.clone(),
                    kind: EntryKind::file(),
                    kind_known: true,
                },
            ],
        })
        .expect("send background batch");
    index_res_tx
        .send(IndexResponse::Finished {
            request_id: 77,
            source: IndexSource::Walker,
        })
        .expect("send background finished");

    app.poll_index_response();

    let background_tab = app.shell.tabs.get(0).expect("tab 0");
    assert_eq!(background_tab.result_state.committed.results.len(), 1);
    assert_eq!(background_tab.result_state.committed.total_match_count, 2);

    app.switch_to_tab_index(0);
    assert_eq!(app.shell.runtime.results.len(), 1);
    assert_eq!(app.shell.runtime.total_match_count, 2);
    assert!(app.status_line_text().contains("Results: 1 of 2 shown"));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_background_empty_terminal_clears_absent_evicted_selection_intent_regression() {
    let root = test_root("tc-207-background-empty-clears-selection-intent");
    fs::create_dir_all(&root).expect("create dir");
    let first = root.join("first.txt");
    let previously_selected = root.join("previously-selected.txt");
    fs::write(&first, "first").expect("write first");
    fs::write(&previously_selected, "selected").expect("write selected");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (index_res_tx, index_res_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = index_res_rx;
    app.create_new_tab();
    let background_tab_id = app.shell.tabs.get(0).expect("background tab").id;
    {
        let tab = app.shell.tabs.get_mut(0).expect("background tab");
        tab.result_state.evicted_selected_path = Some(previously_selected.clone());
        tab.index_state.pending_index_request_id = Some(807);
        tab.index_state.index_in_progress = true;
    }
    app.shell
        .indexing
        .request_tabs
        .insert(807, background_tab_id);
    app.shell
        .indexing
        .latest_request_ids
        .lock()
        .expect("latest index requests")
        .insert(background_tab_id, 807);

    index_res_tx
        .send(IndexResponse::Finished {
            request_id: 807,
            source: IndexSource::Walker,
        })
        .expect("send empty terminal");
    app.poll_index_response();

    let background = app.shell.tabs.get(0).expect("background tab");
    assert!(background.result_state.committed.results.is_empty());
    assert!(background.result_state.evicted_selected_path.is_none());

    {
        let tab = app.shell.tabs.get_mut(0).expect("background tab");
        tab.index_state.pending_index_request_id = Some(808);
        tab.index_state.index_in_progress = true;
    }
    app.shell
        .indexing
        .request_tabs
        .insert(808, background_tab_id);
    app.shell
        .indexing
        .latest_request_ids
        .lock()
        .expect("latest index requests")
        .insert(background_tab_id, 808);
    index_res_tx
        .send(IndexResponse::Batch {
            request_id: 808,
            entries: vec![
                IndexEntry {
                    path: first.clone(),
                    kind: EntryKind::file(),
                    kind_known: true,
                },
                IndexEntry {
                    path: previously_selected.clone(),
                    kind: EntryKind::file(),
                    kind_known: true,
                },
            ],
        })
        .expect("send later batch");
    index_res_tx
        .send(IndexResponse::Finished {
            request_id: 808,
            source: IndexSource::Walker,
        })
        .expect("send later terminal");
    app.poll_index_response();

    let background = app.shell.tabs.get(0).expect("background tab");
    assert_eq!(background.result_state.committed.current_row, Some(0));
    assert_eq!(background.result_state.committed.results[0].0, first);
    assert!(background.result_state.evicted_selected_path.is_none());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn background_tab_search_and_index_responses_do_not_override_active_results() {
    let root = test_root("background-tab-response-isolation");
    fs::create_dir_all(&root).expect("create dir");
    let active_file = root.join("active.txt");
    let background_file = root.join("background.txt");
    fs::write(&active_file, "active").expect("write active");
    fs::write(&background_file, "background").expect("write background");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (index_req_tx, index_req_rx) = bounded_request_channel::<IndexRequest>(2);
    let (index_res_tx, index_res_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.tx = index_req_tx;
    app.shell.indexing.rx = index_res_rx;
    reset_index_request_state_for_test(&mut app);
    let (search_tx_req, search_rx_req) = mpsc::channel::<SearchRequest>();
    let (search_tx_res, search_rx_res) = mpsc::channel::<SearchResponse>();
    app.shell.search.tx = search_tx_req;
    app.shell.search.rx = search_rx_res;

    app.shell.runtime.entries = Arc::new(vec![file_entry(active_file.clone())]);
    app.shell.runtime.all_entries = Arc::new(vec![file_entry(active_file.clone())]);
    app.shell.runtime.results = vec![(active_file.clone(), 0.0)];
    app.shell.runtime.base_results = app.shell.runtime.results.clone();
    app.shell.runtime.current_row = Some(0);
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    app.shell
        .indexing
        .set_committed_snapshot_present_for_test(true);
    app.sync_active_tab_state();

    app.create_new_tab();
    assert_eq!(app.shell.tabs.active_tab, 1);
    app.shell.runtime.entries = Arc::new(vec![file_entry(active_file.clone())]);
    app.shell.runtime.all_entries = Arc::new(vec![file_entry(active_file.clone())]);
    app.shell.runtime.results = vec![(active_file.clone(), 0.0)];
    app.shell.runtime.base_results = app.shell.runtime.results.clone();
    app.shell.runtime.current_row = Some(0);
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    app.shell
        .indexing
        .set_committed_snapshot_present_for_test(true);
    app.sync_active_tab_state();

    app.switch_to_tab_index(0);
    app.shell.runtime.query_state.query = "background".to_string();
    app.sync_active_tab_state();
    app.switch_to_tab_index(1);

    let background_tab_id = app.shell.tabs.get(0).expect("tab 0").id;
    let background_index_request = IndexRequest {
        request_id: 88,
        tab_id: background_tab_id,
        root: root.clone(),
        use_filelist: true,
        include_files: true,
        include_dirs: true,
        max_depth: crate::indexer::MaxDepth::unlimited(),
        follow_links: false,
    };
    app.shell
        .indexing
        .request_tabs
        .insert(88, background_tab_id);
    app.shell
        .indexing
        .latest_request_ids
        .lock()
        .expect("latest index requests")
        .insert(background_tab_id, 88);
    app.shell
        .tabs
        .get_mut(0)
        .expect("tab 0")
        .index_state
        .pending_index_request_id = Some(88);
    app.shell
        .tabs
        .get_mut(0)
        .expect("tab 0")
        .index_state
        .index_in_progress = true;
    app.shell.search.bind_request_tab(89, background_tab_id);
    app.shell.tabs.get_mut(0).expect("tab 0").pending_request_id = Some(89);
    app.shell.tabs.get_mut(0).expect("tab 0").search_in_progress = true;

    let active_results = app.shell.runtime.results.clone();
    let active_base_results = app.shell.runtime.base_results.clone();
    let active_current_row = app.shell.runtime.current_row;

    search_tx_res
        .send(SearchResponse {
            request_id: 89,
            results: vec![(background_file.clone(), 9.0)],
            total_match_count: 1,
            sort_mode: ResultSortMode::Score,
            sort_scope: ResultSortScope::ShownResults,
            error: None,
        })
        .expect("send background search response");
    index_res_tx
        .send(IndexResponse::Batch {
            request_id: background_index_request.request_id,
            entries: vec![IndexEntry {
                path: background_file.clone(),
                kind: EntryKind::file(),
                kind_known: true,
            }],
        })
        .expect("send background batch");
    index_res_tx
        .send(IndexResponse::Finished {
            request_id: background_index_request.request_id,
            source: IndexSource::Walker,
        })
        .expect("send background finished");

    app.poll_search_response();
    app.poll_index_response();

    assert_eq!(app.shell.runtime.results, active_results);
    assert_eq!(app.shell.runtime.base_results, active_base_results);
    assert_eq!(app.shell.runtime.current_row, active_current_row);
    assert!(app
        .shell
        .tabs
        .get(0)
        .expect("tab 0")
        .result_state
        .committed
        .base_results
        .is_empty());
    assert!(
        search_rx_req.try_recv().is_ok(),
        "the committed background snapshot must be searched again"
    );
    assert_eq!(
        app.shell
            .tabs
            .get(0)
            .expect("tab 0")
            .result_state
            .committed
            .entries
            .len(),
        1
    );
    assert_eq!(
        app.shell
            .tabs
            .get(0)
            .expect("tab 0")
            .result_state
            .committed
            .entries[0],
        background_file
    );
    assert!(index_req_rx.try_recv().is_err());

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn background_walker_truncated_notice_points_to_config_file_setting() {
    let root = test_root("background-walker-truncated-config-notice");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (_index_req_tx, _index_req_rx) = bounded_request_channel::<IndexRequest>(2);
    let (index_res_tx, index_res_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = index_res_rx;

    app.create_new_tab();
    assert_eq!(app.shell.tabs.active_tab, 1);
    let background_tab_id = app.shell.tabs.get(0).expect("tab 0").id;
    app.shell
        .indexing
        .request_tabs
        .insert(92, background_tab_id);
    app.shell
        .indexing
        .latest_request_ids
        .lock()
        .expect("latest index requests")
        .insert(background_tab_id, 92);
    app.shell
        .tabs
        .get_mut(0)
        .expect("tab 0")
        .index_state
        .pending_index_request_id = Some(92);

    index_res_tx
        .send(IndexResponse::Truncated {
            request_id: 92,
            limit: 500_000,
        })
        .expect("send background truncated response");

    app.poll_index_response();

    let notice = &app.shell.tabs.get(0).expect("tab 0").notice;
    assert_eq!(
        notice,
        "Walker capped at 500000 entries (set walker_max_entries in the config file to adjust)"
    );
    assert!(!notice.contains("FLISTWALKER_WALKER_MAX_ENTRIES"));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_background_finish_waits_for_reclaimer_without_dropping_old_snapshot() {
    let root = test_root("tc-207-background-finish-reclaimer-debt");
    fs::create_dir_all(&root).expect("create root");
    let old = file_entry(root.join("old.txt"));
    let new = file_entry(root.join("new.txt"));
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    let background_tab_id = app.shell.tabs.get(0).expect("background tab").id;
    {
        let tab = app.shell.tabs.get_mut(0).expect("background tab");
        tab.index_state
            .set_lifecycle_for_test(TabResourceLifecycle::Refreshing);
        tab.index_state
            .set_committed_snapshot_present_for_test(true);
        tab.result_state.committed.all_entries = Arc::new(vec![old.clone()]);
        tab.result_state.committed.entries = Arc::new(vec![old.clone()]);
        tab.result_state.committed.results = vec![(old.path.clone(), 0.0)];
        tab.index_state.pending_index_request_id = Some(407);
        tab.index_state.index_in_progress = true;
    }
    app.shell
        .indexing
        .request_tabs
        .insert(407, background_tab_id);
    app.shell.indexing.warm_tab_id = Some(background_tab_id);
    app.shell.indexing.background_states.insert(
        407,
        BackgroundIndexState {
            source: Some(IndexSource::Walker),
            entries: vec![new.clone()],
            replaced: true,
        },
    );
    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut tab = app.capture_active_tab_state(1_400 + index as u64);
        tab.result_state.committed.all_entries =
            Arc::new(vec![file_entry(root.join(format!("held-{index}.txt")))]);
        tab.index_state
            .set_committed_snapshot_present_for_test(true);
        app.shell
            .tabs
            .retire_tab_resources_for_test(tab.take_heavy_resources())
            .expect("fill reclaimer");
    }

    let effect = app.apply_background_index_response(
        0,
        IndexResponse::Finished {
            request_id: 407,
            source: IndexSource::Walker,
        },
    );

    assert!(effect.cleanup_request_id.is_none());
    let waiting = app.shell.tabs.get(0).expect("background tab");
    assert_eq!(waiting.result_state.committed.all_entries.as_ref(), &[old]);
    assert!(waiting.index_state.pending_index_finish.is_some());
    assert!(waiting
        .notice
        .contains("Waiting for background tab resource"));

    app.shell.tabs.resume_resource_reclaimer();
    let frame_times =
        settle_background_finish_with_frame_budget(&mut app, 0, 407, IndexSource::Walker);
    assert!(
        frame_times
            .iter()
            .all(|elapsed| *elapsed < Duration::from_millis(100)),
        "frame times: {frame_times:?}"
    );

    let completed = app.shell.tabs.get(0).expect("background tab");
    assert_eq!(
        completed.result_state.committed.all_entries.as_ref(),
        &[new]
    );
    assert!(completed.index_state.pending_index_finish.is_none());
    assert_eq!(
        completed.index_state.lifecycle(),
        TabResourceLifecycle::Ready
    );
    assert!(!app.shell.indexing.request_tabs.contains_key(&407));
    assert_eq!(app.shell.indexing.warm_tab_id, Some(background_tab_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_background_finished_commits_100k_default_snapshot_within_ui_budget() {
    let root = test_root("tc-207-background-finish-100k");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    let background_tab_id = app.shell.tabs.get(0).expect("background tab").id;
    let request_id = app
        .shell
        .indexing
        .allocate_request_id(Some(background_tab_id));
    {
        let tab = app.shell.tabs.get_mut(0).expect("background tab");
        tab.include_files = true;
        tab.include_dirs = true;
        tab.index_state
            .set_lifecycle_for_test(TabResourceLifecycle::Loading);
        tab.index_state.pending_index_request_id = Some(request_id);
        tab.index_state.index_in_progress = true;
    }
    app.shell.indexing.background_states.insert(
        request_id,
        BackgroundIndexState {
            source: Some(IndexSource::FileList(root.join("FileList.txt"))),
            entries: (0..100_000)
                .map(|index| file_entry(root.join(format!("entry-{index}.txt"))))
                .collect(),
            replaced: true,
        },
    );

    let started = Instant::now();
    app.handle_background_index_response(
        0,
        IndexResponse::Finished {
            request_id,
            source: IndexSource::FileList(root.join("FileList.txt")),
        },
    );
    let elapsed = started.elapsed();

    assert!(
        elapsed < Duration::from_millis(100),
        "100k background finish admission took {elapsed:?}"
    );
    let staged = app
        .shell
        .indexing
        .background_finalizations
        .get(&request_id)
        .expect("100k finalization remains pending after admission");
    assert!((1..=2_048).contains(&staged.completed_entries.len()));
    assert!(app
        .shell
        .tabs
        .get(0)
        .expect("background tab")
        .index_state
        .pending_index_finish
        .is_some());
    let frames = drive_background_finalization_with_frame_budget(
        &mut app,
        0,
        request_id,
        IndexSource::FileList(root.join("FileList.txt")),
        BackgroundFinalizationTarget::Removed,
    );
    for frame in &frames {
        if let (Some(before), Some(after)) = (frame.before, frame.after) {
            let added = after.completed.saturating_sub(before.completed);
            assert!(added <= 2_048, "one frame assembled {added} entries");
        }
    }
    assert!(
        frames.len() > 1,
        "100k finalization must span multiple frames"
    );
    let tab = app.shell.tabs.get(0).expect("completed background tab");
    assert_eq!(tab.result_state.committed.all_entries.len(), 100_000);
    assert_eq!(tab.index_state.lifecycle(), TabResourceLifecycle::Ready);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_background_finished_mixed_owners_settles_incrementally() {
    let root = test_root("tc-207-background-finish-mixed-100k");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    let tab_id = app.shell.tabs.get(0).expect("background tab").id;
    let request_id = app.shell.indexing.allocate_request_id(Some(tab_id));
    {
        let tab = app.shell.tabs.get_mut(0).expect("background tab");
        tab.index_state.pending_index_request_id = Some(request_id);
        tab.index_state.index_in_progress = true;
        tab.index_state.build.index.entries = (0..40_000)
            .map(|index| file_entry(root.join(format!("base-{index}.txt"))))
            .collect();
        tab.index_state.pending_index_entries_request_id = Some(request_id);
        tab.index_state.build.pending_entries = (0..30_000)
            .map(|index| IndexEntry {
                path: root.join(format!("pending-{index}.txt")),
                kind: EntryKind::file(),
                kind_known: true,
            })
            .collect();
    }
    app.shell.indexing.background_states.insert(
        request_id,
        BackgroundIndexState {
            source: Some(IndexSource::FileList(root.join("FileList.txt"))),
            entries: (0..30_000)
                .map(|index| file_entry(root.join(format!("tail-{index}.txt"))))
                .collect(),
            replaced: false,
        },
    );

    let started = Instant::now();
    app.handle_background_index_response(
        0,
        IndexResponse::Finished {
            request_id,
            source: IndexSource::FileList(root.join("FileList.txt")),
        },
    );
    assert!(started.elapsed() < Duration::from_millis(100));
    let frames = settle_background_finish_with_frame_budget(
        &mut app,
        0,
        request_id,
        IndexSource::FileList(root.join("FileList.txt")),
    );
    assert!(frames
        .iter()
        .all(|elapsed| *elapsed < Duration::from_millis(100)));
    let tab = app.shell.tabs.get(0).expect("background tab");
    assert_eq!(tab.result_state.committed.all_entries.len(), 100_000);
    assert!(tab.result_state.committed.all_entries[0]
        .path
        .ends_with("base-0.txt"));
    assert!(tab.result_state.committed.all_entries[40_000]
        .path
        .ends_with("pending-0.txt"));
    assert!(tab.result_state.committed.all_entries[70_000]
        .path
        .ends_with("tail-0.txt"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_background_finished_file_filter_settles_incrementally() {
    let root = test_root("tc-207-background-finish-filter-100k");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.shell.ui.ignore_list_enabled = true;
    app.shell.runtime.ignore_list_terms = Arc::new(vec!["ignored".to_string()]);
    let tab_id = app.shell.tabs.get(0).expect("background tab").id;
    let request_id = app.shell.indexing.allocate_request_id(Some(tab_id));
    {
        let tab = app.shell.tabs.get_mut(0).expect("background tab");
        tab.include_files = true;
        tab.include_dirs = false;
        tab.index_state.pending_index_request_id = Some(request_id);
        tab.index_state.index_in_progress = true;
    }
    app.shell.indexing.background_states.insert(
        request_id,
        BackgroundIndexState {
            source: Some(IndexSource::FileList(root.join("FileList.txt"))),
            entries: (0..100_000)
                .map(|index| {
                    let name = if index % 4 == 0 {
                        format!("ignored-{index}")
                    } else {
                        format!("entry-{index}")
                    };
                    let path = root.join(name);
                    if index % 2 == 0 {
                        file_entry(path)
                    } else {
                        dir_entry(path)
                    }
                })
                .collect(),
            replaced: true,
        },
    );

    app.handle_background_index_response(
        0,
        IndexResponse::Finished {
            request_id,
            source: IndexSource::FileList(root.join("FileList.txt")),
        },
    );
    let frames = settle_background_finish_with_frame_budget(
        &mut app,
        0,
        request_id,
        IndexSource::FileList(root.join("FileList.txt")),
    );
    assert!(frames
        .iter()
        .all(|elapsed| *elapsed < Duration::from_millis(100)));
    let tab = app.shell.tabs.get(0).expect("background tab");
    assert_eq!(tab.result_state.committed.all_entries.len(), 100_000);
    assert_eq!(tab.result_state.committed.entries.len(), 25_000);
    assert!(tab
        .result_state
        .committed
        .entries
        .iter()
        .all(|entry| entry.kind == Some(EntryKind::file())));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_background_finalization_tracks_ignore_policy_changes_both_directions() {
    for (starts_enabled, expected_len) in [(false, 50_000), (true, 100_000)] {
        let case = if starts_enabled { "on-off" } else { "off-on" };
        let root = test_root(&format!("tc-207-finalize-ignore-{case}"));
        fs::create_dir_all(&root).expect("create root");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.create_new_tab();
        app.shell.ui.ignore_list_enabled = starts_enabled;
        app.shell.runtime.ignore_list_terms = Arc::new(vec!["ignored".to_string()]);
        let tab_id = app.shell.tabs.get(0).expect("background tab").id;
        let request_id = app.shell.indexing.allocate_request_id(Some(tab_id));
        {
            let tab = app.shell.tabs.get_mut(0).expect("background tab");
            tab.include_files = true;
            tab.include_dirs = true;
            tab.index_state.pending_index_request_id = Some(request_id);
            tab.index_state.index_in_progress = true;
        }
        app.shell.indexing.background_states.insert(
            request_id,
            BackgroundIndexState {
                source: Some(IndexSource::FileList(root.join("FileList.txt"))),
                entries: (0..100_000)
                    .map(|index| {
                        let prefix = if index % 2 == 0 { "ignored" } else { "kept" };
                        file_entry(root.join(format!("{prefix}-{index}.txt")))
                    })
                    .collect(),
                replaced: true,
            },
        );

        app.handle_background_index_response(
            0,
            IndexResponse::Finished {
                request_id,
                source: IndexSource::FileList(root.join("FileList.txt")),
            },
        );
        assert!(app
            .shell
            .indexing
            .background_finalizations
            .contains_key(&request_id));
        app.shell.ui.ignore_list_enabled = !starts_enabled;
        let frames = settle_background_finish_with_frame_budget(
            &mut app,
            0,
            request_id,
            IndexSource::FileList(root.join("FileList.txt")),
        );
        assert!(frames.len() > 1);
        assert!(frames
            .iter()
            .all(|elapsed| *elapsed < Duration::from_millis(100)));
        let entries = &app
            .shell
            .tabs
            .get(0)
            .expect("background tab")
            .result_state
            .committed
            .entries;
        assert_eq!(entries.len(), expected_len);
        if !starts_enabled {
            assert!(entries
                .iter()
                .all(|entry| !entry.path.to_string_lossy().contains("ignored")));
        }
        let _ = fs::remove_dir_all(&root);
    }
}

#[test]
fn tc_207_late_filter_policy_change_reclaims_old_output_off_ui() {
    let root = test_root("tc-207-late-filter-policy-reclaim");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.shell.ui.ignore_list_enabled = true;
    app.shell.runtime.ignore_list_terms = Arc::new(vec!["ignored".to_string()]);
    let tab_id = app.shell.tabs.get(0).expect("background tab").id;
    let request_id = app.shell.indexing.allocate_request_id(Some(tab_id));
    {
        let tab = app.shell.tabs.get_mut(0).expect("background tab");
        tab.include_files = true;
        tab.include_dirs = true;
        tab.index_state.pending_index_request_id = Some(request_id);
        tab.index_state.index_in_progress = true;
    }
    app.shell.indexing.background_states.insert(
        request_id,
        BackgroundIndexState {
            source: Some(IndexSource::FileList(root.join("FileList.txt"))),
            entries: (0..100_000)
                .map(|index| {
                    let prefix = if index % 2 == 0 { "ignored" } else { "kept" };
                    file_entry(root.join(format!("{prefix}-{index}.txt")))
                })
                .collect(),
            replaced: true,
        },
    );
    app.handle_background_index_response(
        0,
        IndexResponse::Finished {
            request_id,
            source: IndexSource::FileList(root.join("FileList.txt")),
        },
    );
    let frames = drive_background_finalization_with_frame_budget(
        &mut app,
        0,
        request_id,
        IndexSource::FileList(root.join("FileList.txt")),
        BackgroundFinalizationTarget::FilterCursorAtLeast(50_000),
    );
    let filter_frames = frames
        .iter()
        .filter(|frame| {
            if let (Some(before), Some(after)) = (frame.before, frame.after) {
                let advanced = after.filter_cursor.saturating_sub(before.filter_cursor);
                assert!(advanced <= 2_048, "one frame filtered {advanced} entries");
                advanced > 0
            } else {
                false
            }
        })
        .count();
    assert!(filter_frames > 1);
    assert!(app
        .shell
        .indexing
        .background_finalizations
        .get(&request_id)
        .is_some_and(|state| {
            state
                .filtered_entries
                .as_ref()
                .is_some_and(|entries| entries.len() >= 20_000)
        }));

    let _observer_guard = lock_reclaim_drop_observer_for_test();
    let (drop_tx, drop_rx) = mpsc::channel();
    set_reclaim_drop_observer(Some(drop_tx));
    app.shell.ui.ignore_list_enabled = false;
    let started = Instant::now();
    app.handle_background_index_response(
        0,
        IndexResponse::Finished {
            request_id,
            source: IndexSource::FileList(root.join("FileList.txt")),
        },
    );
    assert!(started.elapsed() < Duration::from_millis(100));
    set_reclaim_drop_observer(None);
    let drop_threads = collect_drop_threads_until(
        &drop_rx,
        "flistwalker-tab-reclaimer",
        Duration::from_millis(250),
    );
    assert!(
        drop_threads
            .iter()
            .any(|name| name == "flistwalker-tab-reclaimer"),
        "drop threads: {drop_threads:?}"
    );
    let _ = settle_background_finish_with_frame_budget(
        &mut app,
        0,
        request_id,
        IndexSource::FileList(root.join("FileList.txt")),
    );
    assert_eq!(
        app.shell
            .tabs
            .get(0)
            .expect("background tab")
            .result_state
            .committed
            .entries
            .len(),
        100_000
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_late_filter_policy_change_rolls_back_while_reclaimer_is_full() {
    let root = test_root("tc-207-late-filter-policy-full");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.shell.ui.ignore_list_enabled = true;
    app.shell.runtime.ignore_list_terms = Arc::new(vec!["ignored".to_string()]);
    let tab_id = app.shell.tabs.get(0).expect("background tab").id;
    let request_id = app.shell.indexing.allocate_request_id(Some(tab_id));
    {
        let tab = app.shell.tabs.get_mut(0).expect("background tab");
        tab.index_state.pending_index_request_id = Some(request_id);
        tab.index_state.index_in_progress = true;
    }
    app.shell.indexing.background_states.insert(
        request_id,
        BackgroundIndexState {
            source: Some(IndexSource::FileList(root.join("FileList.txt"))),
            entries: (0..100_000)
                .map(|index| file_entry(root.join(format!("entry-{index}.txt"))))
                .collect(),
            replaced: true,
        },
    );
    app.handle_background_index_response(
        0,
        IndexResponse::Finished {
            request_id,
            source: IndexSource::FileList(root.join("FileList.txt")),
        },
    );
    let _ = drive_background_finalization_with_frame_budget(
        &mut app,
        0,
        request_id,
        IndexSource::FileList(root.join("FileList.txt")),
        BackgroundFinalizationTarget::FilterCursorAtLeast(50_000),
    );
    let (cursor_before, output_before) = app
        .shell
        .indexing
        .background_finalizations
        .get(&request_id)
        .map(|state| {
            (
                state.filter_cursor,
                state.filtered_entries.as_ref().map_or(0, Vec::len),
            )
        })
        .expect("late filter phase");
    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut held = app.capture_active_tab_state(9_500 + index as u64);
        held.result_state.committed.all_entries =
            Arc::new(vec![file_entry(root.join(format!("held-{index}.txt")))]);
        held.index_state
            .set_committed_snapshot_present_for_test(true);
        app.shell
            .tabs
            .retire_tab_resources_for_test(held.take_heavy_resources())
            .expect("fill reclaimer");
    }
    app.shell.ui.ignore_list_enabled = false;
    let started = Instant::now();
    app.handle_background_index_response(
        0,
        IndexResponse::Finished {
            request_id,
            source: IndexSource::FileList(root.join("FileList.txt")),
        },
    );
    assert!(started.elapsed() < Duration::from_millis(100));
    let state = app
        .shell
        .indexing
        .background_finalizations
        .get(&request_id)
        .expect("policy retry remains pending");
    assert_eq!(state.filter_cursor, cursor_before);
    assert_eq!(
        state.filtered_entries.as_ref().map_or(0, Vec::len),
        output_before
    );
    assert!(state.ignore_list_enabled);

    app.shell.tabs.resume_resource_reclaimer();
    let frames = settle_background_finish_with_frame_budget(
        &mut app,
        0,
        request_id,
        IndexSource::FileList(root.join("FileList.txt")),
    );
    assert!(frames
        .iter()
        .all(|elapsed| *elapsed < Duration::from_millis(100)));
    assert_eq!(
        app.shell
            .tabs
            .get(0)
            .expect("background tab")
            .result_state
            .committed
            .entries
            .len(),
        100_000
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_background_finished_walker_kind_queue_settles_incrementally() {
    let root = test_root("tc-207-background-finish-walker-kind-100k");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    let tab_id = app.shell.tabs.get(0).expect("background tab").id;
    let request_id = app.shell.indexing.allocate_request_id(Some(tab_id));
    {
        let tab = app.shell.tabs.get_mut(0).expect("background tab");
        tab.include_files = true;
        tab.include_dirs = false;
        tab.index_state.pending_index_request_id = Some(request_id);
        tab.index_state.index_in_progress = true;
    }
    app.shell.indexing.background_states.insert(
        request_id,
        BackgroundIndexState {
            source: Some(IndexSource::Walker),
            entries: (0..100_000)
                .map(|index| Entry::unknown(root.join(format!("entry-{index}"))))
                .collect(),
            replaced: true,
        },
    );

    app.handle_background_index_response(
        0,
        IndexResponse::Finished {
            request_id,
            source: IndexSource::Walker,
        },
    );
    let frames = drive_background_finalization_with_frame_budget(
        &mut app,
        0,
        request_id,
        IndexSource::Walker,
        BackgroundFinalizationTarget::Removed,
    );
    let kind_frames = frames
        .iter()
        .filter(|frame| {
            assert!(frame.elapsed < Duration::from_millis(100));
            if let (Some(before), Some(after)) = (frame.before, frame.after) {
                let advanced = after.kind_cursor.saturating_sub(before.kind_cursor);
                assert!(advanced <= 2_048, "one frame scanned {advanced} kinds");
                advanced > 0
            } else {
                false
            }
        })
        .count();
    assert!(kind_frames > 1);
    let tab = app.shell.tabs.get(0).expect("background tab");
    assert_eq!(tab.result_state.committed.all_entries.len(), 100_000);
    assert!(tab.result_state.committed.entries.is_empty());
    assert_eq!(tab.index_state.build.pending_kind_paths.len(), 100_000);
    assert_eq!(tab.index_state.build.pending_kind_paths_set.len(), 100_000);
    assert!(tab.index_state.kind_resolution_in_progress);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_background_finalization_survives_promotion_to_active() {
    let root = test_root("tc-207-background-finalize-promote-active");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    let tab_id = app.shell.tabs.get(0).expect("background tab").id;
    let request_id = app.shell.indexing.allocate_request_id(Some(tab_id));
    {
        let tab = app.shell.tabs.get_mut(0).expect("background tab");
        tab.include_files = true;
        tab.include_dirs = true;
        tab.index_state.pending_index_request_id = Some(request_id);
        tab.index_state.index_in_progress = true;
    }
    app.shell.indexing.background_states.insert(
        request_id,
        BackgroundIndexState {
            source: Some(IndexSource::Walker),
            entries: (0..100_000)
                .map(|index| {
                    let prefix = if index % 4 == 0 || index % 4 == 3 {
                        "ignored"
                    } else {
                        "kept"
                    };
                    let path = root.join(format!("{prefix}-{index}"));
                    if index % 2 == 0 {
                        file_entry(path)
                    } else {
                        dir_entry(path)
                    }
                })
                .collect(),
            replaced: true,
        },
    );
    app.handle_background_index_response(
        0,
        IndexResponse::Finished {
            request_id,
            source: IndexSource::Walker,
        },
    );
    assert!(app
        .shell
        .indexing
        .background_finalizations
        .contains_key(&request_id));
    assert!(app
        .shell
        .tabs
        .get(0)
        .expect("background tab")
        .index_state
        .pending_index_finish
        .is_some());

    app.switch_to_tab_index(0);
    assert_eq!(app.current_tab_id(), Some(tab_id));
    assert!(app.shell.indexing.pending_finish.is_some());
    app.shell.runtime.include_files = false;
    app.shell.runtime.include_dirs = true;
    app.shell.ui.ignore_list_enabled = true;
    app.shell.runtime.ignore_list_terms = Arc::new(vec!["ignored".to_string()]);
    let mut settled = false;
    for _ in 0..2_000 {
        let started = Instant::now();
        app.poll_index_response_with_budget_for_test(Duration::from_millis(4));
        assert!(
            started.elapsed() < Duration::from_millis(100),
            "promotion finalization frame exceeded budget"
        );
        if app.shell.indexing.pending_finish.is_none() {
            settled = true;
            break;
        }
    }

    assert!(settled, "promoted finalization did not settle");
    assert_eq!(app.shell.runtime.all_entries.len(), 100_000);
    assert_eq!(app.shell.runtime.entries.len(), 25_000);
    assert!(app.shell.runtime.entries.iter().all(|entry| {
        entry.kind == Some(EntryKind::dir()) && !entry.path.to_string_lossy().contains("ignored")
    }));
    assert_eq!(app.shell.indexing.lifecycle(), TabResourceLifecycle::Ready);
    assert!(!app
        .shell
        .indexing
        .background_finalizations
        .contains_key(&request_id));
    assert!(!app.shell.indexing.request_tabs.contains_key(&request_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_terminal_finalizers_do_not_block_a_new_active_request_when_reclaimer_is_full() {
    let root = test_root("tc-207-finalizer-slots-active-priority");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.create_new_tab();
    let (index_tx, index_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = index_tx;
    reset_index_request_state_for_test(&mut app);

    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut held = app.capture_active_tab_state(8_000 + index as u64);
        held.result_state.committed.all_entries =
            Arc::new(vec![file_entry(root.join(format!("held-{index}.txt")))]);
        held.index_state
            .set_committed_snapshot_present_for_test(true);
        app.shell
            .tabs
            .retire_tab_resources_for_test(held.take_heavy_resources())
            .expect("fill reclaimer");
    }

    for tab_index in 0..2 {
        let tab_id = app.shell.tabs.get(tab_index).expect("inactive tab").id;
        let request_id = app.shell.indexing.allocate_request_id(Some(tab_id));
        {
            let tab = app.shell.tabs.get_mut(tab_index).expect("inactive tab");
            tab.index_state.pending_index_request_id = Some(request_id);
            tab.index_state.index_in_progress = true;
        }
        app.shell.indexing.background_states.insert(
            request_id,
            BackgroundIndexState {
                source: Some(IndexSource::FileList(root.join("FileList.txt"))),
                entries: (0..100_000)
                    .map(|index| {
                        file_entry(root.join(format!("tab-{tab_index}-entry-{index}.txt")))
                    })
                    .collect(),
                replaced: true,
            },
        );
        app.shell.indexing.inflight_requests.insert(request_id);

        app.handle_background_index_response(
            tab_index,
            IndexResponse::Finished {
                request_id,
                source: IndexSource::FileList(root.join("FileList.txt")),
            },
        );
        assert!(!app.shell.indexing.inflight_requests.contains(&request_id));
    }

    assert!(app.shell.indexing.background_finalizations.is_full());
    assert_eq!(
        app.shell.indexing.background_finalizations.keys().count(),
        2
    );
    let active_tab_id = app.current_tab_id().expect("active tab");
    app.request_index_refresh();
    let request = index_rx
        .try_recv()
        .expect("active request must bypass occupied finalization slots");
    assert_eq!(request.tab_id, active_tab_id);

    app.shell.tabs.resume_resource_reclaimer();
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_terminal_waits_in_mailbox_and_activation_commits_complete_snapshot() {
    let root = test_root("tc-207-terminal-mailbox-activation-barrier");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.create_new_tab();
    app.create_new_tab();
    reset_index_request_state_for_test(&mut app);
    let original_active_id = app.current_tab_id().expect("active tab");

    for tab_index in 0..2 {
        let tab_id = app.shell.tabs.get(tab_index).expect("inactive tab").id;
        let request_id = app.shell.indexing.allocate_request_id(Some(tab_id));
        {
            let tab = app.shell.tabs.get_mut(tab_index).expect("inactive tab");
            tab.index_state.pending_index_request_id = Some(request_id);
            tab.index_state.index_in_progress = true;
        }
        app.shell.indexing.background_states.insert(
            request_id,
            BackgroundIndexState {
                source: Some(IndexSource::FileList(root.join("FileList.txt"))),
                entries: (0..100_000)
                    .map(|index| {
                        file_entry(root.join(format!("slot-{tab_index}-entry-{index}.txt")))
                    })
                    .collect(),
                replaced: true,
            },
        );
        app.handle_background_index_response(
            tab_index,
            IndexResponse::Finished {
                request_id,
                source: IndexSource::FileList(root.join("FileList.txt")),
            },
        );
    }
    assert!(app.shell.indexing.background_finalizations.is_full());

    let waiting_tab_index = 2;
    let waiting_tab_id = app
        .shell
        .tabs
        .get(waiting_tab_index)
        .expect("waiting tab")
        .id;
    let waiting_request_id = app.shell.indexing.allocate_request_id(Some(waiting_tab_id));
    {
        let tab = app
            .shell
            .tabs
            .get_mut(waiting_tab_index)
            .expect("waiting tab");
        tab.index_state.pending_index_request_id = Some(waiting_request_id);
        tab.index_state.index_in_progress = true;
        tab.index_state.build.index.entries = (0..20_000)
            .map(|index| file_entry(root.join(format!("partial-{index}.txt"))))
            .collect();
        tab.index_state.pending_index_entries_request_id = Some(waiting_request_id);
        tab.index_state.build.pending_entries = (0..20_000)
            .map(|index| IndexEntry {
                path: root.join(format!("pending-{index}.txt")),
                kind: EntryKind::file(),
                kind_known: true,
            })
            .collect();
    }
    app.shell.indexing.background_states.insert(
        waiting_request_id,
        BackgroundIndexState {
            source: Some(IndexSource::FileList(root.join("FileList.txt"))),
            entries: (0..60_000)
                .map(|index| file_entry(root.join(format!("continuation-{index}.txt"))))
                .collect(),
            replaced: false,
        },
    );
    let waiting_mailbox = app
        .shell
        .indexing
        .response_mailboxes
        .lock()
        .expect("mailboxes")
        .get(&waiting_request_id)
        .cloned()
        .expect("waiting mailbox");
    waiting_mailbox
        .try_publish(IndexResponse::Finished {
            request_id: waiting_request_id,
            source: IndexSource::FileList(root.join("FileList.txt")),
        })
        .expect("publish terminal");
    app.shell
        .indexing
        .inflight_requests
        .insert(waiting_request_id);

    app.switch_to_tab_index(waiting_tab_index);
    assert_eq!(app.current_tab_id(), Some(original_active_id));
    assert_eq!(
        app.shell.tabs.pending_activation_tab_id,
        Some(waiting_tab_id)
    );
    let current_tab_index = app.shell.tabs.active_tab_index();
    app.switch_to_tab_index(current_tab_index);
    assert_eq!(
        app.shell.tabs.pending_activation_tab_id, None,
        "reselecting the current tab must cancel a deferred activation intent"
    );
    app.switch_to_tab_index(waiting_tab_index);
    assert_eq!(
        app.shell.tabs.pending_activation_tab_id,
        Some(waiting_tab_id)
    );
    assert!(waiting_mailbox.has_terminal_response());
    assert!(app
        .shell
        .tabs
        .get(waiting_tab_index)
        .expect("waiting tab")
        .index_state
        .pending_index_finish
        .is_none());

    let admitted_request_id = *app
        .shell
        .indexing
        .background_finalizations
        .keys()
        .next()
        .expect("occupied finalizer");
    let admitted_tab_id = *app
        .shell
        .indexing
        .request_tabs
        .get(&admitted_request_id)
        .expect("finalizer owner");
    let admitted_tab_index = app
        .find_tab_index_by_id(admitted_tab_id)
        .expect("finalizer tab");
    let source = IndexSource::FileList(root.join("FileList.txt"));
    let _ = settle_background_finish_with_frame_budget(
        &mut app,
        admitted_tab_index,
        admitted_request_id,
        source,
    );

    let mut settled = false;
    for _ in 0..2_000 {
        app.poll_index_response_with_budget_for_test(Duration::from_millis(4));
        if app.current_tab_id() == Some(waiting_tab_id)
            && app.shell.indexing.pending_finish.is_none()
        {
            settled = true;
            break;
        }
    }
    assert!(settled, "deferred activation did not settle");
    assert_eq!(app.shell.runtime.all_entries.len(), 100_000);
    assert!(app
        .shell
        .runtime
        .all_entries
        .iter()
        .any(|entry| entry.path.ends_with("partial-0.txt")));
    assert!(app
        .shell
        .runtime
        .all_entries
        .iter()
        .any(|entry| entry.path.ends_with("pending-0.txt")));
    assert!(app
        .shell
        .runtime
        .all_entries
        .iter()
        .any(|entry| entry.path.ends_with("continuation-0.txt")));
    assert!(!app
        .shell
        .indexing
        .background_states
        .contains_key(&waiting_request_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_promotion_before_terminal_uses_active_finalization_barrier() {
    let root = test_root("tc-207-promotion-before-terminal-barrier");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.create_new_tab();
    app.create_new_tab();
    reset_index_request_state_for_test(&mut app);

    for tab_index in 0..2 {
        let tab_id = app.shell.tabs.get(tab_index).expect("inactive tab").id;
        let request_id = app.shell.indexing.allocate_request_id(Some(tab_id));
        {
            let tab = app.shell.tabs.get_mut(tab_index).expect("inactive tab");
            tab.index_state.pending_index_request_id = Some(request_id);
            tab.index_state.index_in_progress = true;
        }
        app.shell.indexing.background_states.insert(
            request_id,
            BackgroundIndexState {
                source: Some(IndexSource::FileList(root.join("FileList.txt"))),
                entries: (0..100_000)
                    .map(|index| file_entry(root.join(format!("occupied-{tab_index}-{index}.txt"))))
                    .collect(),
                replaced: true,
            },
        );
        app.handle_background_index_response(
            tab_index,
            IndexResponse::Finished {
                request_id,
                source: IndexSource::FileList(root.join("FileList.txt")),
            },
        );
    }
    assert!(app.shell.indexing.background_finalizations.is_full());

    let promoted_index = 2;
    let promoted_tab_id = app.shell.tabs.get(promoted_index).expect("promoted tab").id;
    let request_id = app
        .shell
        .indexing
        .allocate_request_id(Some(promoted_tab_id));
    {
        let tab = app
            .shell
            .tabs
            .get_mut(promoted_index)
            .expect("promoted tab");
        tab.index_state.pending_index_request_id = Some(request_id);
        tab.index_state.index_in_progress = true;
        tab.index_state.build.index.entries = (0..20_000)
            .map(|index| file_entry(root.join(format!("partial-{index}.txt"))))
            .collect();
        tab.index_state.pending_index_entries_request_id = Some(request_id);
        tab.index_state.build.pending_entries = (0..20_000)
            .map(|index| IndexEntry {
                path: root.join(format!("pending-{index}.txt")),
                kind: EntryKind::file(),
                kind_known: true,
            })
            .collect();
    }
    app.shell.indexing.background_states.insert(
        request_id,
        BackgroundIndexState {
            source: Some(IndexSource::FileList(root.join("FileList.txt"))),
            entries: (0..60_000)
                .map(|index| file_entry(root.join(format!("continuation-{index}.txt"))))
                .collect(),
            replaced: false,
        },
    );

    app.switch_to_tab_index(promoted_index);
    assert_eq!(app.current_tab_id(), Some(promoted_tab_id));
    let mailbox = app
        .shell
        .indexing
        .response_mailboxes
        .lock()
        .expect("mailboxes")
        .get(&request_id)
        .cloned()
        .expect("promoted mailbox");
    mailbox
        .try_publish(IndexResponse::Finished {
            request_id,
            source: IndexSource::FileList(root.join("FileList.txt")),
        })
        .expect("publish promoted terminal");
    app.shell.indexing.inflight_requests.insert(request_id);
    app.poll_index_response_with_budget_for_test(Duration::from_millis(4));
    assert!(mailbox.has_terminal_response());
    assert!(!app.shell.indexing.inflight_requests.contains(&request_id));
    assert!(app.shell.indexing.pending_finish.is_none());
    assert!(app
        .shell
        .indexing
        .background_states
        .contains_key(&request_id));

    let admitted_request_id = *app
        .shell
        .indexing
        .background_finalizations
        .keys()
        .next()
        .expect("occupied finalizer");
    let admitted_tab_id = *app
        .shell
        .indexing
        .request_tabs
        .get(&admitted_request_id)
        .expect("finalizer owner");
    let admitted_tab_index = app
        .find_tab_index_by_id(admitted_tab_id)
        .expect("finalizer tab");
    let _ = settle_background_finish_with_frame_budget(
        &mut app,
        admitted_tab_index,
        admitted_request_id,
        IndexSource::FileList(root.join("FileList.txt")),
    );

    let mut settled = false;
    for _ in 0..2_000 {
        app.poll_index_response_with_budget_for_test(Duration::from_millis(4));
        if app.shell.indexing.pending_finish.is_none()
            && !app
                .shell
                .indexing
                .background_states
                .contains_key(&request_id)
            && !app
                .shell
                .indexing
                .background_finalizations
                .contains_key(&request_id)
        {
            settled = true;
            break;
        }
    }
    assert!(settled, "promoted terminal barrier did not settle");
    assert_eq!(app.shell.runtime.all_entries.len(), 100_000);
    assert!(app
        .shell
        .runtime
        .all_entries
        .iter()
        .any(|entry| entry.path.ends_with("partial-0.txt")));
    assert!(app
        .shell
        .runtime
        .all_entries
        .iter()
        .any(|entry| entry.path.ends_with("pending-0.txt")));
    assert!(app
        .shell
        .runtime
        .all_entries
        .iter()
        .any(|entry| entry.path.ends_with("continuation-0.txt")));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_replace_all_discarded_build_moves_to_reclaimer_without_ui_drop() {
    let root = test_root("tc-207-replace-all-discard-reclaimer");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    let tab_id = app.shell.tabs.get(0).expect("background tab").id;
    let request_id = app.shell.indexing.allocate_request_id(Some(tab_id));
    {
        let tab = app.shell.tabs.get_mut(0).expect("background tab");
        tab.index_state.pending_index_request_id = Some(request_id);
        tab.index_state.pending_index_entries_request_id = Some(request_id);
        tab.index_state.index_in_progress = true;
        tab.index_state.build.index.entries = (0..50_000)
            .map(|index| file_entry(root.join(format!("old-{index}.txt"))))
            .collect();
        tab.index_state.build.pending_entries = (0..50_000)
            .map(|index| IndexEntry {
                path: root.join(format!("pending-{index}.txt")),
                kind: EntryKind::file(),
                kind_known: true,
            })
            .collect();
    }
    app.shell.indexing.background_states.insert(
        request_id,
        BackgroundIndexState {
            source: Some(IndexSource::FileList(root.join("FileList.txt"))),
            entries: vec![file_entry(root.join("replacement.txt"))],
            replaced: true,
        },
    );
    let _observer_guard = lock_reclaim_drop_observer_for_test();
    let (drop_tx, drop_rx) = mpsc::channel();
    set_reclaim_drop_observer(Some(drop_tx));

    let started = Instant::now();
    app.handle_background_index_response(
        0,
        IndexResponse::Finished {
            request_id,
            source: IndexSource::FileList(root.join("FileList.txt")),
        },
    );
    assert!(started.elapsed() < Duration::from_millis(100));

    set_reclaim_drop_observer(None);
    let tab = app.shell.tabs.get(0).expect("background tab");
    assert_eq!(tab.result_state.committed.all_entries.len(), 1);
    assert_eq!(
        tab.result_state.committed.all_entries[0].path,
        root.join("replacement.txt")
    );
    let drop_threads = collect_drop_threads_until(
        &drop_rx,
        "flistwalker-tab-reclaimer",
        Duration::from_millis(250),
    );
    assert!(
        drop_threads
            .iter()
            .any(|name| name == "flistwalker-tab-reclaimer"),
        "drop threads: {drop_threads:?}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_background_failure_reclaims_tab_and_coordinator_building_off_ui() {
    let root = test_root("tc-207-background-failure-reclaim");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    let background_tab_id = app.shell.tabs.get(0).expect("background tab").id;
    {
        let tab = app.shell.tabs.get_mut(0).expect("background tab");
        tab.index_state
            .set_lifecycle_for_test(TabResourceLifecycle::Loading);
        tab.index_state.pending_index_request_id = Some(1_207);
        tab.index_state.index_in_progress = true;
        tab.index_state.build.index.entries = (0..1_000)
            .map(|index| file_entry(root.join(format!("tab-building-{index}.txt"))))
            .collect();
    }
    app.shell
        .indexing
        .request_tabs
        .insert(1_207, background_tab_id);
    app.shell.indexing.background_states.insert(
        1_207,
        BackgroundIndexState {
            source: Some(IndexSource::Walker),
            entries: (0..1_000)
                .map(|index| file_entry(root.join(format!("coordinator-{index}.txt"))))
                .collect(),
            replaced: true,
        },
    );
    let _observer_guard = lock_reclaim_drop_observer_for_test();
    let (drop_tx, drop_rx) = mpsc::channel();
    set_reclaim_drop_observer(Some(drop_tx));

    app.handle_background_index_response(
        0,
        IndexResponse::Failed {
            request_id: 1_207,
            error: "fixture failure".to_string(),
        },
    );

    set_reclaim_drop_observer(None);
    let tab = app.shell.tabs.get(0).expect("background tab");
    assert!(!tab.index_state.build_reclaim_pending);
    assert!(tab.index_state.pending_index_request_id.is_none());
    assert_eq!(tab.index_state.build.index.entries.capacity(), 0);
    assert!(!app.shell.indexing.background_states.contains_key(&1_207));
    let drop_threads = collect_drop_threads_until(
        &drop_rx,
        "flistwalker-tab-reclaimer",
        Duration::from_millis(250),
    );
    assert!(
        drop_threads
            .iter()
            .any(|name| name == "flistwalker-tab-reclaimer"),
        "drop threads: {drop_threads:?}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_background_replace_all_waits_for_reclaimer_and_preserves_old_build() {
    let root = test_root("tc-207-background-replace-all-full");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    let background_tab_id = app.shell.tabs.get(0).expect("background tab").id;
    let request_id = app
        .shell
        .indexing
        .allocate_request_id(Some(background_tab_id));
    {
        let tab = app.shell.tabs.get_mut(0).expect("background tab");
        tab.index_state.pending_index_request_id = Some(request_id);
        tab.index_state.index_in_progress = true;
    }
    app.shell.indexing.background_states.insert(
        request_id,
        BackgroundIndexState {
            source: Some(IndexSource::FileList(root.join("FileList.txt"))),
            entries: (0..2_000)
                .map(|index| file_entry(root.join(format!("old-{index}.txt"))))
                .collect(),
            replaced: false,
        },
    );
    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut tab = app.capture_active_tab_state(3_200 + index as u64);
        tab.result_state.committed.all_entries =
            Arc::new(vec![file_entry(root.join(format!("held-{index}.txt")))]);
        app.shell
            .tabs
            .retire_tab_resources_for_test(tab.take_heavy_resources())
            .expect("fill reclaimer");
    }

    assert!(
        !app.try_apply_replace_all_response(IndexResponse::ReplaceAll {
            request_id,
            entries: vec![IndexEntry {
                path: root.join("replacement.txt"),
                kind: EntryKind::file(),
                kind_known: true,
            }],
        })
    );

    assert!(app.shell.indexing.pending_replace_all.is_some());
    assert_eq!(
        app.shell
            .indexing
            .background_states
            .get(&request_id)
            .expect("old state restored")
            .entries
            .len(),
        2_000
    );

    app.shell.tabs.resume_resource_reclaimer();
    app.poll_index_response();

    let replacement = app
        .shell
        .indexing
        .background_states
        .get(&request_id)
        .expect("replacement state");
    assert!(app.shell.indexing.pending_replace_all.is_none());
    assert!(replacement.replaced);
    assert_eq!(replacement.entries.len(), 1);
    assert_eq!(replacement.entries[0].path, root.join("replacement.txt"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_background_failure_debt_switches_active_and_cleans_routing() {
    let root = test_root("tc-207-background-debt-switch-active");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    let background_tab_id = app.shell.tabs.get(0).expect("background tab").id;
    let request_id = app
        .shell
        .indexing
        .allocate_request_id(Some(background_tab_id));
    {
        let tab = app.shell.tabs.get_mut(0).expect("background tab");
        tab.index_state.pending_index_request_id = Some(request_id);
        tab.index_state.index_in_progress = true;
        tab.index_state.build.index.entries = (0..2_000)
            .map(|index| file_entry(root.join(format!("building-{index}.txt"))))
            .collect();
    }
    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut tab = app.capture_active_tab_state(3_300 + index as u64);
        tab.result_state.committed.all_entries =
            Arc::new(vec![file_entry(root.join(format!("held-{index}.txt")))]);
        app.shell
            .tabs
            .retire_tab_resources_for_test(tab.take_heavy_resources())
            .expect("fill reclaimer");
    }

    app.handle_background_index_response(
        0,
        IndexResponse::Failed {
            request_id,
            error: "fixture failure".to_string(),
        },
    );
    assert!(
        app.shell
            .tabs
            .get(0)
            .unwrap()
            .index_state
            .build_reclaim_pending
    );

    app.switch_to_tab_index(0);
    assert_eq!(app.current_tab_id(), Some(background_tab_id));
    assert!(app.shell.indexing.build_reclaim_pending);

    app.shell.tabs.resume_resource_reclaimer();
    app.poll_index_response();

    assert!(!app.shell.indexing.build_reclaim_pending);
    assert!(app.shell.indexing.build_reclaim_request_id.is_none());
    assert!(!app.shell.indexing.request_tabs.contains_key(&request_id));
    assert!(!app
        .shell
        .indexing
        .response_mailboxes
        .lock()
        .expect("mailboxes")
        .contains_key(&request_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_midflight_background_close_reclaims_coordinator_and_mailbox_off_ui() {
    let root = test_root("tc-207-midflight-background-close");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    assert_eq!(app.shell.tabs.active_tab_index(), 1);
    let closing_tab_id = app.shell.tabs.get(0).expect("closing tab").id;
    let request_id = app.shell.indexing.allocate_request_id(Some(closing_tab_id));
    {
        let tab = app.shell.tabs.get_mut(0).expect("closing tab");
        tab.index_state.pending_index_request_id = Some(request_id);
        tab.index_state.index_in_progress = true;
    }
    app.shell.indexing.background_states.insert(
        request_id,
        BackgroundIndexState {
            source: Some(IndexSource::Walker),
            entries: (0..1_000)
                .map(|index| file_entry(root.join(format!("state-{index}.txt"))))
                .collect(),
            replaced: false,
        },
    );
    let mailbox = app
        .shell
        .indexing
        .response_mailboxes
        .lock()
        .expect("mailboxes")
        .get(&request_id)
        .cloned()
        .expect("request mailbox");
    mailbox
        .try_publish(IndexResponse::Batch {
            request_id,
            entries: (0..1_000)
                .map(|index| IndexEntry {
                    path: root.join(format!("mailbox-{index}.txt")),
                    kind: EntryKind::file(),
                    kind_known: true,
                })
                .collect(),
        })
        .expect("seed mailbox payload");
    drop(mailbox);
    let _observer_guard = lock_reclaim_drop_observer_for_test();
    let (drop_tx, drop_rx) = mpsc::channel();
    set_reclaim_drop_observer(Some(drop_tx));
    assert!(app.shell.tabs.get(0).unwrap().index_state.index_in_progress);
    assert_eq!(
        app.shell
            .tabs
            .get(0)
            .unwrap()
            .index_state
            .pending_index_request_id,
        Some(request_id)
    );

    app.close_tab_index(0);

    set_reclaim_drop_observer(None);
    assert_eq!(app.shell.tabs.len(), 1);
    assert_eq!(app.shell.tabs.closed_tab_count(), 1);
    assert!(!app.shell.indexing.request_tabs.contains_key(&request_id));
    assert!(!app
        .shell
        .indexing
        .background_states
        .contains_key(&request_id));
    assert!(!app
        .shell
        .indexing
        .response_mailboxes
        .lock()
        .expect("mailboxes")
        .contains_key(&request_id));
    let drop_threads = collect_drop_threads_until(
        &drop_rx,
        "flistwalker-tab-reclaimer",
        Duration::from_millis(250),
    );
    assert!(
        drop_threads
            .iter()
            .any(|name| name == "flistwalker-tab-reclaimer"),
        "drop threads: {drop_threads:?}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_multigeneration_background_close_reclaims_every_request_owner() {
    let root = test_root("tc-207-multigeneration-background-close");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    let tab_id = app.shell.tabs.get(0).expect("closing tab").id;
    let superseded_request_id = app.shell.indexing.allocate_request_id(Some(tab_id));
    let current_request_id = app.shell.indexing.allocate_request_id(Some(tab_id));
    {
        let tab = app.shell.tabs.get_mut(0).expect("closing tab");
        tab.index_state.pending_index_request_id = Some(current_request_id);
        tab.index_state.index_in_progress = true;
    }
    for request_id in [superseded_request_id, current_request_id] {
        app.shell.indexing.background_states.insert(
            request_id,
            BackgroundIndexState {
                source: Some(IndexSource::Walker),
                entries: (0..1_000)
                    .map(|index| file_entry(root.join(format!("state-{request_id}-{index}.txt"))))
                    .collect(),
                replaced: false,
            },
        );
        let mailbox = app
            .shell
            .indexing
            .response_mailboxes
            .lock()
            .expect("mailboxes")
            .get(&request_id)
            .cloned()
            .expect("mailbox");
        mailbox
            .try_publish(IndexResponse::Batch {
                request_id,
                entries: vec![IndexEntry {
                    path: root.join(format!("mailbox-{request_id}.txt")),
                    kind: EntryKind::file(),
                    kind_known: true,
                }],
            })
            .expect("seed mailbox");
    }
    let _observer_guard = lock_reclaim_drop_observer_for_test();
    let (drop_tx, drop_rx) = mpsc::channel();
    set_reclaim_drop_observer(Some(drop_tx));

    app.close_tab_index(0);

    set_reclaim_drop_observer(None);
    assert_eq!(app.shell.tabs.len(), 1);
    assert_eq!(app.shell.tabs.closed_tab_count(), 1);
    for request_id in [superseded_request_id, current_request_id] {
        assert!(!app.shell.indexing.request_tabs.contains_key(&request_id));
        assert!(!app
            .shell
            .indexing
            .background_states
            .contains_key(&request_id));
        assert!(!app
            .shell
            .indexing
            .response_mailboxes
            .lock()
            .expect("mailboxes")
            .contains_key(&request_id));
    }
    let drop_threads = collect_drop_threads_until(
        &drop_rx,
        "flistwalker-tab-reclaimer",
        Duration::from_millis(250),
    );
    assert!(
        drop_threads
            .iter()
            .any(|name| name == "flistwalker-tab-reclaimer"),
        "drop threads: {drop_threads:?}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_multigeneration_background_close_rolls_back_every_owner_when_full() {
    let root = test_root("tc-207-multigeneration-background-close-full");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    let tab_id = app.shell.tabs.get(0).expect("closing tab").id;
    let superseded_request_id = app.shell.indexing.allocate_request_id(Some(tab_id));
    let current_request_id = app.shell.indexing.allocate_request_id(Some(tab_id));
    {
        let tab = app.shell.tabs.get_mut(0).expect("closing tab");
        tab.index_state.pending_index_request_id = Some(current_request_id);
        tab.index_state.index_in_progress = true;
    }
    for request_id in [superseded_request_id, current_request_id] {
        app.shell.indexing.background_states.insert(
            request_id,
            BackgroundIndexState {
                source: Some(IndexSource::Walker),
                entries: vec![file_entry(root.join(format!("state-{request_id}.txt")))],
                replaced: false,
            },
        );
        let mailbox = app
            .shell
            .indexing
            .response_mailboxes
            .lock()
            .expect("mailboxes")
            .get(&request_id)
            .cloned()
            .expect("mailbox");
        mailbox
            .try_publish(IndexResponse::Batch {
                request_id,
                entries: vec![IndexEntry {
                    path: root.join(format!("mailbox-{request_id}.txt")),
                    kind: EntryKind::file(),
                    kind_known: true,
                }],
            })
            .expect("seed mailbox");
    }
    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut held = app.capture_active_tab_state(7_000 + index as u64);
        held.result_state.committed.all_entries =
            Arc::new(vec![file_entry(root.join(format!("held-{index}.txt")))]);
        app.shell
            .tabs
            .retire_tab_resources_for_test(held.take_heavy_resources())
            .expect("fill reclaimer");
    }

    app.close_tab_index(0);

    assert_eq!(app.shell.tabs.len(), 2);
    assert_eq!(app.shell.tabs.closed_tab_count(), 0);
    assert_eq!(
        app.shell
            .tabs
            .get(0)
            .expect("closing tab retained")
            .index_state
            .pending_index_request_id,
        Some(current_request_id)
    );
    for request_id in [superseded_request_id, current_request_id] {
        assert_eq!(
            app.shell.indexing.request_tabs.get(&request_id),
            Some(&tab_id)
        );
        assert!(app
            .shell
            .indexing
            .background_states
            .contains_key(&request_id));
        let mailbox = app
            .shell
            .indexing
            .response_mailboxes
            .lock()
            .expect("mailboxes")
            .get(&request_id)
            .cloned()
            .expect("mailbox restored");
        assert!(mailbox.has_payload());
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_background_terminal_debt_coalesces_refresh_to_one_follow_up() {
    let root = test_root("tc-207-background-terminal-follow-up");
    fs::create_dir_all(&root).expect("create root");
    let old = file_entry(root.join("old.txt"));
    let new = file_entry(root.join("new.txt"));
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    let background_tab_id = app.shell.tabs.get(0).expect("background tab").id;
    let (request_tx, request_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = request_tx;
    app.shell.indexing.pending_queue.clear();
    app.shell.indexing.inflight_requests.clear();
    app.shell.indexing.request_tabs.clear();
    {
        let tab = app.shell.tabs.get_mut(0).expect("background tab");
        tab.index_state
            .set_lifecycle_for_test(TabResourceLifecycle::Refreshing);
        tab.index_state
            .set_committed_snapshot_present_for_test(true);
        tab.result_state.committed.all_entries = Arc::new(vec![old.clone()]);
        tab.result_state.committed.entries = Arc::new(vec![old]);
        tab.index_state.build.index.entries = vec![new];
        tab.index_state.pending_index_request_id = Some(607);
        tab.index_state.pending_index_finish = Some(PendingActiveIndexFinish {
            request_id: 607,
            source: IndexSource::Walker,
        });
    }
    app.shell
        .indexing
        .request_tabs
        .insert(607, background_tab_id);
    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut tab = app.capture_active_tab_state(1_700 + index as u64);
        tab.result_state.committed.all_entries =
            Arc::new(vec![file_entry(root.join(format!("held-{index}.txt")))]);
        app.shell
            .tabs
            .retire_tab_resources_for_test(tab.take_heavy_resources())
            .expect("fill reclaimer");
    }
    let next_request_id = app.shell.indexing.next_request_id;

    app.request_background_index_refresh_for_tab(0);
    app.request_background_index_refresh_for_tab(0);

    let waiting = app.shell.tabs.get(0).expect("background tab");
    assert_eq!(waiting.index_state.pending_index_request_id, Some(607));
    assert_eq!(waiting.index_state.build.index.entries.len(), 1);
    assert_eq!(app.shell.indexing.next_request_id, next_request_id);
    assert!(request_rx.try_recv().is_err());

    app.shell.tabs.resume_resource_reclaimer();
    app.poll_index_response();

    let replay = request_rx
        .try_recv()
        .expect("one coalesced background refresh");
    assert_eq!(replay.tab_id, background_tab_id);
    assert!(request_rx.try_recv().is_err());
    assert_eq!(app.shell.indexing.next_request_id, next_request_id + 1);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_background_search_restores_evicted_selected_path() {
    let root = test_root("tc-207-background-selected-path");
    fs::create_dir_all(&root).expect("create root");
    let first = root.join("first.txt");
    let selected = root.join("selected.txt");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    let background_tab_id = app.shell.tabs.get(0).expect("background tab").id;
    {
        let tab = app.shell.tabs.get_mut(0).expect("background tab");
        tab.query_state.query = "selected".to_string();
        tab.result_state.evicted_selected_path = Some(selected.clone());
        tab.index_state.index_in_progress = true;
        tab.pending_request_id = Some(707);
        tab.search_in_progress = true;
    }

    app.apply_background_search_response(
        background_tab_id,
        SearchResponse {
            request_id: 706,
            results: vec![(first.clone(), 2.0)],
            total_match_count: 1,
            sort_mode: ResultSortMode::Score,
            sort_scope: ResultSortScope::ShownResults,
            error: None,
        },
    );
    assert_eq!(
        app.shell
            .tabs
            .get(0)
            .expect("background tab")
            .result_state
            .evicted_selected_path
            .as_ref(),
        Some(&selected),
        "background partial search must retain the restore intent"
    );
    app.shell
        .tabs
        .get_mut(0)
        .expect("background tab")
        .index_state
        .index_in_progress = false;

    app.apply_background_search_response(
        background_tab_id,
        SearchResponse {
            request_id: 707,
            results: vec![(first, 2.0), (selected.clone(), 1.0)],
            total_match_count: 2,
            sort_mode: ResultSortMode::Score,
            sort_scope: ResultSortScope::ShownResults,
            error: None,
        },
    );

    let tab = app.shell.tabs.get(0).expect("background tab");
    assert_eq!(tab.result_state.committed.current_row, Some(1));
    assert_eq!(tab.result_state.committed.results[1].0, selected);
    assert!(tab.result_state.evicted_selected_path.is_none());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn canceled_background_preview_settles_and_reloads_on_activation() {
    let root = test_root("preview-cancel-owner");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("sample.txt");
    fs::write(&path, "sample").unwrap();
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.entries = Arc::new(vec![file_entry(path.clone())]);
    app.shell.runtime.results = vec![(path.clone(), 1.0)];
    app.shell.runtime.base_results = app.shell.runtime.results.clone();
    app.shell.runtime.current_row = Some(0);
    app.set_entry_kind(&path, EntryKind::file());
    let (tx, rx) = mpsc::channel();
    app.shell.worker_bus.preview.tx = tx;
    app.request_preview_for_current();
    let obsolete = rx.try_recv().unwrap();
    app.request_preview_for_current();
    let request = rx.try_recv().unwrap();
    app.create_new_tab();
    while rx.try_recv().is_ok() {}
    app.apply_background_preview_response(PreviewResponse {
        request_id: obsolete.request_id,
        path: path.clone(),
        preview: String::new(),
        canceled: true,
    });
    assert_eq!(
        app.shell.tabs.get(0).unwrap().pending_preview_request_id,
        Some(request.request_id)
    );
    app.apply_background_preview_response(PreviewResponse {
        request_id: request.request_id,
        path: path.clone(),
        preview: String::new(),
        canceled: true,
    });
    let tab = app.shell.tabs.get(0).unwrap();
    assert!(!tab.preview_in_progress);
    assert!(tab.pending_preview_request_id.is_none());
    assert!(tab.preview_reload_pending);
    assert_eq!(app.preview_request_tab(request.request_id), None);
    app.switch_to_tab_index(0);
    assert!(rx.try_iter().any(|request| request.path == path));
    let _ = fs::remove_dir_all(root);
}

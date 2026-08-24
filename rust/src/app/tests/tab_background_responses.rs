use super::*;

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
    assert!(first_tab.result_state.results.is_empty());
    assert!(first_tab.result_state.results_compacted);
    assert_eq!(first_tab.result_state.base_results.len(), 1);
    assert_eq!(first_tab.result_state.base_results[0].0, selected);
    assert_eq!(first_tab.result_state.preview, "preview-body");
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
    assert!(background.result_state.preview.is_empty());
    assert!(background.pending_preview_request_id.is_none());
    assert!(background.preview_reload_pending);
    assert_eq!(app.preview_request_tab(old_request.request_id), None);
    app.apply_background_preview_response(PreviewResponse {
        request_id: old_request.request_id,
        path: old_path,
        preview: "late old preview".to_string(),
    });
    app.shell
        .tabs
        .get_mut(0)
        .expect("background tab")
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
    assert!(background.result_state.preview.is_empty());
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
        .preview
        .is_empty());

    app.shell
        .tabs
        .get_mut(0)
        .expect("background tab")
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
    assert_eq!(background.result_state.base_results[0].0, old_path);
    assert_eq!(background.result_state.results[0].0, new_path);
    assert!(background.result_state.preview.is_empty());
    assert!(background.pending_preview_request_id.is_none());
    assert!(background.preview_reload_pending);
    assert_eq!(app.preview_request_tab(94), None);

    app.shell
        .tabs
        .get_mut(0)
        .expect("background tab")
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
    assert_eq!(background.result_state.total_match_count, 1);
    assert_eq!(
        background.result_state.base_results,
        vec![(current.clone(), 0.0)]
    );
    assert_eq!(background.result_state.results, vec![(current, 0.0)]);
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
    assert_eq!(app.shell.runtime.index.entries.len(), 1);

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

    app.request_index_refresh();
    let index_req = index_req_rx.try_recv().expect("index request");
    app.shell.runtime.index.entries = vec![file_entry(drained_file.clone())];
    app.shell.indexing.pending_entries_request_id = Some(index_req.request_id);
    app.shell.indexing.pending_entries.push_back(IndexEntry {
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
    assert!(app.shell.indexing.pending_entries.is_empty());

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

    app.request_index_refresh();
    let index_req = index_req_rx.try_recv().expect("index request");
    app.shell.runtime.index.entries = vec![file_entry(stale_file.clone())];
    app.shell.indexing.pending_entries_request_id = Some(index_req.request_id);
    app.shell.indexing.pending_entries.push_back(IndexEntry {
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
    assert_eq!(background_tab.result_state.results.len(), 1);
    assert_eq!(background_tab.result_state.total_match_count, 2);

    app.switch_to_tab_index(0);
    assert_eq!(app.shell.runtime.results.len(), 1);
    assert_eq!(app.shell.runtime.total_match_count, 2);
    assert!(app.status_line_text().contains("Results: 1 of 2 shown"));

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
    let (search_tx_req, _search_rx_req) = mpsc::channel::<SearchRequest>();
    let (search_tx_res, search_rx_res) = mpsc::channel::<SearchResponse>();
    app.shell.search.tx = search_tx_req;
    app.shell.search.rx = search_rx_res;

    app.shell.runtime.entries = Arc::new(vec![file_entry(active_file.clone())]);
    app.shell.runtime.all_entries = Arc::new(vec![file_entry(active_file.clone())]);
    app.shell.runtime.results = vec![(active_file.clone(), 0.0)];
    app.shell.runtime.base_results = app.shell.runtime.results.clone();
    app.shell.runtime.current_row = Some(0);
    app.sync_active_tab_state();

    app.create_new_tab();
    assert_eq!(app.shell.tabs.active_tab, 1);
    app.shell.runtime.entries = Arc::new(vec![file_entry(active_file.clone())]);
    app.shell.runtime.all_entries = Arc::new(vec![file_entry(active_file.clone())]);
    app.shell.runtime.results = vec![(active_file.clone(), 0.0)];
    app.shell.runtime.base_results = app.shell.runtime.results.clone();
    app.shell.runtime.current_row = Some(0);
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
    };
    app.shell
        .indexing
        .request_tabs
        .insert(88, background_tab_id);
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
    assert_eq!(
        app.shell
            .tabs
            .get(0)
            .expect("tab 0")
            .result_state
            .base_results
            .len(),
        1
    );
    assert_eq!(
        app.shell
            .tabs
            .get(0)
            .expect("tab 0")
            .result_state
            .base_results[0]
            .0,
        background_file
    );
    assert_eq!(
        app.shell
            .tabs
            .get(0)
            .expect("tab 0")
            .index_state
            .entries
            .len(),
        1
    );
    assert_eq!(
        app.shell.tabs.get(0).expect("tab 0").index_state.entries[0],
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

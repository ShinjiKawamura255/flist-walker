use super::*;

#[test]
fn queued_request_for_tab_exists_is_false_when_queue_is_empty() {
    let root = test_root("pipeline-queue-empty");
    fs::create_dir_all(&root).expect("create dir");
    let app = FlistWalkerApp::new(root.clone(), 50, String::new());

    assert!(!app.queued_request_for_tab_exists(1));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn queued_request_for_tab_exists_is_true_for_matching_tab() {
    let root = test_root("pipeline-queue-match");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let tab_id = app.current_tab_id().expect("tab id");
    app.indexing.pending_queue.push_back(IndexRequest {
        request_id: 7,
        tab_id,
        root: root.clone(),
        use_filelist: true,
        include_files: true,
        include_dirs: true,
    });

    assert!(app.queued_request_for_tab_exists(tab_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn has_inflight_for_tab_uses_request_tab_mapping() {
    let root = test_root("pipeline-inflight");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let tab_id = app.current_tab_id().expect("tab id");
    app.indexing.request_tabs.insert(11, tab_id);
    app.indexing.inflight_requests.insert(11);

    assert!(app.has_inflight_for_tab(tab_id));
    assert!(!app.has_inflight_for_tab(tab_id.saturating_add(1)));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn should_refresh_incremental_search_is_false_when_delta_is_zero() {
    let root = test_root("pipeline-refresh-zero");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.indexing.incremental_filtered_entries = vec![unknown_entry(root.join("a.txt"))];
    app.indexing.last_search_snapshot_len = 1;

    assert!(!app.should_refresh_incremental_search());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn should_refresh_incremental_search_is_false_for_small_delta_while_indexing() {
    let root = test_root("pipeline-refresh-small-delta");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "main".to_string());
    app.indexing.in_progress = true;
    app.indexing.incremental_filtered_entries = (0..64)
        .map(|i| unknown_entry(root.join(format!("file-{i}.txt"))))
        .collect();
    app.indexing.last_search_snapshot_len = 0;
    app.indexing.last_incremental_results_refresh =
        Instant::now() - FlistWalkerApp::INCREMENTAL_SEARCH_REFRESH_INTERVAL_DURING_INDEX;

    assert!(!app.should_refresh_incremental_search());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn should_refresh_incremental_search_is_true_for_large_delta_after_interval() {
    let root = test_root("pipeline-refresh-large-delta");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "main".to_string());
    app.indexing.in_progress = true;
    app.indexing.incremental_filtered_entries = (0
        ..(FlistWalkerApp::INCREMENTAL_SEARCH_MIN_DELTA_DURING_INDEX + 1))
        .map(|i| unknown_entry(root.join(format!("file-{i}.txt"))))
        .collect();
    app.indexing.last_search_snapshot_len = 0;
    app.indexing.last_incremental_results_refresh =
        Instant::now() - FlistWalkerApp::INCREMENTAL_SEARCH_REFRESH_INTERVAL_DURING_INDEX;

    assert!(app.should_refresh_incremental_search());
    let _ = fs::remove_dir_all(&root);
}

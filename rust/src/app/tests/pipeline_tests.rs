use super::*;

fn wait_for_filelist_index_settlement(app: &mut FlistWalkerApp, deadline: Instant) {
    while app.shell.indexing.pending_request_id.is_some() {
        assert!(
            Instant::now() < deadline,
            "tiny local FileList index must settle before the GUI liveness deadline"
        );
        app.poll_index_response_with_budget_for_test(Duration::from_millis(10));
        thread::yield_now();
    }
    assert!(matches!(
        app.shell.runtime.index.source,
        IndexSource::FileList(_)
    ));
}

#[test]
fn tc_152_startup_and_refresh_settle_filelist_source_and_entries_regression() {
    let root = test_root("pipeline-native-filelist-settlement");
    fs::create_dir_all(&root).expect("create root");
    fs::write(root.join("first.txt"), "first").expect("write first entry");
    fs::write(root.join("second.txt"), "second").expect("write second entry");
    fs::write(root.join("FileList.txt"), "first.txt\n").expect("write startup FileList");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    wait_for_filelist_index_settlement(&mut app, Instant::now() + Duration::from_secs(2));
    assert_eq!(app.shell.runtime.all_entries.len(), 1);
    assert!(app.shell.runtime.all_entries.iter().any(|entry| entry
        .path
        .file_name()
        .is_some_and(|name| name == "first.txt")));

    fs::write(root.join("FileList.txt"), "first.txt\nsecond.txt\n")
        .expect("update refresh FileList");
    app.request_index_refresh();
    wait_for_filelist_index_settlement(&mut app, Instant::now() + Duration::from_secs(2));
    assert_eq!(app.shell.runtime.all_entries.len(), 2);
    assert!(app.shell.runtime.all_entries.iter().any(|entry| entry
        .path
        .file_name()
        .is_some_and(|name| name == "second.txt")));

    let _ = fs::remove_dir_all(&root);
}

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
    app.shell.indexing.pending_queue.push_back(IndexRequest {
        request_id: 7,
        tab_id,
        root: root.clone(),
        use_filelist: true,
        include_files: true,
        include_dirs: true,
        max_depth: crate::indexer::MaxDepth::unlimited(),
    });

    assert!(app.queued_request_for_tab_exists(tab_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn superseded_queued_index_request_releases_its_route() {
    let root = test_root("pipeline-superseded-route");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let tab_id = app.current_tab_id().expect("tab id");
    let request = |request_id| IndexRequest {
        request_id,
        tab_id,
        root: root.clone(),
        use_filelist: true,
        include_files: true,
        include_dirs: true,
        max_depth: crate::indexer::MaxDepth::unlimited(),
    };
    app.shell.indexing.request_tabs.insert(7, tab_id);
    app.shell.indexing.request_tabs.insert(8, tab_id);
    app.shell.indexing.pending_queue.push_back(request(7));

    app.enqueue_index_request(request(8));

    assert_eq!(app.shell.indexing.pending_queue.len(), 1);
    assert_eq!(app.shell.indexing.pending_queue[0].request_id, 8);
    assert_eq!(app.shell.indexing.request_tabs.get(&7), None);
    assert_eq!(app.shell.indexing.request_tabs.get(&8), Some(&tab_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn has_inflight_for_tab_uses_request_tab_mapping() {
    let root = test_root("pipeline-inflight");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let tab_id = app.current_tab_id().expect("tab id");
    app.shell.indexing.request_tabs.insert(11, tab_id);
    app.shell.indexing.inflight_requests.insert(11);

    assert!(app.has_inflight_for_tab(tab_id));
    assert!(!app.has_inflight_for_tab(tab_id.saturating_add(1)));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn should_refresh_incremental_search_is_false_when_delta_is_zero() {
    let root = test_root("pipeline-refresh-zero");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.indexing.incremental_filtered_entries = vec![unknown_entry(root.join("a.txt"))];
    app.shell.indexing.last_search_snapshot_len = 1;

    assert!(!app.should_refresh_incremental_search());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn should_refresh_incremental_search_is_false_for_small_delta_while_indexing() {
    let root = test_root("pipeline-refresh-small-delta");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "main".to_string());
    app.shell.indexing.in_progress = true;
    app.shell.indexing.incremental_filtered_entries = (0..64)
        .map(|i| unknown_entry(root.join(format!("file-{i}.txt"))))
        .collect();
    app.shell.indexing.last_search_snapshot_len = 0;
    app.shell.indexing.last_incremental_results_refresh =
        Instant::now() - FlistWalkerApp::INCREMENTAL_SEARCH_REFRESH_INTERVAL_DURING_INDEX;

    assert!(!app.should_refresh_incremental_search());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn should_refresh_incremental_search_is_true_for_large_delta_after_interval() {
    let root = test_root("pipeline-refresh-large-delta");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "main".to_string());
    app.shell.indexing.in_progress = true;
    app.shell.indexing.incremental_filtered_entries = (0
        ..(FlistWalkerApp::INCREMENTAL_SEARCH_MIN_DELTA_DURING_INDEX + 1))
        .map(|i| unknown_entry(root.join(format!("file-{i}.txt"))))
        .collect();
    app.shell.indexing.last_search_snapshot_len = 0;
    app.shell.indexing.last_incremental_results_refresh =
        Instant::now() - FlistWalkerApp::INCREMENTAL_SEARCH_REFRESH_INTERVAL_DURING_INDEX;

    assert!(app.should_refresh_incremental_search());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn regression_ignore_list_is_applied_when_files_and_folders_are_both_enabled() {
    let root = test_root("ignore-list-fast-path-regression");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let kept = root.join("keep.txt");
    let ignored_old = root.join("old-cache.txt");
    let ignored_tilde = root.join("backup~.txt");

    app.shell.runtime.all_entries = Arc::new(vec![
        file_entry(ignored_old.clone()),
        file_entry(ignored_tilde.clone()),
        file_entry(kept.clone()),
    ]);
    app.shell.runtime.index.entries.clear();
    app.shell.runtime.index.source = IndexSource::Walker;
    app.shell.runtime.entries = Arc::new(Vec::new());
    app.shell.runtime.include_files = true;
    app.shell.runtime.include_dirs = true;
    app.shell.ui.ignore_list_enabled = true;
    app.shell.runtime.ignore_list_terms = Arc::new(vec!["old".to_string(), "~".to_string()]);

    app.apply_entry_filters(false);

    assert_eq!(
        app.shell.runtime.entries.as_ref(),
        &[file_entry(kept.clone())]
    );
    assert_eq!(app.shell.runtime.results, vec![(kept, 0.0)]);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn regression_ignore_list_toggle_off_keeps_all_entries_visible() {
    let root = test_root("ignore-list-toggle-off-regression");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let kept = root.join("keep.txt");
    let ignored_old = root.join("old-cache.txt");

    app.shell.runtime.all_entries = Arc::new(vec![
        file_entry(ignored_old.clone()),
        file_entry(kept.clone()),
    ]);
    app.shell.runtime.index.entries.clear();
    app.shell.runtime.index.source = IndexSource::Walker;
    app.shell.runtime.entries = Arc::new(Vec::new());
    app.shell.runtime.include_files = true;
    app.shell.runtime.include_dirs = true;
    app.shell.ui.ignore_list_enabled = false;
    app.shell.runtime.ignore_list_terms = Arc::new(vec!["old".to_string()]);

    app.apply_entry_filters(false);

    assert_eq!(
        app.shell.runtime.entries.as_ref(),
        &[file_entry(ignored_old.clone()), file_entry(kept.clone())]
    );
    assert_eq!(
        app.shell.runtime.results,
        vec![(ignored_old, 0.0), (kept, 0.0)]
    );
    let _ = fs::remove_dir_all(&root);
}

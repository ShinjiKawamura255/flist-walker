use super::*;

#[test]
fn close_tab_invalidates_memory_cache_for_immediate_resample() {
    let root = test_root("close-tab-memory-resample");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    assert_eq!(app.shell.tabs.len(), 2);

    let sentinel = u64::MAX;
    app.shell.ui.memory_usage_bytes = Some(sentinel);
    let stale = Instant::now()
        .checked_sub(Duration::from_secs(5))
        .unwrap_or_else(Instant::now);
    app.shell.ui.last_memory_sample = stale;

    app.close_tab_index(1);

    assert_eq!(app.shell.tabs.len(), 1);
    assert_ne!(app.shell.ui.memory_usage_bytes, Some(sentinel));
    assert!(app.shell.ui.last_memory_sample > stale);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn close_tab_clears_filelist_and_request_routing_for_removed_tab() {
    let root = test_root("close-tab-clears-routing");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    assert_eq!(app.shell.tabs.len(), 2);

    let removed_tab_id = app.shell.tabs.get(0).expect("tab 0").id;
    let survivor_tab_id = app.shell.tabs.get(1).expect("tab 1").id;
    let path = root.join("item.txt");
    fs::write(&path, "x").expect("write file");

    app.shell.features.filelist.workflow.pending_after_index = Some(PendingFileListAfterIndex {
        tab_id: removed_tab_id,
        root: root.clone(),
    });
    app.shell.features.filelist.workflow.pending_confirmation = Some(PendingFileListConfirmation {
        tab_id: removed_tab_id,
        root: root.clone(),
        entries: vec![path.clone()],
        existing_path: root.join("FileList.txt"),
    });
    app.shell
        .features
        .filelist
        .workflow
        .pending_ancestor_confirmation = Some(PendingFileListAncestorConfirmation {
        tab_id: removed_tab_id,
        root: root.clone(),
        entries: vec![path.clone()],
    });
    app.shell
        .features
        .filelist
        .workflow
        .pending_use_walker_confirmation = Some(PendingFileListUseWalkerConfirmation {
        source_tab_id: removed_tab_id,
        root: root.clone(),
    });

    app.shell.indexing.request_tabs.insert(11, removed_tab_id);
    app.shell.indexing.request_tabs.insert(12, survivor_tab_id);
    app.shell.indexing.pending_queue.push_back(IndexRequest {
        request_id: 11,
        tab_id: removed_tab_id,
        root: root.clone(),
        use_filelist: true,
        include_files: true,
        include_dirs: true,
    });
    app.shell.indexing.pending_queue.push_back(IndexRequest {
        request_id: 12,
        tab_id: survivor_tab_id,
        root: root.clone(),
        use_filelist: true,
        include_files: true,
        include_dirs: true,
    });
    if let Ok(mut latest) = app.shell.indexing.latest_request_ids.lock() {
        latest.insert(removed_tab_id, 11);
        latest.insert(survivor_tab_id, 12);
    }
    app.shell
        .indexing
        .background_states
        .insert(11, BackgroundIndexState::default());
    app.shell
        .indexing
        .background_states
        .insert(12, BackgroundIndexState::default());

    app.shell.search.bind_request_tab(21, removed_tab_id);
    app.shell.search.bind_request_tab(22, survivor_tab_id);
    app.bind_preview_request_to_tab(31, removed_tab_id);
    app.bind_preview_request_to_tab(32, survivor_tab_id);
    app.bind_action_request_to_tab(41, removed_tab_id);
    app.bind_action_request_to_tab(42, survivor_tab_id);
    app.bind_sort_request_to_tab(51, removed_tab_id);
    app.bind_sort_request_to_tab(52, survivor_tab_id);

    app.close_tab_index(0);

    assert_eq!(app.shell.tabs.len(), 1);
    assert_eq!(app.shell.tabs.get(0).expect("tab 0").id, survivor_tab_id);
    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_after_index
        .is_none());
    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_confirmation
        .is_none());
    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_ancestor_confirmation
        .is_none());
    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_use_walker_confirmation
        .is_none());
    assert_eq!(app.shell.indexing.request_tabs.get(&11), None);
    assert_eq!(
        app.shell.indexing.request_tabs.get(&12),
        Some(&survivor_tab_id)
    );
    assert!(app
        .shell
        .indexing
        .pending_queue
        .iter()
        .all(|req| req.tab_id != removed_tab_id));
    assert!(app.shell.indexing.background_states.contains_key(&12));
    assert!(!app.shell.indexing.background_states.contains_key(&11));
    if let Ok(latest) = app.shell.indexing.latest_request_ids.lock() {
        assert_eq!(latest.get(&removed_tab_id), None);
        assert_eq!(latest.get(&survivor_tab_id), Some(&12));
    }
    assert_eq!(app.shell.search.take_request_tab(21), None);
    assert_eq!(app.shell.search.take_request_tab(22), Some(survivor_tab_id));
    assert_eq!(app.preview_request_tab(31), None);
    assert_eq!(app.preview_request_tab(32), Some(survivor_tab_id));
    assert_eq!(app.action_request_tab(41), None);
    assert_eq!(app.action_request_tab(42), Some(survivor_tab_id));
    assert_eq!(app.sort_request_tab(51), None);
    assert_eq!(app.sort_request_tab(52), Some(survivor_tab_id));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn inactive_tab_results_are_compacted_and_restored_on_activation() {
    let root = test_root("inactive-tab-results-compaction");
    fs::create_dir_all(&root).expect("create dir");
    let first = root.join("first.txt");
    let second = root.join("second.txt");
    fs::write(&first, "a").expect("write first");
    fs::write(&second, "b").expect("write second");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.ui.show_preview = false;
    app.shell.indexing.in_progress = false;
    app.shell.indexing.pending_request_id = None;
    app.shell.runtime.entries = Arc::new(vec![
        unknown_entry(first.clone()),
        unknown_entry(second.clone()),
    ]);
    app.shell.runtime.base_results = vec![(first.clone(), 10.0), (second.clone(), 5.0)];
    app.shell.runtime.results = app.shell.runtime.base_results.clone();
    app.shell.runtime.current_row = Some(1);
    app.shell.runtime.preview = "preview".to_string();

    app.create_new_tab();

    assert_eq!(app.shell.tabs.len(), 2);
    assert!(
        app.shell
            .tabs
            .get(0)
            .expect("tab 0")
            .result_state
            .results_compacted
    );
    assert!(app
        .shell
        .tabs
        .get(0)
        .expect("tab 0")
        .result_state
        .results
        .is_empty());
    assert_eq!(
        app.shell
            .tabs
            .get(0)
            .expect("tab 0")
            .result_state
            .base_results
            .len(),
        2
    );
    assert!(app
        .shell
        .tabs
        .get(0)
        .expect("tab 0")
        .result_state
        .preview
        .is_empty());

    app.switch_to_tab_index(0);

    assert_eq!(app.shell.runtime.results.len(), 2);
    assert_eq!(app.shell.runtime.results[0].0, first);
    assert_eq!(app.shell.runtime.results[1].0, second);
    assert_eq!(app.shell.runtime.current_row, Some(1));
    assert!(
        !app.shell
            .tabs
            .get(0)
            .expect("tab 0")
            .result_state
            .results_compacted
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tab_activation_restores_visible_cursor_when_selection_is_missing() {
    let root = test_root("tab-activation-restore-cursor");
    fs::create_dir_all(&root).expect("create dir");
    let first = root.join("first.txt");
    let second = root.join("second.txt");
    fs::write(&first, "a").expect("write first");
    fs::write(&second, "b").expect("write second");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.ui.show_preview = false;
    app.shell.indexing.in_progress = false;
    app.shell.indexing.pending_request_id = None;
    app.shell.runtime.entries = Arc::new(vec![
        unknown_entry(first.clone()),
        unknown_entry(second.clone()),
    ]);
    app.shell.runtime.base_results = vec![(first.clone(), 10.0), (second.clone(), 5.0)];
    app.shell.runtime.results = app.shell.runtime.base_results.clone();
    app.shell.runtime.current_row = None;
    app.sync_active_tab_state();

    app.create_new_tab();
    app.switch_to_tab_index(0);

    assert_eq!(app.shell.runtime.current_row, Some(0));
    assert!(app.shell.ui.scroll_to_current);
    let _ = fs::remove_dir_all(&root);
}

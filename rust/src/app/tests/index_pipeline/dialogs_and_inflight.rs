use super::*;

#[test]
fn request_index_refresh_reenables_files_when_both_filters_are_off() {
    let root = test_root("request-refresh-filter-guard");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexRequest>();
    app.indexing.tx = tx;
    app.include_files = false;
    app.include_dirs = false;

    app.request_index_refresh();

    let req = rx.try_recv().expect("index request should be sent");
    assert!(req.include_files);
    assert!(!req.include_dirs);
    assert!(app.include_files);
    assert!(!app.include_dirs);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn request_index_refresh_uses_latest_toggle_state() {
    let root = test_root("request-refresh-toggle-state");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexRequest>();
    app.indexing.tx = tx;
    app.use_filelist = false;
    app.include_files = false;
    app.include_dirs = true;

    app.request_index_refresh();

    let req = rx.try_recv().expect("index request should be sent");
    assert!(!req.use_filelist);
    assert!(!req.include_files);
    assert!(req.include_dirs);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn request_create_filelist_walker_refresh_resets_index_state_and_registers_request() {
    let root = test_root("create-filelist-walker-refresh-reset");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "abc".to_string());
    let (tx, rx) = mpsc::channel::<IndexRequest>();
    app.indexing.tx = tx;
    app.indexing.pending_entries.push_back(IndexEntry {
        path: root.join("stale.txt"),
        kind: EntryKind::file(),
        kind_known: true,
    });
    app.indexing.pending_entries_request_id = Some(7);
    app.indexing.pending_kind_paths.push_back(root.join("stale-kind.txt"));
    app.indexing.pending_kind_paths_set.insert(root.join("stale-kind.txt"));
    app.indexing.in_flight_kind_paths.insert(root.join("in-flight.txt"));
    app.indexing.kind_resolution_in_progress = true;
    app.worker_bus.preview.pending_request_id = Some(9);
    app.worker_bus.preview.in_progress = true;

    let tab_id = app.current_tab_id().expect("tab id");
    app.request_create_filelist_walker_refresh();

    let req = rx.try_recv().expect("index request should be sent");
    assert_eq!(req.tab_id, tab_id);
    assert!(!req.use_filelist);
    assert!(app.indexing.inflight_requests.contains(&req.request_id));
    assert!(app.indexing.pending_entries.is_empty());
    assert_eq!(app.indexing.pending_entries_request_id, None);
    assert!(app.indexing.pending_kind_paths.is_empty());
    assert!(app.indexing.pending_kind_paths_set.is_empty());
    assert!(app.indexing.in_flight_kind_paths.is_empty());
    assert!(!app.indexing.kind_resolution_in_progress);
    assert_eq!(app.worker_bus.preview.pending_request_id, None);
    assert!(!app.worker_bus.preview.in_progress);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn files_toggle_change_requests_reindex() {
    let root = test_root("files-toggle-reindex");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexRequest>();
    app.indexing.tx = tx;
    app.use_filelist = false;
    app.include_files = false;
    app.include_dirs = true;

    app.maybe_reindex_from_filter_toggles(false, true, false);

    let req = rx.try_recv().expect("index request should be sent");
    assert!(!req.include_files);
    assert!(req.include_dirs);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn use_filelist_forces_type_filters_to_both_enabled() {
    let root = test_root("use-filelist-forces-type-filters");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexRequest>();
    app.indexing.tx = tx;
    app.use_filelist = true;
    app.include_files = false;
    app.include_dirs = true;

    app.maybe_reindex_from_filter_toggles(true, false, false);

    let req = rx.try_recv().expect("index request should be sent");
    assert!(app.include_files);
    assert!(app.include_dirs);
    assert!(req.use_filelist);
    assert!(req.include_files);
    assert!(req.include_dirs);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn use_filelist_with_walker_source_keeps_type_filters_editable() {
    let root = test_root("use-filelist-walker-keeps-type-filters");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexRequest>();
    app.indexing.tx = tx;
    app.use_filelist = true;
    app.index.source = IndexSource::Walker;
    app.include_files = false;
    app.include_dirs = true;

    app.maybe_reindex_from_filter_toggles(true, false, false);

    let req = rx.try_recv().expect("index request should be sent");
    assert!(req.use_filelist);
    assert!(!req.include_files);
    assert!(req.include_dirs);
    assert!(!app.include_files);
    assert!(app.include_dirs);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn create_filelist_with_use_filelist_enabled_and_walker_source_skips_confirmation() {
    let root = test_root("filelist-use-filelist-walker-source-no-confirm");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (filelist_tx, filelist_rx) = mpsc::channel::<FileListRequest>();
    app.worker_bus.filelist.tx = filelist_tx;
    app.use_filelist = true;
    app.index.source = IndexSource::Walker;
    app.indexing.in_progress = false;

    app.create_filelist();

    assert!(app.filelist_state.pending_use_walker_confirmation.is_none());
    let req = filelist_rx
        .try_recv()
        .expect("filelist request should be sent without confirmation");
    assert_eq!(req.root, root);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn dialog_arrow_keys_move_dialog_selection_not_results() {
    let root = test_root("dialog-arrow-focus");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.results = vec![(root.join("a.txt"), 0.0), (root.join("b.txt"), 0.0)];
    app.current_row = Some(1);
    app.filelist_state.pending_ancestor_confirmation = Some(PendingFileListAncestorConfirmation {
        tab_id: app.current_tab_id().expect("tab id"),
        root: root.clone(),
        entries: vec![root.join("a.txt")],
    });

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::ArrowRight,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );

    assert_eq!(app.current_row, Some(1));
    assert_eq!(
        app.filelist_state.active_dialog,
        Some(FileListDialogKind::Ancestor)
    );
    assert_eq!(app.filelist_state.active_dialog_button, 1);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn dialog_space_confirms_selected_dialog_action() {
    let root = test_root("dialog-space-confirm");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (filelist_tx, filelist_rx) = mpsc::channel::<FileListRequest>();
    app.worker_bus.filelist.tx = filelist_tx;
    app.filelist_state.pending_ancestor_confirmation = Some(PendingFileListAncestorConfirmation {
        tab_id: app.current_tab_id().expect("tab id"),
        root: root.clone(),
        entries: vec![root.join("a.txt")],
    });

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::ArrowRight,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::Space,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );

    let req = filelist_rx
        .try_recv()
        .expect("filelist request should be sent");
    assert!(!req.propagate_to_ancestors);
    assert!(app.filelist_state.pending_ancestor_confirmation.is_none());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn dialog_enter_confirms_without_triggering_main_window_action() {
    let root = test_root("dialog-enter-confirm");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.results = vec![(root.join("a.txt"), 0.0)];
    app.current_row = Some(0);
    app.filelist_state.pending_use_walker_confirmation =
        Some(PendingFileListUseWalkerConfirmation {
            source_tab_id: app.current_tab_id().expect("tab id"),
            root: root.clone(),
        });

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );

    assert_eq!(app.tabs.len(), 1);
    assert!(app.filelist_state.pending_use_walker_confirmation.is_none());
    assert!(app.notice.contains("Preparing background Walker index"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn filelist_use_walker_dialog_text_describes_background_execution() {
    let [line1, line2] = FlistWalkerApp::filelist_use_walker_dialog_lines();

    assert!(line1.contains("Walker indexing"));
    assert!(line2.contains("現在のタブの裏"));
    assert!(!line2.contains("新規タブ"));
}

#[test]
fn preempt_background_when_active_index_is_queued() {
    let root = test_root("index-preempt-active-priority");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.create_new_tab();

    let active_tab_id = app.tabs[2].id;
    let bg_tab_a = app.tabs[0].id;
    let bg_tab_b = app.tabs[1].id;
    app.active_tab = 2;

    app.indexing.inflight_requests.insert(100);
    app.indexing.inflight_requests.insert(101);
    app.indexing.request_tabs.insert(100, bg_tab_a);
    app.indexing.request_tabs.insert(101, bg_tab_b);
    app.indexing.pending_queue.push_back(IndexRequest {
        request_id: 102,
        tab_id: active_tab_id,
        root: root.clone(),
        use_filelist: false,
        include_files: true,
        include_dirs: true,
    });
    {
        let mut latest = app.indexing.latest_request_ids.lock().expect("lock latest");
        latest.insert(bg_tab_a, 100);
        latest.insert(bg_tab_b, 101);
    }

    assert!(app.preempt_background_for_active_request());

    let latest = app.indexing.latest_request_ids.lock().expect("lock latest");
    let preempted =
        latest.get(&bg_tab_a).copied() == Some(0) || latest.get(&bg_tab_b).copied() == Some(0);
    assert!(preempted);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn stale_terminal_index_response_clears_inflight_slot() {
    let root = test_root("stale-terminal-clears-inflight");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.indexing.rx = rx;
    let stale_request_id = 777u64;
    let current_tab_id = app.current_tab_id().expect("tab id");
    app.indexing.pending_request_id = Some(778);
    app.indexing.inflight_requests.insert(stale_request_id);
    app.indexing
        .request_tabs
        .insert(stale_request_id, current_tab_id);

    tx.send(IndexResponse::Finished {
        request_id: stale_request_id,
        source: IndexSource::Walker,
    })
    .expect("send finished");

    app.poll_index_response();

    assert!(!app.indexing.inflight_requests.contains(&stale_request_id));
    assert!(!app.indexing.request_tabs.contains_key(&stale_request_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn current_finished_index_response_clears_inflight_slot() {
    let root = test_root("current-finished-clears-inflight");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.indexing.rx = rx;
    let req_id = app.indexing.pending_request_id.expect("pending request");
    let tab_id = app.current_tab_id().expect("tab id");
    app.indexing.request_tabs.insert(req_id, tab_id);
    app.indexing.inflight_requests.insert(req_id);

    tx.send(IndexResponse::Finished {
        request_id: req_id,
        source: IndexSource::Walker,
    })
    .expect("send finished");

    app.poll_index_response();

    assert!(!app.indexing.inflight_requests.contains(&req_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn same_tab_request_waits_until_previous_inflight_finishes() {
    let root = test_root("same-tab-inflight-serialization");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let tab_id = app.current_tab_id().expect("tab id");

    app.indexing.inflight_requests.insert(1);
    app.indexing.request_tabs.insert(1, tab_id);
    app.indexing.pending_queue.push_back(IndexRequest {
        request_id: 2,
        tab_id,
        root: root.clone(),
        use_filelist: false,
        include_files: true,
        include_dirs: true,
    });

    assert!(app.pop_next_index_request().is_none());

    app.indexing.inflight_requests.remove(&1);
    let popped = app
        .pop_next_index_request()
        .expect("queued same-tab request should run");
    assert_eq!(popped.request_id, 2);
    let _ = fs::remove_dir_all(&root);
}

use super::*;

#[test]
fn root_change_clears_stale_selection_state() {
    let root_old = test_root("root-change-clear-selection-old");
    let root_new = test_root("root-change-clear-selection-new");
    fs::create_dir_all(&root_old).expect("create old dir");
    fs::create_dir_all(&root_new).expect("create new dir");
    let old_path = root_old.join("old.txt");
    fs::write(&old_path, "x").expect("write old file");

    let mut app = FlistWalkerApp::new(root_old.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexRequest>();
    app.shell.indexing.tx = tx;
    app.shell.runtime.pinned_paths.insert(old_path);
    app.shell.runtime.current_row = Some(0);
    app.shell.runtime.preview = "stale preview".to_string();
    app.shell.runtime.results = vec![(root_old.join("result.txt"), 0.0)];

    app.apply_root_change(root_new.clone());

    assert!(app.shell.runtime.pinned_paths.is_empty());
    assert_eq!(app.shell.runtime.current_row, None);
    assert!(app.shell.runtime.preview.is_empty());
    assert!(app.shell.runtime.all_entries.is_empty());
    assert!(app.shell.runtime.entries.is_empty());
    assert!(app.shell.runtime.results.is_empty());
    let active_tab = app.shell.tabs.active_tab;
    assert_eq!(app.shell.tabs.get(active_tab).expect("tab").root, root_new);
    assert!(app
        .shell
        .tabs
        .get(active_tab)
        .expect("tab")
        .index_state
        .all_entries
        .is_empty());
    assert!(app
        .shell
        .tabs
        .get(active_tab)
        .expect("tab")
        .index_state
        .entries
        .is_empty());
    let req = rx.try_recv().expect("index request should be sent");
    assert_eq!(req.root, app.shell.runtime.root);
    let _ = fs::remove_dir_all(&root_old);
    let _ = fs::remove_dir_all(&root_new);
}

#[test]
fn root_change_cancels_pending_filelist_overwrite_confirmation() {
    let root_old = test_root("root-change-cancel-overwrite-old");
    let root_new = test_root("root-change-cancel-overwrite-new");
    fs::create_dir_all(&root_old).expect("create old dir");
    fs::create_dir_all(&root_new).expect("create new dir");

    let mut app = FlistWalkerApp::new(root_old.clone(), 50, String::new());
    let (tx, _rx) = mpsc::channel::<IndexRequest>();
    app.shell.indexing.tx = tx;
    let tab_id = app.current_tab_id().expect("tab id");
    app.shell.features.filelist.workflow.pending_confirmation = Some(PendingFileListConfirmation {
        tab_id,
        root: root_old.clone(),
        entries: vec![root_old.join("a.txt")],
        existing_path: root_old.join("FileList.txt"),
    });

    app.apply_root_change(root_new.clone());

    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_confirmation
        .is_none());
    let _ = fs::remove_dir_all(&root_old);
    let _ = fs::remove_dir_all(&root_new);
}

#[test]
fn root_change_cancels_pending_filelist_ancestor_confirmation() {
    let root_old = test_root("root-change-cancel-ancestor-old");
    let root_new = test_root("root-change-cancel-ancestor-new");
    fs::create_dir_all(&root_old).expect("create old dir");
    fs::create_dir_all(&root_new).expect("create new dir");

    let mut app = FlistWalkerApp::new(root_old.clone(), 50, String::new());
    let (tx, _rx) = mpsc::channel::<IndexRequest>();
    app.shell.indexing.tx = tx;
    let tab_id = app.current_tab_id().expect("tab id");
    app.shell
        .features
        .filelist
        .workflow
        .pending_ancestor_confirmation = Some(PendingFileListAncestorConfirmation {
        tab_id,
        root: root_old.clone(),
        entries: vec![root_old.join("a.txt")],
    });

    app.apply_root_change(root_new.clone());

    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_ancestor_confirmation
        .is_none());
    assert!(app.shell.runtime.notice.contains("Root changed"));
    let _ = fs::remove_dir_all(&root_old);
    let _ = fs::remove_dir_all(&root_new);
}

#[test]
fn root_change_cancels_pending_filelist_use_walker_confirmation() {
    let root_old = test_root("root-change-cancel-use-walker-old");
    let root_new = test_root("root-change-cancel-use-walker-new");
    fs::create_dir_all(&root_old).expect("create old dir");
    fs::create_dir_all(&root_new).expect("create new dir");

    let mut app = FlistWalkerApp::new(root_old.clone(), 50, String::new());
    let (tx, _rx) = mpsc::channel::<IndexRequest>();
    app.shell.indexing.tx = tx;
    let tab_id = app.current_tab_id().expect("tab id");
    app.shell
        .features
        .filelist
        .workflow
        .pending_use_walker_confirmation = Some(PendingFileListUseWalkerConfirmation {
        source_tab_id: tab_id,
        root: root_old.clone(),
    });

    app.apply_root_change(root_new.clone());

    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_use_walker_confirmation
        .is_none());
    assert!(app.shell.runtime.notice.contains("Root changed"));
    let _ = fs::remove_dir_all(&root_old);
    let _ = fs::remove_dir_all(&root_new);
}

#[test]
fn create_filelist_requests_confirmation_before_ancestor_propagation() {
    let top = test_root("filelist-ancestor-confirm");
    let root = top.join("child");
    fs::create_dir_all(&root).expect("create child");
    fs::write(top.join("FileList.txt"), "child/old.txt\n").expect("write parent filelist");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    reset_index_request_state_for_test(&mut app);
    app.shell.runtime.use_filelist = false;
    app.shell.runtime.index.source = IndexSource::Walker;
    app.shell.runtime.include_files = true;
    app.shell.runtime.include_dirs = true;
    app.shell.runtime.all_entries = Arc::new(vec![unknown_entry(root.join("main.rs"))]);
    app.shell.runtime.entries = Arc::clone(&app.shell.runtime.all_entries);

    app.create_filelist();

    assert!(
        app.shell.runtime.notice.contains("ancestor")
            || app.shell.runtime.notice.contains("parent"),
        "notice should mention ancestor confirmation, got: {}",
        app.shell.runtime.notice
    );
    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_ancestor_confirmation
        .is_some());
    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_request_id
        .is_none());
    assert!(!app.shell.features.filelist.workflow.in_progress);
    let _ = fs::remove_dir_all(&top);
}

#[test]
fn denying_ancestor_propagation_still_creates_root_filelist() {
    let top = test_root("filelist-ancestor-deny");
    let root = top.join("child");
    fs::create_dir_all(&root).expect("create child");
    let parent_filelist = top.join("FileList.txt");
    fs::write(&parent_filelist, "child/old.txt\n").expect("write parent filelist");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    reset_index_request_state_for_test(&mut app);
    app.shell.runtime.use_filelist = false;
    app.shell.runtime.index.source = IndexSource::Walker;
    app.shell.runtime.include_files = true;
    app.shell.runtime.include_dirs = true;
    app.shell.runtime.all_entries = Arc::new(vec![unknown_entry(root.join("main.rs"))]);
    app.shell.runtime.entries = Arc::clone(&app.shell.runtime.all_entries);
    let (filelist_tx, filelist_rx) = mpsc::channel::<FileListRequest>();
    app.shell.worker_bus.filelist.tx = filelist_tx;

    app.create_filelist();
    app.skip_pending_filelist_ancestor_propagation();

    let req = filelist_rx
        .try_recv()
        .expect("root filelist creation should proceed without ancestor propagation");
    assert_eq!(req.root, root);
    assert!(!req.propagate_to_ancestors);
    let parent_content = fs::read_to_string(&parent_filelist).expect("read parent filelist");
    assert_eq!(parent_content, "child/old.txt\n");
    let _ = fs::remove_dir_all(&top);
}

#[test]
fn create_filelist_skips_ancestor_confirmation_when_child_reference_is_already_present() {
    let top = test_root("filelist-ancestor-skip-confirm");
    let root = top.join("child");
    fs::create_dir_all(&root).expect("create child");
    fs::write(
        top.join("FileList.txt"),
        format!("./child{}FileList.txt\n", std::path::MAIN_SEPARATOR),
    )
    .expect("write parent filelist");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    reset_index_request_state_for_test(&mut app);
    app.shell.runtime.use_filelist = false;
    app.shell.runtime.index.source = IndexSource::Walker;
    app.shell.runtime.include_files = true;
    app.shell.runtime.include_dirs = true;
    app.shell.runtime.all_entries = Arc::new(vec![unknown_entry(root.join("main.rs"))]);
    app.shell.runtime.entries = Arc::clone(&app.shell.runtime.all_entries);
    let (filelist_tx, filelist_rx) = mpsc::channel::<FileListRequest>();
    app.shell.worker_bus.filelist.tx = filelist_tx;

    app.create_filelist();

    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_ancestor_confirmation
        .is_none());
    let req = filelist_rx
        .try_recv()
        .expect("filelist request should be sent without ancestor prompt");
    assert_eq!(req.root, root);
    assert!(!req.propagate_to_ancestors);
    let _ = fs::remove_dir_all(&top);
}

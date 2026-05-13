use super::*;

#[test]
fn create_filelist_waits_while_indexing() {
    let root = test_root("filelist-waits-indexing");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
    app.shell.indexing.tx = index_tx;
    app.shell.runtime.use_filelist = false;
    app.shell.runtime.index.source = IndexSource::Walker;
    app.shell.runtime.include_files = true;
    app.shell.runtime.include_dirs = true;
    app.shell.indexing.in_progress = true;

    app.create_filelist();

    assert_eq!(
        app.shell
            .features
            .filelist
            .workflow
            .pending_after_index
            .as_ref()
            .map(|pending| pending.root.clone()),
        Some(root.clone())
    );
    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_request_id
        .is_none());
    assert!(!app.shell.features.filelist.workflow.in_progress);
    assert!(index_rx.try_recv().is_err());
    assert!(app
        .shell
        .runtime
        .notice
        .contains("Waiting for current indexing"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn create_filelist_while_indexing_with_filter_change_requests_reindex() {
    let root = test_root("filelist-waits-indexing-needs-reindex");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
    app.shell.indexing.tx = index_tx;
    app.shell.runtime.use_filelist = false;
    app.shell.runtime.index.source = IndexSource::Walker;
    app.shell.runtime.include_files = false;
    app.shell.runtime.include_dirs = true;
    app.shell.indexing.in_progress = true;

    app.create_filelist();

    let req = index_rx.try_recv().expect("reindex request should be sent");
    assert_eq!(req.root, root);
    assert!(req.include_files);
    assert!(req.include_dirs);
    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_after_index
        .is_some());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn create_filelist_forces_files_and_dirs_before_reindex() {
    let root = test_root("filelist-force-files-dirs");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
    app.shell.indexing.tx = index_tx;
    app.shell.runtime.use_filelist = false;
    app.shell.runtime.include_files = false;
    app.shell.runtime.include_dirs = true;
    app.shell.runtime.index.source = IndexSource::Walker;

    app.create_filelist();

    assert!(app.shell.runtime.include_files);
    assert!(app.shell.runtime.include_dirs);
    let req = index_rx.try_recv().expect("reindex request should be sent");
    assert_eq!(req.root, root);
    assert!(!req.use_filelist);
    assert!(req.include_files);
    assert!(req.include_dirs);
    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_after_index
        .is_some());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn create_filelist_with_use_filelist_enabled_confirms_and_prepares_background_walker() {
    let root = test_root("filelist-use-filelist-confirm");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
    app.shell.indexing.tx = index_tx;

    assert!(app.shell.runtime.use_filelist);
    app.create_filelist();
    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_use_walker_confirmation
        .is_some());
    assert_eq!(app.shell.tabs.len(), 1);

    app.confirm_pending_filelist_use_walker();

    assert_eq!(app.shell.tabs.len(), 1);
    assert_eq!(app.shell.tabs.active_tab, 0);
    assert!(app.shell.runtime.use_filelist);
    assert!(app.shell.runtime.include_files);
    assert!(app.shell.runtime.include_dirs);
    let pending = app
        .shell
        .features
        .filelist
        .workflow
        .pending_after_index
        .as_ref()
        .expect("deferred filelist pending");
    let current_tab_id = app.current_tab_id().expect("current tab id");
    assert_eq!(pending.tab_id, current_tab_id);
    assert_eq!(pending.root, root);
    let req = index_rx
        .try_recv()
        .expect("walker index request should be sent");
    assert_eq!(req.tab_id, current_tab_id);
    assert_eq!(req.root, root);
    assert!(!req.use_filelist);
    assert!(app
        .shell
        .runtime
        .notice
        .contains("Preparing background Walker index"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn filelist_finished_updates_state_and_notice() {
    let root = test_root("filelist-finished");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<FileListResponse>();
    app.shell.worker_bus.filelist.rx = rx;
    app.shell.features.filelist.workflow.pending_request_id = Some(11);
    app.shell.features.filelist.workflow.pending_request_tab_id = app.current_tab_id();
    app.shell.features.filelist.workflow.pending_root = Some(root.clone());
    app.shell.features.filelist.workflow.in_progress = true;
    app.shell.runtime.use_filelist = false;

    let filelist = root.join("FileList.txt");
    tx.send(FileListResponse::Finished {
        request_id: 11,
        root: root.clone(),
        path: filelist.clone(),
        count: 3,
    })
    .expect("send filelist response");

    app.poll_filelist_response();

    assert_eq!(
        app.shell.features.filelist.workflow.pending_request_id,
        None
    );
    assert_eq!(
        app.shell.features.filelist.workflow.pending_request_tab_id,
        None
    );
    assert!(!app.shell.features.filelist.workflow.in_progress);
    assert!(app.shell.runtime.use_filelist);
    assert!(app.shell.runtime.notice.contains("Created"));
    assert!(app.shell.runtime.notice.contains("3 entries"));
    assert!(app
        .shell
        .runtime
        .notice
        .contains(filelist.to_string_lossy().as_ref()));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn filelist_finished_enables_use_filelist_for_creator_tab() {
    let root = test_root("filelist-finished-enable-creator-tab");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.shell.runtime.use_filelist = false;
    app.sync_active_tab_state();
    let creator_tab_id = app.shell.tabs.get(0).expect("tab 0").id;
    let (tx, rx) = mpsc::channel::<FileListResponse>();
    app.shell.worker_bus.filelist.rx = rx;
    app.shell.features.filelist.workflow.pending_request_id = Some(101);
    app.shell.features.filelist.workflow.pending_request_tab_id = Some(creator_tab_id);
    app.shell.features.filelist.workflow.pending_root = Some(root.clone());
    app.shell.features.filelist.workflow.in_progress = true;

    tx.send(FileListResponse::Finished {
        request_id: 101,
        root: root.clone(),
        path: root.join("FileList.txt"),
        count: 2,
    })
    .expect("send filelist response");

    app.poll_filelist_response();

    let creator_tab = app
        .shell
        .tabs
        .iter()
        .find(|tab| tab.id == creator_tab_id)
        .expect("creator tab");
    assert!(creator_tab.use_filelist);
    assert!(!app.shell.runtime.use_filelist);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn create_filelist_requests_overwrite_confirmation_when_file_exists() {
    let root = test_root("filelist-overwrite-confirm");
    fs::create_dir_all(&root).expect("create dir");
    fs::write(root.join("FileList.txt"), "old\n").expect("write filelist");
    let path = root.join("main.rs");
    fs::write(&path, "fn main() {}").expect("write file");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.indexing.in_progress = false;
    app.shell.runtime.use_filelist = false;
    app.shell.runtime.all_entries = Arc::new(vec![file_entry(path.clone())]);
    app.set_entry_kind(&path, EntryKind::file());
    app.shell.runtime.index.source = IndexSource::Walker;

    app.create_filelist();

    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_confirmation
        .is_some());
    assert!(!app.shell.features.filelist.workflow.in_progress);
    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_request_id
        .is_none());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn confirm_pending_overwrite_starts_filelist_creation() {
    let root = test_root("filelist-overwrite-confirm-start");
    fs::create_dir_all(&root).expect("create dir");
    let file_path = root.join("FileList.txt");
    let entries = vec![root.join("src/main.rs")];
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (filelist_tx, filelist_rx) = mpsc::channel::<FileListRequest>();
    app.shell.worker_bus.filelist.tx = filelist_tx;
    let tab_id = app.current_tab_id().expect("tab id");
    app.shell.features.filelist.workflow.pending_confirmation = Some(PendingFileListConfirmation {
        tab_id,
        root: root.clone(),
        entries: entries.clone(),
        existing_path: file_path,
    });

    app.confirm_pending_filelist_overwrite();

    let req = filelist_rx
        .try_recv()
        .expect("filelist request should be sent");
    assert_eq!(req.tab_id, tab_id);
    assert_eq!(req.root, root);
    assert_eq!(req.entries, entries);
    assert!(app.shell.features.filelist.workflow.in_progress);
    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_confirmation
        .is_none());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn cancel_create_filelist_clears_pending_after_index() {
    let root = test_root("filelist-cancel-pending-after-index");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.features.filelist.workflow.pending_after_index = Some(PendingFileListAfterIndex {
        tab_id: app.current_tab_id().expect("tab id"),
        root: root.clone(),
    });

    app.cancel_create_filelist();

    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_after_index
        .is_none());
    assert!(app
        .shell
        .runtime
        .notice
        .contains("Create File List canceled"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn filelist_finished_triggers_reindex_when_enabled() {
    let root = test_root("filelist-reindex");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (filelist_tx, filelist_rx) = mpsc::channel::<FileListResponse>();
    app.shell.worker_bus.filelist.rx = filelist_rx;
    let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
    app.shell.indexing.tx = index_tx;
    app.shell.features.filelist.workflow.pending_request_id = Some(12);
    app.shell.features.filelist.workflow.pending_request_tab_id = app.current_tab_id();
    app.shell.features.filelist.workflow.pending_root = Some(root.clone());
    app.shell.features.filelist.workflow.in_progress = true;
    app.shell.runtime.use_filelist = false;

    filelist_tx
        .send(FileListResponse::Finished {
            request_id: 12,
            root: root.clone(),
            path: root.join("FileList.txt"),
            count: 5,
        })
        .expect("send filelist response");

    app.poll_filelist_response();

    let req = index_rx.try_recv().expect("reindex request should be sent");
    assert_eq!(req.root, root);
    assert!(req.use_filelist);
    assert!(app.shell.indexing.in_progress);
    assert!(app.shell.indexing.pending_request_id.is_some());
    let _ = fs::remove_dir_all(&root);
}

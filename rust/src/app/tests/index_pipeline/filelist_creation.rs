use super::*;

#[test]
fn create_filelist_waits_while_indexing() {
    let root = test_root("filelist-waits-indexing");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (index_tx, index_rx) = bounded_request_channel::<IndexRequest>(2);
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
    let (index_tx, index_rx) = bounded_request_channel::<IndexRequest>(2);
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
    let (index_tx, index_rx) = bounded_request_channel::<IndexRequest>(2);
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
    let (index_tx, index_rx) = bounded_request_channel::<IndexRequest>(2);
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
    let (index_tx, index_rx) = bounded_request_channel::<IndexRequest>(2);
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

#[test]
fn regression_filelist_completion_notice_survives_reindex_settlement() {
    let root = test_root("filelist-completion-notice-after-reindex");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (filelist_tx, filelist_rx) = mpsc::channel::<FileListResponse>();
    app.shell.worker_bus.filelist.rx = filelist_rx;
    let (index_tx, index_request_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = index_tx;
    let (index_response_tx, index_response_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = index_response_rx;
    app.shell.features.filelist.workflow.pending_request_id = Some(13);
    app.shell.features.filelist.workflow.pending_request_tab_id = app.current_tab_id();
    app.shell.features.filelist.workflow.pending_root = Some(root.clone());
    app.shell.features.filelist.workflow.in_progress = true;
    app.shell.runtime.use_filelist = false;

    let filelist = root.join("FileList.txt");
    filelist_tx
        .send(FileListResponse::Finished {
            request_id: 13,
            root: root.clone(),
            path: filelist.clone(),
            count: 5,
        })
        .expect("send filelist response");

    app.poll_filelist_response();

    let index_request = index_request_rx
        .try_recv()
        .expect("reindex request should be sent");
    index_response_tx
        .send(IndexResponse::Finished {
            request_id: index_request.request_id,
            source: IndexSource::FileList(filelist.clone()),
        })
        .expect("send index response");

    app.poll_index_response();

    assert!(app.shell.runtime.notice.contains("Created"));
    assert!(app.shell.runtime.notice.contains("5 entries"));
    assert!(app
        .shell
        .runtime
        .notice
        .contains(filelist.to_string_lossy().as_ref()));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn regression_filelist_completion_notice_is_not_cleared_by_another_tab_refresh() {
    let root = test_root("filelist-completion-notice-cross-tab-refresh");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let first_tab_id = app.current_tab_id().expect("first tab id");
    app.create_new_tab();
    let (index_tx, _index_request_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = index_tx;
    app.shell
        .features
        .filelist
        .workflow
        .pending_index_completion_notices
        .insert(
            77,
            PendingFileListIndexCompletionNotice {
                tab_id: first_tab_id,
                root: root.clone(),
                notice: "Created FileList.txt".to_string(),
            },
        );

    app.request_index_refresh();

    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_index_completion_notices
        .contains_key(&77));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn regression_filelist_completion_notice_is_cleared_by_same_tab_supersede() {
    let root = test_root("filelist-completion-notice-same-tab-supersede");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (index_tx, _index_request_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = index_tx;
    let tab_id = app.current_tab_id().expect("tab id");
    app.shell
        .features
        .filelist
        .workflow
        .pending_index_completion_notices
        .insert(
            78,
            PendingFileListIndexCompletionNotice {
                tab_id,
                root: root.clone(),
                notice: "Created FileList.txt".to_string(),
            },
        );

    app.request_create_filelist_walker_refresh();

    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_index_completion_notices
        .is_empty());
    let _ = fs::remove_dir_all(&root);
}

fn seed_filelist_completion_notice(
    app: &mut FlistWalkerApp,
    request_id: u64,
    tab_id: u64,
    root: &Path,
) {
    app.shell
        .features
        .filelist
        .workflow
        .pending_index_completion_notices
        .insert(
            request_id,
            PendingFileListIndexCompletionNotice {
                tab_id,
                root: root.to_path_buf(),
                notice: "Created FileList.txt: 5 entries".to_string(),
            },
        );
}

#[test]
fn filelist_completion_notice_is_restored_after_background_reindex_finishes() {
    let root = test_root("filelist-completion-notice-background-finish");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let creator_tab_id = app.current_tab_id().expect("creator tab id");
    let (filelist_tx, filelist_rx) = mpsc::channel::<FileListResponse>();
    app.shell.worker_bus.filelist.rx = filelist_rx;
    let (index_tx, index_request_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = index_tx;
    let (index_response_tx, index_response_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = index_response_rx;
    app.shell.features.filelist.workflow.pending_request_id = Some(14);
    app.shell.features.filelist.workflow.pending_request_tab_id = Some(creator_tab_id);
    app.shell.features.filelist.workflow.pending_root = Some(root.clone());
    app.shell.features.filelist.workflow.in_progress = true;
    app.shell.runtime.use_filelist = false;
    let filelist = root.join("FileList.txt");

    filelist_tx
        .send(FileListResponse::Finished {
            request_id: 14,
            root: root.clone(),
            path: filelist.clone(),
            count: 5,
        })
        .expect("send filelist response");
    app.poll_filelist_response();
    let index_request = index_request_rx
        .try_recv()
        .expect("reindex request should be sent");
    app.create_new_tab();

    index_response_tx
        .send(IndexResponse::Finished {
            request_id: index_request.request_id,
            source: IndexSource::FileList(filelist),
        })
        .expect("send index response");
    app.poll_index_response();

    let creator_tab = app
        .shell
        .tabs
        .iter()
        .find(|tab| tab.id == creator_tab_id)
        .expect("creator tab");
    assert!(creator_tab.notice.contains("Created"));
    assert!(creator_tab.notice.contains("5 entries"));
    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_index_completion_notices
        .is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn filelist_completion_notice_follows_creator_when_tab_switch_precedes_response() {
    let root = test_root("filelist-completion-notice-switch-before-response");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let creator_tab_id = app.current_tab_id().expect("creator tab id");
    let (filelist_tx, filelist_rx) = mpsc::channel::<FileListResponse>();
    app.shell.worker_bus.filelist.rx = filelist_rx;
    let (index_tx, index_request_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = index_tx;
    let (index_response_tx, index_response_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = index_response_rx;
    app.shell.features.filelist.workflow.pending_request_id = Some(15);
    app.shell.features.filelist.workflow.pending_request_tab_id = Some(creator_tab_id);
    app.shell.features.filelist.workflow.pending_root = Some(root.clone());
    app.shell.features.filelist.workflow.in_progress = true;
    app.shell.runtime.use_filelist = false;
    let filelist = root.join("FileList.txt");
    app.create_new_tab();

    filelist_tx
        .send(FileListResponse::Finished {
            request_id: 15,
            root: root.clone(),
            path: filelist.clone(),
            count: 5,
        })
        .expect("send filelist response");
    app.poll_filelist_response();
    let index_request = index_request_rx
        .try_recv()
        .expect("background reindex request should be sent");
    assert_eq!(index_request.tab_id, creator_tab_id);

    index_response_tx
        .send(IndexResponse::Finished {
            request_id: index_request.request_id,
            source: IndexSource::FileList(filelist),
        })
        .expect("send index response");
    app.poll_index_response();

    let creator_tab = app
        .shell
        .tabs
        .iter()
        .find(|tab| tab.id == creator_tab_id)
        .expect("creator tab");
    assert!(creator_tab.notice.contains("Created"));
    assert!(creator_tab.notice.contains("5 entries"));
    assert!(!app.shell.runtime.notice.contains("Created"));
    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_index_completion_notices
        .is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn filelist_completion_notice_is_discarded_when_reindex_fails() {
    let root = test_root("filelist-completion-notice-index-failure");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let tab_id = app.current_tab_id().expect("tab id");
    let request_id = 91;
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = rx;
    app.shell.indexing.pending_request_id = Some(request_id);
    app.shell.indexing.request_tabs.insert(request_id, tab_id);
    seed_filelist_completion_notice(&mut app, request_id, tab_id, &root);

    tx.send(IndexResponse::Failed {
        request_id,
        error: "fixture failure".to_string(),
    })
    .expect("send failure");
    app.poll_index_response();

    assert!(app.shell.runtime.notice.contains("Indexing failed"));
    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_index_completion_notices
        .is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn filelist_completion_notice_is_restored_when_reindex_is_canceled() {
    let root = test_root("filelist-completion-notice-index-canceled");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let tab_id = app.current_tab_id().expect("tab id");
    let request_id = 92;
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = rx;
    app.shell.indexing.pending_request_id = Some(request_id);
    app.shell.indexing.request_tabs.insert(request_id, tab_id);
    seed_filelist_completion_notice(&mut app, request_id, tab_id, &root);

    tx.send(IndexResponse::Canceled { request_id })
        .expect("send canceled");
    app.poll_index_response();

    assert!(app.shell.runtime.notice.contains("Created"));
    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_index_completion_notices
        .is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn mismatched_index_response_does_not_consume_filelist_completion_notice() {
    let root = test_root("filelist-completion-notice-request-mismatch");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let tab_id = app.current_tab_id().expect("tab id");
    let pending_notice_request_id = 93;
    let active_request_id = 94;
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = rx;
    app.shell.indexing.pending_request_id = Some(active_request_id);
    app.shell
        .indexing
        .request_tabs
        .insert(active_request_id, tab_id);
    seed_filelist_completion_notice(&mut app, pending_notice_request_id, tab_id, &root);

    tx.send(IndexResponse::Canceled {
        request_id: active_request_id,
    })
    .expect("send canceled");
    app.poll_index_response();

    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_index_completion_notices
        .contains_key(&pending_notice_request_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn root_change_discards_filelist_completion_notice_for_same_tab() {
    let root = test_root("filelist-completion-notice-root-change-old");
    let new_root = test_root("filelist-completion-notice-root-change-new");
    fs::create_dir_all(&root).expect("create old root");
    fs::create_dir_all(&new_root).expect("create new root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let tab_id = app.current_tab_id().expect("tab id");
    let (index_tx, _index_request_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = index_tx;
    seed_filelist_completion_notice(&mut app, 95, tab_id, &root);

    app.apply_root_change_direct(new_root.clone());

    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_index_completion_notices
        .is_empty());
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&new_root);
}

#[test]
fn closing_tab_discards_its_filelist_completion_notice() {
    let root = test_root("filelist-completion-notice-close-tab");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let closing_tab_id = app.current_tab_id().expect("closing tab id");
    app.create_new_tab();
    seed_filelist_completion_notice(&mut app, 96, closing_tab_id, &root);

    app.close_tab_index(0);

    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_index_completion_notices
        .is_empty());
    let _ = fs::remove_dir_all(&root);
}

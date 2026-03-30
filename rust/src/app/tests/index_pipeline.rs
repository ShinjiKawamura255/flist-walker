use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[test]
fn search_error_updates_notice() {
    let root = test_root("search-error-notice");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<SearchResponse>();
    app.search_rx = rx;
    app.pending_request_id = Some(7);
    app.search_in_progress = true;

    tx.send(SearchResponse {
        request_id: 7,
        results: Vec::new(),
        error: Some("invalid regex '[*': syntax error".to_string()),
    })
    .expect("send search response");

    app.poll_search_response();

    assert!(!app.search_in_progress);
    assert!(app.notice.contains("Search failed:"));
    assert!(app.notice.contains("invalid regex"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn stale_search_response_is_ignored_after_index_refresh() {
    let root = test_root("stale-search-after-refresh");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "abc".to_string());
    let (search_tx, search_rx) = mpsc::channel::<SearchResponse>();
    let (index_tx, _index_rx) = mpsc::channel::<IndexRequest>();
    app.search_rx = search_rx;
    app.index_tx = index_tx;
    app.pending_request_id = Some(5);
    app.search_in_progress = true;
    app.results = vec![(root.join("before.txt"), 0.0)];

    app.request_index_refresh();

    search_tx
        .send(SearchResponse {
            request_id: 5,
            results: vec![(root.join("stale.txt"), 1.0)],
            error: None,
        })
        .expect("send stale search response");

    app.poll_search_response();

    assert!(!app.search_in_progress);
    assert_eq!(app.pending_request_id, None);
    assert_eq!(app.results[0].0, root.join("before.txt"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn index_refresh_marks_search_resume_pending_for_non_empty_query() {
    let root = test_root("resume-pending-on-refresh");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "abc".to_string());
    let (index_tx, _index_rx) = mpsc::channel::<IndexRequest>();
    app.index_tx = index_tx;

    app.request_index_refresh();

    assert!(app.search_resume_pending);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn non_empty_query_resumes_search_immediately_on_first_index_batch() {
    let root = test_root("resume-first-batch");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "main".to_string());
    let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
    app.index_tx = index_tx;
    // Use a manual search channel so the test can inspect enqueued requests.
    let (search_tx_real, search_rx_real) = mpsc::channel::<SearchRequest>();
    app.search_tx = search_tx_real;

    app.request_index_refresh();
    let req = index_rx.try_recv().expect("index request should be sent");

    let (tx_idx, rx_idx) = mpsc::channel::<IndexResponse>();
    app.index_rx = rx_idx;
    tx_idx
        .send(IndexResponse::Batch {
            request_id: req.request_id,
            entries: vec![IndexEntry {
                path: root.join("main.rs"),
                kind: EntryKind::file(),
                kind_known: true,
            }],
        })
        .expect("send batch");

    // Simulate that normal throttle window has not elapsed yet.
    app.last_incremental_results_refresh = Instant::now();
    app.poll_index_response();

    let search_req = search_rx_real
        .try_recv()
        .expect("search should resume immediately");
    assert_eq!(search_req.query, "main");
    assert_eq!(search_req.entries.len(), 1);
    assert_eq!(search_req.entries[0], root.join("main.rs"));
    assert!(!app.search_resume_pending);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn filtered_out_batch_still_resumes_non_empty_query_search() {
    let root = test_root("resume-first-batch-filtered-out");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "main".to_string());
    app.include_files = false;
    app.include_dirs = true;
    let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
    app.index_tx = index_tx;
    let (search_tx_real, search_rx_real) = mpsc::channel::<SearchRequest>();
    app.search_tx = search_tx_real;

    app.request_index_refresh();
    let req = index_rx.try_recv().expect("index request should be sent");

    let (tx_idx, rx_idx) = mpsc::channel::<IndexResponse>();
    app.index_rx = rx_idx;
    tx_idx
        .send(IndexResponse::Batch {
            request_id: req.request_id,
            entries: vec![IndexEntry {
                path: root.join("main.rs"),
                kind: EntryKind::file(),
                kind_known: true,
            }],
        })
        .expect("send batch");
    app.last_incremental_results_refresh = Instant::now();
    app.poll_index_response();

    let search_req = search_rx_real
        .try_recv()
        .expect("search should still resume even when batch is filtered out");
    assert!(search_req.entries.is_empty());
    assert_eq!(search_req.query, "main");
    assert!(!app.search_resume_pending);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn create_filelist_waits_while_indexing() {
    let root = test_root("filelist-waits-indexing");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
    app.index_tx = index_tx;
    app.use_filelist = false;
    app.index.source = IndexSource::Walker;
    app.include_files = true;
    app.include_dirs = true;
    app.index_in_progress = true;

    app.create_filelist();

    assert_eq!(
        app.filelist_state.pending_after_index
            .as_ref()
            .map(|pending| pending.root.clone()),
        Some(root.clone())
    );
    assert!(app.filelist_state.pending_request_id.is_none());
    assert!(!app.filelist_state.in_progress);
    assert!(index_rx.try_recv().is_err());
    assert!(app.notice.contains("Waiting for current indexing"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn create_filelist_while_indexing_with_filter_change_requests_reindex() {
    let root = test_root("filelist-waits-indexing-needs-reindex");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
    app.index_tx = index_tx;
    app.use_filelist = false;
    app.index.source = IndexSource::Walker;
    app.include_files = false;
    app.include_dirs = true;
    app.index_in_progress = true;

    app.create_filelist();

    let req = index_rx.try_recv().expect("reindex request should be sent");
    assert_eq!(req.root, root);
    assert!(req.include_files);
    assert!(req.include_dirs);
    assert!(app.filelist_state.pending_after_index.is_some());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn create_filelist_forces_files_and_dirs_before_reindex() {
    let root = test_root("filelist-force-files-dirs");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
    app.index_tx = index_tx;
    app.use_filelist = false;
    app.include_files = false;
    app.include_dirs = true;
    app.index.source = IndexSource::Walker;

    app.create_filelist();

    assert!(app.include_files);
    assert!(app.include_dirs);
    let req = index_rx.try_recv().expect("reindex request should be sent");
    assert_eq!(req.root, root);
    assert!(!req.use_filelist);
    assert!(req.include_files);
    assert!(req.include_dirs);
    assert!(app.filelist_state.pending_after_index.is_some());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn create_filelist_with_use_filelist_enabled_confirms_and_prepares_background_walker() {
    let root = test_root("filelist-use-filelist-confirm");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
    app.index_tx = index_tx;

    assert!(app.use_filelist);
    app.create_filelist();
    assert!(app.filelist_state.pending_use_walker_confirmation.is_some());
    assert_eq!(app.tabs.len(), 1);

    app.confirm_pending_filelist_use_walker();

    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.active_tab, 0);
    assert!(app.use_filelist);
    assert!(app.include_files);
    assert!(app.include_dirs);
    let pending = app
        .filelist_state
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
    assert!(app.notice.contains("Preparing background Walker index"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn deferred_filelist_starts_after_index_finished() {
    let root = test_root("filelist-after-index-finished");
    fs::create_dir_all(&root).expect("create dir");
    let path = root.join("main.rs");
    fs::write(&path, "fn main() {}").expect("write file");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (filelist_tx, filelist_rx) = mpsc::channel::<FileListRequest>();
    app.filelist_tx = filelist_tx;
    let (index_tx, index_rx) = mpsc::channel::<IndexResponse>();
    app.index_rx = index_rx;

    app.use_filelist = false;
    app.index_in_progress = true;
    let tab_id = app.current_tab_id().expect("tab id");
    app.create_filelist();
    let request_id = app.pending_index_request_id.expect("pending index request");

    index_tx
        .send(IndexResponse::Batch {
            request_id,
            entries: vec![IndexEntry {
                path: path.clone(),
                kind: EntryKind::file(),
                kind_known: true,
            }],
        })
        .expect("send batch");

    index_tx
        .send(IndexResponse::Finished {
            request_id,
            source: IndexSource::Walker,
        })
        .expect("send finished");
    app.poll_index_response();

    if app.filelist_state.pending_ancestor_confirmation.is_some() {
        app.skip_pending_filelist_ancestor_propagation();
    }

    let req = filelist_rx
        .try_recv()
        .expect("filelist request should be sent");
    assert_eq!(req.tab_id, tab_id);
    assert_eq!(req.root, root);
    assert_eq!(req.entries, vec![path]);
    assert!(app.filelist_state.pending_after_index.is_none());
    assert!(app.filelist_state.in_progress);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn deferred_filelist_is_canceled_when_root_changes() {
    let root_old = test_root("filelist-deferred-cancel-old");
    let root_new = test_root("filelist-deferred-cancel-new");
    fs::create_dir_all(&root_old).expect("create old dir");
    fs::create_dir_all(&root_new).expect("create new dir");
    let mut app = FlistWalkerApp::new(root_old.clone(), 50, String::new());
    let (index_tx, _index_rx) = mpsc::channel::<IndexRequest>();
    app.index_tx = index_tx;
    let tab_id = app.current_tab_id().expect("tab id");
    app.filelist_state.pending_after_index = Some(PendingFileListAfterIndex {
        tab_id,
        root: root_old.clone(),
    });
    app.root = root_new.clone();

    app.request_index_refresh();

    assert!(app.filelist_state.pending_after_index.is_none());
    assert!(app.notice.contains("Deferred Create File List canceled"));
    let _ = fs::remove_dir_all(&root_old);
    let _ = fs::remove_dir_all(&root_new);
}

#[test]
fn filelist_finish_reindexes_original_tab_after_tab_switch() {
    let root_a = test_root("filelist-finish-background-tab-a");
    let root_b = test_root("filelist-finish-background-tab-b");
    fs::create_dir_all(&root_a).expect("create root a");
    fs::create_dir_all(&root_b).expect("create root b");

    let mut app = FlistWalkerApp::new(root_a.clone(), 50, String::new());
    let source_tab_id = app.current_tab_id().expect("source tab id");
    app.use_filelist = false;
    if let Some(tab) = app.tabs.get_mut(app.active_tab) {
        tab.use_filelist = false;
    }
    app.create_new_tab();
    app.apply_root_change(root_b.clone());
    app.switch_to_tab_index(1);

    let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
    let (filelist_tx, filelist_rx) = mpsc::channel::<FileListResponse>();
    app.index_tx = index_tx;
    app.filelist_rx = filelist_rx;
    app.filelist_state.pending_request_id = Some(1);
    app.filelist_state.pending_request_tab_id = Some(source_tab_id);
    app.filelist_state.pending_root = Some(root_a.clone());
    app.filelist_state.pending_cancel = Some(Arc::new(AtomicBool::new(false)));
    app.filelist_state.in_progress = true;

    filelist_tx
        .send(FileListResponse::Finished {
            request_id: 1,
            root: root_a.clone(),
            path: root_a.join("FileList.txt"),
            count: 1,
        })
        .expect("send filelist finished");
    app.poll_filelist_response();

    let req = index_rx
        .try_recv()
        .expect("background reindex request should be sent");
    assert_eq!(req.tab_id, source_tab_id);
    assert_eq!(req.root, root_a);
    assert!(req.use_filelist);
    let source_tab = app
        .tabs
        .iter()
        .find(|tab| tab.id == source_tab_id)
        .expect("source tab should remain");
    assert!(source_tab.use_filelist);
    assert!(source_tab.index_in_progress);
    assert_eq!(app.active_tab, 1);
    assert_eq!(app.root, root_b);
    let _ = fs::remove_dir_all(&root_a);
    let _ = fs::remove_dir_all(&root_b);
}

#[test]
fn filelist_finish_ignores_original_tab_when_its_root_changed() {
    let root_old = test_root("filelist-finish-root-changed-old");
    let root_new = test_root("filelist-finish-root-changed-new");
    fs::create_dir_all(&root_old).expect("create old root");
    fs::create_dir_all(&root_new).expect("create new root");

    let mut app = FlistWalkerApp::new(root_old.clone(), 50, String::new());
    let source_tab_id = app.current_tab_id().expect("source tab id");
    app.use_filelist = false;
    if let Some(tab) = app.tabs.get_mut(app.active_tab) {
        tab.use_filelist = false;
        tab.root = root_new.clone();
    }
    app.root = root_new.clone();

    let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
    let (filelist_tx, filelist_rx) = mpsc::channel::<FileListResponse>();
    app.index_tx = index_tx;
    app.filelist_rx = filelist_rx;
    app.filelist_state.pending_request_id = Some(2);
    app.filelist_state.pending_request_tab_id = Some(source_tab_id);
    app.filelist_state.pending_root = Some(root_old.clone());
    app.filelist_state.pending_cancel = Some(Arc::new(AtomicBool::new(false)));
    app.filelist_state.in_progress = true;

    filelist_tx
        .send(FileListResponse::Finished {
            request_id: 2,
            root: root_old.clone(),
            path: root_old.join("FileList.txt"),
            count: 1,
        })
        .expect("send filelist finished");
    app.poll_filelist_response();

    assert!(index_rx.try_recv().is_err());
    let source_tab = app
        .tabs
        .iter()
        .find(|tab| tab.id == source_tab_id)
        .expect("source tab should remain");
    assert!(!source_tab.use_filelist);
    assert!(!source_tab.index_in_progress);
    let _ = fs::remove_dir_all(&root_old);
    let _ = fs::remove_dir_all(&root_new);
}

#[test]
fn background_index_send_failure_clears_pending_state_for_target_tab() {
    let root_a = test_root("background-index-send-failure-a");
    let root_b = test_root("background-index-send-failure-b");
    fs::create_dir_all(&root_a).expect("create root a");
    fs::create_dir_all(&root_b).expect("create root b");

    let mut app = FlistWalkerApp::new(root_a.clone(), 50, String::new());
    app.create_new_tab();
    app.root = root_b.clone();
    if let Some(tab) = app.tabs.get_mut(app.active_tab) {
        tab.root = root_b.clone();
    }
    app.sync_active_tab_state();
    app.switch_to_tab_index(0);

    let (_, rx) = mpsc::channel::<IndexRequest>();
    let (closed_tx, _) = mpsc::channel::<IndexRequest>();
    drop(rx);
    app.index_tx = closed_tx;

    app.request_background_index_refresh_for_tab(1);

    let background_tab = app.tabs.get(1).expect("background tab");
    assert!(!background_tab.index_in_progress);
    assert_eq!(background_tab.pending_index_request_id, None);
    assert!(background_tab.pending_index_entries.is_empty());
    assert!(background_tab.notice.contains("Index worker is unavailable"));
    assert!(app.notice.contains("Index worker is unavailable"));

    let _ = fs::remove_dir_all(&root_a);
    let _ = fs::remove_dir_all(&root_b);
}

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
    app.index_tx = tx;
    app.pinned_paths.insert(old_path);
    app.current_row = Some(0);
    app.preview = "stale preview".to_string();
    app.results = vec![(root_old.join("result.txt"), 0.0)];

    app.apply_root_change(root_new.clone());

    assert!(app.pinned_paths.is_empty());
    assert_eq!(app.current_row, None);
    assert!(app.preview.is_empty());
    assert!(app.all_entries.is_empty());
    assert!(app.entries.is_empty());
    assert!(app.results.is_empty());
    assert_eq!(app.tabs[app.active_tab].root, root_new);
    assert!(app.tabs[app.active_tab].all_entries.is_empty());
    assert!(app.tabs[app.active_tab].entries.is_empty());
    let req = rx.try_recv().expect("index request should be sent");
    assert_eq!(req.root, app.root);
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
    app.index_tx = tx;
    let tab_id = app.current_tab_id().expect("tab id");
    app.filelist_state.pending_confirmation = Some(PendingFileListConfirmation {
        tab_id,
        root: root_old.clone(),
        entries: vec![root_old.join("a.txt")],
        existing_path: root_old.join("FileList.txt"),
    });

    app.apply_root_change(root_new.clone());

    assert!(app.filelist_state.pending_confirmation.is_none());
    let _ = fs::remove_dir_all(&root_old);
    let _ = fs::remove_dir_all(&root_new);
}

#[test]
fn filelist_finished_updates_state_and_notice() {
    let root = test_root("filelist-finished");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<FileListResponse>();
    app.filelist_rx = rx;
    app.filelist_state.pending_request_id = Some(11);
    app.filelist_state.pending_request_tab_id = app.current_tab_id();
    app.filelist_state.pending_root = Some(root.clone());
    app.filelist_state.in_progress = true;
    app.use_filelist = false;

    let filelist = root.join("FileList.txt");
    tx.send(FileListResponse::Finished {
        request_id: 11,
        root: root.clone(),
        path: filelist.clone(),
        count: 3,
    })
    .expect("send filelist response");

    app.poll_filelist_response();

    assert_eq!(app.filelist_state.pending_request_id, None);
    assert_eq!(app.filelist_state.pending_request_tab_id, None);
    assert!(!app.filelist_state.in_progress);
    assert!(app.use_filelist);
    assert!(app.notice.contains("Created"));
    assert!(app.notice.contains("3 entries"));
    assert!(app.notice.contains(filelist.to_string_lossy().as_ref()));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn filelist_finished_enables_use_filelist_for_creator_tab() {
    let root = test_root("filelist-finished-enable-creator-tab");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.use_filelist = false;
    app.sync_active_tab_state();
    let creator_tab_id = app.tabs[0].id;
    let (tx, rx) = mpsc::channel::<FileListResponse>();
    app.filelist_rx = rx;
    app.filelist_state.pending_request_id = Some(101);
    app.filelist_state.pending_request_tab_id = Some(creator_tab_id);
    app.filelist_state.pending_root = Some(root.clone());
    app.filelist_state.in_progress = true;

    tx.send(FileListResponse::Finished {
        request_id: 101,
        root: root.clone(),
        path: root.join("FileList.txt"),
        count: 2,
    })
    .expect("send filelist response");

    app.poll_filelist_response();

    let creator_tab = app
        .tabs
        .iter()
        .find(|tab| tab.id == creator_tab_id)
        .expect("creator tab");
    assert!(creator_tab.use_filelist);
    assert!(!app.use_filelist);
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
    app.index_in_progress = false;
    app.use_filelist = false;
    app.all_entries = Arc::new(vec![path.clone()]);
    app.entry_kinds.insert(path, EntryKind::file());
    app.index.source = IndexSource::Walker;

    app.create_filelist();

    assert!(app.filelist_state.pending_confirmation.is_some());
    assert!(!app.filelist_state.in_progress);
    assert!(app.filelist_state.pending_request_id.is_none());
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
    app.filelist_tx = filelist_tx;
    let tab_id = app.current_tab_id().expect("tab id");
    app.filelist_state.pending_confirmation = Some(PendingFileListConfirmation {
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
    assert!(app.filelist_state.in_progress);
    assert!(app.filelist_state.pending_confirmation.is_none());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn cancel_create_filelist_clears_pending_after_index() {
    let root = test_root("filelist-cancel-pending-after-index");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.filelist_state.pending_after_index = Some(PendingFileListAfterIndex {
        tab_id: app.current_tab_id().expect("tab id"),
        root: root.clone(),
    });

    app.cancel_create_filelist();

    assert!(app.filelist_state.pending_after_index.is_none());
    assert!(app.notice.contains("Create File List canceled"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn cancel_create_filelist_marks_inflight_request() {
    let root = test_root("filelist-cancel-inflight");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let cancel = Arc::new(AtomicBool::new(false));
    app.filelist_state.pending_request_id = Some(77);
    app.filelist_state.pending_request_tab_id = app.current_tab_id();
    app.filelist_state.pending_root = Some(root.clone());
    app.filelist_state.pending_cancel = Some(Arc::clone(&cancel));
    app.filelist_state.in_progress = true;
    app.filelist_state.cancel_requested = false;

    app.cancel_create_filelist();

    assert!(cancel.load(Ordering::Relaxed));
    assert!(app.filelist_state.cancel_requested);
    assert!(app.notice.contains("Canceling Create File List"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn create_filelist_requests_confirmation_before_ancestor_propagation() {
    let top = test_root("filelist-ancestor-confirm");
    let root = top.join("child");
    fs::create_dir_all(&root).expect("create child");
    fs::write(top.join("FileList.txt"), "child/old.txt\n").expect("write parent filelist");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    reset_index_request_state_for_test(&mut app);
    app.use_filelist = false;
    app.index.source = IndexSource::Walker;
    app.include_files = true;
    app.include_dirs = true;
    app.all_entries = Arc::new(vec![root.join("main.rs")]);
    app.entries = Arc::clone(&app.all_entries);

    app.create_filelist();

    assert!(
        app.notice.contains("ancestor") || app.notice.contains("parent"),
        "notice should mention ancestor confirmation, got: {}",
        app.notice
    );
    assert!(app.filelist_state.pending_ancestor_confirmation.is_some());
    assert!(app.filelist_state.pending_request_id.is_none());
    assert!(!app.filelist_state.in_progress);
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
    app.use_filelist = false;
    app.index.source = IndexSource::Walker;
    app.include_files = true;
    app.include_dirs = true;
    app.all_entries = Arc::new(vec![root.join("main.rs")]);
    app.entries = Arc::clone(&app.all_entries);
    let (filelist_tx, filelist_rx) = mpsc::channel::<FileListRequest>();
    app.filelist_tx = filelist_tx;

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
fn filelist_finished_triggers_reindex_when_enabled() {
    let root = test_root("filelist-reindex");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (filelist_tx, filelist_rx) = mpsc::channel::<FileListResponse>();
    app.filelist_rx = filelist_rx;
    let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
    app.index_tx = index_tx;
    app.filelist_state.pending_request_id = Some(12);
    app.filelist_state.pending_request_tab_id = app.current_tab_id();
    app.filelist_state.pending_root = Some(root.clone());
    app.filelist_state.in_progress = true;
    app.use_filelist = false;

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
    assert!(app.index_in_progress);
    assert!(app.pending_index_request_id.is_some());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn filelist_failed_updates_state_and_notice() {
    let root = test_root("filelist-failed");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<FileListResponse>();
    app.filelist_rx = rx;
    app.filelist_state.pending_request_id = Some(13);
    app.filelist_state.pending_request_tab_id = app.current_tab_id();
    app.filelist_state.pending_root = Some(root.clone());
    app.filelist_state.in_progress = true;

    tx.send(FileListResponse::Failed {
        request_id: 13,
        root: root.clone(),
        error: "disk full".to_string(),
    })
    .expect("send filelist response");

    app.poll_filelist_response();

    assert_eq!(app.filelist_state.pending_request_id, None);
    assert_eq!(app.filelist_state.pending_request_tab_id, None);
    assert!(!app.filelist_state.in_progress);
    assert!(app.notice.contains("Create File List failed: disk full"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn filelist_canceled_updates_state_and_notice() {
    let root = test_root("filelist-canceled");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<FileListResponse>();
    app.filelist_rx = rx;
    app.filelist_state.pending_request_id = Some(14);
    app.filelist_state.pending_request_tab_id = app.current_tab_id();
    app.filelist_state.pending_root = Some(root.clone());
    app.filelist_state.pending_cancel = Some(Arc::new(AtomicBool::new(true)));
    app.filelist_state.in_progress = true;
    app.filelist_state.cancel_requested = true;

    tx.send(FileListResponse::Canceled {
        request_id: 14,
        root: root.clone(),
    })
    .expect("send filelist response");

    app.poll_filelist_response();

    assert_eq!(app.filelist_state.pending_request_id, None);
    assert!(app.filelist_state.pending_cancel.is_none());
    assert!(!app.filelist_state.in_progress);
    assert!(!app.filelist_state.cancel_requested);
    assert!(app.notice.contains("Create File List canceled"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn filelist_finished_for_previous_root_does_not_trigger_reindex() {
    let root_old = test_root("filelist-prev-root-old");
    let root_new = test_root("filelist-prev-root-new");
    fs::create_dir_all(&root_old).expect("create old dir");
    fs::create_dir_all(&root_new).expect("create new dir");
    let mut app = FlistWalkerApp::new(root_old.clone(), 50, String::new());
    let (filelist_tx, filelist_rx) = mpsc::channel::<FileListResponse>();
    app.filelist_rx = filelist_rx;
    let (_index_tx, index_rx) = mpsc::channel::<IndexRequest>();
    app.index_tx = _index_tx;
    app.filelist_state.pending_request_id = Some(51);
    app.filelist_state.pending_request_tab_id = app.current_tab_id();
    app.filelist_state.pending_root = Some(root_old.clone());
    app.filelist_state.in_progress = true;
    app.use_filelist = true;
    app.root = root_new.clone();

    filelist_tx
        .send(FileListResponse::Finished {
            request_id: 51,
            root: root_old.clone(),
            path: root_old.join("FileList.txt"),
            count: 9,
        })
        .expect("send filelist response");

    app.poll_filelist_response();

    assert!(index_rx.try_recv().is_err());
    assert!(!app.filelist_state.in_progress);
    assert!(app.notice.contains("previous root"));
    let _ = fs::remove_dir_all(&root_old);
    let _ = fs::remove_dir_all(&root_new);
}

#[test]
fn filelist_failed_for_previous_root_reports_without_rewinding_state() {
    let root_old = test_root("filelist-prev-root-fail-old");
    let root_new = test_root("filelist-prev-root-fail-new");
    fs::create_dir_all(&root_old).expect("create old dir");
    fs::create_dir_all(&root_new).expect("create new dir");
    let mut app = FlistWalkerApp::new(root_old.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<FileListResponse>();
    app.filelist_rx = rx;
    app.filelist_state.pending_request_id = Some(52);
    app.filelist_state.pending_request_tab_id = app.current_tab_id();
    app.filelist_state.pending_root = Some(root_old.clone());
    app.filelist_state.in_progress = true;
    app.root = root_new;

    tx.send(FileListResponse::Failed {
        request_id: 52,
        root: root_old.clone(),
        error: "permission denied".to_string(),
    })
    .expect("send filelist response");

    app.poll_filelist_response();

    assert_eq!(app.filelist_state.pending_request_id, None);
    assert!(!app.filelist_state.in_progress);
    assert!(app.notice.contains("previous root"));
    let _ = fs::remove_dir_all(&root_old);
}

#[test]
fn non_empty_query_incremental_refresh_skips_small_delta_during_indexing() {
    let root = test_root("incremental-small-delta-skip");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "main".to_string());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.index_rx = rx;
    app.entries = Arc::new(Vec::new());
    app.all_entries = Arc::new(Vec::new());
    app.index.entries.clear();
    app.incremental_filtered_entries.clear();
    app.search_resume_pending = false;
    app.last_search_snapshot_len = 0;
    app.search_in_progress = false;
    app.pending_request_id = None;
    app.pending_index_request_id = Some(21);
    app.index_in_progress = true;
    app.last_incremental_results_refresh = Instant::now() - Duration::from_secs(3);

    let path = root.join("main.rs");
    tx.send(IndexResponse::Batch {
        request_id: 21,
        entries: vec![IndexEntry {
            path: path.clone(),
            kind: EntryKind::file(),
            kind_known: true,
        }],
    })
    .expect("send index batch");

    app.poll_index_response();

    assert!(app.entries.is_empty());
    assert_eq!(app.incremental_filtered_entries, vec![path]);
    assert!(!app.search_rerun_pending);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn non_empty_query_incremental_refresh_updates_entries_with_large_delta() {
    let root = test_root("incremental-large-delta");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "main".to_string());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.index_rx = rx;
    app.entries = Arc::new(Vec::new());
    app.all_entries = Arc::new(Vec::new());
    app.index.entries.clear();
    app.incremental_filtered_entries.clear();
    app.search_resume_pending = false;
    app.last_search_snapshot_len = 0;
    app.search_in_progress = false;
    app.pending_request_id = None;
    app.pending_index_request_id = Some(218);
    app.index_in_progress = true;
    app.last_incremental_results_refresh = Instant::now() - Duration::from_secs(3);

    let entries = (0..FlistWalkerApp::INCREMENTAL_SEARCH_MIN_DELTA_DURING_INDEX)
        .map(|i| IndexEntry {
            path: root.join(format!("main-{i}.rs")),
            kind: EntryKind::file(),
            kind_known: true,
        })
        .collect::<Vec<_>>();
    tx.send(IndexResponse::Batch {
        request_id: 218,
        entries,
    })
    .expect("send index batch");

    for _ in 0..64 {
        app.last_incremental_results_refresh = Instant::now() - Duration::from_secs(3);
        app.poll_index_response();
        if app.entries.len() >= FlistWalkerApp::INCREMENTAL_SEARCH_MIN_DELTA_DURING_INDEX {
            break;
        }
    }

    assert_eq!(
        app.entries.len(),
        FlistWalkerApp::INCREMENTAL_SEARCH_MIN_DELTA_DURING_INDEX
    );
    assert_eq!(app.incremental_filtered_entries.len(), app.entries.len());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn non_empty_query_batch_delta_updates_snapshot_even_without_search_refresh() {
    let root = test_root("incremental-snapshot-delta");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "main".to_string());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.index_rx = rx;
    app.pending_index_request_id = Some(88);
    app.index_in_progress = true;
    app.search_resume_pending = false;
    app.last_incremental_results_refresh = Instant::now();
    app.last_search_snapshot_len = 0;

    let path_a = root.join("main-a.rs");
    let path_b = root.join("main-b.rs");
    tx.send(IndexResponse::Batch {
        request_id: 88,
        entries: vec![
            IndexEntry {
                path: path_a.clone(),
                kind: EntryKind::file(),
                kind_known: true,
            },
            IndexEntry {
                path: path_b.clone(),
                kind: EntryKind::file(),
                kind_known: true,
            },
        ],
    })
    .expect("send index batch");

    app.poll_index_response();
    app.poll_index_response();

    assert!(app.entries.is_empty());
    assert_eq!(app.incremental_filtered_entries.len(), 2);
    assert_eq!(app.incremental_filtered_entries[0], path_a);
    assert_eq!(app.incremental_filtered_entries[1], path_b);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn empty_query_keeps_results_after_batch_and_finished_in_same_poll() {
    let root = test_root("empty-query-finished-priority");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.index_rx = rx;
    app.pending_index_request_id = Some(31);
    app.index_in_progress = true;

    let path = root.join("main.rs");
    tx.send(IndexResponse::Batch {
        request_id: 31,
        entries: vec![IndexEntry {
            path: path.clone(),
            kind: EntryKind::file(),
            kind_known: true,
        }],
    })
    .expect("send index batch");
    tx.send(IndexResponse::Finished {
        request_id: 31,
        source: IndexSource::Walker,
    })
    .expect("send index finished");

    app.poll_index_response();

    assert_eq!(app.entries.len(), 1);
    assert_eq!(app.results.len(), 1);
    assert_eq!(app.entries[0], path);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn status_line_prefers_current_index_count_while_indexing() {
    let root = test_root("status-line-current-index-count");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.index_in_progress = true;
    app.all_entries = Arc::new(
        (0..10)
            .map(|i| root.join(format!("old-{i}.txt")))
            .collect::<Vec<_>>(),
    );
    app.index.entries = (0..3)
        .map(|i| root.join(format!("new-{i}.txt")))
        .collect::<Vec<_>>();

    app.refresh_status_line();

    assert_eq!(entries_count_from_status(&app.status_line), 3);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn request_index_refresh_keeps_existing_entries_visible_until_new_results_arrive() {
    let root = test_root("refresh-keeps-visible");
    fs::create_dir_all(&root).expect("create dir");
    let path = root.join("keep.txt");
    fs::write(&path, "x").expect("write file");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, _rx) = mpsc::channel::<IndexRequest>();
    app.index_tx = tx;
    app.entries = Arc::new(vec![path.clone()]);
    app.results = vec![(path.clone(), 0.0)];
    app.current_row = Some(0);
    app.preview = "keep".to_string();

    app.request_index_refresh();

    assert_eq!(app.entries.len(), 1);
    assert_eq!(app.results.len(), 1);
    assert_eq!(app.current_row, Some(0));
    assert_eq!(app.preview, "keep");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn incremental_empty_query_update_preserves_scroll_position_flag() {
    let root = test_root("incremental-preserve-scroll");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.index_rx = rx;
    app.pending_index_request_id = Some(41);
    app.index_in_progress = true;
    app.scroll_to_current = false;
    app.current_row = Some(0);

    let path = root.join("main.rs");
    tx.send(IndexResponse::Batch {
        request_id: 41,
        entries: vec![IndexEntry {
            path,
            kind: EntryKind::file(),
            kind_known: true,
        }],
    })
    .expect("send index batch");

    app.poll_index_response();

    assert!(!app.scroll_to_current);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn apply_entry_filters_resyncs_incremental_state_during_indexing() {
    let root = test_root("filters-resync-incremental");
    fs::create_dir_all(root.join("dir")).expect("create dir");
    let file = root.join("main.rs");
    let dir = root.join("dir");
    fs::write(&file, "fn main() {}").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.index_in_progress = true;
    app.index.entries = vec![file.clone(), dir.clone()];
    app.entry_kinds.insert(file.clone(), EntryKind::file());
    app.entry_kinds.insert(dir.clone(), EntryKind::dir());
    app.include_files = false;
    app.include_dirs = true;

    app.apply_entry_filters(true);

    assert_eq!(app.entries.as_ref(), &vec![dir.clone()]);
    assert_eq!(app.incremental_filtered_entries, vec![dir]);
    assert!(app.pending_index_entries.is_empty());
    assert!(app.pending_index_entries_request_id.is_none());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn apply_entry_filters_all_filtered_then_next_batch_adds_once() {
    let root = test_root("filters-all-filtered-then-add");
    fs::create_dir_all(root.join("dir")).expect("create dir");
    let file = root.join("main.rs");
    let dir = root.join("dir");
    fs::write(&file, "fn main() {}").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.index_in_progress = true;
    app.index.entries = vec![file.clone()];
    app.entry_kinds.insert(file.clone(), EntryKind::file());
    app.include_files = false;
    app.include_dirs = true;

    app.apply_entry_filters(true);
    assert!(app.entries.is_empty());
    assert!(app.incremental_filtered_entries.is_empty());

    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.index_rx = rx;
    app.pending_index_request_id = Some(201);
    tx.send(IndexResponse::Batch {
        request_id: 201,
        entries: vec![IndexEntry {
            path: dir.clone(),
            kind: EntryKind::dir(),
            kind_known: true,
        }],
    })
    .expect("send index batch");

    app.poll_index_response();

    assert_eq!(app.entries.as_ref(), &vec![dir]);
    assert_eq!(app.results.len(), 1);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn unknown_kind_entries_remain_visible_when_both_filters_enabled() {
    let root = test_root("unknown-kind-visible");
    fs::create_dir_all(&root).expect("create dir");
    let path = root.join("unknown");
    fs::write(&path, "x").expect("write file");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.all_entries = Arc::new(vec![path.clone()]);
    app.include_files = true;
    app.include_dirs = true;
    app.entry_kinds.clear();

    app.apply_entry_filters(true);

    assert_eq!(app.entries.as_ref(), &vec![path]);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn unknown_kind_entries_do_not_queue_resolution_when_both_filters_enabled() {
    let root = test_root("unknown-kind-no-queue");
    fs::create_dir_all(&root).expect("create dir");
    let path = root.join("unknown");
    fs::write(&path, "x").expect("write file");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.all_entries = Arc::new(vec![path.clone()]);
    app.include_files = true;
    app.include_dirs = true;
    app.show_preview = false;
    app.entry_kinds.clear();
    app.pending_kind_paths.clear();
    app.pending_kind_paths_set.clear();
    app.in_flight_kind_paths.clear();

    app.apply_entry_filters(true);

    assert!(!app.pending_kind_paths.iter().any(|p| *p == path));
    assert!(!app.in_flight_kind_paths.contains(&path));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn walker_unknown_kind_batch_still_finishes_and_keeps_entries_visible() {
    let root = test_root("walker-unknown-kind-finish-visible");
    fs::create_dir_all(&root).expect("create dir");
    let path = root.join("app.lnk");
    fs::write(&path, "shortcut").expect("write file");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.index_rx = rx;
    let req_id = app.pending_index_request_id.expect("pending request");
    app.include_files = true;
    app.include_dirs = true;

    tx.send(IndexResponse::Batch {
        request_id: req_id,
        entries: vec![IndexEntry {
            path: path.clone(),
            kind: EntryKind::file(),
            kind_known: false,
        }],
    })
    .expect("send index batch");
    tx.send(IndexResponse::Finished {
        request_id: req_id,
        source: IndexSource::Walker,
    })
    .expect("send index finished");

    app.poll_index_response();

    assert!(!app.index_in_progress);
    assert_eq!(app.entries.as_ref(), &vec![path.clone()]);
    assert_eq!(app.all_entries.as_ref(), &vec![path.clone()]);
    assert!(!app.entry_kinds.contains_key(&path));
    assert!(app.pending_kind_paths.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn walker_finished_queues_unknown_kind_resolution_when_both_filters_enabled() {
    let root = test_root("walker-finished-queues-unknown-kind");
    fs::create_dir_all(&root).expect("create dir");
    let path = root.join("app.lnk");
    fs::write(&path, "shortcut").expect("write file");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    let (kind_tx, kind_rx) = mpsc::channel::<KindResolveRequest>();
    app.index_rx = rx;
    app.kind_tx = kind_tx;
    let req_id = app.pending_index_request_id.expect("pending request");
    app.include_files = true;
    app.include_dirs = true;

    tx.send(IndexResponse::Batch {
        request_id: req_id,
        entries: vec![IndexEntry {
            path: path.clone(),
            kind: EntryKind::file(),
            kind_known: false,
        }],
    })
    .expect("send index batch");
    tx.send(IndexResponse::Finished {
        request_id: req_id,
        source: IndexSource::Walker,
    })
    .expect("send index finished");

    app.poll_index_response();
    app.pump_kind_resolution_requests();

    let req = kind_rx.try_recv().expect("kind resolve request should be queued");
    assert_eq!(req.path, path.clone());
    assert!(app.kind_resolution_in_progress);
    assert!(app.in_flight_kind_paths.contains(&path));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn unknown_kind_entries_are_hidden_when_single_filter_enabled() {
    let root = test_root("unknown-kind-hidden");
    fs::create_dir_all(&root).expect("create dir");
    let path = root.join("unknown");
    fs::write(&path, "x").expect("write file");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.all_entries = Arc::new(vec![path]);
    app.include_files = false;
    app.include_dirs = true;
    app.entry_kinds.clear();

    app.apply_entry_filters(true);

    assert!(app.entries.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn unknown_kind_entries_queue_resolution_when_single_filter_enabled() {
    let root = test_root("unknown-kind-queue");
    fs::create_dir_all(&root).expect("create dir");
    let path = root.join("unknown");
    fs::write(&path, "x").expect("write file");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.all_entries = Arc::new(vec![path.clone()]);
    app.include_files = false;
    app.include_dirs = true;
    app.entry_kinds.clear();

    app.apply_entry_filters(true);

    assert!(app.pending_kind_paths.iter().any(|p| *p == path));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn walker_unknown_kind_batch_queues_resolution_when_single_filter_enabled() {
    let root = test_root("walker-unknown-kind-queue");
    fs::create_dir_all(&root).expect("create dir");
    let path = root.join("app.lnk");
    fs::write(&path, "shortcut").expect("write file");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.index_rx = rx;
    let req_id = app.pending_index_request_id.expect("pending request");
    app.include_files = false;
    app.include_dirs = true;

    tx.send(IndexResponse::Batch {
        request_id: req_id,
        entries: vec![IndexEntry {
            path: path.clone(),
            kind: EntryKind::file(),
            kind_known: false,
        }],
    })
    .expect("send index batch");

    app.poll_index_response();

    assert!(app.entries.is_empty());
    assert!(app.pending_kind_paths.iter().any(|p| *p == path));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn kind_response_updates_filters_when_single_filter_is_enabled() {
    let root = test_root("kind-response-refreshes-filters");
    fs::create_dir_all(root.join("dir")).expect("create dir");
    let dir = root.join("dir");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.all_entries = Arc::new(vec![dir.clone()]);
    app.include_files = false;
    app.include_dirs = true;
    app.entry_kinds.clear();
    app.apply_entry_filters(true);
    assert!(app.entries.is_empty());

    let (tx, rx) = mpsc::channel::<KindResolveResponse>();
    app.kind_rx = rx;
    app.in_flight_kind_paths.insert(dir.clone());
    tx.send(KindResolveResponse {
        epoch: app.kind_resolution_epoch,
        path: dir.clone(),
        kind: Some(EntryKind::dir()),
    })
    .expect("send kind response");

    app.poll_kind_response();

    assert_eq!(app.entry_kinds.get(&dir), Some(&EntryKind::dir()));
    assert_eq!(app.entries.as_ref(), &vec![dir]);
    let _ = fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[test]
fn kind_resolver_marks_symlink_as_link() {
    use std::os::unix::fs::symlink;

    let root = test_root("kind-resolver-symlink-link");
    fs::create_dir_all(&root).expect("create dir");
    let target = root.join("target.txt");
    let link = root.join("target-link.txt");
    fs::write(&target, "hello").expect("write target");
    symlink(&target, &link).expect("create symlink");

    let shutdown = Arc::new(AtomicBool::new(false));
    let (tx, rx, handle) = spawn_kind_resolver_worker(Arc::clone(&shutdown));
    tx.send(KindResolveRequest {
        epoch: 7,
        path: link.clone(),
    })
    .expect("send kind resolve request");
    drop(tx);

    let response = rx
        .recv_timeout(Duration::from_secs(1))
        .expect("kind response");
    shutdown.store(true, Ordering::Relaxed);
    handle.join().expect("join kind resolver");

    assert_eq!(response.epoch, 7);
    assert_eq!(response.path, link);
    assert_eq!(response.kind, Some(EntryKind::link(false)));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn request_preview_queues_on_demand_kind_resolution_when_kind_unknown() {
    let root = test_root("preview-on-demand-kind");
    fs::create_dir_all(&root).expect("create dir");
    let path = root.join("unknown.txt");
    fs::write(&path, "hello").expect("write file");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<KindResolveRequest>();
    app.kind_tx = tx;
    app.results = vec![(path.clone(), 0.0)];
    app.current_row = Some(0);
    app.include_files = true;
    app.include_dirs = true;
    app.entry_kinds.clear();

    app.request_preview_for_current();

    let req = rx.try_recv().expect("kind resolve request should be sent");
    assert_eq!(req.path, path);
    assert_eq!(req.epoch, app.kind_resolution_epoch);
    assert_eq!(app.preview, "Resolving entry type...");
    assert!(app.pending_preview_request_id.is_none());
    assert!(!app.preview_in_progress);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn request_index_refresh_reenables_files_when_both_filters_are_off() {
    let root = test_root("request-refresh-filter-guard");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexRequest>();
    app.index_tx = tx;
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
    app.index_tx = tx;
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
fn files_toggle_change_requests_reindex() {
    let root = test_root("files-toggle-reindex");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexRequest>();
    app.index_tx = tx;
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
    app.index_tx = tx;
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
    app.index_tx = tx;
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
    app.filelist_tx = filelist_tx;
    app.use_filelist = true;
    app.index.source = IndexSource::Walker;
    app.index_in_progress = false;

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
    app.filelist_tx = filelist_tx;
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
    app.filelist_state.pending_use_walker_confirmation = Some(PendingFileListUseWalkerConfirmation {
        source_tab_id: app.current_tab_id().expect("tab id"),
        root: root.clone(),
    });

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::Enter,
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

    app.index_inflight_requests.insert(100);
    app.index_inflight_requests.insert(101);
    app.index_request_tabs.insert(100, bg_tab_a);
    app.index_request_tabs.insert(101, bg_tab_b);
    app.pending_index_queue.push_back(IndexRequest {
        request_id: 102,
        tab_id: active_tab_id,
        root: root.clone(),
        use_filelist: false,
        include_files: true,
        include_dirs: true,
    });
    {
        let mut latest = app.latest_index_request_ids.lock().expect("lock latest");
        latest.insert(bg_tab_a, 100);
        latest.insert(bg_tab_b, 101);
    }

    assert!(app.preempt_background_for_active_request());

    let latest = app.latest_index_request_ids.lock().expect("lock latest");
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
    app.index_rx = rx;
    let stale_request_id = 777u64;
    let current_tab_id = app.current_tab_id().expect("tab id");
    app.pending_index_request_id = Some(778);
    app.index_inflight_requests.insert(stale_request_id);
    app.index_request_tabs
        .insert(stale_request_id, current_tab_id);

    tx.send(IndexResponse::Finished {
        request_id: stale_request_id,
        source: IndexSource::Walker,
    })
    .expect("send finished");

    app.poll_index_response();

    assert!(!app.index_inflight_requests.contains(&stale_request_id));
    assert!(!app.index_request_tabs.contains_key(&stale_request_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn current_finished_index_response_clears_inflight_slot() {
    let root = test_root("current-finished-clears-inflight");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.index_rx = rx;
    let req_id = app.pending_index_request_id.expect("pending request");
    let tab_id = app.current_tab_id().expect("tab id");
    app.index_request_tabs.insert(req_id, tab_id);
    app.index_inflight_requests.insert(req_id);

    tx.send(IndexResponse::Finished {
        request_id: req_id,
        source: IndexSource::Walker,
    })
    .expect("send finished");

    app.poll_index_response();

    assert!(!app.index_inflight_requests.contains(&req_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn same_tab_request_waits_until_previous_inflight_finishes() {
    let root = test_root("same-tab-inflight-serialization");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let tab_id = app.current_tab_id().expect("tab id");

    app.index_inflight_requests.insert(1);
    app.index_request_tabs.insert(1, tab_id);
    app.pending_index_queue.push_back(IndexRequest {
        request_id: 2,
        tab_id,
        root: root.clone(),
        use_filelist: false,
        include_files: true,
        include_dirs: true,
    });

    assert!(app.pop_next_index_request().is_none());

    app.index_inflight_requests.remove(&1);
    let popped = app
        .pop_next_index_request()
        .expect("queued same-tab request should run");
    assert_eq!(popped.request_id, 2);
    let _ = fs::remove_dir_all(&root);
}

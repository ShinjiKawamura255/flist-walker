use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[test]
fn search_error_updates_notice() {
    let root = test_root("search-error-notice");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<SearchResponse>();
    app.search.rx = rx;
    app.search.set_pending_request_id(Some(7));
    app.search.set_in_progress(true);

    tx.send(SearchResponse {
        request_id: 7,
        results: Vec::new(),
        error: Some("invalid regex '[*': syntax error".to_string()),
    })
    .expect("send search response");

    app.poll_search_response();

    assert!(!app.search.in_progress());
    assert!(app.runtime.notice.contains("Search failed:"));
    assert!(app.runtime.notice.contains("invalid regex"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn stale_search_response_is_ignored_after_index_refresh() {
    let root = test_root("stale-search-after-refresh");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "abc".to_string());
    let (search_tx, search_rx) = mpsc::channel::<SearchResponse>();
    let (index_tx, _index_rx) = mpsc::channel::<IndexRequest>();
    app.search.rx = search_rx;
    app.indexing.tx = index_tx;
    app.search.set_pending_request_id(Some(5));
    app.search.set_in_progress(true);
    app.runtime.results = vec![(root.join("before.txt"), 0.0)];

    app.request_index_refresh();

    search_tx
        .send(SearchResponse {
            request_id: 5,
            results: vec![(root.join("stale.txt"), 1.0)],
            error: None,
        })
        .expect("send stale search response");

    app.poll_search_response();

    assert!(!app.search.in_progress());
    assert_eq!(app.search.pending_request_id(), None);
    assert_eq!(app.runtime.results[0].0, root.join("before.txt"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn index_refresh_marks_search_resume_pending_for_non_empty_query() {
    let root = test_root("resume-pending-on-refresh");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "abc".to_string());
    let (index_tx, _index_rx) = mpsc::channel::<IndexRequest>();
    app.indexing.tx = index_tx;

    app.request_index_refresh();

    assert!(app.indexing.search_resume_pending);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn non_empty_query_resumes_search_immediately_on_first_index_batch() {
    let root = test_root("resume-first-batch");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "main".to_string());
    let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
    app.indexing.tx = index_tx;
    // Use a manual search channel so the test can inspect enqueued requests.
    let (search_tx_real, search_rx_real) = mpsc::channel::<SearchRequest>();
    app.search.tx = search_tx_real;

    app.request_index_refresh();
    let req = index_rx.try_recv().expect("index request should be sent");

    let (tx_idx, rx_idx) = mpsc::channel::<IndexResponse>();
    app.indexing.rx = rx_idx;
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
    app.indexing.last_incremental_results_refresh = Instant::now();
    app.poll_index_response();

    let search_req = search_rx_real
        .try_recv()
        .expect("search should resume immediately");
    assert_eq!(search_req.query, "main");
    assert_eq!(search_req.entries.len(), 1);
    assert_eq!(search_req.entries[0], root.join("main.rs"));
    assert!(!app.indexing.search_resume_pending);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn filtered_out_batch_still_resumes_non_empty_query_search() {
    let root = test_root("resume-first-batch-filtered-out");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "main".to_string());
    app.runtime.include_files = false;
    app.runtime.include_dirs = true;
    let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
    app.indexing.tx = index_tx;
    let (search_tx_real, search_rx_real) = mpsc::channel::<SearchRequest>();
    app.search.tx = search_tx_real;

    app.request_index_refresh();
    let req = index_rx.try_recv().expect("index request should be sent");

    let (tx_idx, rx_idx) = mpsc::channel::<IndexResponse>();
    app.indexing.rx = rx_idx;
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
    app.indexing.last_incremental_results_refresh = Instant::now();
    app.poll_index_response();

    let search_req = search_rx_real
        .try_recv()
        .expect("search should still resume even when batch is filtered out");
    assert!(search_req.entries.is_empty());
    assert_eq!(search_req.query, "main");
    assert!(!app.indexing.search_resume_pending);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn create_filelist_waits_while_indexing() {
    let root = test_root("filelist-waits-indexing");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
    app.indexing.tx = index_tx;
    app.runtime.use_filelist = false;
    app.runtime.index.source = IndexSource::Walker;
    app.runtime.include_files = true;
    app.runtime.include_dirs = true;
    app.indexing.in_progress = true;

    app.create_filelist();

    assert_eq!(
        app.features
            .filelist
            .pending_after_index
            .as_ref()
            .map(|pending| pending.root.clone()),
        Some(root.clone())
    );
    assert!(app.features.filelist.pending_request_id.is_none());
    assert!(!app.features.filelist.in_progress);
    assert!(index_rx.try_recv().is_err());
    assert!(app.runtime.notice.contains("Waiting for current indexing"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn create_filelist_while_indexing_with_filter_change_requests_reindex() {
    let root = test_root("filelist-waits-indexing-needs-reindex");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
    app.indexing.tx = index_tx;
    app.runtime.use_filelist = false;
    app.runtime.index.source = IndexSource::Walker;
    app.runtime.include_files = false;
    app.runtime.include_dirs = true;
    app.indexing.in_progress = true;

    app.create_filelist();

    let req = index_rx.try_recv().expect("reindex request should be sent");
    assert_eq!(req.root, root);
    assert!(req.include_files);
    assert!(req.include_dirs);
    assert!(app.features.filelist.pending_after_index.is_some());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn create_filelist_forces_files_and_dirs_before_reindex() {
    let root = test_root("filelist-force-files-dirs");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
    app.indexing.tx = index_tx;
    app.runtime.use_filelist = false;
    app.runtime.include_files = false;
    app.runtime.include_dirs = true;
    app.runtime.index.source = IndexSource::Walker;

    app.create_filelist();

    assert!(app.runtime.include_files);
    assert!(app.runtime.include_dirs);
    let req = index_rx.try_recv().expect("reindex request should be sent");
    assert_eq!(req.root, root);
    assert!(!req.use_filelist);
    assert!(req.include_files);
    assert!(req.include_dirs);
    assert!(app.features.filelist.pending_after_index.is_some());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn create_filelist_with_use_filelist_enabled_confirms_and_prepares_background_walker() {
    let root = test_root("filelist-use-filelist-confirm");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
    app.indexing.tx = index_tx;

    assert!(app.runtime.use_filelist);
    app.create_filelist();
    assert!(app
        .features
        .filelist
        .pending_use_walker_confirmation
        .is_some());
    assert_eq!(app.tabs.len(), 1);

    app.confirm_pending_filelist_use_walker();

    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.tabs.active_tab, 0);
    assert!(app.runtime.use_filelist);
    assert!(app.runtime.include_files);
    assert!(app.runtime.include_dirs);
    let pending = app
        .features
        .filelist
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
    assert!(app.runtime.notice.contains("Preparing background Walker index"));
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
    app.worker_bus.filelist.tx = filelist_tx;
    let (index_tx, index_rx) = mpsc::channel::<IndexResponse>();
    app.indexing.rx = index_rx;

    app.runtime.use_filelist = false;
    app.indexing.in_progress = true;
    let tab_id = app.current_tab_id().expect("tab id");
    app.create_filelist();
    let request_id = app
        .indexing
        .pending_request_id
        .expect("pending index request");

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

    if app
        .features
        .filelist
        .pending_ancestor_confirmation
        .is_some()
    {
        app.skip_pending_filelist_ancestor_propagation();
    }

    let req = filelist_rx
        .try_recv()
        .expect("filelist request should be sent");
    assert_eq!(req.tab_id, tab_id);
    assert_eq!(req.root, root);
    assert_eq!(req.entries, vec![path]);
    assert!(app.features.filelist.pending_after_index.is_none());
    assert!(app.features.filelist.in_progress);
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
    app.indexing.tx = index_tx;
    let tab_id = app.current_tab_id().expect("tab id");
    app.features.filelist.pending_after_index = Some(PendingFileListAfterIndex {
        tab_id,
        root: root_old.clone(),
    });
    app.runtime.root = root_new.clone();

    app.request_index_refresh();

    assert!(app.features.filelist.pending_after_index.is_none());
    assert!(app.runtime.notice.contains("Deferred Create File List canceled"));
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
    app.runtime.use_filelist = false;
    let active_tab = app.tabs.active_tab;
    if let Some(tab) = app.tabs.get_mut(active_tab) {
        tab.use_filelist = false;
    }
    app.create_new_tab();
    app.apply_root_change(root_b.clone());
    app.switch_to_tab_index(1);

    let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
    let (filelist_tx, filelist_rx) = mpsc::channel::<FileListResponse>();
    app.indexing.tx = index_tx;
    app.worker_bus.filelist.rx = filelist_rx;
    app.features.filelist.pending_request_id = Some(1);
    app.features.filelist.pending_request_tab_id = Some(source_tab_id);
    app.features.filelist.pending_root = Some(root_a.clone());
    app.features.filelist.pending_cancel = Some(Arc::new(AtomicBool::new(false)));
    app.features.filelist.in_progress = true;

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
    assert!(source_tab.index_state.index_in_progress);
    assert_eq!(app.tabs.active_tab, 1);
    assert_eq!(app.runtime.root, root_b);
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
    app.runtime.use_filelist = false;
    let active_tab = app.tabs.active_tab;
    if let Some(tab) = app.tabs.get_mut(active_tab) {
        tab.use_filelist = false;
        tab.root = root_new.clone();
    }
    app.runtime.root = root_new.clone();

    let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
    let (filelist_tx, filelist_rx) = mpsc::channel::<FileListResponse>();
    app.indexing.tx = index_tx;
    app.worker_bus.filelist.rx = filelist_rx;
    app.features.filelist.pending_request_id = Some(2);
    app.features.filelist.pending_request_tab_id = Some(source_tab_id);
    app.features.filelist.pending_root = Some(root_old.clone());
    app.features.filelist.pending_cancel = Some(Arc::new(AtomicBool::new(false)));
    app.features.filelist.in_progress = true;

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
    assert!(!source_tab.index_state.index_in_progress);
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
    app.runtime.root = root_b.clone();
    let active_tab = app.tabs.active_tab;
    if let Some(tab) = app.tabs.get_mut(active_tab) {
        tab.root = root_b.clone();
    }
    app.sync_active_tab_state();
    app.switch_to_tab_index(0);
    app.tabs.pending_restore_refresh = true;

    let (_, rx) = mpsc::channel::<IndexRequest>();
    let (closed_tx, _) = mpsc::channel::<IndexRequest>();
    drop(rx);
    app.indexing.tx = closed_tx;

    app.request_background_index_refresh_for_tab(1);

    let background_tab = app.tabs.get(1).expect("background tab");
    assert!(!background_tab.index_state.index_in_progress);
    assert_eq!(background_tab.index_state.pending_index_request_id, None);
    assert!(background_tab.index_state.pending_index_entries.is_empty());
    assert!(!app.tabs.pending_restore_refresh);
    assert!(background_tab
        .notice
        .contains("Index worker is unavailable"));
    assert!(app.runtime.notice.contains("Index worker is unavailable"));

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
    app.indexing.tx = tx;
    app.runtime.pinned_paths.insert(old_path);
    app.runtime.current_row = Some(0);
    app.runtime.preview = "stale preview".to_string();
    app.runtime.results = vec![(root_old.join("result.txt"), 0.0)];

    app.apply_root_change(root_new.clone());

    assert!(app.runtime.pinned_paths.is_empty());
    assert_eq!(app.runtime.current_row, None);
    assert!(app.runtime.preview.is_empty());
    assert!(app.runtime.all_entries.is_empty());
    assert!(app.runtime.entries.is_empty());
    assert!(app.runtime.results.is_empty());
    let active_tab = app.tabs.active_tab;
    assert_eq!(app.tabs[active_tab].root, root_new);
    assert!(app.tabs[active_tab].index_state.all_entries.is_empty());
    assert!(app.tabs[active_tab].index_state.entries.is_empty());
    let req = rx.try_recv().expect("index request should be sent");
    assert_eq!(req.root, app.runtime.root);
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
    app.indexing.tx = tx;
    let tab_id = app.current_tab_id().expect("tab id");
    app.features.filelist.pending_confirmation = Some(PendingFileListConfirmation {
        tab_id,
        root: root_old.clone(),
        entries: vec![root_old.join("a.txt")],
        existing_path: root_old.join("FileList.txt"),
    });

    app.apply_root_change(root_new.clone());

    assert!(app.features.filelist.pending_confirmation.is_none());
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
    app.indexing.tx = tx;
    let tab_id = app.current_tab_id().expect("tab id");
    app.features.filelist.pending_ancestor_confirmation =
        Some(PendingFileListAncestorConfirmation {
            tab_id,
            root: root_old.clone(),
            entries: vec![root_old.join("a.txt")],
        });

    app.apply_root_change(root_new.clone());

    assert!(app
        .features
        .filelist
        .pending_ancestor_confirmation
        .is_none());
    assert!(app.runtime.notice.contains("Root changed"));
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
    app.indexing.tx = tx;
    let tab_id = app.current_tab_id().expect("tab id");
    app.features.filelist.pending_use_walker_confirmation =
        Some(PendingFileListUseWalkerConfirmation {
            source_tab_id: tab_id,
            root: root_old.clone(),
        });

    app.apply_root_change(root_new.clone());

    assert!(app
        .features
        .filelist
        .pending_use_walker_confirmation
        .is_none());
    assert!(app.runtime.notice.contains("Root changed"));
    let _ = fs::remove_dir_all(&root_old);
    let _ = fs::remove_dir_all(&root_new);
}

#[test]
fn filelist_finished_updates_state_and_notice() {
    let root = test_root("filelist-finished");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<FileListResponse>();
    app.worker_bus.filelist.rx = rx;
    app.features.filelist.pending_request_id = Some(11);
    app.features.filelist.pending_request_tab_id = app.current_tab_id();
    app.features.filelist.pending_root = Some(root.clone());
    app.features.filelist.in_progress = true;
    app.runtime.use_filelist = false;

    let filelist = root.join("FileList.txt");
    tx.send(FileListResponse::Finished {
        request_id: 11,
        root: root.clone(),
        path: filelist.clone(),
        count: 3,
    })
    .expect("send filelist response");

    app.poll_filelist_response();

    assert_eq!(app.features.filelist.pending_request_id, None);
    assert_eq!(app.features.filelist.pending_request_tab_id, None);
    assert!(!app.features.filelist.in_progress);
    assert!(app.runtime.use_filelist);
    assert!(app.runtime.notice.contains("Created"));
    assert!(app.runtime.notice.contains("3 entries"));
    assert!(app.runtime.notice.contains(filelist.to_string_lossy().as_ref()));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn filelist_finished_enables_use_filelist_for_creator_tab() {
    let root = test_root("filelist-finished-enable-creator-tab");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.runtime.use_filelist = false;
    app.sync_active_tab_state();
    let creator_tab_id = app.tabs[0].id;
    let (tx, rx) = mpsc::channel::<FileListResponse>();
    app.worker_bus.filelist.rx = rx;
    app.features.filelist.pending_request_id = Some(101);
    app.features.filelist.pending_request_tab_id = Some(creator_tab_id);
    app.features.filelist.pending_root = Some(root.clone());
    app.features.filelist.in_progress = true;

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
    assert!(!app.runtime.use_filelist);
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
    app.indexing.in_progress = false;
    app.runtime.use_filelist = false;
    app.runtime.all_entries = Arc::new(vec![file_entry(path.clone())]);
    app.set_entry_kind(&path, EntryKind::file());
    app.runtime.index.source = IndexSource::Walker;

    app.create_filelist();

    assert!(app.features.filelist.pending_confirmation.is_some());
    assert!(!app.features.filelist.in_progress);
    assert!(app.features.filelist.pending_request_id.is_none());
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
    app.worker_bus.filelist.tx = filelist_tx;
    let tab_id = app.current_tab_id().expect("tab id");
    app.features.filelist.pending_confirmation = Some(PendingFileListConfirmation {
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
    assert!(app.features.filelist.in_progress);
    assert!(app.features.filelist.pending_confirmation.is_none());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn cancel_create_filelist_clears_pending_after_index() {
    let root = test_root("filelist-cancel-pending-after-index");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.features.filelist.pending_after_index = Some(PendingFileListAfterIndex {
        tab_id: app.current_tab_id().expect("tab id"),
        root: root.clone(),
    });

    app.cancel_create_filelist();

    assert!(app.features.filelist.pending_after_index.is_none());
    assert!(app.runtime.notice.contains("Create File List canceled"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn cancel_create_filelist_marks_inflight_request() {
    let root = test_root("filelist-cancel-inflight");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let cancel = Arc::new(AtomicBool::new(false));
    app.features.filelist.pending_request_id = Some(77);
    app.features.filelist.pending_request_tab_id = app.current_tab_id();
    app.features.filelist.pending_root = Some(root.clone());
    app.features.filelist.pending_cancel = Some(Arc::clone(&cancel));
    app.features.filelist.in_progress = true;
    app.features.filelist.cancel_requested = false;

    app.cancel_create_filelist();

    assert!(cancel.load(Ordering::Relaxed));
    assert!(app.features.filelist.cancel_requested);
    assert!(app.runtime.notice.contains("Canceling Create File List"));
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
    app.runtime.use_filelist = false;
    app.runtime.index.source = IndexSource::Walker;
    app.runtime.include_files = true;
    app.runtime.include_dirs = true;
    app.runtime.all_entries = Arc::new(vec![unknown_entry(root.join("main.rs"))]);
    app.runtime.entries = Arc::clone(&app.runtime.all_entries);

    app.create_filelist();

    assert!(
        app.runtime.notice.contains("ancestor") || app.runtime.notice.contains("parent"),
        "notice should mention ancestor confirmation, got: {}",
        app.runtime.notice
    );
    assert!(app
        .features
        .filelist
        .pending_ancestor_confirmation
        .is_some());
    assert!(app.features.filelist.pending_request_id.is_none());
    assert!(!app.features.filelist.in_progress);
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
    app.runtime.use_filelist = false;
    app.runtime.index.source = IndexSource::Walker;
    app.runtime.include_files = true;
    app.runtime.include_dirs = true;
    app.runtime.all_entries = Arc::new(vec![unknown_entry(root.join("main.rs"))]);
    app.runtime.entries = Arc::clone(&app.runtime.all_entries);
    let (filelist_tx, filelist_rx) = mpsc::channel::<FileListRequest>();
    app.worker_bus.filelist.tx = filelist_tx;

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
    app.worker_bus.filelist.rx = filelist_rx;
    let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
    app.indexing.tx = index_tx;
    app.features.filelist.pending_request_id = Some(12);
    app.features.filelist.pending_request_tab_id = app.current_tab_id();
    app.features.filelist.pending_root = Some(root.clone());
    app.features.filelist.in_progress = true;
    app.runtime.use_filelist = false;

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
    assert!(app.indexing.in_progress);
    assert!(app.indexing.pending_request_id.is_some());
    let _ = fs::remove_dir_all(&root);
}

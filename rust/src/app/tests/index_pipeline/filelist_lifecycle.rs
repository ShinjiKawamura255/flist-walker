use super::*;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[test]
fn filelist_failed_updates_state_and_notice() {
    let root = test_root("filelist-failed");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<FileListResponse>();
    app.worker_bus.filelist.rx = rx;
    app.features.filelist.pending_request_id = Some(13);
    app.features.filelist.pending_request_tab_id = app.current_tab_id();
    app.features.filelist.pending_root = Some(root.clone());
    app.features.filelist.in_progress = true;

    tx.send(FileListResponse::Failed {
        request_id: 13,
        root: root.clone(),
        error: "disk full".to_string(),
    })
    .expect("send filelist response");

    app.poll_filelist_response();

    assert_eq!(app.features.filelist.pending_request_id, None);
    assert_eq!(app.features.filelist.pending_request_tab_id, None);
    assert!(!app.features.filelist.in_progress);
    assert!(app.notice.contains("Create File List failed: disk full"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn filelist_canceled_updates_state_and_notice() {
    let root = test_root("filelist-canceled");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<FileListResponse>();
    app.worker_bus.filelist.rx = rx;
    app.features.filelist.pending_request_id = Some(14);
    app.features.filelist.pending_request_tab_id = app.current_tab_id();
    app.features.filelist.pending_root = Some(root.clone());
    app.features.filelist.pending_cancel = Some(Arc::new(AtomicBool::new(true)));
    app.features.filelist.in_progress = true;
    app.features.filelist.cancel_requested = true;

    tx.send(FileListResponse::Canceled {
        request_id: 14,
        root: root.clone(),
    })
    .expect("send filelist response");

    app.poll_filelist_response();

    assert_eq!(app.features.filelist.pending_request_id, None);
    assert!(app.features.filelist.pending_cancel.is_none());
    assert!(!app.features.filelist.in_progress);
    assert!(!app.features.filelist.cancel_requested);
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
    app.worker_bus.filelist.rx = filelist_rx;
    let (_index_tx, index_rx) = mpsc::channel::<IndexRequest>();
    app.indexing.tx = _index_tx;
    app.features.filelist.pending_request_id = Some(51);
    app.features.filelist.pending_request_tab_id = app.current_tab_id();
    app.features.filelist.pending_root = Some(root_old.clone());
    app.features.filelist.in_progress = true;
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
    assert!(!app.features.filelist.in_progress);
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
    app.worker_bus.filelist.rx = rx;
    app.features.filelist.pending_request_id = Some(52);
    app.features.filelist.pending_request_tab_id = app.current_tab_id();
    app.features.filelist.pending_root = Some(root_old.clone());
    app.features.filelist.in_progress = true;
    app.root = root_new;

    tx.send(FileListResponse::Failed {
        request_id: 52,
        root: root_old.clone(),
        error: "permission denied".to_string(),
    })
    .expect("send filelist response");

    app.poll_filelist_response();

    assert_eq!(app.features.filelist.pending_request_id, None);
    assert!(!app.features.filelist.in_progress);
    assert!(app.notice.contains("previous root"));
    let _ = fs::remove_dir_all(&root_old);
}

#[test]
fn filelist_finished_for_stale_requested_root_is_ignored() {
    let root_requested = test_root("filelist-stale-requested-root-requested");
    let root_response = test_root("filelist-stale-requested-root-response");
    fs::create_dir_all(&root_requested).expect("create requested dir");
    fs::create_dir_all(&root_response).expect("create response dir");
    let mut app = FlistWalkerApp::new(root_response.clone(), 50, String::new());
    let (filelist_tx, filelist_rx) = mpsc::channel::<FileListResponse>();
    app.worker_bus.filelist.rx = filelist_rx;
    app.features.filelist.pending_request_id = Some(53);
    app.features.filelist.pending_request_tab_id = app.current_tab_id();
    app.features.filelist.pending_root = Some(root_requested.clone());
    app.features.filelist.in_progress = true;
    app.use_filelist = false;

    filelist_tx
        .send(FileListResponse::Finished {
            request_id: 53,
            root: root_response.clone(),
            path: root_response.join("FileList.txt"),
            count: 4,
        })
        .expect("send filelist response");

    app.poll_filelist_response();

    assert_eq!(app.features.filelist.pending_request_id, None);
    assert!(!app.features.filelist.in_progress);
    assert!(!app.use_filelist);
    assert!(app.notice.is_empty());
    let _ = fs::remove_dir_all(&root_requested);
    let _ = fs::remove_dir_all(&root_response);
}

#[test]
fn non_empty_query_incremental_refresh_skips_small_delta_during_indexing() {
    let root = test_root("incremental-small-delta-skip");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "main".to_string());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.indexing.rx = rx;
    app.entries = Arc::new(Vec::new());
    app.all_entries = Arc::new(Vec::new());
    app.index.entries.clear();
    app.indexing.incremental_filtered_entries.clear();
    app.indexing.search_resume_pending = false;
    app.indexing.last_search_snapshot_len = 0;
    app.search.set_in_progress(false);
    app.search.set_pending_request_id(None);
    app.indexing.pending_request_id = Some(21);
    app.indexing.in_progress = true;
    app.indexing.last_incremental_results_refresh = Instant::now() - Duration::from_secs(3);

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
    assert_eq!(
        app.indexing.incremental_filtered_entries,
        vec![file_entry(path)]
    );
    assert!(!app.indexing.search_rerun_pending);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn non_empty_query_incremental_refresh_updates_entries_with_large_delta() {
    let root = test_root("incremental-large-delta");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "main".to_string());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.indexing.rx = rx;
    app.entries = Arc::new(Vec::new());
    app.all_entries = Arc::new(Vec::new());
    app.index.entries.clear();
    app.indexing.incremental_filtered_entries.clear();
    app.indexing.search_resume_pending = false;
    app.indexing.last_search_snapshot_len = 0;
    app.search.set_in_progress(false);
    app.search.set_pending_request_id(None);
    app.indexing.pending_request_id = Some(218);
    app.indexing.in_progress = true;
    app.indexing.last_incremental_results_refresh = Instant::now() - Duration::from_secs(3);

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
        app.indexing.last_incremental_results_refresh = Instant::now() - Duration::from_secs(3);
        app.poll_index_response();
        if app.entries.len() >= FlistWalkerApp::INCREMENTAL_SEARCH_MIN_DELTA_DURING_INDEX {
            break;
        }
    }

    assert_eq!(
        app.entries.len(),
        FlistWalkerApp::INCREMENTAL_SEARCH_MIN_DELTA_DURING_INDEX
    );
    assert_eq!(
        app.indexing.incremental_filtered_entries.len(),
        app.entries.len()
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn non_empty_query_batch_delta_updates_snapshot_even_without_search_refresh() {
    let root = test_root("incremental-snapshot-delta");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "main".to_string());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.indexing.rx = rx;
    app.indexing.pending_request_id = Some(88);
    app.indexing.in_progress = true;
    app.indexing.search_resume_pending = false;
    app.indexing.last_incremental_results_refresh = Instant::now();
    app.indexing.last_search_snapshot_len = 0;

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
    assert_eq!(app.indexing.incremental_filtered_entries.len(), 2);
    assert_eq!(app.indexing.incremental_filtered_entries[0], path_a);
    assert_eq!(app.indexing.incremental_filtered_entries[1], path_b);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn empty_query_keeps_results_after_batch_and_finished_in_same_poll() {
    let root = test_root("empty-query-finished-priority");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.indexing.rx = rx;
    app.indexing.pending_request_id = Some(31);
    app.indexing.in_progress = true;

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
    app.indexing.in_progress = true;
    app.all_entries = Arc::new(
        (0..10)
            .map(|i| unknown_entry(root.join(format!("old-{i}.txt"))))
            .collect::<Vec<_>>(),
    );
    app.index.entries = (0..3)
        .map(|i| unknown_entry(root.join(format!("new-{i}.txt"))))
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
    app.indexing.tx = tx;
    app.entries = Arc::new(vec![unknown_entry(path.clone())]);
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
    app.indexing.rx = rx;
    app.indexing.pending_request_id = Some(41);
    app.indexing.in_progress = true;
    app.ui.scroll_to_current = false;
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

    assert!(!app.ui.scroll_to_current);
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
    app.indexing.in_progress = true;
    app.index.entries = vec![file_entry(file.clone()), dir_entry(dir.clone())];
    app.set_entry_kind(&file, EntryKind::file());
    app.set_entry_kind(&dir, EntryKind::dir());
    app.include_files = false;
    app.include_dirs = true;

    app.apply_entry_filters(true);

    assert_eq!(app.entries.as_ref(), &vec![dir.clone()]);
    assert_eq!(
        app.indexing.incremental_filtered_entries,
        vec![dir_entry(dir)]
    );
    assert!(app.indexing.pending_entries.is_empty());
    assert!(app.indexing.pending_entries_request_id.is_none());
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
    app.indexing.in_progress = true;
    app.index.entries = vec![file_entry(file.clone())];
    app.set_entry_kind(&file, EntryKind::file());
    app.include_files = false;
    app.include_dirs = true;

    app.apply_entry_filters(true);
    assert!(app.entries.is_empty());
    assert!(app.indexing.incremental_filtered_entries.is_empty());

    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.indexing.rx = rx;
    app.indexing.pending_request_id = Some(201);
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

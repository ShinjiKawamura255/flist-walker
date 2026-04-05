use super::*;
use std::sync::Arc;

#[test]
fn unknown_kind_entries_remain_visible_when_both_filters_enabled() {
    let root = test_root("unknown-kind-visible");
    fs::create_dir_all(&root).expect("create dir");
    let path = root.join("unknown");
    fs::write(&path, "x").expect("write file");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.all_entries = Arc::new(vec![unknown_entry(path.clone())]);
    app.include_files = true;
    app.include_dirs = true;
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
    app.all_entries = Arc::new(vec![unknown_entry(path.clone())]);
    app.include_files = true;
    app.include_dirs = true;
    app.ui.show_preview = false;
    app.indexing.pending_kind_paths.clear();
    app.indexing.pending_kind_paths_set.clear();
    app.indexing.in_flight_kind_paths.clear();

    app.apply_entry_filters(true);

    assert!(!app.indexing.pending_kind_paths.iter().any(|p| *p == path));
    assert!(!app.indexing.in_flight_kind_paths.contains(&path));
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
    app.indexing.rx = rx;
    let req_id = app.indexing.pending_request_id.expect("pending request");
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

    assert!(!app.indexing.in_progress);
    assert_eq!(app.entries.as_ref(), &vec![path.clone()]);
    assert_eq!(app.all_entries.as_ref(), &vec![path.clone()]);
    assert!(app.find_entry_kind(&path).is_none());
    assert!(app.indexing.pending_kind_paths.is_empty());
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
    app.indexing.rx = rx;
    app.worker_bus.kind.tx = kind_tx;
    let req_id = app.indexing.pending_request_id.expect("pending request");
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

    let req = kind_rx
        .try_recv()
        .expect("kind resolve request should be queued");
    assert_eq!(req.path, path.clone());
    assert!(app.indexing.kind_resolution_in_progress);
    assert!(app.indexing.in_flight_kind_paths.contains(&path));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn unknown_kind_entries_are_hidden_when_single_filter_enabled() {
    let root = test_root("unknown-kind-hidden");
    fs::create_dir_all(&root).expect("create dir");
    let path = root.join("unknown");
    fs::write(&path, "x").expect("write file");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.all_entries = Arc::new(vec![unknown_entry(path)]);
    app.include_files = false;
    app.include_dirs = true;
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
    app.all_entries = Arc::new(vec![unknown_entry(path.clone())]);
    app.include_files = false;
    app.include_dirs = true;
    app.apply_entry_filters(true);

    assert!(app.indexing.pending_kind_paths.iter().any(|p| *p == path));
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
    app.indexing.rx = rx;
    let req_id = app.indexing.pending_request_id.expect("pending request");
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
    assert!(app.indexing.pending_kind_paths.iter().any(|p| *p == path));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn kind_response_updates_filters_when_single_filter_is_enabled() {
    let root = test_root("kind-response-refreshes-filters");
    fs::create_dir_all(root.join("dir")).expect("create dir");
    let dir = root.join("dir");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.all_entries = Arc::new(vec![unknown_entry(dir.clone())]);
    app.include_files = false;
    app.include_dirs = true;
    app.apply_entry_filters(true);
    assert!(app.entries.is_empty());

    let (tx, rx) = mpsc::channel::<KindResolveResponse>();
    app.worker_bus.kind.rx = rx;
    app.indexing.in_flight_kind_paths.insert(dir.clone());
    tx.send(KindResolveResponse {
        epoch: app.indexing.kind_resolution_epoch,
        path: dir.clone(),
        kind: Some(EntryKind::dir()),
    })
    .expect("send kind response");

    app.poll_kind_response();

    assert_eq!(app.find_entry_kind(&dir), Some(EntryKind::dir()));
    assert_eq!(app.entries.as_ref(), &vec![dir]);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn kind_response_batch_updates_multiple_entries_in_one_poll() {
    let root = test_root("kind-response-batch-updates");
    let left = root.join("left");
    let right = root.join("right");
    fs::create_dir_all(&left).expect("create left dir");
    fs::create_dir_all(&right).expect("create right dir");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.all_entries = Arc::new(vec![
        unknown_entry(left.clone()),
        unknown_entry(right.clone()),
    ]);
    app.include_files = false;
    app.include_dirs = true;
    app.apply_entry_filters(true);

    let (tx, rx) = mpsc::channel::<KindResolveResponse>();
    app.worker_bus.kind.rx = rx;
    app.indexing.in_flight_kind_paths.insert(left.clone());
    app.indexing.in_flight_kind_paths.insert(right.clone());
    let epoch = app.indexing.kind_resolution_epoch;
    tx.send(KindResolveResponse {
        epoch,
        path: left.clone(),
        kind: Some(EntryKind::dir()),
    })
    .expect("send left kind response");
    tx.send(KindResolveResponse {
        epoch,
        path: right.clone(),
        kind: Some(EntryKind::dir()),
    })
    .expect("send right kind response");

    app.poll_kind_response();

    assert_eq!(app.find_entry_kind(&left), Some(EntryKind::dir()));
    assert_eq!(app.find_entry_kind(&right), Some(EntryKind::dir()));
    assert_eq!(app.entries.as_ref(), &vec![left.clone(), right.clone()]);
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
    app.worker_bus.kind.tx = tx;
    app.results = vec![(path.clone(), 0.0)];
    app.current_row = Some(0);
    app.include_files = true;
    app.include_dirs = true;

    app.request_preview_for_current();

    let req = rx.try_recv().expect("kind resolve request should be sent");
    assert_eq!(req.path, path);
    assert_eq!(req.epoch, app.indexing.kind_resolution_epoch);
    assert_eq!(app.preview, "Resolving entry type...");
    assert!(app.worker_bus.preview.pending_request_id.is_none());
    assert!(!app.worker_bus.preview.in_progress);
    let _ = fs::remove_dir_all(&root);
}

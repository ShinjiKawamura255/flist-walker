use super::*;

#[test]
fn unknown_kind_entries_remain_visible_when_both_filters_enabled() {
    let root = test_root("unknown-kind-visible");
    fs::create_dir_all(&root).expect("create dir");
    let path = root.join("unknown");
    fs::write(&path, "x").expect("write file");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.all_entries = Arc::new(vec![unknown_entry(path.clone())]);
    app.shell.runtime.include_files = true;
    app.shell.runtime.include_dirs = true;
    app.apply_entry_filters(true);

    assert_eq!(app.shell.runtime.entries.as_ref(), &vec![path]);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn unknown_kind_entries_do_not_queue_resolution_when_both_filters_enabled() {
    let root = test_root("unknown-kind-no-queue");
    fs::create_dir_all(&root).expect("create dir");
    let path = root.join("unknown");
    fs::write(&path, "x").expect("write file");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.all_entries = Arc::new(vec![unknown_entry(path.clone())]);
    app.shell.runtime.include_files = true;
    app.shell.runtime.include_dirs = true;
    app.shell.ui.show_preview = false;
    app.shell.indexing.pending_kind_paths.clear();
    app.shell.indexing.pending_kind_paths_set.clear();
    app.shell.indexing.in_flight_kind_paths.clear();

    app.apply_entry_filters(true);

    assert!(!app
        .shell
        .indexing
        .pending_kind_paths
        .iter()
        .any(|p| *p == path));
    assert!(!app.shell.indexing.in_flight_kind_paths.contains(&path));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn active_kind_queue_walks_entries_without_duplicate_requests() {
    let root = test_root("active-kind-queue-no-duplicates");
    fs::create_dir_all(&root).expect("create dir");
    let known = root.join("known.txt");
    let queued = root.join("queued.lnk");
    let pending = root.join("pending.lnk");
    let inflight = root.join("inflight.lnk");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.include_files = false;
    app.shell.runtime.include_dirs = true;
    app.shell.indexing.in_progress = true;
    app.shell.runtime.index.entries = vec![
        file_entry(known.clone()),
        unknown_entry(queued.clone()),
        unknown_entry(pending.clone()),
        unknown_entry(inflight.clone()),
    ];
    app.shell
        .indexing
        .pending_kind_paths_set
        .insert(pending.clone());
    app.shell
        .indexing
        .pending_kind_paths
        .push_back(pending.clone());
    app.shell
        .indexing
        .in_flight_kind_paths
        .insert(inflight.clone());

    app.queue_unknown_kind_paths_for_active_entries();

    let queued_count = app
        .shell
        .indexing
        .pending_kind_paths
        .iter()
        .filter(|path| **path == queued)
        .count();
    let pending_count = app
        .shell
        .indexing
        .pending_kind_paths
        .iter()
        .filter(|path| **path == pending)
        .count();
    assert_eq!(queued_count, 1);
    assert_eq!(pending_count, 1);
    assert!(!app
        .shell
        .indexing
        .pending_kind_paths
        .iter()
        .any(|path| *path == known));
    assert!(!app
        .shell
        .indexing
        .pending_kind_paths
        .iter()
        .any(|path| *path == inflight));

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
    app.shell.indexing.rx = rx;
    let req_id = app
        .shell
        .indexing
        .pending_request_id
        .expect("pending request");
    app.shell.runtime.include_files = true;
    app.shell.runtime.include_dirs = true;

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

    assert!(!app.shell.indexing.in_progress);
    assert_eq!(app.shell.runtime.entries.as_ref(), &vec![path.clone()]);
    assert_eq!(app.shell.runtime.all_entries.as_ref(), &vec![path.clone()]);
    assert!(app.find_entry_kind(&path).is_none());
    assert!(app.shell.indexing.kind_resolution_in_progress);
    assert!(app.shell.indexing.in_flight_kind_paths.contains(&path));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn walker_finished_queues_unknown_kind_resolution_for_visible_results_only_regression() {
    let root = test_root("walker-finished-queues-visible-results-only-regression");
    fs::create_dir_all(&root).expect("create dir");
    let first = root.join("app.lnk");
    let second = root.join("bg.lnk");
    fs::write(&first, "shortcut").expect("write first file");
    fs::write(&second, "shortcut").expect("write second file");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.limit = 1;
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    let (kind_tx, kind_rx) = mpsc::channel::<KindResolveRequest>();
    app.shell.indexing.rx = rx;
    app.shell.worker_bus.kind.tx = kind_tx;
    let req_id = app
        .shell
        .indexing
        .pending_request_id
        .expect("pending request");
    app.shell.runtime.include_files = true;
    app.shell.runtime.include_dirs = true;

    tx.send(IndexResponse::Batch {
        request_id: req_id,
        entries: vec![
            IndexEntry {
                path: first.clone(),
                kind: EntryKind::file(),
                kind_known: false,
            },
            IndexEntry {
                path: second.clone(),
                kind: EntryKind::file(),
                kind_known: false,
            },
        ],
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
        .expect("visible kind resolve request should be queued");
    assert_eq!(req.path, first.clone());
    assert!(kind_rx.try_recv().is_err());
    assert!(app.shell.indexing.kind_resolution_in_progress);
    assert!(app.shell.indexing.in_flight_kind_paths.contains(&first));
    assert!(!app.shell.indexing.in_flight_kind_paths.contains(&second));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn unknown_kind_entries_are_hidden_when_single_filter_enabled() {
    let root = test_root("unknown-kind-hidden");
    fs::create_dir_all(&root).expect("create dir");
    let path = root.join("unknown");
    fs::write(&path, "x").expect("write file");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.all_entries = Arc::new(vec![unknown_entry(path)]);
    app.shell.runtime.include_files = false;
    app.shell.runtime.include_dirs = true;
    app.apply_entry_filters(true);

    assert!(app.shell.runtime.entries.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn unknown_kind_entries_queue_resolution_when_single_filter_enabled() {
    let root = test_root("unknown-kind-queue");
    fs::create_dir_all(&root).expect("create dir");
    let path = root.join("unknown");
    fs::write(&path, "x").expect("write file");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.all_entries = Arc::new(vec![unknown_entry(path.clone())]);
    app.shell.runtime.include_files = false;
    app.shell.runtime.include_dirs = true;
    app.apply_entry_filters(true);

    assert!(app
        .shell
        .indexing
        .pending_kind_paths
        .iter()
        .any(|p| *p == path));
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
    app.shell.indexing.rx = rx;
    let req_id = app
        .shell
        .indexing
        .pending_request_id
        .expect("pending request");
    app.shell.runtime.include_files = false;
    app.shell.runtime.include_dirs = true;

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

    assert!(app.shell.runtime.entries.is_empty());
    assert!(app
        .shell
        .indexing
        .pending_kind_paths
        .iter()
        .any(|p| *p == path));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn kind_response_updates_filters_when_single_filter_is_enabled() {
    let root = test_root("kind-response-refreshes-filters");
    fs::create_dir_all(root.join("dir")).expect("create dir");
    let dir = root.join("dir");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.all_entries = Arc::new(vec![unknown_entry(dir.clone())]);
    app.shell.runtime.include_files = false;
    app.shell.runtime.include_dirs = true;
    app.apply_entry_filters(true);
    assert!(app.shell.runtime.entries.is_empty());

    let (tx, rx) = mpsc::channel::<KindResolveResponse>();
    app.shell.worker_bus.kind.rx = rx;
    app.shell.indexing.in_flight_kind_paths.insert(dir.clone());
    tx.send(KindResolveResponse {
        tab_id: app.current_tab_id().unwrap_or_default(),
        epoch: app.shell.indexing.kind_resolution_epoch,
        path: dir.clone(),
        kind: Some(EntryKind::dir()),
    })
    .expect("send kind response");

    app.poll_kind_response();

    assert_eq!(app.find_entry_kind(&dir), Some(EntryKind::dir()));
    assert_eq!(app.shell.runtime.entries.as_ref(), &vec![dir]);
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
    app.shell.runtime.all_entries = Arc::new(vec![
        unknown_entry(left.clone()),
        unknown_entry(right.clone()),
    ]);
    app.shell.runtime.include_files = false;
    app.shell.runtime.include_dirs = true;
    app.apply_entry_filters(true);

    let (tx, rx) = mpsc::channel::<KindResolveResponse>();
    app.shell.worker_bus.kind.rx = rx;
    app.shell.indexing.in_flight_kind_paths.insert(left.clone());
    app.shell
        .indexing
        .in_flight_kind_paths
        .insert(right.clone());
    let epoch = app.shell.indexing.kind_resolution_epoch;
    tx.send(KindResolveResponse {
        tab_id: app.current_tab_id().unwrap_or_default(),
        epoch,
        path: left.clone(),
        kind: Some(EntryKind::dir()),
    })
    .expect("send left kind response");
    tx.send(KindResolveResponse {
        tab_id: app.current_tab_id().unwrap_or_default(),
        epoch,
        path: right.clone(),
        kind: Some(EntryKind::dir()),
    })
    .expect("send right kind response");

    app.poll_kind_response();

    assert_eq!(app.find_entry_kind(&left), Some(EntryKind::dir()));
    assert_eq!(app.find_entry_kind(&right), Some(EntryKind::dir()));
    assert_eq!(
        app.shell.runtime.entries.as_ref(),
        &vec![left.clone(), right.clone()]
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn inactive_tab_kind_response_is_retained_until_tab_activation() {
    let root = test_root("inactive-tab-kind-response-retained");
    fs::create_dir_all(&root).expect("create dir");
    let link = root.join("tail.lnk");
    fs::write(&link, "shortcut").expect("write shortcut");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    let inactive_tab_id = app.shell.tabs.get(0).expect("inactive tab").id;
    let epoch = 23;
    {
        let tab = app.shell.tabs.get_mut(0).expect("inactive tab");
        tab.index_state.index.source = IndexSource::Walker;
        tab.index_state.all_entries = Arc::new(vec![unknown_entry(link.clone())]);
        tab.index_state.entries = Arc::clone(&tab.index_state.all_entries);
        tab.index_state.kind_resolution_epoch = epoch;
        tab.index_state.in_flight_kind_paths.insert(link.clone());
        tab.index_state.kind_resolution_in_progress = true;
        tab.result_state.results = vec![(link.clone(), 0.0)];
    }

    let (tx, rx) = mpsc::channel::<KindResolveResponse>();
    app.shell.worker_bus.kind.rx = rx;
    tx.send(KindResolveResponse {
        tab_id: inactive_tab_id,
        epoch,
        path: link.clone(),
        kind: Some(EntryKind::link(false)),
    })
    .expect("send inactive tab kind response");

    app.poll_kind_response();

    let inactive_tab = app.shell.tabs.get(0).expect("inactive tab");
    assert!(!inactive_tab
        .index_state
        .in_flight_kind_paths
        .contains(&link));
    assert_eq!(
        inactive_tab.index_state.resolved_kind_updates,
        vec![(link.clone(), EntryKind::link(false))]
    );

    app.switch_to_tab_index(0);
    assert_eq!(app.find_entry_kind(&link), Some(EntryKind::link(false)));
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
        tab_id: 1,
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
    app.shell.worker_bus.kind.tx = tx;
    app.shell.runtime.results = vec![(path.clone(), 0.0)];
    app.shell.runtime.current_row = Some(0);
    app.shell.runtime.include_files = true;
    app.shell.runtime.include_dirs = true;

    app.request_preview_for_current();

    let req = rx.try_recv().expect("kind resolve request should be sent");
    assert_eq!(req.path, path);
    assert_eq!(req.epoch, app.shell.indexing.kind_resolution_epoch);
    assert_eq!(app.shell.runtime.preview, "Resolving entry type...");
    assert!(app.shell.worker_bus.preview.pending_request_id.is_none());
    assert!(!app.shell.worker_bus.preview.in_progress);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn request_preview_does_not_requeue_terminal_other_kind() {
    let root = test_root("preview-terminal-other-kind");
    fs::create_dir_all(&root).expect("create dir");
    let path = root.join("socket");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<KindResolveRequest>();
    app.shell.worker_bus.kind.tx = tx;
    app.shell.runtime.results = vec![(path.clone(), 0.0)];
    app.shell.runtime.current_row = Some(0);
    app.set_entry_kind(&path, EntryKind::other());

    app.request_preview_for_current();

    assert!(rx.try_recv().is_err());
    assert_eq!(app.shell.runtime.preview, "<preview unavailable>");
    assert!(!app.shell.indexing.kind_resolution_in_progress);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn poll_kind_response_does_not_clone_arc_shared_entries_regression() {
    let root = test_root("kind-response-no-arc-clone-regression");
    let left = root.join("left");
    fs::create_dir_all(&left).expect("create left dir");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.all_entries = Arc::new(vec![unknown_entry(left.clone())]);
    app.shell.runtime.entries = Arc::clone(&app.shell.runtime.all_entries);

    // Simulate search worker holding a clone of the Arc, making strong_count > 1
    let worker_entries = Arc::clone(&app.shell.runtime.all_entries);
    assert!(Arc::strong_count(&app.shell.runtime.all_entries) > 1);

    let ptr_before = app.shell.runtime.all_entries.as_ptr();

    let (tx, rx) = mpsc::channel::<KindResolveResponse>();
    app.shell.worker_bus.kind.rx = rx;
    app.shell.indexing.in_flight_kind_paths.insert(left.clone());
    tx.send(KindResolveResponse {
        tab_id: app.current_tab_id().unwrap_or_default(),
        epoch: app.shell.indexing.kind_resolution_epoch,
        path: left.clone(),
        kind: Some(EntryKind::dir()),
    })
    .expect("send left kind response");

    app.poll_kind_response();

    let ptr_after = app.shell.runtime.all_entries.as_ptr();
    assert_eq!(
        ptr_before, ptr_after,
        "Arc<Vec> should not be reallocated/cloned during kind metadata updates. Arc cloning causes severe UI freezes (v0.16.0 regression)."
    );
    assert_eq!(app.find_entry_kind(&left), Some(EntryKind::dir()));

    // Keep it alive until the check passes
    drop(worker_entries);
    let _ = fs::remove_dir_all(&root);
}

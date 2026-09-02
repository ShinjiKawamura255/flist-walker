use super::*;

#[test]
fn process_shutdown_flag_can_be_set_and_cleared() {
    clear_process_shutdown_request();
    assert!(!process_shutdown_requested());
    request_process_shutdown();
    assert!(process_shutdown_requested());
    clear_process_shutdown_request();
    assert!(!process_shutdown_requested());
}

#[test]
fn worker_runtime_join_all_with_timeout_returns_joined_when_workers_finish() {
    let shutdown = Arc::new(AtomicBool::new(false));
    let mut runtime = WorkerRuntime::new(Arc::clone(&shutdown));
    runtime.push("worker-a", thread::spawn(|| {}));
    runtime.push("worker-b", thread::spawn(|| {}));

    let summary = runtime.join_all_with_timeout(Duration::from_millis(500));

    assert_eq!(summary.total, 2);
    assert_eq!(summary.joined, 2);
    assert!(summary.pending.is_empty());
}

#[test]
fn worker_runtime_join_all_with_timeout_returns_early_on_timeout() {
    let shutdown = Arc::new(AtomicBool::new(false));
    let mut runtime = WorkerRuntime::new(Arc::clone(&shutdown));
    runtime.push(
        "slow-worker",
        thread::spawn(|| {
            thread::sleep(Duration::from_millis(200));
        }),
    );

    let summary = runtime.join_all_with_timeout(Duration::from_millis(10));

    assert_eq!(summary.total, 1);
    assert_eq!(summary.joined, 0);
    assert_eq!(summary.pending, vec!["slow-worker".to_string()]);
}

#[test]
fn regression_gui_close_uses_short_worker_join_timeout_budget() {
    assert!(FlistWalkerApp::worker_join_timeout() <= Duration::from_millis(250));
}

#[test]
fn tc_153_runtime_registers_direct_bounded_worker_handles() {
    let root = test_root("tc-153-direct-worker-handles");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let names = app
        .shell
        .worker_runtime
        .as_ref()
        .expect("worker runtime")
        .worker_names();
    assert!(names.contains(&"action-0".to_string()));
    assert!(names.contains(&"action-1".to_string()));
    assert!(names.contains(&"index-0".to_string()));
    assert!(names.contains(&"index-1".to_string()));
    assert!(names.contains(&"search-catalog".to_string()));
    assert!(names.contains(&"tab-reclaimer".to_string()));
    assert!(!names.contains(&"action".to_string()));

    let summary = app
        .shutdown_workers_with_timeout(Duration::from_millis(250), "tc-153")
        .expect("shutdown summary");
    assert_eq!(summary.joined, summary.total);
    assert!(summary.pending.is_empty());
    assert_eq!(summary.total, names.len() + 1);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_shutdown_drops_tab_snapshots_on_the_registered_drain_worker() {
    let root = test_root("tc-207-shutdown-drain-owner");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    {
        let inactive = app.shell.tabs.get_mut(0).expect("inactive tab");
        let entry = file_entry(root.join("heavy.txt"));
        inactive
            .index_state
            .set_lifecycle_for_test(TabResourceLifecycle::Ready);
        inactive
            .index_state
            .set_committed_snapshot_present_for_test(true);
        inactive.result_state.committed.all_entries = Arc::new(vec![entry.clone()]);
        inactive.result_state.committed.entries = Arc::new(vec![entry]);
    }
    let _observer_guard = lock_reclaim_drop_observer_for_test();
    let (drop_tx, drop_rx) = mpsc::channel();
    set_reclaim_drop_observer(Some(drop_tx));

    let summary = app
        .shutdown_workers_with_timeout(Duration::from_millis(250), "tc-207")
        .expect("shutdown summary");

    set_reclaim_drop_observer(None);
    assert_eq!(summary.joined, summary.total);
    let drop_threads = drop_rx.try_iter().collect::<Vec<_>>();
    assert!(
        drop_threads
            .iter()
            .any(|name| name == "flistwalker-tab-shutdown-drain"),
        "drop threads: {drop_threads:?}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_shutdown_drain_owns_mailbox_replace_all_and_stale_reclaim_debts() {
    let root = test_root("tc-207-shutdown-direct-index-debts");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let tab_id = app.current_tab_id().expect("active tab");
    let request_id = app.shell.indexing.allocate_request_id(Some(tab_id));
    let mailbox = app
        .shell
        .indexing
        .response_mailboxes
        .lock()
        .expect("mailboxes")
        .get(&request_id)
        .cloned()
        .expect("request mailbox");
    mailbox
        .try_publish(IndexResponse::Batch {
            request_id,
            entries: (0..1_000)
                .map(|index| IndexEntry {
                    path: root.join(format!("mailbox-{index}.txt")),
                    kind: EntryKind::file(),
                    kind_known: true,
                })
                .collect(),
        })
        .expect("seed mailbox batch");
    drop(mailbox);
    app.shell.indexing.pending_replace_all = Some(IndexResponse::ReplaceAll {
        request_id,
        entries: (0..1_000)
            .map(|index| IndexEntry {
                path: root.join(format!("replace-{index}.txt")),
                kind: EntryKind::file(),
                kind_known: true,
            })
            .collect(),
    });
    let _observer_guard = lock_reclaim_drop_observer_for_test();
    let (drop_tx, drop_rx) = mpsc::channel();
    set_reclaim_drop_observer(Some(drop_tx));
    let mut stale = RetiredIndexBuildResources::empty();
    stale.set_stale_index_entries(
        (0..1_000)
            .map(|index| IndexEntry {
                path: root.join(format!("stale-{index}.txt")),
                kind: EntryKind::file(),
                kind_known: true,
            })
            .collect(),
    );
    app.shell.indexing.pending_stale_build_reclaim = Some((Some(request_id), stale));

    let summary = app
        .shutdown_workers_with_timeout(Duration::from_millis(250), "tc-207-direct-debts")
        .expect("shutdown summary");

    set_reclaim_drop_observer(None);
    assert_eq!(summary.joined, summary.total);
    assert!(app.shell.indexing.pending_replace_all.is_none());
    assert!(app.shell.indexing.pending_stale_build_reclaim.is_none());
    assert!(app
        .shell
        .indexing
        .response_mailboxes
        .lock()
        .expect("mailboxes")
        .is_empty());
    let drop_threads = drop_rx.try_iter().collect::<Vec<_>>();
    assert!(
        drop_threads
            .iter()
            .any(|name| name == "flistwalker-tab-shutdown-drain"),
        "drop threads: {drop_threads:?}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_shutdown_drain_owns_unfinished_and_scratch_pending_finalizers() {
    let root = test_root("tc-207-shutdown-finalization-debts");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.create_new_tab();
    reset_index_request_state_for_test(&mut app);

    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut held = app.capture_active_tab_state(9_000 + index as u64);
        held.result_state.committed.all_entries =
            Arc::new(vec![file_entry(root.join(format!("held-{index}.txt")))]);
        held.index_state
            .set_committed_snapshot_present_for_test(true);
        app.shell
            .tabs
            .retire_tab_resources_for_test(held.take_heavy_resources())
            .expect("fill reclaimer");
    }

    let mut request_ids = Vec::new();
    for (tab_index, entry_count) in [(0, 100_000), (1, 1)] {
        let tab_id = app.shell.tabs.get(tab_index).expect("inactive tab").id;
        let request_id = app.shell.indexing.allocate_request_id(Some(tab_id));
        request_ids.push(request_id);
        {
            let tab = app.shell.tabs.get_mut(tab_index).expect("inactive tab");
            tab.index_state.pending_index_request_id = Some(request_id);
            tab.index_state.index_in_progress = true;
        }
        app.shell.indexing.background_states.insert(
            request_id,
            BackgroundIndexState {
                source: Some(IndexSource::FileList(root.join("FileList.txt"))),
                entries: (0..entry_count)
                    .map(|index| {
                        file_entry(root.join(format!("tab-{tab_index}-entry-{index}.txt")))
                    })
                    .collect(),
                replaced: true,
            },
        );
        app.handle_background_index_response(
            tab_index,
            IndexResponse::Finished {
                request_id,
                source: IndexSource::FileList(root.join("FileList.txt")),
            },
        );
    }

    let unfinished = app
        .shell
        .indexing
        .background_finalizations
        .get(&request_ids[0])
        .expect("unfinished finalizer");
    assert!(!unfinished.is_complete());
    let scratch_pending = app
        .shell
        .indexing
        .background_finalizations
        .get(&request_ids[1])
        .expect("scratch-pending finalizer");
    assert!(scratch_pending.is_complete());
    assert!(!scratch_pending.scratch_reclaimed);

    app.shell.tabs.resume_resource_reclaimer();
    let _observer_guard = lock_reclaim_drop_observer_for_test();
    let (drop_tx, drop_rx) = mpsc::channel();
    set_reclaim_drop_observer(Some(drop_tx));
    let mut drain_probe = RetiredIndexBuildResources::empty();
    drain_probe.set_stale_index_entries(vec![IndexEntry {
        path: root.join("shutdown-drain-probe.txt"),
        kind: EntryKind::file(),
        kind_known: true,
    }]);
    app.shell.indexing.pending_stale_build_reclaim = Some((None, drain_probe));
    let summary = app
        .shutdown_workers_with_timeout(Duration::from_millis(250), "tc-207-finalizers")
        .expect("shutdown summary");
    set_reclaim_drop_observer(None);

    assert_eq!(summary.joined, summary.total);
    assert_eq!(
        app.shell.indexing.background_finalizations.keys().count(),
        0
    );
    let drop_threads = drop_rx.try_iter().collect::<Vec<_>>();
    assert!(
        drop_threads
            .iter()
            .any(|name| name == "flistwalker-tab-shutdown-drain"),
        "drop threads: {drop_threads:?}"
    );
    let _ = fs::remove_dir_all(&root);
}

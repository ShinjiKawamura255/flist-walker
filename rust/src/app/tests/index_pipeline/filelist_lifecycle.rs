use super::*;
use std::sync::atomic::AtomicBool;

#[test]
fn tc_204_empty_committed_snapshot_refreshes_and_cancels_back_to_ready() {
    let root = test_root("tc-204-empty-committed");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (request_tx, _request_rx) = bounded_request_channel::<IndexRequest>(2);
    let (response_tx, response_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.tx = request_tx;
    app.shell.indexing.rx = response_rx;
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    app.shell
        .indexing
        .set_committed_snapshot_present_for_test(true);
    app.shell.runtime.all_entries = Arc::new(Vec::new());
    app.shell.runtime.entries = Arc::new(Vec::new());

    app.request_index_refresh();
    let request_id = app
        .shell
        .indexing
        .pending_request_id
        .expect("refresh request");
    assert_eq!(
        app.shell.indexing.lifecycle(),
        TabResourceLifecycle::Refreshing
    );

    response_tx
        .send(IndexResponse::Canceled { request_id })
        .expect("send canceled");
    app.poll_index_response();

    assert_eq!(app.shell.indexing.lifecycle(), TabResourceLifecycle::Ready);
    assert!(app.shell.indexing.committed_snapshot_present());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_active_failure_reclaims_building_payload_off_ui() {
    let root = test_root("tc-207-active-failure-reclaim");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (response_tx, response_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = response_rx;
    app.shell.indexing.build.index.entries = (0..2_000)
        .map(|index| file_entry(root.join(format!("building-{index}.txt"))))
        .collect();
    app.shell.indexing.pending_request_id = Some(1_007);
    app.shell.indexing.in_progress = true;
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Loading);
    let _observer_guard = lock_reclaim_drop_observer_for_test();
    let (drop_tx, drop_rx) = mpsc::channel();
    set_reclaim_drop_observer(Some(drop_tx));

    response_tx
        .send(IndexResponse::Failed {
            request_id: 1_007,
            error: "fixture failure".to_string(),
        })
        .expect("send failure");
    app.poll_index_response();

    set_reclaim_drop_observer(None);
    assert!(!app.shell.indexing.build_reclaim_pending);
    assert_eq!(app.shell.indexing.build.index.entries.capacity(), 0);
    let drop_threads = drop_rx
        .try_iter()
        .chain(drop_rx.recv_timeout(Duration::from_millis(250)))
        .collect::<Vec<_>>();
    assert!(
        drop_threads
            .iter()
            .any(|name| name == "flistwalker-tab-reclaimer"),
        "drop threads: {drop_threads:?}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_active_cancel_keeps_fixed_build_debt_until_reclaimer_accepts() {
    let root = test_root("tc-207-active-cancel-reclaim-full");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (response_tx, response_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = response_rx;
    app.shell.indexing.build.index.entries = (0..2_000)
        .map(|index| file_entry(root.join(format!("building-{index}.txt"))))
        .collect();
    app.shell.indexing.pending_request_id = Some(1_107);
    app.shell.indexing.in_progress = true;
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Loading);
    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut tab = app.capture_active_tab_state(2_800 + index as u64);
        tab.result_state.committed.all_entries =
            Arc::new(vec![file_entry(root.join(format!("held-{index}.txt")))]);
        app.shell
            .tabs
            .retire_tab_resources_for_test(tab.take_heavy_resources())
            .expect("fill reclaimer");
    }

    response_tx
        .send(IndexResponse::Canceled { request_id: 1_107 })
        .expect("send cancel");
    app.poll_index_response();

    assert!(app.shell.indexing.build_reclaim_pending);
    assert_eq!(app.shell.indexing.build.index.entries.len(), 2_000);

    app.shell.tabs.resume_resource_reclaimer();
    app.poll_index_response();

    assert!(!app.shell.indexing.build_reclaim_pending);
    assert_eq!(app.shell.indexing.build.index.entries.capacity(), 0);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_active_replace_all_waits_for_reclaimer_without_dropping_old_build() {
    let root = test_root("tc-207-active-replace-all-full");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (response_tx, response_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = response_rx;
    app.shell.indexing.pending_request_id = Some(1_207);
    app.shell.indexing.in_progress = true;
    app.shell.indexing.build.index.entries = (0..2_000)
        .map(|index| file_entry(root.join(format!("old-{index}.txt"))))
        .collect();
    app.shell
        .indexing
        .build
        .pending_entries
        .push_back(IndexEntry {
            path: root.join("old-pending.txt"),
            kind: EntryKind::file(),
            kind_known: true,
        });
    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut tab = app.capture_active_tab_state(3_000 + index as u64);
        tab.result_state.committed.all_entries =
            Arc::new(vec![file_entry(root.join(format!("held-{index}.txt")))]);
        app.shell
            .tabs
            .retire_tab_resources_for_test(tab.take_heavy_resources())
            .expect("fill reclaimer");
    }
    response_tx
        .send(IndexResponse::ReplaceAll {
            request_id: 1_207,
            entries: vec![IndexEntry {
                path: root.join("replacement.txt"),
                kind: EntryKind::file(),
                kind_known: true,
            }],
        })
        .expect("send replacement");

    app.poll_index_response();

    assert!(app.shell.indexing.pending_replace_all.is_some());
    assert_eq!(app.shell.indexing.build.index.entries.len(), 2_000);
    assert_eq!(app.shell.indexing.build.pending_entries.len(), 1);

    app.shell.tabs.resume_resource_reclaimer();
    app.poll_index_response();

    assert!(app.shell.indexing.pending_replace_all.is_none());
    assert_eq!(app.shell.indexing.build.index.entries.len(), 1);
    assert_eq!(
        app.shell.indexing.build.index.entries[0].path,
        root.join("replacement.txt")
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_active_failure_debt_switches_to_background_and_cleans_routing() {
    let root = test_root("tc-207-active-debt-switch-background");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    let failing_tab_id = app.current_tab_id().expect("failing tab");
    let request_id = app.shell.indexing.allocate_request_id(Some(failing_tab_id));
    app.shell.indexing.pending_request_id = Some(request_id);
    app.shell.indexing.in_progress = true;
    app.shell.indexing.build.index.entries = (0..2_000)
        .map(|index| file_entry(root.join(format!("building-{index}.txt"))))
        .collect();
    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut tab = app.capture_active_tab_state(3_100 + index as u64);
        tab.result_state.committed.all_entries =
            Arc::new(vec![file_entry(root.join(format!("held-{index}.txt")))]);
        app.shell
            .tabs
            .retire_tab_resources_for_test(tab.take_heavy_resources())
            .expect("fill reclaimer");
    }
    let (response_tx, response_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = response_rx;
    response_tx
        .send(IndexResponse::Failed {
            request_id,
            error: "fixture failure".to_string(),
        })
        .expect("send failure");
    app.poll_index_response();
    assert!(app.shell.indexing.build_reclaim_pending);

    app.switch_to_tab_index(0);
    let failing_index = app
        .find_tab_index_by_id(failing_tab_id)
        .expect("failing tab remains open");
    assert!(
        app.shell
            .tabs
            .get(failing_index)
            .unwrap()
            .index_state
            .build_reclaim_pending
    );

    app.shell.tabs.resume_resource_reclaimer();
    app.poll_index_response();

    let failing = app.shell.tabs.get(failing_index).expect("failing tab");
    assert!(!failing.index_state.build_reclaim_pending);
    assert!(failing.index_state.build_reclaim_request_id.is_none());
    assert!(!app.shell.indexing.request_tabs.contains_key(&request_id));
    assert!(!app
        .shell
        .indexing
        .response_mailboxes
        .lock()
        .expect("mailboxes")
        .contains_key(&request_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_204_refresh_failure_keeps_last_good_snapshot_and_sets_explicit_lifecycle() {
    let root = test_root("tc-204-last-good-refresh-failure");
    fs::create_dir_all(&root).expect("create root");
    let kept = file_entry(root.join("kept.txt"));
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (request_tx, _request_rx) = bounded_request_channel::<IndexRequest>(2);
    let (response_tx, response_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.tx = request_tx;
    app.shell.indexing.rx = response_rx;
    app.shell.runtime.all_entries = Arc::new(vec![kept.clone()]);
    app.shell.runtime.entries = Arc::new(vec![kept.clone()]);
    app.shell.runtime.results = vec![(kept.path.clone(), 0.0)];
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    app.shell
        .indexing
        .set_committed_snapshot_present_for_test(true);

    app.request_index_refresh();
    let request_id = app
        .shell
        .indexing
        .pending_request_id
        .expect("refresh request");
    assert_eq!(
        app.shell.indexing.lifecycle(),
        TabResourceLifecycle::Refreshing
    );
    assert_eq!(
        app.shell.runtime.all_entries.as_ref(),
        std::slice::from_ref(&kept)
    );

    response_tx
        .send(IndexResponse::Failed {
            request_id,
            error: "read failed".to_string(),
        })
        .expect("send failure");
    app.poll_index_response();

    assert_eq!(app.shell.indexing.lifecycle(), TabResourceLifecycle::Failed);
    assert_eq!(
        app.shell.runtime.all_entries.as_ref(),
        std::slice::from_ref(&kept)
    );
    assert_eq!(app.shell.runtime.results, vec![(kept.path, 0.0)]);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_terminal_commit_waits_when_reclaimer_is_full() {
    let root = test_root("tc-207-terminal-reclaimer-full");
    fs::create_dir_all(&root).expect("create root");
    let old = file_entry(root.join("old.txt"));
    let new = file_entry(root.join("new.txt"));
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (response_tx, response_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = response_rx;
    app.shell.runtime.all_entries = Arc::new(vec![old.clone()]);
    app.shell.runtime.entries = Arc::new(vec![old.clone()]);
    app.shell.runtime.results = vec![(old.path.clone(), 0.0)];
    app.shell.indexing.build.index.entries = vec![new.clone()];
    app.shell.indexing.pending_request_id = Some(207);
    app.shell.indexing.in_progress = true;
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Refreshing);
    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut tab = app.capture_active_tab_state(800 + index as u64);
        tab.result_state.committed.all_entries =
            Arc::new(vec![file_entry(root.join(format!("held-{index}.txt")))]);
        app.shell
            .tabs
            .retire_tab_resources_for_test(tab.take_heavy_resources())
            .expect("fill reclaimer");
    }
    response_tx
        .send(IndexResponse::Finished {
            request_id: 207,
            source: IndexSource::Walker,
        })
        .expect("send finished");

    app.poll_index_response();

    assert!(app.shell.indexing.pending_finish.is_some());
    assert_eq!(
        app.shell.runtime.all_entries.as_ref(),
        std::slice::from_ref(&old)
    );
    assert_eq!(app.shell.runtime.results, vec![(old.path.clone(), 0.0)]);
    assert!(app
        .status_line_text()
        .contains("Waiting for background tab resource reclamation"));

    app.shell.tabs.resume_resource_reclaimer();
    app.poll_index_response();
    assert!(app.shell.indexing.pending_finish.is_none());
    assert_eq!(app.shell.runtime.all_entries.as_ref(), &[new]);
    assert_eq!(app.shell.indexing.lifecycle(), TabResourceLifecycle::Ready);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_terminal_reclaimer_debt_coalesces_repeated_refresh_to_one_generation() {
    let root = test_root("tc-207-terminal-refresh-coalescing");
    fs::create_dir_all(&root).expect("create root");
    let old = file_entry(root.join("old.txt"));
    let new = file_entry(root.join("new.txt"));
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (request_tx, request_rx) = bounded_request_channel::<IndexRequest>(2);
    let (response_tx, response_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.tx = request_tx;
    app.shell.indexing.rx = response_rx;
    app.shell.indexing.pending_queue.clear();
    app.shell.indexing.inflight_requests.clear();
    app.shell.indexing.request_tabs.clear();
    app.shell.runtime.all_entries = Arc::new(vec![old.clone()]);
    app.shell.runtime.entries = Arc::new(vec![old]);
    app.shell
        .indexing
        .set_committed_snapshot_present_for_test(true);
    app.shell.indexing.build.index.entries = vec![new];
    app.shell.indexing.pending_request_id = Some(207);
    app.shell.indexing.in_progress = true;
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Refreshing);
    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut tab = app.capture_active_tab_state(900 + index as u64);
        tab.result_state.committed.all_entries =
            Arc::new(vec![file_entry(root.join(format!("held-{index}.txt")))]);
        tab.index_state
            .set_committed_snapshot_present_for_test(true);
        app.shell
            .tabs
            .retire_tab_resources_for_test(tab.take_heavy_resources())
            .expect("fill reclaimer");
    }
    response_tx
        .send(IndexResponse::Finished {
            request_id: 207,
            source: IndexSource::Walker,
        })
        .expect("send finished");
    app.poll_index_response();
    let next_before = app.shell.indexing.next_request_id;

    app.request_index_refresh();
    app.request_index_refresh();

    assert_eq!(app.shell.indexing.next_request_id, next_before);
    assert_eq!(app.shell.indexing.pending_request_id, Some(207));
    assert!(app.shell.indexing.pending_finish.is_some());
    assert_eq!(app.shell.indexing.build.index.entries.len(), 1);
    assert!(request_rx.try_recv().is_err());

    app.shell.tabs.resume_resource_reclaimer();
    app.poll_index_response();

    assert_eq!(app.shell.indexing.next_request_id, next_before + 1);
    let replay = request_rx.try_recv().expect("one coalesced refresh");
    assert_eq!(
        Some(replay.request_id),
        app.shell.indexing.pending_request_id
    );
    assert!(request_rx.try_recv().is_err());
    assert_eq!(
        app.shell.indexing.lifecycle(),
        TabResourceLifecycle::Refreshing
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_terminal_reclaimer_debt_preserves_create_filelist_mode_for_latest_root() {
    let root_a = test_root("tc-207-terminal-root-a");
    let root_b = test_root("tc-207-terminal-root-b");
    let root_c = test_root("tc-207-terminal-root-c");
    for root in [&root_a, &root_b, &root_c] {
        fs::create_dir_all(root).expect("create root");
    }
    let old = file_entry(root_a.join("old.txt"));
    let new = file_entry(root_a.join("new.txt"));
    let mut app = FlistWalkerApp::new(root_a.clone(), 50, String::new());
    let (request_tx, request_rx) = bounded_request_channel::<IndexRequest>(2);
    let (response_tx, response_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.tx = request_tx;
    app.shell.indexing.rx = response_rx;
    app.shell.indexing.pending_queue.clear();
    app.shell.indexing.inflight_requests.clear();
    app.shell.indexing.request_tabs.clear();
    app.shell.runtime.all_entries = Arc::new(vec![old.clone()]);
    app.shell.runtime.entries = Arc::new(vec![old]);
    app.shell
        .indexing
        .set_committed_snapshot_present_for_test(true);
    app.shell.indexing.build.index.entries = vec![new];
    app.shell.indexing.pending_request_id = Some(307);
    app.shell.indexing.in_progress = true;
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Refreshing);
    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut tab = app.capture_active_tab_state(1_300 + index as u64);
        tab.result_state.committed.all_entries =
            Arc::new(vec![file_entry(root_a.join(format!("held-{index}.txt")))]);
        tab.index_state
            .set_committed_snapshot_present_for_test(true);
        app.shell
            .tabs
            .retire_tab_resources_for_test(tab.take_heavy_resources())
            .expect("fill reclaimer");
    }
    response_tx
        .send(IndexResponse::Finished {
            request_id: 307,
            source: IndexSource::Walker,
        })
        .expect("send finished");
    app.poll_index_response();

    app.apply_root_change_direct(root_b.clone());
    app.apply_root_change_direct(root_c.clone());
    app.shell.runtime.use_filelist = true;
    app.shell.runtime.max_depth = crate::indexer::MaxDepth::limited(3).expect("valid max depth");
    app.shell.indexing.refresh_after_pending_finish =
        Some(super::PendingIndexRefreshMode::CreateFileListWalker);

    assert_eq!(path_key(&app.shell.runtime.root), path_key(&root_a));
    assert_eq!(
        app.shell
            .indexing
            .root_after_pending_finish
            .as_ref()
            .map(|root| path_key(root)),
        Some(path_key(&root_c))
    );
    assert!(request_rx.try_recv().is_err());

    app.shell.tabs.resume_resource_reclaimer();
    app.poll_index_response();

    assert_eq!(path_key(&app.shell.runtime.root), path_key(&root_c));
    let replay = request_rx.try_recv().expect("one latest-root refresh");
    assert_eq!(path_key(&replay.root), path_key(&root_c));
    assert!(!replay.use_filelist);
    assert!(replay.max_depth.is_unlimited());
    assert!(request_rx.try_recv().is_err());
    for root in [&root_a, &root_b, &root_c] {
        let _ = fs::remove_dir_all(root);
    }
}

#[test]
fn tc_207_create_filelist_terminal_root_survives_switch_and_replays_on_original_tab() {
    let root_a = test_root("tc-207-switched-terminal-root-a");
    let root_b = test_root("tc-207-switched-terminal-root-b");
    for root in [&root_a, &root_b] {
        fs::create_dir_all(root).expect("create root");
    }
    let old = file_entry(root_a.join("old.txt"));
    let new = file_entry(root_a.join("new.txt"));
    let mut app = FlistWalkerApp::new(root_a.clone(), 50, String::new());
    app.create_new_tab();
    app.switch_to_tab_index(0);
    let original_tab_id = app.current_tab_id().expect("original tab");
    let (request_tx, request_rx) = bounded_request_channel::<IndexRequest>(2);
    let (response_tx, response_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.tx = request_tx;
    app.shell.indexing.rx = response_rx;
    app.shell.indexing.pending_queue.clear();
    app.shell.indexing.inflight_requests.clear();
    app.shell.indexing.request_tabs.clear();
    app.shell.runtime.all_entries = Arc::new(vec![old.clone()]);
    app.shell.runtime.entries = Arc::new(vec![old]);
    app.shell.indexing.build.index.entries = vec![new];
    app.shell
        .indexing
        .set_committed_snapshot_present_for_test(true);
    app.shell.indexing.pending_request_id = Some(507);
    app.shell.indexing.in_progress = true;
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Refreshing);
    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut tab = app.capture_active_tab_state(1_600 + index as u64);
        tab.result_state.committed.all_entries =
            Arc::new(vec![file_entry(root_a.join(format!("held-{index}.txt")))]);
        app.shell
            .tabs
            .retire_tab_resources_for_test(tab.take_heavy_resources())
            .expect("fill reclaimer");
    }
    response_tx
        .send(IndexResponse::Finished {
            request_id: 507,
            source: IndexSource::Walker,
        })
        .expect("send finished");
    app.poll_index_response();
    app.apply_root_change_direct(root_b.clone());
    app.shell.runtime.use_filelist = true;
    app.shell.runtime.max_depth = crate::indexer::MaxDepth::limited(4).expect("valid max depth");
    app.shell.indexing.refresh_after_pending_finish =
        Some(super::PendingIndexRefreshMode::CreateFileListWalker);
    {
        let other = app.shell.tabs.get_mut(1).expect("other tab");
        other
            .index_state
            .set_lifecycle_for_test(TabResourceLifecycle::Ready);
        other
            .index_state
            .set_committed_snapshot_present_for_test(true);
    }
    app.switch_to_tab_index(1);

    app.shell.tabs.resume_resource_reclaimer();
    app.poll_index_response();

    let original = app.shell.tabs.get(0).expect("original background tab");
    assert_eq!(original.id, original_tab_id);
    assert_eq!(path_key(&original.root), path_key(&root_b));
    assert!(!original.index_state.committed_snapshot_present());
    assert!(original.index_state.pending_index_finish.is_none());
    let replay = request_rx.try_recv().expect("one background root replay");
    assert_eq!(replay.tab_id, original_tab_id);
    assert_eq!(path_key(&replay.root), path_key(&root_b));
    assert!(!replay.use_filelist);
    assert!(replay.max_depth.is_unlimited());
    assert!(request_rx.try_recv().is_err());
    for root in [&root_a, &root_b] {
        let _ = fs::remove_dir_all(root);
    }
}

#[test]
fn tc_207_terminal_root_close_is_atomic_and_restore_starts_one_target_request() {
    let root_a = test_root("tc-207-terminal-close-a");
    let root_b = test_root("tc-207-terminal-close-b");
    for root in [&root_a, &root_b] {
        fs::create_dir_all(root).expect("create root");
    }
    let old = file_entry(root_a.join("old.txt"));
    let new = file_entry(root_a.join("new.txt"));
    let mut app = FlistWalkerApp::new(root_a.clone(), 50, String::new());
    app.create_new_tab();
    {
        let other = app.shell.tabs.get_mut(0).expect("other tab");
        other
            .index_state
            .set_lifecycle_for_test(TabResourceLifecycle::Ready);
        other
            .index_state
            .set_committed_snapshot_present_for_test(true);
    }
    let (request_tx, request_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = request_tx;
    app.shell.indexing.pending_queue.clear();
    app.shell.indexing.inflight_requests.clear();
    app.shell.indexing.request_tabs.clear();
    app.shell.runtime.all_entries = Arc::new(vec![old.clone()]);
    app.shell.runtime.entries = Arc::new(vec![old.clone()]);
    app.shell.runtime.results = vec![(old.path.clone(), 0.0)];
    app.shell.indexing.build.index.entries = vec![new];
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Refreshing);
    app.shell
        .indexing
        .set_committed_snapshot_present_for_test(true);
    app.shell.indexing.pending_request_id = Some(807);
    app.shell.indexing.pending_finish = Some(PendingActiveIndexFinish {
        request_id: 807,
        source: IndexSource::Walker,
    });
    app.shell.indexing.root_after_pending_finish = Some(root_b.clone());
    app.shell.indexing.refresh_after_pending_finish = Some(super::PendingIndexRefreshMode::Normal);
    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut tab = app.capture_active_tab_state(1_900 + index as u64);
        tab.result_state.committed.all_entries =
            Arc::new(vec![file_entry(root_a.join(format!("held-{index}.txt")))]);
        app.shell
            .tabs
            .retire_tab_resources_for_test(tab.take_heavy_resources())
            .expect("fill reclaimer");
    }

    app.close_active_tab();

    assert_eq!(app.shell.tabs.len(), 2);
    assert_eq!(path_key(&app.shell.runtime.root), path_key(&root_a));
    assert_eq!(app.shell.runtime.all_entries.as_ref(), &[old]);
    assert!(app.shell.indexing.pending_finish.is_some());
    assert!(request_rx.try_recv().is_err());

    app.shell.tabs.resume_resource_reclaimer();
    app.close_active_tab();
    assert_eq!(app.shell.tabs.len(), 1);
    app.restore_recently_closed_tab();

    assert_eq!(path_key(&app.shell.runtime.root), path_key(&root_b));
    assert!(!app.shell.indexing.committed_snapshot_present());
    assert!(app.shell.runtime.all_entries.is_empty());
    let request = request_rx.try_recv().expect("one restored-root request");
    assert_eq!(path_key(&request.root), path_key(&root_b));
    assert!(request_rx.try_recv().is_err());
    for root in [&root_a, &root_b] {
        let _ = fs::remove_dir_all(root);
    }
}

#[test]
fn tc_207_terminal_root_close_rolls_back_when_history_preflight_consumes_last_slot() {
    let root_a = test_root("tc-207-terminal-close-history-a");
    let root_b = test_root("tc-207-terminal-close-history-b");
    for root in [&root_a, &root_b] {
        fs::create_dir_all(root).expect("create root");
    }
    let mut app = FlistWalkerApp::new(root_a.clone(), 50, String::new());
    for index in 0..25 {
        app.create_new_tab();
        app.shell.runtime.query_state.query = format!("closed-{index}");
        app.close_active_tab();
    }
    assert_eq!(app.shell.tabs.closed_tab_count(), 25);
    app.shell
        .tabs
        .seed_oldest_closed_snapshot(file_entry(root_a.join("oldest-heavy.txt")));
    app.create_new_tab();
    let old = file_entry(root_a.join("old.txt"));
    let new = file_entry(root_a.join("new.txt"));
    app.shell.runtime.all_entries = Arc::new(vec![old.clone()]);
    app.shell.runtime.entries = Arc::new(vec![old.clone()]);
    app.shell.runtime.results = vec![(old.path.clone(), 0.0)];
    app.shell.indexing.build.index.entries = vec![new];
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Refreshing);
    app.shell
        .indexing
        .set_committed_snapshot_present_for_test(true);
    app.shell.indexing.pending_request_id = Some(907);
    app.shell.indexing.pending_finish = Some(PendingActiveIndexFinish {
        request_id: 907,
        source: IndexSource::Walker,
    });
    app.shell.indexing.root_after_pending_finish = Some(root_b.clone());
    app.shell.indexing.refresh_after_pending_finish = Some(super::PendingIndexRefreshMode::Normal);
    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY - 1 {
        let mut tab = app.capture_active_tab_state(2_100 + index as u64);
        tab.result_state.committed.all_entries =
            Arc::new(vec![file_entry(root_a.join(format!("held-{index}.txt")))]);
        app.shell
            .tabs
            .retire_tab_resources_for_test(tab.take_heavy_resources())
            .expect("reserve all but one reclaimer slot");
    }

    app.close_active_tab();

    assert_eq!(app.shell.tabs.len(), 2);
    assert_eq!(app.shell.tabs.closed_tab_count(), 25);
    assert_eq!(path_key(&app.shell.runtime.root), path_key(&root_a));
    assert_eq!(app.shell.runtime.all_entries.as_ref(), &[old]);
    assert!(app.shell.indexing.pending_finish.is_some());
    assert_eq!(
        app.shell.indexing.root_after_pending_finish.as_ref(),
        Some(&root_b)
    );
    assert!(app
        .shell
        .runtime
        .notice
        .contains("Waiting for background tab resource reclamation"));
    for root in [&root_a, &root_b] {
        let _ = fs::remove_dir_all(root);
    }
}

#[test]
fn tc_207_rootless_terminal_close_is_noop_before_closed_history_preflight() {
    let root = test_root("tc-207-rootless-terminal-close");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    for index in 0..25 {
        app.create_new_tab();
        app.shell.runtime.query_state.query = format!("closed-{index}");
        app.close_active_tab();
    }
    app.shell
        .tabs
        .seed_oldest_closed_snapshot(file_entry(root.join("oldest-heavy.txt")));
    app.create_new_tab();
    app.shell.indexing.pending_request_id = Some(1_107);
    app.shell.indexing.pending_finish = Some(PendingActiveIndexFinish {
        request_id: 1_107,
        source: IndexSource::Walker,
    });
    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY - 1 {
        let mut tab = app.capture_active_tab_state(2_200 + index as u64);
        tab.result_state.committed.all_entries =
            Arc::new(vec![file_entry(root.join(format!("held-{index}.txt")))]);
        app.shell
            .tabs
            .retire_tab_resources_for_test(tab.take_heavy_resources())
            .expect("reserve all but one reclaimer slot");
    }
    let pending_before = app.shell.tabs.reclaimer_pending();

    app.close_active_tab();

    assert_eq!(app.shell.tabs.len(), 2);
    assert_eq!(app.shell.tabs.closed_tab_count(), 25);
    assert_eq!(app.shell.tabs.reclaimer_pending(), pending_before);
    assert!(app.shell.indexing.pending_finish.is_some());
    assert!(app.shell.indexing.root_after_pending_finish.is_none());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn filelist_failed_updates_state_and_notice() {
    let root = test_root("filelist-failed");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<FileListResponse>();
    app.shell.worker_bus.filelist.rx = rx;
    app.shell.features.filelist.workflow.pending_request_id = Some(13);
    app.shell.features.filelist.workflow.pending_request_tab_id = app.current_tab_id();
    app.shell.features.filelist.workflow.pending_root = Some(root.clone());
    app.shell.features.filelist.workflow.in_progress = true;

    tx.send(FileListResponse::Failed {
        request_id: 13,
        root: root.clone(),
        error: "disk full".to_string(),
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
    assert!(app
        .shell
        .runtime
        .notice
        .contains("Create File List failed: disk full"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn filelist_canceled_updates_state_and_notice() {
    let root = test_root("filelist-canceled");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<FileListResponse>();
    app.shell.worker_bus.filelist.rx = rx;
    app.shell.features.filelist.workflow.pending_request_id = Some(14);
    app.shell.features.filelist.workflow.pending_request_tab_id = app.current_tab_id();
    app.shell.features.filelist.workflow.pending_root = Some(root.clone());
    app.shell.features.filelist.workflow.pending_cancel = Some(Arc::new(AtomicBool::new(true)));
    app.shell.features.filelist.workflow.in_progress = true;
    app.shell.features.filelist.workflow.cancel_requested = true;

    tx.send(FileListResponse::Canceled {
        request_id: 14,
        root: root.clone(),
    })
    .expect("send filelist response");

    app.poll_filelist_response();

    assert_eq!(
        app.shell.features.filelist.workflow.pending_request_id,
        None
    );
    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_cancel
        .is_none());
    assert!(!app.shell.features.filelist.workflow.in_progress);
    assert!(!app.shell.features.filelist.workflow.cancel_requested);
    assert!(app
        .shell
        .runtime
        .notice
        .contains("Create File List canceled"));
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
    app.shell.worker_bus.filelist.rx = filelist_rx;
    let (_index_tx, index_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = _index_tx;
    app.shell.features.filelist.workflow.pending_request_id = Some(51);
    app.shell.features.filelist.workflow.pending_request_tab_id = app.current_tab_id();
    app.shell.features.filelist.workflow.pending_root = Some(root_old.clone());
    app.shell.features.filelist.workflow.in_progress = true;
    app.shell.runtime.use_filelist = true;
    app.shell.runtime.root = root_new.clone();

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
    assert!(!app.shell.features.filelist.workflow.in_progress);
    assert!(app.shell.runtime.notice.contains("previous root"));
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
    app.shell.worker_bus.filelist.rx = rx;
    app.shell.features.filelist.workflow.pending_request_id = Some(52);
    app.shell.features.filelist.workflow.pending_request_tab_id = app.current_tab_id();
    app.shell.features.filelist.workflow.pending_root = Some(root_old.clone());
    app.shell.features.filelist.workflow.in_progress = true;
    app.shell.runtime.root = root_new;

    tx.send(FileListResponse::Failed {
        request_id: 52,
        root: root_old.clone(),
        error: "permission denied".to_string(),
    })
    .expect("send filelist response");

    app.poll_filelist_response();

    assert_eq!(
        app.shell.features.filelist.workflow.pending_request_id,
        None
    );
    assert!(!app.shell.features.filelist.workflow.in_progress);
    assert!(app.shell.runtime.notice.contains("previous root"));
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
    app.shell.worker_bus.filelist.rx = filelist_rx;
    app.shell.features.filelist.workflow.pending_request_id = Some(53);
    app.shell.features.filelist.workflow.pending_request_tab_id = app.current_tab_id();
    app.shell.features.filelist.workflow.pending_root = Some(root_requested.clone());
    app.shell.features.filelist.workflow.in_progress = true;
    app.shell.runtime.use_filelist = false;

    filelist_tx
        .send(FileListResponse::Finished {
            request_id: 53,
            root: root_response.clone(),
            path: root_response.join("FileList.txt"),
            count: 4,
        })
        .expect("send filelist response");

    app.poll_filelist_response();

    assert_eq!(
        app.shell.features.filelist.workflow.pending_request_id,
        None
    );
    assert!(!app.shell.features.filelist.workflow.in_progress);
    assert!(!app.shell.runtime.use_filelist);
    assert!(app.shell.runtime.notice.is_empty());
    let _ = fs::remove_dir_all(&root_requested);
    let _ = fs::remove_dir_all(&root_response);
}

#[test]
fn non_empty_query_incremental_refresh_skips_small_delta_during_indexing() {
    let root = test_root("incremental-small-delta-skip");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "main".to_string());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = rx;
    app.shell.runtime.entries = Arc::new(Vec::new());
    app.shell.runtime.all_entries = Arc::new(Vec::new());
    app.shell.indexing.build.index.entries.clear();
    app.shell
        .indexing
        .build
        .incremental_filtered_entries
        .clear();
    app.shell.indexing.search_resume_pending = false;
    app.shell.indexing.last_search_snapshot_len = 0;
    app.shell.search.set_in_progress(false);
    app.shell.search.set_pending_request_id(None);
    app.shell.indexing.pending_request_id = Some(21);
    app.shell.indexing.in_progress = true;
    app.shell.indexing.last_incremental_results_refresh = Instant::now() - Duration::from_secs(3);

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

    assert!(app.shell.runtime.entries.is_empty());
    assert_eq!(
        app.shell.indexing.build.incremental_filtered_entries,
        vec![file_entry(path)]
    );
    assert!(!app.shell.indexing.search_rerun_pending);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn walker_truncated_notice_points_to_config_file_setting() {
    let root = test_root("walker-truncated-config-notice");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = rx;
    app.shell.indexing.pending_request_id = Some(91);
    app.shell.indexing.in_progress = true;

    tx.send(IndexResponse::Truncated {
        request_id: 91,
        limit: 500_000,
    })
    .expect("send truncated response");

    app.poll_index_response();

    assert_eq!(
        app.shell.runtime.notice,
        "Walker capped at 500000 entries (set walker_max_entries in the config file to adjust)"
    );
    assert!(!app
        .shell
        .runtime
        .notice
        .contains("FLISTWALKER_WALKER_MAX_ENTRIES"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn non_empty_query_incremental_refresh_updates_entries_with_large_delta() {
    let root = test_root("incremental-large-delta");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "main".to_string());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = rx;
    app.shell.runtime.entries = Arc::new(Vec::new());
    app.shell.runtime.all_entries = Arc::new(Vec::new());
    app.shell.indexing.build.index.entries.clear();
    app.shell
        .indexing
        .build
        .incremental_filtered_entries
        .clear();
    app.shell.indexing.search_resume_pending = false;
    app.shell.indexing.last_search_snapshot_len = 0;
    app.shell.search.set_in_progress(false);
    app.shell.search.set_pending_request_id(None);
    app.shell.indexing.pending_request_id = Some(218);
    app.shell.indexing.in_progress = true;
    app.shell.indexing.last_incremental_results_refresh = Instant::now() - Duration::from_secs(3);

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
        app.shell.indexing.last_incremental_results_refresh =
            Instant::now() - Duration::from_secs(3);
        app.poll_index_response();
        if app.shell.runtime.entries.len()
            >= FlistWalkerApp::INCREMENTAL_SEARCH_MIN_DELTA_DURING_INDEX
        {
            break;
        }
    }

    assert_eq!(
        app.shell.runtime.entries.len(),
        FlistWalkerApp::INCREMENTAL_SEARCH_MIN_DELTA_DURING_INDEX
    );
    assert_eq!(
        app.shell.indexing.build.incremental_filtered_entries.len(),
        app.shell.runtime.entries.len()
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn non_empty_query_batch_delta_updates_snapshot_even_without_search_refresh() {
    let root = test_root("incremental-snapshot-delta");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "main".to_string());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = rx;
    app.shell.indexing.pending_request_id = Some(88);
    app.shell.indexing.in_progress = true;
    app.shell.indexing.search_resume_pending = false;
    app.shell.indexing.last_incremental_results_refresh = Instant::now();
    app.shell.indexing.last_search_snapshot_len = 0;

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

    assert!(app.shell.runtime.entries.is_empty());
    assert_eq!(
        app.shell.indexing.build.incremental_filtered_entries.len(),
        2
    );
    assert_eq!(
        app.shell.indexing.build.incremental_filtered_entries[0],
        path_a
    );
    assert_eq!(
        app.shell.indexing.build.incremental_filtered_entries[1],
        path_b
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn empty_query_keeps_results_after_batch_and_finished_in_same_poll() {
    let root = test_root("empty-query-finished-priority");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.total_match_count = 99;
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = rx;
    app.shell.indexing.pending_request_id = Some(31);
    app.shell.indexing.in_progress = true;

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

    assert_eq!(app.shell.runtime.entries.len(), 1);
    assert_eq!(app.shell.runtime.results.len(), 1);
    assert_eq!(app.shell.runtime.total_match_count, 1);
    assert_eq!(app.shell.runtime.entries[0], path);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_evicted_selection_survives_partial_empty_query_miss_and_restores_later_regression() {
    let root = test_root("evicted-selection-partial-empty-query");
    fs::create_dir_all(&root).expect("create dir");
    let first = root.join("first.txt");
    let selected = root.join("selected.txt");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = rx;
    app.shell.indexing.pending_request_id = Some(32);
    app.shell.indexing.in_progress = true;
    app.shell.runtime.evicted_selected_path = Some(selected.clone());

    tx.send(IndexResponse::Batch {
        request_id: 32,
        entries: vec![IndexEntry {
            path: first,
            kind: EntryKind::file(),
            kind_known: true,
        }],
    })
    .expect("send partial index batch");
    app.poll_index_response();

    assert_eq!(
        app.shell.runtime.evicted_selected_path.as_ref(),
        Some(&selected),
        "a partial miss must retain the restore intent"
    );

    tx.send(IndexResponse::Batch {
        request_id: 32,
        entries: vec![IndexEntry {
            path: selected.clone(),
            kind: EntryKind::file(),
            kind_known: true,
        }],
    })
    .expect("send selected path in later batch");
    app.poll_index_response();

    assert_eq!(
        app.shell.runtime.current_row.and_then(|row| app
            .shell
            .runtime
            .results
            .get(row)
            .map(|(path, _)| path)),
        Some(&selected)
    );
    assert!(app.shell.runtime.evicted_selected_path.is_none());

    tx.send(IndexResponse::Finished {
        request_id: 32,
        source: IndexSource::Walker,
    })
    .expect("send index finished");
    app.poll_index_response();

    assert!(!app.shell.indexing.in_progress);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_empty_query_terminal_absence_clears_evicted_selection_intent_regression() {
    let root = test_root("evicted-selection-empty-query-absence");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = rx;
    app.shell.indexing.pending_request_id = Some(33);
    app.shell.indexing.in_progress = true;
    app.shell.runtime.evicted_selected_path = Some(root.join("selected.txt"));

    tx.send(IndexResponse::Batch {
        request_id: 33,
        entries: vec![IndexEntry {
            path: root.join("other.txt"),
            kind: EntryKind::file(),
            kind_known: true,
        }],
    })
    .expect("send partial index batch");
    app.poll_index_response();
    assert!(app.shell.runtime.evicted_selected_path.is_some());

    tx.send(IndexResponse::Finished {
        request_id: 33,
        source: IndexSource::Walker,
    })
    .expect("send index finished");
    app.poll_index_response();

    assert!(!app.shell.indexing.in_progress);
    assert!(app.shell.runtime.evicted_selected_path.is_none());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn status_line_prefers_current_index_count_while_indexing() {
    let root = test_root("status-line-current-index-count");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.indexing.in_progress = true;
    app.shell.runtime.all_entries = Arc::new(
        (0..10)
            .map(|i| unknown_entry(root.join(format!("old-{i}.txt"))))
            .collect::<Vec<_>>(),
    );
    app.shell.indexing.build.index.entries = (0..3)
        .map(|i| unknown_entry(root.join(format!("new-{i}.txt"))))
        .collect::<Vec<_>>();

    app.refresh_status_line();

    assert_eq!(entries_count_from_status(&app.shell.runtime.status_line), 3);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn status_line_counts_pending_index_entries_while_indexing() {
    let root = test_root("status-line-pending-index-count");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.indexing.in_progress = true;
    app.shell.runtime.all_entries = Arc::new(
        (0..10)
            .map(|i| unknown_entry(root.join(format!("old-{i}.txt"))))
            .collect::<Vec<_>>(),
    );
    app.shell.indexing.build.index.entries = (0..3)
        .map(|i| unknown_entry(root.join(format!("new-{i}.txt"))))
        .collect::<Vec<_>>();
    app.shell.indexing.build.pending_entries = (0..4)
        .map(|i| IndexEntry {
            path: root.join(format!("pending-{i}.txt")),
            kind: EntryKind::file(),
            kind_known: true,
        })
        .collect::<VecDeque<_>>();

    app.refresh_status_line();

    assert_eq!(entries_count_from_status(&app.shell.runtime.status_line), 7);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn request_index_refresh_keeps_existing_entries_visible_until_new_results_arrive() {
    let root = test_root("refresh-keeps-visible");
    fs::create_dir_all(&root).expect("create dir");
    let path = root.join("keep.txt");
    fs::write(&path, "x").expect("write file");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, _rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = tx;
    app.shell.runtime.entries = Arc::new(vec![unknown_entry(path.clone())]);
    app.shell.runtime.results = vec![(path.clone(), 0.0)];
    app.shell.runtime.current_row = Some(0);
    app.shell.runtime.preview = "keep".to_string();

    app.request_index_refresh();

    assert_eq!(app.shell.runtime.entries.len(), 1);
    assert_eq!(app.shell.runtime.results.len(), 1);
    assert_eq!(app.shell.runtime.current_row, Some(0));
    assert_eq!(app.shell.runtime.preview, "keep");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn incremental_empty_query_update_preserves_scroll_position_flag() {
    let root = test_root("incremental-preserve-scroll");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = rx;
    app.shell.indexing.pending_request_id = Some(41);
    app.shell.indexing.in_progress = true;
    app.shell.ui.scroll_to_current = false;
    app.shell.runtime.current_row = Some(0);

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

    assert!(!app.shell.ui.scroll_to_current);
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
    app.shell.indexing.in_progress = true;
    app.shell.indexing.build.index.entries = vec![file_entry(file.clone()), dir_entry(dir.clone())];
    app.set_entry_kind(&file, EntryKind::file());
    app.set_entry_kind(&dir, EntryKind::dir());
    app.shell.runtime.include_files = false;
    app.shell.runtime.include_dirs = true;

    app.apply_entry_filters(true);

    assert_eq!(app.shell.runtime.entries.as_ref(), &vec![dir.clone()]);
    assert_eq!(
        app.shell.indexing.build.incremental_filtered_entries,
        vec![dir_entry(dir)]
    );
    assert!(app.shell.indexing.build.pending_entries.is_empty());
    assert!(app.shell.indexing.pending_entries_request_id.is_none());
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
    app.shell.indexing.in_progress = true;
    app.shell.indexing.build.index.entries = vec![file_entry(file.clone())];
    app.set_entry_kind(&file, EntryKind::file());
    app.shell.runtime.include_files = false;
    app.shell.runtime.include_dirs = true;

    app.apply_entry_filters(true);
    assert!(app.shell.runtime.entries.is_empty());
    assert!(app
        .shell
        .indexing
        .build
        .incremental_filtered_entries
        .is_empty());

    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = rx;
    app.shell.indexing.pending_request_id = Some(201);
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

    assert_eq!(app.shell.runtime.entries.as_ref(), &vec![dir]);
    assert_eq!(app.shell.runtime.results.len(), 1);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn active_indexing_empty_query_without_filters_does_not_clone_full_entries_snapshot() {
    let root = test_root("active-index-no-filter-no-clone");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 2, String::new());
    app.shell.indexing.in_progress = true;
    app.shell.runtime.include_files = true;
    app.shell.runtime.include_dirs = true;
    app.shell.ui.ignore_list_enabled = false;
    app.shell.runtime.entries = Arc::new(Vec::new());
    app.shell.indexing.build.index.entries = (0..5)
        .map(|idx| file_entry(root.join(format!("file-{idx}.txt"))))
        .collect();

    app.apply_entry_filters(true);

    assert!(app.shell.runtime.entries.is_empty());
    assert!(app
        .shell
        .indexing
        .build
        .incremental_filtered_entries
        .is_empty());
    assert_eq!(app.shell.indexing.last_search_snapshot_len, 5);
    assert_eq!(app.shell.runtime.results.len(), 2);
    assert_eq!(app.shell.runtime.results[0].0, root.join("file-0.txt"));
    assert_eq!(app.shell.runtime.results[1].0, root.join("file-1.txt"));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn finished_index_response_drains_pending_entries_over_multiple_frames() {
    let root = test_root("finished-drain-budget");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = rx;
    app.shell.indexing.pending_request_id = Some(301);
    app.shell.indexing.in_progress = true;
    app.shell.indexing.pending_entries_request_id = Some(301);
    app.shell.indexing.build.pending_entries = (0..50_000)
        .map(|index| IndexEntry {
            path: root.join(format!("file-{index}.txt")),
            kind: EntryKind::file(),
            kind_known: true,
        })
        .collect();

    tx.send(IndexResponse::Finished {
        request_id: 301,
        source: IndexSource::Walker,
    })
    .expect("send finished");

    app.poll_index_response();

    assert!(!app.shell.indexing.in_progress);
    assert!(app.shell.indexing.pending_finish.is_some());
    assert!(app.shell.indexing.build.pending_entries.len() < 50_000);
    assert!(app.shell.indexing.build.pending_entries.len() >= 47_952);
    assert!(!app.shell.indexing.build.pending_entries.is_empty());
    assert!(!app.status_line_text().contains("Indexing..."));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn active_index_backlog_stops_receiving_before_queue_growth_exceeds_frame_guard() {
    const BACKLOG_GUARD: usize = 32_768;

    let root = test_root("active-index-backlog-guard");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = rx;
    app.shell.indexing.pending_request_id = Some(311);
    app.shell.indexing.in_progress = true;
    app.shell.indexing.pending_entries_request_id = Some(311);
    app.shell.indexing.build.pending_entries = (0..BACKLOG_GUARD)
        .map(|index| IndexEntry {
            path: root.join(format!("queued-{index}.txt")),
            kind: EntryKind::file(),
            kind_known: true,
        })
        .collect();
    tx.send(IndexResponse::Batch {
        request_id: 311,
        entries: (0..1_024)
            .map(|index| IndexEntry {
                path: root.join(format!("received-{index}.txt")),
                kind: EntryKind::file(),
                kind_known: true,
            })
            .collect(),
    })
    .expect("send queued batch");

    app.poll_index_response_with_budget_for_test(Duration::ZERO);

    assert_eq!(app.shell.indexing.build.index.entries.len(), 32);
    assert_eq!(
        app.shell.indexing.build.pending_entries.len(),
        BACKLOG_GUARD - 32,
        "the UI must drain its existing backlog before accepting another batch"
    );

    app.poll_index_response_with_budget_for_test(Duration::ZERO);

    assert_eq!(app.shell.indexing.build.index.entries.len(), 64);
    assert_eq!(
        app.shell.indexing.build.pending_entries.len(),
        BACKLOG_GUARD - 64 + 1_024
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn pending_finished_index_finalizes_after_budgeted_drain_completes() {
    let root = test_root("finished-drain-complete");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = rx;
    app.shell.indexing.pending_request_id = Some(302);
    app.shell.indexing.in_progress = true;
    app.shell.indexing.pending_entries_request_id = Some(302);
    app.shell.indexing.build.pending_entries = (0..600)
        .map(|index| IndexEntry {
            path: root.join(format!("file-{index}.txt")),
            kind: EntryKind::file(),
            kind_known: true,
        })
        .collect();
    tx.send(IndexResponse::Finished {
        request_id: 302,
        source: IndexSource::Walker,
    })
    .expect("send finished");

    for _ in 0..8 {
        app.poll_index_response();
    }

    assert!(!app.shell.indexing.in_progress);
    assert!(app.shell.indexing.pending_finish.is_none());
    assert!(app.shell.indexing.build.pending_entries.is_empty());
    assert_eq!(app.shell.runtime.all_entries.len(), 600);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn pending_finished_index_finalization_does_not_shrink_drained_queue_regression() {
    let root = test_root("finished-no-shrink-drained-queue");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = rx;
    app.shell.indexing.pending_request_id = Some(305);
    app.shell.indexing.in_progress = true;
    app.shell.indexing.pending_entries_request_id = Some(305);
    app.shell.indexing.build.pending_entries.reserve(20_000);
    app.shell.indexing.build.pending_entries = (0..600)
        .map(|index| IndexEntry {
            path: root.join(format!("file-{index}.txt")),
            kind: EntryKind::file(),
            kind_known: true,
        })
        .collect();
    app.shell.indexing.build.pending_entries.reserve(20_000);
    let capacity_before = app.shell.indexing.build.pending_entries.capacity();
    tx.send(IndexResponse::Finished {
        request_id: 305,
        source: IndexSource::Walker,
    })
    .expect("send finished");

    app.poll_index_response();
    // Simulate the post-Finished per-frame entry cap deterministically. Repeated
    // wall-clock-budget polling is scheduler-sensitive under the parallel suite.
    for _ in 0..3 {
        if app.shell.indexing.pending_finish.is_none() {
            break;
        }
        app.drain_queued_index_entries(303, 2_048);
        app.poll_index_response();
    }

    assert!(capacity_before >= 20_000);
    assert!(app.shell.indexing.build.pending_entries.is_empty());
    assert!(app.shell.indexing.build.pending_entries.capacity() >= capacity_before);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn pending_finished_index_finalization_does_not_sample_memory_when_notice_clears() {
    let root = test_root("finished-no-memory-sample");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let stale = Instant::now()
        .checked_sub(Duration::from_secs(5))
        .unwrap_or_else(Instant::now);
    app.shell.ui.memory_usage_bytes = Some(123);
    app.shell.ui.last_memory_sample = stale;
    app.set_notice(
        "Walker capped at 500000 entries (set walker_max_entries in the config file to adjust)",
    );
    app.shell.ui.last_memory_sample = stale;
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = rx;
    app.shell.indexing.pending_request_id = Some(306);
    app.shell.indexing.in_progress = true;
    tx.send(IndexResponse::Finished {
        request_id: 306,
        source: IndexSource::Walker,
    })
    .expect("send finished");

    app.poll_index_response();

    assert!(app.shell.runtime.notice.is_empty());
    assert_eq!(app.shell.ui.memory_usage_bytes, Some(123));
    assert_eq!(app.shell.ui.last_memory_sample, stale);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn finished_index_with_filters_reuses_incremental_snapshot_without_full_rescan() {
    let root = test_root("finished-filter-incremental-snapshot");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.include_files = false;
    app.shell.runtime.include_dirs = true;
    app.shell.ui.ignore_list_enabled = false;
    app.shell.runtime.total_match_count = 99;

    let kept = dir_entry(root.join("kept"));
    let other = dir_entry(root.join("other"));
    app.shell.indexing.build.index.entries = vec![kept.clone(), other.clone()];
    app.shell.indexing.build.incremental_filtered_entries = vec![kept.clone()];
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = rx;
    app.shell.indexing.pending_request_id = Some(307);
    app.shell.indexing.in_progress = true;
    tx.send(IndexResponse::Finished {
        request_id: 307,
        source: IndexSource::Walker,
    })
    .expect("send finished");

    app.poll_index_response();

    assert_eq!(
        app.shell.runtime.all_entries.as_ref(),
        &vec![kept.clone(), other]
    );
    assert_eq!(app.shell.runtime.entries.as_ref(), &vec![kept.clone()]);
    assert_eq!(app.shell.runtime.results, vec![(kept.path, 0.0)]);
    assert_eq!(app.shell.runtime.total_match_count, 1);
    assert!(app
        .shell
        .indexing
        .build
        .incremental_filtered_entries
        .is_empty());

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn capped_walker_finished_drains_large_backlog_without_long_tail_regression() {
    let root = test_root("capped-finished-large-backlog");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = rx;
    app.shell.indexing.pending_request_id = Some(303);
    app.shell.indexing.in_progress = true;
    app.shell.indexing.pending_entries_request_id = Some(303);
    app.shell.indexing.build.pending_entries = (0..5_000)
        .map(|index| IndexEntry {
            path: root.join(format!("file-{index}.txt")),
            kind: EntryKind::file(),
            kind_known: true,
        })
        .collect();
    tx.send(IndexResponse::Truncated {
        request_id: 303,
        limit: 500_000,
    })
    .expect("send truncated");
    tx.send(IndexResponse::Finished {
        request_id: 303,
        source: IndexSource::Walker,
    })
    .expect("send finished");

    // Keep this assertion about the per-poll entry cap, not host wall-clock speed.
    for _ in 0..8 {
        app.poll_index_response_with_budget_for_test(Duration::from_secs(1));
        if app.shell.indexing.pending_finish.is_none() {
            break;
        }
    }

    assert!(!app.shell.indexing.in_progress);
    assert!(app.shell.indexing.pending_finish.is_none());
    assert!(app.shell.indexing.build.pending_entries.is_empty());
    assert_eq!(app.shell.runtime.all_entries.len(), 5_000);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn empty_query_unfiltered_indexing_does_not_duplicate_incremental_snapshot() {
    let root = test_root("empty-query-no-incremental-dup");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = rx;
    app.shell.indexing.pending_request_id = Some(304);
    app.shell.indexing.in_progress = true;
    let entries = (0..2_000)
        .map(|index| IndexEntry {
            path: root.join(format!("file-{index}.txt")),
            kind: EntryKind::file(),
            kind_known: true,
        })
        .collect();
    tx.send(IndexResponse::Batch {
        request_id: 304,
        entries,
    })
    .expect("send batch");

    app.poll_index_response();

    assert!(app
        .shell
        .indexing
        .build
        .incremental_filtered_entries
        .is_empty());
    assert!(app.shell.runtime.entries.is_empty());
    assert!(app.shell.indexing.build.index.entries.len() >= 50);
    assert!(app.shell.indexing.build.index.entries.len() < 2_000);
    assert_eq!(app.shell.runtime.results.len(), 50);

    let _ = fs::remove_dir_all(&root);
}

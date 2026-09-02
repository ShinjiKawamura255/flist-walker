use super::*;

fn wait_for_filelist_index_settlement(app: &mut FlistWalkerApp, deadline: Instant) {
    while app.shell.indexing.pending_request_id.is_some() {
        assert!(
            Instant::now() < deadline,
            "tiny local FileList index must settle before the GUI liveness deadline"
        );
        app.poll_index_response_with_budget_for_test(Duration::from_millis(10));
        thread::yield_now();
    }
    assert!(matches!(
        app.shell.indexing.build.index.source,
        IndexSource::FileList(_)
    ));
}

#[test]
fn tc_152_startup_and_refresh_settle_filelist_source_and_entries_regression() {
    let root = test_root("pipeline-native-filelist-settlement");
    fs::create_dir_all(&root).expect("create root");
    fs::write(root.join("first.txt"), "first").expect("write first entry");
    fs::write(root.join("second.txt"), "second").expect("write second entry");
    fs::write(root.join("FileList.txt"), "first.txt\n").expect("write startup FileList");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    wait_for_filelist_index_settlement(&mut app, Instant::now() + Duration::from_secs(2));
    assert_eq!(app.shell.runtime.all_entries.len(), 1);
    assert!(app.shell.runtime.all_entries.iter().any(|entry| entry
        .path
        .file_name()
        .is_some_and(|name| name == "first.txt")));

    fs::write(root.join("FileList.txt"), "first.txt\nsecond.txt\n")
        .expect("update refresh FileList");
    app.request_index_refresh();
    wait_for_filelist_index_settlement(&mut app, Instant::now() + Duration::from_secs(2));
    assert_eq!(app.shell.runtime.all_entries.len(), 2);
    assert!(app.shell.runtime.all_entries.iter().any(|entry| entry
        .path
        .file_name()
        .is_some_and(|name| name == "second.txt")));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn queued_request_for_tab_exists_is_false_when_queue_is_empty() {
    let root = test_root("pipeline-queue-empty");
    fs::create_dir_all(&root).expect("create dir");
    let app = FlistWalkerApp::new(root.clone(), 50, String::new());

    assert!(!app.queued_request_for_tab_exists(1));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn queued_request_for_tab_exists_is_true_for_matching_tab() {
    let root = test_root("pipeline-queue-match");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let tab_id = app.current_tab_id().expect("tab id");
    app.shell.indexing.pending_queue.push_back(IndexRequest {
        request_id: 7,
        tab_id,
        root: root.clone(),
        use_filelist: true,
        include_files: true,
        include_dirs: true,
        max_depth: crate::indexer::MaxDepth::unlimited(),
    });

    assert!(app.queued_request_for_tab_exists(tab_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn superseded_queued_index_request_releases_its_route() {
    let root = test_root("pipeline-superseded-route");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let tab_id = app.current_tab_id().expect("tab id");
    let request = |request_id| IndexRequest {
        request_id,
        tab_id,
        root: root.clone(),
        use_filelist: true,
        include_files: true,
        include_dirs: true,
        max_depth: crate::indexer::MaxDepth::unlimited(),
    };
    app.shell.indexing.request_tabs.insert(7, tab_id);
    app.shell.indexing.request_tabs.insert(8, tab_id);
    app.shell.indexing.pending_queue.push_back(request(7));

    app.enqueue_index_request(request(8));

    assert_eq!(app.shell.indexing.pending_queue.len(), 1);
    assert_eq!(app.shell.indexing.pending_queue[0].request_id, 8);
    assert_eq!(app.shell.indexing.request_tabs.get(&7), None);
    assert_eq!(app.shell.indexing.request_tabs.get(&8), Some(&tab_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn has_inflight_for_tab_uses_request_tab_mapping() {
    let root = test_root("pipeline-inflight");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let tab_id = app.current_tab_id().expect("tab id");
    app.shell.indexing.request_tabs.insert(11, tab_id);
    app.shell.indexing.inflight_requests.insert(11);

    assert!(app.has_inflight_for_tab(tab_id));
    assert!(!app.has_inflight_for_tab(tab_id.saturating_add(1)));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_replaced_warm_request_is_explicitly_stale_until_cleanup() {
    let root = test_root("tc-207-replaced-warm-stale-routing");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    reset_index_request_state_for_test(&mut app);
    let old_warm_tab_id = app.shell.tabs.get(0).expect("old warm tab").id;
    let replacement_warm_tab_id = app.current_tab_id().expect("replacement warm tab");
    let request_id = app
        .shell
        .indexing
        .allocate_request_id(Some(old_warm_tab_id));
    app.shell.indexing.warm_tab_id = Some(old_warm_tab_id);

    app.shell
        .indexing
        .replace_warm_tab(Some(replacement_warm_tab_id));

    assert!(app
        .shell
        .indexing
        .superseded_request_ids
        .contains(&request_id));
    assert!(matches!(
        app.shell.indexing.route_response(request_id),
        IndexResponseRoute::Stale
    ));
    let (index_tx, index_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = index_tx;
    {
        let old_warm_tab = app.shell.tabs.get_mut(0).expect("old warm tab");
        old_warm_tab.index_state.begin_index_request(request_id);
    }

    app.switch_to_tab_index(0);

    let replacement = index_rx
        .try_recv()
        .expect("reactivating a superseded warm tab must start a fresh generation");
    assert_eq!(replacement.tab_id, old_warm_tab_id);
    assert_ne!(replacement.request_id, request_id);
    assert_eq!(
        app.shell.indexing.pending_request_id,
        Some(replacement.request_id)
    );
    assert!(!app
        .shell
        .indexing
        .superseded_request_ids
        .contains(&request_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_superseded_warm_reactivation_rolls_back_until_reclaimer_capacity() {
    let root = test_root("tc-207-superseded-warm-reactivation-full");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    reset_index_request_state_for_test(&mut app);
    let active_tab_id = app.current_tab_id().expect("active tab");
    let target_index = 0;
    let target_tab_id = app.shell.tabs.get(target_index).expect("target tab").id;
    let request_id = app.shell.indexing.allocate_request_id(Some(target_tab_id));
    {
        let target = app.shell.tabs.get_mut(target_index).expect("target tab");
        target.index_state.begin_index_request(request_id);
    }
    app.shell.indexing.background_states.insert(
        request_id,
        BackgroundIndexState {
            source: Some(IndexSource::FileList(root.join("FileList.txt"))),
            entries: (0..100_000)
                .map(|index| file_entry(root.join(format!("old-{index}.txt"))))
                .collect(),
            replaced: true,
        },
    );
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
        .try_publish(IndexResponse::Finished {
            request_id,
            source: IndexSource::FileList(root.join("FileList.txt")),
        })
        .expect("seed terminal mailbox");
    app.shell.indexing.warm_tab_id = Some(target_tab_id);
    app.shell.indexing.replace_warm_tab(Some(active_tab_id));
    let (index_tx, index_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = index_tx;

    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut held = app.capture_active_tab_state(12_000 + index as u64);
        held.result_state.committed.all_entries =
            Arc::new(vec![file_entry(root.join(format!("held-{index}.txt")))]);
        held.index_state
            .set_committed_snapshot_present_for_test(true);
        app.shell
            .tabs
            .retire_tab_resources_for_test(held.take_heavy_resources())
            .expect("fill reclaimer");
    }

    app.switch_to_tab_index(target_index);

    assert_eq!(app.current_tab_id(), Some(active_tab_id));
    assert_eq!(
        app.shell.tabs.pending_activation_tab_id,
        Some(target_tab_id)
    );
    assert_eq!(
        app.shell
            .indexing
            .background_states
            .get(&request_id)
            .expect("rolled back background owner")
            .entries
            .len(),
        100_000
    );
    assert!(mailbox.has_terminal_response());
    assert!(index_rx.try_recv().is_err());

    app.poll_index_response();
    assert_eq!(app.current_tab_id(), Some(active_tab_id));
    assert_eq!(
        app.shell.tabs.pending_activation_tab_id,
        Some(target_tab_id)
    );
    assert!(app.shell.indexing.pending_stale_build_reclaim.is_some());

    app.shell.tabs.resume_resource_reclaimer();
    let deadline = Instant::now() + Duration::from_secs(2);
    while app.current_tab_id() != Some(target_tab_id) {
        assert!(
            Instant::now() < deadline,
            "deferred superseded activation must settle after reclaimer capacity returns"
        );
        app.poll_index_response();
        thread::yield_now();
    }
    let replacement = index_rx.try_recv().expect("one fresh generation");
    assert_eq!(replacement.tab_id, target_tab_id);
    assert_ne!(replacement.request_id, request_id);
    assert!(index_rx.try_recv().is_err());
    assert!(!app
        .shell
        .indexing
        .superseded_request_ids
        .contains(&request_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_repeated_rapid_demote_keeps_request_owners_bounded_and_quiesces() {
    let root = test_root("tc-207-repeated-demote-owner-bound");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.create_new_tab();
    reset_index_request_state_for_test(&mut app);
    if let Ok(mut mailboxes) = app.shell.indexing.response_mailboxes.lock() {
        for mailbox in mailboxes.values() {
            mailbox.close();
        }
        mailboxes.clear();
    }
    let (index_tx, index_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = index_tx;
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Dormant);
    app.shell
        .indexing
        .set_committed_snapshot_present_for_test(false);
    for tab in app.shell.tabs.iter_mut() {
        tab.index_state
            .set_lifecycle_for_test(TabResourceLifecycle::Dormant);
        tab.index_state
            .set_committed_snapshot_present_for_test(false);
        tab.index_state.clear_index_request_state();
    }
    app.request_index_refresh();

    for _ in 0..24 {
        let next = (app.shell.tabs.active_tab_index() + 1) % app.shell.tabs.len();
        app.switch_to_tab_index(next);
        while index_rx.try_recv().is_ok() {}
        let route_count = app.shell.indexing.request_tabs.len();
        let mailbox_count = app
            .shell
            .indexing
            .response_mailboxes
            .lock()
            .expect("mailboxes")
            .len();
        assert!(app.shell.indexing.inflight_requests.len() <= FlistWalkerApp::INDEX_MAX_CONCURRENT);
        assert!(app.shell.indexing.pending_queue.len() <= FlistWalkerApp::INDEX_MAX_QUEUE);
        assert!(
            route_count <= FlistWalkerApp::INDEX_MAX_CONCURRENT + FlistWalkerApp::INDEX_MAX_QUEUE
        );
        assert_eq!(mailbox_count, route_count);
        assert!(
            app.shell.indexing.superseded_request_ids.len()
                <= FlistWalkerApp::INDEX_MAX_CONCURRENT + FlistWalkerApp::INDEX_MAX_QUEUE
        );
        assert!(app
            .shell
            .indexing
            .superseded_request_ids
            .iter()
            .all(|request_id| app.shell.indexing.request_tabs.contains_key(request_id)));
        assert!(app.shell.indexing.background_finalizations.keys().count() <= 2);
        assert!(app.shell.indexing.pending_stale_build_reclaim.is_none());
    }

    let deadline = Instant::now() + Duration::from_secs(2);
    while !app.shell.indexing.request_tabs.is_empty() {
        assert!(
            Instant::now() < deadline,
            "all bounded request owners must reach quiescence"
        );
        app.dispatch_index_queue();
        while index_rx.try_recv().is_ok() {}
        let inflight = app
            .shell
            .indexing
            .inflight_requests
            .iter()
            .copied()
            .collect::<Vec<_>>();
        for request_id in inflight {
            let mailbox = app
                .shell
                .indexing
                .response_mailboxes
                .lock()
                .expect("mailboxes")
                .get(&request_id)
                .cloned()
                .expect("inflight mailbox");
            if !mailbox.has_terminal_response() {
                mailbox
                    .try_publish(IndexResponse::Canceled { request_id })
                    .expect("publish terminal cancellation");
            }
        }
        app.poll_index_response();
        thread::yield_now();
    }
    assert!(
        app.shell.indexing.superseded_request_ids.is_empty(),
        "orphan superseded ids: {:?}",
        (
            &app.shell.indexing.superseded_request_ids,
            app.shell
                .indexing
                .latest_request_ids
                .lock()
                .expect("latest requests")
                .clone()
        )
    );
    assert!(app.shell.indexing.inflight_requests.is_empty());
    assert!(app.shell.indexing.pending_queue.is_empty());
    assert!(app
        .shell
        .indexing
        .response_mailboxes
        .lock()
        .expect("mailboxes")
        .is_empty());
    assert!(app.shell.indexing.pending_stale_build_reclaim.is_none());
    assert_eq!(
        app.shell.indexing.background_finalizations.keys().count(),
        0
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_206_dequeued_worker_request_regression_cannot_recreate_mailbox_after_cleanup() {
    let root = test_root("tc-206-dequeued-cleanup-mailbox-race");
    let mut app = FlistWalkerApp::new(root, 50, String::new());
    reset_index_request_state_for_test(&mut app);
    let tab_id = app.current_tab_id().expect("active tab");
    let dequeued_request_id = app.shell.indexing.allocate_request_id(Some(tab_id));
    let mailboxes = Arc::clone(&app.shell.indexing.response_mailboxes);
    let retired_mailbox = mailboxes
        .lock()
        .expect("mailboxes")
        .get(&dequeued_request_id)
        .cloned()
        .expect("allocated request mailbox");

    // Deterministic race ordering: the request has already been dequeued, then the
    // UI cleanup removes its registered mailbox before the worker resolves the sink.
    app.shell.indexing.cleanup_request(dequeued_request_id);
    let mailbox_count_after_cleanup = mailboxes.lock().expect("mailboxes").len();
    let worker_mailbox = crate::app::index_worker::mailbox_for_dequeued_request(
        mailboxes.as_ref(),
        dequeued_request_id,
    );

    assert!(worker_mailbox.is_none());
    let mailboxes_after_worker_lookup = mailboxes.lock().expect("mailboxes");
    assert_eq!(
        mailboxes_after_worker_lookup.len(),
        mailbox_count_after_cleanup,
        "worker lookup must not recreate an owner after cleanup"
    );
    assert!(!mailboxes_after_worker_lookup.contains_key(&dequeued_request_id));
    drop(mailboxes_after_worker_lookup);
    assert!(retired_mailbox
        .try_publish(IndexResponse::Canceled {
            request_id: dequeued_request_id,
        })
        .is_err());
    assert!(!retired_mailbox.has_terminal_response());
    assert!(matches!(
        app.shell.indexing.route_response(dequeued_request_id),
        IndexResponseRoute::Stale
    ));
    assert!(app.shell.indexing.request_tabs.is_empty());
}

#[test]
fn should_refresh_incremental_search_is_false_when_delta_is_zero() {
    let root = test_root("pipeline-refresh-zero");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.indexing.build.incremental_filtered_entries = vec![unknown_entry(root.join("a.txt"))];
    app.shell.indexing.last_search_snapshot_len = 1;

    assert!(!app.should_refresh_incremental_search());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn should_refresh_incremental_search_is_false_for_small_delta_while_indexing() {
    let root = test_root("pipeline-refresh-small-delta");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "main".to_string());
    app.shell.indexing.in_progress = true;
    app.shell.indexing.build.incremental_filtered_entries = (0..64)
        .map(|i| unknown_entry(root.join(format!("file-{i}.txt"))))
        .collect();
    app.shell.indexing.last_search_snapshot_len = 0;
    app.shell.indexing.last_incremental_results_refresh =
        Instant::now() - FlistWalkerApp::INCREMENTAL_SEARCH_REFRESH_INTERVAL_DURING_INDEX;

    assert!(!app.should_refresh_incremental_search());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn should_refresh_incremental_search_is_true_for_large_delta_after_interval() {
    let root = test_root("pipeline-refresh-large-delta");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "main".to_string());
    app.shell.indexing.in_progress = true;
    app.shell.indexing.build.incremental_filtered_entries = (0
        ..(FlistWalkerApp::INCREMENTAL_SEARCH_MIN_DELTA_DURING_INDEX + 1))
        .map(|i| unknown_entry(root.join(format!("file-{i}.txt"))))
        .collect();
    app.shell.indexing.last_search_snapshot_len = 0;
    app.shell.indexing.last_incremental_results_refresh =
        Instant::now() - FlistWalkerApp::INCREMENTAL_SEARCH_REFRESH_INTERVAL_DURING_INDEX;

    assert!(app.should_refresh_incremental_search());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn regression_ignore_list_is_applied_when_files_and_folders_are_both_enabled() {
    let root = test_root("ignore-list-fast-path-regression");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let kept = root.join("keep.txt");
    let ignored_old = root.join("old-cache.txt");
    let ignored_tilde = root.join("backup~.txt");

    app.shell.runtime.all_entries = Arc::new(vec![
        file_entry(ignored_old.clone()),
        file_entry(ignored_tilde.clone()),
        file_entry(kept.clone()),
    ]);
    app.shell.indexing.build.index.entries.clear();
    app.shell.indexing.build.index.source = IndexSource::Walker;
    app.shell.runtime.entries = Arc::new(Vec::new());
    app.shell.runtime.include_files = true;
    app.shell.runtime.include_dirs = true;
    app.shell.ui.ignore_list_enabled = true;
    app.shell.runtime.ignore_list_terms = Arc::new(vec!["old".to_string(), "~".to_string()]);

    app.apply_entry_filters(false);

    assert_eq!(
        app.shell.runtime.entries.as_ref(),
        &[file_entry(kept.clone())]
    );
    assert_eq!(app.shell.runtime.results, vec![(kept, 0.0)]);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn regression_ignore_list_toggle_off_keeps_all_entries_visible() {
    let root = test_root("ignore-list-toggle-off-regression");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let kept = root.join("keep.txt");
    let ignored_old = root.join("old-cache.txt");

    app.shell.runtime.all_entries = Arc::new(vec![
        file_entry(ignored_old.clone()),
        file_entry(kept.clone()),
    ]);
    app.shell.indexing.build.index.entries.clear();
    app.shell.indexing.build.index.source = IndexSource::Walker;
    app.shell.runtime.entries = Arc::new(Vec::new());
    app.shell.runtime.include_files = true;
    app.shell.runtime.include_dirs = true;
    app.shell.ui.ignore_list_enabled = false;
    app.shell.runtime.ignore_list_terms = Arc::new(vec!["old".to_string()]);

    app.apply_entry_filters(false);

    assert_eq!(
        app.shell.runtime.entries.as_ref(),
        &[file_entry(ignored_old.clone()), file_entry(kept.clone())]
    );
    assert_eq!(
        app.shell.runtime.results,
        vec![(ignored_old, 0.0), (kept, 0.0)]
    );
    let _ = fs::remove_dir_all(&root);
}

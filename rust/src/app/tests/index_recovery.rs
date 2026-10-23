use super::*;

#[test]
fn tc_236_stop_current_invalidates_terminal_and_retains_committed_snapshot() {
    let root = test_root("tc236-stop-current");
    fs::create_dir_all(&root).unwrap();
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let tab = app.current_tab_id().unwrap();
    let entries = Arc::new(vec![file_entry(root.join("committed"))]);
    app.shell.runtime.committed_for_test_mut().entries = Arc::clone(&entries);
    app.shell.runtime.committed_for_test_mut().all_entries = Arc::clone(&entries);
    let request = app.shell.indexing.allocate_request_id(Some(tab));
    app.shell.indexing.pending_request_id = Some(request);
    app.shell.indexing.in_progress = true;
    app.shell.indexing.inflight_requests.insert(request);
    app.shell.indexing.pending_finish = Some(PendingActiveIndexFinish {
        request_id: request,
        source: IndexSource::Walker,
    });
    app.shell.indexing.refresh_after_pending_finish = Some(PendingIndexRefreshMode::Normal);
    app.stop_current_indexing();
    assert!(!app.shell.indexing.index_admitted(tab));
    assert!(app.shell.indexing.pending_finish.is_none());
    assert!(app.shell.indexing.refresh_after_pending_finish.is_none());
    assert!(matches!(
        app.shell.indexing.route_response(request),
        IndexResponseRoute::Stale
    ));
    assert!(Arc::ptr_eq(&entries, &app.shell.runtime.all_entries));
    app.request_index_refresh();
    assert!(app.shell.indexing.pending_request_id.is_none());
    app.restart_current_index();
    assert!(app.shell.indexing.index_admitted(tab));
    assert_ne!(app.shell.indexing.pending_request_id, Some(request));
    assert!(app.shell.indexing.pending_request_id.is_some());
}

#[test]
fn tc_236_stop_all_blocks_future_tabs_and_refresh_resumes_only_target() {
    let root = test_root("tc236-stop-all");
    fs::create_dir_all(&root).unwrap();
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let first = app.current_tab_id().unwrap();
    app.stop_all_indexing();
    app.create_new_tab();
    let second = app.current_tab_id().unwrap();
    assert_ne!(first, second);
    assert!(!app.shell.indexing.index_admitted(first));
    assert!(!app.shell.indexing.index_admitted(second));
    assert!(app.shell.indexing.pending_request_id.is_none());
    app.restart_current_index();
    assert!(app.shell.indexing.index_admitted(second));
    assert!(!app.shell.indexing.index_admitted(first));
}

#[test]
fn tc_236_paused_tab_activation_filter_root_and_deferred_finish_cannot_restart() {
    let root = test_root("tc236-admission");
    let other = root.join("other");
    fs::create_dir_all(&other).unwrap();
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.stop_all_indexing();
    app.switch_to_tab_index(0);
    assert_eq!(app.shell.tabs.active_tab_index(), 0);
    app.maybe_reindex_from_filter_toggles(true, true, true, false);
    app.apply_root_change(other.clone());
    app.poll_index_response();
    assert_eq!(app.shell.runtime.root, other);
    assert!(app.shell.indexing.pending_request_id.is_none());
    assert!(app.shell.indexing.pending_queue.is_empty());
    assert!(app.shell.indexing.inflight_requests.is_empty());
    let id = app.current_tab_id().unwrap();
    assert!(!app.shell.indexing.index_admitted(id));
    app.restart_current_index();
    assert!(app.shell.indexing.pending_request_id.is_some());
}

#[test]
fn tc_236_failed_request_suspends_auto_retry_until_explicit_refresh() {
    let root = test_root("tc236-failure");
    fs::create_dir_all(&root).unwrap();
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let tab = app.current_tab_id().unwrap();
    let request = app.shell.indexing.allocate_request_id(Some(tab));
    app.shell.indexing.pending_request_id = Some(request);
    app.shell.indexing.in_progress = true;
    let mut owner =
        crate::app::index_response_effects::IndexResponseApplicationOwner::new(&mut app);
    owner.apply(crate::app::index_response_effects::RoutedIndexResponse {
        route: IndexResponseRoute::Active,
        response: IndexResponse::Failed {
            request_id: request,
            error: "unavailable".into(),
        },
        active_mailbox_blocked: false,
        from_shared_response_queue: false,
    });
    assert!(!app.shell.indexing.index_admitted(tab));
    app.request_index_refresh();
    assert!(app.shell.indexing.pending_request_id.is_none());
    app.restart_current_index();
    assert!(app.shell.indexing.index_admitted(tab));
    assert!(app.shell.indexing.pending_request_id.is_some());
}

#[test]
fn tc_236_stop_cancels_queued_and_inactive_generations_and_deferred_creation() {
    let root = test_root("tc236-inactive");
    fs::create_dir_all(&root).unwrap();
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let first = app.current_tab_id().unwrap();
    app.create_new_tab();
    let active = app.current_tab_id().unwrap();
    let request = app.shell.indexing.allocate_request_id(Some(first));
    {
        let tab = app.shell.tabs.get_mut(0).unwrap();
        tab.index_state.pending_index_request_id = Some(request);
        tab.index_state.index_in_progress = true;
        tab.index_state.pending_index_finish = Some(PendingActiveIndexFinish {
            request_id: request,
            source: IndexSource::Walker,
        });
        tab.index_state.refresh_after_pending_finish = Some(PendingIndexRefreshMode::Normal);
    }
    app.shell.indexing.pending_queue.push_back(IndexRequest {
        request_id: request,
        tab_id: first,
        root: root.clone(),
        use_filelist: false,
        include_files: true,
        include_dirs: true,
        max_depth: crate::indexer::MaxDepth::unlimited(),
        follow_links: false,
        complete_walker_snapshot: false,
    });
    app.shell.features.filelist.workflow.pending_after_index = Some(PendingFileListAfterIndex {
        tab_id: first,
        root: root.clone(),
        index_request_id: Some(request),
    });
    app.shell.features.filelist.workflow.in_progress = true;
    app.stop_all_indexing();
    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_after_index
        .is_none());
    assert!(
        app.shell.features.filelist.workflow.in_progress,
        "write transaction is outside indexing cancellation"
    );
    app.handle_background_index_response(
        0,
        IndexResponse::Finished {
            request_id: request,
            source: IndexSource::Walker,
        },
    );
    app.retry_pending_background_index_finish();
    let tab = app.shell.tabs.get(0).unwrap();
    assert!(tab.index_state.pending_index_finish.is_none());
    assert!(tab.index_state.refresh_after_pending_finish.is_none());
    assert!(tab.result_state.committed.all_entries.is_empty());
    assert!(app.shell.indexing.pending_queue.is_empty());
    assert!(!app.shell.indexing.index_admitted(active));
    assert!(!app.shell.indexing.index_admitted(first));
}

#[test]
fn tc_236_stop_full_reclaimer_retains_payload_but_invalidates_pending_finish() {
    let root = test_root("tc236-reclaimer-full");
    fs::create_dir_all(&root).unwrap();
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.tabs.pause_resource_reclaimer();
    for _ in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut retired = RetiredIndexBuildResources::empty();
        retired.set_stale_index_entries(vec![IndexEntry {
            path: root.join("retired"),
            kind: EntryKind::file(),
            kind_known: true,
        }]);
        assert!(app
            .shell
            .tabs
            .try_retire_index_build_resources(retired)
            .is_ok());
    }
    let tab = app.current_tab_id().unwrap();
    let request = app.shell.indexing.allocate_request_id(Some(tab));
    app.shell.indexing.pending_request_id = Some(request);
    app.shell.indexing.pending_finish = Some(PendingActiveIndexFinish {
        request_id: request,
        source: IndexSource::Walker,
    });
    app.shell
        .indexing
        .build
        .index
        .entries
        .push(file_entry(root.join("partial")));
    let allocation = app.shell.indexing.build.index.entries.as_ptr();
    app.stop_current_indexing();
    assert_eq!(app.shell.indexing.build.index.entries.as_ptr(), allocation);
    assert!(app.shell.indexing.pending_finish.is_none());
    assert!(!app.try_finish_active_index_after_pending_drain());
    assert!(!app.shell.indexing.index_start_admitted(tab));
    assert!(matches!(
        app.shell.indexing.route_response(request),
        IndexResponseRoute::Stale
    ));
    let (index_tx, index_rx) = bounded_request_channel::<IndexRequest>(8);
    app.shell.indexing.tx = index_tx;
    app.restart_current_index();
    assert!(app.shell.indexing.index_admitted(tab));
    assert!(!app.shell.indexing.index_start_admitted(tab));
    assert!(app.shell.indexing.pending_request_id.is_none());
    app.shell.tabs.resume_resource_reclaimer();
    app.poll_index_response();
    let restarted = index_rx
        .try_recv()
        .expect("deferred Refresh must start after reclamation drains");
    assert_eq!(restarted.tab_id, tab);
    assert_ne!(restarted.request_id, request);
    assert_eq!(
        app.shell.indexing.pending_request_id,
        Some(restarted.request_id)
    );
    let allocated = app.shell.indexing.next_request_id;
    for _ in 0..3 {
        app.poll_index_response();
    }
    assert!(index_rx.try_recv().is_err(), "Refresh must start once only");
    assert_eq!(app.shell.indexing.next_request_id, allocated);
}

#[test]
fn tc_236_gui_recovery_actions_visible_in_narrow_frame_and_dispatchable() {
    use crate::app::render::{RenderCommand, RenderTopActionCommand};
    let root = test_root("tc236-gui");
    fs::create_dir_all(&root).unwrap();
    let mut app = FlistWalkerApp::new(root, 50, String::new());
    let ctx = egui::Context::default();
    let input = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(560.0, 900.0),
        )),
        ..Default::default()
    };
    for _ in 0..3 {
        let _ = ctx.run_ui(input(), |ui| app.run_ui_frame(ui));
    }
    let output = ctx.run_ui(input(), |ui| app.run_ui_frame(ui));
    let texts: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::epaint::Shape::Text(text) => Some(text.galley.job.text.as_str()),
            _ => None,
        })
        .collect();
    for label in ["Refresh Index", "Stop Indexing", "Stop All Indexing"] {
        assert!(texts.contains(&label), "{label} missing from {texts:?}");
        assert!(FlistWalkerApp::top_action_command(label).is_some());
    }
    app.queue_render_command(RenderCommand::TopAction(
        RenderTopActionCommand::StopAllIndexing,
    ));
    app.dispatch_render_commands(&ctx);
    let tab = app.current_tab_id().unwrap();
    assert!(!app.shell.indexing.index_admitted(tab));
    app.queue_render_command(RenderCommand::TopAction(
        RenderTopActionCommand::RefreshIndex,
    ));
    app.dispatch_render_commands(&ctx);
    assert!(app.shell.indexing.index_admitted(tab));
}

#[test]
fn tc_236_old_finished_after_explicit_refresh_cannot_commit_or_cancel_new_generation() {
    let root = test_root("tc236-late-finished");
    fs::create_dir_all(&root).unwrap();
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let tab = app.current_tab_id().unwrap();
    let old = app.shell.indexing.allocate_request_id(Some(tab));
    app.shell.indexing.pending_request_id = Some(old);
    app.shell.indexing.in_progress = true;
    app.shell
        .indexing
        .build
        .index
        .entries
        .push(file_entry(root.join("partial")));
    app.restart_current_index();
    let new = app.shell.indexing.pending_request_id.unwrap();
    assert_ne!(old, new);
    let snapshot = Arc::clone(&app.shell.runtime.all_entries);
    let route = app.shell.indexing.route_response(old);
    assert!(matches!(route, IndexResponseRoute::Stale));
    crate::app::index_response_effects::IndexResponseApplicationOwner::new(&mut app).apply(
        crate::app::index_response_effects::RoutedIndexResponse {
            route,
            response: IndexResponse::Finished {
                request_id: old,
                source: IndexSource::Walker,
            },
            active_mailbox_blocked: false,
            from_shared_response_queue: false,
        },
    );
    assert_eq!(app.shell.indexing.pending_request_id, Some(new));
    assert_eq!(app.shell.indexing.latest_request_for_tab(tab), Some(new));
    assert!(Arc::ptr_eq(&snapshot, &app.shell.runtime.all_entries));
    assert!(app.shell.indexing.pending_finish.is_none());
}

#[test]
fn tc_236_background_failure_suspends_only_its_tab() {
    let root = test_root("tc236-background-failure");
    fs::create_dir_all(&root).unwrap();
    let mut app = FlistWalkerApp::new(root, 50, String::new());
    let first = app.current_tab_id().unwrap();
    app.create_new_tab();
    let current = app.current_tab_id().unwrap();
    let request = app.shell.indexing.allocate_request_id(Some(first));
    app.shell
        .tabs
        .get_mut(0)
        .unwrap()
        .index_state
        .pending_index_request_id = Some(request);
    app.handle_background_index_response(
        0,
        IndexResponse::Failed {
            request_id: request,
            error: "server unavailable".into(),
        },
    );
    assert!(!app.shell.indexing.index_admitted(first));
    assert!(app.shell.indexing.index_admitted(current));
    app.request_background_index_refresh_for_tab(0);
    assert!(app
        .shell
        .tabs
        .get(0)
        .unwrap()
        .index_state
        .pending_index_request_id
        .is_none());
    app.switch_to_tab_index(0);
    assert!(app.shell.indexing.pending_request_id.is_none());
    app.restart_current_index();
    assert!(app.shell.indexing.index_admitted(first));
}

/// Runs the production supervisor, IPC and actual executable through the GUI
/// coordinator. Build both binaries first; this is separate from algorithm hooks.
#[test]
#[ignore = "requires cargo build --bins; run tc_235_real_process_gui_recovery explicitly"]
fn tc_235_real_process_gui_recovery() {
    let _settings = test_settings_scope("tc235-real-gui");
    let root = test_root("tc235-real-gui");
    fs::create_dir_all(&root).unwrap();
    let listing = (0..2000)
        .map(|i| format!("item-{i}.txt\n"))
        .collect::<String>();
    fs::write(root.join("FileList.txt"), listing).unwrap();
    let executable = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/debug")
        .join(if cfg!(windows) {
            "flistwalker.exe"
        } else {
            "flistwalker"
        });
    assert!(
        executable.is_file(),
        "build production executable first: {executable:?}"
    );
    let mut app = FlistWalkerApp::new(root.clone(), 100, String::new());
    reset_index_request_state_for_test(&mut app);
    let shutdown = app.shell.worker_runtime.as_ref().unwrap().shutdown_token();
    let (tx, rx, mailboxes, handles) =
        crate::app::index_worker::spawn_isolated_index_workers_for_test(
            shutdown,
            Arc::clone(&app.shell.indexing.latest_request_ids),
            executable,
        );
    app.shell.indexing.tx = tx;
    app.shell.indexing.rx = rx;
    app.shell.indexing.response_mailboxes = mailboxes;
    for (i, handle) in handles.into_iter().enumerate() {
        app.shell
            .worker_runtime
            .as_mut()
            .unwrap()
            .push(format!("real-index-{i}"), handle);
    }
    fn finish(app: &mut FlistWalkerApp) {
        let start = Instant::now();
        while app.shell.indexing.pending_request_id.is_some()
            || app.shell.indexing.in_progress
            || app.shell.indexing.pending_finish.is_some()
        {
            app.poll_index_response();
            app.poll_search_response();
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "actual child GUI completion timed out: {}",
                app.status_line_text()
            );
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            app.shell.runtime.all_entries.len(),
            2000,
            "{}",
            app.status_line_text()
        );
        assert!(
            app.shell.runtime.freshness.is_some(),
            "child metadata must reach committed snapshot"
        );
    }
    app.restart_current_index();
    finish(&mut app);
    for _ in 0..5 {
        app.restart_current_index();
        app.stop_current_indexing();
        assert!(!app.shell.indexing.in_progress);
        assert_eq!(app.shell.runtime.all_entries.len(), 2000);
        app.restart_current_index();
        finish(&mut app);
    }
    // A real child-side root failure keeps the previous committed snapshot and
    // suspends automatic retries; restoring the root plus Refresh recovers it.
    let previous = Arc::clone(&app.shell.runtime.all_entries);
    fs::remove_file(root.join("FileList.txt")).unwrap();
    fs::remove_dir(&root).unwrap();
    app.restart_current_index();
    finish(&mut app);
    assert!(!app
        .shell
        .indexing
        .index_admitted(app.current_tab_id().unwrap()));
    assert!(app.shell.runtime.notice.contains("Indexing failed:"));
    assert!(Arc::ptr_eq(&previous, &app.shell.runtime.all_entries));
    fs::create_dir_all(&root).unwrap();
    let listing = (0..2000)
        .map(|i| format!("item-{i}.txt\n"))
        .collect::<String>();
    fs::write(root.join("FileList.txt"), listing).unwrap();
    app.restart_current_index();
    finish(&mut app);
    app.stop_all_indexing();
    app.create_new_tab();
    assert!(app.shell.indexing.pending_request_id.is_none());
    app.restart_current_index();
    finish(&mut app);
    assert!(app.shell.indexing.pending_queue.is_empty());
    drop(app);
    fs::remove_dir_all(root).unwrap();
}

fn deferred_refresh_behind_full_reclaimer(
    name: &str,
) -> (
    FlistWalkerApp,
    u64,
    crate::app::worker::channel::BoundedReceiver<IndexRequest>,
) {
    let root = test_root(name);
    fs::create_dir_all(&root).unwrap();
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.stop_all_indexing();
    app.switch_to_tab_index(0);
    let tab = app.current_tab_id().unwrap();
    app.shell.tabs.pause_resource_reclaimer();
    for _ in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut retired = RetiredIndexBuildResources::empty();
        retired.set_stale_index_entries(vec![IndexEntry {
            path: root.join("retired"),
            kind: EntryKind::file(),
            kind_known: true,
        }]);
        assert!(app
            .shell
            .tabs
            .try_retire_index_build_resources(retired)
            .is_ok());
    }
    let request = app.shell.indexing.allocate_request_id(Some(tab));
    app.shell.indexing.pending_request_id = Some(request);
    app.shell
        .indexing
        .build
        .index
        .entries
        .push(file_entry(root.join("partial")));
    let (tx, rx) = bounded_request_channel::<IndexRequest>(8);
    app.shell.indexing.tx = tx;
    app.restart_current_index_with_mode(PendingIndexRefreshMode::PreserveSort);
    assert!(app.shell.indexing.pending_request_id.is_none());
    assert!(!app.shell.indexing.index_start_admitted(tab));
    (app, tab, rx)
}

#[test]
fn tc_236_deferred_refresh_follows_original_tab_after_switch_and_starts_once() {
    let (mut app, original, rx) = deferred_refresh_behind_full_reclaimer("tc236-deferred-inactive");
    app.switch_to_tab_index(1);
    assert_eq!(app.shell.tabs.active_tab_index(), 1);
    assert_ne!(app.current_tab_id(), Some(original));
    assert_eq!(
        app.shell
            .tabs
            .get(0)
            .unwrap()
            .index_state
            .refresh_after_pending_finish,
        Some(PendingIndexRefreshMode::PreserveSort)
    );
    app.shell.tabs.resume_resource_reclaimer();
    app.poll_index_response();
    let request = rx
        .try_recv()
        .expect("original tab deferred Refresh must start while inactive");
    assert_eq!(request.tab_id, original);
    assert_eq!(
        app.shell
            .tabs
            .get(0)
            .unwrap()
            .index_state
            .pending_index_request_id,
        Some(request.request_id)
    );
    assert!(
        app.shell.indexing.pending_request_id.is_none(),
        "other tab remains paused"
    );
    let next = app.shell.indexing.next_request_id;
    for _ in 0..3 {
        app.poll_index_response();
    }
    assert!(rx.try_recv().is_err());
    assert_eq!(app.shell.indexing.next_request_id, next);
}

#[test]
fn tc_236_stop_all_cancels_refresh_waiting_for_full_reclaimer() {
    let (mut app, tab, rx) = deferred_refresh_behind_full_reclaimer("tc236-deferred-stop-all");
    app.stop_all_indexing();
    let next = app.shell.indexing.next_request_id;
    app.shell.tabs.resume_resource_reclaimer();
    for _ in 0..3 {
        app.poll_index_response();
    }
    assert!(!app.shell.indexing.index_admitted(tab));
    assert!(app.shell.indexing.pending_request_id.is_none());
    assert!(app.shell.indexing.refresh_after_pending_finish.is_none());
    assert!(rx.try_recv().is_err());
    assert_eq!(app.shell.indexing.next_request_id, next);
}

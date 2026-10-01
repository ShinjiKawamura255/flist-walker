use super::*;

fn restored_index_app(mode: usize) -> (FlistWalkerApp, PathBuf, u64) {
    let root = test_root(&format!("empty-query-progress-{mode}"));
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = tx;
    reset_index_request_state_for_test(&mut app);
    app.initialize_tabs_from_saved(
        vec![SavedTabState {
            root: root.to_string_lossy().to_string(),
            use_filelist: true,
            use_regex: false,
            ignore_case: true,
            include_files: true,
            include_dirs: mode != 1,
            max_depth: crate::indexer::MaxDepth::unlimited(),
            follow_links: false,
            query: "file".into(),
            query_history: Vec::new(),
            tab_accent: None,
        }],
        0,
    );
    let request = rx.try_recv().expect("restored tab index request");
    app.shell.indexing.build.index.source = IndexSource::FileList(root.join("FileList.txt"));
    app.shell.ui.ignore_list_enabled = mode == 2;
    app.shell.runtime.ignore_list_terms = Arc::new(vec!["hidden".into()]);
    // Keep the real index worker's response lane out of this deterministic fixture.
    let (_response_tx, response_rx) = mpsc::channel();
    app.shell.indexing.rx = response_rx;
    (app, root, request.request_id)
}

fn ingest_files(app: &mut FlistWalkerApp, root: &Path, request_id: u64, count: usize) {
    app.queue_index_batch(
        request_id,
        (0..count)
            .map(|i| IndexEntry {
                path: root.join(format!("file-{i}.txt")),
                kind: EntryKind::file(),
                kind_known: true,
            })
            .collect(),
    );
    assert!(app.drain_queued_index_entries(request_id, count));
}

#[test]
fn tc_151_restored_query_clear_retires_obsolete_preparation_and_keeps_ingesting() {
    for mode in 0..3 {
        let (mut app, root, request_id) = restored_index_app(mode);
        ingest_files(&mut app, &root, request_id, 100_000);
        app.apply_entry_filters(true);
        app.poll_active_entry_filter();
        assert!(app.active_entry_filter_pending());
        app.clear_query_and_selection();

        assert!(app.shell.runtime.query_state.query.is_empty());
        assert!(!app.active_entry_filter_pending(), "mode={mode}");
        assert!(!app.shell.search.in_progress());
        assert!(!app.status_line_text().contains("Searching..."));
        assert_eq!(app.shell.runtime.total_match_count, 100_000);
        assert_eq!(app.shell.runtime.results.len(), 50);
        assert_eq!(app.shell.runtime.current_row, Some(0));

        for _ in 0..3 {
            ingest_files(&mut app, &root, request_id, 1);
            app.apply_incremental_empty_query_results();
            assert!(!app.active_entry_filter_pending(), "mode={mode}");
            assert_eq!(
                app.shell.runtime.total_match_count,
                app.shell.indexing.build.index.entries.len()
            );
        }
    }
}

#[test]
fn tc_151_empty_query_filtered_batches_reuse_candidates_without_preparation() {
    for mode in [1, 2] {
        let (mut app, root, request_id) = restored_index_app(mode);
        app.clear_query_and_selection();
        ingest_files(&mut app, &root, request_id, 20_000);
        app.apply_incremental_empty_query_results();
        assert!(!app.active_entry_filter_pending(), "mode={mode}");
        assert_eq!(app.shell.runtime.total_match_count, 20_000);
        assert_eq!(app.shell.runtime.results.len(), 50);
    }
}

#[test]
fn tc_151_query_after_live_empty_results_prepares_current_candidates() {
    for (mode, all_matches) in (0..3).flat_map(|mode| [(mode, false), (mode, true)]) {
        let (mut app, root, request_id) = restored_index_app(mode);
        app.clear_query_and_selection();
        ingest_files(&mut app, &root, request_id, 2048);
        app.apply_incremental_empty_query_results();
        let incremental_ptr = app
            .shell
            .indexing
            .build
            .incremental_filtered_entries
            .as_ptr();
        let incremental_capacity = app
            .shell
            .indexing
            .build
            .incremental_filtered_entries
            .capacity();
        let (tx, rx) = mpsc::channel();
        app.shell.search.tx = tx;
        app.shell.runtime.query_state.query = if all_matches { "" } else { "file" }.into();
        if all_matches {
            app.shell.runtime.result_sort_mode = ResultSortMode::NameAsc;
            app.shell.runtime.result_sort_scope = ResultSortScope::AllMatches;
        }
        app.update_results();
        for _ in 0..100 {
            if !app.active_entry_filter_pending() {
                break;
            }
            app.poll_active_entry_filter();
        }
        assert!(!app.active_entry_filter_pending());
        let request = rx
            .try_recv()
            .expect("search resumes without another index batch");
        assert_eq!(request.entries.len(), 2048, "mode={mode}");
        if mode != 0 {
            assert_eq!(
                app.shell
                    .indexing
                    .build
                    .incremental_filtered_entries
                    .as_ptr(),
                incremental_ptr,
                "proven membership owner must be reused"
            );
            assert_eq!(
                app.shell
                    .indexing
                    .build
                    .incremental_filtered_entries
                    .capacity(),
                incremental_capacity
            );
        }
        assert_eq!(request.query, if all_matches { "" } else { "file" });
        assert_eq!(request.sort_scope, app.shell.runtime.result_sort_scope);
        assert!(rx.try_recv().is_err());
    }
}

#[test]
fn tc_151_live_empty_gui_sort_prepares_current_candidates() {
    for (mode, metadata_sort) in (0..3).flat_map(|mode| [(mode, false), (mode, true)]) {
        let (mut app, root, request_id) = restored_index_app(mode);
        app.clear_query_and_selection();
        ingest_files(&mut app, &root, request_id, 2048);
        app.apply_incremental_empty_query_results();
        let (tx, rx) = mpsc::channel();
        app.shell.search.tx = tx;
        if metadata_sort {
            app.select_result_sort_mode(ResultSortMode::ModifiedDesc);
        } else {
            app.select_result_sort_mode(ResultSortMode::NameAsc);
            app.set_result_sort_scope(ResultSortScope::AllMatches);
        }
        for _ in 0..100 {
            if !app.active_entry_filter_pending() {
                break;
            }
            app.poll_active_entry_filter();
        }
        assert!(!app.active_entry_filter_pending());
        let request = rx.try_recv().expect("GUI sort submits current candidates");
        assert_eq!(
            request.entries.len(),
            2048,
            "mode={mode}, metadata={metadata_sort}"
        );
        assert_eq!(request.sort_scope, ResultSortScope::AllMatches);
        assert!(rx.try_recv().is_err(), "only one authoritative request");
    }
}

#[test]
fn tc_151_empty_query_filter_change_never_reuses_old_membership() {
    let (mut app, root, request_id) = restored_index_app(1);
    ingest_files(&mut app, &root, request_id, 2048);
    app.clear_query_and_selection();
    app.shell.runtime.ignore_list_terms = Arc::new(vec!["file-1".into()]);
    app.shell.ui.ignore_list_enabled = true;
    app.apply_entry_filters(true);
    assert!(app.active_entry_filter_pending());
    for _ in 0..100 {
        if !app.active_entry_filter_pending() {
            break;
        }
        app.poll_active_entry_filter();
    }
    assert!(!app.active_entry_filter_pending());
    let expected = app
        .shell
        .indexing
        .build
        .index
        .entries
        .iter()
        .filter(|entry| !entry.path.to_string_lossy().contains("file-1"))
        .count();
    assert_eq!(app.shell.runtime.total_match_count, expected);
    assert!(app
        .shell
        .runtime
        .results
        .iter()
        .all(|(path, _)| !path.to_string_lossy().contains("file-1")));
    ingest_files(&mut app, &root, request_id, 1);
    app.apply_incremental_empty_query_results();
    assert!(!app.active_entry_filter_pending());
    assert_eq!(app.shell.runtime.total_match_count, expected + 1);
}

#[test]
fn tc_151_empty_query_clear_full_reclaimer_retains_owner_and_retries() {
    let _observer_guard = lock_reclaim_drop_observer_for_test();
    let (mut app, root, request_id) = restored_index_app(0);
    ingest_files(&mut app, &root, request_id, 100_000);
    app.apply_entry_filters(true);
    app.poll_active_entry_filter();
    let previous = app.shell.runtime.results.clone();
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
    app.clear_query_and_selection();
    assert!(app.active_entry_filter_pending());
    assert_eq!(app.shell.runtime.results, previous);
    assert_eq!(
        app.shell.indexing.build.incremental_filtered_entries.len(),
        100_000
    );
    for _ in 0..3 {
        app.poll_active_entry_filter();
        assert!(app.active_entry_filter_pending());
    }
    app.shell.tabs.resume_resource_reclaimer();
    // Failed submissions discard only their emptied wrapper on the UI. Observe
    // the successful submission after capacity is available to track the scratch.
    let (drop_tx, drop_rx) = mpsc::channel();
    set_reclaim_drop_observer(Some(drop_tx));
    let deadline = Instant::now() + Duration::from_secs(3);
    while app.active_entry_filter_pending() {
        assert!(Instant::now() < deadline);
        app.poll_active_entry_filter();
        thread::yield_now();
    }
    assert_eq!(app.shell.runtime.total_match_count, 100_000);
    assert_eq!(app.shell.runtime.results.len(), 50);
    assert!(app
        .shell
        .indexing
        .build
        .incremental_filtered_entries
        .is_empty());
    let dropped_on = drop_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    set_reclaim_drop_observer(None);
    assert_eq!(dropped_on, "flistwalker-tab-reclaimer");
    ingest_files(&mut app, &root, request_id, 1);
}

#[test]
fn tc_151_rendered_cancel_cancels_search_and_ignores_late_response() {
    for key in [egui::Key::Escape, egui::Key::G] {
        let (mut app, root, request_id) = restored_index_app(1);
        app.shell.runtime.emacs_keybindings_enabled = true;
        ingest_files(&mut app, &root, request_id, 2048);
        app.apply_entry_filters(true);
        let (response_tx, response_rx) = mpsc::channel();
        app.shell.search.rx = response_rx;
        let (obsolete_id, cancel) = app.shell.search.begin_active_request(app.current_tab_id());
        let modifiers = egui::Modifiers {
            ctrl: key == egui::Key::G,
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(
            egui::RawInput {
                modifiers,
                events: vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
                ..Default::default()
            },
            |ui| {
                ui.ctx()
                    .memory_mut(|memory| memory.request_focus(app.shell.ui.query_input_id));
                app.run_ui_frame(ui);
            },
        );
        assert!(app.shell.runtime.query_state.query.is_empty());
        assert!(cancel.load(std::sync::atomic::Ordering::Acquire));
        assert!(!app.active_entry_filter_pending());
        assert_eq!(app.shell.runtime.total_match_count, 2048);
        response_tx
            .send(SearchResponse {
                request_id: obsolete_id,
                results: vec![(root.join("obsolete"), 9.0)],
                total_match_count: 1,
                sort_mode: ResultSortMode::Score,
                sort_scope: ResultSortScope::ShownResults,
                error: None,
            })
            .unwrap();
        app.poll_search_response();
        assert_eq!(app.shell.runtime.total_match_count, 2048);
        assert_eq!(app.shell.runtime.results[0].0, root.join("file-0.txt"));
    }
}

#[test]
fn tc_151_empty_query_replace_all_discards_old_filtered_prefix() {
    let (mut app, root, request_id) = restored_index_app(2);
    ingest_files(&mut app, &root, request_id, 2048);
    app.clear_query_and_selection();
    assert!(
        app.try_apply_replace_all_response(IndexResponse::ReplaceAll {
            request_id,
            entries: vec![
                IndexEntry {
                    path: root.join("replacement"),
                    kind: EntryKind::file(),
                    kind_known: true
                },
                IndexEntry {
                    path: root.join("hidden"),
                    kind: EntryKind::file(),
                    kind_known: true
                },
            ],
        })
    );
    assert!(app.drain_queued_index_entries(request_id, 2));
    app.apply_incremental_empty_query_results();
    assert!(!app.active_entry_filter_pending());
    assert_eq!(app.shell.runtime.total_match_count, 1);
    assert_eq!(app.shell.runtime.results[0].0, root.join("replacement"));
}

#[test]
fn tc_151_empty_query_all_filtered_terminal_uses_complete_empty_snapshot() {
    let (mut app, root, request_id) = restored_index_app(2);
    app.shell.runtime.ignore_list_terms = Arc::new(vec!["file".into()]);
    app.clear_query_and_selection();
    ingest_files(&mut app, &root, request_id, 20_000);
    app.apply_incremental_empty_query_results();
    assert_eq!(app.shell.runtime.total_match_count, 0);
    let (tx, rx) = mpsc::channel();
    app.shell.indexing.rx = rx;
    tx.send(IndexResponse::Finished {
        request_id,
        source: IndexSource::Walker,
    })
    .unwrap();
    app.poll_index_response_with_budget_for_test(Duration::from_secs(1));
    assert!(
        !app.active_entry_filter_pending(),
        "an empty filtered snapshot is complete too"
    );
    assert_eq!(app.shell.indexing.pending_request_id, None);
    assert_eq!(app.shell.runtime.total_match_count, 0);
    assert_eq!(app.shell.runtime.current_row, None);
}

#[test]
fn tc_151_empty_query_kind_change_refilters_membership_before_reuse() {
    let (mut app, root, request_id) = restored_index_app(1);
    ingest_files(&mut app, &root, request_id, 2048);
    app.clear_query_and_selection();
    let changed = root.join("file-0.txt");
    app.set_entry_kind(&changed, EntryKind::dir());
    app.shell
        .indexing
        .build
        .resolved_kind_updates
        .push((changed.clone(), EntryKind::dir()));
    app.apply_incremental_empty_query_results();
    assert!(app.active_entry_filter_pending());
    for _ in 0..100 {
        if !app.active_entry_filter_pending() {
            break;
        }
        app.poll_active_entry_filter();
    }
    assert!(!app.active_entry_filter_pending());
    assert_eq!(app.shell.runtime.total_match_count, 2047);
    assert!(!app
        .shell
        .runtime
        .results
        .iter()
        .any(|(path, _)| path == &changed));
    app.apply_incremental_empty_query_results();
    assert!(!app.active_entry_filter_pending());
}

#[test]
fn tc_151_empty_query_case_change_rechecks_ignore_membership() {
    let (mut app, root, request_id) = restored_index_app(2);
    app.shell.runtime.ignore_list_terms = Arc::new(vec!["FILE-".into()]);
    app.clear_query_and_selection();
    ingest_files(&mut app, &root, request_id, 2048);
    app.apply_incremental_empty_query_results();
    assert_eq!(app.shell.runtime.total_match_count, 0);
    app.shell.runtime.ignore_case = false;
    app.apply_entry_filters(true);
    for _ in 0..100 {
        if !app.active_entry_filter_pending() {
            break;
        }
        app.poll_active_entry_filter();
    }
    assert!(!app.active_entry_filter_pending());
    assert_eq!(app.shell.runtime.total_match_count, 2048);
    app.apply_incremental_empty_query_results();
    assert!(!app.active_entry_filter_pending());
}

#[test]
fn tc_151_empty_query_runtime_frames_keep_batch_ingestion_progressing() {
    let (mut app, root, request_id) = restored_index_app(1);
    app.clear_query_and_selection();
    let (tx, rx) = mpsc::channel();
    app.shell.indexing.rx = rx;
    for batch in 0..20 {
        tx.send(IndexResponse::Batch {
            request_id,
            entries: (0..1024)
                .map(|i| IndexEntry {
                    path: root.join(format!("batch-{batch}-{i}")),
                    kind: EntryKind::file(),
                    kind_known: true,
                })
                .collect(),
        })
        .unwrap();
        app.poll_index_response_with_budget_for_test(Duration::from_secs(1));
        app.poll_active_entry_filter();
        assert!(!app.active_entry_filter_pending(), "batch={batch}");
        assert_eq!(
            app.shell.indexing.build.index.entries.len(),
            (batch + 1) * 1024
        );
        assert_eq!(app.shell.runtime.total_match_count, (batch + 1) * 1024);
    }
}

fn click_ignore_case(app: &mut FlistWalkerApp) {
    let ctx = egui::Context::default();
    let frame = |app: &mut FlistWalkerApp, events| {
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1600.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ui| super::super::render_panels::render_top_panel(app, ui),
        )
    };
    for _ in 0..3 {
        frame(app, vec![]);
    }
    let output = frame(app, vec![]);
    let pos = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::epaint::Shape::Text(text) if text.galley.job.text == "Ignore Case" => {
                Some(text.pos + text.galley.size() / 2.0)
            }
            _ => None,
        })
        .expect("Ignore Case checkbox label is rendered");
    for pressed in [true, false] {
        frame(
            app,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    assert!(
        !app.shell.runtime.ignore_case,
        "actual checkbox changed case"
    );
}

#[test]
fn tc_151_rendered_case_toggle_refreshes_ignore_membership() {
    for (live, query) in [(true, ""), (false, ""), (true, "file"), (false, "file")] {
        let (mut app, root, request_id) = restored_index_app(2);
        app.shell.runtime.ignore_list_terms = Arc::new(vec!["FILE-".into()]);
        app.clear_query_and_selection();
        ingest_files(&mut app, &root, request_id, 2048);
        app.apply_incremental_empty_query_results();
        assert_eq!(app.shell.runtime.total_match_count, 0);
        if !live {
            app.shell.indexing.pending_finish = Some(PendingActiveIndexFinish {
                request_id,
                source: IndexSource::Walker,
            });
            assert!(app.try_finish_active_index_after_pending_drain());
        }
        let (tx, rx) = mpsc::channel();
        app.shell.search.tx = tx;
        app.shell.runtime.query_state.query = query.into();
        click_ignore_case(&mut app);
        for _ in 0..100 {
            if !app.active_entry_filter_pending() {
                break;
            }
            app.poll_active_entry_filter();
        }
        assert!(!app.active_entry_filter_pending());
        if query.is_empty() {
            assert_eq!(app.shell.runtime.total_match_count, 2048, "live={live}");
        } else {
            assert_eq!(
                rx.try_recv()
                    .expect("case toggle dispatches search")
                    .entries
                    .len(),
                2048,
                "live={live}"
            );
        }
    }
}

#[test]
fn tc_151_known_candidate_preparation_advances_beyond_unknown_budget() {
    let (mut app, root, request_id) = restored_index_app(0);
    ingest_files(&mut app, &root, request_id, 100_000);
    app.apply_entry_filters(true);
    app.poll_active_entry_filter_with_budget(Duration::from_secs(1));
    let cursor = app
        .shell
        .indexing
        .build
        .active_filter
        .as_ref()
        .unwrap()
        .cursor;
    assert!(
        cursor > 512,
        "known candidates must not be paced at 512 per frame: {cursor}"
    );
    assert!(cursor <= 32768);
}

#[test]
fn tc_151_terminal_scratch_backpressure_preserves_owner_and_last_good() {
    for (disconnected, capture_filelist) in
        [(false, false), (true, false), (false, true), (true, true)]
    {
        let (mut app, root, request_id) = restored_index_app(0);
        ingest_files(&mut app, &root, request_id, 20_000);
        app.shell
            .runtime
            .replace_results(vec![(root.join("last-good"), 1.0)]);
        app.shell.runtime.set_total_match_count(7);
        app.shell.runtime.set_current_row(Some(0));
        let previous_entries = Arc::clone(&app.shell.runtime.entries);
        let scratch = &app.shell.indexing.build.incremental_filtered_entries;
        let (ptr, cap) = (scratch.as_ptr(), scratch.capacity());
        assert_eq!(scratch.len(), 20_000);
        if disconnected {
            app.shell.tabs.disconnect_resource_reclaimer();
        } else {
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
        }
        let (filelist_tx, _filelist_rx) = mpsc::channel();
        app.shell.worker_bus.filelist.tx = filelist_tx;
        if capture_filelist {
            app.shell.features.filelist.workflow.pending_after_index =
                Some(PendingFileListAfterIndex {
                    tab_id: app.current_tab_id().unwrap(),
                    root: root.clone(),
                    index_request_id: Some(request_id),
                });
        }
        app.shell.indexing.pending_finish = Some(PendingActiveIndexFinish {
            request_id,
            source: IndexSource::Walker,
        });
        for _ in 0..3 {
            assert!(!app.try_finish_active_index_after_pending_drain());
            let scratch = &app.shell.indexing.build.incremental_filtered_entries;
            assert_eq!(
                (scratch.as_ptr(), scratch.capacity(), scratch.len()),
                (ptr, cap, 20_000)
            );
            assert_eq!(
                app.shell
                    .indexing
                    .pending_finish
                    .as_ref()
                    .unwrap()
                    .request_id,
                request_id
            );
            assert!(Arc::ptr_eq(&previous_entries, &app.shell.runtime.entries));
            assert_eq!(
                app.shell.runtime.results,
                vec![(root.join("last-good"), 1.0)]
            );
            assert_eq!(app.shell.runtime.total_match_count, 7);
            assert_eq!(app.shell.runtime.current_row, Some(0));
        }
        app.shell.tabs.resume_resource_reclaimer();
        let deadline = Instant::now() + Duration::from_secs(3);
        while !app.try_finish_active_index_after_pending_drain() {
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        assert!(app.shell.indexing.pending_finish.is_none());
        assert_eq!(
            app.shell
                .indexing
                .build
                .incremental_filtered_entries
                .capacity(),
            0
        );
        assert_eq!(app.shell.runtime.entries.len(), 20_000);
        assert!(
            !app.try_finish_active_index_after_pending_drain(),
            "settles once"
        );
    }
}

#[test]
fn tc_151_terminal_scratch_retires_on_reclaimer_thread() {
    let _observer_guard = lock_reclaim_drop_observer_for_test();
    let (mut app, root, request_id) = restored_index_app(0);
    ingest_files(&mut app, &root, request_id, 20_000);
    let (tx, rx) = mpsc::channel();
    set_reclaim_drop_observer(Some(tx));
    app.shell.indexing.pending_finish = Some(PendingActiveIndexFinish {
        request_id,
        source: IndexSource::Walker,
    });
    assert!(app.try_finish_active_index_after_pending_drain());
    let dropped_on = rx.recv_timeout(Duration::from_secs(3));
    set_reclaim_drop_observer(None);
    assert_eq!(dropped_on.unwrap(), "flistwalker-tab-reclaimer");
    assert_eq!(
        app.shell
            .indexing
            .build
            .incremental_filtered_entries
            .capacity(),
        0
    );
}

/// Measures warm release GUI dispatch, excluding fixture creation/compilation.
/// OS event delivery is separate; egui receives a real focused TextEdit event.
#[test]
#[ignore = "release GUI preparation latency guard; run explicitly"]
fn perf_live_query_input_to_dispatch_100k() {
    for count in [100_000, 500_000] {
        for mode in 0..3 {
            let mut samples = Vec::new();
            for _ in 0..5 {
                let (mut app, root, request_id) = restored_index_app(mode);
                // FileList locks both kind toggles; use the Walker GUI source for
                // the file-only mode so rendering does not trigger a new index.
                if mode == 1 {
                    app.shell.indexing.build.index.source = IndexSource::Walker;
                }
                let (_index_tx, index_rx) = mpsc::channel();
                app.shell.indexing.rx = index_rx;
                app.clear_query_and_selection();
                ingest_files(&mut app, &root, request_id, count);
                app.apply_incremental_empty_query_results();
                let (search_tx, search_rx) = mpsc::channel();
                app.shell.search.tx = search_tx;
                let ctx = egui::Context::default();
                let frame = |app: &mut FlistWalkerApp, events| {
                    ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(1600.0, 900.0),
                            )),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            ui.ctx().memory_mut(|memory| {
                                memory.request_focus(app.shell.ui.query_input_id)
                            });
                            app.run_ui_frame(ui);
                        },
                    )
                };
                for _ in 0..3 {
                    frame(&mut app, vec![]);
                }
                let started = Instant::now();
                frame(&mut app, vec![egui::Event::Text("file".into())]);
                assert_eq!(app.shell.runtime.query_state.query, "file");
                app.queue_index_batch(
                    request_id,
                    vec![IndexEntry {
                        path: root.join("queued-next-file"),
                        kind: EntryKind::file(),
                        kind_known: true,
                    }],
                );
                let mut polls = 0;
                let mut max_poll = Duration::ZERO;
                let request = loop {
                    if let Ok(request) = search_rx.try_recv() {
                        break request;
                    }
                    assert!(
                        started.elapsed() < Duration::from_secs(10),
                        "dispatch stalled"
                    );
                    // Match a 16ms frame cadence, including computation/render time.
                    let next_frame = started + Duration::from_millis(16 * (polls + 1));
                    thread::sleep(next_frame.saturating_duration_since(Instant::now()));
                    let poll = Instant::now();
                    app.poll_active_entry_filter();
                    max_poll = max_poll.max(poll.elapsed());
                    polls += 1;
                    frame(&mut app, vec![]);
                };
                let dispatch = started.elapsed();
                assert_eq!(request.entries.len(), count);
                assert!(
                    app.drain_queued_index_entries(request_id, 1),
                    "queued ingestion resumes after preparation"
                );
                let resumed = started.elapsed();
                eprintln!("TC-151 count={count} mode={mode} polls={polls} dispatch_ms={:.3} resumed_ms={:.3} max_poll_ms={:.3}", dispatch.as_secs_f64()*1000.0, resumed.as_secs_f64()*1000.0, max_poll.as_secs_f64()*1000.0);
                samples.push(dispatch);
            }
            samples.sort();
            eprintln!(
                "TC-151 count={count} mode={mode} median_ms={:.3} maximum_ms={:.3}",
                samples[2].as_secs_f64() * 1000.0,
                samples[4].as_secs_f64() * 1000.0
            );
            if count == 100_000 {
                assert!(
                    samples[2] <= Duration::from_millis(100),
                    "100k median responsiveness target"
                );
                assert!(
                    samples[4] <= Duration::from_millis(250),
                    "100k maximum responsiveness ceiling"
                );
            }
        }
    }
}

#[test]
fn tc_151_partial_incremental_reuse_cancel_keeps_filtered_subset() {
    let (mut app, root, request_id) = restored_index_app(2);
    app.clear_query_and_selection();
    app.queue_index_batch(
        request_id,
        (0..100_000)
            .map(|i| IndexEntry {
                path: root.join(format!(
                    "{}-{i}",
                    if i % 2 == 0 { "file" } else { "hidden" }
                )),
                kind: EntryKind::file(),
                kind_known: true,
            })
            .collect(),
    );
    assert!(app.drain_queued_index_entries(request_id, 100_000));
    app.apply_incremental_empty_query_results();
    assert_eq!(app.shell.runtime.total_match_count, 50_000);
    let ptr = app
        .shell
        .indexing
        .build
        .incremental_filtered_entries
        .as_ptr();
    app.shell.runtime.query_state.query = "file".into();
    app.update_results();
    app.poll_active_entry_filter_with_budget(Duration::from_secs(1));
    assert!(app.active_entry_filter_pending());
    app.clear_query_and_selection();
    assert!(!app.active_entry_filter_pending());
    assert_eq!(app.shell.runtime.total_match_count, 50_000);
    assert_eq!(
        app.shell
            .indexing
            .build
            .incremental_filtered_entries
            .as_ptr(),
        ptr
    );
    ingest_files(&mut app, &root, request_id, 1);
    app.apply_incremental_empty_query_results();
    assert_eq!(app.shell.runtime.total_match_count, 50_001);
}

#[test]
fn tc_151_incremental_reuse_publication_backpressure_preserves_both_owners() {
    for disconnected in [false, true] {
        let (mut app, root, request_id) = restored_index_app(2);
        app.clear_query_and_selection();
        ingest_files(&mut app, &root, request_id, 2048);
        app.apply_incremental_empty_query_results();
        let ptr = app
            .shell
            .indexing
            .build
            .incremental_filtered_entries
            .as_ptr();
        app.shell.runtime.replace_visible_entries(Arc::new(
            (0..2048)
                .map(|i| file_entry(root.join(format!("last-good-{i}"))))
                .collect(),
        ));
        let previous = Arc::clone(&app.shell.runtime.entries);
        let previous_results = app.shell.runtime.results.clone();
        if disconnected {
            app.shell.tabs.disconnect_resource_reclaimer();
        } else {
            app.shell.tabs.pause_resource_reclaimer();
            for _ in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
                let mut retired = RetiredIndexBuildResources::empty();
                retired.set_stale_index_entries(vec![IndexEntry {
                    path: root.join("held"),
                    kind: EntryKind::file(),
                    kind_known: true,
                }]);
                assert!(app
                    .shell
                    .tabs
                    .try_retire_index_build_resources(retired)
                    .is_ok());
            }
        }
        let (tx, rx) = mpsc::channel();
        app.shell.search.tx = tx;
        app.shell.runtime.query_state.query = "file".into();
        app.update_results();
        for _ in 0..3 {
            app.poll_active_entry_filter_with_budget(Duration::from_secs(1));
            assert!(app.active_entry_filter_pending());
            assert!(Arc::ptr_eq(&previous, &app.shell.runtime.entries));
            assert_eq!(app.shell.runtime.results, previous_results);
            assert_eq!(
                app.shell
                    .indexing
                    .build
                    .incremental_filtered_entries
                    .as_ptr(),
                ptr
            );
            assert_eq!(
                app.shell.indexing.build.incremental_filtered_entries.len(),
                2048
            );
            assert!(rx.try_recv().is_err());
        }
        app.shell.tabs.resume_resource_reclaimer();
        let deadline = Instant::now() + Duration::from_secs(3);
        while app.active_entry_filter_pending() {
            assert!(Instant::now() < deadline);
            app.poll_active_entry_filter();
            thread::yield_now();
        }
        assert_eq!(rx.try_recv().unwrap().entries.len(), 2048);
        assert_eq!(
            app.shell
                .indexing
                .build
                .incremental_filtered_entries
                .as_ptr(),
            ptr
        );
    }
}

#[test]
fn tc_151_incremental_reuse_kind_revision_restarts_with_new_membership() {
    let (mut app, root, request_id) = restored_index_app(1);
    app.clear_query_and_selection();
    ingest_files(&mut app, &root, request_id, 100_000);
    app.apply_incremental_empty_query_results();
    let (tx, rx) = mpsc::channel();
    app.shell.search.tx = tx;
    app.shell.runtime.query_state.query = "file".into();
    app.update_results();
    app.poll_active_entry_filter_with_budget(Duration::from_secs(1));
    assert!(app.active_entry_filter_pending());
    let changed = root.join("file-0.txt");
    let (kind_tx, kind_rx) = mpsc::channel();
    app.shell.worker_bus.kind.rx = kind_rx;
    kind_tx
        .send(KindResolveResponse {
            tab_id: app.current_tab_id().unwrap(),
            epoch: app.shell.indexing.kind_resolution_epoch,
            path: changed.clone(),
            kind: Some(EntryKind::dir()),
        })
        .unwrap();
    app.poll_kind_response();
    let deadline = Instant::now() + Duration::from_secs(3);
    while app.active_entry_filter_pending() {
        assert!(Instant::now() < deadline);
        app.poll_active_entry_filter();
        thread::yield_now();
    }
    let request = rx.try_recv().unwrap();
    assert_eq!(request.entries.len(), 99_999);
    assert!(!request.entries.iter().any(|entry| entry.path == changed));
    assert!(rx.try_recv().is_err());
}

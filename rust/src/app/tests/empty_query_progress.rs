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
        ingest_files(&mut app, &root, request_id, 20_000);
        app.apply_entry_filters(true);
        app.poll_active_entry_filter();
        assert!(app.active_entry_filter_pending());
        app.clear_query_and_selection();

        assert!(app.shell.runtime.query_state.query.is_empty());
        assert!(!app.active_entry_filter_pending(), "mode={mode}");
        assert!(!app.shell.search.in_progress());
        assert!(!app.status_line_text().contains("Searching..."));
        assert_eq!(app.shell.runtime.total_match_count, 20_000);
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
    ingest_files(&mut app, &root, request_id, 2048);
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
        2048
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
    assert_eq!(app.shell.runtime.total_match_count, 2048);
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

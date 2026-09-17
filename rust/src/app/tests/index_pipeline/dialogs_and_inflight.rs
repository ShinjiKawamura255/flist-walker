use super::*;

#[test]
fn request_index_refresh_reenables_files_when_both_filters_are_off() {
    let root = test_root("request-refresh-filter-guard");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = tx;
    reset_index_request_state_for_test(&mut app);
    app.shell.runtime.include_files = false;
    app.shell.runtime.include_dirs = false;

    app.request_index_refresh();

    let req = rx.try_recv().expect("index request should be sent");
    assert!(req.include_files);
    assert!(!req.include_dirs);
    assert!(app.shell.runtime.include_files);
    assert!(!app.shell.runtime.include_dirs);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn request_index_refresh_uses_latest_toggle_state() {
    let root = test_root("request-refresh-toggle-state");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = tx;
    reset_index_request_state_for_test(&mut app);
    app.shell.runtime.use_filelist = false;
    app.shell.runtime.include_files = false;
    app.shell.runtime.include_dirs = true;

    app.request_index_refresh();

    let req = rx.try_recv().expect("index request should be sent");
    assert!(!req.use_filelist);
    assert!(!req.include_files);
    assert!(req.include_dirs);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn request_create_filelist_walker_refresh_resets_index_state_and_registers_request() {
    let root = test_root("create-filelist-walker-refresh-reset");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "abc".to_string());
    let (tx, rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = tx;
    reset_index_request_state_for_test(&mut app);
    app.shell
        .indexing
        .build
        .pending_entries
        .push_back(IndexEntry {
            path: root.join("stale.txt"),
            kind: EntryKind::file(),
            kind_known: true,
        });
    app.shell.indexing.pending_entries_request_id = Some(7);
    app.shell
        .indexing
        .build
        .pending_kind_paths
        .push_back(root.join("stale-kind.txt"));
    app.shell
        .indexing
        .build
        .pending_kind_paths_set
        .insert(root.join("stale-kind.txt"));
    app.shell
        .indexing
        .build
        .in_flight_kind_paths
        .insert(root.join("in-flight.txt"));
    app.shell.indexing.kind_resolution_in_progress = true;
    app.shell.worker_bus.preview.pending_request_id = Some(9);
    app.shell.worker_bus.preview.in_progress = true;

    let tab_id = app.current_tab_id().expect("tab id");
    app.request_create_filelist_walker_refresh();

    let req = rx.try_recv().expect("index request should be sent");
    assert_eq!(req.tab_id, tab_id);
    assert!(!req.use_filelist);
    assert!(req.complete_walker_snapshot);
    assert!(app
        .shell
        .indexing
        .inflight_requests
        .contains(&req.request_id));
    assert!(app.shell.indexing.build.pending_entries.is_empty());
    assert_eq!(app.shell.indexing.pending_entries_request_id, None);
    assert!(app.shell.indexing.build.pending_kind_paths.is_empty());
    assert!(app.shell.indexing.build.pending_kind_paths_set.is_empty());
    assert!(app.shell.indexing.build.in_flight_kind_paths.is_empty());
    assert!(!app.shell.indexing.kind_resolution_in_progress);
    assert_eq!(app.shell.worker_bus.preview.pending_request_id, None);
    assert!(!app.shell.worker_bus.preview.in_progress);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn files_toggle_change_requests_reindex() {
    let root = test_root("files-toggle-reindex");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = tx;
    reset_index_request_state_for_test(&mut app);
    app.shell.runtime.use_filelist = false;
    app.shell.runtime.include_files = false;
    app.shell.runtime.include_dirs = true;

    app.maybe_reindex_from_filter_toggles(false, true, false, false);

    let req = rx.try_recv().expect("index request should be sent");
    assert!(!req.include_files);
    assert!(req.include_dirs);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_110_ignore_list_toggle_requests_reindex_without_replacing_visible_snapshot() {
    let root = test_root("ignore-list-toggle-reindex");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = tx;
    reset_index_request_state_for_test(&mut app);
    app.shell.runtime.use_filelist = false;
    app.shell.ui.ignore_list_enabled = true;
    app.shell.runtime.ignore_list_terms = Arc::new(vec!["ignored".to_string()]);
    app.shell.runtime.result_sort_mode = ResultSortMode::NameAsc;
    app.shell.runtime.result_sort_scope = ResultSortScope::AllMatches;
    app.shell.runtime.committed_for_test_mut().all_entries = Arc::new(vec![
        file_entry(root.join("keep.txt")),
        file_entry(root.join("ignored.txt")),
    ]);
    app.shell.runtime.committed_for_test_mut().entries = Arc::clone(&app.shell.runtime.all_entries);
    let visible_before = Arc::clone(&app.shell.runtime.entries);

    app.maybe_reindex_from_filter_toggles(false, false, false, true);

    rx.try_recv().expect("index request should be sent");
    assert!(Arc::ptr_eq(&app.shell.runtime.entries, &visible_before));
    assert_eq!(app.shell.runtime.entries.len(), 2);
    assert!(app.shell.indexing.in_progress);
    assert_eq!(app.shell.runtime.result_sort_mode, ResultSortMode::NameAsc);
    assert_eq!(
        app.shell.runtime.result_sort_scope,
        ResultSortScope::AllMatches
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_110_deferred_ignore_list_refresh_preserves_sort_when_reclaim_completes() {
    let root = test_root("deferred-ignore-list-toggle-reindex");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = tx;
    reset_index_request_state_for_test(&mut app);
    app.shell.runtime.use_filelist = false;
    app.shell.runtime.result_sort_mode = ResultSortMode::NameAsc;
    app.shell.runtime.result_sort_scope = ResultSortScope::AllMatches;
    app.shell.indexing.pending_finish = Some(PendingActiveIndexFinish {
        request_id: 41,
        source: IndexSource::Walker,
    });

    app.maybe_reindex_from_filter_toggles(false, false, false, true);

    assert!(rx.try_recv().is_err());
    app.shell.indexing.pending_finish = None;
    app.shell.indexing.build_reclaim_pending = true;
    app.retry_pending_active_index_build_reclaim();

    rx.try_recv()
        .expect("deferred index request should be sent after reclaim");
    assert_eq!(app.shell.runtime.result_sort_mode, ResultSortMode::NameAsc);
    assert_eq!(
        app.shell.runtime.result_sort_scope,
        ResultSortScope::AllMatches
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn deferred_normal_refresh_takes_precedence_over_later_ignore_list_toggle() {
    let root = test_root("deferred-normal-before-ignore-list-toggle");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = tx;
    reset_index_request_state_for_test(&mut app);
    app.shell.runtime.use_filelist = false;
    app.shell.runtime.result_sort_mode = ResultSortMode::NameAsc;
    app.shell.runtime.result_sort_scope = ResultSortScope::AllMatches;
    app.shell.indexing.pending_finish = Some(PendingActiveIndexFinish {
        request_id: 41,
        source: IndexSource::Walker,
    });

    app.maybe_reindex_from_filter_toggles(false, true, false, false);
    app.maybe_reindex_from_filter_toggles(false, false, false, true);

    assert!(rx.try_recv().is_err());
    app.shell.indexing.pending_finish = None;
    app.shell.indexing.build_reclaim_pending = true;
    app.retry_pending_active_index_build_reclaim();

    rx.try_recv()
        .expect("deferred index request should be sent after reclaim");
    assert_eq!(app.shell.runtime.result_sort_mode, ResultSortMode::Score);
    assert_eq!(
        app.shell.runtime.result_sort_scope,
        ResultSortScope::ShownResults
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_110_ignore_refresh_keeps_sorted_snapshot_until_empty_query_terminal_sort() {
    let root = test_root("ignore-refresh-empty-query-sort");
    fs::create_dir_all(&root).expect("create dir");
    let old_a = root.join("old-a.txt");
    let old_b = root.join("old-b.txt");
    let new_a = root.join("a.txt");
    let new_z = root.join("z.txt");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (request_tx, request_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = request_tx;
    let (response_tx, response_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = response_rx;
    reset_index_request_state_for_test(&mut app);
    app.shell.runtime.use_filelist = false;
    app.shell.ui.ignore_list_enabled = true;
    app.shell.runtime.ignore_list_terms = Arc::new(vec!["never-match".to_string()]);
    app.replace_results_snapshot(vec![(old_a.clone(), 0.0), (old_b.clone(), 0.0)], false);
    app.shell.runtime.result_sort_mode = ResultSortMode::NameAsc;
    app.shell.runtime.result_sort_scope = ResultSortScope::ShownResults;

    app.maybe_reindex_from_filter_toggles(false, false, false, true);
    let request = request_rx.try_recv().expect("index refresh request");
    app.shell.indexing.last_incremental_results_refresh = Instant::now() - Duration::from_secs(3);
    response_tx
        .send(IndexResponse::Batch {
            request_id: request.request_id,
            entries: vec![
                IndexEntry {
                    path: new_z.clone(),
                    kind: EntryKind::file(),
                    kind_known: true,
                },
                IndexEntry {
                    path: new_a.clone(),
                    kind: EntryKind::file(),
                    kind_known: true,
                },
            ],
        })
        .expect("send reverse-order batch");
    app.poll_index_response();

    assert_eq!(
        app.shell.runtime.results,
        vec![(old_a, 0.0), (old_b, 0.0)],
        "incremental arrival order must not replace the sorted last-good snapshot"
    );

    response_tx
        .send(IndexResponse::Finished {
            request_id: request.request_id,
            source: IndexSource::Walker,
        })
        .expect("send terminal response");
    for _ in 0..4 {
        app.poll_index_response();
    }

    assert_eq!(
        app.shell
            .runtime
            .results
            .iter()
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>(),
        vec![new_a, new_z]
    );
    assert_eq!(app.shell.runtime.result_sort_mode, ResultSortMode::NameAsc);
    assert_eq!(
        app.shell.runtime.result_sort_scope,
        ResultSortScope::ShownResults
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_110_metadata_sort_keeps_last_good_snapshot_until_sort_worker_finishes() {
    let root = test_root("ignore-refresh-metadata-sort");
    fs::create_dir_all(&root).expect("create dir");
    let old = root.join("old.txt");
    let new_a = root.join("a.txt");
    let new_z = root.join("z.txt");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (request_tx, request_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = request_tx;
    let (response_tx, response_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = response_rx;
    let (sort_tx, sort_rx) = mpsc::channel::<SortMetadataRequest>();
    app.shell.worker_bus.sort.tx = sort_tx;
    let (sort_response_tx, sort_response_rx) = mpsc::channel::<SortMetadataResponse>();
    app.shell.worker_bus.sort.rx = sort_response_rx;
    reset_index_request_state_for_test(&mut app);
    app.shell.runtime.use_filelist = false;
    app.shell.ui.ignore_list_enabled = true;
    app.shell.runtime.ignore_list_terms = Arc::new(vec!["never-match".to_string()]);
    app.replace_results_snapshot(vec![(old.clone(), 0.0)], false);
    app.shell.runtime.set_total_match_count(7);
    app.shell.runtime.result_sort_mode = ResultSortMode::SizeDesc;
    app.shell.runtime.result_sort_scope = ResultSortScope::ShownResults;

    app.maybe_reindex_from_filter_toggles(false, false, false, true);
    let request = request_rx.try_recv().expect("index refresh request");
    response_tx
        .send(IndexResponse::Batch {
            request_id: request.request_id,
            entries: vec![
                IndexEntry {
                    path: new_z.clone(),
                    kind: EntryKind::file(),
                    kind_known: true,
                },
                IndexEntry {
                    path: new_a.clone(),
                    kind: EntryKind::file(),
                    kind_known: true,
                },
            ],
        })
        .expect("send reverse-order batch");
    response_tx
        .send(IndexResponse::Finished {
            request_id: request.request_id,
            source: IndexSource::Walker,
        })
        .expect("send terminal response");
    for _ in 0..4 {
        app.poll_index_response();
    }

    let sort_request = sort_rx.try_recv().expect("metadata sort request");
    assert_eq!(sort_request.mode, ResultSortMode::SizeDesc);
    assert_eq!(sort_request.paths, vec![new_z.clone(), new_a.clone()]);
    assert_eq!(app.shell.runtime.results, vec![(old, 0.0)]);
    assert_eq!(app.shell.runtime.total_match_count, 7);
    sort_response_tx
        .send(SortMetadataResponse {
            request_id: sort_request.request_id,
            entries: vec![
                (
                    new_z.clone(),
                    SortMetadata {
                        size_bytes: Some(2),
                        ..SortMetadata::default()
                    },
                ),
                (
                    new_a.clone(),
                    SortMetadata {
                        size_bytes: Some(1),
                        ..SortMetadata::default()
                    },
                ),
            ],
            mode: ResultSortMode::SizeDesc,
        })
        .expect("send metadata sort response");
    app.poll_sort_response();
    assert_eq!(app.shell.runtime.results, vec![(new_z, 0.0), (new_a, 0.0)]);
    assert_eq!(app.shell.runtime.total_match_count, 2);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_110_metadata_sort_worker_failure_keeps_last_good_snapshot_and_count() {
    let root = test_root("ignore-refresh-metadata-sort-worker-failure");
    fs::create_dir_all(&root).expect("create dir");
    let old = root.join("old.txt");
    let new_entry = root.join("new.txt");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (request_tx, request_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = request_tx;
    let (response_tx, response_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = response_rx;
    let (sort_tx, sort_rx) = mpsc::channel::<SortMetadataRequest>();
    drop(sort_rx);
    app.shell.worker_bus.sort.tx = sort_tx;
    reset_index_request_state_for_test(&mut app);
    app.shell.runtime.use_filelist = false;
    app.replace_results_snapshot(vec![(old.clone(), 0.0)], false);
    app.shell.runtime.set_total_match_count(7);
    app.shell.runtime.result_sort_mode = ResultSortMode::SizeDesc;
    app.shell.runtime.result_sort_scope = ResultSortScope::ShownResults;

    app.maybe_reindex_from_filter_toggles(false, false, false, true);
    let request = request_rx.try_recv().expect("index refresh request");
    response_tx
        .send(IndexResponse::ReplaceAll {
            request_id: request.request_id,
            entries: vec![IndexEntry {
                path: new_entry.clone(),
                kind: EntryKind::file(),
                kind_known: true,
            }],
        })
        .expect("send replacement index");
    response_tx
        .send(IndexResponse::Finished {
            request_id: request.request_id,
            source: IndexSource::Walker,
        })
        .expect("send terminal response");
    for _ in 0..4 {
        app.poll_index_response();
    }

    assert_eq!(app.shell.runtime.results, vec![(old, 0.0)]);
    assert_eq!(app.shell.runtime.total_match_count, 7);
    assert_eq!(app.shell.runtime.base_results, vec![(new_entry, 0.0)]);
    assert!(!app.shell.worker_bus.sort.in_progress);
    assert_eq!(app.shell.runtime.notice, "Sort worker is unavailable");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_110_non_empty_query_refresh_keeps_sort_contract() {
    let root = test_root("ignore-refresh-non-empty-query-sort");
    fs::create_dir_all(&root).expect("create dir");
    let old = root.join("old-match.txt");
    let new_a = root.join("a-match.txt");
    let new_z = root.join("z-match.txt");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "match".to_string());
    let (request_tx, request_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = request_tx;
    let (index_response_tx, index_response_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = index_response_rx;
    let (search_tx, search_request_rx) = mpsc::channel::<SearchRequest>();
    app.shell.search.tx = search_tx;
    let (search_response_tx, search_response_rx) = mpsc::channel::<SearchResponse>();
    app.shell.search.rx = search_response_rx;
    reset_index_request_state_for_test(&mut app);
    app.shell.runtime.use_filelist = false;
    app.shell.ui.ignore_list_enabled = true;
    app.shell.runtime.ignore_list_terms = Arc::new(vec!["never-match".to_string()]);
    app.replace_results_snapshot(vec![(old.clone(), 1.0)], false);
    app.shell.runtime.result_sort_mode = ResultSortMode::NameAsc;
    app.shell.runtime.result_sort_scope = ResultSortScope::AllMatches;

    app.maybe_reindex_from_filter_toggles(false, false, false, true);
    let index_request = request_rx.try_recv().expect("index refresh request");
    index_response_tx
        .send(IndexResponse::Batch {
            request_id: index_request.request_id,
            entries: vec![
                IndexEntry {
                    path: new_z.clone(),
                    kind: EntryKind::file(),
                    kind_known: true,
                },
                IndexEntry {
                    path: new_a.clone(),
                    kind: EntryKind::file(),
                    kind_known: true,
                },
            ],
        })
        .expect("send reverse-order batch");
    app.poll_index_response();

    let search_request = search_request_rx
        .try_recv()
        .expect("incremental search request");
    assert_eq!(search_request.sort_mode, ResultSortMode::NameAsc);
    assert_eq!(search_request.sort_scope, ResultSortScope::AllMatches);
    assert_eq!(app.shell.runtime.results, vec![(old, 1.0)]);

    search_response_tx
        .send(SearchResponse {
            request_id: search_request.request_id,
            results: vec![(new_a.clone(), 2.0), (new_z.clone(), 1.0)],
            total_match_count: 2,
            sort_mode: search_request.sort_mode,
            sort_scope: search_request.sort_scope,
            error: None,
        })
        .expect("send sorted search response");
    app.poll_search_response();

    assert_eq!(app.shell.runtime.results, vec![(new_a, 2.0), (new_z, 1.0)]);
    assert_eq!(app.shell.runtime.result_sort_mode, ResultSortMode::NameAsc);
    assert_eq!(
        app.shell.runtime.result_sort_scope,
        ResultSortScope::AllMatches
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_110_non_empty_metadata_sort_keeps_last_good_until_sort_response() {
    let root = test_root("ignore-refresh-non-empty-metadata-sort");
    fs::create_dir_all(&root).expect("create dir");
    let old = root.join("old-match.txt");
    let new_a = root.join("a-match.txt");
    let new_z = root.join("z-match.txt");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "match".to_string());
    let (sort_tx, sort_rx) = mpsc::channel::<SortMetadataRequest>();
    app.shell.worker_bus.sort.tx = sort_tx;
    let (sort_response_tx, sort_response_rx) = mpsc::channel::<SortMetadataResponse>();
    app.shell.worker_bus.sort.rx = sort_response_rx;
    app.replace_results_snapshot(vec![(old.clone(), 1.0)], false);
    app.shell.runtime.set_total_match_count(7);
    app.shell.runtime.result_sort_mode = ResultSortMode::SizeDesc;
    app.shell.runtime.result_sort_scope = ResultSortScope::ShownResults;
    app.shell.indexing.in_progress = true;
    app.shell.search.set_pending_request_id(Some(77));
    app.shell.search.set_in_progress(true);

    assert!(crate::app::result_reducer::apply_active_search_response(
        &mut app,
        SearchResponse {
            request_id: 77,
            results: vec![(new_z.clone(), 2.0), (new_a.clone(), 1.0)],
            total_match_count: 2,
            sort_mode: ResultSortMode::SizeDesc,
            sort_scope: ResultSortScope::ShownResults,
            error: None,
        }
    ));

    let sort_request = sort_rx.try_recv().expect("metadata sort request");
    assert_eq!(sort_request.mode, ResultSortMode::SizeDesc);
    assert_eq!(sort_request.paths, vec![new_z.clone(), new_a.clone()]);
    assert_eq!(app.shell.runtime.results, vec![(old.clone(), 1.0)]);
    assert_eq!(app.shell.runtime.total_match_count, 7);
    assert_eq!(app.shell.runtime.result_sort_mode, ResultSortMode::SizeDesc);
    assert_eq!(
        app.shell.runtime.result_sort_scope,
        ResultSortScope::ShownResults
    );
    sort_response_tx
        .send(SortMetadataResponse {
            request_id: sort_request.request_id,
            entries: vec![
                (
                    new_z.clone(),
                    SortMetadata {
                        size_bytes: Some(2),
                        ..SortMetadata::default()
                    },
                ),
                (
                    new_a.clone(),
                    SortMetadata {
                        size_bytes: Some(1),
                        ..SortMetadata::default()
                    },
                ),
            ],
            mode: ResultSortMode::SizeDesc,
        })
        .expect("send metadata sort response");
    app.poll_sort_response();

    assert_eq!(app.shell.runtime.results, vec![(new_z, 2.0), (new_a, 1.0)]);
    assert_eq!(app.shell.runtime.total_match_count, 2);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_110_empty_search_response_replaces_last_good_sorted_snapshot() {
    let root = test_root("ignore-refresh-empty-search-response");
    fs::create_dir_all(&root).expect("create dir");
    let old = root.join("old-match.txt");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "match".to_string());
    app.replace_results_snapshot(vec![(old, 1.0)], false);
    app.shell.runtime.set_total_match_count(1);
    app.shell.runtime.result_sort_mode = ResultSortMode::NameAsc;
    app.shell.runtime.result_sort_scope = ResultSortScope::ShownResults;
    app.shell.indexing.in_progress = true;
    app.shell.search.set_pending_request_id(Some(78));
    app.shell.search.set_in_progress(true);

    assert!(crate::app::result_reducer::apply_active_search_response(
        &mut app,
        SearchResponse {
            request_id: 78,
            results: Vec::new(),
            total_match_count: 0,
            sort_mode: ResultSortMode::NameAsc,
            sort_scope: ResultSortScope::ShownResults,
            error: None,
        }
    ));

    assert!(app.shell.runtime.results.is_empty());
    assert_eq!(app.shell.runtime.total_match_count, 0);
    assert_eq!(app.shell.runtime.current_row, None);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn deferred_create_filelist_refresh_takes_precedence_over_ignore_list_toggle() {
    let root = test_root("deferred-filelist-before-ignore-list-toggle");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = tx;
    reset_index_request_state_for_test(&mut app);
    app.shell.runtime.use_filelist = true;
    app.shell.indexing.pending_finish = Some(PendingActiveIndexFinish {
        request_id: 41,
        source: IndexSource::FileList(root.join("FileList.txt")),
    });

    app.request_create_filelist_walker_refresh();
    app.maybe_reindex_from_filter_toggles(false, false, false, true);

    assert!(rx.try_recv().is_err());
    app.shell.indexing.pending_finish = None;
    app.shell.indexing.build_reclaim_pending = true;
    app.retry_pending_active_index_build_reclaim();

    let request = rx
        .try_recv()
        .expect("deferred create-filelist request should be sent after reclaim");
    assert!(request.complete_walker_snapshot);
    assert!(!request.use_filelist);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn use_filelist_forces_type_filters_to_both_enabled() {
    let root = test_root("use-filelist-forces-type-filters");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = tx;
    reset_index_request_state_for_test(&mut app);
    app.shell.runtime.use_filelist = true;
    app.shell.runtime.include_files = false;
    app.shell.runtime.include_dirs = true;

    app.maybe_reindex_from_filter_toggles(true, false, false, false);

    let req = rx.try_recv().expect("index request should be sent");
    assert!(app.shell.runtime.include_files);
    assert!(app.shell.runtime.include_dirs);
    assert!(req.use_filelist);
    assert!(req.include_files);
    assert!(req.include_dirs);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn use_filelist_with_walker_source_keeps_type_filters_editable() {
    let root = test_root("use-filelist-walker-keeps-type-filters");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = tx;
    reset_index_request_state_for_test(&mut app);
    app.shell.runtime.use_filelist = true;
    app.shell.indexing.build.index.source = IndexSource::Walker;
    app.shell.runtime.include_files = false;
    app.shell.runtime.include_dirs = true;

    app.maybe_reindex_from_filter_toggles(true, false, false, false);

    let req = rx.try_recv().expect("index request should be sent");
    assert!(req.use_filelist);
    assert!(!req.include_files);
    assert!(req.include_dirs);
    assert!(!app.shell.runtime.include_files);
    assert!(app.shell.runtime.include_dirs);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn create_filelist_with_use_filelist_enabled_and_walker_source_skips_confirmation() {
    let root = test_root("filelist-use-filelist-walker-source-no-confirm");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (filelist_tx, filelist_rx) = mpsc::channel::<FileListRequest>();
    let (index_tx, index_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.worker_bus.filelist.tx = filelist_tx;
    app.shell.indexing.tx = index_tx;
    reset_index_request_state_for_test(&mut app);
    app.shell.runtime.use_filelist = true;
    app.shell.indexing.build.index.source = IndexSource::Walker;
    app.shell.indexing.in_progress = false;

    app.create_filelist();

    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_use_walker_confirmation
        .is_none());
    assert!(filelist_rx.try_recv().is_err());
    let req = index_rx
        .try_recv()
        .expect("Walker snapshot request should be sent without confirmation");
    assert_eq!(req.root, root);
    assert!(!req.use_filelist);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn dialog_arrow_keys_move_dialog_selection_not_results() {
    let root = test_root("dialog-arrow-focus");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.committed_for_test_mut().results =
        vec![(root.join("a.txt"), 0.0), (root.join("b.txt"), 0.0)];
    app.shell.runtime.committed_for_test_mut().current_row = Some(1);
    app.shell
        .features
        .filelist
        .workflow
        .pending_ancestor_confirmation = Some(PendingFileListAncestorConfirmation {
        tab_id: app.current_tab_id().expect("tab id"),
        root: root.clone(),
        prepared_request_id: 41,
    });

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::ArrowRight,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );

    assert_eq!(app.shell.runtime.current_row, Some(1));
    assert_eq!(
        app.shell.features.filelist.workflow.active_dialog,
        Some(FileListDialogKind::Ancestor)
    );
    assert_eq!(app.shell.features.filelist.workflow.active_dialog_button, 1);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn dialog_space_confirms_selected_dialog_action() {
    let root = test_root("dialog-space-confirm");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (filelist_tx, filelist_rx) = mpsc::channel::<FileListRequest>();
    app.shell.worker_bus.filelist.tx = filelist_tx;
    app.shell
        .features
        .filelist
        .workflow
        .pending_ancestor_confirmation = Some(PendingFileListAncestorConfirmation {
        tab_id: app.current_tab_id().expect("tab id"),
        root: root.clone(),
        prepared_request_id: 42,
    });

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::ArrowRight,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::Space,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );

    let req = filelist_rx
        .try_recv()
        .expect("filelist request should be sent");
    assert!(!req.propagate_to_ancestors);
    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_ancestor_confirmation
        .is_none());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn dialog_enter_confirms_without_triggering_main_window_action() {
    let root = test_root("dialog-enter-confirm");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.committed_for_test_mut().results = vec![(root.join("a.txt"), 0.0)];
    app.shell.runtime.committed_for_test_mut().current_row = Some(0);
    app.shell
        .features
        .filelist
        .workflow
        .pending_use_walker_confirmation = Some(PendingFileListUseWalkerConfirmation {
        source_tab_id: app.current_tab_id().expect("tab id"),
        root: root.clone(),
    });

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );

    assert_eq!(app.shell.tabs.len(), 1);
    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_use_walker_confirmation
        .is_none());
    assert!(app
        .shell
        .runtime
        .notice
        .contains("Preparing background Walker index"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn filelist_use_walker_dialog_text_describes_background_execution() {
    let [line1, line2] = FlistWalkerApp::filelist_use_walker_dialog_lines();

    assert!(line1.contains("Walker indexing"));
    assert!(line2.contains("現在のタブの裏"));
    assert!(!line2.contains("新規タブ"));
}

#[test]
fn tc_205_active_request_preempts_only_the_non_preferred_warm_generation() {
    let root = test_root("index-preempt-active-priority");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (request_tx, request_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = request_tx;
    let (response_tx, response_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = response_rx;
    reset_index_request_state_for_test(&mut app);
    app.create_new_tab();
    app.create_new_tab();

    let active_tab_id = app.shell.tabs.get(2).expect("tab 2").id;
    let bg_tab_a = app.shell.tabs.get(0).expect("tab 0").id;
    let bg_tab_b = app.shell.tabs.get(1).expect("tab 1").id;
    app.shell.tabs.active_tab = 2;

    app.shell.indexing.inflight_requests.insert(100);
    app.shell.indexing.inflight_requests.insert(101);
    app.shell.indexing.request_tabs.insert(100, bg_tab_a);
    app.shell.indexing.request_tabs.insert(101, bg_tab_b);
    app.shell.indexing.warm_tab_id = Some(bg_tab_b);
    app.shell.indexing.pending_queue.push_back(IndexRequest {
        request_id: 102,
        tab_id: active_tab_id,
        root: root.clone(),
        use_filelist: false,
        include_files: true,
        include_dirs: true,
        max_depth: crate::indexer::MaxDepth::unlimited(),
        follow_links: false,
        complete_walker_snapshot: false,
    });
    {
        let mut latest = app
            .shell
            .indexing
            .latest_request_ids
            .lock()
            .expect("lock latest");
        latest.insert(bg_tab_a, 100);
        latest.insert(bg_tab_b, 101);
    }

    assert!(app.preempt_background_for_active_request());

    let latest = app
        .shell
        .indexing
        .latest_request_ids
        .lock()
        .expect("lock latest");
    assert_eq!(latest.get(&bg_tab_a).copied(), Some(0));
    assert_eq!(latest.get(&bg_tab_b).copied(), Some(101));
    drop(latest);
    assert_eq!(
        app.shell
            .tabs
            .get(0)
            .expect("tab A")
            .index_state
            .lifecycle(),
        TabResourceLifecycle::Dormant
    );

    app.switch_to_tab_index(0);

    let replacement_request_id = app
        .shell
        .indexing
        .pending_request_id
        .expect("reactivation must establish a replacement request");
    assert!(app.shell.indexing.in_progress);
    assert!(app.shell.runtime.status_line.contains("Indexing..."));
    assert!(matches!(
        app.shell.indexing.lifecycle(),
        TabResourceLifecycle::Loading | TabResourceLifecycle::Refreshing
    ));
    assert!(app
        .shell
        .indexing
        .pending_queue
        .iter()
        .any(|request| request.request_id == replacement_request_id));

    response_tx
        .send(IndexResponse::Canceled { request_id: 100 })
        .expect("send old cancellation");
    app.poll_index_response();

    let dispatched = request_rx
        .try_recv()
        .expect("replacement dispatches after old terminal response");
    assert_eq!(dispatched.request_id, replacement_request_id);
    assert_eq!(dispatched.tab_id, bg_tab_a);
    assert_eq!(
        app.shell.indexing.pending_request_id,
        Some(replacement_request_id)
    );
    assert!(app.shell.indexing.in_progress);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_206_request_mailbox_preserves_control_data_terminal_order_under_full_pressure() {
    use crate::app::index_mailbox::{IndexMailboxPublishError, IndexResponseMailbox};

    let mailbox = IndexResponseMailbox::with_data_capacity(1);
    mailbox
        .try_publish(IndexResponse::Started {
            request_id: 7,
            source: IndexSource::Walker,
        })
        .expect("started control slot");
    mailbox
        .try_publish(IndexResponse::Batch {
            request_id: 7,
            entries: vec![IndexEntry {
                path: PathBuf::from("first.txt"),
                kind: EntryKind::file(),
                kind_known: true,
            }],
        })
        .expect("first data slot");
    let full = mailbox.try_publish(IndexResponse::Batch {
        request_id: 7,
        entries: vec![IndexEntry {
            path: PathBuf::from("second.txt"),
            kind: EntryKind::file(),
            kind_known: true,
        }],
    });
    assert!(matches!(full, Err(IndexMailboxPublishError::Full(_))));
    mailbox
        .try_publish(IndexResponse::Truncated {
            request_id: 7,
            limit: 1,
        })
        .expect("truncated control slot is independent from data capacity");
    mailbox
        .try_publish(IndexResponse::Finished {
            request_id: 7,
            source: IndexSource::Walker,
        })
        .expect("terminal slot is independent from data capacity");

    assert!(matches!(
        mailbox.try_recv(),
        Some(IndexResponse::Started { request_id: 7, .. })
    ));
    assert!(matches!(
        mailbox.try_recv(),
        Some(IndexResponse::Batch { request_id: 7, .. })
    ));
    assert!(matches!(
        mailbox.try_recv(),
        Some(IndexResponse::Truncated {
            request_id: 7,
            limit: 1
        })
    ));
    assert!(matches!(
        mailbox.try_recv(),
        Some(IndexResponse::Finished { request_id: 7, .. })
    ));
    assert!(mailbox.try_recv().is_none());
}

#[test]
fn stale_nonterminal_index_responses_keep_inflight_until_terminal_response() {
    let root = test_root("stale-nonterminal-keeps-inflight");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = rx;
    reset_index_request_state_for_test(&mut app);
    let request_id = 776;
    app.shell.indexing.inflight_requests.insert(request_id);

    tx.send(IndexResponse::Started {
        request_id,
        source: IndexSource::Walker,
    })
    .expect("send stale started");
    tx.send(IndexResponse::Batch {
        request_id,
        entries: vec![IndexEntry {
            path: root.join("stale.txt"),
            kind: EntryKind::file(),
            kind_known: true,
        }],
    })
    .expect("send stale batch");
    app.poll_index_response();

    assert!(app.shell.indexing.inflight_requests.contains(&request_id));

    tx.send(IndexResponse::Canceled { request_id })
        .expect("send stale terminal response");
    app.poll_index_response();

    assert!(!app.shell.indexing.inflight_requests.contains(&request_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn stale_terminal_index_response_clears_inflight_slot() {
    let root = test_root("stale-terminal-clears-inflight");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = rx;
    let stale_request_id = 777u64;
    let current_tab_id = app.current_tab_id().expect("tab id");
    app.shell.indexing.pending_request_id = Some(778);
    app.shell
        .indexing
        .inflight_requests
        .insert(stale_request_id);
    app.shell
        .indexing
        .request_tabs
        .insert(stale_request_id, current_tab_id);

    tx.send(IndexResponse::Finished {
        request_id: stale_request_id,
        source: IndexSource::Walker,
    })
    .expect("send finished");

    app.poll_index_response();

    assert!(!app
        .shell
        .indexing
        .inflight_requests
        .contains(&stale_request_id));
    assert!(!app
        .shell
        .indexing
        .request_tabs
        .contains_key(&stale_request_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn current_finished_index_response_clears_inflight_slot() {
    let root = test_root("current-finished-clears-inflight");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = rx;
    let req_id = app
        .shell
        .indexing
        .pending_request_id
        .expect("pending request");
    let tab_id = app.current_tab_id().expect("tab id");
    app.shell.indexing.request_tabs.insert(req_id, tab_id);
    app.shell.indexing.inflight_requests.insert(req_id);

    tx.send(IndexResponse::Finished {
        request_id: req_id,
        source: IndexSource::Walker,
    })
    .expect("send finished");

    app.poll_index_response();

    assert!(!app.shell.indexing.inflight_requests.contains(&req_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn stale_failed_index_response_clears_inflight_slot() {
    let root = test_root("stale-failed-clears-inflight");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = rx;
    let stale_request_id = 779u64;
    let current_tab_id = app.current_tab_id().expect("tab id");
    app.shell.indexing.pending_request_id = Some(780);
    app.shell
        .indexing
        .inflight_requests
        .insert(stale_request_id);
    app.shell
        .indexing
        .request_tabs
        .insert(stale_request_id, current_tab_id);

    tx.send(IndexResponse::Failed {
        request_id: stale_request_id,
        error: "old request".to_string(),
    })
    .expect("send failed");

    app.poll_index_response();

    assert!(!app
        .shell
        .indexing
        .inflight_requests
        .contains(&stale_request_id));
    assert!(!app
        .shell
        .indexing
        .request_tabs
        .contains_key(&stale_request_id));
    assert_eq!(app.shell.indexing.pending_request_id, Some(780));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn current_canceled_index_response_clears_active_request_state() {
    let root = test_root("current-canceled-clears-active");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = rx;
    let req_id = app
        .shell
        .indexing
        .pending_request_id
        .expect("pending request");
    let tab_id = app.current_tab_id().expect("tab id");
    app.shell.indexing.request_tabs.insert(req_id, tab_id);
    app.shell.indexing.inflight_requests.insert(req_id);

    tx.send(IndexResponse::Canceled { request_id: req_id })
        .expect("send canceled");

    app.poll_index_response();

    assert!(!app.shell.indexing.inflight_requests.contains(&req_id));
    assert_eq!(app.shell.indexing.pending_request_id, None);
    assert!(!app.shell.indexing.in_progress);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn same_tab_request_waits_until_previous_inflight_finishes() {
    let root = test_root("same-tab-inflight-serialization");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let tab_id = app.current_tab_id().expect("tab id");

    app.shell.indexing.inflight_requests.insert(1);
    app.shell.indexing.request_tabs.insert(1, tab_id);
    app.shell.indexing.pending_queue.push_back(IndexRequest {
        request_id: 2,
        tab_id,
        root: root.clone(),
        use_filelist: false,
        include_files: true,
        include_dirs: true,
        max_depth: crate::indexer::MaxDepth::unlimited(),
        follow_links: false,
        complete_walker_snapshot: false,
    });

    assert!(app.pop_next_index_request().is_none());

    app.shell.indexing.inflight_requests.remove(&1);
    let popped = app
        .pop_next_index_request()
        .expect("queued same-tab request should run");
    assert_eq!(popped.request_id, 2);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn replacement_request_keeps_real_inflight_accounting_until_terminal_response() {
    let root = test_root("replacement-keeps-real-inflight-accounting");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let tab_id = app.current_tab_id().expect("tab id");
    let old_request_id = 41;
    let replacement_request_id = 42;

    app.shell.indexing.inflight_requests.insert(old_request_id);
    app.shell
        .indexing
        .request_tabs
        .insert(old_request_id, tab_id);
    app.shell
        .indexing
        .background_states
        .insert(old_request_id, BackgroundIndexState::default());
    if let Ok(mut latest) = app.shell.indexing.latest_request_ids.lock() {
        latest.insert(tab_id, replacement_request_id);
    }

    app.enqueue_index_request(IndexRequest {
        request_id: replacement_request_id,
        tab_id,
        root: root.clone(),
        use_filelist: true,
        include_files: true,
        include_dirs: true,
        max_depth: crate::indexer::MaxDepth::unlimited(),
        follow_links: false,
        complete_walker_snapshot: false,
    });

    assert!(app
        .shell
        .indexing
        .inflight_requests
        .contains(&old_request_id));
    assert_eq!(
        app.shell.indexing.request_tabs.get(&old_request_id),
        Some(&tab_id)
    );
    assert!(app
        .shell
        .indexing
        .background_states
        .contains_key(&old_request_id));
    assert!(app.pop_next_index_request().is_none());
    assert_eq!(app.shell.indexing.pending_queue.len(), 1);
    assert_eq!(
        app.shell.indexing.pending_queue[0].request_id,
        replacement_request_id
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn pending_queue_eviction_restores_background_tab_refresh_on_reactivation() {
    let root = test_root("pending-queue-eviction-reactivation");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = tx;
    reset_index_request_state_for_test(&mut app);
    for _ in 0..5 {
        app.create_new_tab();
    }
    let background_tab_ids = (0..5)
        .map(|index| app.shell.tabs.get(index).expect("background tab").id)
        .collect::<Vec<_>>();

    for (offset, tab_id) in background_tab_ids.iter().copied().enumerate() {
        let request_id = app.shell.indexing.allocate_request_id(Some(tab_id));
        app.shell
            .tabs
            .get_mut(offset)
            .expect("background tab")
            .index_state
            .begin_index_request(request_id);
        app.enqueue_index_request(IndexRequest {
            request_id,
            tab_id,
            root: root.join(format!("tab-{offset}")),
            use_filelist: false,
            include_files: true,
            include_dirs: true,
            max_depth: crate::indexer::MaxDepth::unlimited(),
            follow_links: false,
            complete_walker_snapshot: false,
        });
    }

    let evicted_tab_id = background_tab_ids[0];
    assert_eq!(app.shell.indexing.pending_queue.len(), 4);
    assert_eq!(
        app.shell
            .tabs
            .get(0)
            .expect("evicted tab")
            .index_state
            .lifecycle(),
        TabResourceLifecycle::Dormant
    );
    assert_eq!(
        app.shell
            .tabs
            .get(0)
            .expect("evicted tab")
            .index_state
            .pending_index_request_id,
        None
    );

    app.switch_to_tab_index(0);

    let replacement_request_id = app
        .shell
        .indexing
        .pending_request_id
        .expect("reactivation replacement request");
    let dispatched = rx
        .try_recv()
        .expect("reactivated tab request must dispatch first");
    assert_eq!(dispatched.tab_id, evicted_tab_id);
    assert_eq!(dispatched.request_id, replacement_request_id);
    assert!(app.shell.indexing.in_progress);
    assert!(app.shell.runtime.status_line.contains("Indexing..."));
    assert!(matches!(
        app.shell.indexing.lifecycle(),
        TabResourceLifecycle::Loading | TabResourceLifecycle::Refreshing
    ));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_152_full_index_worker_queue_requeues_without_marking_inflight() {
    let root = test_root("tc-152-index-full-requeue");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let active_tab = app.current_tab_id().expect("active tab");
    let (tx, rx) = bounded_request_channel::<IndexRequest>(2);
    for request_id in 90..=91 {
        tx.send(IndexRequest {
            request_id,
            tab_id: request_id,
            root: root.clone(),
            use_filelist: false,
            include_files: true,
            include_dirs: true,
            max_depth: crate::indexer::MaxDepth::unlimited(),
            follow_links: false,
            complete_walker_snapshot: false,
        })
        .expect("fill worker queue");
    }
    app.shell.indexing.tx = tx;
    let queued = IndexRequest {
        request_id: 92,
        tab_id: active_tab,
        root: root.clone(),
        use_filelist: false,
        include_files: true,
        include_dirs: true,
        max_depth: crate::indexer::MaxDepth::unlimited(),
        follow_links: false,
        complete_walker_snapshot: false,
    };
    app.shell
        .indexing
        .request_tabs
        .insert(queued.request_id, active_tab);
    app.shell.indexing.pending_queue.push_back(queued);

    app.dispatch_index_queue();

    assert_eq!(app.shell.indexing.pending_queue.len(), 1);
    assert_eq!(app.shell.indexing.pending_queue[0].request_id, 92);
    assert!(!app.shell.indexing.inflight_requests.contains(&92));
    drop(rx);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_152_dispatch_keeps_coordinator_inflight_at_two() {
    let root = test_root("tc-152-index-coordinator-bound");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let active_tab = app.current_tab_id().expect("active tab");
    let (tx, rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = tx;
    app.shell.indexing.pending_queue.clear();
    app.shell.indexing.inflight_requests.clear();
    app.shell.indexing.request_tabs.clear();
    for request_id in 100..103 {
        let tab_id = if request_id == 100 {
            active_tab
        } else {
            request_id
        };
        app.shell.indexing.request_tabs.insert(request_id, tab_id);
        app.shell.indexing.pending_queue.push_back(IndexRequest {
            request_id,
            tab_id,
            root: root.clone(),
            use_filelist: false,
            include_files: true,
            include_dirs: true,
            max_depth: crate::indexer::MaxDepth::unlimited(),
            follow_links: false,
            complete_walker_snapshot: false,
        });
    }

    app.dispatch_index_queue();

    assert_eq!(app.shell.indexing.inflight_requests.len(), 2);
    assert_eq!(app.shell.indexing.pending_queue.len(), 1);
    assert_eq!(rx.try_recv().expect("first dispatched").request_id, 100);
    assert!(rx.try_recv().is_ok());
    assert!(rx.try_recv().is_err());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_152_full_index_queue_retries_after_capacity_returns_regression() {
    let root = test_root("tc-152-full-retry");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let tab_id = app.current_tab_id().expect("active tab");
    let (tx, rx) = bounded_request_channel::<IndexRequest>(2);
    for request_id in 1..=2 {
        tx.send(IndexRequest {
            request_id,
            tab_id,
            root: root.clone(),
            use_filelist: false,
            include_files: true,
            include_dirs: true,
            max_depth: crate::indexer::MaxDepth::unlimited(),
            follow_links: false,
            complete_walker_snapshot: false,
        })
        .expect("fill queue");
    }
    app.shell.indexing.tx = tx;
    app.shell.indexing.pending_queue.clear();
    app.shell.indexing.inflight_requests.clear();
    app.shell.indexing.request_tabs.clear();
    app.shell.indexing.pending_queue.push_back(IndexRequest {
        request_id: 3,
        tab_id,
        root: root.clone(),
        use_filelist: false,
        include_files: true,
        include_dirs: true,
        max_depth: crate::indexer::MaxDepth::unlimited(),
        follow_links: false,
        complete_walker_snapshot: false,
    });

    app.dispatch_index_queue();
    assert_eq!(app.shell.indexing.pending_queue.len(), 1);
    let _ = rx.try_recv().expect("free one worker slot");
    app.dispatch_index_queue();

    assert!(app.shell.indexing.pending_queue.is_empty());
    let _ = rx.try_recv().expect("remaining filler request");
    assert_eq!(rx.try_recv().expect("retried request").request_id, 3);
    assert!(app.shell.indexing.inflight_requests.contains(&3));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_152_disconnected_index_dispatch_settles_active_request_regression() {
    let root = test_root("tc-152-disconnected-settlement");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = bounded_request_channel::<IndexRequest>(2);
    drop(rx);
    app.shell.indexing.tx = tx;
    reset_index_request_state_for_test(&mut app);

    app.request_index_refresh();

    assert_eq!(app.shell.indexing.pending_request_id, None);
    assert!(!app.shell.indexing.in_progress);
    assert!(app.shell.indexing.pending_queue.is_empty());
    assert!(app.shell.indexing.request_tabs.is_empty());
    assert!(app
        .shell
        .runtime
        .notice
        .contains("Index worker is unavailable"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_152_restored_active_tab_dispatches_in_same_terminal_poll_regression() {
    let root = test_root("tc-152-restore-priority");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.create_new_tab();
    let _closed_id = app.current_tab_id().expect("tab to restore");
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Dormant);
    app.close_active_tab();
    let background_ids = [
        app.shell.tabs.get(0).expect("tab 0").id,
        app.shell.tabs.get(1).expect("tab 1").id,
    ];
    let (request_tx, request_rx) = bounded_request_channel::<IndexRequest>(2);
    let (response_tx, response_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.tx = request_tx;
    app.shell.indexing.rx = response_rx;
    app.shell.indexing.pending_queue.clear();
    app.shell.indexing.inflight_requests = [11, 12].into_iter().collect();
    for (index, (request_id, tab_id)) in [11, 12].into_iter().zip(background_ids).enumerate() {
        app.shell.indexing.request_tabs.insert(request_id, tab_id);
        let tab = app.shell.tabs.get_mut(index).expect("background tab");
        tab.index_state.pending_index_request_id = Some(request_id);
        tab.index_state.index_in_progress = true;
    }
    app.shell.indexing.next_request_id = 100;

    app.restore_recently_closed_tab();
    let restored_id = app.current_tab_id().expect("restored active tab");
    assert!(app
        .shell
        .indexing
        .pending_queue
        .iter()
        .any(|request| request.tab_id == restored_id));

    for index in 0..128 {
        response_tx
            .send(IndexResponse::Batch {
                request_id: 11,
                entries: vec![IndexEntry {
                    path: root.join(format!("stale-background-{index}.txt")),
                    kind: EntryKind::file(),
                    kind_known: true,
                }],
            })
            .expect("send stale background batch");
    }
    response_tx
        .send(IndexResponse::Canceled { request_id: 11 })
        .expect("send background terminal response");
    app.poll_index_response();

    let dispatched = request_rx.try_recv().expect("same-poll active dispatch");
    assert_eq!(dispatched.tab_id, restored_id);
    assert!(app
        .shell
        .indexing
        .inflight_requests
        .contains(&dispatched.request_id));
    let canceled_tab = app
        .shell
        .tabs
        .iter()
        .find(|tab| tab.id == background_ids[0])
        .expect("canceled background tab");
    assert_eq!(canceled_tab.index_state.pending_index_request_id, None);
    assert!(!canceled_tab.index_state.index_in_progress);
    let _ = fs::remove_dir_all(&root);
}

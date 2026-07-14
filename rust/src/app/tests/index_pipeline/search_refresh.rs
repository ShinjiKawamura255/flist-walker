use super::*;

#[test]
fn search_error_updates_notice() {
    let root = test_root("search-error-notice");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<SearchResponse>();
    app.shell.search.rx = rx;
    app.shell.search.set_pending_request_id(Some(7));
    app.shell.search.set_in_progress(true);

    tx.send(SearchResponse {
        request_id: 7,
        results: Vec::new(),
        total_match_count: 0,
        sort_mode: ResultSortMode::Score,
        sort_scope: ResultSortScope::ShownResults,
        error: Some("invalid regex '[*': syntax error".to_string()),
    })
    .expect("send search response");

    app.poll_search_response();

    assert!(!app.shell.search.in_progress());
    assert!(app.shell.runtime.notice.contains("Search failed:"));
    assert!(app.shell.runtime.notice.contains("invalid regex"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn search_response_requeues_unknown_walker_result_kind() {
    let root = test_root("search-response-requeues-unknown-kind");
    fs::create_dir_all(&root).expect("create dir");
    let link = root.join("tail.lnk");
    fs::write(&link, "shortcut").expect("write shortcut");

    let mut app = FlistWalkerApp::new(root.clone(), 50, "tail".to_string());
    app.shell.runtime.index.source = IndexSource::Walker;
    app.shell.runtime.entries = Arc::new(vec![unknown_entry(link.clone())]);
    app.shell.runtime.results.clear();
    app.shell.cache.entry_kind.clear();
    app.shell.search.set_pending_request_id(Some(71));
    app.shell.search.set_in_progress(true);
    let (tx, rx) = mpsc::channel::<SearchResponse>();
    app.shell.search.rx = rx;

    tx.send(SearchResponse {
        request_id: 71,
        results: vec![(link.clone(), 0.0)],
        total_match_count: 1,
        sort_mode: ResultSortMode::Score,
        sort_scope: ResultSortScope::ShownResults,
        error: None,
    })
    .expect("send search response");

    app.poll_search_response();

    assert_eq!(app.shell.runtime.results, vec![(link.clone(), 0.0)]);
    assert_eq!(app.find_entry_kind(&link), None);
    assert!(matches!(
        app.shell.runtime.index.source,
        IndexSource::Walker
    ));
    assert!(
        app.shell
            .indexing
            .pending_kind_paths
            .iter()
            .any(|path| path == &link)
            || app.shell.indexing.in_flight_kind_paths.contains(&link)
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn stale_search_response_is_ignored_after_index_refresh() {
    let root = test_root("stale-search-after-refresh");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "abc".to_string());
    let (search_tx, search_rx) = mpsc::channel::<SearchResponse>();
    let (index_tx, _index_rx) = mpsc::channel::<IndexRequest>();
    app.shell.search.rx = search_rx;
    app.shell.indexing.tx = index_tx;
    app.shell.search.set_pending_request_id(Some(5));
    app.shell.search.set_in_progress(true);
    app.shell.runtime.results = vec![(root.join("before.txt"), 0.0)];

    app.request_index_refresh();

    search_tx
        .send(SearchResponse {
            request_id: 5,
            results: vec![(root.join("stale.txt"), 1.0)],
            total_match_count: 1,
            sort_mode: ResultSortMode::Score,
            sort_scope: ResultSortScope::ShownResults,
            error: None,
        })
        .expect("send stale search response");

    app.poll_search_response();

    assert!(!app.shell.search.in_progress());
    assert_eq!(app.shell.search.pending_request_id(), None);
    assert_eq!(app.shell.runtime.results[0].0, root.join("before.txt"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn index_refresh_marks_search_resume_pending_for_non_empty_query() {
    let root = test_root("resume-pending-on-refresh");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "abc".to_string());
    let (index_tx, _index_rx) = mpsc::channel::<IndexRequest>();
    app.shell.indexing.tx = index_tx;

    app.request_index_refresh();

    assert!(app.shell.indexing.search_resume_pending);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn non_empty_query_resumes_search_immediately_on_first_index_batch() {
    let root = test_root("resume-first-batch");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "main".to_string());
    let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
    app.shell.indexing.tx = index_tx;
    // Use a manual search channel so the test can inspect enqueued requests.
    let (search_tx_real, search_rx_real) = mpsc::channel::<SearchRequest>();
    app.shell.search.tx = search_tx_real;

    app.request_index_refresh();
    let req = index_rx.try_recv().expect("index request should be sent");

    let (tx_idx, rx_idx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = rx_idx;
    tx_idx
        .send(IndexResponse::Batch {
            request_id: req.request_id,
            entries: vec![IndexEntry {
                path: root.join("main.rs"),
                kind: EntryKind::file(),
                kind_known: true,
            }],
        })
        .expect("send batch");

    // Simulate that normal throttle window has not elapsed yet.
    app.shell.indexing.last_incremental_results_refresh = Instant::now();
    app.poll_index_response();

    let search_req = search_rx_real
        .try_recv()
        .expect("search should resume immediately");
    assert_eq!(search_req.query, "main");
    assert_eq!(search_req.entries.len(), 1);
    assert_eq!(search_req.entries[0], root.join("main.rs"));
    assert!(!app.shell.indexing.search_resume_pending);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn filtered_out_batch_still_resumes_non_empty_query_search() {
    let root = test_root("resume-first-batch-filtered-out");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "main".to_string());
    app.shell.runtime.include_files = false;
    app.shell.runtime.include_dirs = true;
    let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
    app.shell.indexing.tx = index_tx;
    let (search_tx_real, search_rx_real) = mpsc::channel::<SearchRequest>();
    app.shell.search.tx = search_tx_real;

    app.request_index_refresh();
    let req = index_rx.try_recv().expect("index request should be sent");

    let (tx_idx, rx_idx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = rx_idx;
    tx_idx
        .send(IndexResponse::Batch {
            request_id: req.request_id,
            entries: vec![IndexEntry {
                path: root.join("main.rs"),
                kind: EntryKind::file(),
                kind_known: true,
            }],
        })
        .expect("send batch");
    app.shell.indexing.last_incremental_results_refresh = Instant::now();
    app.poll_index_response();

    let search_req = search_rx_real
        .try_recv()
        .expect("search should still resume even when batch is filtered out");
    assert!(search_req.entries.is_empty());
    assert_eq!(search_req.query, "main");
    assert!(!app.shell.indexing.search_resume_pending);
    let _ = fs::remove_dir_all(&root);
}

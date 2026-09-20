use super::*;

fn seed_results(app: &mut FlistWalkerApp, path: &Path) {
    reset_index_request_state_for_test(app);
    app.shell.ui.show_preview = false;
    let committed = app.shell.runtime.committed_for_test_mut();
    committed.results = vec![(path.to_path_buf(), 1.0)];
    committed.base_results = committed.results.clone();
    committed.current_row = Some(0);
    committed.total_match_count = 1;
    app.shell.runtime.pinned_paths.insert(path.to_path_buf());
}

#[test]
fn search_failure_disconnect_settles_all_tabs_and_preserves_last_good() {
    for failure_source in ["response", "active_send", "background_send"] {
        let scope = test_settings_scope("search-disconnect");
        let root = test_root("search-disconnect-root");
        let old = root.join("old.txt");
        let mut app = scope.app(root, 50, "old".into());
        let (request_tx, request_rx) = mpsc::channel();
        let (response_tx, response_rx) = mpsc::channel();
        app.shell.search = SearchCoordinator::new(request_tx, response_rx);
        seed_results(&mut app, &old);
        app.enqueue_search_request();
        let background_request = request_rx.try_recv().expect("accepted background request");
        app.create_new_tab();
        seed_results(&mut app, &old);
        app.shell.runtime.query_state.query = "active".into();
        app.enqueue_search_request();
        let active_request = request_rx.try_recv().expect("accepted active request");
        if failure_source != "response" {
            drop(request_rx);
            if failure_source == "active_send" {
                app.enqueue_search_request();
            } else {
                super::super::pipeline_owner::PipelineOwner::new(&mut app)
                    .enqueue_search_request_for_tab_index(0);
            }
        } else {
            drop(response_tx);
            app.poll_search_response();
        }

        assert!(
            !app.shell.search.in_progress(),
            "disconnected worker must settle active search"
        );
        assert!(app.shell.search.pending_request_id().is_none());
        assert!(app.shell.search.request_routes_for_test().is_empty());
        assert!(matches!(
            app.shell
                .search
                .route_response(background_request.request_id),
            SearchResponseRoute::Stale
        ));
        assert!(background_request.cancel.load(Ordering::Acquire));
        assert!(active_request.cancel.load(Ordering::Acquire));
        assert_eq!(app.shell.runtime.query_state.query, "active");
        assert_eq!(app.shell.runtime.results, vec![(old.clone(), 1.0)]);
        assert!(app.shell.runtime.pinned_paths.contains(&old));
        assert_eq!(app.shell.runtime.current_row, Some(0));
        let background = app.shell.tabs.get(0).expect("background tab");
        assert!(!background.search_in_progress);
        assert!(background.pending_request_id.is_none());
        assert!(background.query_state.search_error.is_some());
        assert_eq!(background.result_state.committed.total_match_count, 1);

        app.set_notice("Unrelated later notice");
        app.poll_search_response();
        assert_eq!(
            app.shell.runtime.notice, "Unrelated later notice",
            "cleanup is idempotent"
        );
        app.switch_to_tab_index(0);
        assert!(
            !app.shell.search.in_progress(),
            "tab activation must not resurrect waiting"
        );
        assert_eq!(app.shell.runtime.query_state.query, "old");
        assert_eq!(app.shell.runtime.results, vec![(old.clone(), 1.0)]);
        assert!(app.shell.runtime.pinned_paths.contains(&old));
    }
}

#[test]
fn search_failure_does_not_dispatch_more_work_to_an_unavailable_worker() {
    let scope = test_settings_scope("search-disconnect-stop");
    let mut app = scope.app(test_root("search-disconnect-stop-root"), 50, "query".into());
    let (request_tx, request_rx) = mpsc::channel();
    let (response_tx, response_rx) = mpsc::channel();
    app.shell.search = SearchCoordinator::new(request_tx, response_rx);
    drop(response_tx);
    app.poll_search_response();
    app.enqueue_search_request();
    assert!(matches!(
        request_rx.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    assert!(!app.shell.search.in_progress());
    assert!(app.shell.runtime.query_state.search_error.is_some());
}

#[test]
fn search_failure_drains_completed_response_before_disconnection() {
    let scope = test_settings_scope("search-disconnect-drain");
    let root = test_root("search-disconnect-drain-root");
    let found = root.join("found.txt");
    let mut app = scope.app(root, 50, "found".into());
    let (request_tx, request_rx) = mpsc::channel();
    let (response_tx, response_rx) = mpsc::channel();
    app.shell.search = SearchCoordinator::new(request_tx, response_rx);
    reset_index_request_state_for_test(&mut app);
    app.enqueue_search_request();
    let request = request_rx.try_recv().expect("accepted request");
    response_tx
        .send(SearchResponse {
            request_id: request.request_id,
            results: vec![(found.clone(), 2.0)],
            total_match_count: 1,
            sort_mode: ResultSortMode::Score,
            sort_scope: ResultSortScope::ShownResults,
            error: None,
        })
        .expect("completed response");
    drop(response_tx);
    app.poll_search_response();
    assert_eq!(app.shell.runtime.results, vec![(found, 2.0)]);
    assert!(app.shell.runtime.query_state.search_error.is_none());
    assert!(!app.shell.search.in_progress());
    assert!(
        !request.cancel.load(Ordering::Acquire),
        "completed requests are not canceled"
    );
}

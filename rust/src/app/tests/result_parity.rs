use super::*;

#[test]
fn active_and_background_search_publication_follow_the_same_contract() {
    for mode in [
        ResultSortMode::Score,
        ResultSortMode::NameAsc,
        ResultSortMode::ModifiedDesc,
    ] {
        for scope in [ResultSortScope::ShownResults, ResultSortScope::AllMatches] {
            for case in ["results", "empty", "failed", "metadata-ready"] {
                let root = test_root("result-publication-parity");
                fs::create_dir_all(&root).unwrap();
                let mut app = FlistWalkerApp::new(root.clone(), 50, "needle".into());
                reset_index_request_state_for_test(&mut app);
                app.create_new_tab();
                reset_index_request_state_for_test(&mut app);
                app.shell.indexing.build.index.source = IndexSource::None;
                app.shell.runtime.query_state.query = "needle".into();
                let (sort_tx, _sort_rx) = mpsc::channel();
                app.shell.worker_bus.sort.tx = sort_tx;
                let (preview_tx, _preview_rx) = mpsc::channel();
                app.shell.worker_bus.preview.tx = preview_tx;
                let previous = vec![(root.join("old-a.txt"), 4.0), (root.join("old-z.txt"), 3.0)];
                {
                    let committed = app.shell.runtime.committed_for_test_mut();
                    committed.base_results = previous.clone();
                    committed.results = previous.clone();
                    committed.total_match_count = 17;
                    committed.current_row = Some(1);
                }
                let background = app.shell.tabs.get_mut(0).unwrap();
                let tab_id = background.id;
                background.query_state.query = "needle".into();
                background.index_state.index_in_progress = false;
                background.result_state.committed.base_results = previous.clone();
                background.result_state.committed.results = previous;
                background.result_state.committed.total_match_count = 17;
                background.result_state.committed.current_row = Some(1);
                let results = if matches!(case, "empty" | "failed") {
                    Vec::new()
                } else {
                    vec![(root.join("z.txt"), 2.0), (root.join("a.txt"), 1.0)]
                };
                if case == "metadata-ready" {
                    for (index, (path, _)) in results.iter().enumerate() {
                        app.cache_sort_metadata(
                            path.clone(),
                            SortMetadata {
                                modified: Some(UNIX_EPOCH + Duration::from_secs(index as u64)),
                                ..SortMetadata::default()
                            },
                        );
                    }
                }
                let error = (case == "failed").then(|| "invalid regex".to_owned());
                let response = |request_id| SearchResponse {
                    request_id,
                    results: results.clone(),
                    total_match_count: if results.is_empty() { 0 } else { 43 },
                    sort_mode: mode,
                    sort_scope: scope,
                    error: error.clone(),
                };
                app.shell.search.set_pending_request_id(Some(900));
                app.shell.search.set_in_progress(true);
                assert!(crate::app::result_reducer::apply_active_search_response(
                    &mut app,
                    response(900)
                ));
                crate::app::result_reducer::apply_background_search_response(
                    &mut app,
                    tab_id,
                    response(901),
                );
                let background = app.shell.tabs.get(0).unwrap();
                let snapshot = &background.result_state.committed;
                let context = format!("{mode:?}/{scope:?}/{case}");
                assert_eq!(app.shell.runtime.results, snapshot.results, "{context}");
                assert_eq!(
                    app.shell.runtime.base_results, snapshot.base_results,
                    "{context}"
                );
                assert_eq!(
                    app.shell.runtime.base_results_are_score_ranked,
                    snapshot.base_results_are_score_ranked,
                    "{context}"
                );
                assert_eq!(
                    app.shell.runtime.total_match_count, snapshot.total_match_count,
                    "{context}"
                );
                assert_eq!(
                    app.shell.runtime.current_row, snapshot.current_row,
                    "{context}"
                );
                assert_eq!(
                    app.shell.runtime.query_state.search_error, background.query_state.search_error,
                    "{context}"
                );
                assert_eq!(app.shell.runtime.notice, background.notice, "{context}");
                assert_eq!(
                    app.shell.worker_bus.sort.pending_total_match_count,
                    background.result_state.pending_sorted_total_match_count,
                    "{context}"
                );
                drop(app);
                fs::remove_dir_all(root).unwrap();
            }
        }
    }
}

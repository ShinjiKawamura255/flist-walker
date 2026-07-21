use super::*;
use crate::search::SearchPrefixCache;

#[test]
fn preview_cache_is_bounded() {
    let root = test_root("preview-cache-bounded");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

    let chunk = "x".repeat(1024 * 1024);
    let count = 40usize;
    for i in 0..count {
        let path = root.join(format!("file-{i}.txt"));
        app.cache_preview(path, chunk.clone());
    }

    assert!(app.shell.cache.preview.total_bytes() <= FlistWalkerApp::PREVIEW_CACHE_MAX_BYTES);
    assert!(app.shell.cache.preview.order_len() > 0);
    assert_eq!(
        app.shell.cache.preview.len(),
        app.shell.cache.preview.order_len()
    );
    let evicted = root.join("file-0.txt");
    assert!(!app.shell.cache.preview.contains(&evicted));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn result_sort_name_can_be_applied_and_score_can_be_restored() {
    let root = test_root("result-sort-name-score");
    fs::create_dir_all(&root).expect("create dir");
    let alpha = root.join("alpha.txt");
    let beta = root.join("beta.txt");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "a".to_string());
    let base = vec![(beta.clone(), 10.0), (alpha.clone(), 9.0)];

    app.replace_results_snapshot(base.clone(), false);
    app.shell.runtime.current_row = Some(0);
    app.set_result_sort_mode(ResultSortMode::NameAsc);

    assert_eq!(app.shell.runtime.result_sort_mode, ResultSortMode::NameAsc);
    assert_eq!(app.shell.runtime.current_row, Some(0));
    assert_eq!(
        app.shell
            .runtime
            .results
            .iter()
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>(),
        vec![alpha.clone(), beta.clone()]
    );

    app.set_result_sort_mode(ResultSortMode::Score);

    assert_eq!(app.shell.runtime.result_sort_mode, ResultSortMode::Score);
    assert_eq!(app.shell.runtime.current_row, Some(0));
    assert_eq!(app.shell.runtime.results, base);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn all_matches_sort_scope_reissues_search_request_for_non_score_sort() {
    let root = test_root("result-sort-all-matches-research");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 2, "module".to_string());
    let (search_tx, search_rx) = mpsc::channel::<SearchRequest>();
    app.shell.search.tx = search_tx;
    app.shell.runtime.entries = Arc::new(vec![
        file_entry(root.join("zeta").join("module.rs")),
        file_entry(root.join("alpha").join("module.rs")),
        file_entry(root.join("beta").join("module.rs")),
    ]);
    app.replace_results_snapshot(
        app.shell
            .runtime
            .entries
            .iter()
            .take(2)
            .map(|entry| (entry.path.clone(), 0.0))
            .collect(),
        false,
    );

    app.set_result_sort_scope(ResultSortScope::AllMatches);
    app.set_result_sort_mode(ResultSortMode::NameAsc);

    let request = search_rx.try_recv().expect("all-match sort search request");
    assert_eq!(request.query, "module");
    assert_eq!(request.limit, 2);
    assert_eq!(request.sort_scope, ResultSortScope::AllMatches);
    assert_eq!(request.sort_mode, ResultSortMode::NameAsc);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn search_result_refresh_clamps_cursor_row_instead_of_following_path_regression() {
    let root = test_root("search-refresh-clamp-row");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "abc".to_string());
    app.shell.ui.show_preview = false;
    app.shell.runtime.current_row = Some(100);
    app.shell.runtime.preview = "stale".to_string();

    let results = vec![
        (root.join("first.txt"), 1.0),
        (root.join("second.txt"), 1.0),
        (root.join("third.txt"), 1.0),
    ];

    app.replace_results_snapshot(results, false);

    assert_eq!(app.shell.runtime.current_row, Some(2));
    assert!(app.shell.runtime.preview.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn search_result_refresh_does_not_auto_select_first_row_without_user_action_regression() {
    let root = test_root("search-refresh-keep-none");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "abc".to_string());
    app.shell.ui.show_preview = false;
    app.shell.runtime.current_row = None;
    app.shell.runtime.preview = "stale".to_string();

    let results = vec![
        (root.join("first.txt"), 1.0),
        (root.join("second.txt"), 1.0),
    ];

    app.replace_results_snapshot(results, false);

    assert_eq!(app.shell.runtime.current_row, None);
    assert!(app.shell.runtime.preview.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn clear_query_and_selection_restores_first_row_regression() {
    let root = test_root("clear-query-row-reset");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "abc".to_string());
    app.shell.ui.show_preview = false;
    app.shell.runtime.query_state.query = "abc".to_string();
    app.shell.runtime.current_row = Some(2);
    app.shell.runtime.preview = "stale".to_string();
    app.shell.runtime.entries = Arc::new(vec![
        unknown_entry(root.join("first.txt")),
        unknown_entry(root.join("second.txt")),
        unknown_entry(root.join("third.txt")),
    ]);
    app.shell.runtime.results = vec![
        (root.join("first.txt"), 1.0),
        (root.join("second.txt"), 1.0),
        (root.join("third.txt"), 1.0),
    ];

    app.clear_query_and_selection();

    assert!(app.shell.runtime.query_state.query.is_empty());
    assert_eq!(app.shell.runtime.current_row, Some(0));
    assert!(app.shell.runtime.preview.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn query_edit_invalidates_result_sort_and_cancels_pending_request() {
    let root = test_root("result-sort-reset-on-query-edit");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "abc".to_string());

    app.shell.runtime.result_sort_mode = ResultSortMode::ModifiedDesc;
    app.shell.worker_bus.sort.in_progress = true;
    app.shell.worker_bus.sort.pending_request_id = Some(42);

    app.mark_query_edited();

    assert_eq!(app.shell.runtime.result_sort_mode, ResultSortMode::Score);
    assert!(!app.shell.worker_bus.sort.in_progress);
    assert!(app.shell.worker_bus.sort.pending_request_id.is_none());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn created_sort_places_missing_timestamps_last() {
    let root = test_root("result-sort-created-none-last");
    fs::create_dir_all(&root).expect("create dir");
    let has_created = root.join("has-created.txt");
    let missing_created = root.join("missing-created.txt");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "a".to_string());
    let base = vec![(missing_created.clone(), 10.0), (has_created.clone(), 9.0)];

    app.replace_results_snapshot(base, false);
    app.cache_sort_metadata(
        missing_created.clone(),
        SortMetadata {
            modified: None,
            created: None,
            size_bytes: None,
        },
    );
    app.cache_sort_metadata(
        has_created.clone(),
        SortMetadata {
            modified: None,
            created: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(5)),
            size_bytes: None,
        },
    );

    app.set_result_sort_mode(ResultSortMode::CreatedDesc);

    assert_eq!(
        app.shell
            .runtime
            .results
            .iter()
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>(),
        vec![has_created, missing_created]
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn size_sort_places_folders_and_unknown_sizes_last() {
    let root = test_root("result-sort-size-folders-last");
    fs::create_dir_all(&root).expect("create dir");
    let large = root.join("large.txt");
    let small = root.join("small.txt");
    let folder = root.join("folder");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "a".to_string());

    app.replace_results_snapshot(
        vec![
            (folder.clone(), 8.0),
            (small.clone(), 7.0),
            (large.clone(), 6.0),
        ],
        false,
    );
    app.cache_sort_metadata(
        folder.clone(),
        SortMetadata {
            modified: None,
            created: None,
            size_bytes: None,
        },
    );
    app.cache_sort_metadata(
        small.clone(),
        SortMetadata {
            modified: None,
            created: None,
            size_bytes: Some(10),
        },
    );
    app.cache_sort_metadata(
        large.clone(),
        SortMetadata {
            modified: None,
            created: None,
            size_bytes: Some(100),
        },
    );

    app.set_result_sort_mode(ResultSortMode::SizeDesc);
    assert_eq!(
        app.shell
            .runtime
            .results
            .iter()
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>(),
        vec![large.clone(), small.clone(), folder.clone()]
    );

    app.set_result_sort_mode(ResultSortMode::SizeAsc);
    assert_eq!(
        app.shell
            .runtime
            .results
            .iter()
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>(),
        vec![small, large, folder]
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn sort_metadata_cache_is_bounded() {
    let root = test_root("result-sort-cache-bounded");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

    for i in 0..(FlistWalkerApp::SORT_METADATA_CACHE_MAX + 8) {
        app.cache_sort_metadata(
            root.join(format!("entry-{i}.txt")),
            SortMetadata {
                modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(i as u64)),
                created: None,
                size_bytes: None,
            },
        );
    }

    assert!(app.shell.cache.sort_metadata.len() <= FlistWalkerApp::SORT_METADATA_CACHE_MAX);
    assert!(app.shell.cache.sort_metadata.order_len() <= FlistWalkerApp::SORT_METADATA_CACHE_MAX);
    assert!(!app
        .shell
        .cache
        .sort_metadata
        .contains_public(&root.join("entry-0.txt")));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn search_prefix_cache_accepts_only_plain_single_token_queries() {
    assert!(!SearchPrefixCache::is_cacheable_query("ab"));
    assert!(SearchPrefixCache::is_cacheable_query("abc"));
    assert!(!SearchPrefixCache::is_cacheable_query("abc def"));
    assert!(!SearchPrefixCache::is_cacheable_query("abc|def"));
    assert!(!SearchPrefixCache::is_cacheable_query("'abc"));
    assert!(!SearchPrefixCache::is_cacheable_query("!abc"));
    assert!(!SearchPrefixCache::is_cacheable_query("^abc"));
    assert!(!SearchPrefixCache::is_cacheable_query("abc$"));
    assert!(SearchPrefixCache::is_safe_prefix_extension("abc", "abcd"));
    assert!(!SearchPrefixCache::is_safe_prefix_extension("abc", "ab"));
}

#[test]
fn search_prefix_cache_prefers_longest_prefix_and_evicts_old_entries() {
    let root = PathBuf::from("/tmp/cache-root");
    let entries = Arc::new(
        (0..100)
            .map(|index| Entry::unknown(root.join(format!("file-{index}"))))
            .collect(),
    );
    let mut cache = SearchPrefixCache::default();
    cache.maybe_store(&entries, &root, true, true, "abc", vec![0, 1, 2, 3]);
    cache.maybe_store(&entries, &root, true, true, "abcd", vec![1, 3]);

    let candidates = cache
        .lookup_candidates(&entries, &root, true, true, "abcde")
        .expect("cached candidates");
    assert_eq!(candidates.as_ref(), &vec![1, 3]);

    for idx in 0..(SearchPrefixCache::MAX_ENTRIES + 4) {
        cache.maybe_store(
            &entries,
            &root,
            true,
            true,
            &format!("q{:03}", idx),
            vec![idx],
        );
    }
    assert!(cache.entries.len() <= SearchPrefixCache::MAX_ENTRIES);
    assert!(cache.total_bytes <= SearchPrefixCache::MAX_BYTES);
}

#[test]
fn search_prefix_cache_skips_oversized_match_sets() {
    let root = PathBuf::from("/tmp/cache-root");
    let entries = Arc::new(vec![Entry::unknown(root.join("one"))]);
    let mut cache = SearchPrefixCache::default();
    let oversized = (0..=SearchPrefixCache::MAX_MATCHED_INDICES).collect::<Vec<_>>();

    cache.maybe_store(&entries, &root, true, true, "oversized", oversized);

    assert!(cache
        .lookup_candidates(&entries, &root, true, true, "oversizedx")
        .is_none());
    assert_eq!(cache.total_bytes, 0);
}

#[test]
fn tc_155_search_prefix_cache_isolates_semantic_scope_and_snapshot_lifetime() {
    let root = PathBuf::from("/tmp/cache-root");
    let entries = Arc::new(vec![Entry::unknown(root.join("Alpha.txt"))]);
    let mut cache = SearchPrefixCache::default();
    cache.maybe_store(&entries, &root, true, true, "alp", vec![0]);

    assert!(cache
        .lookup_candidates(&entries, &root, false, true, "alph")
        .is_none());
    assert!(cache
        .lookup_candidates(&entries, &root, true, false, "alph")
        .is_none());
    assert!(cache
        .lookup_candidates(
            &entries,
            &PathBuf::from("/tmp/other-root"),
            true,
            true,
            "alph",
        )
        .is_none());

    let replacement = Arc::new(entries.as_ref().clone());
    assert!(cache
        .lookup_candidates(&replacement, &root, true, true, "alph")
        .is_none());
    assert!(cache
        .lookup_candidates(&entries, &root, true, true, "alph")
        .is_some());
}

#[test]
fn tc_155_ignore_matcher_cache_compiles_once_per_terms_and_case_scope() {
    crate::query::reset_compile_counts();
    let mut cache = IgnoreMatcherCacheState::default();
    let terms = vec!["target".to_string()];

    let first = cache.compiled(&terms, true);
    let second = cache.compiled(&terms, true);
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(crate::query::ignore_compile_count(), 1);

    let case_sensitive = cache.compiled(&terms, false);
    assert!(!Arc::ptr_eq(&first, &case_sensitive));
    assert_eq!(crate::query::ignore_compile_count(), 2);
}

#[test]
fn tc_155_regression_highlight_scope_compiles_once_for_multiple_rows() {
    let root = PathBuf::from("/tmp/highlight-scope");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.query_state.query = "main".to_string();
    crate::query::reset_compile_counts();

    let first = app.highlight_positions_for_path_cached(&root.join("src/main.rs"), true);
    let second = app.highlight_positions_for_path_cached(&root.join("tests/main_test.rs"), true);

    assert!(!first.is_empty());
    assert!(!second.is_empty());
    assert_eq!(crate::query::query_compile_count(), 1);
}

#[test]
fn request_preview_is_skipped_when_preview_is_hidden() {
    let root = test_root("preview-hidden");
    fs::create_dir_all(&root).expect("create dir");
    let file = root.join("a.txt");
    fs::write(&file, "content").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

    app.shell.ui.show_preview = false;
    app.shell.runtime.results = vec![(file.clone(), 0.0)];
    app.shell.runtime.current_row = Some(0);
    app.set_entry_kind(&file, EntryKind::file());
    app.shell.runtime.preview = "stale preview".to_string();
    app.shell.worker_bus.preview.pending_request_id = Some(99);
    app.shell.worker_bus.preview.in_progress = true;

    app.request_preview_for_current();

    assert!(app.shell.runtime.preview.is_empty());
    assert!(!app.shell.worker_bus.preview.in_progress);
    assert!(app.shell.worker_bus.preview.pending_request_id.is_none());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn request_preview_when_hidden_keeps_post_index_kind_resolution_queue() {
    let root = test_root("preview-hidden-keeps-kind-queue");
    fs::create_dir_all(&root).expect("create dir");
    let file = root.join("a.lnk");
    fs::write(&file, "shortcut").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

    app.shell.ui.show_preview = false;
    app.shell.runtime.results = vec![(file.clone(), 0.0)];
    app.shell.runtime.current_row = Some(0);
    app.shell
        .indexing
        .pending_kind_paths
        .push_back(file.clone());
    app.shell
        .indexing
        .pending_kind_paths_set
        .insert(file.clone());
    app.shell.indexing.kind_resolution_in_progress = true;

    app.request_preview_for_current();

    assert!(app
        .shell
        .indexing
        .pending_kind_paths
        .iter()
        .any(|p| *p == file));
    assert!(app.shell.indexing.pending_kind_paths_set.contains(&file));
    assert!(app.shell.indexing.kind_resolution_in_progress);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn entry_kind_cache_survives_tab_state_roundtrip() {
    let root = test_root("entry-kind-cache-roundtrip");
    fs::create_dir_all(&root).expect("create dir");
    let path = root.join("shared.txt");
    fs::write(&path, "content").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

    app.set_entry_kind(&path, EntryKind::link(false));

    assert_eq!(app.find_entry_kind(&path), Some(EntryKind::link(false)));

    let tab_id = app.current_tab_id().expect("active tab id");
    let snapshot = app.capture_active_tab_state(tab_id);
    app.shell.cache.entry_kind.clear();
    app.apply_tab_state(&snapshot);

    assert_eq!(app.find_entry_kind(&path), Some(EntryKind::link(false)));
    let _ = fs::remove_dir_all(&root);
}

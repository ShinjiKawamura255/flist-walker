use super::*;

#[test]
fn clear_query_and_selection_clears_state() {
    let root = test_root("clear");
    fs::create_dir_all(&root).expect("create dir");
    let file = root.join("a.txt");
    fs::write(&file, "x").expect("write file");

    let mut app = FlistWalkerApp::new(root.clone(), 50, "abc".to_string());
    app.pinned_paths.insert(file.clone());
    app.current_row = Some(0);
    app.preview = "preview".to_string();

    app.clear_query_and_selection();

    assert!(app.query.is_empty());
    assert!(app.pinned_paths.is_empty());
    assert!(app.focus_query_requested);
    assert!(app.notice.contains("Cleared selection and query"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn startup_requests_query_focus() {
    let root = test_root("startup-focus");
    fs::create_dir_all(&root).expect("create dir");
    let app = FlistWalkerApp::new(root.clone(), 50, String::new());
    assert!(app.focus_query_requested);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn startup_index_request_is_bound_to_active_tab() {
    let root = test_root("startup-index-tab-binding");
    fs::create_dir_all(&root).expect("create dir");
    let app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let req_id = app.pending_index_request_id.expect("pending index request");
    let tab_id = app.current_tab_id().expect("active tab id");
    assert_eq!(app.index_request_tabs.get(&req_id).copied(), Some(tab_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn move_row_sets_scroll_tracking() {
    let root = test_root("scroll");
    fs::create_dir_all(&root).expect("create dir");
    let file1 = root.join("a.txt");
    let file2 = root.join("b.txt");
    fs::write(&file1, "x").expect("write file1");
    fs::write(&file2, "x").expect("write file2");

    let mut app = FlistWalkerApp::new(root.clone(), 50, "".to_string());
    app.results = vec![(file1, 0.0), (file2, 0.0)];
    app.current_row = Some(0);
    app.scroll_to_current = false;

    app.move_row(1);

    assert_eq!(app.current_row, Some(1));
    assert!(app.scroll_to_current);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn execute_selected_enqueues_action_request_without_sync_io() {
    let root = test_root("async-action-enqueue");
    fs::create_dir_all(&root).expect("create dir");
    let missing = root.join("missing-not-executed");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (action_tx_req, action_rx_req) = mpsc::channel::<ActionRequest>();
    let (_action_tx_res, action_rx_res) = mpsc::channel::<ActionResponse>();
    app.action_tx = action_tx_req;
    app.action_rx = action_rx_res;
    app.results = vec![(missing.clone(), 0.0)];
    app.current_row = Some(0);

    app.execute_selected();

    let req = action_rx_req
        .try_recv()
        .expect("action request should be enqueued");
    assert_eq!(req.paths, vec![missing]);
    assert!(!req.open_parent_for_files);
    assert!(app.pending_action_request_id.is_some());
    assert!(app.action_in_progress);
    assert!(!app.notice.starts_with("Action failed:"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn execute_selected_blocks_path_outside_current_root() {
    let root = test_root("action-block-outside-root");
    let outside_root = test_root("action-block-outside-root-other");
    let outside = outside_root.join("tool.exe");
    fs::create_dir_all(&root).expect("create root");
    fs::create_dir_all(outside.parent().expect("outside parent")).expect("create outside parent");
    fs::write(&outside, "x").expect("write outside file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (action_tx_req, action_rx_req) = mpsc::channel::<ActionRequest>();
    let (_action_tx_res, action_rx_res) = mpsc::channel::<ActionResponse>();
    app.action_tx = action_tx_req;
    app.action_rx = action_rx_res;
    app.results = vec![(outside.clone(), 0.0)];
    app.current_row = Some(0);

    app.execute_selected();

    assert!(
        action_rx_req.try_recv().is_err(),
        "action request must not be enqueued"
    );
    assert!(app.notice.contains("outside current root"));
    assert!(app.pending_action_request_id.is_none());
    assert!(!app.action_in_progress);
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&outside_root);
}

#[test]
fn execute_selected_allows_unc_like_path_when_under_current_root() {
    let root = PathBuf::from(r"\\server\share\workspace");
    let child = PathBuf::from(r"\\server\share\workspace\bin\tool.exe");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (action_tx_req, action_rx_req) = mpsc::channel::<ActionRequest>();
    let (_action_tx_res, action_rx_res) = mpsc::channel::<ActionResponse>();
    app.action_tx = action_tx_req;
    app.action_rx = action_rx_res;
    app.results = vec![(child.clone(), 0.0)];
    app.current_row = Some(0);

    app.execute_selected();

    let req = action_rx_req
        .try_recv()
        .expect("UNC-like child should be enqueued");
    assert_eq!(req.paths, vec![child]);
    assert!(app.pending_action_request_id.is_some());
    assert!(app.action_in_progress);
}

#[test]
fn action_target_path_for_open_in_folder_maps_file_and_directory() {
    let root = test_root("open-folder-target");
    let dir = root.join("dir");
    fs::create_dir_all(&dir).expect("create dir");
    let file = dir.join("main.rs");
    fs::write(&file, "fn main() {}").expect("write file");

    let from_file = action_target_path_for_open_in_folder(&file);
    let from_dir = action_target_path_for_open_in_folder(&dir);

    assert_eq!(from_file, dir);
    assert_eq!(from_dir, root.join("dir"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn action_targets_for_request_deduplicates_same_parent_directory() {
    let root = test_root("open-folder-target-dedup");
    let dir_a = root.join("dir-a");
    let dir_b = root.join("dir-b");
    fs::create_dir_all(&dir_a).expect("create dir a");
    fs::create_dir_all(&dir_b).expect("create dir b");
    let file_a1 = dir_a.join("main.rs");
    let file_a2 = dir_a.join("lib.rs");
    let file_b = dir_b.join("mod.rs");
    fs::write(&file_a1, "fn main() {}").expect("write file a1");
    fs::write(&file_a2, "pub fn f() {}").expect("write file a2");
    fs::write(&file_b, "pub fn g() {}").expect("write file b");

    let targets = action_targets_for_request(&[file_a1, file_a2, file_b, dir_a.clone()], true);

    assert_eq!(targets, vec![dir_a, dir_b]);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn stale_action_completion_is_ignored_by_request_id() {
    let root = test_root("stale-action-request-id");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<ActionResponse>();
    app.action_rx = rx;
    app.notice = "latest notice".to_string();
    app.pending_action_request_id = Some(2);
    app.action_in_progress = true;
    let tab_id = app.current_tab_id().expect("tab id");
    app.action_request_tabs.insert(1, tab_id);
    app.action_request_tabs.insert(2, tab_id);
    app.tabs[app.active_tab].pending_action_request_id = Some(2);
    app.tabs[app.active_tab].action_in_progress = true;

    tx.send(ActionResponse {
        request_id: 1,
        notice: "Action failed: stale".to_string(),
    })
    .expect("send stale action response");
    app.poll_action_response();

    assert_eq!(app.notice, "latest notice");
    assert_eq!(app.pending_action_request_id, Some(2));
    assert!(app.action_in_progress);

    tx.send(ActionResponse {
        request_id: 2,
        notice: "Action: latest".to_string(),
    })
    .expect("send latest action response");
    app.poll_action_response();

    assert_eq!(app.notice, "Action: latest");
    assert_eq!(app.pending_action_request_id, None);
    assert!(!app.action_in_progress);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn action_progress_label_is_shown_only_while_action_runs() {
    let root = test_root("action-progress-label");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

    assert_eq!(app.action_progress_label(), None);

    app.action_in_progress = true;
    assert_eq!(app.action_progress_label(), Some("Opening..."));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn prefer_relative_display_is_enabled_for_filelist_source() {
    let root = test_root("prefer-relative-filelist");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.index.source = IndexSource::FileList(root.join("FileList.txt"));

    assert!(app.prefer_relative_display());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn results_scroll_is_disabled_during_preview_resize() {
    assert!(!FlistWalkerApp::results_scroll_enabled(true));
}

#[test]
fn results_scroll_is_enabled_when_preview_resize_not_active() {
    assert!(FlistWalkerApp::results_scroll_enabled(false));
}

#[test]
fn regex_query_is_not_filtered_out_by_visible_match_guard() {
    let root = PathBuf::from("/tmp");
    let results = vec![(PathBuf::from("/tmp/src/main.py"), 42.0)];

    let out = filter_search_results(results, &root, "ma.*py", true, true);

    assert_eq!(out.len(), 1);
}

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

    assert!(app.preview_cache_total_bytes <= FlistWalkerApp::PREVIEW_CACHE_MAX_BYTES);
    assert!(!app.preview_cache_order.is_empty());
    assert_eq!(app.preview_cache.len(), app.preview_cache_order.len());
    let evicted = root.join("file-0.txt");
    assert!(!app.preview_cache.contains_key(&evicted));
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
    let snapshot = SearchEntriesSnapshotKey { ptr: 1, len: 100 };
    let mut cache = SearchPrefixCache::default();
    cache.maybe_store(snapshot, "abc", vec![0, 1, 2, 3]);
    cache.maybe_store(snapshot, "abcd", vec![1, 3]);

    let candidates = cache
        .lookup_candidates(snapshot, "abcde")
        .expect("cached candidates");
    assert_eq!(candidates.as_ref(), &vec![1, 3]);

    for idx in 0..(SearchPrefixCache::MAX_ENTRIES + 4) {
        cache.maybe_store(snapshot, &format!("q{:03}", idx), vec![idx]);
    }
    assert!(cache.entries.len() <= SearchPrefixCache::MAX_ENTRIES);
    assert!(cache.total_bytes <= SearchPrefixCache::MAX_BYTES);
}

#[test]
fn search_prefix_cache_skips_oversized_match_sets() {
    let snapshot = SearchEntriesSnapshotKey {
        ptr: 2,
        len: 1_000_000,
    };
    let mut cache = SearchPrefixCache::default();
    let oversized = (0..=SearchPrefixCache::MAX_MATCHED_INDICES).collect::<Vec<_>>();

    cache.maybe_store(snapshot, "oversized", oversized);

    assert!(cache.lookup_candidates(snapshot, "oversizedx").is_none());
    assert_eq!(cache.total_bytes, 0);
}

#[test]
fn request_preview_is_skipped_when_preview_is_hidden() {
    let root = test_root("preview-hidden");
    fs::create_dir_all(&root).expect("create dir");
    let file = root.join("a.txt");
    fs::write(&file, "content").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

    app.show_preview = false;
    app.results = vec![(file.clone(), 0.0)];
    app.current_row = Some(0);
    app.entry_kinds.insert(file, false);
    app.preview = "stale preview".to_string();
    app.pending_preview_request_id = Some(99);
    app.preview_in_progress = true;

    app.request_preview_for_current();

    assert!(app.preview.is_empty());
    assert!(!app.preview_in_progress);
    assert!(app.pending_preview_request_id.is_none());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn close_tab_invalidates_memory_cache_for_immediate_resample() {
    let root = test_root("close-tab-memory-resample");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    assert_eq!(app.tabs.len(), 2);

    let sentinel = u64::MAX;
    app.memory_usage_bytes = Some(sentinel);
    let stale = Instant::now()
        .checked_sub(Duration::from_secs(5))
        .unwrap_or_else(Instant::now);
    app.last_memory_sample = stale;

    app.close_tab_index(1);

    assert_eq!(app.tabs.len(), 1);
    assert_ne!(app.memory_usage_bytes, Some(sentinel));
    assert!(app.last_memory_sample > stale);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn app_defaults_use_filelist_on() {
    let root = test_root("default-use-filelist-on");
    fs::create_dir_all(&root).expect("create dir");
    let app = FlistWalkerApp::new(root.clone(), 50, String::new());
    assert!(app.use_filelist);
    let _ = fs::remove_dir_all(&root);
}

#[test]
#[cfg(target_os = "windows")]
fn clipboard_text_normalizes_extended_and_unc_paths() {
    let paths = vec![
        PathBuf::from(r"\\?\C:\Users\tester\file.txt"),
        PathBuf::from(r"\\?\UNC\server\share\folder\file.txt"),
    ];
    let text = FlistWalkerApp::clipboard_paths_text(&paths);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], r"C:\Users\tester\file.txt");
    assert_eq!(lines[1], r"\\server\share\folder\file.txt");
}

#[test]
#[cfg(target_os = "windows")]
fn copy_selected_paths_notice_normalizes_extended_prefix() {
    let root = test_root("copy-path-notice-normalize");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.results = vec![(PathBuf::from(r"\\?\C:\Users\tester\file.txt"), 0.0)];
    app.current_row = Some(0);
    let ctx = egui::Context::default();

    app.copy_selected_paths(&ctx);

    assert!(app
        .notice
        .contains(r"Copied path: C:\Users\tester\file.txt"));
    assert!(!app.notice.contains(r"\\?\"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn process_shutdown_flag_can_be_set_and_cleared() {
    clear_process_shutdown_request();
    assert!(!process_shutdown_requested());
    request_process_shutdown();
    assert!(process_shutdown_requested());
    clear_process_shutdown_request();
    assert!(!process_shutdown_requested());
}

#[test]
fn worker_runtime_join_all_with_timeout_returns_joined_when_workers_finish() {
    let shutdown = Arc::new(AtomicBool::new(false));
    let mut runtime = WorkerRuntime::new(Arc::clone(&shutdown));
    runtime.push(thread::spawn(|| {}));
    runtime.push(thread::spawn(|| {}));

    let summary = runtime.join_all_with_timeout(Duration::from_millis(500));

    assert_eq!(summary.total, 2);
    assert_eq!(summary.joined, 2);
}

#[test]
fn worker_runtime_join_all_with_timeout_returns_early_on_timeout() {
    let shutdown = Arc::new(AtomicBool::new(false));
    let mut runtime = WorkerRuntime::new(Arc::clone(&shutdown));
    runtime.push(thread::spawn(|| {
        thread::sleep(Duration::from_millis(200));
    }));

    let summary = runtime.join_all_with_timeout(Duration::from_millis(10));

    assert_eq!(summary.total, 1);
    assert_eq!(summary.joined, 0);
}

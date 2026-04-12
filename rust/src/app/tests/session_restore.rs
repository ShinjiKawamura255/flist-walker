use super::*;

fn canonical_or_self(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[test]
fn sanitize_saved_tabs_filters_missing_roots_and_clamps_active_tab() {
    let root = test_root("saved-tabs-sanitize");
    fs::create_dir_all(&root).expect("create root");
    let tabs = vec![
        SavedTabState {
            root: root.to_string_lossy().to_string(),
            use_filelist: true,
            use_regex: false,
            ignore_case: true,
            include_files: true,
            include_dirs: true,
            query: "ok".to_string(),
            query_history: Vec::new(),
            tab_accent: Some(TabAccentColor::Teal),
        },
        SavedTabState {
            root: root.join("missing").to_string_lossy().to_string(),
            use_filelist: false,
            use_regex: true,
            ignore_case: true,
            include_files: true,
            include_dirs: false,
            query: "skip".to_string(),
            query_history: Vec::new(),
            tab_accent: Some(TabAccentColor::Amber),
        },
    ];

    let (sanitized, active) =
        FlistWalkerApp::sanitize_saved_tabs(&tabs, Some(99)).expect("sanitized tabs");
    assert_eq!(sanitized.len(), 1);
    assert_eq!(active, 0);
    assert_eq!(sanitized[0].query, "ok");
    assert_eq!(sanitized[0].tab_accent, Some(TabAccentColor::Teal));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn choose_startup_root_prefers_last_root_over_default_root() {
    let fallback_root = PathBuf::from("/fallback");
    let last_root = PathBuf::from("/last");
    let default_root = PathBuf::from("/default");

    let chosen = FlistWalkerApp::choose_startup_root(
        fallback_root.clone(),
        false,
        true,
        None,
        Some(last_root.clone()),
        Some(default_root),
    );

    assert_eq!(chosen, last_root);
}

#[test]
fn choose_startup_root_prefers_default_root_when_restore_tabs_is_disabled() {
    let fallback_root = PathBuf::from("/fallback");
    let last_root = PathBuf::from("/last");
    let default_root = PathBuf::from("/default");

    let chosen = FlistWalkerApp::choose_startup_root(
        fallback_root,
        false,
        false,
        None,
        Some(last_root),
        Some(default_root.clone()),
    );

    assert_eq!(chosen, default_root);
}

#[test]
fn set_as_default_is_disabled_while_restore_tabs_env_is_enabled() {
    let root = test_root("set-default-disabled-by-restore-tabs");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    assert!(!FlistWalkerApp::can_set_current_root_as_default_with(true));

    app.set_current_root_as_default_with(true);

    assert!(app.shell.features.root_browser.default_root.is_none());
    assert!(app
        .shell
        .runtime
        .notice
        .contains("Set as default is disabled"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn set_as_default_is_enabled_when_restore_tabs_env_is_disabled() {
    let root = test_root("set-default-enabled-without-restore-tabs");
    fs::create_dir_all(&root).expect("create root");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

    assert!(FlistWalkerApp::can_set_current_root_as_default_with(false));
    app.set_current_root_as_default_with(false);

    let saved = app
        .shell
        .features
        .root_browser
        .default_root
        .as_ref()
        .expect("default root");
    assert_eq!(canonical_or_self(saved), canonical_or_self(&root));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn save_ui_state_uses_default_root_as_last_root_when_restore_tabs_is_disabled() {
    let default_root = test_root("save-ui-state-default-root");
    let current_root = test_root("save-ui-state-current-root");
    let ui_state_dir = test_root("save-ui-state-dir");
    let ui_state_path = ui_state_dir.join(".flistwalker_ui_state.json");
    fs::create_dir_all(&default_root).expect("create default root");
    fs::create_dir_all(&current_root).expect("create current root");
    fs::create_dir_all(&ui_state_dir).expect("create ui state dir");

    let mut app = FlistWalkerApp::new(current_root.clone(), 50, String::new());
    app.shell.features.root_browser.default_root = Some(default_root.clone());
    app.save_ui_state_to_path(&ui_state_path);

    let saved = FlistWalkerApp::load_ui_state_from_path(&ui_state_path);
    let saved_last_root = saved.last_root.expect("last root");
    let saved_default_root = saved.default_root.expect("default root");
    assert_eq!(
        canonical_or_self(Path::new(&saved_last_root)),
        canonical_or_self(&default_root)
    );
    assert_eq!(
        canonical_or_self(Path::new(&saved_default_root)),
        canonical_or_self(&default_root)
    );

    let _ = fs::remove_file(&ui_state_path);
    let _ = fs::remove_dir_all(&default_root);
    let _ = fs::remove_dir_all(&current_root);
    let _ = fs::remove_dir_all(&ui_state_dir);
}

#[test]
fn choose_startup_root_prefers_restored_tab_over_last_root() {
    let restored_root = PathBuf::from("/restored");
    let last_root = PathBuf::from("/last");
    let tabs = vec![SavedTabState {
        root: restored_root.to_string_lossy().to_string(),
        use_filelist: true,
        use_regex: false,
        ignore_case: true,
        include_files: true,
        include_dirs: true,
        query: String::new(),
        query_history: Vec::new(),
        tab_accent: Some(TabAccentColor::Emerald),
    }];

    let chosen = FlistWalkerApp::choose_startup_root(
        PathBuf::from("/fallback"),
        false,
        true,
        Some(&(tabs, 0)),
        Some(last_root),
        None,
    );

    assert_eq!(chosen, restored_root);
}

#[test]
fn initialize_tabs_from_saved_restores_active_tab_and_defers_background_refresh() {
    let root_a = test_root("restore-tabs-a");
    let root_b = test_root("restore-tabs-b");
    fs::create_dir_all(&root_a).expect("create root a");
    fs::create_dir_all(&root_b).expect("create root b");
    let mut app = FlistWalkerApp::new(root_a.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexRequest>();
    app.shell.indexing.tx = tx;
    reset_index_request_state_for_test(&mut app);

    app.initialize_tabs_from_saved(
        vec![
            SavedTabState {
                root: root_a.to_string_lossy().to_string(),
                use_filelist: true,
                use_regex: false,
                ignore_case: true,
                include_files: true,
                include_dirs: true,
                query: "alpha".to_string(),
                query_history: Vec::new(),
                tab_accent: Some(TabAccentColor::Azure),
            },
            SavedTabState {
                root: root_b.to_string_lossy().to_string(),
                use_filelist: false,
                use_regex: true,
                ignore_case: true,
                include_files: true,
                include_dirs: false,
                query: "beta".to_string(),
                query_history: Vec::new(),
                tab_accent: Some(TabAccentColor::Crimson),
            },
        ],
        1,
    );

    assert_eq!(app.shell.tabs.len(), 2);
    assert_eq!(app.shell.tabs.active_tab, 1);
    assert_eq!(app.shell.runtime.root, root_b);
    assert_eq!(app.shell.runtime.query_state.query, "beta");
    assert_eq!(app.shell.tabs[1].tab_accent, Some(TabAccentColor::Crimson));
    assert!(!app.shell.tabs.pending_restore_refresh);

    let req = rx.try_recv().expect("active tab refresh");
    assert_eq!(req.root, root_b);
    assert!(rx.try_recv().is_err());

    let _ = fs::remove_dir_all(&root_a);
    let _ = fs::remove_dir_all(&root_b);
}

#[test]
fn initialize_tabs_from_saved_defaults_current_row_to_first_row_regression() {
    let root = test_root("restore-tabs-default-row");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, _rx) = mpsc::channel::<IndexRequest>();
    app.shell.indexing.tx = tx;
    reset_index_request_state_for_test(&mut app);

    app.initialize_tabs_from_saved(
        vec![SavedTabState {
            root: root.to_string_lossy().to_string(),
            use_filelist: true,
            use_regex: false,
            ignore_case: true,
            include_files: true,
            include_dirs: true,
            query: String::new(),
            query_history: Vec::new(),
            tab_accent: None,
        }],
        0,
    );

    assert_eq!(app.shell.runtime.current_row, Some(0));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn switching_to_restored_background_tab_triggers_lazy_refresh() {
    let root_a = test_root("restore-tabs-switch-a");
    let root_b = test_root("restore-tabs-switch-b");
    fs::create_dir_all(&root_a).expect("create root a");
    fs::create_dir_all(&root_b).expect("create root b");
    let mut app = FlistWalkerApp::new(root_a.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexRequest>();
    app.shell.indexing.tx = tx;
    reset_index_request_state_for_test(&mut app);

    app.initialize_tabs_from_saved(
        vec![
            SavedTabState {
                root: root_a.to_string_lossy().to_string(),
                use_filelist: true,
                use_regex: false,
                ignore_case: true,
                include_files: true,
                include_dirs: true,
                query: "alpha".to_string(),
                query_history: Vec::new(),
                tab_accent: Some(TabAccentColor::Olive),
            },
            SavedTabState {
                root: root_b.to_string_lossy().to_string(),
                use_filelist: true,
                use_regex: false,
                ignore_case: true,
                include_files: true,
                include_dirs: true,
                query: "beta".to_string(),
                query_history: Vec::new(),
                tab_accent: Some(TabAccentColor::Indigo),
            },
        ],
        1,
    );
    let _ = rx.try_recv().expect("initial active refresh");

    app.switch_to_tab_index(0);

    let req = rx.try_recv().expect("background tab lazy refresh");
    assert_eq!(req.root, root_a);
    assert!(!app.shell.tabs.pending_restore_refresh);

    let _ = fs::remove_dir_all(&root_a);
    let _ = fs::remove_dir_all(&root_b);
}

#[test]
fn background_tab_activation_consumes_pending_restore_refresh_once() {
    let root_a = test_root("background-activation-consumes-pending-a");
    let root_b = test_root("background-activation-consumes-pending-b");
    fs::create_dir_all(&root_a).expect("create root a");
    fs::create_dir_all(&root_b).expect("create root b");
    let indexed_file = root_a.join("indexed.txt");
    fs::write(&indexed_file, "indexed").expect("write indexed file");

    let mut app = FlistWalkerApp::new(root_a.clone(), 50, String::new());
    let (index_req_tx, index_req_rx) = mpsc::channel::<IndexRequest>();
    let (index_res_tx, index_res_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.tx = index_req_tx;
    app.shell.indexing.rx = index_res_rx;
    reset_index_request_state_for_test(&mut app);

    app.initialize_tabs_from_saved(
        vec![
            SavedTabState {
                root: root_a.to_string_lossy().to_string(),
                use_filelist: true,
                use_regex: false,
                ignore_case: true,
                include_files: true,
                include_dirs: true,
                query: String::new(),
                query_history: Vec::new(),
                tab_accent: Some(TabAccentColor::Olive),
            },
            SavedTabState {
                root: root_b.to_string_lossy().to_string(),
                use_filelist: true,
                use_regex: false,
                ignore_case: true,
                include_files: true,
                include_dirs: true,
                query: String::new(),
                query_history: Vec::new(),
                tab_accent: Some(TabAccentColor::Indigo),
            },
        ],
        1,
    );
    let _ = index_req_rx.try_recv().expect("initial active refresh");

    let background_tab_id = app.shell.tabs[0].id;

    let (search_tx_req, _search_rx_req) = mpsc::channel::<SearchRequest>();
    let (search_tx_res, search_rx_res) = mpsc::channel::<SearchResponse>();
    app.shell.search.tx = search_tx_req;
    app.shell.search.rx = search_rx_res;
    let search_request_id = app.shell.search.allocate_request_id();
    app.shell
        .search
        .bind_request_tab(search_request_id, background_tab_id);

    let (preview_tx_req, _preview_rx_req) = mpsc::channel::<PreviewRequest>();
    let (preview_tx_res, preview_rx_res) = mpsc::channel::<PreviewResponse>();
    app.shell.worker_bus.preview.tx = preview_tx_req;
    app.shell.worker_bus.preview.rx = preview_rx_res;
    let preview_request_id = 41;
    app.bind_preview_request_to_tab(preview_request_id, background_tab_id);

    let background_index_request_id = 77;
    app.shell
        .indexing
        .request_tabs
        .insert(background_index_request_id, background_tab_id);
    app.shell.tabs[0].index_state.pending_index_request_id = Some(background_index_request_id);
    app.shell.tabs[0].index_state.index_in_progress = true;

    search_tx_res
        .send(SearchResponse {
            request_id: search_request_id,
            results: vec![(indexed_file.clone(), 9.0)],
            error: None,
        })
        .expect("send background search response");
    preview_tx_res
        .send(PreviewResponse {
            request_id: preview_request_id,
            path: indexed_file.clone(),
            preview: "preview-body".to_string(),
        })
        .expect("send background preview response");
    index_res_tx
        .send(IndexResponse::Batch {
            request_id: background_index_request_id,
            entries: vec![IndexEntry {
                path: indexed_file.clone(),
                kind: EntryKind::file(),
                kind_known: true,
            }],
        })
        .expect("send background batch");
    index_res_tx
        .send(IndexResponse::Finished {
            request_id: background_index_request_id,
            source: IndexSource::Walker,
        })
        .expect("send background finished");

    app.poll_search_response();
    app.poll_preview_response();
    app.poll_index_response();

    assert_eq!(app.shell.tabs.active_tab, 1);
    assert_eq!(app.shell.runtime.root, root_b);
    assert_eq!(app.shell.tabs[0].result_state.preview, "preview-body");
    assert_eq!(app.shell.tabs[0].index_state.entries.len(), 1);
    assert_eq!(app.shell.tabs[0].index_state.entries[0], indexed_file);

    app.switch_to_tab_index(0);

    let refresh_req = index_req_rx
        .try_recv()
        .expect("lazy refresh request for activated background tab");
    assert_eq!(refresh_req.root, root_a);
    assert!(index_req_rx.try_recv().is_err());
    assert_eq!(app.shell.tabs.active_tab, 0);
    assert_eq!(app.shell.runtime.root, root_a);
    assert!(!app.shell.tabs.pending_restore_refresh);
    assert_eq!(app.shell.runtime.preview, "preview-body");
    assert_eq!(app.shell.runtime.results.len(), 1);
    assert_eq!(app.shell.runtime.results[0].0, indexed_file);

    let _ = fs::remove_dir_all(&root_a);
    let _ = fs::remove_dir_all(&root_b);
}

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

    assert!(app.root_browser.default_root.is_none());
    assert!(app.notice.contains("Set as default is disabled"));
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
    app.root_browser.default_root = Some(default_root.clone());
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
fn history_search_hides_non_history_actions() {
    let root = test_root("history-search-action-visibility");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

    assert_eq!(
        app.top_action_labels(),
        vec![
            "Open / Execute",
            "Copy Path(s)",
            "Clear Selected",
            "Create File List",
            "Refresh Index",
        ]
    );

    app.start_history_search();

    assert_eq!(
        app.top_action_labels(),
        vec!["Apply History", "Cancel History Search"]
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn initialize_tabs_from_saved_restores_active_tab_and_defers_background_refresh() {
    let root_a = test_root("restore-tabs-a");
    let root_b = test_root("restore-tabs-b");
    fs::create_dir_all(&root_a).expect("create root a");
    fs::create_dir_all(&root_b).expect("create root b");
    let mut app = FlistWalkerApp::new(root_a.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexRequest>();
    app.indexing.tx = tx;
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

    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.active_tab, 1);
    assert_eq!(app.root, root_b);
    assert_eq!(app.query_state.query, "beta");
    assert_eq!(app.tabs[1].tab_accent, Some(TabAccentColor::Crimson));
    assert!(!app.pending_restore_refresh);
    assert!(app.tabs[0].pending_restore_refresh);
    assert!(!app.tabs[1].pending_restore_refresh);

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
    app.indexing.tx = tx;
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

    assert_eq!(app.current_row, Some(0));
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
    app.indexing.tx = tx;
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
    assert!(!app.pending_restore_refresh);
    assert!(!app.tabs[0].pending_restore_refresh);

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
    app.indexing.tx = index_req_tx;
    app.indexing.rx = index_res_rx;
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

    let background_tab_id = app.tabs[0].id;

    let (search_tx_req, _search_rx_req) = mpsc::channel::<SearchRequest>();
    let (search_tx_res, search_rx_res) = mpsc::channel::<SearchResponse>();
    app.search.tx = search_tx_req;
    app.search.rx = search_rx_res;
    let search_request_id = app.search.allocate_request_id();
    app.search
        .bind_request_tab(search_request_id, background_tab_id);

    let (preview_tx_req, _preview_rx_req) = mpsc::channel::<PreviewRequest>();
    let (preview_tx_res, preview_rx_res) = mpsc::channel::<PreviewResponse>();
    app.worker_bus.preview.tx = preview_tx_req;
    app.worker_bus.preview.rx = preview_rx_res;
    let preview_request_id = 41;
    app.bind_preview_request_to_tab(preview_request_id, background_tab_id);

    let background_index_request_id = 77;
    app.indexing
        .request_tabs
        .insert(background_index_request_id, background_tab_id);
    app.tabs[0].index_state.pending_index_request_id = Some(background_index_request_id);
    app.tabs[0].index_state.index_in_progress = true;

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

    assert_eq!(app.active_tab, 1);
    assert_eq!(app.root, root_b);
    assert!(app.tabs[0].pending_restore_refresh);
    assert_eq!(app.tabs[0].result_state.preview, "preview-body");
    assert_eq!(app.tabs[0].index_state.entries.len(), 1);
    assert_eq!(app.tabs[0].index_state.entries[0], indexed_file);

    app.switch_to_tab_index(0);

    let refresh_req = index_req_rx
        .try_recv()
        .expect("lazy refresh request for activated background tab");
    assert_eq!(refresh_req.root, root_a);
    assert!(index_req_rx.try_recv().is_err());
    assert_eq!(app.active_tab, 0);
    assert_eq!(app.root, root_a);
    assert!(!app.pending_restore_refresh);
    assert!(!app.tabs[0].pending_restore_refresh);
    assert_eq!(app.preview, "preview-body");
    assert_eq!(app.results.len(), 1);
    assert_eq!(app.results[0].0, indexed_file);

    let _ = fs::remove_dir_all(&root_a);
    let _ = fs::remove_dir_all(&root_b);
}

#[test]
fn close_tab_ignores_late_background_responses_for_removed_tab() {
    let root = test_root("close-tab-ignores-late-background");
    fs::create_dir_all(&root).expect("create dir");
    let active_file = root.join("active.txt");
    let removed_file = root.join("removed.txt");
    fs::write(&active_file, "active").expect("write active");
    fs::write(&removed_file, "removed").expect("write removed");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (index_req_tx, _index_req_rx) = mpsc::channel::<IndexRequest>();
    let (index_res_tx, index_res_rx) = mpsc::channel::<IndexResponse>();
    app.indexing.tx = index_req_tx;
    app.indexing.rx = index_res_rx;
    let (search_tx_req, _search_rx_req) = mpsc::channel::<SearchRequest>();
    let (search_tx_res, search_rx_res) = mpsc::channel::<SearchResponse>();
    app.search.tx = search_tx_req;
    app.search.rx = search_rx_res;
    let (preview_tx_req, _preview_rx_req) = mpsc::channel::<PreviewRequest>();
    let (preview_tx_res, preview_rx_res) = mpsc::channel::<PreviewResponse>();
    app.worker_bus.preview.tx = preview_tx_req;
    app.worker_bus.preview.rx = preview_rx_res;

    app.entries = Arc::new(vec![file_entry(active_file.clone())]);
    app.all_entries = Arc::new(vec![file_entry(active_file.clone())]);
    app.results = vec![(active_file.clone(), 0.0)];
    app.base_results = app.results.clone();
    app.preview = "active-preview".to_string();
    app.current_row = Some(0);
    app.sync_active_tab_state();

    app.create_new_tab();
    app.switch_to_tab_index(0);
    let removed_tab_id = app.tabs[1].id;
    let survivor_tab_id = app.tabs[0].id;

    app.search.bind_request_tab(501, removed_tab_id);
    app.bind_preview_request_to_tab(601, removed_tab_id);
    app.indexing.request_tabs.insert(701, removed_tab_id);
    app.tabs[1].index_state.pending_index_request_id = Some(701);
    app.tabs[1].index_state.index_in_progress = true;

    app.close_tab_index(1);
    let expected_preview = app.preview.clone();
    let expected_results = app.results.clone();
    let expected_entries = app.entries.clone();

    search_tx_res
        .send(SearchResponse {
            request_id: 501,
            results: vec![(removed_file.clone(), 9.0)],
            error: None,
        })
        .expect("send stale search response");
    preview_tx_res
        .send(PreviewResponse {
            request_id: 601,
            path: removed_file.clone(),
            preview: "removed-preview".to_string(),
        })
        .expect("send stale preview response");
    index_res_tx
        .send(IndexResponse::Batch {
            request_id: 701,
            entries: vec![IndexEntry {
                path: removed_file.clone(),
                kind: EntryKind::file(),
                kind_known: true,
            }],
        })
        .expect("send stale batch");
    index_res_tx
        .send(IndexResponse::Finished {
            request_id: 701,
            source: IndexSource::Walker,
        })
        .expect("send stale finished");

    app.poll_search_response();
    app.poll_preview_response();
    app.poll_index_response();

    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.tabs[0].id, survivor_tab_id);
    assert_eq!(&*app.entries, &*expected_entries);
    assert_eq!(app.results, expected_results);
    assert_eq!(app.preview, expected_preview);
    assert_eq!(app.search.take_request_tab(501), None);
    assert_eq!(app.preview_request_tab(601), None);
    assert_eq!(app.indexing.request_tabs.get(&701), None);
    assert!(!app.indexing.background_states.contains_key(&701));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn late_sort_metadata_response_is_ignored_for_removed_tab() {
    let root = test_root("late-sort-response-removed-tab");
    fs::create_dir_all(&root).expect("create dir");
    let active_file = root.join("active.txt");
    let removed_file = root.join("removed.txt");
    fs::write(&active_file, "active").expect("write active");
    fs::write(&removed_file, "removed").expect("write removed");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (_sort_tx_req, _sort_rx_req) = mpsc::channel::<SortMetadataRequest>();
    let (sort_tx_res, sort_rx_res) = mpsc::channel::<SortMetadataResponse>();
    app.worker_bus.sort.rx = sort_rx_res;

    app.entries = Arc::new(vec![file_entry(active_file.clone())]);
    app.all_entries = Arc::new(vec![file_entry(active_file.clone())]);
    app.results = vec![(active_file.clone(), 1.0)];
    app.base_results = app.results.clone();
    app.result_sort_mode = ResultSortMode::Score;
    app.current_row = Some(0);
    app.sync_active_tab_state();

    app.create_new_tab();
    app.switch_to_tab_index(0);
    let removed_tab_index = 1;
    let removed_tab_id = app.tabs[removed_tab_index].id;
    let survivor_tab_id = app.tabs[0].id;

    app.tabs[removed_tab_index].result_state.base_results = vec![(removed_file.clone(), 1.0)];
    app.tabs[removed_tab_index].result_state.results = vec![(removed_file.clone(), 1.0)];
    app.tabs[removed_tab_index].result_state.result_sort_mode = ResultSortMode::ModifiedDesc;
    app.tabs[removed_tab_index]
        .result_state
        .pending_sort_request_id = Some(801);
    app.tabs[removed_tab_index].result_state.sort_in_progress = true;
    app.bind_sort_request_to_tab(801, removed_tab_id);

    app.close_tab_index(removed_tab_index);
    let expected_results = app.results.clone();

    sort_tx_res
        .send(SortMetadataResponse {
            request_id: 801,
            mode: ResultSortMode::ModifiedDesc,
            entries: vec![(
                removed_file.clone(),
                SortMetadata {
                    modified: Some(SystemTime::UNIX_EPOCH),
                    created: Some(SystemTime::UNIX_EPOCH),
                },
            )],
        })
        .expect("send stale sort response");

    app.poll_sort_response();

    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.tabs[0].id, survivor_tab_id);
    assert_eq!(app.results, expected_results);
    assert_eq!(app.sort_request_tab(801), None);
    assert!(app.tabs.iter().all(|tab| tab.id != removed_tab_id));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ctrl_t_creates_new_tab_and_activates_it() {
    let root = test_root("shortcut-ctrl-t-new-tab");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "query".to_string());
    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.active_tab, 0);

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::T,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: gui_shortcut_modifiers(false),
        }],
    );

    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.active_tab, 1);
    assert!(app.query_state.query.is_empty());
    assert!(app.use_filelist);
    assert_eq!(app.tabs[1].tab_accent, None);
    assert!(app.ui.focus_query_requested);
    assert!(!app.ui.unfocus_query_requested);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn save_ui_state_persists_tab_accent() {
    let root = test_root("save-ui-state-tab-accent");
    let ui_state_dir = test_root("save-ui-state-tab-accent-ui");
    let ui_state_path = ui_state_dir.join(".flistwalker_ui_state.json");
    fs::create_dir_all(&root).expect("create root");
    fs::create_dir_all(&ui_state_dir).expect("create ui state dir");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.tabs[0].tab_accent = Some(TabAccentColor::Magenta);
    app.save_ui_state_to_path(&ui_state_path);

    let saved = FlistWalkerApp::load_ui_state_from_path(&ui_state_path);
    assert_eq!(saved.tabs.len(), 1);
    assert_eq!(saved.tabs[0].tab_accent, Some(TabAccentColor::Magenta));

    let _ = fs::remove_file(&ui_state_path);
    let _ = fs::remove_dir_all(&ui_state_dir);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tab_accent_palette_matches_dropsendto_slot_colors() {
    let dark_teal = TabAccentColor::Teal.palette(true);
    assert_eq!(
        dark_teal.background,
        egui::Color32::from_rgb(0x10, 0x2A, 0x30)
    );
    assert_eq!(dark_teal.border, egui::Color32::from_rgb(0x1F, 0x76, 0x7D));
    assert_eq!(
        dark_teal.foreground,
        egui::Color32::from_rgb(0xE4, 0xFD, 0xFF)
    );

    let light_magenta = TabAccentColor::Magenta.palette(false);
    assert_eq!(
        light_magenta.background,
        egui::Color32::from_rgb(0xF7, 0xE8, 0xF8)
    );
    assert_eq!(
        light_magenta.border,
        egui::Color32::from_rgb(0xD0, 0x8F, 0xD8)
    );
    assert_eq!(
        light_magenta.foreground,
        egui::Color32::from_rgb(0x5A, 0x1F, 0x60)
    );

    let dark_clear = TabAccentPalette::clear_outline(true);
    assert_eq!(
        dark_clear.background,
        egui::Color32::from_rgb(0x23, 0x27, 0x2E)
    );
    assert_eq!(dark_clear.border, egui::Color32::from_rgb(0x55, 0x5D, 0x68));

    let light_clear = TabAccentPalette::clear_outline(false);
    assert_eq!(
        light_clear.background,
        egui::Color32::from_rgb(0xF2, 0xF4, 0xF7)
    );
    assert_eq!(
        light_clear.border,
        egui::Color32::from_rgb(0xC8, 0xCF, 0xD8)
    );
}

#[test]
fn ctrl_w_closes_current_tab_and_keeps_last_tab() {
    let root = test_root("shortcut-ctrl-w-close-tab");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    assert_eq!(app.tabs.len(), 2);
    app.tabs[0].focus_query_requested = false;
    app.tabs[0].unfocus_query_requested = true;

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::W,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: gui_shortcut_modifiers(false),
        }],
    );
    assert_eq!(app.tabs.len(), 1);
    assert!(!app.ui.focus_query_requested);
    assert!(app.ui.unfocus_query_requested);

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::W,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: gui_shortcut_modifiers(false),
        }],
    );
    assert_eq!(app.tabs.len(), 1);
    assert!(app.notice.contains("Cannot close the last tab"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ctrl_tab_and_ctrl_shift_tab_switch_active_tab() {
    let root = test_root("shortcut-ctrl-tab-switch");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.create_new_tab();
    assert_eq!(app.tabs.len(), 3);
    assert_eq!(app.active_tab, 2);

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: tab_switch_shortcut_modifiers(false),
        }],
    );
    assert_eq!(app.active_tab, 0);
    assert!(app.ui.focus_query_requested);
    assert!(!app.ui.unfocus_query_requested);

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: tab_switch_shortcut_modifiers(true),
        }],
    );
    assert_eq!(app.active_tab, 2);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ctrl_number_switches_to_matching_tab_from_left() {
    let root = test_root("shortcut-ctrl-number-tab-switch");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.create_new_tab();
    app.create_new_tab();
    assert_eq!(app.tabs.len(), 4);
    assert_eq!(app.active_tab, 3);

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::Num2,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: gui_shortcut_modifiers(false),
        }],
    );

    assert_eq!(app.active_tab, 1);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ctrl_number_without_matching_tab_does_not_switch() {
    let root = test_root("shortcut-ctrl-number-no-tab");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.active_tab, 1);

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::Num3,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: gui_shortcut_modifiers(false),
        }],
    );

    assert_eq!(app.active_tab, 1);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn switching_tabs_restores_root_per_tab() {
    let root_a = test_root("tab-root-a");
    let root_b = test_root("tab-root-b");
    fs::create_dir_all(&root_a).expect("create root a");
    fs::create_dir_all(&root_b).expect("create root b");
    let mut app = FlistWalkerApp::new(root_a.clone(), 50, String::new());

    app.create_new_tab();
    app.root = root_b.clone();
    app.sync_active_tab_state();
    assert_eq!(app.active_tab, 1);

    app.switch_to_tab_index(0);
    assert_eq!(app.root, root_a);

    app.switch_to_tab_index(1);
    assert_eq!(app.root, root_b);

    let _ = fs::remove_dir_all(&root_a);
    let _ = fs::remove_dir_all(&root_b);
}

#[test]
fn switching_tabs_restores_entries_and_filters_per_tab() {
    let root = test_root("tab-entries-filters");
    fs::create_dir_all(&root).expect("create dir");
    let a = root.join("a.txt");
    let b = root.join("b.txt");
    fs::write(&a, "a").expect("write a");
    fs::write(&b, "b").expect("write b");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.entries = Arc::new(vec![unknown_entry(a.clone()), unknown_entry(b.clone())]);
    app.all_entries = Arc::new(vec![unknown_entry(a.clone()), unknown_entry(b.clone())]);
    app.include_files = true;
    app.include_dirs = true;
    app.sync_active_tab_state();

    app.create_new_tab();
    app.entries = Arc::new(vec![unknown_entry(a.clone())]);
    app.all_entries = Arc::new(vec![unknown_entry(a.clone())]);
    app.include_files = true;
    app.include_dirs = false;
    app.sync_active_tab_state();

    app.switch_to_tab_index(0);
    assert_eq!(app.entries.len(), 2);
    assert_eq!(app.all_entries.len(), 2);
    assert!(app.include_files);
    assert!(app.include_dirs);

    app.switch_to_tab_index(1);
    assert_eq!(app.entries.len(), 1);
    assert_eq!(app.all_entries.len(), 1);
    assert!(app.include_files);
    assert!(!app.include_dirs);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn move_tab_reorders_tabs_and_preserves_active_tab_identity() {
    let root_a = test_root("move-tab-root-a");
    let root_b = test_root("move-tab-root-b");
    let root_c = test_root("move-tab-root-c");
    fs::create_dir_all(&root_a).expect("create root a");
    fs::create_dir_all(&root_b).expect("create root b");
    fs::create_dir_all(&root_c).expect("create root c");
    let mut app = FlistWalkerApp::new(root_a.clone(), 50, String::new());

    app.create_new_tab();
    app.root = root_b.clone();
    app.sync_active_tab_state();
    app.create_new_tab();
    app.root = root_c.clone();
    app.sync_active_tab_state();
    assert_eq!(app.active_tab, 2);

    app.move_tab(2, 0);

    assert_eq!(app.active_tab, 0);
    assert_eq!(app.root, root_c);
    assert_eq!(app.tabs[0].root, root_c);
    assert_eq!(app.tabs[1].root, root_a);
    assert_eq!(app.tabs[2].root, root_b);

    let _ = fs::remove_dir_all(&root_a);
    let _ = fs::remove_dir_all(&root_b);
    let _ = fs::remove_dir_all(&root_c);
}

#[test]
fn move_tab_updates_active_index_when_other_tab_crosses_it() {
    let root_a = test_root("move-tab-cross-root-a");
    let root_b = test_root("move-tab-cross-root-b");
    let root_c = test_root("move-tab-cross-root-c");
    fs::create_dir_all(&root_a).expect("create root a");
    fs::create_dir_all(&root_b).expect("create root b");
    fs::create_dir_all(&root_c).expect("create root c");
    let mut app = FlistWalkerApp::new(root_a.clone(), 50, String::new());

    app.create_new_tab();
    app.root = root_b.clone();
    app.sync_active_tab_state();
    app.create_new_tab();
    app.root = root_c.clone();
    app.sync_active_tab_state();
    app.switch_to_tab_index(1);
    assert_eq!(app.root, root_b);
    assert_eq!(app.active_tab, 1);

    app.move_tab(0, 2);

    assert_eq!(app.active_tab, 0);
    assert_eq!(app.root, root_b);
    assert_eq!(app.tabs[0].root, root_b);
    assert_eq!(app.tabs[1].root, root_c);
    assert_eq!(app.tabs[2].root, root_a);

    let _ = fs::remove_dir_all(&root_a);
    let _ = fs::remove_dir_all(&root_b);
    let _ = fs::remove_dir_all(&root_c);
}

#[test]
fn move_tab_ignores_invalid_or_noop_indices() {
    let root = test_root("move-tab-ignore-invalid");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    let original_ids: Vec<u64> = app.tabs.iter().map(|tab| tab.id).collect();
    let original_active = app.active_tab;

    app.move_tab(1, 1);
    app.move_tab(99, 0);
    app.move_tab(0, 99);

    assert_eq!(
        app.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>(),
        original_ids
    );
    assert_eq!(app.active_tab, original_active);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn move_tab_preserves_per_tab_state_carryover_after_reorder() {
    let root_a = test_root("move-tab-carryover-a");
    let root_b = test_root("move-tab-carryover-b");
    let root_c = test_root("move-tab-carryover-c");
    fs::create_dir_all(&root_a).expect("create root a");
    fs::create_dir_all(&root_b).expect("create root b");
    fs::create_dir_all(&root_c).expect("create root c");
    let mut app = FlistWalkerApp::new(root_a.clone(), 50, "alpha".to_string());

    app.include_dirs = false;
    app.preview = "preview-a".to_string();
    app.ui.focus_query_requested = true;
    app.ui.unfocus_query_requested = false;
    app.sync_active_tab_state();

    app.create_new_tab();
    app.root = root_b.clone();
    app.query_state.query = "beta".to_string();
    app.include_files = false;
    app.include_dirs = true;
    app.preview = "preview-b".to_string();
    app.ui.focus_query_requested = false;
    app.ui.unfocus_query_requested = true;
    app.sync_active_tab_state();

    app.create_new_tab();
    app.root = root_c.clone();
    app.query_state.query = "gamma".to_string();
    app.include_files = true;
    app.include_dirs = true;
    app.preview = "preview-c".to_string();
    app.ui.focus_query_requested = true;
    app.ui.unfocus_query_requested = false;
    app.sync_active_tab_state();

    app.switch_to_tab_index(1);
    assert_eq!(app.root, root_b);
    assert_eq!(app.query_state.query, "beta");
    let expected_active_root = app.root.clone();
    let expected_active_query = app.query_state.query.clone();
    let expected_active_preview = app.preview.clone();
    let expected_active_include_files = app.include_files;
    let expected_active_include_dirs = app.include_dirs;
    let expected_active_focus = app.ui.focus_query_requested;
    let expected_active_unfocus = app.ui.unfocus_query_requested;
    let expected_tab_b = app.tabs[1].clone();
    let expected_tab_c = app.tabs[2].clone();
    let expected_tab_a = app.tabs[0].clone();

    app.move_tab(0, 2);

    assert_eq!(app.active_tab, 0);
    assert_eq!(app.root, expected_active_root);
    assert_eq!(app.query_state.query, expected_active_query);
    assert_eq!(app.preview, expected_active_preview);
    assert_eq!(app.include_files, expected_active_include_files);
    assert_eq!(app.include_dirs, expected_active_include_dirs);
    assert_eq!(app.ui.focus_query_requested, expected_active_focus);
    assert_eq!(app.ui.unfocus_query_requested, expected_active_unfocus);

    assert_eq!(app.tabs[0].root, expected_tab_b.root);
    assert_eq!(
        app.tabs[0].query_state.query,
        expected_tab_b.query_state.query
    );
    assert_eq!(
        app.tabs[0].result_state.preview,
        expected_tab_b.result_state.preview
    );
    assert_eq!(app.tabs[1].root, expected_tab_c.root);
    assert_eq!(
        app.tabs[1].query_state.query,
        expected_tab_c.query_state.query
    );
    assert_eq!(
        app.tabs[1].result_state.preview,
        expected_tab_c.result_state.preview
    );
    assert_eq!(app.tabs[2].root, expected_tab_a.root);
    assert_eq!(
        app.tabs[2].query_state.query,
        expected_tab_a.query_state.query
    );
    assert_eq!(
        app.tabs[2].result_state.preview,
        expected_tab_a.result_state.preview
    );

    let _ = fs::remove_dir_all(&root_a);
    let _ = fs::remove_dir_all(&root_b);
    let _ = fs::remove_dir_all(&root_c);
}

#[test]
fn move_tab_clears_drag_state_on_direct_reorder() {
    let root = test_root("move-tab-clears-drag-state");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.create_new_tab();
    app.ui.tab_drag_state = Some(TabDragState {
        source_index: 2,
        hover_index: 0,
        press_pos: egui::pos2(10.0, 12.0),
        dragging: true,
    });

    app.move_tab(2, 0);

    assert_eq!(app.ui.tab_drag_state, None);
    app.switch_to_tab_index(1);
    assert_eq!(app.ui.tab_drag_state, None);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tab_drag_below_threshold_does_not_reorder_on_release() {
    let root = test_root("tab-drag-below-threshold");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    let tab_rects = vec![
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 24.0)),
        egui::Rect::from_min_size(egui::pos2(100.0, 0.0), egui::vec2(100.0, 24.0)),
    ];
    let mut state = TabDragState {
        source_index: 0,
        hover_index: 0,
        press_pos: egui::pos2(10.0, 12.0),
        dragging: false,
    };

    let dragging =
        app.update_tab_drag_state(&mut state, &tab_rects, Some(egui::pos2(13.0, 12.0)), true);
    assert_eq!(dragging, None);
    assert!(!state.dragging);
    assert_eq!(app.ui.tab_drag_state, Some(state));

    let released =
        app.update_tab_drag_state(&mut state, &tab_rects, Some(egui::pos2(13.0, 12.0)), false);
    assert_eq!(released, None);
    assert_eq!(app.ui.tab_drag_state, None);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tab_drag_above_threshold_reorders_on_release() {
    let root = test_root("tab-drag-above-threshold");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    let tab_rects = vec![
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 24.0)),
        egui::Rect::from_min_size(egui::pos2(100.0, 0.0), egui::vec2(100.0, 24.0)),
    ];
    let mut state = TabDragState {
        source_index: 0,
        hover_index: 0,
        press_pos: egui::pos2(10.0, 12.0),
        dragging: false,
    };

    let dragging =
        app.update_tab_drag_state(&mut state, &tab_rects, Some(egui::pos2(140.0, 12.0)), true);
    assert_eq!(dragging, None);
    assert!(state.dragging);
    assert_eq!(state.hover_index, 1);
    assert_eq!(app.ui.tab_drag_state, Some(state));

    let released =
        app.update_tab_drag_state(&mut state, &tab_rects, Some(egui::pos2(140.0, 12.0)), false);
    assert_eq!(released, Some((0, 1)));
    assert_eq!(app.ui.tab_drag_state, None);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn background_tab_search_and_preview_responses_are_retained() {
    let root = test_root("background-tab-search-preview");
    fs::create_dir_all(&root).expect("create dir");
    let selected = root.join("picked.txt");
    fs::write(&selected, "hello").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "picked".to_string());
    app.indexing.in_progress = false;
    app.indexing.pending_request_id = None;
    app.entries = Arc::new(vec![file_entry(selected.clone())]);
    app.results = vec![(selected.clone(), 0.0)];
    app.current_row = Some(0);
    app.set_entry_kind(&selected, EntryKind::file());

    let (search_tx_req, _search_rx_req) = mpsc::channel::<SearchRequest>();
    let (search_tx_res, search_rx_res) = mpsc::channel::<SearchResponse>();
    app.search.tx = search_tx_req;
    app.search.rx = search_rx_res;
    app.enqueue_search_request();
    let search_request_id = app.search.pending_request_id().expect("search request id");
    let first_tab_id = app.tabs[0].id;

    let (preview_tx_req, _preview_rx_req) = mpsc::channel::<PreviewRequest>();
    let (preview_tx_res, preview_rx_res) = mpsc::channel::<PreviewResponse>();
    app.worker_bus.preview.tx = preview_tx_req;
    app.worker_bus.preview.rx = preview_rx_res;
    app.request_preview_for_current();
    let preview_request_id = app
        .worker_bus
        .preview
        .pending_request_id
        .expect("preview request id");

    app.create_new_tab();
    assert_eq!(app.active_tab, 1);

    search_tx_res
        .send(SearchResponse {
            request_id: search_request_id,
            results: vec![(selected.clone(), 9.0)],
            error: None,
        })
        .expect("send search response");
    preview_tx_res
        .send(PreviewResponse {
            request_id: preview_request_id,
            path: selected.clone(),
            preview: "preview-body".to_string(),
        })
        .expect("send preview response");
    app.poll_search_response();
    app.poll_preview_response();

    let first_tab = app
        .tabs
        .iter()
        .find(|tab| tab.id == first_tab_id)
        .expect("first tab");
    assert!(first_tab.result_state.results.is_empty());
    assert!(first_tab.result_state.results_compacted);
    assert_eq!(first_tab.result_state.base_results.len(), 1);
    assert_eq!(first_tab.result_state.base_results[0].0, selected);
    assert_eq!(first_tab.result_state.preview, "preview-body");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn background_tab_switch_does_not_stop_indexing_progress() {
    let root = test_root("background-tab-indexing-progress");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.indexing.in_progress = true;
    app.create_new_tab();

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: tab_switch_shortcut_modifiers(true),
        }],
    );

    assert!(app.indexing.in_progress);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn background_tab_index_batches_do_not_override_active_tab_entries() {
    let root = test_root("background-tab-index-isolation");
    fs::create_dir_all(&root).expect("create dir");
    let active_file = root.join("active.txt");
    let indexed_file = root.join("indexed.txt");
    fs::write(&active_file, "a").expect("write active");
    fs::write(&indexed_file, "b").expect("write indexed");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (index_req_tx, index_req_rx) = mpsc::channel::<IndexRequest>();
    app.indexing.tx = index_req_tx;
    let (index_res_tx, index_res_rx) = mpsc::channel::<IndexResponse>();
    app.indexing.rx = index_res_rx;

    app.request_index_refresh();
    let index_req = index_req_rx.try_recv().expect("index request");
    app.entries = Arc::new(vec![unknown_entry(active_file.clone())]);
    app.all_entries = Arc::new(vec![unknown_entry(active_file.clone())]);
    app.sync_active_tab_state();

    app.create_new_tab();
    assert_eq!(app.active_tab, 1);
    app.entries = Arc::new(vec![unknown_entry(active_file.clone())]);
    app.all_entries = Arc::new(vec![unknown_entry(active_file.clone())]);
    app.sync_active_tab_state();

    index_res_tx
        .send(IndexResponse::Batch {
            request_id: index_req.request_id,
            entries: vec![IndexEntry {
                path: indexed_file.clone(),
                kind: EntryKind::file(),
                kind_known: true,
            }],
        })
        .expect("send batch");
    index_res_tx
        .send(IndexResponse::Finished {
            request_id: index_req.request_id,
            source: IndexSource::Walker,
        })
        .expect("send finished");

    app.poll_index_response();

    assert_eq!(app.entries.len(), 1);
    assert_eq!(app.entries[0], active_file);

    app.switch_to_tab_index(0);
    assert_eq!(app.entries.len(), 1);
    assert_eq!(app.entries[0], indexed_file);
    assert!(!app.indexing.in_progress);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn background_tab_search_and_index_responses_do_not_override_active_results() {
    let root = test_root("background-tab-response-isolation");
    fs::create_dir_all(&root).expect("create dir");
    let active_file = root.join("active.txt");
    let background_file = root.join("background.txt");
    fs::write(&active_file, "active").expect("write active");
    fs::write(&background_file, "background").expect("write background");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (index_req_tx, index_req_rx) = mpsc::channel::<IndexRequest>();
    let (index_res_tx, index_res_rx) = mpsc::channel::<IndexResponse>();
    app.indexing.tx = index_req_tx;
    app.indexing.rx = index_res_rx;
    let (search_tx_req, _search_rx_req) = mpsc::channel::<SearchRequest>();
    let (search_tx_res, search_rx_res) = mpsc::channel::<SearchResponse>();
    app.search.tx = search_tx_req;
    app.search.rx = search_rx_res;

    app.entries = Arc::new(vec![file_entry(active_file.clone())]);
    app.all_entries = Arc::new(vec![file_entry(active_file.clone())]);
    app.results = vec![(active_file.clone(), 0.0)];
    app.base_results = app.results.clone();
    app.current_row = Some(0);
    app.sync_active_tab_state();

    app.create_new_tab();
    assert_eq!(app.active_tab, 1);
    app.entries = Arc::new(vec![file_entry(active_file.clone())]);
    app.all_entries = Arc::new(vec![file_entry(active_file.clone())]);
    app.results = vec![(active_file.clone(), 0.0)];
    app.base_results = app.results.clone();
    app.current_row = Some(0);
    app.sync_active_tab_state();

    app.switch_to_tab_index(0);
    app.query_state.query = "background".to_string();
    app.sync_active_tab_state();
    app.switch_to_tab_index(1);

    let background_tab_id = app.tabs[0].id;
    let background_index_request = IndexRequest {
        request_id: 88,
        tab_id: background_tab_id,
        root: root.clone(),
        use_filelist: true,
        include_files: true,
        include_dirs: true,
    };
    app.indexing.request_tabs.insert(88, background_tab_id);
    app.tabs[0].index_state.pending_index_request_id = Some(88);
    app.tabs[0].index_state.index_in_progress = true;
    app.search.bind_request_tab(89, background_tab_id);
    app.tabs[0].pending_request_id = Some(89);
    app.tabs[0].search_in_progress = true;

    let active_results = app.results.clone();
    let active_base_results = app.base_results.clone();
    let active_current_row = app.current_row;

    search_tx_res
        .send(SearchResponse {
            request_id: 89,
            results: vec![(background_file.clone(), 9.0)],
            error: None,
        })
        .expect("send background search response");
    index_res_tx
        .send(IndexResponse::Batch {
            request_id: background_index_request.request_id,
            entries: vec![IndexEntry {
                path: background_file.clone(),
                kind: EntryKind::file(),
                kind_known: true,
            }],
        })
        .expect("send background batch");
    index_res_tx
        .send(IndexResponse::Finished {
            request_id: background_index_request.request_id,
            source: IndexSource::Walker,
        })
        .expect("send background finished");

    app.poll_search_response();
    app.poll_index_response();

    assert_eq!(app.results, active_results);
    assert_eq!(app.base_results, active_base_results);
    assert_eq!(app.current_row, active_current_row);
    assert_eq!(app.tabs[0].result_state.base_results.len(), 1);
    assert_eq!(app.tabs[0].result_state.base_results[0].0, background_file);
    assert_eq!(app.tabs[0].index_state.entries.len(), 1);
    assert_eq!(app.tabs[0].index_state.entries[0], background_file);
    assert!(index_req_rx.try_recv().is_err());

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tab_root_label_uses_leaf_directory_name() {
    let root = PathBuf::from("/tmp/flistwalker-tab-root-label");
    assert_eq!(
        FlistWalkerApp::tab_root_label(&root),
        "flistwalker-tab-root-label"
    );
    assert_eq!(FlistWalkerApp::tab_root_label(Path::new("/")), "/");
}

#[test]
fn tab_root_label_keeps_drive_like_root() {
    assert_eq!(FlistWalkerApp::tab_root_label(Path::new("C:\\")), "C:");
    assert_eq!(FlistWalkerApp::tab_root_label(Path::new("C:")), "C:");
}

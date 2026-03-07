use super::*;

#[test]
fn sanitize_saved_tabs_filters_missing_roots_and_clamps_active_tab() {
    let root = test_root("saved-tabs-sanitize");
    fs::create_dir_all(&root).expect("create root");
    let tabs = vec![
        SavedTabState {
            root: root.to_string_lossy().to_string(),
            use_filelist: true,
            use_regex: false,
            include_files: true,
            include_dirs: true,
            query: "ok".to_string(),
            query_history: Vec::new(),
        },
        SavedTabState {
            root: root.join("missing").to_string_lossy().to_string(),
            use_filelist: false,
            use_regex: true,
            include_files: true,
            include_dirs: false,
            query: "skip".to_string(),
            query_history: Vec::new(),
        },
    ];

    let (sanitized, active) =
        FlistWalkerApp::sanitize_saved_tabs(&tabs, Some(99)).expect("sanitized tabs");
    assert_eq!(sanitized.len(), 1);
    assert_eq!(active, 0);
    assert_eq!(sanitized[0].query, "ok");
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
        None,
        Some(last_root.clone()),
        Some(default_root),
    );

    assert_eq!(chosen, last_root);
}

#[test]
fn choose_startup_root_prefers_restored_tab_over_last_root() {
    let restored_root = PathBuf::from("/restored");
    let last_root = PathBuf::from("/last");
    let tabs = vec![SavedTabState {
        root: restored_root.to_string_lossy().to_string(),
        use_filelist: true,
        use_regex: false,
        include_files: true,
        include_dirs: true,
        query: String::new(),
        query_history: Vec::new(),
    }];

    let chosen = FlistWalkerApp::choose_startup_root(
        PathBuf::from("/fallback"),
        false,
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
    app.index_tx = tx;
    reset_index_request_state_for_test(&mut app);

    app.initialize_tabs_from_saved(
        vec![
            SavedTabState {
                root: root_a.to_string_lossy().to_string(),
                use_filelist: true,
                use_regex: false,
                include_files: true,
                include_dirs: true,
                query: "alpha".to_string(),
                query_history: Vec::new(),
            },
            SavedTabState {
                root: root_b.to_string_lossy().to_string(),
                use_filelist: false,
                use_regex: true,
                include_files: true,
                include_dirs: false,
                query: "beta".to_string(),
                query_history: Vec::new(),
            },
        ],
        1,
    );

    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.active_tab, 1);
    assert_eq!(app.root, root_b);
    assert_eq!(app.query, "beta");
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
fn switching_to_restored_background_tab_triggers_lazy_refresh() {
    let root_a = test_root("restore-tabs-switch-a");
    let root_b = test_root("restore-tabs-switch-b");
    fs::create_dir_all(&root_a).expect("create root a");
    fs::create_dir_all(&root_b).expect("create root b");
    let mut app = FlistWalkerApp::new(root_a.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<IndexRequest>();
    app.index_tx = tx;
    reset_index_request_state_for_test(&mut app);

    app.initialize_tabs_from_saved(
        vec![
            SavedTabState {
                root: root_a.to_string_lossy().to_string(),
                use_filelist: true,
                use_regex: false,
                include_files: true,
                include_dirs: true,
                query: "alpha".to_string(),
                query_history: Vec::new(),
            },
            SavedTabState {
                root: root_b.to_string_lossy().to_string(),
                use_filelist: true,
                use_regex: false,
                include_files: true,
                include_dirs: true,
                query: "beta".to_string(),
                query_history: Vec::new(),
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
            pressed: true,
            repeat: false,
            modifiers: gui_shortcut_modifiers(false),
        }],
    );

    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.active_tab, 1);
    assert!(app.query.is_empty());
    assert!(app.use_filelist);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ctrl_w_closes_current_tab_and_keeps_last_tab() {
    let root = test_root("shortcut-ctrl-w-close-tab");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    assert_eq!(app.tabs.len(), 2);

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::W,
            pressed: true,
            repeat: false,
            modifiers: gui_shortcut_modifiers(false),
        }],
    );
    assert_eq!(app.tabs.len(), 1);

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::W,
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
            pressed: true,
            repeat: false,
            modifiers: gui_shortcut_modifiers(false),
        }],
    );
    assert_eq!(app.active_tab, 0);

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::Tab,
            pressed: true,
            repeat: false,
            modifiers: gui_shortcut_modifiers(true),
        }],
    );
    assert_eq!(app.active_tab, 2);
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
    app.entries = Arc::new(vec![a.clone(), b.clone()]);
    app.all_entries = Arc::new(vec![a.clone(), b.clone()]);
    app.include_files = true;
    app.include_dirs = true;
    app.sync_active_tab_state();

    app.create_new_tab();
    app.entries = Arc::new(vec![a.clone()]);
    app.all_entries = Arc::new(vec![a.clone()]);
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
fn background_tab_search_and_preview_responses_are_retained() {
    let root = test_root("background-tab-search-preview");
    fs::create_dir_all(&root).expect("create dir");
    let selected = root.join("picked.txt");
    fs::write(&selected, "hello").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "picked".to_string());
    app.entries = Arc::new(vec![selected.clone()]);
    app.results = vec![(selected.clone(), 0.0)];
    app.current_row = Some(0);
    app.entry_kinds.insert(selected.clone(), false);

    let (search_tx_req, _search_rx_req) = mpsc::channel::<SearchRequest>();
    let (search_tx_res, search_rx_res) = mpsc::channel::<SearchResponse>();
    app.search_tx = search_tx_req;
    app.search_rx = search_rx_res;
    app.enqueue_search_request();
    let search_request_id = app.pending_request_id.expect("search request id");
    let first_tab_id = app.tabs[0].id;

    let (preview_tx_req, _preview_rx_req) = mpsc::channel::<PreviewRequest>();
    let (preview_tx_res, preview_rx_res) = mpsc::channel::<PreviewResponse>();
    app.preview_tx = preview_tx_req;
    app.preview_rx = preview_rx_res;
    app.request_preview_for_current();
    let preview_request_id = app.pending_preview_request_id.expect("preview request id");

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
    assert_eq!(first_tab.results.len(), 1);
    assert_eq!(first_tab.results[0].0, selected);
    assert_eq!(first_tab.preview, "preview-body");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn background_tab_switch_does_not_stop_indexing_progress() {
    let root = test_root("background-tab-indexing-progress");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.index_in_progress = true;
    app.create_new_tab();

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::Tab,
            pressed: true,
            repeat: false,
            modifiers: gui_shortcut_modifiers(true),
        }],
    );

    assert!(app.index_in_progress);
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
    app.index_tx = index_req_tx;
    let (index_res_tx, index_res_rx) = mpsc::channel::<IndexResponse>();
    app.index_rx = index_res_rx;

    app.request_index_refresh();
    let index_req = index_req_rx.try_recv().expect("index request");
    app.entries = Arc::new(vec![active_file.clone()]);
    app.all_entries = Arc::new(vec![active_file.clone()]);
    app.sync_active_tab_state();

    app.create_new_tab();
    assert_eq!(app.active_tab, 1);
    app.entries = Arc::new(vec![active_file.clone()]);
    app.all_entries = Arc::new(vec![active_file.clone()]);
    app.sync_active_tab_state();

    index_res_tx
        .send(IndexResponse::Batch {
            request_id: index_req.request_id,
            entries: vec![IndexEntry {
                path: indexed_file.clone(),
                is_dir: false,
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
    assert!(!app.index_in_progress);

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

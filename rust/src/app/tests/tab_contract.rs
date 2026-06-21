use super::*;
use crate::app::tab_state::{AppTabState, TabIndexState, TabQueryState, TabResultState};

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
fn save_ui_state_persists_tab_accent() {
    let root = test_root("save-ui-state-tab-accent");
    let ui_state_dir = test_root("save-ui-state-tab-accent-ui");
    let ui_state_path = ui_state_dir.join(".flistwalker_ui_state.json");
    fs::create_dir_all(&root).expect("create root");
    fs::create_dir_all(&ui_state_dir).expect("create ui state dir");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.tabs.get_mut(0).expect("tab 0").tab_accent = Some(TabAccentColor::Magenta);
    app.save_ui_state_to_path(&ui_state_path);

    let saved = FlistWalkerApp::load_ui_state_from_path(&ui_state_path);
    assert_eq!(saved.tabs.len(), 1);
    assert_eq!(saved.tabs[0].tab_accent, Some(TabAccentColor::Magenta));

    let _ = fs::remove_file(&ui_state_path);
    let _ = fs::remove_dir_all(&ui_state_dir);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tab_state_contract_round_trip_pins_field_layout() {
    let root = test_root("tab-state-contract-round-trip");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "seed".to_string());
    let active_tab = app.shell.tabs.active_tab;
    app.shell
        .tabs
        .get_mut(active_tab)
        .expect("active tab")
        .tab_accent = Some(TabAccentColor::Emerald);

    let index_state = TabIndexState {
        index: IndexBuildResult {
            entries: vec![file_entry(root.join("indexed.txt"))],
            source: IndexSource::Walker,
        },
        all_entries: Arc::new(vec![file_entry(root.join("all.txt"))]),
        entries: Arc::new(vec![file_entry(root.join("visible.txt"))]),
        pending_index_request_id: Some(11),
        index_in_progress: true,
        pending_index_entries: VecDeque::new(),
        pending_index_entries_request_id: Some(12),
        pending_index_finish: Some(PendingActiveIndexFinish {
            request_id: 11,
            source: IndexSource::Walker,
        }),
        pending_kind_paths: VecDeque::from(vec![root.join("kind.txt")]),
        pending_kind_paths_set: HashSet::from([root.join("kind.txt")]),
        in_flight_kind_paths: HashSet::from([root.join("kind-in-flight.txt")]),
        kind_resolution_epoch: 9,
        kind_resolution_in_progress: true,
        incremental_filtered_entries: vec![file_entry(root.join("filtered.txt"))],
        last_incremental_results_refresh: Instant::now(),
        last_search_snapshot_len: 3,
        search_resume_pending: true,
        search_rerun_pending: false,
    };
    let query_state = TabQueryState {
        query: "tab-contract".to_string(),
        query_history: VecDeque::from(vec!["first".to_string(), "second".to_string()]),
        query_history_cursor: Some(1),
        query_history_draft: Some("draft".to_string()),
        history_search_active: true,
        history_search_query: "history".to_string(),
        history_search_original_query: "original".to_string(),
        history_search_results: vec!["second".to_string()],
        history_search_current: Some(0),
    };
    let result_state = TabResultState {
        base_results: vec![(root.join("base.txt"), 0.5)],
        results: vec![(root.join("visible.txt"), 1.0)],
        result_sort_mode: ResultSortMode::NameAsc,
        result_sort_scope: ResultSortScope::AllMatches,
        total_match_count: 12,
        pending_sort_request_id: Some(21),
        sort_in_progress: true,
        pinned_paths: HashSet::from([root.join("pinned.txt")]),
        current_row: Some(0),
        preview: "preview".to_string(),
        results_compacted: false,
    };
    let snapshot = AppTabState {
        id: 99,
        root: root.clone(),
        tab_accent: Some(TabAccentColor::Emerald),
        use_filelist: false,
        use_regex: true,
        ignore_case: true,
        include_files: false,
        include_dirs: true,
        index_state,
        query_state,
        result_state,
        notice: "contract notice".to_string(),
        pending_request_id: Some(31),
        pending_preview_request_id: Some(32),
        pending_action_request_id: Some(33),
        search_in_progress: true,
        preview_in_progress: false,
        action_in_progress: true,
    };

    snapshot.apply_shell(&mut app);
    let restored = AppTabState::from_shell(&app, snapshot.id);

    assert_eq!(app.shell.runtime.root, snapshot.root);
    assert_eq!(app.shell.runtime.use_filelist, snapshot.use_filelist);
    assert_eq!(app.shell.runtime.use_regex, snapshot.use_regex);
    assert_eq!(app.shell.runtime.ignore_case, snapshot.ignore_case);
    assert_eq!(app.shell.runtime.include_files, snapshot.include_files);
    assert_eq!(app.shell.runtime.include_dirs, snapshot.include_dirs);
    assert_eq!(app.shell.runtime.notice, snapshot.notice);
    assert_eq!(
        app.shell.search.pending_request_id(),
        snapshot.pending_request_id
    );
    assert_eq!(
        app.shell.worker_bus.preview.pending_request_id,
        snapshot.pending_preview_request_id
    );
    assert_eq!(
        app.shell.worker_bus.action.pending_request_id,
        snapshot.pending_action_request_id
    );
    assert_eq!(app.shell.search.in_progress(), snapshot.search_in_progress);
    assert_eq!(
        app.shell.worker_bus.preview.in_progress,
        snapshot.preview_in_progress
    );
    assert_eq!(
        app.shell.worker_bus.action.in_progress,
        snapshot.action_in_progress
    );

    assert_eq!(restored.id, snapshot.id);
    assert_eq!(restored.root, snapshot.root);
    assert_eq!(restored.tab_accent, snapshot.tab_accent);
    assert_eq!(restored.use_filelist, snapshot.use_filelist);
    assert_eq!(restored.use_regex, snapshot.use_regex);
    assert_eq!(restored.ignore_case, snapshot.ignore_case);
    assert_eq!(restored.include_files, snapshot.include_files);
    assert_eq!(restored.include_dirs, snapshot.include_dirs);
    assert_eq!(
        restored.index_state.pending_index_request_id,
        snapshot.index_state.pending_index_request_id
    );
    assert_eq!(
        restored.index_state.index_in_progress,
        snapshot.index_state.index_in_progress
    );
    assert_eq!(
        restored.index_state.pending_index_entries_request_id,
        snapshot.index_state.pending_index_entries_request_id
    );
    assert_eq!(
        restored
            .index_state
            .pending_index_finish
            .as_ref()
            .map(|finish| finish.request_id),
        snapshot
            .index_state
            .pending_index_finish
            .as_ref()
            .map(|finish| finish.request_id)
    );
    assert_eq!(
        restored.index_state.kind_resolution_epoch,
        snapshot.index_state.kind_resolution_epoch
    );
    assert_eq!(
        restored.index_state.kind_resolution_in_progress,
        snapshot.index_state.kind_resolution_in_progress
    );
    assert_eq!(
        restored.index_state.last_search_snapshot_len,
        snapshot.index_state.last_search_snapshot_len
    );
    assert_eq!(
        restored.index_state.search_resume_pending,
        snapshot.index_state.search_resume_pending
    );
    assert_eq!(
        restored.index_state.search_rerun_pending,
        snapshot.index_state.search_rerun_pending
    );
    assert_eq!(restored.query_state.query, snapshot.query_state.query);
    assert_eq!(
        restored.query_state.query_history_cursor,
        snapshot.query_state.query_history_cursor
    );
    assert_eq!(
        restored.query_state.query_history_draft,
        snapshot.query_state.query_history_draft
    );
    assert_eq!(
        restored.query_state.history_search_active,
        snapshot.query_state.history_search_active
    );
    assert_eq!(
        restored.query_state.history_search_query,
        snapshot.query_state.history_search_query
    );
    assert_eq!(
        restored.query_state.history_search_original_query,
        snapshot.query_state.history_search_original_query
    );
    assert_eq!(
        restored.query_state.history_search_results,
        snapshot.query_state.history_search_results
    );
    assert_eq!(
        restored.query_state.history_search_current,
        snapshot.query_state.history_search_current
    );
    assert_eq!(
        restored.result_state.result_sort_mode,
        snapshot.result_state.result_sort_mode
    );
    assert_eq!(
        restored.result_state.result_sort_scope,
        snapshot.result_state.result_sort_scope
    );
    assert_eq!(
        restored.result_state.total_match_count,
        snapshot.result_state.total_match_count
    );
    assert_eq!(
        restored.result_state.pending_sort_request_id,
        snapshot.result_state.pending_sort_request_id
    );
    assert_eq!(
        restored.result_state.sort_in_progress,
        snapshot.result_state.sort_in_progress
    );
    assert_eq!(
        restored.result_state.current_row,
        snapshot.result_state.current_row
    );
    assert_eq!(restored.result_state.preview, snapshot.result_state.preview);
    assert_eq!(restored.notice, snapshot.notice);
    assert_eq!(restored.pending_request_id, snapshot.pending_request_id);
    assert_eq!(
        restored.pending_preview_request_id,
        snapshot.pending_preview_request_id
    );
    assert_eq!(
        restored.pending_action_request_id,
        snapshot.pending_action_request_id
    );
    assert_eq!(restored.search_in_progress, snapshot.search_in_progress);
    assert_eq!(restored.preview_in_progress, snapshot.preview_in_progress);
    assert_eq!(restored.action_in_progress, snapshot.action_in_progress);

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

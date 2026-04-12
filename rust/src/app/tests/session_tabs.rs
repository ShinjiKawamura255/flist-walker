use super::*;
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
fn ctrl_t_creates_new_tab_and_activates_it() {
    let root = test_root("shortcut-ctrl-t-new-tab");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "query".to_string());
    assert_eq!(app.shell.tabs.len(), 1);
    assert_eq!(app.shell.tabs.active_tab, 0);

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

    assert_eq!(app.shell.tabs.len(), 2);
    assert_eq!(app.shell.tabs.active_tab, 1);
    assert!(app.shell.runtime.query_state.query.is_empty());
    assert!(app.shell.runtime.use_filelist);
    assert_eq!(app.shell.tabs[1].tab_accent, None);
    assert!(app.shell.ui.focus_query_requested);
    assert!(!app.shell.ui.unfocus_query_requested);
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
    app.shell.tabs[0].tab_accent = Some(TabAccentColor::Magenta);
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
    assert_eq!(app.shell.tabs.len(), 2);
    app.shell.tabs[0].focus_query_requested = false;
    app.shell.tabs[0].unfocus_query_requested = true;

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
    assert_eq!(app.shell.tabs.len(), 1);
    assert!(!app.shell.ui.focus_query_requested);
    assert!(app.shell.ui.unfocus_query_requested);

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
    assert_eq!(app.shell.tabs.len(), 1);
    assert!(app
        .shell
        .runtime
        .notice
        .contains("Cannot close the last tab"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ctrl_tab_and_ctrl_shift_tab_switch_active_tab() {
    let root = test_root("shortcut-ctrl-tab-switch");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.create_new_tab();
    assert_eq!(app.shell.tabs.len(), 3);
    assert_eq!(app.shell.tabs.active_tab, 2);

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
    assert_eq!(app.shell.tabs.active_tab, 0);
    assert!(app.shell.ui.focus_query_requested);
    assert!(!app.shell.ui.unfocus_query_requested);

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
    assert_eq!(app.shell.tabs.active_tab, 2);
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
    assert_eq!(app.shell.tabs.len(), 4);
    assert_eq!(app.shell.tabs.active_tab, 3);

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

    assert_eq!(app.shell.tabs.active_tab, 1);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ctrl_number_without_matching_tab_does_not_switch() {
    let root = test_root("shortcut-ctrl-number-no-tab");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    assert_eq!(app.shell.tabs.len(), 2);
    assert_eq!(app.shell.tabs.active_tab, 1);

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

    assert_eq!(app.shell.tabs.active_tab, 1);
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
    app.shell.runtime.root = root_b.clone();
    app.sync_active_tab_state();
    assert_eq!(app.shell.tabs.active_tab, 1);

    app.switch_to_tab_index(0);
    assert_eq!(app.shell.runtime.root, root_a);

    app.switch_to_tab_index(1);
    assert_eq!(app.shell.runtime.root, root_b);

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
    app.shell.runtime.entries = Arc::new(vec![unknown_entry(a.clone()), unknown_entry(b.clone())]);
    app.shell.runtime.all_entries =
        Arc::new(vec![unknown_entry(a.clone()), unknown_entry(b.clone())]);
    app.shell.runtime.include_files = true;
    app.shell.runtime.include_dirs = true;
    app.sync_active_tab_state();

    app.create_new_tab();
    app.shell.runtime.entries = Arc::new(vec![unknown_entry(a.clone())]);
    app.shell.runtime.all_entries = Arc::new(vec![unknown_entry(a.clone())]);
    app.shell.runtime.include_files = true;
    app.shell.runtime.include_dirs = false;
    app.sync_active_tab_state();

    app.switch_to_tab_index(0);
    assert_eq!(app.shell.runtime.entries.len(), 2);
    assert_eq!(app.shell.runtime.all_entries.len(), 2);
    assert!(app.shell.runtime.include_files);
    assert!(app.shell.runtime.include_dirs);

    app.switch_to_tab_index(1);
    assert_eq!(app.shell.runtime.entries.len(), 1);
    assert_eq!(app.shell.runtime.all_entries.len(), 1);
    assert!(app.shell.runtime.include_files);
    assert!(!app.shell.runtime.include_dirs);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn root_dropdown_selection_closes_popup_and_applies_selected_root() {
    let root_a = test_root("root-dropdown-select-a");
    let root_b = test_root("root-dropdown-select-b");
    fs::create_dir_all(&root_a).expect("create root a");
    fs::create_dir_all(&root_b).expect("create root b");
    let mut app = FlistWalkerApp::new(root_a.clone(), 50, String::new());
    app.shell.features.root_browser.saved_roots = vec![root_a.clone(), root_b.clone()];
    let ctx = egui::Context::default();

    app.open_root_dropdown(&ctx);
    app.move_root_dropdown_selection(1);
    assert!(app.is_root_dropdown_open(&ctx));
    assert_eq!(app.shell.ui.root_dropdown_highlight, Some(1));

    app.apply_root_dropdown_selection(&ctx);

    assert!(!app.is_root_dropdown_open(&ctx));
    assert_eq!(app.shell.runtime.root, root_b);
    assert_eq!(app.shell.ui.root_dropdown_highlight, Some(1));
    let _ = fs::remove_dir_all(&root_a);
    let _ = fs::remove_dir_all(&root_b);
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
    app.shell.runtime.root = root_b.clone();
    app.sync_active_tab_state();
    app.create_new_tab();
    app.shell.runtime.root = root_c.clone();
    app.sync_active_tab_state();
    assert_eq!(app.shell.tabs.active_tab, 2);

    app.move_tab(2, 0);

    assert_eq!(app.shell.tabs.active_tab, 0);
    assert_eq!(app.shell.runtime.root, root_c);
    assert_eq!(app.shell.tabs[0].root, root_c);
    assert_eq!(app.shell.tabs[1].root, root_a);
    assert_eq!(app.shell.tabs[2].root, root_b);

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
    app.shell.runtime.root = root_b.clone();
    app.sync_active_tab_state();
    app.create_new_tab();
    app.shell.runtime.root = root_c.clone();
    app.sync_active_tab_state();
    app.switch_to_tab_index(1);
    assert_eq!(app.shell.runtime.root, root_b);
    assert_eq!(app.shell.tabs.active_tab, 1);

    app.move_tab(0, 2);

    assert_eq!(app.shell.tabs.active_tab, 0);
    assert_eq!(app.shell.runtime.root, root_b);
    assert_eq!(app.shell.tabs[0].root, root_b);
    assert_eq!(app.shell.tabs[1].root, root_c);
    assert_eq!(app.shell.tabs[2].root, root_a);

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
    let original_ids: Vec<u64> = app.shell.tabs.iter().map(|tab| tab.id).collect();
    let original_active = app.shell.tabs.active_tab;

    app.move_tab(1, 1);
    app.move_tab(99, 0);
    app.move_tab(0, 99);

    assert_eq!(
        app.shell.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>(),
        original_ids
    );
    assert_eq!(app.shell.tabs.active_tab, original_active);

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

    app.shell.runtime.include_dirs = false;
    app.shell.runtime.preview = "preview-a".to_string();
    app.shell.ui.focus_query_requested = true;
    app.shell.ui.unfocus_query_requested = false;
    app.sync_active_tab_state();

    app.create_new_tab();
    app.shell.runtime.root = root_b.clone();
    app.shell.runtime.query_state.query = "beta".to_string();
    app.shell.runtime.include_files = false;
    app.shell.runtime.include_dirs = true;
    app.shell.runtime.preview = "preview-b".to_string();
    app.shell.ui.focus_query_requested = false;
    app.shell.ui.unfocus_query_requested = true;
    app.sync_active_tab_state();

    app.create_new_tab();
    app.shell.runtime.root = root_c.clone();
    app.shell.runtime.query_state.query = "gamma".to_string();
    app.shell.runtime.include_files = true;
    app.shell.runtime.include_dirs = true;
    app.shell.runtime.preview = "preview-c".to_string();
    app.shell.ui.focus_query_requested = true;
    app.shell.ui.unfocus_query_requested = false;
    app.sync_active_tab_state();

    app.switch_to_tab_index(1);
    assert_eq!(app.shell.runtime.root, root_b);
    assert_eq!(app.shell.runtime.query_state.query, "beta");
    let expected_active_root = app.shell.runtime.root.clone();
    let expected_active_query = app.shell.runtime.query_state.query.clone();
    let expected_active_preview = app.shell.runtime.preview.clone();
    let expected_active_include_files = app.shell.runtime.include_files;
    let expected_active_include_dirs = app.shell.runtime.include_dirs;
    let expected_active_focus = app.shell.ui.focus_query_requested;
    let expected_active_unfocus = app.shell.ui.unfocus_query_requested;
    let expected_tab_b = app.shell.tabs[1].clone();
    let expected_tab_c = app.shell.tabs[2].clone();
    let expected_tab_a = app.shell.tabs[0].clone();

    app.move_tab(0, 2);

    assert_eq!(app.shell.tabs.active_tab, 0);
    assert_eq!(app.shell.runtime.root, expected_active_root);
    assert_eq!(app.shell.runtime.query_state.query, expected_active_query);
    assert_eq!(app.shell.runtime.preview, expected_active_preview);
    assert_eq!(
        app.shell.runtime.include_files,
        expected_active_include_files
    );
    assert_eq!(app.shell.runtime.include_dirs, expected_active_include_dirs);
    assert_eq!(app.shell.ui.focus_query_requested, expected_active_focus);
    assert_eq!(
        app.shell.ui.unfocus_query_requested,
        expected_active_unfocus
    );

    assert_eq!(app.shell.tabs[0].root, expected_tab_b.root);
    assert_eq!(
        app.shell.tabs[0].query_state.query,
        expected_tab_b.query_state.query
    );
    assert_eq!(
        app.shell.tabs[0].result_state.preview,
        expected_tab_b.result_state.preview
    );
    assert_eq!(app.shell.tabs[1].root, expected_tab_c.root);
    assert_eq!(
        app.shell.tabs[1].query_state.query,
        expected_tab_c.query_state.query
    );
    assert_eq!(
        app.shell.tabs[1].result_state.preview,
        expected_tab_c.result_state.preview
    );
    assert_eq!(app.shell.tabs[2].root, expected_tab_a.root);
    assert_eq!(
        app.shell.tabs[2].query_state.query,
        expected_tab_a.query_state.query
    );
    assert_eq!(
        app.shell.tabs[2].result_state.preview,
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
    app.shell.ui.tab_drag_state = Some(TabDragState {
        source_index: 2,
        hover_index: 0,
        press_pos: egui::pos2(10.0, 12.0),
        dragging: true,
    });

    app.move_tab(2, 0);

    assert_eq!(app.shell.ui.tab_drag_state, None);
    app.switch_to_tab_index(1);
    assert_eq!(app.shell.ui.tab_drag_state, None);
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
    assert_eq!(app.shell.ui.tab_drag_state, Some(state));

    let released =
        app.update_tab_drag_state(&mut state, &tab_rects, Some(egui::pos2(13.0, 12.0)), false);
    assert_eq!(released, None);
    assert_eq!(app.shell.ui.tab_drag_state, None);
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
    assert_eq!(app.shell.ui.tab_drag_state, Some(state));

    let released =
        app.update_tab_drag_state(&mut state, &tab_rects, Some(egui::pos2(140.0, 12.0)), false);
    assert_eq!(released, Some((0, 1)));
    assert_eq!(app.shell.ui.tab_drag_state, None);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn background_tab_search_and_preview_responses_are_retained() {
    let root = test_root("background-tab-search-preview");
    fs::create_dir_all(&root).expect("create dir");
    let selected = root.join("picked.txt");
    fs::write(&selected, "hello").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "picked".to_string());
    app.shell.indexing.in_progress = false;
    app.shell.indexing.pending_request_id = None;
    app.shell.runtime.entries = Arc::new(vec![file_entry(selected.clone())]);
    app.shell.runtime.results = vec![(selected.clone(), 0.0)];
    app.shell.runtime.current_row = Some(0);
    app.set_entry_kind(&selected, EntryKind::file());

    let (search_tx_req, _search_rx_req) = mpsc::channel::<SearchRequest>();
    let (search_tx_res, search_rx_res) = mpsc::channel::<SearchResponse>();
    app.shell.search.tx = search_tx_req;
    app.shell.search.rx = search_rx_res;
    app.enqueue_search_request();
    let search_request_id = app
        .shell
        .search
        .pending_request_id()
        .expect("search request id");
    let first_tab_id = app.shell.tabs[0].id;

    let (preview_tx_req, _preview_rx_req) = mpsc::channel::<PreviewRequest>();
    let (preview_tx_res, preview_rx_res) = mpsc::channel::<PreviewResponse>();
    app.shell.worker_bus.preview.tx = preview_tx_req;
    app.shell.worker_bus.preview.rx = preview_rx_res;
    app.request_preview_for_current();
    let preview_request_id = app
        .shell
        .worker_bus
        .preview
        .pending_request_id
        .expect("preview request id");

    app.create_new_tab();
    assert_eq!(app.shell.tabs.active_tab, 1);

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
        .shell
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
    app.shell.indexing.in_progress = true;
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

    assert!(app.shell.indexing.in_progress);
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
    app.shell.indexing.tx = index_req_tx;
    let (index_res_tx, index_res_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = index_res_rx;

    app.request_index_refresh();
    let index_req = index_req_rx.try_recv().expect("index request");
    app.shell.runtime.entries = Arc::new(vec![unknown_entry(active_file.clone())]);
    app.shell.runtime.all_entries = Arc::new(vec![unknown_entry(active_file.clone())]);
    app.sync_active_tab_state();

    app.create_new_tab();
    assert_eq!(app.shell.tabs.active_tab, 1);
    app.shell.runtime.entries = Arc::new(vec![unknown_entry(active_file.clone())]);
    app.shell.runtime.all_entries = Arc::new(vec![unknown_entry(active_file.clone())]);
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

    assert_eq!(app.shell.runtime.entries.len(), 1);
    assert_eq!(app.shell.runtime.entries[0], active_file);

    app.switch_to_tab_index(0);
    assert_eq!(app.shell.runtime.entries.len(), 1);
    assert_eq!(app.shell.runtime.entries[0], indexed_file);
    assert!(!app.shell.indexing.in_progress);

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
    app.shell.indexing.tx = index_req_tx;
    app.shell.indexing.rx = index_res_rx;
    let (search_tx_req, _search_rx_req) = mpsc::channel::<SearchRequest>();
    let (search_tx_res, search_rx_res) = mpsc::channel::<SearchResponse>();
    app.shell.search.tx = search_tx_req;
    app.shell.search.rx = search_rx_res;

    app.shell.runtime.entries = Arc::new(vec![file_entry(active_file.clone())]);
    app.shell.runtime.all_entries = Arc::new(vec![file_entry(active_file.clone())]);
    app.shell.runtime.results = vec![(active_file.clone(), 0.0)];
    app.shell.runtime.base_results = app.shell.runtime.results.clone();
    app.shell.runtime.current_row = Some(0);
    app.sync_active_tab_state();

    app.create_new_tab();
    assert_eq!(app.shell.tabs.active_tab, 1);
    app.shell.runtime.entries = Arc::new(vec![file_entry(active_file.clone())]);
    app.shell.runtime.all_entries = Arc::new(vec![file_entry(active_file.clone())]);
    app.shell.runtime.results = vec![(active_file.clone(), 0.0)];
    app.shell.runtime.base_results = app.shell.runtime.results.clone();
    app.shell.runtime.current_row = Some(0);
    app.sync_active_tab_state();

    app.switch_to_tab_index(0);
    app.shell.runtime.query_state.query = "background".to_string();
    app.sync_active_tab_state();
    app.switch_to_tab_index(1);

    let background_tab_id = app.shell.tabs[0].id;
    let background_index_request = IndexRequest {
        request_id: 88,
        tab_id: background_tab_id,
        root: root.clone(),
        use_filelist: true,
        include_files: true,
        include_dirs: true,
    };
    app.shell
        .indexing
        .request_tabs
        .insert(88, background_tab_id);
    app.shell.tabs[0].index_state.pending_index_request_id = Some(88);
    app.shell.tabs[0].index_state.index_in_progress = true;
    app.shell.search.bind_request_tab(89, background_tab_id);
    app.shell.tabs[0].pending_request_id = Some(89);
    app.shell.tabs[0].search_in_progress = true;

    let active_results = app.shell.runtime.results.clone();
    let active_base_results = app.shell.runtime.base_results.clone();
    let active_current_row = app.shell.runtime.current_row;

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

    assert_eq!(app.shell.runtime.results, active_results);
    assert_eq!(app.shell.runtime.base_results, active_base_results);
    assert_eq!(app.shell.runtime.current_row, active_current_row);
    assert_eq!(app.shell.tabs[0].result_state.base_results.len(), 1);
    assert_eq!(
        app.shell.tabs[0].result_state.base_results[0].0,
        background_file
    );
    assert_eq!(app.shell.tabs[0].index_state.entries.len(), 1);
    assert_eq!(app.shell.tabs[0].index_state.entries[0], background_file);
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

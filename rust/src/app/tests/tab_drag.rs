use super::*;

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
    assert_eq!(app.shell.tabs.get(0).expect("tab 0").root, root_c);
    assert_eq!(app.shell.tabs.get(1).expect("tab 1").root, root_a);
    assert_eq!(app.shell.tabs.get(2).expect("tab 2").root, root_b);

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
    assert_eq!(app.shell.tabs.get(0).expect("tab 0").root, root_b);
    assert_eq!(app.shell.tabs.get(1).expect("tab 1").root, root_c);
    assert_eq!(app.shell.tabs.get(2).expect("tab 2").root, root_a);

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
    let expected_tab_b = app.shell.tabs.get(1).expect("tab 1").clone();
    let expected_tab_c = app.shell.tabs.get(2).expect("tab 2").clone();
    let expected_tab_a = app.shell.tabs.get(0).expect("tab 0").clone();

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

    assert_eq!(
        app.shell.tabs.get(0).expect("tab 0").root,
        expected_tab_b.root
    );
    assert_eq!(
        app.shell.tabs.get(0).expect("tab 0").query_state.query,
        expected_tab_b.query_state.query
    );
    assert_eq!(
        app.shell.tabs.get(0).expect("tab 0").result_state.preview,
        expected_tab_b.result_state.preview
    );
    assert_eq!(
        app.shell.tabs.get(1).expect("tab 1").root,
        expected_tab_c.root
    );
    assert_eq!(
        app.shell.tabs.get(1).expect("tab 1").query_state.query,
        expected_tab_c.query_state.query
    );
    assert_eq!(
        app.shell.tabs.get(1).expect("tab 1").result_state.preview,
        expected_tab_c.result_state.preview
    );
    assert_eq!(
        app.shell.tabs.get(2).expect("tab 2").root,
        expected_tab_a.root
    );
    assert_eq!(
        app.shell.tabs.get(2).expect("tab 2").query_state.query,
        expected_tab_a.query_state.query
    );
    assert_eq!(
        app.shell.tabs.get(2).expect("tab 2").result_state.preview,
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

    let dragging = super::render_tabs::update_tab_drag_state(
        &mut app,
        &mut state,
        &tab_rects,
        Some(egui::pos2(13.0, 12.0)),
        true,
    );
    assert_eq!(dragging, None);
    assert!(!state.dragging);
    assert_eq!(app.shell.ui.tab_drag_state, Some(state));

    let released = super::render_tabs::update_tab_drag_state(
        &mut app,
        &mut state,
        &tab_rects,
        Some(egui::pos2(13.0, 12.0)),
        false,
    );
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

    let dragging = super::render_tabs::update_tab_drag_state(
        &mut app,
        &mut state,
        &tab_rects,
        Some(egui::pos2(140.0, 12.0)),
        true,
    );
    assert_eq!(dragging, None);
    assert!(state.dragging);
    assert_eq!(state.hover_index, 1);
    assert_eq!(app.shell.ui.tab_drag_state, Some(state));

    let released = super::render_tabs::update_tab_drag_state(
        &mut app,
        &mut state,
        &tab_rects,
        Some(egui::pos2(140.0, 12.0)),
        false,
    );
    assert_eq!(released, Some((0, 1)));
    assert_eq!(app.shell.ui.tab_drag_state, None);
    let _ = fs::remove_dir_all(&root);
}

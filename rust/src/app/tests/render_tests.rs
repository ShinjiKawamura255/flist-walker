use super::*;

#[test]
fn filelist_use_walker_dialog_lines_are_stable() {
    let lines = FlistWalkerApp::filelist_use_walker_dialog_lines();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("Walker indexing"));
    assert!(lines[1].contains("裏で一時的に Walker"));
}

#[test]
fn top_action_labels_show_history_actions_while_history_search_is_active() {
    let root = test_root("render-history-actions");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.history_search_active = true;

    assert_eq!(
        app.top_action_labels(),
        vec!["Apply History", "Cancel History Search"]
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn top_action_labels_show_default_create_label_when_idle() {
    let root = test_root("render-default-actions");
    fs::create_dir_all(&root).expect("create dir");
    let app = FlistWalkerApp::new(root.clone(), 50, String::new());

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
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn top_action_labels_show_running_create_label_when_filelist_is_in_progress() {
    let root = test_root("render-running-actions");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.filelist_state.in_progress = true;

    assert_eq!(app.top_action_labels()[3], "Create File List (Running...)");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn results_scroll_enabled_is_true_when_preview_resize_is_inactive() {
    assert!(FlistWalkerApp::results_scroll_enabled(false));
}

#[test]
fn results_scroll_enabled_is_false_when_preview_resize_is_active() {
    assert!(!FlistWalkerApp::results_scroll_enabled(true));
}

#[test]
fn result_row_text_pos_is_left_aligned_and_vertically_centered() {
    let inner = egui::Rect::from_min_max(egui::pos2(8.0, 10.0), egui::pos2(208.0, 34.0));
    let galley_size = egui::vec2(120.0, 14.0);

    let pos = FlistWalkerApp::result_row_text_pos(inner, galley_size);

    assert_eq!(pos.x, inner.left());
    assert_eq!(pos.y, inner.center().y - (galley_size.y * 0.5));
}

#[test]
fn tab_drop_index_returns_none_for_empty_tabs() {
    assert_eq!(FlistWalkerApp::tab_drop_index(&[], egui::pos2(10.0, 10.0)), None);
}

#[test]
fn tab_drop_index_chooses_first_tab_before_first_center() {
    let rects = vec![
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(100.0, 24.0)),
        egui::Rect::from_min_max(egui::pos2(110.0, 0.0), egui::pos2(210.0, 24.0)),
    ];

    assert_eq!(FlistWalkerApp::tab_drop_index(&rects, egui::pos2(20.0, 12.0)), Some(0));
}

#[test]
fn tab_drop_index_chooses_middle_tab_when_pointer_is_between_centers() {
    let rects = vec![
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(100.0, 24.0)),
        egui::Rect::from_min_max(egui::pos2(110.0, 0.0), egui::pos2(210.0, 24.0)),
        egui::Rect::from_min_max(egui::pos2(220.0, 0.0), egui::pos2(320.0, 24.0)),
    ];

    assert_eq!(FlistWalkerApp::tab_drop_index(&rects, egui::pos2(170.0, 12.0)), Some(2));
}

#[test]
fn tab_drop_index_returns_last_tab_after_all_centers() {
    let rects = vec![
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(100.0, 24.0)),
        egui::Rect::from_min_max(egui::pos2(110.0, 0.0), egui::pos2(210.0, 24.0)),
    ];

    assert_eq!(FlistWalkerApp::tab_drop_index(&rects, egui::pos2(260.0, 12.0)), Some(1));
}

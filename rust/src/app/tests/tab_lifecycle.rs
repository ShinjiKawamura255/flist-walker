use super::*;

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
    assert_eq!(app.shell.tabs.get(1).expect("tab 1").tab_accent, None);
    assert!(app.shell.ui.focus_query_requested);
    assert!(!app.shell.ui.unfocus_query_requested);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn create_new_tab_resets_total_match_count_to_current_entries() {
    let root = test_root("new-tab-total-count");
    fs::create_dir_all(&root).expect("create dir");
    let first = root.join("first.txt");
    let second = root.join("second.txt");
    fs::write(&first, "a").expect("write first");
    fs::write(&second, "b").expect("write second");

    let mut app = FlistWalkerApp::new(root.clone(), 1, "previous".to_string());
    app.shell.runtime.entries = Arc::new(vec![file_entry(first), file_entry(second)]);
    app.shell.runtime.total_match_count = 99;

    app.create_new_tab();

    assert!(app.shell.runtime.query_state.query.is_empty());
    assert_eq!(app.shell.runtime.results.len(), 1);
    assert_eq!(app.shell.runtime.total_match_count, 2);
    assert!(app.status_line_text().contains("Results: 1 of 2 shown"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ctrl_w_closes_current_tab_and_keeps_last_tab() {
    let root = test_root("shortcut-ctrl-w-close-tab");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    assert_eq!(app.shell.tabs.len(), 2);
    app.shell.ui.focus_query_requested = false;
    app.shell.ui.unfocus_query_requested = true;

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
fn closing_active_tab_compacts_restorable_tab_results() {
    let root = test_root("close-active-tab-compacts-closed-stack");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let path_a = root.join("a.txt");
    let path_b = root.join("b.txt");

    app.create_new_tab();
    app.shell.runtime.base_results = vec![(path_a.clone(), 2.0), (path_b.clone(), 1.0)];
    app.shell.runtime.results = app.shell.runtime.base_results.clone();
    app.shell.runtime.preview = "preview body".to_string();
    app.sync_active_tab_state();

    app.close_active_tab();

    assert_eq!(app.shell.tabs.len(), 1);
    assert_eq!(
        app.shell.tabs.last_closed_tab_results_compacted(),
        Some(true)
    );
    app.restore_recently_closed_tab();
    assert_eq!(app.shell.tabs.active_tab, 1);
    assert_eq!(app.shell.runtime.results.len(), 2);
    assert_eq!(app.shell.runtime.results[0].0, path_a);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn restoring_closed_tab_prefers_original_position() {
    let root = test_root("tab-restore-original-position");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

    app.create_new_tab();
    app.shell.runtime.query_state.query = "middle".to_string();
    app.sync_active_tab_state();
    let middle_tab_id = app.shell.tabs.get(1).expect("middle tab").id;

    app.create_new_tab();
    app.shell.runtime.query_state.query = "right".to_string();
    app.sync_active_tab_state();
    let right_tab_id = app.shell.tabs.get(2).expect("right tab").id;

    app.close_tab_index(1);
    assert_eq!(app.shell.tabs.len(), 2);
    assert_eq!(app.shell.tabs.get(1).expect("right tab").id, right_tab_id);

    app.restore_recently_closed_tab();

    assert_eq!(app.shell.tabs.len(), 3);
    assert_eq!(app.shell.tabs.active_tab, 1);
    assert_eq!(app.shell.runtime.query_state.query, "middle");
    assert_ne!(
        app.shell.tabs.get(1).expect("restored middle tab").id,
        middle_tab_id
    );
    assert_eq!(app.shell.tabs.get(2).expect("right tab").id, right_tab_id);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ctrl_shift_t_restores_most_recently_closed_tab_as_active() {
    let root_a = test_root("shortcut-ctrl-shift-t-restore-a");
    let root_b = test_root("shortcut-ctrl-shift-t-restore-b");
    fs::create_dir_all(&root_a).expect("create root a");
    fs::create_dir_all(&root_b).expect("create root b");
    let mut app = FlistWalkerApp::new(root_a.clone(), 50, String::new());
    let original_tab_id = app.shell.tabs.get(0).expect("tab 0").id;

    app.create_new_tab();
    app.shell.runtime.root = root_b.clone();
    app.shell.runtime.query_state.query = "needle".to_string();
    app.shell.runtime.include_dirs = false;
    app.sync_active_tab_state();
    let closed_tab_id = app.shell.tabs.get(1).expect("tab 1").id;
    app.shell.search.set_pending_request_id(Some(31));
    app.shell.search.set_in_progress(true);
    app.shell.indexing.pending_request_id = Some(32);
    app.shell.indexing.in_progress = true;
    app.shell.worker_bus.preview.pending_request_id = Some(33);
    app.shell.worker_bus.preview.in_progress = true;
    app.shell.tabs.bind_preview_request(33, closed_tab_id);
    app.shell.worker_bus.action.pending_request_id = Some(34);
    app.shell.worker_bus.action.in_progress = true;
    app.shell.tabs.bind_action_request(34, closed_tab_id);
    app.shell.worker_bus.sort.pending_request_id = Some(35);
    app.shell.worker_bus.sort.in_progress = true;
    app.shell.tabs.bind_sort_request(35, closed_tab_id);
    app.shell.search.bind_request_tab(31, closed_tab_id);

    app.close_active_tab();
    assert_eq!(app.shell.tabs.len(), 1);
    assert_eq!(app.shell.tabs.active_tab, 0);
    assert_eq!(app.shell.tabs.get(0).expect("tab 0").id, original_tab_id);

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::T,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: gui_shortcut_modifiers(true),
        }],
    );

    assert_eq!(app.shell.tabs.len(), 2);
    assert_eq!(app.shell.tabs.active_tab, 1);
    assert_eq!(app.shell.runtime.root, root_b);
    assert_eq!(app.shell.runtime.query_state.query, "needle");
    assert!(!app.shell.runtime.include_dirs);
    assert_ne!(
        app.shell.tabs.get(1).expect("restored tab").id,
        closed_tab_id
    );
    assert_eq!(app.shell.search.pending_request_id(), None);
    assert!(!app.shell.search.in_progress());
    assert_eq!(app.shell.indexing.pending_request_id, None);
    assert!(!app.shell.indexing.in_progress);
    assert_eq!(app.shell.worker_bus.preview.pending_request_id, None);
    assert!(!app.shell.worker_bus.preview.in_progress);
    assert_eq!(app.shell.worker_bus.action.pending_request_id, None);
    assert!(!app.shell.worker_bus.action.in_progress);
    assert_eq!(app.shell.worker_bus.sort.pending_request_id, None);
    assert!(!app.shell.worker_bus.sort.in_progress);
    assert!(matches!(
        app.shell.search.route_response(31),
        SearchResponseRoute::Stale
    ));
    assert_eq!(app.shell.tabs.preview_request_tab(33), None);
    assert_eq!(app.shell.tabs.action_request_tab(34), None);
    assert_eq!(app.shell.tabs.sort_request_tab(35), None);
    assert!(app.shell.ui.focus_query_requested);
    assert!(!app.shell.ui.unfocus_query_requested);
    let _ = fs::remove_dir_all(&root_a);
    let _ = fs::remove_dir_all(&root_b);
}

#[test]
fn restoring_closed_tabs_uses_lifo_order_and_empty_stack_is_noop() {
    let root_a = test_root("tab-restore-lifo-a");
    let root_b = test_root("tab-restore-lifo-b");
    let root_c = test_root("tab-restore-lifo-c");
    fs::create_dir_all(&root_a).expect("create root a");
    fs::create_dir_all(&root_b).expect("create root b");
    fs::create_dir_all(&root_c).expect("create root c");
    let mut app = FlistWalkerApp::new(root_a.clone(), 50, String::new());

    app.create_new_tab();
    app.shell.runtime.root = root_b.clone();
    app.shell.runtime.query_state.query = "second".to_string();
    app.sync_active_tab_state();

    app.create_new_tab();
    app.shell.runtime.root = root_c.clone();
    app.shell.runtime.query_state.query = "third".to_string();
    app.sync_active_tab_state();

    app.close_tab_index(1);
    app.close_tab_index(1);
    assert_eq!(app.shell.tabs.len(), 1);

    app.restore_recently_closed_tab();
    assert_eq!(app.shell.tabs.len(), 2);
    assert_eq!(app.shell.tabs.active_tab, 1);
    assert_eq!(app.shell.runtime.root, root_c);
    assert_eq!(app.shell.runtime.query_state.query, "third");

    app.restore_recently_closed_tab();
    assert_eq!(app.shell.tabs.len(), 3);
    assert_eq!(app.shell.tabs.active_tab, 1);
    assert_eq!(app.shell.runtime.root, root_b);
    assert_eq!(app.shell.runtime.query_state.query, "second");

    app.restore_recently_closed_tab();
    assert_eq!(app.shell.tabs.len(), 3);
    assert!(app
        .shell
        .runtime
        .notice
        .contains("No closed tab to restore"));
    let _ = fs::remove_dir_all(&root_a);
    let _ = fs::remove_dir_all(&root_b);
    let _ = fs::remove_dir_all(&root_c);
}

#[test]
fn closed_tab_restore_stack_keeps_only_recent_entries() {
    let root = test_root("tab-restore-stack-limit");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

    for index in 0..26 {
        app.create_new_tab();
        app.shell.runtime.query_state.query = format!("closed-{index}");
        app.sync_active_tab_state();
        app.close_active_tab();
    }

    app.restore_recently_closed_tab();
    assert_eq!(app.shell.runtime.query_state.query, "closed-25");

    for _ in 0..24 {
        app.restore_recently_closed_tab();
    }
    assert_eq!(app.shell.runtime.query_state.query, "closed-1");

    app.restore_recently_closed_tab();
    assert_eq!(app.shell.tabs.len(), 26);
    assert!(app
        .shell
        .runtime
        .notice
        .contains("No closed tab to restore"));
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

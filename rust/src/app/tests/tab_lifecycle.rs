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

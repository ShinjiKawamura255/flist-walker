use super::*;

#[test]
fn normalize_singleline_input_removes_formatting_chars_and_leading_tabs() {
    let mut text = "\u{feff}\talpha\u{200b}\n\tbeta\tgamma\u{2060}".to_string();

    let changed = FlistWalkerApp::normalize_singleline_input(&mut text);

    assert!(changed);
    assert_eq!(text, "alpha beta gamma");
}

#[test]
fn ctrl_h_deletes_only_one_char_when_widget_did_not_change_text() {
    let root = test_root("ctrl-h-single");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.query_state.query = "abcd".to_string();
    let mut cursor = 4usize;
    let mut anchor = 4usize;

    let (text_changed, cursor_changed) = app.apply_ctrl_h_delete(&mut cursor, &mut anchor, false);

    assert!(text_changed);
    assert!(cursor_changed);
    assert_eq!(app.shell.runtime.query_state.query, "abc");
    assert_eq!(cursor, 3);
    assert_eq!(anchor, 3);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ctrl_h_does_not_delete_twice_when_widget_already_changed_text() {
    let root = test_root("ctrl-h-guard");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    // Simulate that TextEdit already handled one Backspace and this frame query is already updated.
    app.shell.runtime.query_state.query = "abc".to_string();
    let mut cursor = 3usize;
    let mut anchor = 3usize;

    let (text_changed, cursor_changed) = app.apply_ctrl_h_delete(&mut cursor, &mut anchor, true);

    assert!(!text_changed);
    assert!(!cursor_changed);
    assert_eq!(app.shell.runtime.query_state.query, "abc");
    assert_eq!(cursor, 3);
    assert_eq!(anchor, 3);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn move_page_moves_by_fixed_rows_and_clamps() {
    let root = test_root("move-page");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.results = (0..30)
        .map(|i| (root.join(format!("f{i}.txt")), 0.0))
        .collect();
    app.shell.runtime.current_row = Some(0);

    app.move_page(1);
    assert_eq!(app.shell.runtime.current_row, Some(10));

    app.move_page(-1);
    assert_eq!(app.shell.runtime.current_row, Some(0));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ctrl_n_and_ctrl_p_move_selection_even_when_query_is_focused() {
    let root = test_root("shortcut-ctrl-np-query-focus");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.results = vec![
        (root.join("a.txt"), 0.0),
        (root.join("b.txt"), 0.0),
        (root.join("c.txt"), 0.0),
    ];
    app.shell.runtime.current_row = Some(0);

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::N,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: emacs_shortcut_modifiers(false),
        }],
    );
    assert_eq!(app.shell.runtime.current_row, Some(1));

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::P,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: emacs_shortcut_modifiers(false),
        }],
    );
    assert_eq!(app.shell.runtime.current_row, Some(0));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ctrl_g_clears_query_and_resets_selection_even_when_query_is_focused() {
    let root = test_root("shortcut-ctrl-g-query-focus");
    fs::create_dir_all(&root).expect("create dir");
    let selected = root.join("picked.txt");
    fs::write(&selected, "x").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "query".to_string());
    app.shell.runtime.entries = Arc::new(vec![unknown_entry(selected.clone())]);
    app.shell.runtime.results = vec![(selected.clone(), 0.0)];
    app.shell.runtime.current_row = Some(0);
    app.shell.runtime.pinned_paths.insert(selected);

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::G,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: emacs_shortcut_modifiers(false),
        }],
    );

    assert!(app.shell.runtime.query_state.query.is_empty());
    assert!(app.shell.runtime.pinned_paths.is_empty());
    assert_eq!(app.shell.runtime.results.len(), 1);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn escape_clears_query_and_resets_selection_even_when_query_is_focused() {
    let root = test_root("shortcut-escape-query-focus");
    fs::create_dir_all(&root).expect("create dir");
    let selected = root.join("picked.txt");
    fs::write(&selected, "x").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "query".to_string());
    app.shell.runtime.entries = Arc::new(vec![unknown_entry(selected.clone())]);
    app.shell.runtime.results = vec![(selected.clone(), 0.0)];
    app.shell.runtime.current_row = Some(0);
    app.shell.runtime.pinned_paths.insert(selected);

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );

    assert!(app.shell.runtime.query_state.query.is_empty());
    assert!(app.shell.runtime.pinned_paths.is_empty());
    assert_eq!(app.shell.runtime.results.len(), 1);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn home_and_end_move_selection_when_query_not_focused() {
    let root = test_root("shortcut-home-end-no-focus");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.results = (0..5)
        .map(|i| (root.join(format!("f{i}.txt")), 0.0))
        .collect();
    app.shell.runtime.current_row = Some(2);

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::Home,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert_eq!(app.shell.runtime.current_row, Some(0));

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::End,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert_eq!(app.shell.runtime.current_row, Some(4));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn page_up_down_move_selection_when_query_not_focused() {
    let root = test_root("shortcut-page-no-focus");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.results = (0..30)
        .map(|i| (root.join(format!("f{i}.txt")), 0.0))
        .collect();
    app.shell.runtime.current_row = Some(15);

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::PageUp,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert_eq!(app.shell.runtime.current_row, Some(5));

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::PageDown,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert_eq!(app.shell.runtime.current_row, Some(15));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ctrl_v_and_alt_v_page_move_when_query_not_focused() {
    let root = test_root("shortcut-emacs-page-no-focus");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.results = (0..30)
        .map(|i| (root.join(format!("f{i}.txt")), 0.0))
        .collect();
    app.shell.runtime.current_row = Some(15);

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::V,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                ctrl: true,
                ..Default::default()
            },
        }],
    );
    assert_eq!(app.shell.runtime.current_row, Some(25));

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::V,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                alt: true,
                ..Default::default()
            },
        }],
    );
    assert_eq!(app.shell.runtime.current_row, Some(15));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ctrl_v_paste_event_pages_down_only_when_query_not_focused() {
    let root = test_root("shortcut-ctrl-v-paste-event");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.results = (0..30)
        .map(|i| (root.join(format!("f{i}.txt")), 0.0))
        .collect();
    app.shell.runtime.current_row = Some(15);
    let ctrl_mods = egui::Modifiers {
        ctrl: true,
        ..Default::default()
    };

    run_shortcuts_frame_with_modifiers(
        &mut app,
        false,
        ctrl_mods,
        vec![egui::Event::Paste("clipboard".to_string())],
    );
    assert_eq!(app.shell.runtime.current_row, Some(25));

    run_shortcuts_frame_with_modifiers(
        &mut app,
        true,
        ctrl_mods,
        vec![egui::Event::Paste("clipboard".to_string())],
    );
    assert_eq!(app.shell.runtime.current_row, Some(25));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn regression_arrow_keys_move_selection_even_when_query_focused() {
    let root = test_root("regression-arrow-query-focus");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.results = vec![
        (root.join("a.txt"), 0.0),
        (root.join("b.txt"), 0.0),
        (root.join("c.txt"), 0.0),
    ];
    app.shell.runtime.current_row = Some(0);

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::ArrowDown,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert_eq!(app.shell.runtime.current_row, Some(1));

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::ArrowUp,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert_eq!(app.shell.runtime.current_row, Some(0));
    let _ = fs::remove_dir_all(&root);
}

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
fn ctrl_shift_r_opens_root_dropdown_without_starting_history_search() {
    let root = test_root("shortcut-ctrl-shift-r-root-dropdown");
    let alt = root.join("alt");
    fs::create_dir_all(&root).expect("create dir");
    fs::create_dir_all(&alt).expect("create alt dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "draft".to_string());
    app.shell.features.root_browser.saved_roots = vec![root.clone(), alt];
    let ctx = egui::Context::default();

    ctx.begin_pass(egui::RawInput {
        modifiers: gui_shortcut_modifiers(true),
        events: vec![egui::Event::Key {
            key: egui::Key::R,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: gui_shortcut_modifiers(true),
        }],
        ..Default::default()
    });
    ctx.memory_mut(|m| m.request_focus(app.shell.ui.query_input_id));
    app.handle_shortcuts(&ctx);

    assert!(app.is_root_dropdown_open(&ctx));
    assert_eq!(app.shell.ui.root_dropdown_highlight, Some(0));
    assert!(!app.shell.runtime.query_state.history_search_active);
    assert_eq!(app.shell.runtime.query_state.query, "draft");

    let _ = ctx.end_pass();
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn root_dropdown_ctrl_n_and_ctrl_p_move_selection() {
    let root = test_root("root-dropdown-ctrl-np");
    let second = root.join("second");
    let third = root.join("third");
    fs::create_dir_all(&root).expect("create dir");
    fs::create_dir_all(&second).expect("create second");
    fs::create_dir_all(&third).expect("create third");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.features.root_browser.saved_roots = vec![root.clone(), second, third];
    let ctx = egui::Context::default();
    app.open_root_dropdown(&ctx);

    ctx.begin_pass(egui::RawInput {
        modifiers: emacs_shortcut_modifiers(false),
        events: vec![egui::Event::Key {
            key: egui::Key::N,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: emacs_shortcut_modifiers(false),
        }],
        ..Default::default()
    });
    app.handle_shortcuts(&ctx);
    assert_eq!(app.shell.ui.root_dropdown_highlight, Some(1));
    let _ = ctx.end_pass();

    ctx.begin_pass(egui::RawInput {
        modifiers: emacs_shortcut_modifiers(false),
        events: vec![egui::Event::Key {
            key: egui::Key::P,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: emacs_shortcut_modifiers(false),
        }],
        ..Default::default()
    });
    app.handle_shortcuts(&ctx);
    assert_eq!(app.shell.ui.root_dropdown_highlight, Some(0));
    let _ = ctx.end_pass();

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn root_dropdown_ctrl_g_closes_without_clearing_query() {
    let root = test_root("root-dropdown-ctrl-g-close");
    let second = root.join("second");
    fs::create_dir_all(&root).expect("create dir");
    fs::create_dir_all(&second).expect("create second");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "draft".to_string());
    app.shell.features.root_browser.saved_roots = vec![root.clone(), second];
    let ctx = egui::Context::default();
    app.open_root_dropdown(&ctx);

    ctx.begin_pass(egui::RawInput {
        modifiers: emacs_shortcut_modifiers(false),
        events: vec![egui::Event::Key {
            key: egui::Key::G,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: emacs_shortcut_modifiers(false),
        }],
        ..Default::default()
    });
    app.handle_shortcuts(&ctx);

    assert!(!app.is_root_dropdown_open(&ctx));
    assert_eq!(app.shell.runtime.query_state.query, "draft");
    let _ = ctx.end_pass();
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn root_dropdown_ctrl_j_and_ctrl_m_accept_selection() {
    for (name, key) in [
        ("root-dropdown-ctrl-j-accept", egui::Key::J),
        ("root-dropdown-ctrl-m-accept", egui::Key::M),
    ] {
        let root = test_root(name);
        let second = root.join("second");
        fs::create_dir_all(&root).expect("create dir");
        fs::create_dir_all(&second).expect("create second");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.shell.features.root_browser.saved_roots = vec![root.clone(), second.clone()];
        let ctx = egui::Context::default();
        app.open_root_dropdown(&ctx);
        app.move_root_dropdown_selection(1);

        ctx.begin_pass(egui::RawInput {
            modifiers: emacs_shortcut_modifiers(false),
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: emacs_shortcut_modifiers(false),
            }],
            ..Default::default()
        });
        app.handle_shortcuts(&ctx);

        assert!(!app.is_root_dropdown_open(&ctx));
        assert_eq!(app.shell.runtime.root, second);
        let _ = ctx.end_pass();
        let _ = fs::remove_dir_all(&root);
    }
}

#[test]
fn history_search_enter_accepts_selected_query() {
    let root = test_root("history-search-enter");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "draft".to_string());
    app.shell.runtime.query_state.query_history =
        VecDeque::from(["alpha".to_string(), "beta".to_string(), "gamma".to_string()]);
    app.start_history_search();
    app.shell.runtime.query_state.history_search_query = "be".to_string();
    app.refresh_history_search_results();

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );

    assert!(!app.shell.runtime.query_state.history_search_active);
    assert_eq!(app.shell.runtime.query_state.query, "beta");
    assert_eq!(app.shell.runtime.notice, "Loaded query from history");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn history_search_ctrl_j_and_ctrl_m_accept_selected_query() {
    for (name, key) in [
        ("history-search-ctrl-j", egui::Key::J),
        ("history-search-ctrl-m", egui::Key::M),
    ] {
        let root = test_root(name);
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, "draft".to_string());
        app.shell.runtime.query_state.query_history =
            VecDeque::from(["alpha".to_string(), "beta".to_string(), "gamma".to_string()]);
        app.start_history_search();
        app.shell.runtime.query_state.history_search_query = "ga".to_string();
        app.refresh_history_search_results();

        run_shortcuts_frame(
            &mut app,
            true,
            vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: emacs_shortcut_modifiers(false),
            }],
        );

        assert!(!app.shell.runtime.query_state.history_search_active);
        assert_eq!(app.shell.runtime.query_state.query, "gamma");
        let _ = fs::remove_dir_all(&root);
    }
}

#[test]
fn history_search_escape_and_ctrl_g_cancel_and_restore_original_query() {
    for (name, event) in [
        (
            "history-search-escape-cancel",
            egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
        ),
        (
            "history-search-ctrl-g-cancel",
            egui::Event::Key {
                key: egui::Key::G,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: emacs_shortcut_modifiers(false),
            },
        ),
    ] {
        let root = test_root(name);
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, "draft".to_string());
        app.shell.runtime.query_state.query_history =
            VecDeque::from(["alpha".to_string(), "beta".to_string(), "gamma".to_string()]);
        app.start_history_search();
        app.shell.runtime.query_state.history_search_query = "ga".to_string();
        app.refresh_history_search_results();

        run_shortcuts_frame(&mut app, true, vec![event]);

        assert!(!app.shell.runtime.query_state.history_search_active);
        assert_eq!(app.shell.runtime.query_state.query, "draft");
        assert_eq!(app.shell.runtime.notice, "Canceled history search");
        let _ = fs::remove_dir_all(&root);
    }
}

#[test]
fn ctrl_shift_c_is_deferred_and_copies_selected_path_even_when_query_is_focused() {
    let root = test_root("shortcut-copy-query-focus");
    fs::create_dir_all(&root).expect("create dir");
    let selected = root.join("picked.txt");
    fs::write(&selected, "x").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "query".to_string());
    app.shell.runtime.results = vec![(selected.clone(), 0.0)];
    app.shell.runtime.current_row = Some(0);

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::C,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: gui_shortcut_modifiers(true),
        }],
    );

    assert!(!app.shell.ui.pending_copy_shortcut);
    assert!(app.shell.runtime.notice.contains(&format!(
        "Copied path: {}",
        normalize_path_for_display(&selected)
    )));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ctrl_o_browses_and_changes_root() {
    let root = test_root("shortcut-ctrl-o");
    let new_root = root.join("new-root");
    fs::create_dir_all(&root).expect("create dir");
    fs::create_dir_all(&new_root).expect("create new root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.features.root_browser.browse_dialog_result = Some(Ok(Some(new_root.clone())));

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::O,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: gui_shortcut_modifiers(false),
        }],
    );

    assert_eq!(app.shell.runtime.root, new_root);
    assert_eq!(app.shell.tabs.len(), 1);
    assert_eq!(app.shell.tabs.active_tab, 0);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ctrl_shift_o_browses_in_new_tab() {
    let root = test_root("shortcut-ctrl-shift-o");
    let new_root = root.join("new-root");
    fs::create_dir_all(&root).expect("create dir");
    fs::create_dir_all(&new_root).expect("create new root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let original_tab_id = app.shell.tabs[0].id;
    app.shell.features.root_browser.browse_dialog_result = Some(Ok(Some(new_root.clone())));

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::O,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: gui_shortcut_modifiers(true),
        }],
    );

    assert_eq!(app.shell.tabs.len(), 2);
    assert_eq!(app.shell.tabs.active_tab, 1);
    assert_eq!(app.shell.runtime.root, new_root);
    assert_eq!(app.shell.tabs[0].id, original_tab_id);
    assert_eq!(app.shell.tabs[0].root, root);
    assert_eq!(app.shell.tabs[1].root, new_root);
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
fn tab_toggles_pin_without_moving_current_row_when_query_not_focused() {
    let root = test_root("shortcut-tab-pin-no-focus");
    fs::create_dir_all(&root).expect("create dir");
    let selected = root.join("picked.txt");
    fs::write(&selected, "x").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.results = vec![(selected.clone(), 0.0), (root.join("next.txt"), 0.0)];
    app.shell.runtime.current_row = Some(0);

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(app.shell.runtime.pinned_paths.contains(&selected));
    assert_eq!(app.shell.runtime.current_row, Some(0));

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(!app.shell.runtime.pinned_paths.contains(&selected));
    assert_eq!(app.shell.runtime.current_row, Some(0));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tab_toggles_pin_without_moving_current_row_when_query_focused() {
    let root = test_root("shortcut-tab-pin-query-focus");
    fs::create_dir_all(&root).expect("create dir");
    let selected = root.join("picked.txt");
    fs::write(&selected, "x").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.results = vec![(selected.clone(), 0.0), (root.join("next.txt"), 0.0)];
    app.shell.runtime.current_row = Some(0);

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(app.shell.runtime.pinned_paths.contains(&selected));
    assert_eq!(app.shell.runtime.current_row, Some(0));

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(!app.shell.runtime.pinned_paths.contains(&selected));
    assert_eq!(app.shell.runtime.current_row, Some(0));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn regression_tab_shortcut_clears_focus_traversal_target() {
    let root = test_root("regression-tab-focus-traversal");
    fs::create_dir_all(&root).expect("create dir");
    let selected = root.join("picked.txt");
    fs::write(&selected, "x").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.results = vec![(selected.clone(), 0.0)];
    app.shell.runtime.current_row = Some(0);
    let ctx = egui::Context::default();
    let dummy_focus = egui::Id::new("dummy-focus");
    ctx.memory_mut(|m| m.request_focus(dummy_focus));

    ctx.begin_pass(egui::RawInput {
        modifiers: egui::Modifiers::NONE,
        events: vec![egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        ..Default::default()
    });
    app.handle_shortcuts_with_focus(&ctx, false);
    let focused_after = ctx.memory(|m| m.focused());
    let _ = ctx.end_pass();

    assert!(app.shell.runtime.pinned_paths.contains(&selected));
    assert_eq!(app.shell.runtime.current_row, Some(0));
    assert!(focused_after.is_none());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn regression_tab_keeps_query_focus_when_query_is_active() {
    let root = test_root("regression-tab-keep-query-focus");
    fs::create_dir_all(&root).expect("create dir");
    let selected = root.join("picked.txt");
    fs::write(&selected, "x").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.results = vec![(selected.clone(), 0.0)];
    app.shell.runtime.current_row = Some(0);
    let ctx = egui::Context::default();
    ctx.memory_mut(|m| m.request_focus(app.shell.ui.query_input_id));

    ctx.begin_pass(egui::RawInput {
        modifiers: egui::Modifiers::NONE,
        events: vec![egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        ..Default::default()
    });
    app.handle_shortcuts_with_focus(&ctx, true);
    let query_still_focused = ctx.memory(|m| m.has_focus(app.shell.ui.query_input_id));
    let _ = ctx.end_pass();

    assert!(app.shell.runtime.pinned_paths.contains(&selected));
    assert!(query_still_focused);
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

#[test]
fn regression_ctrl_i_toggles_pin_regardless_of_query_focus() {
    let root = test_root("regression-ctrl-i-pin-toggle");
    fs::create_dir_all(&root).expect("create dir");
    let selected = root.join("picked.txt");
    fs::write(&selected, "x").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.results = vec![(selected.clone(), 0.0), (root.join("next.txt"), 0.0)];
    app.shell.runtime.current_row = Some(0);

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::I,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: emacs_shortcut_modifiers(false),
        }],
    );
    assert!(app.shell.runtime.pinned_paths.contains(&selected));
    assert_eq!(app.shell.runtime.current_row, Some(0));

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::I,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: emacs_shortcut_modifiers(false),
        }],
    );
    assert!(!app.shell.runtime.pinned_paths.contains(&selected));
    assert_eq!(app.shell.runtime.current_row, Some(0));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn regression_ctrl_j_and_ctrl_m_execute_even_when_query_focused() {
    let root = test_root("regression-ctrl-jm-query-focus");
    fs::create_dir_all(&root).expect("create dir");
    #[cfg(target_os = "windows")]
    let selected = root.join("picked.exe");
    #[cfg(not(target_os = "windows"))]
    let selected = root.join("picked.sh");
    fs::write(&selected, "echo test").expect("write file");
    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&selected).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&selected, perms).expect("set permissions");
    }
    let mut app = FlistWalkerApp::new(root.clone(), 50, "query".to_string());
    app.shell.runtime.results = vec![(selected.clone(), 0.0)];
    app.shell.runtime.current_row = Some(0);

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::J,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: emacs_shortcut_modifiers(false),
        }],
    );
    assert!(is_action_notice(&app.shell.runtime.notice));

    app.shell.runtime.notice.clear();
    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::M,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: emacs_shortcut_modifiers(false),
        }],
    );
    assert!(is_action_notice(&app.shell.runtime.notice));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn regression_enter_executes_regardless_of_query_focus() {
    let root = test_root("regression-enter-query-focus");
    fs::create_dir_all(&root).expect("create dir");
    #[cfg(target_os = "windows")]
    let selected = root.join("picked.exe");
    #[cfg(not(target_os = "windows"))]
    let selected = root.join("picked.sh");
    fs::write(&selected, "echo test").expect("write file");
    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&selected).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&selected, perms).expect("set permissions");
    }
    let mut app = FlistWalkerApp::new(root.clone(), 50, "query".to_string());
    app.shell.runtime.results = vec![(selected, 0.0)];
    app.shell.runtime.current_row = Some(0);

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(is_action_notice(&app.shell.runtime.notice));

    app.shell.runtime.notice.clear();
    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(is_action_notice(&app.shell.runtime.notice));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn regression_shift_enter_opens_containing_folder_regardless_of_query_focus() {
    let root = test_root("regression-shift-enter-query-focus");
    let folder = root.join("src");
    fs::create_dir_all(&folder).expect("create dir");
    let selected_file = folder.join("picked.txt");
    fs::write(&selected_file, "x").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "query".to_string());
    let (action_tx_req, action_rx_req) = mpsc::channel::<ActionRequest>();
    let (_action_tx_res, action_rx_res) = mpsc::channel::<ActionResponse>();
    app.shell.worker_bus.action.tx = action_tx_req;
    app.shell.worker_bus.action.rx = action_rx_res;
    app.shell.runtime.results = vec![(selected_file.clone(), 0.0)];
    app.shell.runtime.current_row = Some(0);

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                shift: true,
                ..Default::default()
            },
        }],
    );
    let req1 = action_rx_req
        .try_recv()
        .expect("action request should be enqueued");
    assert_eq!(req1.paths, vec![selected_file.clone()]);
    assert!(req1.open_parent_for_files);

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                shift: true,
                ..Default::default()
            },
        }],
    );
    let req2 = action_rx_req
        .try_recv()
        .expect("action request should be enqueued");
    assert_eq!(req2.paths, vec![selected_file]);
    assert!(req2.open_parent_for_files);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn deferred_copy_shortcut_copies_selected_path_even_with_query_text() {
    let root = test_root("deferred-copy-shortcut");
    fs::create_dir_all(&root).expect("create dir");
    let selected = root.join("picked.txt");
    fs::write(&selected, "x").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "query text".to_string());
    app.shell.runtime.results = vec![(selected.clone(), 0.0)];
    app.shell.runtime.current_row = Some(0);
    app.shell.ui.pending_copy_shortcut = true;
    let ctx = egui::Context::default();

    app.run_deferred_shortcuts(&ctx);

    assert!(!app.shell.ui.pending_copy_shortcut);
    assert!(app.shell.ui.focus_query_requested);
    assert!(app.shell.runtime.notice.contains(&format!(
        "Copied path: {}",
        normalize_path_for_display(&selected)
    )));
    let _ = fs::remove_dir_all(&root);
}

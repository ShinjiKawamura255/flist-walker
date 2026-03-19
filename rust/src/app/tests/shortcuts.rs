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
    app.query = "abcd".to_string();
    let mut cursor = 4usize;
    let mut anchor = 4usize;

    let (text_changed, cursor_changed) = app.apply_ctrl_h_delete(&mut cursor, &mut anchor, false);

    assert!(text_changed);
    assert!(cursor_changed);
    assert_eq!(app.query, "abc");
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
    app.query = "abc".to_string();
    let mut cursor = 3usize;
    let mut anchor = 3usize;

    let (text_changed, cursor_changed) = app.apply_ctrl_h_delete(&mut cursor, &mut anchor, true);

    assert!(!text_changed);
    assert!(!cursor_changed);
    assert_eq!(app.query, "abc");
    assert_eq!(cursor, 3);
    assert_eq!(anchor, 3);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn move_page_moves_by_fixed_rows_and_clamps() {
    let root = test_root("move-page");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.results = (0..30)
        .map(|i| (root.join(format!("f{i}.txt")), 0.0))
        .collect();
    app.current_row = Some(0);

    app.move_page(1);
    assert_eq!(app.current_row, Some(10));

    app.move_page(-1);
    assert_eq!(app.current_row, Some(0));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ctrl_n_and_ctrl_p_move_selection_even_when_query_is_focused() {
    let root = test_root("shortcut-ctrl-np-query-focus");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.results = vec![
        (root.join("a.txt"), 0.0),
        (root.join("b.txt"), 0.0),
        (root.join("c.txt"), 0.0),
    ];
    app.current_row = Some(0);

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::N,
            pressed: true,
            repeat: false,
            modifiers: emacs_shortcut_modifiers(false),
        }],
    );
    assert_eq!(app.current_row, Some(1));

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::P,
            pressed: true,
            repeat: false,
            modifiers: emacs_shortcut_modifiers(false),
        }],
    );
    assert_eq!(app.current_row, Some(0));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ctrl_g_clears_query_and_resets_selection_even_when_query_is_focused() {
    let root = test_root("shortcut-ctrl-g-query-focus");
    fs::create_dir_all(&root).expect("create dir");
    let selected = root.join("picked.txt");
    fs::write(&selected, "x").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "query".to_string());
    app.entries = Arc::new(vec![selected.clone()]);
    app.results = vec![(selected.clone(), 0.0)];
    app.current_row = Some(0);
    app.pinned_paths.insert(selected);

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::G,
            pressed: true,
            repeat: false,
            modifiers: emacs_shortcut_modifiers(false),
        }],
    );

    assert!(app.query.is_empty());
    assert!(app.pinned_paths.is_empty());
    assert_eq!(app.results.len(), 1);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn escape_clears_query_and_resets_selection_even_when_query_is_focused() {
    let root = test_root("shortcut-escape-query-focus");
    fs::create_dir_all(&root).expect("create dir");
    let selected = root.join("picked.txt");
    fs::write(&selected, "x").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "query".to_string());
    app.entries = Arc::new(vec![selected.clone()]);
    app.results = vec![(selected.clone(), 0.0)];
    app.current_row = Some(0);
    app.pinned_paths.insert(selected);

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );

    assert!(app.query.is_empty());
    assert!(app.pinned_paths.is_empty());
    assert_eq!(app.results.len(), 1);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ctrl_shift_r_does_not_start_history_search() {
    let root = test_root("shortcut-ctrl-shift-r-noop");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "draft".to_string());
    app.query_history =
        VecDeque::from(["alpha".to_string(), "beta".to_string(), "gamma".to_string()]);

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::R,
            pressed: true,
            repeat: false,
            modifiers: emacs_shortcut_modifiers(true),
        }],
    );

    assert!(!app.history_search_active);
    assert_eq!(app.query, "draft");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn history_search_enter_accepts_selected_query() {
    let root = test_root("history-search-enter");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "draft".to_string());
    app.query_history =
        VecDeque::from(["alpha".to_string(), "beta".to_string(), "gamma".to_string()]);
    app.start_history_search();
    app.history_search_query = "be".to_string();
    app.refresh_history_search_results();

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::Enter,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );

    assert!(!app.history_search_active);
    assert_eq!(app.query, "beta");
    assert_eq!(app.notice, "Loaded query from history");
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
        app.query_history =
            VecDeque::from(["alpha".to_string(), "beta".to_string(), "gamma".to_string()]);
        app.start_history_search();
        app.history_search_query = "ga".to_string();
        app.refresh_history_search_results();

        run_shortcuts_frame(
            &mut app,
            true,
            vec![egui::Event::Key {
                key,
                pressed: true,
                repeat: false,
                modifiers: emacs_shortcut_modifiers(false),
            }],
        );

        assert!(!app.history_search_active);
        assert_eq!(app.query, "gamma");
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
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
        ),
        (
            "history-search-ctrl-g-cancel",
            egui::Event::Key {
                key: egui::Key::G,
                pressed: true,
                repeat: false,
                modifiers: emacs_shortcut_modifiers(false),
            },
        ),
    ] {
        let root = test_root(name);
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, "draft".to_string());
        app.query_history =
            VecDeque::from(["alpha".to_string(), "beta".to_string(), "gamma".to_string()]);
        app.start_history_search();
        app.history_search_query = "ga".to_string();
        app.refresh_history_search_results();

        run_shortcuts_frame(&mut app, true, vec![event]);

        assert!(!app.history_search_active);
        assert_eq!(app.query, "draft");
        assert_eq!(app.notice, "Canceled history search");
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
    app.results = vec![(selected.clone(), 0.0)];
    app.current_row = Some(0);

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::C,
            pressed: true,
            repeat: false,
            modifiers: gui_shortcut_modifiers(true),
        }],
    );

    assert!(!app.pending_copy_shortcut);
    assert!(app.notice.contains(&format!(
        "Copied path: {}",
        normalize_path_for_display(&selected)
    )));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tab_toggles_pin_without_moving_current_row_when_query_not_focused() {
    let root = test_root("shortcut-tab-pin-no-focus");
    fs::create_dir_all(&root).expect("create dir");
    let selected = root.join("picked.txt");
    fs::write(&selected, "x").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.results = vec![(selected.clone(), 0.0), (root.join("next.txt"), 0.0)];
    app.current_row = Some(0);

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::Tab,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(app.pinned_paths.contains(&selected));
    assert_eq!(app.current_row, Some(0));

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::Tab,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(!app.pinned_paths.contains(&selected));
    assert_eq!(app.current_row, Some(0));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tab_toggles_pin_without_moving_current_row_when_query_focused() {
    let root = test_root("shortcut-tab-pin-query-focus");
    fs::create_dir_all(&root).expect("create dir");
    let selected = root.join("picked.txt");
    fs::write(&selected, "x").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.results = vec![(selected.clone(), 0.0), (root.join("next.txt"), 0.0)];
    app.current_row = Some(0);

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::Tab,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(app.pinned_paths.contains(&selected));
    assert_eq!(app.current_row, Some(0));

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::Tab,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(!app.pinned_paths.contains(&selected));
    assert_eq!(app.current_row, Some(0));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn regression_tab_shortcut_clears_focus_traversal_target() {
    let root = test_root("regression-tab-focus-traversal");
    fs::create_dir_all(&root).expect("create dir");
    let selected = root.join("picked.txt");
    fs::write(&selected, "x").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.results = vec![(selected.clone(), 0.0)];
    app.current_row = Some(0);
    let ctx = egui::Context::default();
    let dummy_focus = egui::Id::new("dummy-focus");
    ctx.memory_mut(|m| m.request_focus(dummy_focus));

    ctx.begin_frame(egui::RawInput {
        modifiers: egui::Modifiers::NONE,
        events: vec![egui::Event::Key {
            key: egui::Key::Tab,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        ..Default::default()
    });
    app.handle_shortcuts_with_focus(&ctx, false);
    let focused_after = ctx.memory(|m| m.focus());
    let _ = ctx.end_frame();

    assert!(app.pinned_paths.contains(&selected));
    assert_eq!(app.current_row, Some(0));
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
    app.results = vec![(selected.clone(), 0.0)];
    app.current_row = Some(0);
    let ctx = egui::Context::default();
    ctx.memory_mut(|m| m.request_focus(app.query_input_id));

    ctx.begin_frame(egui::RawInput {
        modifiers: egui::Modifiers::NONE,
        events: vec![egui::Event::Key {
            key: egui::Key::Tab,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        ..Default::default()
    });
    app.handle_shortcuts_with_focus(&ctx, true);
    let query_still_focused = ctx.memory(|m| m.has_focus(app.query_input_id));
    let _ = ctx.end_frame();

    assert!(app.pinned_paths.contains(&selected));
    assert!(query_still_focused);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn regression_arrow_keys_move_selection_even_when_query_focused() {
    let root = test_root("regression-arrow-query-focus");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.results = vec![
        (root.join("a.txt"), 0.0),
        (root.join("b.txt"), 0.0),
        (root.join("c.txt"), 0.0),
    ];
    app.current_row = Some(0);

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::ArrowDown,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert_eq!(app.current_row, Some(1));

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::ArrowUp,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert_eq!(app.current_row, Some(0));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn regression_ctrl_i_toggles_pin_regardless_of_query_focus() {
    let root = test_root("regression-ctrl-i-pin-toggle");
    fs::create_dir_all(&root).expect("create dir");
    let selected = root.join("picked.txt");
    fs::write(&selected, "x").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.results = vec![(selected.clone(), 0.0), (root.join("next.txt"), 0.0)];
    app.current_row = Some(0);

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::I,
            pressed: true,
            repeat: false,
            modifiers: emacs_shortcut_modifiers(false),
        }],
    );
    assert!(app.pinned_paths.contains(&selected));
    assert_eq!(app.current_row, Some(0));

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::I,
            pressed: true,
            repeat: false,
            modifiers: emacs_shortcut_modifiers(false),
        }],
    );
    assert!(!app.pinned_paths.contains(&selected));
    assert_eq!(app.current_row, Some(0));
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
    app.results = vec![(selected.clone(), 0.0)];
    app.current_row = Some(0);

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::J,
            pressed: true,
            repeat: false,
            modifiers: emacs_shortcut_modifiers(false),
        }],
    );
    assert!(is_action_notice(&app.notice));

    app.notice.clear();
    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::M,
            pressed: true,
            repeat: false,
            modifiers: emacs_shortcut_modifiers(false),
        }],
    );
    assert!(is_action_notice(&app.notice));
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
    app.results = vec![(selected, 0.0)];
    app.current_row = Some(0);

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::Enter,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(is_action_notice(&app.notice));

    app.notice.clear();
    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::Enter,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(is_action_notice(&app.notice));
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
    app.action_tx = action_tx_req;
    app.action_rx = action_rx_res;
    app.results = vec![(selected_file.clone(), 0.0)];
    app.current_row = Some(0);

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::Enter,
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
    app.results = vec![(selected.clone(), 0.0)];
    app.current_row = Some(0);
    app.pending_copy_shortcut = true;
    let ctx = egui::Context::default();

    app.run_deferred_shortcuts(&ctx);

    assert!(!app.pending_copy_shortcut);
    assert!(app.focus_query_requested);
    assert!(app.notice.contains(&format!(
        "Copied path: {}",
        normalize_path_for_display(&selected)
    )));
    let _ = fs::remove_dir_all(&root);
}

use super::*;

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

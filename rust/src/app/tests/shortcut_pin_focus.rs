use super::*;

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

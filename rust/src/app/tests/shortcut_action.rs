use super::*;

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
fn regression_ctrl_shift_c_copy_event_copies_selected_path_even_when_query_is_focused() {
    let root = test_root("regression-copy-event-query-focus");
    fs::create_dir_all(&root).expect("create dir");
    let selected = root.join("picked.txt");
    fs::write(&selected, "x").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "query".to_string());
    app.shell.runtime.results = vec![(selected.clone(), 0.0)];
    app.shell.runtime.current_row = Some(0);

    run_shortcuts_frame_with_modifiers(
        &mut app,
        true,
        gui_shortcut_modifiers(true),
        vec![egui::Event::Copy],
    );

    assert!(!app.shell.ui.pending_copy_shortcut);
    assert!(app.shell.runtime.notice.contains(&format!(
        "Copied path: {}",
        normalize_path_for_display(&selected)
    )));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn copy_event_without_shift_does_not_trigger_path_copy_shortcut() {
    let root = test_root("copy-event-without-shift");
    fs::create_dir_all(&root).expect("create dir");
    let selected = root.join("picked.txt");
    fs::write(&selected, "x").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "query".to_string());
    app.shell.runtime.results = vec![(selected, 0.0)];
    app.shell.runtime.current_row = Some(0);

    run_shortcuts_frame_with_modifiers(
        &mut app,
        true,
        gui_shortcut_modifiers(false),
        vec![egui::Event::Copy],
    );

    assert!(!app.shell.ui.pending_copy_shortcut);
    assert!(app.shell.runtime.notice.is_empty());
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
    let original_tab_id = app.shell.tabs.get(0).expect("tab 0").id;
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
    assert_eq!(app.shell.tabs.get(0).expect("tab 0").id, original_tab_id);
    assert_eq!(app.shell.tabs.get(0).expect("tab 0").root, root);
    assert_eq!(app.shell.tabs.get(1).expect("tab 1").root, new_root);
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

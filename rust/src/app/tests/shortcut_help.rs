use super::*;

fn key_event(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

#[test]
fn f1_opens_help_and_escape_closes_it() {
    let root = test_root("shortcut-help-open-close");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

    run_shortcuts_frame(
        &mut app,
        true,
        vec![key_event(egui::Key::F1, egui::Modifiers::NONE)],
    );
    assert!(app.shell.ui.help_open);

    run_shortcuts_frame(
        &mut app,
        true,
        vec![key_event(egui::Key::Escape, egui::Modifiers::NONE)],
    );
    assert!(!app.shell.ui.help_open);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn open_help_consumes_background_copy_shortcut() {
    let root = test_root("shortcut-help-blocks-copy");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.ui.help_open = true;

    run_shortcuts_frame(
        &mut app,
        true,
        vec![key_event(egui::Key::C, gui_shortcut_modifiers(true))],
    );

    assert!(!app.shell.ui.pending_copy_shortcut);
    assert!(app.shell.ui.help_open);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn gui_help_lines_follow_platform_and_emacs_settings() {
    let enabled = FlistWalkerApp::gui_help_lines(true, true).join("\n");
    let enabled_without_ctrl_w = FlistWalkerApp::gui_help_lines(true, false).join("\n");
    let disabled = FlistWalkerApp::gui_help_lines(false, true).join("\n");
    let primary = FlistWalkerApp::primary_shortcut_label();

    assert!(enabled.contains(&format!("{primary}+T")));
    assert!(enabled.contains(&format!("{primary}+Shift+P")));
    assert!(enabled.contains("Type to filter preset names"));
    assert!(enabled.contains("Enter to apply"));
    assert!(enabled.contains("F2 — Edit the selected preset"));
    assert!(enabled.contains("Add / Delete — Save the current pure-search state"));
    assert!(enabled.contains(&format!("{primary}+Enter — Save the preset draft")));
    assert!(enabled.contains("Named roots"));
    assert!(enabled.contains("F2 to edit and Delete to remove"));
    assert!(enabled.contains("Ctrl+N / Ctrl+P"));
    assert!(enabled.contains("Query syntax"));
    assert!(enabled.contains("name:TERM"));
    assert!(enabled.contains("path:TERM"));
    assert!(enabled.contains("dir:TERM"));
    assert!(enabled.contains("ext:EXT"));
    assert!(enabled.contains("dir:src ext:rs !dir:target"));
    assert!(enabled.contains("Ctrl+W / Ctrl+K"));
    assert!(!enabled_without_ctrl_w.contains("Ctrl+W / Ctrl+K"));
    assert!(enabled_without_ctrl_w.contains("Ctrl+K — Delete through the end"));
    assert!(disabled.contains(&format!("{primary}+T")));
    assert!(!disabled.contains("Ctrl+N / Ctrl+P"));
    assert!(disabled.contains("Emacs-style shortcuts are disabled"));
}

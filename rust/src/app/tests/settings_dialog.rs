use super::*;
use crate::app::render::RenderCommand;
use crate::app::settings_dialog::SettingsView;
use crate::app::worker::config_open::ConfigOpenService;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

fn settle_settings_dialog(app: &mut FlistWalkerApp) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while app.shell.worker_bus.config_settings.in_progress() {
        app.poll_config_settings_response();
        assert!(Instant::now() < deadline, "settings worker did not settle");
        thread::yield_now();
    }
}

#[test]
fn gui_settings_draft_saves_for_next_launch_without_changing_current_session() {
    let scope = test_settings_scope("gui-settings-save");
    let path = scope.runtime_config_path();
    fs::write(
        &path,
        r#"{"restore_tabs_enabled":false,"walker_max_entries":500000,"extra":{"keep":true}}"#,
    )
    .expect("seed config");
    let mut app = scope.app(test_root("gui-settings-root"), 30, String::new());
    let old_keys = app.shell.runtime.emacs_keybindings_enabled;
    app.open_settings_dialog();
    settle_settings_dialog(&mut app);
    let SettingsView::Editing {
        draft, limit_text, ..
    } = &mut app.settings_dialog.view
    else {
        panic!("settings draft not loaded")
    };
    draft.restore_tabs_enabled = true;
    draft.emacs_keybindings_enabled = !old_keys;
    *limit_text = "72".into();
    app.request_settings_save();
    settle_settings_dialog(&mut app);
    assert!(matches!(app.settings_dialog.view, SettingsView::Closed));
    assert_eq!(app.shell.runtime.emacs_keybindings_enabled, old_keys);
    let saved: serde_json::Value =
        serde_json::from_slice(&fs::read(path).expect("saved config")).expect("valid JSON");
    assert_eq!(saved["restore_tabs_enabled"], true);
    assert_eq!(saved["walker_max_entries"], 72);
    assert_eq!(saved["extra"]["keep"], true);
}

#[test]
fn gui_settings_external_edit_keeps_draft_and_external_bytes() {
    let scope = test_settings_scope("gui-settings-conflict");
    let path = scope.runtime_config_path();
    fs::write(&path, "{}").expect("seed config");
    let mut app = scope.app(test_root("gui-settings-root"), 30, String::new());
    app.open_settings_dialog();
    settle_settings_dialog(&mut app);
    if let SettingsView::Editing { draft, .. } = &mut app.settings_dialog.view {
        draft.restore_tabs_enabled = true;
    } else {
        panic!("settings draft not loaded")
    }
    let external = r#"{"external":"editor"}"#;
    fs::write(&path, external).expect("external edit");
    app.request_settings_save();
    settle_settings_dialog(&mut app);
    let SettingsView::Editing { draft, error, .. } = &app.settings_dialog.view else {
        panic!("failed save must retain draft")
    };
    assert!(draft.restore_tabs_enabled);
    assert!(error
        .as_ref()
        .is_some_and(|message| message.contains("changed")));
    assert_eq!(fs::read_to_string(&path).expect("external bytes"), external);
    app.close_settings_dialog();
    assert!(matches!(app.settings_dialog.view, SettingsView::Closed));
}

#[test]
fn gui_settings_invalid_limit_and_cancel_do_not_write() {
    let scope = test_settings_scope("gui-settings-cancel");
    let path = scope.runtime_config_path();
    fs::write(&path, "{}").expect("seed config");
    let mut app = scope.app(test_root("gui-settings-root"), 30, String::new());
    app.open_settings_dialog();
    settle_settings_dialog(&mut app);
    if let SettingsView::Editing { limit_text, .. } = &mut app.settings_dialog.view {
        *limit_text = "0".into();
    } else {
        panic!("settings draft not loaded")
    }
    app.request_settings_save();
    assert!(!app.shell.worker_bus.config_settings.in_progress());
    assert_eq!(fs::read_to_string(&path).expect("unchanged"), "{}");
    app.close_settings_dialog();
    assert!(matches!(app.settings_dialog.view, SettingsView::Closed));
}

#[test]
fn gui_settings_reload_requires_explicit_discard_of_changed_draft() {
    let scope = test_settings_scope("gui-settings-reload");
    let path = scope.runtime_config_path();
    fs::write(&path, "{}").expect("seed config");
    let mut app = scope.app(test_root("gui-settings-root"), 30, String::new());
    app.open_settings_dialog();
    settle_settings_dialog(&mut app);
    if let SettingsView::Editing { draft, .. } = &mut app.settings_dialog.view {
        draft.restore_tabs_enabled = true;
    } else {
        panic!("settings draft not loaded")
    }
    app.request_settings_reload();
    assert!(matches!(
        app.settings_dialog.view,
        SettingsView::Editing {
            confirm_reload: true,
            ..
        }
    ));
    assert!(!app.shell.worker_bus.config_settings.in_progress());
    fs::write(&path, r#"{"walker_max_entries":123}"#).expect("external edit");
    app.request_settings_reload();
    settle_settings_dialog(&mut app);
    let SettingsView::Editing { draft, .. } = &app.settings_dialog.view else {
        panic!("reloaded state")
    };
    assert!(!draft.restore_tabs_enabled);
    assert_eq!(draft.walker_max_entries, 123);
}

#[test]
fn gui_settings_failed_reload_keeps_dirty_draft() {
    let scope = test_settings_scope("gui-settings-reload-failure");
    let path = scope.runtime_config_path();
    fs::write(&path, "{}").expect("seed config");
    let mut app = scope.app(test_root("gui-settings-root"), 30, String::new());
    app.open_settings_dialog();
    settle_settings_dialog(&mut app);
    if let SettingsView::Editing { draft, .. } = &mut app.settings_dialog.view {
        draft.restore_tabs_enabled = true;
    }
    app.request_settings_reload();
    fs::write(&path, "{").expect("external invalid edit");
    app.request_settings_reload();
    settle_settings_dialog(&mut app);
    let SettingsView::Editing { draft, error, .. } = &app.settings_dialog.view else {
        panic!("failed reload must restore draft")
    };
    assert!(draft.restore_tabs_enabled);
    assert!(error.is_some());
}

#[test]
fn gui_settings_cancel_loading_ignores_delayed_response() {
    let scope = test_settings_scope("gui-settings-cancel-loading");
    fs::write(scope.runtime_config_path(), "{}").expect("seed config");
    let mut app = scope.app(test_root("gui-settings-root"), 30, String::new());
    app.open_settings_dialog();
    app.close_settings_dialog();
    settle_settings_dialog(&mut app);
    assert!(matches!(app.settings_dialog.view, SettingsView::Closed));
}

#[test]
fn gui_settings_json_open_uses_existing_service_without_discarding_draft() {
    let scope = test_settings_scope("gui-settings-json-open");
    fs::write(scope.runtime_config_path(), "{}").expect("seed config");
    let mut app = scope.app(test_root("gui-settings-root"), 30, String::new());
    app.open_settings_dialog();
    settle_settings_dialog(&mut app);
    if let SettingsView::Editing { draft, .. } = &mut app.settings_dialog.view {
        draft.restore_tabs_enabled = true;
    }
    let expected = scope.runtime_config_path();
    let (fake, handle) =
        ConfigOpenService::spawn_with(Arc::new(AtomicBool::new(false)), move || {
            Ok(expected.clone())
        });
    let original = std::mem::replace(&mut app.shell.worker_bus.config_open, fake);
    drop(original);
    let ctx = egui::Context::default();
    app.queue_render_command(RenderCommand::OpenRuntimeConfig);
    app.dispatch_render_commands(&ctx);
    assert!(app.shell.worker_bus.config_open.in_progress());
    app.queue_render_command(RenderCommand::OpenRuntimeConfig);
    app.dispatch_render_commands(&ctx);
    let deadline = Instant::now() + Duration::from_secs(2);
    while app.shell.worker_bus.config_open.in_progress() {
        app.poll_config_open_response();
        assert!(Instant::now() < deadline, "fake opener did not settle");
        thread::yield_now();
    }
    let SettingsView::Editing { draft, .. } = &app.settings_dialog.view else {
        panic!("JSON open must keep settings draft")
    };
    assert!(draft.restore_tabs_enabled);
    assert_eq!(
        fs::read_to_string(scope.runtime_config_path()).unwrap(),
        "{}"
    );
    app.shell.worker_bus.config_open.disconnect();
    handle.join().expect("fake opener worker");
}

#[test]
fn gui_settings_modal_blocks_background_tab_close_shortcut() {
    let scope = test_settings_scope("gui-settings-shortcut");
    fs::write(scope.runtime_config_path(), "{}").expect("seed config");
    let mut app = scope.app(test_root("gui-settings-root"), 30, String::new());
    app.create_new_tab();
    let tabs_before = app.shell.tabs.len();
    app.open_settings_dialog();
    settle_settings_dialog(&mut app);
    let ctx = egui::Context::default();
    let modifiers = egui::Modifiers {
        command: true,
        ..Default::default()
    };
    let _ = ctx.run_ui(
        egui::RawInput {
            modifiers,
            events: vec![egui::Event::Key {
                key: egui::Key::W,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        },
        |ui| app.run_ui_frame(ui),
    );
    assert_eq!(app.shell.tabs.len(), tabs_before);
    assert!(app.settings_dialog.is_open());
}

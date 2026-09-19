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

#[test]
fn gui_settings_limit_uses_runtime_emacs_shortcuts_and_shared_kill_buffer() {
    let scope = test_settings_scope("gui-settings-emacs-limit");
    fs::write(
        scope.runtime_config_path(),
        r#"{"walker_max_entries":1234}"#,
    )
    .expect("seed config");
    let mut app = scope.app(test_root("gui-settings-emacs-root"), 30, String::new());
    app.shell.runtime.emacs_keybindings_enabled = true;
    app.open_settings_dialog();
    settle_settings_dialog(&mut app);
    if let SettingsView::Editing { draft, .. } = &mut app.settings_dialog.view {
        draft.emacs_keybindings_enabled = false;
    }
    let ctx = egui::Context::default();
    let input_id = egui::Id::new("settings-walker-entry-limit");
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| app.run_ui_frame(ui));
    app.clear_focus_query_request();
    ctx.memory_mut(|memory| memory.request_focus(input_id));
    let mut state = egui::widgets::text_edit::TextEditState::load(&ctx, input_id)
        .expect("settings limit text edit state");
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(2),
        )));
    state.store(&ctx, input_id);
    let modifiers = emacs_shortcut_modifiers(false);
    let key = |key| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    };
    let _ = ctx.run_ui(
        egui::RawInput {
            modifiers,
            events: vec![key(egui::Key::K)],
            ..Default::default()
        },
        |ui| app.run_ui_frame(ui),
    );
    let SettingsView::Editing { limit_text, .. } = &app.settings_dialog.view else {
        panic!("settings draft must remain open")
    };
    assert_eq!(limit_text, "12");
    assert_eq!(app.shell.runtime.query_state.kill_buffer, "34");
    let _ = ctx.run_ui(
        egui::RawInput {
            modifiers,
            events: vec![key(egui::Key::Y)],
            ..Default::default()
        },
        |ui| app.run_ui_frame(ui),
    );
    let SettingsView::Editing { limit_text, .. } = &app.settings_dialog.view else {
        panic!("settings draft must remain open")
    };
    assert_eq!(limit_text, "1234");
}

#[test]
fn tc_216_gui_settings_six_fields_round_trip_to_next_launch() {
    let scope = test_settings_scope("gui-settings-six-fields");
    let path = scope.runtime_config_path();
    fs::write(&path, r#"{"future_key":"retained"}"#).expect("seed config");
    let mut app = scope.app(test_root("gui-settings-six-fields-root"), 30, String::new());
    let effective_emacs = app.shell.runtime.emacs_keybindings_enabled;
    app.open_settings_dialog();
    settle_settings_dialog(&mut app);
    let expected = crate::runtime_config::EditableSettings {
        restore_tabs_enabled: true,
        history_persist_disabled: true,
        emacs_keybindings_enabled: false,
        ctrl_w_deletes_word_in_query: true,
        tab_pin_moves_to_next_row: true,
        walker_max_entries: 1,
    };
    let SettingsView::Editing {
        draft, limit_text, ..
    } = &mut app.settings_dialog.view
    else {
        panic!("settings draft not loaded")
    };
    *draft = expected.clone();
    *limit_text = "1".into();
    app.request_settings_save();
    settle_settings_dialog(&mut app);
    assert!(matches!(app.settings_dialog.view, SettingsView::Closed));
    assert_eq!(app.shell.runtime.emacs_keybindings_enabled, effective_emacs);
    let saved_bytes = fs::read(&path).expect("saved settings");
    let saved_json: serde_json::Value = serde_json::from_slice(&saved_bytes).expect("saved JSON");
    assert_eq!(saved_json["future_key"], "retained");
    let next_launch = crate::runtime_config::load_runtime_config_from_path(&path)
        .expect("next launch config through startup loader");
    assert_eq!(
        next_launch.restore_tabs_enabled,
        expected.restore_tabs_enabled
    );
    assert_eq!(
        next_launch.history_persist_disabled,
        expected.history_persist_disabled
    );
    assert_eq!(
        next_launch.emacs_keybindings_enabled,
        expected.emacs_keybindings_enabled
    );
    assert_eq!(
        next_launch.ctrl_w_deletes_word_in_query,
        expected.ctrl_w_deletes_word_in_query
    );
    assert_eq!(
        next_launch.tab_pin_moves_to_next_row,
        expected.tab_pin_moves_to_next_row
    );
    assert_eq!(next_launch.walker_max_entries, expected.walker_max_entries);
    assert!(
        !next_launch.emacs_keybindings_enabled && next_launch.ctrl_w_deletes_word_in_query,
        "dependent Ctrl+W preference is retained while Emacs shortcuts are disabled"
    );
    let mut restarted = scope.app(
        test_root("gui-settings-next-launch-root"),
        30,
        String::new(),
    );
    restarted.open_settings_dialog();
    settle_settings_dialog(&mut restarted);
    let SettingsView::Editing {
        draft, limit_text, ..
    } = &restarted.settings_dialog.view
    else {
        panic!("restarted settings draft not loaded")
    };
    assert_eq!(draft, &expected);
    assert_eq!(limit_text, "1");
}

#[test]
fn tc_216_gui_settings_rejects_every_invalid_limit_boundary_without_writing() {
    let scope = test_settings_scope("gui-settings-limit-boundaries");
    let path = scope.runtime_config_path();
    let original = r#"{"walker_max_entries":500000,"future_key":17}"#;
    fs::write(&path, original).expect("seed config");
    let mut app = scope.app(
        test_root("gui-settings-limit-boundaries-root"),
        30,
        String::new(),
    );
    app.open_settings_dialog();
    settle_settings_dialog(&mut app);
    for invalid in ["", "0", "-1", "1.5", "184467440737095516160"] {
        let SettingsView::Editing { limit_text, .. } = &mut app.settings_dialog.view else {
            panic!("invalid input must leave draft open")
        };
        *limit_text = invalid.into();
        app.request_settings_save();
        assert!(
            !app.shell.worker_bus.config_settings.in_progress(),
            "{invalid}"
        );
        assert!(
            matches!(
                app.settings_dialog.view,
                SettingsView::Editing { error: Some(_), .. }
            ),
            "{invalid}"
        );
        assert_eq!(
            fs::read_to_string(&path).expect("unchanged config"),
            original
        );
    }
    let SettingsView::Editing { limit_text, .. } = &mut app.settings_dialog.view else {
        panic!("settings draft remains open")
    };
    *limit_text = usize::MAX.to_string();
    app.request_settings_save();
    settle_settings_dialog(&mut app);
    assert!(matches!(app.settings_dialog.view, SettingsView::Closed));
    let saved: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).expect("saved config")).expect("valid JSON");
    assert_eq!(saved["walker_max_entries"], serde_json::json!(usize::MAX));
    assert_eq!(saved["future_key"], 17);
}

#[test]
fn tc_216_gui_history_checkbox_inverts_persist_disabled() {
    let scope = test_settings_scope("gui-settings-history-checkbox");
    fs::write(
        scope.runtime_config_path(),
        r#"{"history_persist_disabled":false}"#,
    )
    .expect("seed config");
    let mut app = scope.app(
        test_root("gui-settings-history-checkbox-root"),
        30,
        String::new(),
    );
    app.open_settings_dialog();
    settle_settings_dialog(&mut app);
    let ctx = egui::Context::default();
    let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 800.0));
    let _ = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(screen_rect),
            ..Default::default()
        },
        |ui| app.run_ui_frame(ui),
    );
    let output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(screen_rect),
            ..Default::default()
        },
        |ui| app.run_ui_frame(ui),
    );
    fn find_text<'a>(shape: &'a egui::Shape, label: &str) -> Option<&'a egui::epaint::TextShape> {
        match shape {
            egui::Shape::Text(text) if text.galley.text() == label => Some(text),
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| find_text(shape, label)),
            _ => None,
        }
    }
    let label = output
        .shapes
        .iter()
        .find_map(|shape| find_text(&shape.shape, "Save search history"))
        .expect("history checkbox rendered");
    let target = label.pos + egui::vec2(8.0, label.galley.size().y / 2.0);
    let press = egui::Event::PointerButton {
        pos: target,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    };
    let release = egui::Event::PointerButton {
        pos: target,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    };
    let _ = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(screen_rect),
            events: vec![egui::Event::PointerMoved(target), press],
            ..Default::default()
        },
        |ui| app.run_ui_frame(ui),
    );
    let _ = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(screen_rect),
            events: vec![release],
            ..Default::default()
        },
        |ui| app.run_ui_frame(ui),
    );
    let SettingsView::Editing { draft, .. } = &app.settings_dialog.view else {
        panic!("settings draft remains open")
    };
    assert!(draft.history_persist_disabled);
    app.request_settings_save();
    settle_settings_dialog(&mut app);
    let saved: serde_json::Value = serde_json::from_slice(
        &fs::read(scope.runtime_config_path()).expect("saved history setting"),
    )
    .expect("valid JSON");
    assert_eq!(saved["history_persist_disabled"], true);
}

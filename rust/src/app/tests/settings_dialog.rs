use super::*;
use crate::app::render::RenderCommand;
use crate::app::settings_dialog::SettingsView;
use crate::app::worker::config_open::ConfigOpenService;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

// A GUI owns the process-wide runtime config. Exercise each complete settings
// flow in its own process so parallel tests cannot inherit another app's edits.
fn isolated_settings_process(name: &str) -> bool {
    let full_name = format!("app::tests::settings_dialog::{name}");
    if std::env::var("FLISTWALKER_SETTINGS_TEST_CHILD")
        .ok()
        .as_deref()
        == Some(&full_name)
    {
        return false;
    }
    let output = std::process::Command::new(std::env::current_exe().expect("test executable"))
        .args(["--exact", &full_name, "--nocapture", "--test-threads=1"])
        .env("FLISTWALKER_SETTINGS_TEST_CHILD", &full_name)
        .output()
        .expect("isolated settings test");
    assert!(
        output.status.success(),
        "{full_name}:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    true
}

// Request bookkeeping and UI state are not physical worker timing observations.
// This formatting is evaluated only by a failed assertion (or its focused test).
fn settings_wait_diagnostics(app: &FlistWalkerApp, deadline: Instant) -> String {
    let phase = match &app.settings_dialog.view {
        SettingsView::Closed => "closed",
        SettingsView::Loading => "loading",
        SettingsView::Reloading { .. } => "reloading",
        SettingsView::Editing { .. } => "editing",
        SettingsView::Saving { .. } => "saving",
        SettingsView::Failed(_) => "failed",
    };
    let pending = app
        .shell
        .worker_bus
        .config_settings
        .pending_operation_for_test();
    let generation = pending.map_or_else(
        || "none".to_string(),
        |(generation, _)| generation.to_string(),
    );
    let operation = pending.map_or("none", |(_, is_save)| if is_save { "save" } else { "load" });
    format!(
        "settings worker did not settle; ui_phase={phase} expected_generation={} pending_generation={generation} pending_operation={operation} overdue_ms={} (after poll; physical worker phase unobserved)",
        app.settings_dialog.generation,
        Instant::now().saturating_duration_since(deadline).as_millis(),
    )
}

fn settle_settings_dialog(app: &mut FlistWalkerApp) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while app.shell.worker_bus.config_settings.in_progress() {
        app.poll_config_settings_response();
        assert!(
            Instant::now() < deadline,
            "{}",
            settings_wait_diagnostics(app, deadline)
        );
        thread::yield_now();
    }
}

#[test]
fn gui_settings_draft_save_updates_current_session_after_success() {
    if isolated_settings_process("gui_settings_draft_save_updates_current_session_after_success") {
        return;
    }
    let _config_guard = crate::runtime_config::runtime_config_test_guard();
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
    assert_eq!(app.shell.runtime.emacs_keybindings_enabled, !old_keys);
    let saved: serde_json::Value =
        serde_json::from_slice(&fs::read(path).expect("saved config")).expect("valid JSON");
    assert_eq!(saved["restore_tabs_enabled"], true);
    assert_eq!(saved["walker_max_entries"], 72);
    assert_eq!(saved["extra"]["keep"], true);
}

#[test]
fn gui_settings_external_edit_keeps_draft_and_external_bytes() {
    if isolated_settings_process("gui_settings_external_edit_keeps_draft_and_external_bytes") {
        return;
    }
    let _config_guard = crate::runtime_config::runtime_config_test_guard();
    let scope = test_settings_scope("gui-settings-conflict");
    let path = scope.runtime_config_path();
    fs::write(&path, "{}").expect("seed config");
    let mut app = scope.app(test_root("gui-settings-root"), 30, String::new());
    app.open_settings_dialog();
    settle_settings_dialog(&mut app);
    if let SettingsView::Editing { draft, .. } = &mut app.settings_dialog.view {
        draft.restore_tabs_enabled = true;
        draft.filelist_auto_check_enabled = false;
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
    assert!(!draft.filelist_auto_check_enabled);
    assert!(error
        .as_ref()
        .is_some_and(|message| message.contains("changed")));
    assert_eq!(fs::read_to_string(&path).expect("external bytes"), external);
    app.close_settings_dialog();
    assert!(matches!(app.settings_dialog.view, SettingsView::Closed));
}

#[test]
fn gui_settings_invalid_limit_and_cancel_do_not_write() {
    if isolated_settings_process("gui_settings_invalid_limit_and_cancel_do_not_write") {
        return;
    }
    let _config_guard = crate::runtime_config::runtime_config_test_guard();
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
    if isolated_settings_process("gui_settings_reload_requires_explicit_discard_of_changed_draft") {
        return;
    }
    let _config_guard = crate::runtime_config::runtime_config_test_guard();
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
    if isolated_settings_process("gui_settings_failed_reload_keeps_dirty_draft") {
        return;
    }
    let _config_guard = crate::runtime_config::runtime_config_test_guard();
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
    if isolated_settings_process("gui_settings_cancel_loading_ignores_delayed_response") {
        return;
    }
    let _config_guard = crate::runtime_config::runtime_config_test_guard();
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
    if isolated_settings_process(
        "gui_settings_json_open_uses_existing_service_without_discarding_draft",
    ) {
        return;
    }
    let _config_guard = crate::runtime_config::runtime_config_test_guard();
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
    if isolated_settings_process("gui_settings_modal_blocks_background_tab_close_shortcut") {
        return;
    }
    let _config_guard = crate::runtime_config::runtime_config_test_guard();
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
    if isolated_settings_process(
        "gui_settings_limit_uses_runtime_emacs_shortcuts_and_shared_kill_buffer",
    ) {
        return;
    }
    let _config_guard = crate::runtime_config::runtime_config_test_guard();
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
fn tc_216_gui_settings_all_fields_round_trip_to_next_launch() {
    if isolated_settings_process("tc_216_gui_settings_all_fields_round_trip_to_next_launch") {
        return;
    }
    let _config_guard = crate::runtime_config::runtime_config_test_guard();
    let scope = test_settings_scope("gui-settings-six-fields");
    let path = scope.runtime_config_path();
    fs::write(&path, r#"{"future_key":"retained"}"#).expect("seed config");
    let mut app = scope.app(test_root("gui-settings-six-fields-root"), 30, String::new());
    app.open_settings_dialog();
    settle_settings_dialog(&mut app);
    let expected = crate::runtime_config::EditableSettings {
        restore_tabs_enabled: true,
        restore_queries_enabled: false,
        theme: crate::runtime_config::ThemeMode::Dark,
        history_persist_disabled: true,
        emacs_keybindings_enabled: false,
        ctrl_w_deletes_word_in_query: true,
        tab_pin_moves_to_next_row: true,
        modifier_click_pins_enabled: false,
        walker_max_entries: 1,
        filelist_auto_check_enabled: false,
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
    assert_eq!(
        app.shell.runtime.emacs_keybindings_enabled,
        expected.emacs_keybindings_enabled
    );
    assert!(!app.shell.runtime.modifier_click_pins_enabled);
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
        next_launch.restore_queries_enabled,
        expected.restore_queries_enabled
    );
    assert_eq!(next_launch.theme, expected.theme);
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
    assert_eq!(
        next_launch.modifier_click_pins_enabled,
        expected.modifier_click_pins_enabled
    );
    assert_eq!(next_launch.walker_max_entries, expected.walker_max_entries);
    assert_eq!(
        next_launch.filelist_auto_check_enabled,
        expected.filelist_auto_check_enabled
    );
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
    if isolated_settings_process(
        "tc_216_gui_settings_rejects_every_invalid_limit_boundary_without_writing",
    ) {
        return;
    }
    let _config_guard = crate::runtime_config::runtime_config_test_guard();
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
    if isolated_settings_process("tc_216_gui_history_checkbox_inverts_persist_disabled") {
        return;
    }
    let _config_guard = crate::runtime_config::runtime_config_test_guard();
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

#[test]
fn gui_filelist_auto_check_checkbox_persists_opt_out_for_next_launch() {
    if isolated_settings_process(
        "gui_filelist_auto_check_checkbox_persists_opt_out_for_next_launch",
    ) {
        return;
    }
    let _config_guard = crate::runtime_config::runtime_config_test_guard();
    let scope = test_settings_scope("gui-settings-filelist-auto-check");
    fs::write(
        scope.runtime_config_path(),
        r#"{"filelist_auto_check_enabled":true}"#,
    )
    .expect("seed config");
    let mut app = scope.app(
        test_root("gui-settings-filelist-auto-check-root"),
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
        .find_map(|shape| find_text(&shape.shape, "Check root FileList for changes"))
        .expect("FileList automatic check checkbox rendered");
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
    assert!(!draft.filelist_auto_check_enabled);
    app.request_settings_save();
    settle_settings_dialog(&mut app);
    let saved: serde_json::Value = serde_json::from_slice(
        &fs::read(scope.runtime_config_path()).expect("saved FileList check setting"),
    )
    .expect("valid JSON");
    assert_eq!(saved["filelist_auto_check_enabled"], false);
    app.open_settings_dialog();
    settle_settings_dialog(&mut app);
    let SettingsView::Editing { draft, .. } = &app.settings_dialog.view else {
        panic!("saved FileList opt-out draft is reloaded")
    };
    assert!(!draft.filelist_auto_check_enabled);
}

#[test]
fn regression_narrow_settings_title_and_footer_stay_inside_client_area() {
    if isolated_settings_process(
        "regression_narrow_settings_title_and_footer_stay_inside_client_area",
    ) {
        return;
    }
    let _config_guard = crate::runtime_config::runtime_config_test_guard();
    fn text_rect(shape: &egui::Shape, label: &str) -> Option<egui::Rect> {
        match shape {
            egui::Shape::Text(text) if text.galley.text() == label => {
                Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| text_rect(shape, label)),
            _ => None,
        }
    }
    for size in [
        egui::vec2(760.0, 560.0),
        egui::vec2(640.0, 400.0),
        egui::vec2(1200.0, 800.0),
    ] {
        let scope = test_settings_scope("settings-client-bounds");
        fs::write(scope.runtime_config_path(), "{}").expect("seed config");
        let mut app = scope.app(test_root("settings-client-bounds-root"), 30, String::new());
        app.open_settings_dialog();
        settle_settings_dialog(&mut app);
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        for state in 0..4 {
            if let SettingsView::Editing {
                baseline,
                draft,
                error,
                confirm_reload,
                ..
            } = &mut app.settings_dialog.view
            {
                if state == 1 {
                    draft.restore_tabs_enabled = !baseline.values.restore_tabs_enabled;
                    *confirm_reload = true;
                } else if state == 2 {
                    *error = Some("Settings were changed by another process. ".repeat(30));
                }
            }
            if state == 3 {
                app.settings_dialog.view =
                    SettingsView::Failed("Invalid settings JSON. ".repeat(100));
            }
            let mut output = egui::FullOutput::default();
            for _ in 0..4 {
                output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        ..Default::default()
                    },
                    |ui| app.run_ui_frame(ui),
                );
            }
            let labels: &[&str] = if state == 3 {
                &["Settings", "Retry", "Open settings JSON", "Close"]
            } else if state == 0 {
                &[
                    "Settings",
                    "Save",
                    "Cancel",
                    "Reset displayed settings to defaults",
                    "Reload JSON",
                    "Open settings JSON",
                ]
            } else {
                &[
                    "Settings",
                    "Save",
                    "Cancel",
                    "Reset displayed settings to defaults",
                    "Discard changes and reload JSON",
                    "Open settings JSON",
                ]
            };
            if state < 3 {
                let rect = |label| {
                    output
                        .shapes
                        .iter()
                        .find_map(|shape| text_rect(&shape.shape, label))
                        .unwrap()
                };
                let save = rect("Save");
                let cancel = rect("Cancel");
                let reset = rect("Reset displayed settings to defaults");
                assert!(save.right() < cancel.left(), "Save precedes Cancel");
                assert!((save.center().y - cancel.center().y).abs() < 1.0);
                assert!(
                    reset.right() + 24.0 < save.left(),
                    "separate draft actions from commit actions"
                );
                assert!(
                    rect("Open settings JSON").bottom() < save.top(),
                    "JSON actions precede footer"
                );
                assert!(
                    cancel.right() > rect("Settings").left() + 400.0,
                    "commit actions are right aligned"
                );
            }
            for label in labels {
                let rect = output
                    .shapes
                    .iter()
                    .find_map(|shape| text_rect(&shape.shape, label))
                    .unwrap_or_else(|| panic!("missing {label}: size={size:?}, state={state}"));
                assert!(screen.contains_rect(rect), "settings control must fit: size={size:?}, state={state}, label={label}, rect={rect:?}");
            }
        }
    }
}

#[test]
fn tc_216_settings_wait_diagnostic_identifies_owned_operations_and_completion() {
    if isolated_settings_process(
        "tc_216_settings_wait_diagnostic_identifies_owned_operations_and_completion",
    ) {
        return;
    }
    let _config_guard = crate::runtime_config::runtime_config_test_guard();
    let scope = test_settings_scope("settings-wait-diagnostic");
    fs::write(scope.runtime_config_path(), "{}").expect("seed config");
    let mut app = scope.app(
        test_root("settings-wait-diagnostic-root"),
        30,
        String::new(),
    );
    let deadline = Instant::now();
    app.open_settings_dialog();
    let generation = app.settings_dialog.generation;
    assert_eq!(
        app.shell
            .worker_bus
            .config_settings
            .pending_operation_for_test(),
        Some((generation, false))
    );
    let diagnostic = settings_wait_diagnostics(&app, deadline);
    assert!(diagnostic.contains("ui_phase=loading"), "{diagnostic}");
    assert!(
        diagnostic.contains("pending_operation=load"),
        "{diagnostic}"
    );
    assert!(
        diagnostic.contains(&format!("expected_generation={generation}")),
        "{diagnostic}"
    );
    assert!(
        diagnostic.contains(&format!("pending_generation={generation}")),
        "{diagnostic}"
    );
    settle_settings_dialog(&mut app);
    assert_eq!(
        app.shell
            .worker_bus
            .config_settings
            .pending_operation_for_test(),
        None
    );
    let diagnostic = settings_wait_diagnostics(&app, deadline);
    assert!(diagnostic.contains("ui_phase=editing"), "{diagnostic}");
    assert!(
        diagnostic.contains("pending_generation=none"),
        "{diagnostic}"
    );
    assert!(
        diagnostic.contains("pending_operation=none"),
        "{diagnostic}"
    );
    let SettingsView::Editing { limit_text, .. } = &mut app.settings_dialog.view else {
        panic!("owned real load must reach Editing");
    };
    *limit_text = "1234".into();
    app.request_settings_save();
    let generation = app.settings_dialog.generation;
    assert_eq!(
        app.shell
            .worker_bus
            .config_settings
            .pending_operation_for_test(),
        Some((generation, true))
    );
    let diagnostic = settings_wait_diagnostics(&app, deadline);
    assert!(diagnostic.contains("ui_phase=saving"), "{diagnostic}");
    assert!(
        diagnostic.contains("pending_operation=save"),
        "{diagnostic}"
    );
    assert!(
        diagnostic.contains(&format!("pending_generation={generation}")),
        "{diagnostic}"
    );
    settle_settings_dialog(&mut app);
    assert!(matches!(app.settings_dialog.view, SettingsView::Closed));
    assert!(settings_wait_diagnostics(&app, deadline).contains("ui_phase=closed"));
    let saved: serde_json::Value =
        serde_json::from_slice(&fs::read(scope.runtime_config_path()).unwrap()).unwrap();
    assert_eq!(saved["walker_max_entries"], 1234);
}

#[test]
fn settings_query_restore_subsetting_and_theme_controls_follow_pointer_input() {
    if isolated_settings_process(
        "settings_query_restore_subsetting_and_theme_controls_follow_pointer_input",
    ) {
        return;
    }
    let _config_guard = crate::runtime_config::runtime_config_test_guard();
    use crate::runtime_config::ThemeMode;
    let scope = test_settings_scope("settings-query-theme-controls");
    let path = scope.runtime_config_path();
    let original = r#"{"restore_tabs_enabled":false}"#;
    fs::write(&path, original).unwrap();
    let mut app = scope.app(test_root("settings-query-theme-root"), 30, String::new());
    app.open_settings_dialog();
    settle_settings_dialog(&mut app);
    let ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 1200.0));
    fn text_target(shape: &egui::Shape, label: &str) -> Option<egui::Pos2> {
        match shape {
            egui::Shape::Text(text) if text.galley.text() == label => {
                Some(text.pos + egui::vec2(8.0, text.galley.size().y / 2.0))
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| text_target(shape, label)),
            _ => None,
        }
    }
    let click = |app: &mut FlistWalkerApp, label: &str| {
        for _ in 0..2 {
            let _ = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ui| app.run_ui_frame(ui),
            );
        }
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ui| app.run_ui_frame(ui),
        );
        let target = output
            .shapes
            .iter()
            .find_map(|shape| text_target(&shape.shape, label))
            .unwrap_or_else(|| panic!("missing settings control: {label}"));
        for pressed in [true, false] {
            let _ = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events: vec![
                        egui::Event::PointerMoved(target),
                        egui::Event::PointerButton {
                            pos: target,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |ui| app.run_ui_frame(ui),
            );
        }
    };
    click(&mut app, "Restore search queries");
    let SettingsView::Editing { draft, .. } = &app.settings_dialog.view else {
        panic!("editing")
    };
    assert!(
        draft.restore_queries_enabled,
        "disabled child ignores pointer input"
    );
    click(&mut app, "Restore previous tabs");
    click(&mut app, "Restore search queries");
    click(&mut app, "Restore previous tabs");
    let SettingsView::Editing { draft, .. } = &app.settings_dialog.view else {
        panic!("editing")
    };
    assert!(!draft.restore_tabs_enabled);
    assert!(
        !draft.restore_queries_enabled,
        "child preference retained when parent is disabled"
    );
    for (label, expected) in [
        ("Dark", ThemeMode::Dark),
        ("Light", ThemeMode::Light),
        ("System", ThemeMode::System),
    ] {
        click(&mut app, label);
        let SettingsView::Editing { draft, .. } = &app.settings_dialog.view else {
            panic!("editing")
        };
        assert_eq!(draft.theme, expected);
    }
    click(&mut app, "Reset displayed settings to defaults");
    let SettingsView::Editing { draft, .. } = &app.settings_dialog.view else {
        panic!("editing")
    };
    assert!(draft.restore_queries_enabled);
    assert_eq!(draft.theme, ThemeMode::System);
    click(&mut app, "Dark");
    click(&mut app, "Cancel");
    assert!(matches!(app.settings_dialog.view, SettingsView::Closed));
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
}

#[test]
fn settings_save_applies_all_fields_and_repeated_changes_after_success() {
    if isolated_settings_process(
        "settings_save_applies_all_fields_and_repeated_changes_after_success",
    ) {
        return;
    }
    let _guard = crate::runtime_config::runtime_config_test_guard();
    let scope = test_settings_scope("live-settings");
    fs::write(scope.runtime_config_path(), "{}").unwrap();
    let mut app = scope.app(test_root("live-root"), 30, "keep query".into());
    let old = crate::runtime_config::current_runtime_config();
    let env_before = std::env::var_os("FLISTWALKER_DISABLE_HISTORY_PERSIST");
    for (index, theme) in [
        crate::runtime_config::ThemeMode::Dark,
        crate::runtime_config::ThemeMode::Light,
        crate::runtime_config::ThemeMode::System,
    ]
    .into_iter()
    .enumerate()
    {
        app.open_settings_dialog();
        settle_settings_dialog(&mut app);
        let mut expected = crate::runtime_config::EditableSettings::default();
        let enabled = index % 2 == 0;
        expected.restore_tabs_enabled = enabled;
        expected.restore_queries_enabled = !enabled;
        expected.history_persist_disabled = enabled;
        expected.theme = theme;
        expected.emacs_keybindings_enabled = !enabled;
        expected.ctrl_w_deletes_word_in_query = enabled;
        expected.tab_pin_moves_to_next_row = enabled;
        expected.modifier_click_pins_enabled = !enabled;
        expected.filelist_auto_check_enabled = !enabled;
        expected.walker_max_entries = 17 + index;
        if let SettingsView::Editing {
            draft, limit_text, ..
        } = &mut app.settings_dialog.view
        {
            *draft = expected.clone();
            *limit_text = expected.walker_max_entries.to_string();
        } else {
            panic!("editing");
        }
        let before = crate::runtime_config::current_runtime_config();
        app.request_settings_save();
        assert_eq!(
            crate::runtime_config::current_runtime_config(),
            before,
            "acceptance is not save success"
        );
        app.close_settings_dialog();
        assert!(matches!(
            app.settings_dialog.view,
            SettingsView::Saving { .. }
        ));
        app.request_settings_save(); // duplicate must not replace the owned request
        settle_settings_dialog(&mut app);
        assert_eq!(
            crate::runtime_config::EditableSettings::from(
                &crate::runtime_config::current_runtime_config()
            ),
            expected
        );
        assert_eq!(app.shell.runtime.emacs_keybindings_enabled, !enabled);
        assert_eq!(app.shell.runtime.ctrl_w_deletes_word_in_query, enabled);
        assert_eq!(app.shell.runtime.tab_pin_moves_to_next_row, enabled);
        assert_eq!(app.shell.runtime.modifier_click_pins_enabled, !enabled);
        assert_eq!(app.filelist_auto_check_enabled, !enabled);
        assert_eq!(FlistWalkerApp::restore_tabs_enabled(), enabled);
        assert_eq!(FlistWalkerApp::history_persist_disabled(), enabled);
        assert_eq!(
            crate::walker_runtime::walker_runtime_settings(
                &crate::runtime_config::current_runtime_config()
            )
            .max_entries,
            expected.walker_max_entries
        );
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| app.run_ui_frame(ui));
        assert_eq!(
            ctx.options(|o| o.theme_preference),
            match theme {
                crate::runtime_config::ThemeMode::Dark => egui::ThemePreference::Dark,
                crate::runtime_config::ThemeMode::Light => egui::ThemePreference::Light,
                crate::runtime_config::ThemeMode::System => egui::ThemePreference::System,
            }
        );
        assert_eq!(
            std::env::var_os("FLISTWALKER_DISABLE_HISTORY_PERSIST"),
            env_before
        );
        assert_eq!(
            crate::runtime_config::current_runtime_config().developer,
            old.developer
        );
        assert_eq!(
            crate::runtime_config::current_runtime_config().search_threads,
            old.search_threads
        );
    }
}

#[test]
fn settings_painted_icons_align_with_label_ink_across_font_metrics() {
    if isolated_settings_process("settings_painted_icons_align_with_label_ink_across_font_metrics")
    {
        return;
    }
    let _guard = crate::runtime_config::runtime_config_test_guard();
    fn collect<'a>(
        s: &'a egui::Shape,
        texts: &mut Vec<&'a egui::epaint::TextShape>,
        icons: &mut Vec<egui::Rect>,
    ) {
        match s {
            egui::Shape::Vec(v) => {
                for s in v {
                    collect(s, texts, icons);
                }
            }
            egui::Shape::Text(t) => texts.push(t),
            egui::Shape::Rect(r) if r.rect.width() < 22.0 && r.rect.height() < 22.0 => {
                icons.push(r.rect)
            }
            egui::Shape::Circle(c) if c.radius > 4.0 && c.radius < 11.0 => icons.push(
                egui::Rect::from_center_size(c.center, egui::Vec2::splat(c.radius * 2.0)),
            ),
            _ => {}
        }
    }
    for scale in [1.0, 1.25, 2.0] {
        for offset in [-3.0, 0.0, 3.0] {
            let scope = test_settings_scope("settings-ink");
            fs::write(scope.runtime_config_path(), "{}").unwrap();
            let mut app = scope.app(test_root("settings-ink-root"), 30, String::new());
            app.open_settings_dialog();
            settle_settings_dialog(&mut app);
            let ctx = egui::Context::default();
            ctx.set_pixels_per_point(scale);
            let mut fonts = egui::FontDefinitions::default();
            for f in fonts.font_data.values_mut() {
                Arc::make_mut(f).tweak.y_offset = offset;
            }
            ctx.set_fonts(fonts);
            ctx.all_styles_mut(|style| {
                style.text_styles.insert(
                    egui::TextStyle::Button,
                    egui::FontId::proportional(16.0 + offset),
                );
            });
            let mut output = egui::FullOutput::default();
            for _ in 0..3 {
                output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1200.0, 1100.0),
                        )),
                        ..Default::default()
                    },
                    |ui| app.run_ui_frame(ui),
                );
            }
            let mut texts = vec![];
            let mut icons = vec![];
            for s in &output.shapes {
                collect(&s.shape, &mut texts, &mut icons);
            }
            for label in [
                "Restore previous tabs",
                "Restore search queries",
                "Save search history",
                "System",
                "Dark",
                "Light",
                "Use Emacs-style shortcuts",
            ] {
                let t = texts
                    .iter()
                    .find(|t| t.galley.text() == label)
                    .unwrap_or_else(|| panic!("missing {label}"));
                let ink = t.visual_bounding_rect();
                let icon = icons
                    .iter()
                    .filter(|r| {
                        r.right() <= ink.left()
                            && ink.left() - r.right() < 20.0
                            && (r.center().y - ink.center().y).abs() < 15.0
                    })
                    .min_by(|a, b| {
                        (a.center().y - ink.center().y)
                            .abs()
                            .total_cmp(&(b.center().y - ink.center().y).abs())
                    })
                    .unwrap_or_else(|| panic!("missing icon {label}"));
                assert!(
                    (icon.center().y - ink.center().y).abs() <= 1.0 / scale,
                    "{label} scale={scale} offset={offset}: icon={icon:?} ink={ink:?}"
                );
            }
        }
    }
}

#[test]
fn settings_cancel_reload_and_failed_save_preserve_live_config() {
    if isolated_settings_process("settings_cancel_reload_and_failed_save_preserve_live_config") {
        return;
    }
    let _guard = crate::runtime_config::runtime_config_test_guard();
    let scope = test_settings_scope("live-failure");
    let path = scope.runtime_config_path();
    fs::write(&path, "{}").unwrap();
    let mut app = scope.app(test_root("live-failure-root"), 30, String::new());
    let before = crate::runtime_config::current_runtime_config();
    let env_before = std::env::var_os("FLISTWALKER_DISABLE_HISTORY_PERSIST");
    app.open_settings_dialog();
    settle_settings_dialog(&mut app);
    if let SettingsView::Editing { draft, .. } = &mut app.settings_dialog.view {
        draft.emacs_keybindings_enabled = !before.emacs_keybindings_enabled;
    }
    fs::write(
        &path,
        r#"{"theme":"light","history_persist_disabled":true}"#,
    )
    .unwrap();
    app.request_settings_save();
    settle_settings_dialog(&mut app);
    assert!(matches!(
        app.settings_dialog.view,
        SettingsView::Editing { error: Some(_), .. }
    ));
    assert_eq!(crate::runtime_config::current_runtime_config(), before);
    app.request_settings_reload();
    app.request_settings_reload();
    settle_settings_dialog(&mut app);
    assert_eq!(
        crate::runtime_config::current_runtime_config(),
        before,
        "Reload only replaces the draft"
    );
    app.close_settings_dialog();
    assert_eq!(crate::runtime_config::current_runtime_config(), before);
    assert_eq!(
        std::env::var_os("FLISTWALKER_DISABLE_HISTORY_PERSIST"),
        env_before
    );
}

#[test]
fn settings_choices_wrap_and_preserve_pointer_keyboard_disabled_semantics() {
    if isolated_settings_process(
        "settings_choices_wrap_and_preserve_pointer_keyboard_disabled_semantics",
    ) {
        return;
    }
    use crate::app::render_panels::{aligned_checkbox, aligned_radio_value};
    let ctx = egui::Context::default();
    let mut checked = false;
    let mut selected = 0;
    let mut response = None;
    let mut radio = None;
    let mut disabled = None;
    let label = "Long choice label with descenders gjpq and accents É to wrap safely";
    let mut frame = |events, enabled| {
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(220.0, 320.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                ui.set_max_width(180.0);
                response = Some(aligned_checkbox(ui, &mut checked, label));
                radio = Some(aligned_radio_value(ui, &mut selected, 1, "Selected choice"));
                disabled = Some(
                    ui.add_enabled_ui(enabled, |ui| {
                        aligned_checkbox(ui, &mut false, "Disabled choice")
                    })
                    .inner,
                );
            },
        );
        (
            output,
            response.clone().unwrap(),
            radio.clone().unwrap(),
            disabled.clone().unwrap(),
            checked,
            selected,
        )
    };
    let (output, r, _, _, _, _) = frame(vec![], false);
    assert!(r.rect.height() > 25.0, "long labels wrap");
    fn find<'a>(s: &'a egui::Shape, label: &str) -> Option<&'a egui::epaint::TextShape> {
        match s {
            egui::Shape::Text(t) if t.galley.text() == label => Some(t),
            egui::Shape::Vec(v) => v.iter().find_map(|s| find(s, label)),
            _ => None,
        }
    }
    let t = output
        .shapes
        .iter()
        .find_map(|s| find(&s.shape, label))
        .unwrap();
    assert!(t.galley.rows.len() > 1);
    assert!(
        r.rect.contains_rect(t.visual_bounding_rect()),
        "ink fits hit target: hit={:?}, ink={:?}",
        r.rect,
        t.visual_bounding_rect()
    );
    let click = |pos, pressed| {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    };
    // Both the icon and the last wrapped line belong to the same clickable target.
    for pos in [
        r.rect.left_center() + egui::vec2(7.0, 0.0),
        t.visual_bounding_rect().center_bottom() - egui::vec2(0.0, 2.0),
    ] {
        frame(click(pos, true), false);
        let (_, response, _, _, _, _) = frame(click(pos, false), false);
        assert!(response.changed());
    }
    let (_, r, radio, disabled, checked, _) = frame(vec![], false);
    assert!(!checked);
    ctx.memory_mut(|m| m.request_focus(r.id));
    let (_, r, _, _, checked, _) = frame(
        vec![egui::Event::Key {
            key: egui::Key::Space,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        false,
    );
    assert!(checked && r.changed());
    for _ in 0..2 {
        frame(click(radio.rect.center(), true), false);
        let (_, _, _, _, _, value) = frame(click(radio.rect.center(), false), false);
        assert_eq!(value, 1, "radio cannot toggle itself off");
    }
    frame(click(disabled.rect.center(), true), false);
    let (_, _, _, r, _, _) = frame(click(disabled.rect.center(), false), false);
    assert!(!r.changed());
}

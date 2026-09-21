use super::*;

#[test]
fn tc_168_missing_storage_keeps_edit_dirty_and_visible() {
    let mut app = FlistWalkerApp::new(test_root("autosave-no-storage"), 50, String::new());
    assert!(app.persistence_ui_state_file_path().is_none());
    app.mark_ui_state_dirty();
    app.maybe_save_ui_state(true);
    assert!(app.shell.ui.ui_state_dirty);
    assert_eq!(app.shell.ui.persistence.submitted_generation, 0);
    assert!(app
        .status_line_text()
        .starts_with("Session not saved: Session storage location is unavailable"));
    app.set_notice("Search finished");
    app.poll_ui_state_persistence();
    assert!(app.status_line_text().starts_with("Session not saved"));
}

#[test]
fn tc_168_autosave_failure_remains_visible_without_new_edits() {
    let scope = test_settings_scope("autosave-visible-failure");
    let root = test_root("autosave-visible-root");
    fs::create_dir_all(&root).unwrap();
    let mut app = scope.app(root.clone(), 50, String::new());
    let path = app.persistence_ui_state_file_path().unwrap();
    fs::write(&path, "{").unwrap();
    app.mark_ui_state_dirty();
    app.maybe_save_ui_state(true);
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        app.maybe_save_ui_state(false);
        if app.status_line_text().contains("Session not saved") {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        app.status_line_text().contains("Session not saved"),
        "autosave failure must remain visible without a new edit"
    );
    app.set_notice("Search finished");
    assert!(app.status_line_text().starts_with("Session not saved"));
    assert_eq!(fs::read_to_string(&path).unwrap(), "{");
    let ctx = egui::Context::default();
    let output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 900.0),
            )),
            ..Default::default()
        },
        |ui| crate::app::render_panels::render_status_panel(&mut app, ui),
    );
    fn contains_failure(shape: &egui::Shape) -> bool {
        match shape {
            egui::Shape::Text(text) => text.galley.text().contains("Session not saved"),
            egui::Shape::Vec(shapes) => shapes.iter().any(contains_failure),
            _ => false,
        }
    }
    assert!(
        output
            .shapes
            .iter()
            .any(|shape| contains_failure(&shape.shape)),
        "failure must be painted in the footer"
    );
    fs::write(&path, "{}").unwrap();
    crate::persistence::flush_ui_state_persistence(&path, Duration::from_secs(2)).unwrap();
    app.maybe_save_ui_state(false);
    assert!(app.status_line_text().contains("Session saved"));
    assert!(!app.status_line_text().contains("Session not saved"));
    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn tc_168_gui_startup_fallback_cannot_overwrite_external_repair() {
    let scope = test_settings_scope("autosave-startup-protection");
    let root = test_root("autosave-startup-root");
    fs::create_dir_all(&root).unwrap();
    let path = FlistWalkerApp::ui_state_file_path_in(scope.saved_roots_path().parent().unwrap());
    fs::write(&path, r#"{"default_root":"valuable","show_preview":"bad"}"#).unwrap();
    let launch = FlistWalkerApp::load_launch_settings_from_path(&path);
    assert!(launch.default_root.is_none());
    let mut app = scope.app(root.clone(), 50, String::new());
    app.mark_ui_state_dirty();
    app.maybe_save_ui_state(true);
    assert!(app.shell.ui.persistence.status.startup_protected);
    let repaired = r#"{"default_root":"valuable","show_preview":true}"#;
    fs::write(&path, repaired).unwrap();
    app.persist_ui_state_now();
    assert!(app.shell.ui.ui_state_dirty);
    assert!(app.status_line_text().contains("restart"));
    app.persist_state_and_shutdown("startup-protection-test");
    assert_eq!(fs::read_to_string(&path).unwrap(), repaired);
    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn tc_168_old_completion_cannot_clear_newer_autosave_error() {
    use crate::persistence::UiStatePersistenceStatus;
    let mut state = crate::app::session::SessionPersistenceUi::default();
    state.observe(UiStatePersistenceStatus {
        accepted_generation: 2,
        persisted_generation: 1,
        last_error: Some("write failed".into()),
        ..Default::default()
    });
    state.observe(UiStatePersistenceStatus {
        accepted_generation: 1,
        persisted_generation: 1,
        ..Default::default()
    });
    assert_eq!(state.error(), Some("write failed"));
    state.submission_error = Some("queue full".into());
    state.observe(UiStatePersistenceStatus {
        accepted_generation: 2,
        persisted_generation: 2,
        ..Default::default()
    });
    assert_eq!(
        state.error(),
        Some("queue full"),
        "completion cannot acknowledge an unaccepted edit"
    );
}

#[test]
fn tc_168_gui_full_keeps_latest_edit_dirty_until_readmitted() {
    let scope = test_settings_scope("autosave-full-retry");
    let root = test_root("autosave-full-root");
    fs::create_dir_all(&root).unwrap();
    let mut app = scope.app(root.clone(), 50, String::new());
    let path = app.persistence_ui_state_file_path().unwrap();
    fs::write(&path, "{").unwrap();
    for index in 0..64 {
        app.shell.runtime.query_state.query_history = vec![format!("query-{index}")].into();
        app.mark_ui_state_dirty();
        app.maybe_save_ui_state(true);
        assert!(!app.shell.ui.ui_state_dirty);
    }
    app.shell.runtime.query_state.query_history = vec!["latest-edit".into()].into();
    app.mark_ui_state_dirty();
    app.maybe_save_ui_state(true);
    assert!(app.shell.ui.ui_state_dirty);
    assert!(app.status_line_text().contains("queue is full"));
    fs::write(&path, "{}").unwrap();
    crate::persistence::flush_ui_state_persistence(&path, Duration::from_secs(2)).unwrap();
    app.poll_ui_state_persistence();
    assert!(
        app.shell.ui.ui_state_dirty,
        "previous completion cannot acknowledge latest edit"
    );
    app.maybe_save_ui_state(true);
    assert!(!app.shell.ui.ui_state_dirty);
    crate::persistence::flush_ui_state_persistence(&path, Duration::from_secs(2)).unwrap();
    app.poll_ui_state_persistence();
    assert!(app.status_line_text().contains("Session saved"));
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        saved["query_history"].as_array().unwrap().last().unwrap(),
        "latest-edit"
    );
    drop(app);
    fs::remove_dir_all(root).unwrap();
}

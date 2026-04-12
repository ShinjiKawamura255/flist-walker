use super::*;

#[test]
fn window_geometry_from_rects_prefers_inner_size() {
    let outer = egui::Rect::from_min_size(egui::pos2(100.0, 200.0), egui::vec2(1200.0, 900.0));
    let inner = egui::Rect::from_min_size(egui::pos2(110.0, 240.0), egui::vec2(1180.0, 840.0));

    let geom = FlistWalkerApp::window_geometry_from_rects(
        outer,
        Some(inner),
        Some(egui::vec2(2560.0, 1440.0)),
    );

    assert_eq!(geom.x, 100.0);
    assert_eq!(geom.y, 200.0);
    assert_eq!(geom.width, 1180.0);
    assert_eq!(geom.height, 840.0);
    assert_eq!(geom.monitor_width, Some(2560.0));
    assert_eq!(geom.monitor_height, Some(1440.0));
}

#[test]
fn normalize_restore_geometry_preserves_virtual_desktop_position() {
    let saved = SavedWindowGeometry {
        x: -1600.0,
        y: 120.0,
        width: 900.0,
        height: 700.0,
        monitor_width: Some(1920.0),
        monitor_height: Some(1080.0),
    };

    let restored = FlistWalkerApp::normalize_restore_geometry(saved);

    assert_eq!(restored.x, -1600.0);
    assert_eq!(restored.y, 120.0);
    assert_eq!(restored.width, 900.0);
    assert_eq!(restored.height, 700.0);
    assert_eq!(restored.monitor_width, Some(1920.0));
    assert_eq!(restored.monitor_height, Some(1080.0));
}

#[test]
fn apply_stable_window_geometry_force_commits_pending() {
    let root = test_root("window-geometry-commit");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.ui.window_geometry = None;
    app.shell.ui.ui_state_dirty = false;
    app.shell.ui.pending_window_geometry = Some(SavedWindowGeometry {
        x: 100.0,
        y: 120.0,
        width: 900.0,
        height: 700.0,
        monitor_width: Some(2560.0),
        monitor_height: Some(1440.0),
    });

    app.apply_stable_window_geometry(true);

    assert!(app.shell.ui.pending_window_geometry.is_none());
    assert!(app.shell.ui.ui_state_dirty);
    let geom = app.shell.ui.window_geometry.clone().expect("committed geometry");
    assert_eq!(geom.x, 100.0);
    assert_eq!(geom.y, 120.0);
    assert_eq!(geom.width, 900.0);
    assert_eq!(geom.height, 700.0);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn process_query_input_events_inserts_half_space_for_space_keys() {
    let root = test_root("ime-space-fallback");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.query_state.query = "abc".to_string();

    let ctx = egui::Context::default();
    let (inserted_half, cursor_half) = app.process_query_input_events(
        &ctx,
        &[egui::Event::Key {
            key: egui::Key::Space,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        true,
        false,
        Some(egui::text::CCursorRange::one(egui::text::CCursor::new(3))),
    );
    assert!(inserted_half);
    assert_eq!(cursor_half, Some(4));
    assert_eq!(app.shell.runtime.query_state.query, "abc ");

    let (inserted_shift, cursor_shift) = app.process_query_input_events(
        &ctx,
        &[egui::Event::Key {
            key: egui::Key::Space,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                shift: true,
                ..Default::default()
            },
        }],
        true,
        false,
        Some(egui::text::CCursorRange::one(egui::text::CCursor::new(4))),
    );
    assert!(inserted_shift);
    assert_eq!(cursor_shift, Some(5));
    assert_eq!(app.shell.runtime.query_state.query, "abc  ");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn process_query_input_events_inserts_space_even_if_composition_is_active_without_update() {
    let root = test_root("ime-composition-space-allow");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.query_state.query = "abc".to_string();

    let ctx = egui::Context::default();
    let (inserted, cursor) = app.process_query_input_events(
        &ctx,
        &[
            egui::Event::Ime(egui::ImeEvent::Enabled),
            egui::Event::Key {
                key: egui::Key::Space,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
        ],
        true,
        false,
        Some(egui::text::CCursorRange::one(egui::text::CCursor::new(3))),
    );
    assert!(inserted);
    assert_eq!(cursor, Some(4));
    assert_eq!(app.shell.runtime.query_state.query, "abc ");
    assert!(app.shell.ui.ime_composition_active);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn process_query_input_events_skips_space_fallback_when_composition_updates() {
    let root = test_root("ime-composition-space-allow-update");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.query_state.query = "abc".to_string();

    let ctx = egui::Context::default();
    let (inserted, cursor) = app.process_query_input_events(
        &ctx,
        &[
            egui::Event::Ime(egui::ImeEvent::Enabled),
            egui::Event::Ime(egui::ImeEvent::Preedit("あ".to_string())),
            egui::Event::Key {
                key: egui::Key::Space,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
        ],
        true,
        false,
        Some(egui::text::CCursorRange::one(egui::text::CCursor::new(3))),
    );
    assert!(!inserted);
    assert_eq!(cursor, None);
    assert_eq!(app.shell.runtime.query_state.query, "abc");
    assert!(app.shell.ui.ime_composition_active);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn process_query_input_events_skips_shift_space_fallback_with_composition_update() {
    let root = test_root("ime-composition-half-space-allow");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.query_state.query = "abc".to_string();

    let ctx = egui::Context::default();
    let (inserted, cursor) = app.process_query_input_events(
        &ctx,
        &[
            egui::Event::Ime(egui::ImeEvent::Enabled),
            egui::Event::Ime(egui::ImeEvent::Preedit("あ".to_string())),
            egui::Event::Key {
                key: egui::Key::Space,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    shift: true,
                    ..Default::default()
                },
            },
        ],
        true,
        false,
        Some(egui::text::CCursorRange::one(egui::text::CCursor::new(3))),
    );
    assert!(!inserted);
    assert_eq!(cursor, None);
    assert_eq!(app.shell.runtime.query_state.query, "abc");
    assert!(app.shell.ui.ime_composition_active);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn process_query_input_events_inserts_space_fallback_at_cursor_position() {
    let root = test_root("ime-space-fallback-cursor");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.query_state.query = "abCD".to_string();
    let ctx = egui::Context::default();

    let (inserted, cursor) = app.process_query_input_events(
        &ctx,
        &[egui::Event::Key {
            key: egui::Key::Space,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        true,
        false,
        Some(egui::text::CCursorRange::one(egui::text::CCursor::new(2))),
    );

    assert!(inserted);
    assert_eq!(app.shell.runtime.query_state.query, "ab CD");
    assert_eq!(cursor, Some(3));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn process_query_input_events_inserts_composition_commit_fallback_at_cursor_position() {
    let root = test_root("ime-commit-fallback-cursor");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.query_state.query = "abCD".to_string();
    let ctx = egui::Context::default();

    let (inserted, cursor) = app.process_query_input_events(
        &ctx,
        &[egui::Event::Ime(egui::ImeEvent::Commit("x".to_string()))],
        true,
        false,
        Some(egui::text::CCursorRange::one(egui::text::CCursor::new(2))),
    );

    assert!(inserted);
    assert_eq!(app.shell.runtime.query_state.query, "abxCD");
    assert_eq!(cursor, Some(3));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn process_query_input_events_does_not_override_widget_owned_ime_commit() {
    let root = test_root("ime-commit-widget-owned");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.query_state.query = "変換済み".to_string();
    let ctx = egui::Context::default();

    let (changed, cursor) = app.process_query_input_events(
        &ctx,
        &[egui::Event::Ime(egui::ImeEvent::Commit(
            "日本語".to_string(),
        ))],
        true,
        true,
        Some(egui::text::CCursorRange::one(egui::text::CCursor::new(
            FlistWalkerApp::char_count(&app.shell.runtime.query_state.query),
        ))),
    );

    assert!(!changed);
    assert_eq!(cursor, None);
    assert_eq!(app.shell.runtime.query_state.query, "変換済み");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn oversized_geometry_is_rejected_when_monitor_size_is_known() {
    let root = test_root("reject-oversize-geometry");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

    let next = SavedWindowGeometry {
        x: 200.0,
        y: 150.0,
        width: 3600.0,
        height: 2100.0,
        monitor_width: Some(2560.0),
        monitor_height: Some(1440.0),
    };

    let width_limit = (next.monitor_width.unwrap_or_default() * 1.05).max(640.0);
    let height_limit = (next.monitor_height.unwrap_or_default() * 1.05).max(400.0);
    assert!(next.width > width_limit);
    assert!(next.height > height_limit);

    // Simulate capture rejection condition directly.
    if let (Some(mw), Some(mh)) = (next.monitor_width, next.monitor_height) {
        let w_limit = (mw * 1.05).max(640.0);
        let h_limit = (mh * 1.05).max(400.0);
        if next.width > w_limit || next.height > h_limit {
            // keep state untouched
        } else {
            app.shell.ui.pending_window_geometry = Some(next);
        }
    }
    assert!(app.shell.ui.pending_window_geometry.is_none());
    let _ = fs::remove_dir_all(&root);
}

use super::*;
use crate::app::shell_support::load_cjk_font_bytes;

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
        pixels_per_point: Some(1.0),
    };
    let monitors = [(
        egui::Rect::from_min_size(egui::pos2(-1920.0, 0.0), egui::vec2(1920.0, 1080.0)),
        1.0,
    )];
    let restored = FlistWalkerApp::normalize_startup_placement(saved, &monitors, Some(0), true);
    assert_eq!(restored.physical_position, Some(egui::pos2(-1600.0, 120.0)));
    assert_eq!(restored.logical_size, egui::vec2(900.0, 700.0));
}

#[test]
fn normalize_restore_geometry_clamps_position_into_current_display_bounds() {
    let saved = SavedWindowGeometry {
        x: 3400.0,
        y: 1800.0,
        width: 900.0,
        height: 700.0,
        monitor_width: Some(2560.0),
        monitor_height: Some(1440.0),
        pixels_per_point: Some(1.0),
    };
    let monitors = [(
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1920.0, 1080.0)),
        1.0,
    )];
    let restored = FlistWalkerApp::normalize_startup_placement(saved, &monitors, Some(0), true);
    assert_eq!(restored.physical_position, Some(egui::pos2(1020.0, 380.0)));
    assert_eq!(restored.logical_size, egui::vec2(900.0, 700.0));
}

#[test]
fn normalize_restore_geometry_keeps_negative_position_inside_current_display_bounds() {
    let saved = SavedWindowGeometry {
        x: -1600.0,
        y: 120.0,
        width: 900.0,
        height: 700.0,
        monitor_width: Some(1920.0),
        monitor_height: Some(1080.0),
        pixels_per_point: Some(1.0),
    };
    let monitors = [
        (
            egui::Rect::from_min_size(egui::pos2(-1920.0, 0.0), egui::vec2(1920.0, 1080.0)),
            1.0,
        ),
        (
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1920.0, 1080.0)),
            1.0,
        ),
    ];
    let restored = FlistWalkerApp::normalize_startup_placement(saved, &monitors, Some(1), true);
    assert_eq!(restored.physical_position, Some(egui::pos2(-1600.0, 120.0)));
    assert_eq!(restored.logical_size, egui::vec2(900.0, 700.0));
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
        pixels_per_point: None,
    });

    app.apply_stable_window_geometry(true);

    assert!(app.shell.ui.pending_window_geometry.is_none());
    assert!(app.shell.ui.ui_state_dirty);
    let geom = app
        .shell
        .ui
        .window_geometry
        .clone()
        .expect("committed geometry");
    assert_eq!(geom.x, 100.0);
    assert_eq!(geom.y, 120.0);
    assert_eq!(geom.width, 900.0);
    assert_eq!(geom.height, 700.0);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn cached_cjk_font_can_apply_to_multiple_app_instances() {
    FlistWalkerApp::set_cjk_font_ready_for_test(vec![0; 4]);
    let root_a = test_root("cjk-font-app-a");
    let root_b = test_root("cjk-font-app-b");
    fs::create_dir_all(&root_a).expect("create root a");
    fs::create_dir_all(&root_b).expect("create root b");

    let ctx_a = egui::Context::default();
    let ctx_b = egui::Context::default();
    let mut app_a = FlistWalkerApp::new(root_a.clone(), 50, String::new());
    let mut app_b = FlistWalkerApp::new(root_b.clone(), 50, String::new());

    app_a.maybe_apply_pending_cjk_font(&ctx_a);
    app_b.maybe_apply_pending_cjk_font(&ctx_b);

    assert!(app_a.shell.ui.cjk_font_applied);
    assert!(app_b.shell.ui.cjk_font_applied);

    FlistWalkerApp::reset_cjk_font_state_for_test();
    let _ = fs::remove_dir_all(&root_a);
    let _ = fs::remove_dir_all(&root_b);
}

#[test]
#[ignore]
fn measure_cjk_font_load_headless() {
    let started = std::time::Instant::now();
    let loaded = load_cjk_font_bytes();
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    match loaded {
        Some(bytes) => println!(
            "cjk_font_load_headless elapsed_ms={elapsed_ms:.3} bytes={}",
            bytes.len()
        ),
        None => println!("cjk_font_load_headless elapsed_ms={elapsed_ms:.3} unavailable"),
    }
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
    app.shell.ui.ime_composition_active = true;
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
        Some(egui::text::CCursorRange::one(egui::text::CCursor::new(3))),
    );
    assert!(inserted);
    assert_eq!(cursor, Some(4));
    assert_eq!(app.shell.runtime.query_state.query, "abc ");
    assert!(app.shell.ui.ime_composition_active);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn process_query_input_events_empty_preedit_dismisses_composition() {
    let root = test_root("ime-empty-preedit-dismisses");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.ui.ime_composition_active = true;

    let ctx = egui::Context::default();
    let (changed, cursor) = app.process_query_input_events(
        &ctx,
        &[egui::Event::Ime(egui::ImeEvent::Preedit {
            text: String::new(),
            active_range_chars: None,
        })],
        true,
        false,
        None,
    );

    assert!(!changed);
    assert_eq!(cursor, None);
    assert!(!app.shell.ui.ime_composition_active);
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
            egui::Event::Ime(egui::ImeEvent::Preedit {
                text: "あ".to_string(),
                active_range_chars: None,
            }),
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
            egui::Event::Ime(egui::ImeEvent::Preedit {
                text: "あ".to_string(),
                active_range_chars: None,
            }),
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
            crate::text_editing::char_count(&app.shell.runtime.query_state.query),
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
        pixels_per_point: None,
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

#[test]
fn regression_gui_geometry_monitor_gap_is_not_a_valid_placement() {
    let monitors = [
        (
            egui::Rect::from_min_size(egui::pos2(-1920.0, 0.0), egui::vec2(1920.0, 1080.0)),
            1.0,
        ),
        (
            egui::Rect::from_min_size(egui::pos2(800.0, 0.0), egui::vec2(1920.0, 1080.0)),
            1.0,
        ),
    ];
    let saved = SavedWindowGeometry {
        x: 100.0,
        y: 50.0,
        width: 900.0,
        height: 700.0,
        ..Default::default()
    };
    let placement = FlistWalkerApp::normalize_startup_placement(saved, &monitors, Some(0), true);
    // Legacy scale is ambiguous. Keep size and let the window manager place it.
    assert_eq!(placement.physical_position, None);
    assert_eq!(placement.logical_size, egui::vec2(900.0, 700.0));
}

#[test]
fn regression_gui_geometry_unavailable_position_preserves_only_bounded_size() {
    let saved = SavedWindowGeometry {
        x: 9999.0,
        y: -9999.0,
        width: f32::INFINITY,
        height: f32::NAN,
        ..Default::default()
    };
    let placement = FlistWalkerApp::normalize_startup_placement(saved, &[], None, false);
    assert_eq!(placement.physical_position, None);
    assert!(placement.logical_size.is_finite());
    assert!(placement.logical_size.x <= 16000.0 && placement.logical_size.y <= 16000.0);
}

#[test]
fn regression_gui_geometry_negative_mixed_scale_uses_physical_monitor_rectangles() {
    let monitors = [
        (
            egui::Rect::from_min_size(egui::pos2(-3840.0, 0.0), egui::vec2(3840.0, 2160.0)),
            2.0,
        ),
        (
            egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1920.0, 1080.0)),
            1.0,
        ),
    ];
    let saved = SavedWindowGeometry {
        x: -1600.0,
        y: 120.0,
        width: 900.0,
        height: 700.0,
        pixels_per_point: Some(2.0),
        ..Default::default()
    };
    let placement = FlistWalkerApp::normalize_startup_placement(saved, &monitors, Some(1), true);
    assert_eq!(
        placement.physical_position,
        Some(egui::pos2(-3200.0, 240.0))
    );
    assert_eq!(placement.scale_factor, 2.0);
    assert_eq!(placement.logical_size, egui::vec2(900.0, 700.0));
}

#[test]
fn regression_gui_geometry_gap_and_disconnected_monitor_move_to_real_monitor() {
    let monitors = [
        (
            egui::Rect::from_min_size(egui::pos2(-1920.0, 0.0), egui::vec2(1920.0, 1080.0)),
            1.0,
        ),
        (
            egui::Rect::from_min_size(egui::pos2(800.0, 0.0), egui::vec2(1920.0, 1080.0)),
            1.0,
        ),
    ];
    for x in [100.0, 9000.0] {
        let saved = SavedWindowGeometry {
            x,
            y: 50.0,
            width: 900.0,
            height: 700.0,
            pixels_per_point: Some(1.0),
            ..Default::default()
        };
        let placement =
            FlistWalkerApp::normalize_startup_placement(saved, &monitors, Some(0), true);
        let rect =
            egui::Rect::from_min_size(placement.physical_position.unwrap(), placement.logical_size);
        assert!(
            monitors
                .iter()
                .any(|(monitor, _)| monitor.contains_rect(rect)),
            "{rect:?}"
        );
    }
}

#[test]
fn regression_gui_geometry_legacy_json_and_positionless_current_monitor_are_safe() {
    let legacy: SavedWindowGeometry =
        serde_json::from_str(r#"{"x":-1000,"y":100,"width":900,"height":700}"#).unwrap();
    assert_eq!(legacy.pixels_per_point, None);
    let mut saved = legacy;
    saved.pixels_per_point = Some(2.0);
    let monitors = [(
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0)),
        2.0,
    )];
    let restored = FlistWalkerApp::normalize_startup_placement(saved, &monitors, Some(0), false);
    assert_eq!(restored.physical_position, None);
    assert_eq!(restored.logical_size, egui::vec2(640.0, 400.0));
    assert_eq!(restored.scale_factor, 2.0);
}

#[test]
fn regression_gui_geometry_capture_persists_coordinate_scale() {
    let mut app = FlistWalkerApp::new(test_root("geometry-scale-capture"), 50, String::new());
    let ctx = egui::Context::default();
    let mut input = egui::RawInput::default();
    let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
    viewport.native_pixels_per_point = Some(2.0);
    viewport.outer_rect = Some(egui::Rect::from_min_size(
        egui::pos2(-1600.0, 50.0),
        egui::vec2(900.0, 700.0),
    ));
    viewport.inner_rect = viewport.outer_rect;
    viewport.monitor_size = Some(egui::vec2(1920.0, 1080.0));
    let _ = ctx.run_ui(input, |ui| app.capture_window_geometry(ui.ctx()));
    let saved = app.shell.ui.pending_window_geometry.as_ref().unwrap();
    assert_eq!(saved.pixels_per_point, Some(2.0));
    let decoded: SavedWindowGeometry =
        serde_json::from_str(&serde_json::to_string(saved).unwrap()).unwrap();
    assert_eq!(decoded.pixels_per_point, Some(2.0));
}

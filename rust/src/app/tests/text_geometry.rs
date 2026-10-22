use super::*;

#[test]
fn single_line_labels_escape_line_separators_without_changing_ordinary_paths() {
    use crate::app::render::single_line_display_text;
    let ordinary = r"C:\work\日本語\literal\n";
    assert!(matches!(
        single_line_display_text(ordinary),
        std::borrow::Cow::Borrowed(value) if value == ordinary
    ));
    assert_eq!(
        single_line_display_text("/a\r\nb\u{85}c\u{2028}d\u{2029}e"),
        r"/a\r\nb\u{85}c\u{2028}d\u{2029}e"
    );
}

fn typography_context(pixels_per_point: f32, font_size: f32, glyph_offset: f32) -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(pixels_per_point);
    let mut fonts = egui::FontDefinitions::default();
    // Model platform font ink extending beyond the nominal line box without
    // depending on a proprietary Windows font being installed on the test host.
    for font in fonts.font_data.values_mut() {
        Arc::make_mut(font).tweak.y_offset = glyph_offset;
    }
    ctx.set_fonts(fonts);
    ctx.all_styles_mut(|style| {
        style.text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::proportional(font_size),
        );
    });
    ctx
}

fn painted_text<'a>(
    output: &'a egui::FullOutput,
    label: &str,
) -> (&'a egui::epaint::TextShape, egui::Rect) {
    fn find<'a>(shape: &'a egui::Shape, label: &str) -> Option<&'a egui::epaint::TextShape> {
        match shape {
            egui::Shape::Text(text) if text.galley.text() == label => Some(text),
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, label)),
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .find_map(|shape| find(&shape.shape, label).map(|text| (text, shape.clip_rect)))
        .unwrap_or_else(|| panic!("missing painted text: {label}"))
}

fn assert_vertical_ink_visible(text: &egui::epaint::TextShape, clip: egui::Rect) {
    let ink = text.visual_bounding_rect();
    assert!(
        ink.top() >= clip.top() && ink.bottom() <= clip.bottom(),
        "glyph ink must fit vertically: label={}, ink={ink:?}, clip={clip:?}",
        text.galley.text()
    );
}

#[test]
fn root_selector_preserves_vertical_glyph_ink_across_font_metrics_and_dpi() {
    let root = test_root("typography-Ågj-日本語");
    fs::create_dir_all(&root).expect("fixture root");
    let mut app = FlistWalkerApp::new(root.clone(), 30, String::new());
    for scale in [1.0, 1.25, 1.5, 2.0] {
        for size in [14.0, 22.0] {
            for offset in [-4.0, 0.0, 4.0] {
                let ctx = typography_context(scale, size, offset);
                let mut output = egui::FullOutput::default();
                for _ in 0..3 {
                    output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(1200.0, 800.0),
                            )),
                            ..Default::default()
                        },
                        |ui| crate::app::render_panels::render_top_panel(&mut app, ui),
                    );
                }
                let (text, clip) = painted_text(&output, &app.root_display_text());
                assert_vertical_ink_visible(text, clip);
                let (browse, _) = painted_text(&output, "Browse...");
                assert!(clip.right() < browse.pos.x, "root must stay before actions");
            }
        }
    }
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn selectable_path_rows_preserve_ink_and_parent_clip_across_font_metrics_and_dpi() {
    let label = format!("C:\\Ågj_日本語\\{}", "long-path-".repeat(20));
    for scale in [1.0, 1.25, 1.5, 2.0] {
        for size in [14.0, 22.0] {
            for offset in [-4.0, 0.0, 4.0] {
                let ctx = typography_context(scale, size, offset);
                let mut row = egui::Rect::NOTHING;
                let mut parent_clip = egui::Rect::NOTHING;
                let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    let mut clip = ui.clip_rect();
                    clip.max.x = 160.0;
                    ui.set_clip_rect(clip);
                    parent_clip = ui.clip_rect();
                    row = FlistWalkerApp::selectable_row(ui, true, &label).rect;
                });
                let (text, clip) = painted_text(&output, &label);
                assert_vertical_ink_visible(text, clip);
                assert!(row.contains_rect(clip), "keep horizontal row clipping");
                assert!(
                    parent_clip.contains_rect(clip),
                    "honor scroll viewport clipping"
                );
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn regression_multiline_root_keeps_query_visible_and_preserves_the_path() {
    let root = test_root(&format!("multiline-root-{}end", "x\n".repeat(40)));
    fs::create_dir_all(&root).expect("create valid POSIX directory with newlines");
    let mut app = FlistWalkerApp::new(root.clone(), 30, String::new());
    let ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 400.0));
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
    let query = ctx
        .read_response(app.shell.ui.query_input_id)
        .expect("query response");
    assert!(
        screen.contains_rect(query.rect),
        "Root must not push the query out of the client area: {:?}",
        query.rect
    );
    let expected_label = app.root_display_text().replace('\n', "\\n");
    let (text, clip) = painted_text(&output, &expected_label);
    assert_eq!(text.galley.rows.len(), 1);
    assert_vertical_ink_visible(text, clip);
    assert_eq!(app.shell.runtime.root, root, "escape display text only");
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn regression_offscreen_selectable_rows_do_not_fill_the_text_layout_cache() {
    for count in [100, 10_000] {
        let ctx = typography_context(1.0, 14.0, 0.0);
        for scale in [1.0, 1.5] {
            ctx.set_pixels_per_point(scale);
            for offset in [0.0, count as f32 * 10.0] {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(640.0, 400.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(240.0)
                            .vertical_scroll_offset(offset)
                            .show(ui, |ui| {
                                for index in 0..count {
                                    FlistWalkerApp::selectable_row(
                                        ui,
                                        false,
                                        &format!("C:\\Ågj_日本語\\root-{index:05}"),
                                    );
                                }
                            });
                    },
                );
                let cached = ctx.fonts_mut(|fonts| fonts.num_galleys_in_cache());
                assert!(
                    cached < 64,
                    "only viewport-sized text should be laid out, including after DPI changes: \
                 count={count}, scale={scale}, offset={offset}, cached={cached}, shapes={}",
                    output.shapes.len()
                );
            }
        }
    }
}

#[test]
fn selectable_row_heights_stay_stable_for_labels_and_font_overrides() {
    let ctx = typography_context(1.25, 14.0, -4.0);
    for override_font in [None, Some(egui::FontId::proportional(28.0))] {
        ctx.all_styles_mut(|style| style.override_font_id = override_font.clone());
        let labels = ["plain", "Ågj_日本語", "line\r\nbreak", ""];
        let mut heights = Vec::new();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            for label in labels {
                heights.push(
                    FlistWalkerApp::selectable_row(ui, false, label)
                        .rect
                        .height(),
                );
            }
        });
        assert!(heights.iter().all(|height| *height == heights[0]));
        for label in &labels[..3] {
            let label = crate::app::render::single_line_display_text(label);
            let (text, clip) = painted_text(&output, &label);
            assert_eq!(text.galley.rows.len(), 1);
            assert_vertical_ink_visible(text, clip);
        }
    }
}

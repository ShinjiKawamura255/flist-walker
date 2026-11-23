use super::*;
use crate::runtime_config::ThemeMode;

#[test]
fn preset_editor_combo_captions_and_popup_rows_align_with_controls() {
    use crate::search_catalog::{PresetEntryType, PresetSortMode, PresetSource};

    let scope = test_settings_scope("preset-combo-ink");
    let mut app = scope.app(test_root("preset-combo-ink"), 30, String::new());
    app.shell.features.presets.picker.open = true;
    app.start_add_preset();
    let editor = &mut app.shell.features.presets.picker.editor;
    editor.entry_type = PresetEntryType::File;
    editor.sort = PresetSortMode::Score;
    for (scale, offset) in [(1.0, -4.0), (1.25, 0.0), (2.0, 4.0)] {
        app.shell.features.presets.picker.editor.source = PresetSource::Walker;
        let ctx = typography_context(scale, 16.0, offset);
        let font = egui::FontId::proportional(16.0);
        ctx.all_styles_mut(|style| {
            style
                .text_styles
                .insert(egui::TextStyle::Body, font.clone());
        });
        let mut frame = |events| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200.0, 800.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| crate::app::render_dialogs::render_preset_picker_dialog(&mut app, ui.ctx()),
            )
        };
        frame(vec![]);
        frame(vec![]);
        let output = frame(vec![]);
        let cap = ctx
            .fonts_mut(|fonts| fonts.layout_no_wrap("H".into(), font.clone(), egui::Color32::WHITE))
            .mesh_bounds
            .center()
            .y;
        let mut source_arrow = egui::Pos2::ZERO;
        for (caption, value) in [
            ("Named root", "Path snapshot"),
            ("Entry type", "Files"),
            ("Source", "Walker"),
            ("Sort", "Score"),
        ] {
            let (text, _) = painted_text(&output, value);
            let arrow = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Path(path) if path.closed && path.points.len() == 3 => {
                        Some(egui::Rect::from_points(&path.points).center())
                    }
                    _ => None,
                })
                .filter(|point| point.x > text.pos.x)
                .min_by(|a, b| {
                    let y = text.visual_bounding_rect().center().y;
                    (a.y - y).abs().total_cmp(&(b.y - y).abs())
                })
                .expect("preset dropdown arrow");
            for label in [caption, value] {
                let (text, clip) = painted_text(&output, label);
                assert_vertical_ink_visible(text, clip);
                assert!(
                    (text.pos.y + cap - arrow.y).abs() <= 1.0 / scale,
                    "{label} at {scale}× offset {offset}: cap center {}, arrow {}",
                    text.pos.y + cap,
                    arrow.y
                );
            }
            if caption == "Source" {
                source_arrow = arrow;
            }
        }
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
        frame(click(source_arrow, true));
        frame(click(source_arrow, false));
        frame(vec![]);
        let popup = frame(vec![]);
        let option = painted_text(&popup, "FileList")
            .0
            .visual_bounding_rect()
            .center();
        let hovered = frame(vec![egui::Event::PointerMoved(option)]);
        let (text, clip) = painted_text(&hovered, "FileList");
        assert_vertical_ink_visible(text, clip);
        let row = hovered
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Rect(r) if r.rect.contains(option) => Some(r.rect),
                _ => None,
            })
            .min_by(|a, b| a.height().total_cmp(&b.height()))
            .expect("hovered popup row background");
        assert!(
            (text.pos.y + cap - row.center().y).abs() <= 1.0 / scale,
            "popup at {scale}× offset {offset}: cap center {}, row {row:?}",
            text.pos.y + cap
        );
        frame(click(option, true));
        frame(click(option, false));
        assert_eq!(
            app.shell.features.presets.picker.editor.source,
            PresetSource::Filelist
        );
    }
}

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
fn tab_title_and_symbols_center_visible_ink_in_their_controls() {
    let scope = test_settings_scope("tab-title-ink");
    let mut app = scope.app(test_root("tab-title-ink").join("docs"), 30, String::new());
    for (scale, offset) in [(1.0, -4.0), (1.25, 0.0), (2.0, 4.0)] {
        let ctx = typography_context(scale, 16.0, offset);
        let frame = |app: &mut FlistWalkerApp| {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                render_tabs::render_tab_bar(app, ui);
            })
        };
        frame(&mut app);
        let output = frame(&mut app);
        let controls = ctx
            .data(|data| {
                data.get_temp::<render_tabs::TabBarRenderProbe>(egui::Id::new("tab-bar-probe"))
            })
            .unwrap();
        let tab = controls.active.unwrap();
        for (label, rect) in [
            ("docs", tab),
            ("×", tab),
            ("+", controls.add),
            ("⚙", controls.settings),
        ] {
            let (text, clip) = painted_text(&output, label);
            let ink = text.visual_bounding_rect();
            assert_vertical_ink_visible(text, clip);
            assert!(
                (ink.center().y - rect.center().y).abs() * scale <= 1.0,
                "{label} at {scale}× offset {offset}: ink {ink:?}, control {rect:?}"
            );
        }
    }
}

#[test]
fn single_line_editor_uses_its_padding_to_center_text_with_the_caret() {
    let scope = test_settings_scope("editor-ink");
    let mut app = scope.app(test_root("editor-ink"), 30, "Report".to_owned());
    let ctx = typography_context(1.0, 14.0, -3.0);
    let mut frame = || {
        ctx.run_ui(egui::RawInput::default(), |ui| {
            render_panels::render_top_panel(&mut app, ui);
        })
    };
    frame();
    let output = frame();
    let (text, clip) = painted_text(&output, "Report");
    let field = ctx
        .read_response(app.shell.ui.query_input_id())
        .unwrap()
        .rect;
    let ink = text.visual_bounding_rect();
    assert_vertical_ink_visible(text, clip);
    assert!(
        (ink.center().y - field.center().y).abs() <= 1.0,
        "ink={ink:?}, field={field:?}"
    );
    let ime = output
        .platform_output
        .ime
        .expect("focused query editor reports its caret");
    assert!(field.contains_rect(ime.rect));
    let line = text.galley.pos_from_cursor(egui::text::CCursor::new(0));
    assert!((ime.cursor_rect.center().y - (text.pos.y + line.center().y)).abs() <= 1.0);
    assert!(ime.cursor_rect.bottom() >= ink.bottom());
}

#[test]
fn compact_checkbox_centers_its_icon_and_visible_label_across_font_metrics() {
    for scale in [1.0, 1.25, 2.0] {
        for offset in [-4.0, 0.0, 4.0] {
            let ctx = typography_context(scale, 16.0, offset);
            let mut controls = Vec::new();
            let mut checked = false;
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                ui.horizontal(|ui| {
                    for label in ["Regex", "Files", "Ignore Case", "Use FileList"] {
                        controls.push((
                            label,
                            render_panels::centered_checkbox(ui, &mut checked, label).rect,
                        ));
                    }
                });
            });
            let mut previous_checkbox_y = None;
            for (label, rect) in controls {
                let (text, clip) = painted_text(&output, label);
                let checkbox = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Rect(shape)
                            if rect.contains(shape.rect.center())
                                && shape.rect.right() < text.pos.x =>
                        {
                            Some(shape.rect)
                        }
                        _ => None,
                    })
                    .expect("painted checkbox frame");
                let ink = text.visual_bounding_rect();
                if let Some(y) = previous_checkbox_y {
                    assert_eq!(
                        checkbox.center().y,
                        y,
                        "checkboxes in one row must line up at {scale}×"
                    );
                }
                previous_checkbox_y = Some(checkbox.center().y);
                // Capitals and descending letters on a shared baseline have
                // slightly different ink centers. Allow a point plus raster rounding.
                assert!(
                    (checkbox.center().y - ink.center().y).abs() <= 1.0 + 0.5 / scale,
                    "checkbox={checkbox:?}, ink={ink:?}, scale={scale}, offset={offset}"
                );
                assert!(rect.contains_rect(checkbox) && rect.contains_rect(ink));
                assert_vertical_ink_visible(text, clip);
            }
        }
    }
}

#[test]
fn results_and_preview_headings_align_with_the_results_header() {
    let scope = test_settings_scope("panel-heading-centers");
    let mut app = scope.app(test_root("panel-heading-centers"), 30, String::new());
    app.shell.ui.set_show_preview(true);
    for (scale, offset) in [(1.0, -4.0), (1.25, 0.0), (2.0, 4.0)] {
        let ctx = typography_context(scale, 14.0, offset);
        let frame = |app: &mut FlistWalkerApp| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1600.0, 800.0),
                    )),
                    ..Default::default()
                },
                |ui| render_panels::render_results_and_preview(app, ui),
            )
        };
        frame(&mut app);
        let output = frame(&mut app);
        let (sort, _) = painted_text(&output, "Score");
        for label in ["Results", "Preview"] {
            let (heading, clip) = painted_text(&output, label);
            let center = heading.visual_bounding_rect().center().y;
            let sort_center = sort.visual_bounding_rect().center().y;
            assert!(
                (center - sort_center).abs() <= 1.0 / scale,
                "heading {label} center={center}, sort={sort_center} at {scale}×, offset={offset}"
            );
            assert_vertical_ink_visible(heading, clip);
        }
    }
}

// Assert relationships in the actual paint output, rather than reproducing
// the layout algorithm or fixing all widget coordinates in a golden file.
fn assert_toolbar_run(
    output: &egui::FullOutput,
    labels: &[&str],
    screen: egui::Rect,
    scale: f32,
) -> egui::Rect {
    let mut bounds = egui::Rect::NOTHING;
    let mut previous: Option<&egui::epaint::TextShape> = None;
    let baseline = |text: &egui::epaint::TextShape| {
        let row = &text.galley.rows[0];
        text.pos.y + row.pos.y + row.glyphs[0].pos.y
    };
    for label in labels {
        let (text, clip) = painted_text(output, label);
        let ink = text.visual_bounding_rect();
        assert_eq!(text.galley.rows.len(), 1, "single-line control: {label}");
        assert_vertical_ink_visible(text, clip);
        assert!(
            screen.contains_rect(ink) && clip.contains_rect(ink),
            "toolbar text must fit: label={label}, ink={ink:?}, clip={clip:?}, screen={screen:?}"
        );
        if let Some(previous) = previous {
            let previous_ink = previous.visual_bounding_rect();
            if ink.left() > previous_ink.left() {
                assert!(
                    (baseline(text) - baseline(previous)).abs() <= 1.0 / scale,
                    "same-row toolbar baselines: {} -> {label}",
                    previous.galley.text()
                );
                assert!(previous_ink.right() < ink.left(), "no overlap: {label}");
            } else {
                assert!(
                    previous_ink.bottom() < ink.top(),
                    "controls must retain reading order when wrapping: {} -> {label}",
                    previous.galley.text()
                );
            }
        }
        bounds = bounds.union(ink);
        previous = Some(text);
    }
    bounds
}

#[test]
fn top_toolbar_geometry_preserves_order_baselines_and_query_space() {
    let scope = test_settings_scope("top-toolbar-geometry");
    let mut app = scope.app(test_root("top-toolbar-geometry-root"), 30, String::new());
    reset_index_request_state_for_test(&mut app);
    // These are the reviewed existing surface order, deliberately independent
    // of top_action_labels() so a renderer reorder cannot change the oracle.
    let filters = [
        "Use FileList",
        "Regex",
        "Ignore Case",
        "Use Ignore List",
        "Files",
        "Folders",
        "Follow links",
        "Preview",
    ];
    let actions = [
        "Open / Execute",
        "Copy Path(s)",
        "Clear Selected",
        "Create File List",
        "Refresh Index",
        "Stop Indexing",
        "Stop All Indexing",
        "Presets...",
        "Help",
    ];
    for size in [egui::vec2(640.0, 400.0), egui::vec2(1200.0, 800.0)] {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for theme in [ThemeMode::Light, ThemeMode::Dark] {
                let ctx = typography_context(scale, 14.0, 0.0);
                render_theme::apply_theme(&ctx, theme);
                let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
                let mut output = egui::FullOutput::default();
                for _ in 0..4 {
                    output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(screen),
                            ..Default::default()
                        },
                        |ui| crate::app::render_panels::render_top_panel(&mut app, ui),
                    );
                }
                let filters = assert_toolbar_run(&output, &filters, screen, scale);
                let actions = assert_toolbar_run(&output, &actions, screen, scale);
                let root_actions = assert_toolbar_run(
                    &output,
                    &["Browse...", "Set as default", "Manage list"],
                    screen,
                    scale,
                );
                assert!(
                    root_actions.bottom() < filters.top(),
                    "filters must remain below Root actions: {root_actions:?}, {filters:?}"
                );
                let query = ctx
                    .read_response(app.shell.ui.query_input_id)
                    .expect("query response")
                    .rect;
                assert!(
                    screen.contains_rect(query),
                    "query stays visible: {query:?}"
                );
                assert!(filters.bottom() < query.top() && query.bottom() < actions.top(),
                    "filters, query, actions retain vertical order: {filters:?}, {query:?}, {actions:?}");
                if size.x == 640.0 {
                    let (first, _) = painted_text(&output, "Open / Execute");
                    let (last, _) = painted_text(&output, "Help");
                    assert!(
                        last.pos.y > first.pos.y,
                        "narrow case must exercise wrapping"
                    );
                }
            }
        }
    }
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
                assert!((text.visual_bounding_rect().center().y - clip.center().y).abs() <= 1.0 / scale,
                    "root text must be centered in its button: scale={scale}, size={size}, offset={offset}");
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

#[test]
fn results_header_controls_align_with_shown_text_across_fonts_and_dpi() {
    let scope = test_settings_scope("results-header-alignment");
    let mut app = scope.app(
        test_root("results-header-alignment-root"),
        30,
        String::new(),
    );
    reset_index_request_state_for_test(&mut app);
    for scale in [1.0, 1.25, 1.5, 2.0] {
        for size in [14.0, 22.0] {
            for offset in [-4.0, 0.0, 4.0] {
                let ctx = typography_context(scale, size, offset);
                ctx.all_styles_mut(|style| {
                    style
                        .text_styles
                        .insert(egui::TextStyle::Body, egui::FontId::proportional(size));
                    style.text_styles.insert(
                        egui::TextStyle::Heading,
                        egui::FontId::proportional(size + 4.0),
                    );
                });
                let mut output = egui::FullOutput::default();
                for _ in 0..3 {
                    output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(1400.0, 800.0),
                            )),
                            ..Default::default()
                        },
                        |ui| crate::app::render_panels::render_results_list(&mut app, ui),
                    );
                }
                let (shown, _) = painted_text(&output, "0 shown");
                let baseline = |text: &egui::epaint::TextShape| {
                    let row = &text.galley.rows[0];
                    text.pos.y + row.pos.y + row.glyphs[0].pos.y
                };
                for label in ["Sorted by", "Score", "Scope", "Shown results"] {
                    let (text, clip) = painted_text(&output, label);
                    assert!((baseline(text) - baseline(shown)).abs() <= 1.0 / scale,
                        "Results controls must share shown-text baseline: label={label}, scale={scale}, size={size}, offset={offset}, shown={}, actual={}, shown_pos={:?}, text_pos={:?}", baseline(shown), baseline(text), shown.pos, text.pos);
                    assert_vertical_ink_visible(text, clip);
                    if matches!(label, "Score" | "Shown results") {
                        let arrow = output
                            .shapes
                            .iter()
                            .filter_map(|shape| match &shape.shape {
                                egui::Shape::Path(path)
                                    if path.closed && path.points.len() == 3 =>
                                {
                                    Some(egui::Rect::from_points(&path.points))
                                }
                                _ => None,
                            })
                            .filter(|arrow| {
                                arrow.left() >= clip.right()
                                    && (arrow.center().y - text.visual_bounding_rect().center().y)
                                        .abs()
                                        < 20.0
                            })
                            .min_by(|a, b| a.left().total_cmp(&b.left()))
                            .expect("dropdown arrow");
                        assert!((text.visual_bounding_rect().center().y - arrow.center().y).abs() <= 1.0 / scale,
                            "dropdown ink and arrow centers: label={label}, scale={scale}, size={size}, offset={offset}, ink={:?}, arrow={arrow:?}", text.visual_bounding_rect());
                    }
                }
            }
        }
    }
}

#[test]
fn ordinary_buttons_keep_a_common_baseline_and_center_cap_height() {
    let scope = test_settings_scope("button-ink-baseline");
    let app = scope.app(test_root("button-ink-baseline"), 30, String::new());
    for (scale, offset) in [(1.0, -4.0), (1.25, 0.0), (2.0, 4.0)] {
        let ctx = typography_context(scale, 16.0, offset);
        let mut center = 0.0;
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.horizontal(|ui| {
                center = app.dialog_button(ui, "H", false).rect.center().y;
                app.dialog_button(ui, "Apply", true);
            });
        });
        let (cap, _) = painted_text(&output, "H");
        let (apply, clip) = painted_text(&output, "Apply");
        assert!(
            (cap.visual_bounding_rect().center().y - center).abs() <= 1.0 / scale,
            "ordinary cap ink at scale {scale} offset {offset}: {:?}, center={center}",
            cap.visual_bounding_rect()
        );
        assert!(
            (cap.pos.y - apply.pos.y).abs() <= 1.0 / scale,
            "different baselines"
        );
        assert_vertical_ink_visible(apply, clip);
    }
}

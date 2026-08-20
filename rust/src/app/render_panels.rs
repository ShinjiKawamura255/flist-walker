mod top_panel;
mod widgets;

use self::widgets::{centered_top_panel_label, paint_compact_combo_selected_text};
use super::{
    render_theme, EntryDisplayKind, EntryKind, FlistWalkerApp, ResultSortMode, ResultSortScope,
};
use eframe::egui;
use std::path::Path;

pub(super) fn centered_checkbox(
    ui: &mut egui::Ui,
    checked: &mut bool,
    label: &str,
) -> egui::Response {
    top_panel::centered_checkbox(ui, checked, label)
}

#[cfg(test)]
pub(super) fn centered_checkbox_layout(
    rect: egui::Rect,
    checkbox_size: f32,
    icon_spacing: f32,
    text_size: egui::Vec2,
    label_y_offset: f32,
    checkbox_y_offset: f32,
) -> (egui::Rect, egui::Pos2) {
    top_panel::centered_checkbox_layout(
        rect,
        checkbox_size,
        icon_spacing,
        text_size,
        label_y_offset,
        checkbox_y_offset,
    )
}

#[cfg(test)]
pub(super) fn update_max_depth_draft_for_unlimited(draft: &mut usize, unlimited: bool) {
    top_panel::update_max_depth_draft_for_unlimited(draft, unlimited);
}

pub(super) fn render_top_panel(app: &mut FlistWalkerApp, ui: &mut egui::Ui) {
    top_panel::render(app, ui);
}

pub(super) fn preview_text_style() -> egui::TextStyle {
    // The proportional family has the asynchronously loaded CJK font first.
    // Using the monospace fallback made Japanese glyphs and Latin text use
    // different vertical metrics, which visibly shifted mixed-language lines.
    egui::TextStyle::Body
}

pub(super) fn render_status_panel(app: &mut FlistWalkerApp, ui: &mut egui::Ui) {
    egui::Panel::bottom("status")
        .resizable(false)
        .exact_size(24.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let version_text = format!("v{}", env!("CARGO_PKG_VERSION"));
                let version_font = egui::TextStyle::Body.resolve(ui.style());
                let version_width = ui
                    .painter()
                    .layout_no_wrap(
                        version_text.clone(),
                        version_font.clone(),
                        ui.visuals().text_color(),
                    )
                    .size()
                    .x;
                if let Some(label) = app.action_progress_label() {
                    ui.add(egui::Spinner::new().size(14.0));
                    ui.label(label);
                    ui.separator();
                }
                if app.can_cancel_create_filelist() {
                    let cancel_label = if app.shell.features.filelist.workflow.cancel_requested {
                        "Canceling FileList..."
                    } else {
                        "Cancel Create File List"
                    };
                    if ui
                        .add_enabled(
                            !app.shell.features.filelist.workflow.cancel_requested,
                            egui::Button::new(cancel_label),
                        )
                        .clicked()
                    {
                        app.cancel_create_filelist();
                    }
                    ui.separator();
                }
                let reserved_width =
                    version_width + ui.spacing().item_spacing.x + ui.spacing().icon_width;
                let status_width = (ui.available_width() - reserved_width).max(0.0);
                let status_line = app.status_line_text();
                ui.allocate_ui_with_layout(
                    egui::vec2(status_width, ui.available_height()),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_width(status_width);
                        ui.add(egui::Label::new(status_line).truncate());
                    },
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(version_text);
                    ui.separator();
                });
            });
        });
}

fn result_row_height(ui: &egui::Ui) -> f32 {
    ui.text_style_height(&egui::TextStyle::Body) + (FlistWalkerApp::RESULT_ROW_V_MARGIN * 2.0)
}

pub(super) fn render_results_and_preview(app: &mut FlistWalkerApp, ui: &mut egui::Ui) {
    if app.shell.runtime.query_state.history_search_active {
        app.shell.ui.set_preview_resize_in_progress(false);
        render_history_search_results(app, ui);
        app.clear_scroll_to_current();
        return;
    }
    if app.shell.ui.show_preview() {
        let max_preview_width = (ui.available_width() - FlistWalkerApp::MIN_RESULTS_PANEL_WIDTH)
            .max(FlistWalkerApp::MIN_PREVIEW_PANEL_WIDTH);
        let panel = egui::Panel::right("preview-panel")
            .resizable(true)
            .default_size(app.shell.ui.preview_panel_width().min(max_preview_width))
            .min_size(FlistWalkerApp::MIN_PREVIEW_PANEL_WIDTH)
            .max_size(max_preview_width);
        let response = panel.show(ui, |ui| {
            ui.heading("Preview");
            let preview_width = ui.available_width();
            let preview_height = ui.available_height();
            ui.allocate_ui_with_layout(
                egui::vec2(preview_width, preview_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    let frame_fill = ui.visuals().extreme_bg_color;
                    egui::Frame::NONE.fill(frame_fill).show(ui, |ui| {
                        ui.set_min_size(egui::vec2(preview_width, preview_height));
                        egui::ScrollArea::both()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.add_sized(
                                    egui::vec2(preview_width, preview_height),
                                    egui::TextEdit::multiline(&mut app.shell.runtime.preview)
                                        .interactive(false)
                                        .font(preview_text_style())
                                        .desired_width(f32::INFINITY)
                                        .desired_rows(1),
                                );
                            });
                    });
                },
            );
        });
        let new_width = response
            .response
            .rect
            .width()
            .max(FlistWalkerApp::MIN_PREVIEW_PANEL_WIDTH);
        if (new_width - app.shell.ui.preview_panel_width()).abs() > 1.0 {
            app.shell.ui.set_preview_panel_width(new_width);
            app.mark_ui_state_dirty();
        }
        let splitter_x = response.response.rect.left();
        let splitter_pressed = ui.input(|i| {
            let Some(pos) = i.pointer.interact_pos() else {
                return false;
            };
            i.pointer.primary_down() && (pos.x - splitter_x).abs() <= 8.0
        });
        app.shell
            .ui
            .set_preview_resize_in_progress(response.response.dragged() || splitter_pressed);
        render_results_list(app, ui);
    } else {
        app.shell.ui.set_preview_resize_in_progress(false);
        render_results_list(app, ui);
    }
    app.clear_scroll_to_current();
}

pub(super) fn render_results_list(app: &mut FlistWalkerApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.heading("Results");
        let total = app.shell.runtime.total_match_count;
        let shown = app.shell.runtime.results.len();
        if total > shown {
            ui.label(format!("{shown} of {total} shown"));
        } else {
            ui.label(format!("{shown} shown"));
        }
        let row_height = ui.spacing().interact_size.y;
        let row_width = ui.available_width();
        ui.allocate_ui_with_layout(
            egui::vec2(row_width, row_height),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                let mut selected_scope = app.shell.runtime.result_sort_scope;
                let scope_response = egui::ComboBox::from_id_salt("results-sort-scope-selector")
                    .width(126.0)
                    .selected_text("")
                    .show_ui(ui, |ui| {
                        ui.set_min_width(126.0);
                        for scope in [ResultSortScope::ShownResults, ResultSortScope::AllMatches] {
                            ui.selectable_value(&mut selected_scope, scope, scope.label());
                        }
                    })
                    .response;
                paint_compact_combo_selected_text(ui, &scope_response, selected_scope.label());
                centered_top_panel_label(ui, "Scope");
                let mut selected = app.shell.runtime.result_sort_mode;
                let sort_response = egui::ComboBox::from_id_salt("results-sort-selector")
                    .width(FlistWalkerApp::RESULT_SORT_SELECTOR_WIDTH)
                    .selected_text("")
                    .show_ui(ui, |ui| {
                        ui.set_min_width(FlistWalkerApp::RESULT_SORT_SELECTOR_WIDTH);
                        for mode in [
                            ResultSortMode::Score,
                            ResultSortMode::NameAsc,
                            ResultSortMode::NameDesc,
                            ResultSortMode::ModifiedDesc,
                            ResultSortMode::ModifiedAsc,
                            ResultSortMode::CreatedDesc,
                            ResultSortMode::CreatedAsc,
                            ResultSortMode::SizeDesc,
                            ResultSortMode::SizeAsc,
                        ] {
                            ui.selectable_value(&mut selected, mode, mode.label());
                        }
                    })
                    .response;
                paint_compact_combo_selected_text(ui, &sort_response, selected.label());
                centered_top_panel_label(ui, "Sorted by");
                if selected != app.shell.runtime.result_sort_mode {
                    app.set_result_sort_mode(selected);
                }
                if selected_scope != app.shell.runtime.result_sort_scope {
                    app.set_result_sort_scope(selected_scope);
                }
            },
        );
    });
    let scroll_enabled =
        FlistWalkerApp::results_scroll_enabled(app.shell.ui.preview_resize_in_progress());
    let scroll_source = if scroll_enabled {
        egui::scroll_area::ScrollSource::SCROLL_BAR | egui::scroll_area::ScrollSource::MOUSE_WHEEL
    } else {
        egui::scroll_area::ScrollSource::NONE
    };
    egui::ScrollArea::both()
        .scroll_source(scroll_source)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut clicked_row: Option<usize> = None;
            let mut execute_row: Option<usize> = None;
            let prefer_relative = app.prefer_relative_display();
            app.ensure_highlight_cache_scope(prefer_relative);
            let clip_rect = ui.clip_rect();
            let row_width = ui.available_width().max(0.0);
            let row_height = result_row_height(ui);

            for i in 0..app.shell.runtime.results.len() {
                let Some((path, _score)) = app.shell.runtime.results.get(i) else {
                    continue;
                };
                let path = path.clone();
                let is_current = app.shell.runtime.current_row == Some(i);
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(row_width, row_height), egui::Sense::click());
                if is_current && app.shell.ui.scroll_to_current() {
                    ui.scroll_to_rect(rect, None);
                }
                if clip_rect.intersects(rect) {
                    render_result_row(app, ui, rect, &path, is_current, prefer_relative);
                }
                if response.clicked() {
                    clicked_row = Some(i);
                }
                if response.double_clicked() {
                    execute_row = Some(i);
                }
            }
            if let Some(i) = clicked_row {
                app.set_current_row(Some(i));
                app.request_preview_for_current();
                app.refresh_status_line();
            }
            if let Some(i) = execute_row {
                app.set_current_row(Some(i));
                let open_parent_for_files = ui.input(|i| i.modifiers.shift);
                app.execute_selected_for_activation(open_parent_for_files);
            }
        });
}

pub(super) fn render_history_search_results(app: &mut FlistWalkerApp, ui: &mut egui::Ui) {
    ui.heading("History Results");
    ui.label(format!(
        "{} items in history, {} matches",
        app.shell.runtime.query_state.query_history.len(),
        app.shell.runtime.query_state.history_search_results.len()
    ));
    egui::ScrollArea::vertical()
        .scroll_source(
            egui::scroll_area::ScrollSource::SCROLL_BAR
                | egui::scroll_area::ScrollSource::MOUSE_WHEEL,
        )
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut clicked_row: Option<usize> = None;
            let mut accept_row: Option<usize> = None;

            for (index, entry) in app
                .shell
                .runtime
                .query_state
                .history_search_results
                .iter()
                .enumerate()
            {
                let is_current =
                    app.shell.runtime.query_state.history_search_current == Some(index);
                let prefix = if is_current { "▶" } else { "·" };
                let text = format!("{prefix} {entry}");
                let selected_bg = render_theme::selected_fill(ui.visuals().dark_mode);
                let fill = if is_current {
                    selected_bg
                } else {
                    egui::Color32::TRANSPARENT
                };
                let row = egui::Frame::NONE
                    .fill(fill)
                    .inner_margin(egui::Margin::symmetric(3, 2))
                    .corner_radius(egui::CornerRadius::same(3))
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new(text).monospace())
                                .extend()
                                .sense(egui::Sense::click()),
                        )
                    });
                let response = row.inner;
                if response.clicked() {
                    clicked_row = Some(index);
                }
                if response.double_clicked() {
                    accept_row = Some(index);
                }
            }

            if let Some(index) = clicked_row {
                app.shell.runtime.query_state.history_search_current = Some(index);
            }
            if let Some(index) = accept_row {
                app.shell.runtime.query_state.history_search_current = Some(index);
                app.accept_history_search();
            }
        });
}

fn render_result_row(
    app: &mut FlistWalkerApp,
    ui: &egui::Ui,
    rect: egui::Rect,
    path: &Path,
    is_current: bool,
    prefer_relative: bool,
) {
    let is_pinned = app.shell.runtime.pinned_paths.contains(path);
    let kind = app.find_entry_kind(path);
    let display = super::display_path_with_mode(path, &app.shell.runtime.root, prefer_relative);
    let positions = app.highlight_positions_for_path_cached(path, prefer_relative);
    let job = build_result_row_job(
        ui,
        &display,
        positions.as_slice(),
        is_current,
        is_pinned,
        kind,
    );
    let selected_bg = render_theme::selected_fill(ui.visuals().dark_mode);
    if is_current {
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(FlistWalkerApp::RESULT_ROW_ROUNDING as u8),
            selected_bg,
        );
    }

    let inner_rect = rect.shrink2(egui::vec2(
        FlistWalkerApp::RESULT_ROW_H_MARGIN,
        FlistWalkerApp::RESULT_ROW_V_MARGIN,
    ));
    let galley = ui.painter().layout_job(job);
    let text_pos = FlistWalkerApp::result_row_text_pos(inner_rect, galley.size());
    ui.painter()
        .galley(text_pos, galley, ui.visuals().text_color());
}

fn build_result_row_job(
    ui: &egui::Ui,
    display: &str,
    positions: &[u16],
    is_current: bool,
    is_pinned: bool,
    kind: Option<EntryKind>,
) -> egui::text::LayoutJob {
    let marker_current = if is_current { "▶" } else { "·" };
    let marker_pin = if is_pinned { "◆" } else { "·" };
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &format!("{} {} ", marker_current, marker_pin),
        0.0,
        egui::TextFormat {
            color: if is_current {
                egui::Color32::LIGHT_BLUE
            } else {
                ui.visuals().weak_text_color()
            },
            ..Default::default()
        },
    );
    let (kind_label, kind_color) = match kind.map(|k| k.display) {
        Some(display_kind @ EntryDisplayKind::Dir) => {
            ("DIR ", render_theme::entry_kind_color(display_kind))
        }
        Some(display_kind @ EntryDisplayKind::File) => {
            ("FILE", render_theme::entry_kind_color(display_kind))
        }
        Some(display_kind @ EntryDisplayKind::Link) => {
            ("LINK", render_theme::entry_kind_color(display_kind))
        }
        Some(display_kind @ EntryDisplayKind::Other) => {
            ("OTHR", render_theme::entry_kind_color(display_kind))
        }
        None => ("....", ui.visuals().weak_text_color()),
    };
    job.append(
        kind_label,
        0.0,
        egui::TextFormat {
            color: kind_color,
            ..Default::default()
        },
    );
    job.append(" ", 0.0, egui::TextFormat::default());

    for (idx, ch) in display.chars().enumerate() {
        let color = if FlistWalkerApp::is_highlighted_position(positions, idx) {
            render_theme::highlight_text_color()
        } else {
            ui.visuals().text_color()
        };
        job.append(
            &ch.to_string(),
            0.0,
            egui::TextFormat {
                color,
                ..Default::default()
            },
        );
    }
    job
}

pub(super) fn render_central_panel(app: &mut FlistWalkerApp, ui: &mut egui::Ui) {
    egui::CentralPanel::default().show(ui, |ui| {
        render_results_and_preview(app, ui);
    });
}

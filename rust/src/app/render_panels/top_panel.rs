use super::widgets::centered_top_panel_label;
use crate::app::{render_tabs, FlistWalkerApp};
use crate::text_editing::char_count;
use crate::ui_model::normalize_path_for_display;
use eframe::egui;
use std::path::PathBuf;

const COMPACT_ROW_TEXT_Y_OFFSET: f32 = 2.0;
const COMPACT_ROW_CHECKBOX_Y_OFFSET: f32 = -1.0;

fn paint_root_selector_button(
    ui: &egui::Ui,
    rect: egui::Rect,
    response: &egui::Response,
    text: &str,
    popup_open: bool,
) {
    let visuals = if popup_open {
        &ui.visuals().widgets.open
    } else {
        ui.style().interact(response)
    };
    let rounding = ui.visuals().widgets.inactive.corner_radius;
    ui.painter().rect(
        rect.expand(visuals.expansion),
        rounding,
        visuals.bg_fill,
        visuals.bg_stroke,
        egui::StrokeKind::Inside,
    );

    let inner_rect = rect.shrink2(ui.spacing().button_padding);
    let icon_size = egui::Vec2::splat(ui.spacing().icon_width);
    let icon_rect = egui::Align2::RIGHT_CENTER.align_size_within_rect(icon_size, inner_rect);
    let icon_center = icon_rect.center();
    let icon_width = icon_rect.width() * 0.45;
    let icon_height = icon_rect.height() * 0.28;
    ui.painter().add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(icon_center.x - icon_width, icon_center.y - icon_height),
            egui::pos2(icon_center.x + icon_width, icon_center.y - icon_height),
            egui::pos2(icon_center.x, icon_center.y + icon_height),
        ],
        visuals.fg_stroke.color,
        egui::Stroke::NONE,
    ));

    let text_right = icon_rect.left() - ui.spacing().icon_spacing;
    let text_rect = egui::Rect::from_min_max(
        inner_rect.left_top(),
        egui::pos2(text_right.max(inner_rect.left()), inner_rect.bottom()),
    );
    let galley = egui::WidgetText::from(text.to_owned()).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Button,
    );
    let text_pos = egui::Align2::LEFT_CENTER.align_size_within_rect(galley.size(), text_rect);
    ui.painter()
        .galley(text_pos.min, galley, visuals.text_color());
}

pub(super) fn centered_checkbox_layout(
    rect: egui::Rect,
    checkbox_size: f32,
    icon_spacing: f32,
    text_size: egui::Vec2,
    label_y_offset: f32,
    checkbox_y_offset: f32,
) -> (egui::Rect, egui::Pos2) {
    let checkbox_rect = egui::Rect::from_center_size(
        egui::pos2(
            rect.left() + (checkbox_size / 2.0),
            rect.center().y + checkbox_y_offset,
        ),
        egui::Vec2::splat(checkbox_size),
    );
    let text_pos = egui::pos2(
        checkbox_rect.right() + icon_spacing,
        rect.center().y - (text_size.y / 2.0) + label_y_offset,
    );
    (checkbox_rect, text_pos)
}

pub(super) fn centered_checkbox(
    ui: &mut egui::Ui,
    checked: &mut bool,
    label: &str,
) -> egui::Response {
    let font_id = egui::TextStyle::Button.resolve(ui.style());
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font_id, ui.visuals().text_color());
    let checkbox_size = ui.spacing().icon_width;
    let desired_size = egui::vec2(
        checkbox_size + ui.spacing().icon_spacing + galley.size().x,
        ui.spacing()
            .interact_size
            .y
            .max(checkbox_size)
            .max(galley.size().y),
    );
    let (rect, mut response) = ui.allocate_exact_size(desired_size, egui::Sense::click());
    if response.clicked() {
        *checked = !*checked;
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *checked, label)
    });

    if ui.is_rect_visible(rect) {
        let checkbox_style = ui.style().checkbox_style(
            &egui::widget_style::Classes::default(),
            response.widget_state(),
        );
        let (checkbox_rect, text_pos) = centered_checkbox_layout(
            rect,
            checkbox_style.checkbox_size,
            ui.spacing().icon_spacing,
            galley.size(),
            COMPACT_ROW_TEXT_Y_OFFSET,
            COMPACT_ROW_CHECKBOX_Y_OFFSET,
        );
        ui.painter().add(egui::epaint::RectShape::new(
            checkbox_rect.expand(checkbox_style.checkbox_frame.inner_margin.left.into()),
            checkbox_style.checkbox_frame.corner_radius,
            checkbox_style.checkbox_frame.fill,
            checkbox_style.checkbox_frame.stroke,
            egui::epaint::StrokeKind::Inside,
        ));

        if *checked {
            let check_rect = egui::Rect::from_center_size(
                checkbox_rect.center(),
                egui::Vec2::splat(checkbox_style.check_size),
            );
            ui.painter().add(egui::Shape::line(
                vec![
                    egui::pos2(check_rect.left(), check_rect.center().y),
                    egui::pos2(check_rect.center().x, check_rect.bottom()),
                    egui::pos2(check_rect.right(), check_rect.top()),
                ],
                checkbox_style.check_stroke,
            ));
        }
        ui.painter()
            .galley(text_pos, galley, checkbox_style.text_style.color);
    }

    response
}

pub(super) fn update_max_depth_draft_for_unlimited(draft: &mut usize, unlimited: bool) {
    *draft = if unlimited { 0 } else { (*draft).max(1) };
}

pub(super) fn render(app: &mut FlistWalkerApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    egui::Panel::top("top").show(ui, |ui| {
        render_tabs::render_tab_bar(app, ui);
        ui.separator();
        ui.horizontal(|ui| {
            let row_height = ui.spacing().interact_size.y;
            ui.add_sized([44.0, row_height], egui::Label::new("Root:"));
            let button_width = 96.0;
            let set_default_width = 130.0;
            let manage_width = 104.0;
            let field_width = (ui.available_width()
                - button_width
                - set_default_width
                - manage_width
                - (ui.spacing().item_spacing.x * 3.0))
                .max(120.0);
            let selected_text = app.root_display_text();
            let mut next_root: Option<PathBuf> = None;
            ui.allocate_ui_with_layout(
                egui::vec2(field_width, row_height),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    app.sync_root_dropdown_highlight();
                    let popup_open = app.is_root_dropdown_open(ui.ctx());
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(field_width, row_height),
                        egui::Sense::click(),
                    );
                    paint_root_selector_button(ui, rect, &response, &selected_text, popup_open);
                    if response.clicked() {
                        if popup_open {
                            app.close_root_dropdown(ui.ctx());
                        } else {
                            app.open_root_dropdown(ui.ctx());
                        }
                    }
                    let popup_id = FlistWalkerApp::root_selector_popup_id();
                    egui::Popup::from_response(&response)
                        .id(popup_id)
                        .open_memory(None)
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                        .width(field_width)
                        .show(|ui: &mut egui::Ui| {
                            ui.set_min_width(field_width);
                            for (index, path) in app
                                .shell
                                .features
                                .root_browser
                                .saved_roots()
                                .iter()
                                .enumerate()
                            {
                                let text = normalize_path_for_display(path);
                                let is_selected = app.shell.ui.root_dropdown_highlight() == Some(index);
                                if FlistWalkerApp::selectable_row(ui, is_selected, &text).clicked() {
                                    next_root = Some(path.clone());
                                }
                            }
                        });
                },
            );
            if ui
                .add_sized([button_width, row_height], egui::Button::new("Browse..."))
                .clicked()
            {
                app.browse_for_root();
            }
            let set_default_enabled = app.can_set_current_root_as_default();
            let set_default_response = ui.add_enabled_ui(set_default_enabled, |ui| {
                ui.add_sized(
                    [set_default_width, row_height],
                    egui::Button::new("Set as default"),
                )
            });
            let set_default_response = set_default_response.inner;
            let set_default_clicked = set_default_response.clicked();
            if !set_default_enabled {
                set_default_response.on_disabled_hover_text(
                    FlistWalkerApp::SET_DEFAULT_DISABLED_BY_RESTORE_TABS_TOOLTIP,
                );
            }
            if set_default_enabled && set_default_clicked {
                app.set_current_root_as_default();
            }
            if ui
                .add_sized([manage_width, row_height], egui::Button::new("Manage list"))
                .clicked()
            {
                app.open_manage_root_list();
            }
            if let Some(root) = next_root {
                app.close_root_dropdown(ui.ctx());
                app.apply_root_change(root);
            }
        });

        ui.horizontal(|ui| {
            let use_filelist_changed =
                centered_checkbox(ui, &mut app.shell.runtime.use_filelist, "Use FileList")
                    .changed();
            if centered_checkbox(ui, &mut app.shell.runtime.use_regex, "Regex").changed() {
                app.invalidate_result_sort(true);
                app.update_results();
            }
            if centered_checkbox(ui, &mut app.shell.runtime.ignore_case, "Ignore Case").changed()
            {
                app.invalidate_result_sort(true);
                app.update_results();
            }
            let ignore_list_response = centered_checkbox(
                ui,
                &mut app.shell.ui.ignore_list_enabled,
                "Use Ignore List",
            )
                .on_hover_text("Apply executable-relative rules from flistwalker.ignore.txt");
            if ignore_list_response.changed()
            {
                app.apply_entry_filters(false);
                app.mark_ui_state_dirty();
                app.persist_ui_state_now();
            }
            let (files_changed, dirs_changed) = if app.use_filelist_requires_locked_filters() {
                let mut forced_changed = false;
                if !app.shell.runtime.include_files || !app.shell.runtime.include_dirs {
                    app.shell.runtime.include_files = true;
                    app.shell.runtime.include_dirs = true;
                    forced_changed = true;
                }
                ui.add_enabled_ui(false, |ui| {
                    centered_checkbox(ui, &mut app.shell.runtime.include_files, "Files");
                });
                ui.add_enabled_ui(false, |ui| {
                    centered_checkbox(ui, &mut app.shell.runtime.include_dirs, "Folders");
                });
                (forced_changed, forced_changed)
            } else {
                (
                    centered_checkbox(ui, &mut app.shell.runtime.include_files, "Files").changed(),
                    centered_checkbox(ui, &mut app.shell.runtime.include_dirs, "Folders").changed(),
                )
            };
            let depth_label = app
                .shell
                .runtime
                .max_depth
                .value()
                .map_or_else(|| "Depth: All".to_string(), |depth| format!("Depth: ≤ {depth}"));
            let depth_popup_id = egui::Id::new("max-depth-popup");
            let depth_response = ui.button(depth_label);
            if depth_response.clicked() {
                app.shell.ui.max_depth_draft = app.shell.runtime.max_depth.value().unwrap_or(0);
                egui::Popup::open_id(ui.ctx(), depth_popup_id);
            }
            let mut apply_depth = false;
            let mut cancel_depth = false;
            egui::Popup::from_response(&depth_response)
                .id(depth_popup_id)
                .open_memory(None)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .width(220.0)
                .show(|ui: &mut egui::Ui| {
                    let mut unlimited = app.shell.ui.max_depth_draft == 0;
                    if centered_checkbox(ui, &mut unlimited, "Unlimited").changed() {
                        update_max_depth_draft_for_unlimited(
                            &mut app.shell.ui.max_depth_draft,
                            unlimited,
                        );
                    }
                    ui.horizontal(|ui| {
                        ui.label("Maximum depth");
                        if unlimited {
                            let mut disabled_depth = 1_usize;
                            ui.add_enabled(
                                false,
                                egui::DragValue::new(&mut disabled_depth)
                                    .range(1..=u32::MAX as usize),
                            );
                        } else {
                            ui.add(
                                egui::DragValue::new(&mut app.shell.ui.max_depth_draft)
                                    .range(1..=u32::MAX as usize),
                            );
                        }
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("Apply").clicked() {
                            apply_depth = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel_depth = true;
                        }
                    });
                });
            if apply_depth {
                let next_depth = crate::indexer::MaxDepth::limited(app.shell.ui.max_depth_draft)
                    .unwrap_or_default();
                egui::Popup::close_id(ui.ctx(), depth_popup_id);
                if next_depth != app.shell.runtime.max_depth {
                    app.shell.runtime.max_depth = next_depth;
                    app.sync_active_tab_state();
                    app.mark_ui_state_dirty();
                    app.persist_ui_state_now();
                    app.request_index_refresh();
                }
            } else if cancel_depth {
                egui::Popup::close_id(ui.ctx(), depth_popup_id);
            }
            let mut show_preview = app.shell.ui.show_preview();
            if centered_checkbox(ui, &mut show_preview, "Preview").changed() {
                app.shell.ui.set_show_preview(show_preview);
                if !show_preview {
                    app.clear_preview_cache();
                }
                app.mark_ui_state_dirty();
                app.persist_ui_state_now();
            }
            ui.separator();
            centered_top_panel_label(ui, app.source_text());
            app.maybe_reindex_from_filter_toggles(
                use_filelist_changed,
                files_changed,
                dirs_changed,
            );
        });

        if app.shell.runtime.query_state.history_search_active {
            ui.label(
                egui::RichText::new("History Search")
                    .strong()
                    .color(ui.visuals().strong_text_color()),
            );
        }
        let editing_history_search = app.shell.runtime.query_state.history_search_active;
        let query_input_id = app.shell.ui.query_input_id();
        let query_focused_before_text_edit = ctx.memory(|m| m.has_focus(query_input_id));
        FlistWalkerApp::consume_disabled_emacs_text_edit_shortcuts(
            &ctx,
            query_focused_before_text_edit,
            app.shell.runtime.emacs_keybindings_enabled,
        );
        let text_before_widget = if editing_history_search {
            app.shell
                .runtime
                .query_state
                .history_search_query
                .clone()
        } else {
            app.shell.runtime.query_state.query.clone()
        };
        let mut output = egui::TextEdit::singleline(if editing_history_search {
            &mut app.shell.runtime.query_state.history_search_query
        } else {
            &mut app.shell.runtime.query_state.query
        })
            .id(query_input_id)
            .lock_focus(true)
            .desired_width(f32::INFINITY)
            .hint_text(if editing_history_search {
                "Type to fuzzy-search query history..."
            } else {
                "Type to fuzzy-search files/folders..."
            })
            .show(ui);
        let _ = egui::Response::clone(&output.response).on_hover_ui_at_pointer(|ui| {
            if editing_history_search {
                ui.label("Ctrl+R で履歴検索を開始。Enter / Ctrl+J / Ctrl+M で確定、Esc / Ctrl+G でキャンセル。");
            } else {
                ui.label(FlistWalkerApp::SEARCH_HINTS_TOOLTIP);
            }
        });
        if app.shell.ui.focus_query_requested() {
            output.response.request_focus();
            app.clear_focus_query_request();
        }
        if app.shell.ui.unfocus_query_requested() {
            output.response.surrender_focus();
            app.clear_unfocus_query_request();
        }
        if !editing_history_search && app.query_cursor_to_end_requested() {
            let end = char_count(&app.shell.runtime.query_state.query);
            output
                .state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(
                    egui::text::CCursor::new(end),
                )));
            output.state.clone().store(&ctx, output.response.id);
            app.clear_query_cursor_to_end_request();
        }
        let events = ctx.input(|i| i.events.clone());
        if !editing_history_search {
            let (query_event_changed, query_cursor_after_fallback) = app.process_query_input_events(
                &ctx,
                &events,
                output.response.has_focus(),
                output.response.changed(),
                output.state.cursor.char_range(),
            );
            if query_event_changed {
                app.mark_query_edited();
                if output.response.has_focus() {
                    let end = query_cursor_after_fallback.unwrap_or_else(|| {
                        char_count(&app.shell.runtime.query_state.query)
                    });
                    output
                        .state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(
                            egui::text::CCursor::new(end),
                        )));
                    output.state.clone().store(&ctx, output.response.id);
                }
                app.update_results();
            }
            if app.apply_emacs_query_shortcuts(&ctx, &mut output, &text_before_widget) {
                app.mark_query_edited();
                app.update_results();
            }
            if output.response.changed() {
                let normalized =
                    FlistWalkerApp::normalize_singleline_input(&mut app.shell.runtime.query_state.query);
                if normalized && output.response.has_focus() {
                    let end = char_count(&app.shell.runtime.query_state.query);
                    output
                        .state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(
                            egui::text::CCursor::new(end),
                        )));
                    output.state.clone().store(&ctx, output.response.id);
                }
                app.mark_query_edited();
                FlistWalkerApp::append_window_trace(
                    "query_text_changed",
                    &FlistWalkerApp::query_trace_summary(&app.shell.runtime.query_state.query),
                );
                app.update_results();
            }
        } else {
            let emacs_text_changed =
                app.apply_emacs_history_search_shortcuts(&ctx, &mut output, &text_before_widget);
            if output.response.changed() || emacs_text_changed {
                if FlistWalkerApp::normalize_singleline_input(
                    &mut app.shell.runtime.query_state.history_search_query,
                ) && output.response.has_focus()
                {
                    let end = char_count(&app.shell.runtime.query_state.history_search_query);
                    output
                        .state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(
                            egui::text::CCursor::new(end),
                        )));
                    output.state.clone().store(&ctx, output.response.id);
                }
                app.refresh_history_search_results();
            }
        }
        app.run_deferred_shortcuts(&ctx);

        ui.horizontal(|ui| {
            for label in app.top_action_labels() {
                let mut response = ui.button(label);
                if label == "Presets..." {
                    response = response.on_hover_text(FlistWalkerApp::preset_top_action_tooltip());
                }
                if !response.clicked() {
                    continue;
                }
                if let Some(command) = FlistWalkerApp::top_action_command(label) {
                    app.queue_render_command(crate::app::render::RenderCommand::TopAction(command));
                }
            }
        });
    });
}

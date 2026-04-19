use super::FlistWalkerApp;
use crate::path_utils::normalize_windows_path_buf;
use eframe::egui;
use std::path::PathBuf;

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
    let rounding = ui.visuals().widgets.inactive.rounding;
    ui.painter().rect(
        rect.expand(visuals.expansion),
        rounding,
        visuals.bg_fill,
        visuals.bg_stroke,
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
    ui.painter().galley(text_pos.min, galley, visuals.text_color());
}

pub(super) fn render_top_panel(app: &mut FlistWalkerApp, ctx: &egui::Context) {
    egui::TopBottomPanel::top("top").show(ctx, |ui| {
        app.render_tab_bar(ui);
        ui.separator();
        ui.horizontal(|ui| {
            let row_height = ui.spacing().interact_size.y;
            ui.add_sized([44.0, row_height], egui::Label::new("Root:"));
            let button_width = 96.0;
            let set_default_width = 130.0;
            let add_width = 100.0;
            let remove_width = 130.0;
            let field_width = (ui.available_width()
                - button_width
                - add_width
                - set_default_width
                - remove_width
                - (ui.spacing().item_spacing.x * 4.0))
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
                    let below = egui::AboveOrBelow::Below;
                    egui::popup::popup_above_or_below_widget(
                        ui,
                        popup_id,
                        &response,
                        below,
                        egui::popup::PopupCloseBehavior::CloseOnClickOutside,
                        |ui: &mut egui::Ui| {
                            ui.set_min_width(field_width);
                            for (index, path) in app
                                .shell
                                .features
                                .root_browser
                                .saved_roots()
                                .iter()
                                .enumerate()
                            {
                                let text = normalize_windows_path_buf(path.clone())
                                    .to_string_lossy()
                                    .to_string();
                                let is_selected = app.shell.ui.root_dropdown_highlight() == Some(index);
                                if ui.selectable_label(is_selected, text).clicked() {
                                    next_root = Some(path.clone());
                                }
                            }
                        },
                    );
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
                    "Disabled while FLISTWALKER_RESTORE_TABS is enabled",
                );
            }
            if set_default_enabled && set_default_clicked {
                app.set_current_root_as_default();
            }
            if ui
                .add_sized([add_width, row_height], egui::Button::new("Add to list"))
                .clicked()
            {
                app.add_current_root_to_saved();
            }
            if ui
                .add_sized(
                    [remove_width, row_height],
                    egui::Button::new("Remove from list"),
                )
                .clicked()
            {
                app.remove_current_root_from_saved();
            }
            if let Some(root) = next_root {
                app.close_root_dropdown(ui.ctx());
                app.apply_root_change(root);
            }
        });

        ui.horizontal(|ui| {
            let use_filelist_changed = ui
                .checkbox(&mut app.shell.runtime.use_filelist, "Use FileList")
                .changed();
            if ui.checkbox(&mut app.shell.runtime.use_regex, "Regex").changed() {
                app.invalidate_result_sort(true);
                app.update_results();
            }
            if ui
                .checkbox(&mut app.shell.runtime.ignore_case, "Ignore Case")
                .changed()
            {
                app.invalidate_result_sort(true);
                app.update_results();
            }
            let (files_changed, dirs_changed) = if app.use_filelist_requires_locked_filters() {
                let mut forced_changed = false;
                if !app.shell.runtime.include_files || !app.shell.runtime.include_dirs {
                    app.shell.runtime.include_files = true;
                    app.shell.runtime.include_dirs = true;
                    forced_changed = true;
                }
                ui.add_enabled(false, egui::Checkbox::new(&mut app.shell.runtime.include_files, "Files"));
                ui.add_enabled(
                    false,
                    egui::Checkbox::new(&mut app.shell.runtime.include_dirs, "Folders"),
                );
                (forced_changed, forced_changed)
            } else {
                (
                    ui.checkbox(&mut app.shell.runtime.include_files, "Files").changed(),
                    ui.checkbox(&mut app.shell.runtime.include_dirs, "Folders").changed(),
                )
            };
            let mut show_preview = app.shell.ui.show_preview();
            if ui.checkbox(&mut show_preview, "Preview").changed() {
                app.shell.ui.set_show_preview(show_preview);
                if !show_preview {
                    app.clear_preview_cache();
                }
                app.mark_ui_state_dirty();
                app.persist_ui_state_now();
            }
            ui.separator();
            ui.label(app.source_text());
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
        let _ = output.response.clone().on_hover_ui_at_pointer(|ui| {
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
        let events = ctx.input(|i| i.events.clone());
        if !editing_history_search {
            let (query_event_changed, query_cursor_after_fallback) = app.process_query_input_events(
                ctx,
                &events,
                output.response.has_focus(),
                output.response.changed(),
                output.state.cursor.char_range(),
            );
            if query_event_changed {
                app.mark_query_edited();
                if output.response.has_focus() {
                    let end = query_cursor_after_fallback.unwrap_or_else(|| {
                        FlistWalkerApp::char_count(&app.shell.runtime.query_state.query)
                    });
                    output
                        .state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(
                            egui::text::CCursor::new(end),
                        )));
                    output.state.clone().store(ctx, output.response.id);
                }
                app.update_results();
            }
            if app.apply_emacs_query_shortcuts(ctx, &mut output) {
                app.mark_query_edited();
                app.update_results();
            }
            if output.response.changed() {
                let normalized =
                    FlistWalkerApp::normalize_singleline_input(&mut app.shell.runtime.query_state.query);
                if normalized && output.response.has_focus() {
                    let end = FlistWalkerApp::char_count(&app.shell.runtime.query_state.query);
                    output
                        .state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(
                            egui::text::CCursor::new(end),
                        )));
                    output.state.clone().store(ctx, output.response.id);
                }
                app.mark_query_edited();
                FlistWalkerApp::append_window_trace(
                    "query_text_changed",
                    &format!(
                        "chars={} has_half_space={} has_full_space={}",
                        app.shell.runtime.query_state.query.chars().count(),
                        app.shell.runtime.query_state.query.contains(' '),
                        app.shell.runtime.query_state.query.contains('\u{3000}')
                    ),
                );
                app.update_results();
            }
        } else if output.response.changed() {
            if FlistWalkerApp::normalize_singleline_input(
                &mut app.shell.runtime.query_state.history_search_query,
            ) && output.response.has_focus()
            {
                let end =
                    FlistWalkerApp::char_count(&app.shell.runtime.query_state.history_search_query);
                output
                    .state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::one(
                        egui::text::CCursor::new(end),
                    )));
                output.state.clone().store(ctx, output.response.id);
            }
            app.refresh_history_search_results();
        }
        app.run_deferred_shortcuts(ctx);

        ui.horizontal(|ui| {
            for label in app.top_action_labels() {
                if !ui.button(label).clicked() {
                    continue;
                }
                if let Some(command) = FlistWalkerApp::top_action_command(label) {
                    app.queue_render_command(super::render::RenderCommand::TopAction(command));
                }
            }
        });
    });
}

pub(super) fn render_status_panel(app: &mut FlistWalkerApp, ctx: &egui::Context) {
    egui::TopBottomPanel::bottom("status")
        .resizable(false)
        .exact_height(24.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                let version_text = format!("v{}", env!("CARGO_PKG_VERSION"));
                let version_font = egui::TextStyle::Body.resolve(ui.style());
                let version_width = ui.fonts(|fonts| {
                    fonts
                        .layout_no_wrap(
                            version_text.clone(),
                            version_font.clone(),
                            ui.visuals().text_color(),
                        )
                        .size()
                        .x
                });
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

pub(super) fn render_central_panel(app: &mut FlistWalkerApp, ctx: &egui::Context) {
    egui::CentralPanel::default().show(ctx, |ui| {
        app.render_results_and_preview(ui);
    });
}

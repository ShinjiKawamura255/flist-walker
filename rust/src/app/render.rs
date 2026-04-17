use super::{
    display_path_with_mode, render_tabs, EntryDisplayKind, EntryKind, FileListDialogKind,
    FlistWalkerApp, ResultSortMode, TabAccentColor, UpdateSupport,
};
use eframe::egui;
use std::path::{Path, PathBuf};
use crate::path_utils::normalize_windows_path_buf;

// Render command surface. Render.rs stays focused on drawing and input
// collection while FlistWalkerApp dispatches the resulting commands.
#[derive(Clone, Copy)]
pub(super) enum RenderTopActionCommand {
    ApplyHistory,
    CancelHistorySearch,
    ExecuteSelected,
    CopySelectedPaths,
    ClearPinned,
    CreateFileList,
    RefreshIndex,
}

#[derive(Clone, Copy)]
pub(super) enum RenderFileListDialogCommand {
    ConfirmOverwrite,
    CancelOverwrite,
    ConfirmAncestorPropagation,
    SkipAncestorPropagation,
    CancelAncestorConfirmation,
    ConfirmUseWalker,
    CancelUseWalker,
}

#[derive(Clone, Copy)]
pub(super) enum RenderUpdateDialogCommand {
    StartInstall,
    SkipPromptUntilNextVersion,
    DismissPrompt,
    SuppressCheckFailures,
    DismissCheckFailure,
}

#[derive(Clone, Copy)]
pub(super) enum RenderTabBarCommand {
    CreateNewTab,
    CloseTab(usize),
    MoveTab {
        from_index: usize,
        to_index: usize,
    },
    SwitchToTab(usize),
    ClearTabAccent(usize),
    SetTabAccent {
        index: usize,
        accent: TabAccentColor,
    },
}

#[derive(Clone, Copy)]
pub(super) enum RenderCommand {
    TopAction(RenderTopActionCommand),
    FileListDialog(RenderFileListDialogCommand),
    UpdateDialog(RenderUpdateDialogCommand),
    TabBar(RenderTabBarCommand),
}

impl FlistWalkerApp {
    const RESULT_SORT_SELECTOR_WIDTH: f32 = 132.0;
    const RESULT_ROW_H_MARGIN: f32 = 3.0;
    const RESULT_ROW_V_MARGIN: f32 = 2.0;
    const RESULT_ROW_ROUNDING: f32 = 3.0;
    pub(super) const TAB_ROUNDING: f32 = 4.0;
    pub(super) const TAB_ACCENT_GLOW_HEIGHT: f32 = 8.0;
    pub(super) const TAB_ACCENT_LINE_HEIGHT: f32 = 3.0;
    pub(super) const TAB_ACTIVE_BORDER_WIDTH: f32 = 2.0;
    pub(super) const TAB_INACTIVE_BORDER_WIDTH: f32 = 1.0;

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
        ui.painter()
            .galley(text_pos.min, galley, visuals.text_color());
    }

    pub(super) fn filelist_use_walker_dialog_lines() -> [&'static str; 2] {
        [
            "Use FileList が有効です。Create File List には Walker indexing が必要です。",
            "FileList インデックスからは生成せず、現在のタブの裏で一時的に Walker を実行します。続行しますか？",
        ]
    }

    fn dialog_button(&self, ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
        let mut button = egui::Button::new(label);
        if selected {
            button = button.fill(if ui.visuals().dark_mode {
                egui::Color32::from_rgb(48, 53, 62)
            } else {
                egui::Color32::from_rgb(228, 232, 238)
            });
        }
        ui.add(button)
    }

    pub(super) fn top_action_labels(&self) -> Vec<&'static str> {
        if self.shell.runtime.query_state.history_search_active {
            return vec!["Apply History", "Cancel History Search"];
        }

        let create_label = if self.shell.features.filelist.workflow.in_progress {
            "Create File List (Running...)"
        } else {
            "Create File List"
        };
        vec![
            "Open / Execute",
            "Copy Path(s)",
            "Clear Selected",
            create_label,
            "Refresh Index",
        ]
    }

    fn top_action_command(label: &str) -> Option<RenderTopActionCommand> {
        match label {
            "Apply History" => Some(RenderTopActionCommand::ApplyHistory),
            "Cancel History Search" => Some(RenderTopActionCommand::CancelHistorySearch),
            "Open / Execute" => Some(RenderTopActionCommand::ExecuteSelected),
            "Copy Path(s)" => Some(RenderTopActionCommand::CopySelectedPaths),
            "Clear Selected" => Some(RenderTopActionCommand::ClearPinned),
            "Create File List" | "Create File List (Running...)" => {
                Some(RenderTopActionCommand::CreateFileList)
            }
            "Refresh Index" => Some(RenderTopActionCommand::RefreshIndex),
            _ => None,
        }
    }

    pub(super) fn schedule_frame_repaint(&mut self, ctx: &egui::Context) {
        let memory_elapsed = self.shell.ui.last_memory_sample.elapsed();
        if memory_elapsed >= Self::MEMORY_SAMPLE_INTERVAL {
            self.refresh_status_line();
        } else {
            ctx.request_repaint_after(Self::MEMORY_SAMPLE_INTERVAL - memory_elapsed);
        }
        if self.shell.search.in_progress()
            || self.shell.indexing.in_progress
            || self.shell.worker_bus.preview.in_progress
            || self.shell.worker_bus.action.in_progress
            || self.shell.worker_bus.sort.in_progress
            || self.shell.indexing.kind_resolution_in_progress
            || self.shell.features.filelist.workflow.in_progress
            || self.shell.features.update.state.in_progress
            || self.any_tab_async_in_progress()
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
    }

    pub(super) fn run_ui_frame(&mut self, ctx: &egui::Context) {
        self.capture_window_geometry(ctx);
        self.apply_stable_window_geometry(false);
        // Handle app shortcuts before widget rendering so Tab is not consumed by egui focus traversal.
        self.handle_shortcuts(ctx);

        self.render_top_panel(ctx);
        self.render_status_panel(ctx);
        self.render_filelist_dialogs(ctx);
        self.render_update_dialog(ctx);
        self.render_central_panel(ctx);
        self.dispatch_render_commands(ctx);
        self.maybe_save_ui_state(false);
    }

    pub(super) fn render_results_and_preview(&mut self, ui: &mut egui::Ui) {
        if self.shell.runtime.query_state.history_search_active {
            self.shell.ui.preview_resize_in_progress = false;
            self.render_history_search_results(ui);
            self.clear_scroll_to_current();
            return;
        }
        if self.shell.ui.show_preview {
            let max_preview_width = (ui.available_width() - Self::MIN_RESULTS_PANEL_WIDTH)
                .max(Self::MIN_PREVIEW_PANEL_WIDTH);
            let panel = egui::SidePanel::right("preview-panel")
                .resizable(true)
                .default_width(self.shell.ui.preview_panel_width.min(max_preview_width))
                .min_width(Self::MIN_PREVIEW_PANEL_WIDTH)
                .max_width(max_preview_width);
            let response = panel.show_inside(ui, |ui| {
                ui.heading("Preview");
                let preview_width = ui.available_width();
                let preview_height = ui.available_height();
                ui.allocate_ui_with_layout(
                    egui::vec2(preview_width, preview_height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        let frame_fill = ui.visuals().extreme_bg_color;
                        egui::Frame::none().fill(frame_fill).show(ui, |ui| {
                            ui.set_min_size(egui::vec2(preview_width, preview_height));
                            egui::ScrollArea::both()
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.add_sized(
                                        egui::vec2(preview_width, preview_height),
                                        egui::TextEdit::multiline(&mut self.shell.runtime.preview)
                                            .interactive(false)
                                            .font(egui::TextStyle::Monospace)
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
                .max(Self::MIN_PREVIEW_PANEL_WIDTH);
            if (new_width - self.shell.ui.preview_panel_width).abs() > 1.0 {
                self.shell.ui.preview_panel_width = new_width;
                self.mark_ui_state_dirty();
            }
            let splitter_x = response.response.rect.left();
            let splitter_pressed = ui.input(|i| {
                let Some(pos) = i.pointer.interact_pos() else {
                    return false;
                };
                i.pointer.primary_down() && (pos.x - splitter_x).abs() <= 8.0
            });
            self.shell.ui.preview_resize_in_progress =
                response.response.dragged() || splitter_pressed;
            self.render_results_list(ui);
        } else {
            self.shell.ui.preview_resize_in_progress = false;
            self.render_results_list(ui);
        }
        self.clear_scroll_to_current();
    }

    pub(super) fn results_scroll_enabled(preview_resize_in_progress: bool) -> bool {
        !preview_resize_in_progress
    }

    pub(super) fn render_results_list(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Results");
            let row_height = ui.spacing().interact_size.y;
            let row_width = ui.available_width();
            ui.allocate_ui_with_layout(
                egui::vec2(row_width, row_height),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    let mut selected = self.shell.runtime.result_sort_mode;
                    egui::ComboBox::from_id_salt("results-sort-selector")
                        .width(Self::RESULT_SORT_SELECTOR_WIDTH)
                        .selected_text(selected.label())
                        .show_ui(ui, |ui| {
                            ui.set_min_width(Self::RESULT_SORT_SELECTOR_WIDTH);
                            for mode in [
                                ResultSortMode::Score,
                                ResultSortMode::NameAsc,
                                ResultSortMode::NameDesc,
                                ResultSortMode::ModifiedDesc,
                                ResultSortMode::ModifiedAsc,
                                ResultSortMode::CreatedDesc,
                                ResultSortMode::CreatedAsc,
                            ] {
                                ui.selectable_value(&mut selected, mode, mode.label());
                            }
                        });
                    ui.label("Sorted by");
                    if selected != self.shell.runtime.result_sort_mode {
                        self.set_result_sort_mode(selected);
                    }
                },
            );
        });
        let scroll_enabled = Self::results_scroll_enabled(self.shell.ui.preview_resize_in_progress);
        egui::ScrollArea::both()
            .enable_scrolling(scroll_enabled)
            .drag_to_scroll(false)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut clicked_row: Option<usize> = None;
                let mut execute_row: Option<usize> = None;
                let prefer_relative = self.prefer_relative_display();
                self.ensure_highlight_cache_scope(prefer_relative);
                let clip_rect = ui.clip_rect();
                let row_width = ui.available_width().max(0.0);
                let row_height = Self::result_row_height(ui);

                for i in 0..self.shell.runtime.results.len() {
                    let Some((path, _score)) = self.shell.runtime.results.get(i) else {
                        continue;
                    };
                    let path = path.clone();
                    let is_current = self.shell.runtime.current_row == Some(i);
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(row_width, row_height),
                        egui::Sense::click(),
                    );
                    if is_current && self.shell.ui.scroll_to_current {
                        ui.scroll_to_rect(rect, None);
                    }
                    if clip_rect.intersects(rect) {
                        self.render_result_row(ui, rect, &path, is_current, prefer_relative);
                    }
                    if response.clicked() {
                        clicked_row = Some(i);
                    }
                    if response.double_clicked() {
                        execute_row = Some(i);
                    }
                }
                if let Some(i) = clicked_row {
                    self.set_current_row(Some(i));
                    self.request_preview_for_current();
                    self.refresh_status_line();
                }
                if let Some(i) = execute_row {
                    self.set_current_row(Some(i));
                    let open_parent_for_files = ui.input(|i| i.modifiers.shift);
                    self.execute_selected_for_activation(open_parent_for_files);
                }
            });
    }

    fn result_row_height(ui: &egui::Ui) -> f32 {
        ui.text_style_height(&egui::TextStyle::Body) + (Self::RESULT_ROW_V_MARGIN * 2.0)
    }

    pub(super) fn result_row_text_pos(
        inner_rect: egui::Rect,
        galley_size: egui::Vec2,
    ) -> egui::Pos2 {
        egui::pos2(
            inner_rect.left(),
            inner_rect.center().y - (galley_size.y * 0.5),
        )
    }

    fn render_result_row(
        &mut self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        path: &Path,
        is_current: bool,
        prefer_relative: bool,
    ) {
        let is_pinned = self.shell.runtime.pinned_paths.contains(path);
        let kind = self.find_entry_kind(path);
        let display = display_path_with_mode(path, &self.shell.runtime.root, prefer_relative);
        let positions = self.highlight_positions_for_path_cached(path, prefer_relative);
        let job = self.build_result_row_job(
            ui,
            &display,
            positions.as_slice(),
            is_current,
            is_pinned,
            kind,
        );
        let selected_bg = if ui.visuals().dark_mode {
            egui::Color32::from_rgb(48, 53, 62)
        } else {
            egui::Color32::from_rgb(228, 232, 238)
        };
        if is_current {
            ui.painter().rect_filled(
                rect,
                egui::Rounding::same(Self::RESULT_ROW_ROUNDING),
                selected_bg,
            );
        }

        let inner_rect = rect.shrink2(egui::vec2(
            Self::RESULT_ROW_H_MARGIN,
            Self::RESULT_ROW_V_MARGIN,
        ));
        let galley = ui.painter().layout_job(job);
        let text_pos = Self::result_row_text_pos(inner_rect, galley.size());
        ui.painter()
            .galley(text_pos, galley, ui.visuals().text_color());
    }

    fn build_result_row_job(
        &self,
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
            Some(EntryDisplayKind::Dir) => ("DIR ", egui::Color32::from_rgb(52, 211, 153)),
            Some(EntryDisplayKind::File) => ("FILE", egui::Color32::from_rgb(96, 165, 250)),
            Some(EntryDisplayKind::Link) => ("LINK", egui::Color32::from_rgb(250, 204, 21)),
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
            let color = if Self::is_highlighted_position(positions, idx) {
                egui::Color32::from_rgb(245, 158, 11)
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

    pub(super) fn render_history_search_results(&mut self, ui: &mut egui::Ui) {
        ui.heading("History Results");
        ui.label(format!(
            "{} items in history, {} matches",
            self.shell.runtime.query_state.query_history.len(),
            self.shell.runtime.query_state.history_search_results.len()
        ));
        egui::ScrollArea::vertical()
            .drag_to_scroll(false)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut clicked_row: Option<usize> = None;
                let mut accept_row: Option<usize> = None;

                for (index, entry) in self
                    .shell
                    .runtime
                    .query_state
                    .history_search_results
                    .iter()
                    .enumerate()
                {
                    let is_current =
                        self.shell.runtime.query_state.history_search_current == Some(index);
                    let prefix = if is_current { "▶" } else { "·" };
                    let text = format!("{prefix} {entry}");
                    let selected_bg = if ui.visuals().dark_mode {
                        egui::Color32::from_rgb(48, 53, 62)
                    } else {
                        egui::Color32::from_rgb(228, 232, 238)
                    };
                    let fill = if is_current {
                        selected_bg
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    let row = egui::Frame::none()
                        .fill(fill)
                        .inner_margin(egui::Margin::symmetric(3.0, 2.0))
                        .rounding(egui::Rounding::same(3.0))
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
                    self.shell.runtime.query_state.history_search_current = Some(index);
                }
                if let Some(index) = accept_row {
                    self.shell.runtime.query_state.history_search_current = Some(index);
                    self.accept_history_search();
                }
            });
    }

    pub(super) fn render_tab_bar(&mut self, ui: &mut egui::Ui) {
        render_tabs::render_tab_bar(self, ui);
    }

    pub(super) fn render_top_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            self.render_tab_bar(ui);
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
                let selected_text = self.root_display_text();
                let mut next_root: Option<PathBuf> = None;
                ui.allocate_ui_with_layout(
                    egui::vec2(field_width, row_height),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        self.sync_root_dropdown_highlight();
                        let popup_open = self.is_root_dropdown_open(ui.ctx());
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(field_width, row_height),
                            egui::Sense::click(),
                        );
                        Self::paint_root_selector_button(
                            ui,
                            rect,
                            &response,
                            &selected_text,
                            popup_open,
                        );
                        if response.clicked() {
                            if popup_open {
                                self.close_root_dropdown(ui.ctx());
                            } else {
                                self.open_root_dropdown(ui.ctx());
                            }
                        }
                        let popup_id = Self::root_selector_popup_id();
                        let below = egui::AboveOrBelow::Below;
                        egui::popup::popup_above_or_below_widget(
                            ui,
                            popup_id,
                            &response,
                            below,
                            egui::popup::PopupCloseBehavior::CloseOnClickOutside,
                            |ui: &mut egui::Ui| {
                                ui.set_min_width(field_width);
                                for (index, path) in
                                    self.shell.features.root_browser.saved_roots.iter().enumerate()
                                {
                                    let text = normalize_windows_path_buf(path.clone())
                                        .to_string_lossy()
                                        .to_string();
                                    let is_selected =
                                        self.shell.ui.root_dropdown_highlight == Some(index);
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
                    self.browse_for_root();
                }
                let set_default_enabled = self.can_set_current_root_as_default();
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
                    self.set_current_root_as_default();
                }
                if ui
                    .add_sized([add_width, row_height], egui::Button::new("Add to list"))
                    .clicked()
                {
                    self.add_current_root_to_saved();
                }
                if ui
                    .add_sized(
                        [remove_width, row_height],
                        egui::Button::new("Remove from list"),
                    )
                    .clicked()
                {
                    self.remove_current_root_from_saved();
                }
                if let Some(root) = next_root {
                    self.close_root_dropdown(ui.ctx());
                    self.apply_root_change(root);
                }
            });

            ui.horizontal(|ui| {
                let use_filelist_changed = ui
                    .checkbox(&mut self.shell.runtime.use_filelist, "Use FileList")
                    .changed();
                if ui.checkbox(&mut self.shell.runtime.use_regex, "Regex").changed() {
                    self.invalidate_result_sort(true);
                    self.update_results();
                }
                if ui.checkbox(&mut self.shell.runtime.ignore_case, "Ignore Case").changed() {
                    self.invalidate_result_sort(true);
                    self.update_results();
                }
                let (files_changed, dirs_changed) = if self.use_filelist_requires_locked_filters() {
                    let mut forced_changed = false;
                    if !self.shell.runtime.include_files || !self.shell.runtime.include_dirs {
                        self.shell.runtime.include_files = true;
                        self.shell.runtime.include_dirs = true;
                        forced_changed = true;
                    }
                    ui.add_enabled(false, egui::Checkbox::new(&mut self.shell.runtime.include_files, "Files"));
                    ui.add_enabled(
                        false,
                        egui::Checkbox::new(&mut self.shell.runtime.include_dirs, "Folders"),
                    );
                    (forced_changed, forced_changed)
                } else {
                    (
                        ui.checkbox(&mut self.shell.runtime.include_files, "Files").changed(),
                        ui.checkbox(&mut self.shell.runtime.include_dirs, "Folders").changed(),
                    )
                };
                if ui.checkbox(&mut self.shell.ui.show_preview, "Preview").changed() {
                    if !self.shell.ui.show_preview {
                        self.clear_preview_cache();
                    }
                    self.mark_ui_state_dirty();
                    self.persist_ui_state_now();
                }
                ui.separator();
                ui.label(self.source_text());
                self.maybe_reindex_from_filter_toggles(
                    use_filelist_changed,
                    files_changed,
                    dirs_changed,
                );
            });

            if self.shell.runtime.query_state.history_search_active {
                ui.label(
                    egui::RichText::new("History Search")
                        .strong()
                        .color(ui.visuals().strong_text_color()),
                );
            }
            let editing_history_search = self.shell.runtime.query_state.history_search_active;
            let query_input_id = self.shell.ui.query_input_id;
            let mut output = egui::TextEdit::singleline(if editing_history_search {
                &mut self.shell.runtime.query_state.history_search_query
            } else {
                &mut self.shell.runtime.query_state.query
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
                    ui.label(Self::SEARCH_HINTS_TOOLTIP);
                }
            });
            if self.shell.ui.focus_query_requested {
                output.response.request_focus();
                self.clear_focus_query_request();
            }
            if self.shell.ui.unfocus_query_requested {
                output.response.surrender_focus();
                self.clear_unfocus_query_request();
            }
            let events = ctx.input(|i| i.events.clone());
            if !editing_history_search {
                let (query_event_changed, query_cursor_after_fallback) = self
                    .process_query_input_events(
                        ctx,
                        &events,
                        output.response.has_focus(),
                        output.response.changed(),
                        output.state.cursor.char_range(),
                    );
                if query_event_changed {
                    self.mark_query_edited();
                    if output.response.has_focus() {
                        let end = query_cursor_after_fallback
                            .unwrap_or_else(|| Self::char_count(&self.shell.runtime.query_state.query));
                        output
                            .state
                            .cursor
                            .set_char_range(Some(egui::text::CCursorRange::one(
                                egui::text::CCursor::new(end),
                            )));
                        output.state.clone().store(ctx, output.response.id);
                    }
                    self.update_results();
                }
                if self.apply_emacs_query_shortcuts(ctx, &mut output) {
                    self.mark_query_edited();
                    self.update_results();
                }
                if output.response.changed() {
                    let normalized = Self::normalize_singleline_input(&mut self.shell.runtime.query_state.query);
                    if normalized && output.response.has_focus() {
                        let end = Self::char_count(&self.shell.runtime.query_state.query);
                        output
                            .state
                            .cursor
                            .set_char_range(Some(egui::text::CCursorRange::one(
                                egui::text::CCursor::new(end),
                            )));
                        output.state.clone().store(ctx, output.response.id);
                    }
                    self.mark_query_edited();
                    Self::append_window_trace(
                        "query_text_changed",
                        &format!(
                            "chars={} has_half_space={} has_full_space={}",
                            self.shell.runtime.query_state.query.chars().count(),
                            self.shell.runtime.query_state.query.contains(' '),
                            self.shell.runtime.query_state.query.contains('\u{3000}')
                        ),
                    );
                    self.update_results();
                }
            } else if output.response.changed() {
                if Self::normalize_singleline_input(&mut self.shell.runtime.query_state.history_search_query)
                    && output.response.has_focus()
                {
                    let end = Self::char_count(&self.shell.runtime.query_state.history_search_query);
                    output
                        .state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(
                            egui::text::CCursor::new(end),
                        )));
                    output.state.clone().store(ctx, output.response.id);
                }
                self.refresh_history_search_results();
            }
            self.run_deferred_shortcuts(ctx);

            ui.horizontal(|ui| {
                for label in self.top_action_labels() {
                    if !ui.button(label).clicked() {
                        continue;
                    }
                    if let Some(command) = Self::top_action_command(label) {
                        self.queue_render_command(RenderCommand::TopAction(command));
                    }
                }
            });
        });
    }

    pub(super) fn render_status_panel(&mut self, ctx: &egui::Context) {
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
                    if let Some(label) = self.action_progress_label() {
                        ui.add(egui::Spinner::new().size(14.0));
                        ui.label(label);
                        ui.separator();
                    }
                    if self.can_cancel_create_filelist() {
                        let cancel_label = if self.shell.features.filelist.workflow.cancel_requested {
                            "Canceling FileList..."
                        } else {
                            "Cancel Create File List"
                        };
                        if ui
                            .add_enabled(
                                !self.shell.features.filelist.workflow.cancel_requested,
                                egui::Button::new(cancel_label),
                            )
                            .clicked()
                        {
                            self.cancel_create_filelist();
                        }
                        ui.separator();
                    }
                    let reserved_width =
                        version_width + ui.spacing().item_spacing.x + ui.spacing().icon_width;
                    let status_width = (ui.available_width() - reserved_width).max(0.0);
                    let status_line = self.status_line_text();
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

    pub(super) fn render_filelist_dialogs(&mut self, ctx: &egui::Context) {
        let mut overwrite = false;
        let mut cancel_overwrite = false;
        let current_tab_id = self.current_tab_id().unwrap_or_default();
        if let Some(existing_path) = self
            .shell
            .features
            .filelist
            .workflow.pending_confirmation
            .as_ref()
            .filter(|pending| pending.tab_id == current_tab_id)
            .map(|pending| pending.existing_path.clone())
        {
            self.sync_filelist_dialog_selection(FileListDialogKind::Overwrite);
            egui::Window::new("Overwrite FileList?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.label(format!(
                        "{} already exists. Overwrite it?",
                        existing_path.display()
                    ));
                    ui.horizontal(|ui| {
                        if self
                            .dialog_button(
                                ui,
                                "Overwrite",
                                self.shell.features.filelist.workflow.active_dialog_button == 0,
                            )
                            .clicked()
                        {
                            overwrite = true;
                        }
                        if self
                            .dialog_button(
                                ui,
                                "Cancel",
                                self.shell.features.filelist.workflow.active_dialog_button == 1,
                            )
                            .clicked()
                        {
                            cancel_overwrite = true;
                        }
                    });
                });
        }
        if overwrite {
            self.queue_render_command(RenderCommand::FileListDialog(
                RenderFileListDialogCommand::ConfirmOverwrite,
            ));
        } else if cancel_overwrite {
            self.queue_render_command(RenderCommand::FileListDialog(
                RenderFileListDialogCommand::CancelOverwrite,
            ));
        }

        let mut confirm_ancestor = false;
        let mut current_root_only = false;
        let mut cancel_ancestor = false;
        if self
            .shell
            .features
            .filelist
            .workflow.pending_ancestor_confirmation
            .as_ref()
            .is_some_and(|pending| pending.tab_id == current_tab_id)
        {
            self.sync_filelist_dialog_selection(FileListDialogKind::Ancestor);
            egui::Window::new("Update Ancestor FileLists?")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                    .show(ctx, |ui| {
                        ui.label("親ディレクトリ直下の既存 FileList にも参照を追記します。");
                        ui.label(
                            "Continue は祖先も更新し、Current Root Only は現在 root の FileList だけを作成します。",
                        );
                        ui.horizontal(|ui| {
                            if self
                                .dialog_button(
                                    ui,
                                    "Continue",
                                    self.shell.features.filelist.workflow.active_dialog_button == 0,
                                )
                                .clicked()
                            {
                                confirm_ancestor = true;
                            }
                            if self
                                .dialog_button(
                                    ui,
                                    "Current Root Only",
                                    self.shell.features.filelist.workflow.active_dialog_button == 1,
                                )
                                .clicked()
                            {
                                current_root_only = true;
                            }
                            if self
                                .dialog_button(
                                    ui,
                                    "Cancel",
                                    self.shell.features.filelist.workflow.active_dialog_button == 2,
                                )
                                .clicked()
                            {
                                cancel_ancestor = true;
                            }
                        });
                    });
        }
        if confirm_ancestor {
            self.queue_render_command(RenderCommand::FileListDialog(
                RenderFileListDialogCommand::ConfirmAncestorPropagation,
            ));
        } else if current_root_only {
            self.queue_render_command(RenderCommand::FileListDialog(
                RenderFileListDialogCommand::SkipAncestorPropagation,
            ));
        } else if cancel_ancestor {
            self.queue_render_command(RenderCommand::FileListDialog(
                RenderFileListDialogCommand::CancelAncestorConfirmation,
            ));
        }

        let mut confirm_walker = false;
        let mut cancel_walker = false;
        if self
            .shell
            .features
            .filelist
            .workflow.pending_use_walker_confirmation
            .as_ref()
            .is_some_and(|pending| pending.source_tab_id == current_tab_id)
        {
            let [line1, line2] = Self::filelist_use_walker_dialog_lines();
            self.sync_filelist_dialog_selection(FileListDialogKind::UseWalker);
            egui::Window::new("Create File List?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.label(line1);
                    ui.label(line2);
                    ui.horizontal(|ui| {
                        if self
                            .dialog_button(
                                ui,
                                "Continue",
                                self.shell.features.filelist.workflow.active_dialog_button == 0,
                            )
                            .clicked()
                        {
                            confirm_walker = true;
                        }
                        if self
                            .dialog_button(
                                ui,
                                "Cancel",
                                self.shell.features.filelist.workflow.active_dialog_button == 1,
                            )
                            .clicked()
                        {
                            cancel_walker = true;
                        }
                    });
                });
        }
        if confirm_walker {
            self.queue_render_command(RenderCommand::FileListDialog(
                RenderFileListDialogCommand::ConfirmUseWalker,
            ));
        } else if cancel_walker {
            self.queue_render_command(RenderCommand::FileListDialog(
                RenderFileListDialogCommand::CancelUseWalker,
            ));
        }
        if self.current_filelist_dialog_kind().is_none() {
            self.clear_filelist_dialog_selection();
        }
    }

    pub(super) fn render_update_dialog(&mut self, ctx: &egui::Context) {
        if let Some(prompt) = self.shell.features.update.state.prompt.as_ref().cloned() {
            let mut confirm = false;
            let mut later = false;
            let mut skip_until_next_version = prompt.skip_until_next_version;
            egui::Window::new("Update Available")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.label(format!(
                        "FlistWalker {} is available. Current version is {}.",
                        prompt.candidate.target_version, prompt.candidate.current_version
                    ));
                    match &prompt.candidate.support {
                        UpdateSupport::Auto => {
                            ui.label(
                                "Download the new release, replace the current binary, and restart?",
                            );
                            if prompt.install_started {
                                ui.label("Downloading update... please wait.");
                            }
                            ui.checkbox(
                                &mut skip_until_next_version,
                                "Don't show again until the next version",
                            );
                            ui.horizontal(|ui| {
                                if ui
                                    .add_enabled(
                                        !prompt.install_started,
                                        egui::Button::new("Download and Restart"),
                                    )
                                    .clicked()
                                {
                                    confirm = true;
                                }
                                if ui
                                    .add_enabled(
                                        !prompt.install_started,
                                        egui::Button::new("Later"),
                                    )
                                    .clicked()
                                {
                                    later = true;
                                }
                            });
                        }
                        UpdateSupport::ManualOnly { message } => {
                            ui.label(message);
                            ui.label(format!("Release: {}", prompt.candidate.release_url));
                            ui.checkbox(
                                &mut skip_until_next_version,
                                "Don't show again until the next version",
                            );
                            if ui.button("Later").clicked() {
                                later = true;
                            }
                        }
                    }
                });

            self.shell
                .features
                .update
                .set_prompt_skip_until_next_version(skip_until_next_version);

            if confirm {
                self.queue_render_command(RenderCommand::UpdateDialog(
                    RenderUpdateDialogCommand::StartInstall,
                ));
            } else if later {
                if skip_until_next_version {
                    self.queue_render_command(RenderCommand::UpdateDialog(
                        RenderUpdateDialogCommand::SkipPromptUntilNextVersion,
                    ));
                } else {
                    self.queue_render_command(RenderCommand::UpdateDialog(
                        RenderUpdateDialogCommand::DismissPrompt,
                    ));
                }
            }
        }

        if let Some(failure) = self.shell.features.update.state.check_failure.as_ref().cloned() {
            let mut close = false;
            let mut suppress_future_errors = failure.suppress_future_errors;
            egui::Window::new("Update Check Failed")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 88.0))
                .show(ctx, |ui| {
                    ui.label("FlistWalker couldn't check for updates right now.");
                    ui.label("You can keep using the app as usual and try again later.");
                    ui.add_space(6.0);
                    ui.separator();
                    ui.label("Details");
                    ui.monospace(&failure.error);
                    ui.add_space(6.0);
                    ui.checkbox(
                        &mut suppress_future_errors,
                        "Don't show this again for update check errors",
                    );
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                });

            self.shell
                .features
                .update
                .set_check_failure_suppress_future_errors(suppress_future_errors);

            if close {
                if suppress_future_errors {
                    self.queue_render_command(RenderCommand::UpdateDialog(
                        RenderUpdateDialogCommand::SuppressCheckFailures,
                    ));
                } else {
                    self.queue_render_command(RenderCommand::UpdateDialog(
                        RenderUpdateDialogCommand::DismissCheckFailure,
                    ));
                }
            }
        }
    }

    pub(super) fn render_central_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_results_and_preview(ui);
        });
    }

    #[allow(dead_code)]
    pub(super) fn queue_render_command(&mut self, command: RenderCommand) {
        self.shell.ui.pending_render_commands.push(command);
    }

    pub(super) fn dispatch_render_commands(&mut self, ctx: &egui::Context) {
        let commands = std::mem::take(&mut self.shell.ui.pending_render_commands);
        for command in commands {
            match command {
                RenderCommand::TopAction(RenderTopActionCommand::ApplyHistory) => {
                    self.accept_history_search();
                }
                RenderCommand::TopAction(RenderTopActionCommand::CancelHistorySearch) => {
                    self.cancel_history_search();
                }
                RenderCommand::TopAction(RenderTopActionCommand::ExecuteSelected) => {
                    self.execute_selected();
                }
                RenderCommand::TopAction(RenderTopActionCommand::CopySelectedPaths) => {
                    self.copy_selected_paths(ctx);
                }
                RenderCommand::TopAction(RenderTopActionCommand::ClearPinned) => {
                    self.clear_pinned();
                }
                RenderCommand::TopAction(RenderTopActionCommand::CreateFileList) => {
                    self.create_filelist();
                }
                RenderCommand::TopAction(RenderTopActionCommand::RefreshIndex) => {
                    self.request_index_refresh();
                }
                RenderCommand::FileListDialog(RenderFileListDialogCommand::ConfirmOverwrite) => {
                    self.confirm_pending_filelist_overwrite();
                }
                RenderCommand::FileListDialog(RenderFileListDialogCommand::CancelOverwrite) => {
                    self.cancel_pending_filelist_overwrite();
                }
                RenderCommand::FileListDialog(
                    RenderFileListDialogCommand::ConfirmAncestorPropagation,
                ) => {
                    self.confirm_pending_filelist_ancestor_propagation();
                }
                RenderCommand::FileListDialog(
                    RenderFileListDialogCommand::SkipAncestorPropagation,
                ) => {
                    self.skip_pending_filelist_ancestor_propagation();
                }
                RenderCommand::FileListDialog(
                    RenderFileListDialogCommand::CancelAncestorConfirmation,
                ) => {
                    self.cancel_pending_filelist_ancestor_confirmation();
                }
                RenderCommand::FileListDialog(RenderFileListDialogCommand::ConfirmUseWalker) => {
                    self.confirm_pending_filelist_use_walker();
                }
                RenderCommand::FileListDialog(RenderFileListDialogCommand::CancelUseWalker) => {
                    self.cancel_pending_filelist_use_walker();
                }
                RenderCommand::UpdateDialog(RenderUpdateDialogCommand::StartInstall) => {
                    self.start_update_install();
                }
                RenderCommand::UpdateDialog(
                    RenderUpdateDialogCommand::SkipPromptUntilNextVersion,
                ) => {
                    self.skip_update_prompt_until_next_version();
                }
                RenderCommand::UpdateDialog(RenderUpdateDialogCommand::DismissPrompt) => {
                    self.dismiss_update_prompt();
                }
                RenderCommand::UpdateDialog(RenderUpdateDialogCommand::SuppressCheckFailures) => {
                    self.suppress_update_check_failures();
                }
                RenderCommand::UpdateDialog(RenderUpdateDialogCommand::DismissCheckFailure) => {
                    self.dismiss_update_check_failure();
                }
                RenderCommand::TabBar(RenderTabBarCommand::CreateNewTab) => {
                    self.create_new_tab();
                }
                RenderCommand::TabBar(RenderTabBarCommand::CloseTab(index)) => {
                    self.close_tab_index(index);
                }
                RenderCommand::TabBar(RenderTabBarCommand::MoveTab {
                    from_index,
                    to_index,
                }) => {
                    self.move_tab(from_index, to_index);
                }
                RenderCommand::TabBar(RenderTabBarCommand::SwitchToTab(index)) => {
                    self.switch_to_tab_index(index);
                }
                RenderCommand::TabBar(RenderTabBarCommand::ClearTabAccent(index)) => {
                    self.set_tab_accent(index, None);
                }
                RenderCommand::TabBar(RenderTabBarCommand::SetTabAccent { index, accent }) => {
                    self.set_tab_accent(index, Some(accent));
                }
            }
        }
    }
}

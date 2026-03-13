use super::*;

impl FlistWalkerApp {
    pub(super) fn top_action_labels(&self) -> Vec<&'static str> {
        if self.history_search_active {
            return vec!["Apply History", "Cancel History Search"];
        }

        let create_label = if self.filelist_in_progress {
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

    pub(super) fn render_results_and_preview(&mut self, ui: &mut egui::Ui) {
        if self.history_search_active {
            self.preview_resize_in_progress = false;
            self.render_history_search_results(ui);
            self.scroll_to_current = false;
            return;
        }
        if self.show_preview {
            let max_preview_width = (ui.available_width() - Self::MIN_RESULTS_PANEL_WIDTH)
                .max(Self::MIN_PREVIEW_PANEL_WIDTH);
            let panel = egui::SidePanel::right("preview-panel")
                .resizable(true)
                .default_width(self.preview_panel_width.min(max_preview_width))
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
                                        egui::TextEdit::multiline(&mut self.preview)
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
            if (new_width - self.preview_panel_width).abs() > 1.0 {
                self.preview_panel_width = new_width;
                self.mark_ui_state_dirty();
            }
            let splitter_x = response.response.rect.left();
            let splitter_pressed = ui.input(|i| {
                let Some(pos) = i.pointer.interact_pos() else {
                    return false;
                };
                i.pointer.primary_down() && (pos.x - splitter_x).abs() <= 8.0
            });
            self.preview_resize_in_progress = response.response.dragged() || splitter_pressed;
            self.render_results_list(ui);
        } else {
            self.preview_resize_in_progress = false;
            self.render_results_list(ui);
        }
        self.scroll_to_current = false;
    }

    pub(super) fn results_scroll_enabled(preview_resize_in_progress: bool) -> bool {
        !preview_resize_in_progress
    }

    pub(super) fn render_results_list(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Results");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut selected = self.result_sort_mode;
                egui::ComboBox::from_id_source("results-sort-selector")
                    .selected_text(selected.label())
                    .show_ui(ui, |ui| {
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
                if selected != self.result_sort_mode {
                    self.set_result_sort_mode(selected);
                }
            });
        });
        let scroll_enabled = Self::results_scroll_enabled(self.preview_resize_in_progress);
        egui::ScrollArea::both()
            .enable_scrolling(scroll_enabled)
            .drag_to_scroll(false)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut clicked_row: Option<usize> = None;
                let mut execute_row: Option<usize> = None;
                let prefer_relative = self.prefer_relative_display();
                self.ensure_highlight_cache_scope(prefer_relative);

                for i in 0..self.results.len() {
                    let Some((path, _score)) = self.results.get(i) else {
                        continue;
                    };
                    let path = path.clone();
                    let is_current = self.current_row == Some(i);
                    let is_pinned = self.pinned_paths.contains(&path);
                    let marker_current = if is_current { "▶" } else { "·" };
                    let marker_pin = if is_pinned { "◆" } else { "·" };
                    let kind = self.entry_kinds.get(&path).copied();
                    let display = display_path_with_mode(&path, &self.root, prefer_relative);
                    let positions =
                        self.highlight_positions_for_path_cached(&path, prefer_relative);

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
                    let (kind_label, kind_color) = match kind {
                        Some(true) => ("DIR ", egui::Color32::from_rgb(52, 211, 153)),
                        Some(false) => ("FILE", egui::Color32::from_rgb(96, 165, 250)),
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
                        let color = if Self::is_highlighted_position(positions.as_slice(), idx) {
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
                                egui::Label::new(job)
                                    .wrap(false)
                                    .sense(egui::Sense::click()),
                            )
                        });
                    let response = row.inner;
                    if is_current && self.scroll_to_current {
                        response.scroll_to_me(None);
                    }
                    if response.clicked() {
                        clicked_row = Some(i);
                    }
                    if response.double_clicked() {
                        execute_row = Some(i);
                    }
                }
                if let Some(i) = clicked_row {
                    self.current_row = Some(i);
                    self.request_preview_for_current();
                    self.refresh_status_line();
                }
                if let Some(i) = execute_row {
                    self.current_row = Some(i);
                    self.execute_selected();
                }
            });
    }

    pub(super) fn render_history_search_results(&mut self, ui: &mut egui::Ui) {
        ui.heading("History Results");
        ui.label(format!(
            "{} items in history, {} matches",
            self.query_history.len(),
            self.history_search_results.len()
        ));
        egui::ScrollArea::vertical()
            .drag_to_scroll(false)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut clicked_row: Option<usize> = None;
                let mut accept_row: Option<usize> = None;

                for (index, entry) in self.history_search_results.iter().enumerate() {
                    let is_current = self.history_search_current == Some(index);
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
                                    .wrap(false)
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
                    self.history_search_current = Some(index);
                }
                if let Some(index) = accept_row {
                    self.history_search_current = Some(index);
                    self.accept_history_search();
                }
            });
    }

    pub(super) fn render_tab_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let mut switch_to: Option<usize> = None;
            let mut close_tab: Option<usize> = None;
            for i in 0..self.tabs.len() {
                let is_active = self.active_tab == i;
                let active_fill = if ui.visuals().dark_mode {
                    egui::Color32::from_rgb(48, 53, 62)
                } else {
                    egui::Color32::from_rgb(228, 232, 238)
                };
                egui::Frame::none()
                    .fill(if is_active {
                        active_fill
                    } else {
                        egui::Color32::TRANSPARENT
                    })
                    .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                    .rounding(egui::Rounding::same(4.0))
                    .inner_margin(egui::Margin::symmetric(6.0, 2.0))
                    .show(ui, |ui| {
                        let title = self
                            .tabs
                            .get(i)
                            .map(|tab| self.tab_title(tab, i))
                            .unwrap_or_else(|| format!("Tab {}", i + 1));
                        let title_response = ui.add(
                            egui::Button::new(egui::RichText::new(title).strong().color(
                                if is_active {
                                    ui.visuals().strong_text_color()
                                } else {
                                    ui.visuals().text_color()
                                },
                            ))
                            .frame(false),
                        );
                        if title_response.clicked_by(egui::PointerButton::Middle) {
                            close_tab = Some(i);
                        } else if title_response.clicked() {
                            switch_to = Some(i);
                        }
                        if ui
                            .add_enabled(
                                self.tabs.len() > 1,
                                egui::Button::new("×").small().frame(false),
                            )
                            .on_hover_text("Close tab")
                            .clicked()
                        {
                            close_tab = Some(i);
                        }
                    });
            }
            if ui
                .button("+")
                .on_hover_text(format!("New tab ({}+T)", Self::primary_shortcut_label()))
                .clicked()
            {
                self.create_new_tab();
                return;
            }
            if let Some(index) = close_tab {
                self.close_tab_index(index);
                return;
            }
            if let Some(idx) = switch_to {
                self.switch_to_tab_index(idx);
            }
        });
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
                        egui::ComboBox::from_id_source("root-selector")
                            .width(field_width)
                            .selected_text(selected_text)
                            .show_ui(ui, |ui| {
                                for p in &self.saved_roots {
                                    let text = Self::normalize_windows_path(p.clone())
                                        .to_string_lossy()
                                        .to_string();
                                    let is_selected =
                                        Self::path_key(p) == Self::path_key(&self.root);
                                    if ui.selectable_label(is_selected, text).clicked() {
                                        next_root = Some(p.clone());
                                    }
                                }
                            });
                    },
                );
                if ui
                    .add_sized([button_width, row_height], egui::Button::new("Browse..."))
                    .clicked()
                {
                    let dialog_root = Self::normalize_windows_path(self.root.clone());
                    match native_dialog::FileDialog::new()
                        .set_location(&dialog_root)
                        .show_open_single_dir()
                    {
                        Ok(Some(dir)) => {
                            self.apply_root_change(dir);
                        }
                        Ok(None) => {}
                        Err(err) => {
                            self.set_notice(format!("Browse failed: {}", err));
                        }
                    }
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
                    self.apply_root_change(root);
                }
            });

            ui.horizontal(|ui| {
                let use_filelist_changed = ui
                    .checkbox(&mut self.use_filelist, "Use FileList")
                    .changed();
                if ui.checkbox(&mut self.use_regex, "Regex").changed() {
                    self.invalidate_result_sort(true);
                    self.update_results();
                }
                let (files_changed, dirs_changed) = if self.use_filelist_requires_locked_filters() {
                    let mut forced_changed = false;
                    if !self.include_files || !self.include_dirs {
                        self.include_files = true;
                        self.include_dirs = true;
                        forced_changed = true;
                    }
                    ui.add_enabled(false, egui::Checkbox::new(&mut self.include_files, "Files"));
                    ui.add_enabled(
                        false,
                        egui::Checkbox::new(&mut self.include_dirs, "Folders"),
                    );
                    (forced_changed, forced_changed)
                } else {
                    (
                        ui.checkbox(&mut self.include_files, "Files").changed(),
                        ui.checkbox(&mut self.include_dirs, "Folders").changed(),
                    )
                };
                if ui.checkbox(&mut self.show_preview, "Preview").changed() {
                    if !self.show_preview {
                        self.preview_cache.clear();
                        self.preview_cache_order.clear();
                        self.preview_cache_total_bytes = 0;
                    }
                    self.mark_ui_state_dirty();
                }
                ui.separator();
                ui.label(self.source_text());
                self.maybe_reindex_from_filter_toggles(
                    use_filelist_changed,
                    files_changed,
                    dirs_changed,
                );
            });

            if self.history_search_active {
                ui.label(
                    egui::RichText::new("History Search")
                        .strong()
                        .color(ui.visuals().strong_text_color()),
                );
            }
            let editing_history_search = self.history_search_active;
            let mut output = egui::TextEdit::singleline(if editing_history_search {
                &mut self.history_search_query
            } else {
                &mut self.query
            })
                .id(self.query_input_id)
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
            if self.focus_query_requested {
                output.response.request_focus();
                self.focus_query_requested = false;
            }
            if self.unfocus_query_requested {
                output.response.surrender_focus();
                self.unfocus_query_requested = false;
            }
            let events = ctx.input(|i| i.events.clone());
            if !editing_history_search {
                let (query_event_changed, query_cursor_after_fallback) = self
                    .process_query_input_events(
                        ctx,
                        &events,
                        output.response.has_focus(),
                        output.response.changed(),
                        output.state.ccursor_range(),
                    );
                if query_event_changed {
                    self.mark_query_edited();
                    if output.response.has_focus() {
                        let end = query_cursor_after_fallback
                            .unwrap_or_else(|| Self::char_count(&self.query));
                        output
                            .state
                            .set_ccursor_range(Some(egui::text_edit::CCursorRange::one(
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
                    self.mark_query_edited();
                    Self::append_window_trace(
                        "query_text_changed",
                        &format!(
                            "chars={} has_half_space={} has_full_space={}",
                            self.query.chars().count(),
                            self.query.contains(' '),
                            self.query.contains('\u{3000}')
                        ),
                    );
                    self.update_results();
                }
            } else if output.response.changed() {
                self.refresh_history_search_results();
            }
            self.run_deferred_shortcuts(ctx);

            ui.horizontal(|ui| {
                for label in self.top_action_labels() {
                    if !ui.button(label).clicked() {
                        continue;
                    }
                    match label {
                        "Apply History" => self.accept_history_search(),
                        "Cancel History Search" => self.cancel_history_search(),
                        "Open / Execute" => self.execute_selected(),
                        "Copy Path(s)" => self.copy_selected_paths(ctx),
                        "Clear Selected" => self.clear_pinned(),
                        "Create File List" | "Create File List (Running...)" => {
                            self.create_filelist()
                        }
                        "Refresh Index" => self.request_index_refresh(),
                        _ => {}
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
                    if let Some(label) = self.action_progress_label() {
                        ui.add(egui::Spinner::new().size(14.0));
                        ui.label(label);
                        ui.separator();
                    }
                    ui.add(egui::Label::new(&self.status_line).truncate(true));
                });
            });
    }

    pub(super) fn render_filelist_dialogs(&mut self, ctx: &egui::Context) {
        let mut overwrite = false;
        let mut cancel_overwrite = false;
        let current_tab_id = self.current_tab_id().unwrap_or_default();
        if let Some(pending) = &self.pending_filelist_confirmation {
            if pending.tab_id == current_tab_id {
                egui::Window::new("Overwrite FileList?")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                    .show(ctx, |ui| {
                        ui.label(format!(
                            "{} already exists. Overwrite it?",
                            pending.existing_path.display()
                        ));
                        ui.horizontal(|ui| {
                            if ui.button("Overwrite").clicked() {
                                overwrite = true;
                            }
                            if ui.button("Cancel").clicked() {
                                cancel_overwrite = true;
                            }
                        });
                    });
            }
        }
        if overwrite {
            self.confirm_pending_filelist_overwrite();
        } else if cancel_overwrite {
            self.cancel_pending_filelist_overwrite();
        }

        let mut confirm_ancestor = false;
        let mut current_root_only = false;
        let mut cancel_ancestor = false;
        if let Some(pending) = &self.pending_filelist_ancestor_confirmation {
            if pending.tab_id == current_tab_id {
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
                            if ui.button("Continue").clicked() {
                                confirm_ancestor = true;
                            }
                            if ui.button("Current Root Only").clicked() {
                                current_root_only = true;
                            }
                            if ui.button("Cancel").clicked() {
                                cancel_ancestor = true;
                            }
                        });
                    });
            }
        }
        if confirm_ancestor {
            self.confirm_pending_filelist_ancestor_propagation();
        } else if current_root_only {
            self.skip_pending_filelist_ancestor_propagation();
        } else if cancel_ancestor {
            self.cancel_pending_filelist_ancestor_confirmation();
        }

        let mut confirm_walker = false;
        let mut cancel_walker = false;
        if let Some(pending) = &self.pending_filelist_use_walker_confirmation {
            if pending.source_tab_id == current_tab_id {
                egui::Window::new("Create File List?")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                    .show(ctx, |ui| {
                        ui.label(
                            "Use FileList が有効です。Create File List には Walker実行が必要です。",
                        );
                        ui.label(
                            "FileListインデックスからは生成しません。新規タブで実行しますか？",
                        );
                        ui.horizontal(|ui| {
                            if ui.button("Continue").clicked() {
                                confirm_walker = true;
                            }
                            if ui.button("Cancel").clicked() {
                                cancel_walker = true;
                            }
                        });
                    });
            }
        }
        if confirm_walker {
            self.confirm_pending_filelist_use_walker();
        } else if cancel_walker {
            self.cancel_pending_filelist_use_walker();
        }
    }

    pub(super) fn render_central_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_results_and_preview(ui);
        });
    }
}

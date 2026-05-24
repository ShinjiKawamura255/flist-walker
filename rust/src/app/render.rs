use super::{
    render_dialogs, render_panels, render_snapshot, render_tabs, render_theme, FlistWalkerApp,
    TabAccentColor,
};
use eframe::egui;

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
    OpenRuntimeConfig,
    FileListDialog(RenderFileListDialogCommand),
    UpdateDialog(RenderUpdateDialogCommand),
    TabBar(RenderTabBarCommand),
}

impl FlistWalkerApp {
    pub(super) const RESULT_SORT_SELECTOR_WIDTH: f32 = 132.0;
    pub(super) const RESULT_ROW_H_MARGIN: f32 = 3.0;
    pub(super) const RESULT_ROW_V_MARGIN: f32 = 2.0;
    pub(super) const RESULT_ROW_ROUNDING: f32 = 3.0;
    pub(super) const TAB_ROUNDING: f32 = 4.0;
    pub(super) const TAB_ACCENT_GLOW_HEIGHT: f32 = 8.0;
    pub(super) const TAB_ACCENT_LINE_HEIGHT: f32 = 3.0;
    pub(super) const TAB_ACTIVE_BORDER_WIDTH: f32 = 2.0;
    pub(super) const TAB_INACTIVE_BORDER_WIDTH: f32 = 1.0;

    pub(super) fn filelist_use_walker_dialog_lines() -> [&'static str; 2] {
        [
            "Use FileList が有効です。Create File List には Walker indexing が必要です。",
            "FileList インデックスからは生成せず、現在のタブの裏で一時的に Walker を実行します。続行しますか？",
        ]
    }

    pub(super) fn dialog_button(
        &self,
        ui: &mut egui::Ui,
        label: &str,
        selected: bool,
    ) -> egui::Response {
        let mut button = egui::Button::new(label);
        if selected {
            button = button.fill(render_theme::selected_fill(ui.visuals().dark_mode));
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

    pub(super) fn top_action_command(label: &str) -> Option<RenderTopActionCommand> {
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
            || self.shell.indexing.pending_finish.is_some()
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

        render_panels::render_top_panel(self, ctx);
        render_panels::render_status_panel(self, ctx);
        render_dialogs::render_filelist_dialogs(self, ctx);
        render_dialogs::render_update_dialog(self, ctx);
        self.render_central_panel(ctx);
        self.dispatch_render_commands(ctx);
        self.maybe_save_ui_state(false);
    }

    pub(super) fn results_scroll_enabled(preview_resize_in_progress: bool) -> bool {
        !preview_resize_in_progress
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

    #[allow(dead_code)]
    pub(super) fn render_tab_bar(&mut self, ui: &mut egui::Ui) {
        render_tabs::render_tab_bar(self, ui);
    }

    pub(super) fn render_central_panel(&mut self, ctx: &egui::Context) {
        render_panels::render_central_panel(self, ctx);
    }

    #[allow(dead_code)]
    pub(super) fn gui_surface_snapshot(&self) -> render_snapshot::GuiSurfaceSnapshot {
        render_snapshot::gui_surface_snapshot(self)
    }

    #[allow(dead_code)]
    pub(super) fn queue_render_command(&mut self, command: RenderCommand) {
        self.shell.ui.pending_render_commands.push(command);
    }

    pub(super) fn dispatch_render_commands(&mut self, ctx: &egui::Context) {
        let commands = std::mem::take(&mut self.shell.ui.pending_render_commands);
        for command in commands {
            match command {
                RenderCommand::OpenRuntimeConfig => {
                    self.open_runtime_config_file();
                }
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

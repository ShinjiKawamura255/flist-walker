use super::{FileListDialogKind, FlistWalkerApp, UpdateSupport};
use eframe::egui;

pub(super) fn render_filelist_dialogs(app: &mut FlistWalkerApp, ctx: &egui::Context) {
    let mut overwrite = false;
    let mut cancel_overwrite = false;
    let current_tab_id = app.current_tab_id().unwrap_or_default();
    if let Some(existing_path) = app
        .shell
        .features
        .filelist
        .workflow
        .pending_confirmation
        .as_ref()
        .filter(|pending| pending.tab_id == current_tab_id)
        .map(|pending| pending.existing_path.clone())
    {
        app.sync_filelist_dialog_selection(FileListDialogKind::Overwrite);
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
                    if app
                        .dialog_button(
                            ui,
                            "Overwrite",
                            app.shell.features.filelist.workflow.active_dialog_button == 0,
                        )
                        .clicked()
                    {
                        overwrite = true;
                    }
                    if app
                        .dialog_button(
                            ui,
                            "Cancel",
                            app.shell.features.filelist.workflow.active_dialog_button == 1,
                        )
                        .clicked()
                    {
                        cancel_overwrite = true;
                    }
                });
            });
    }
    if overwrite {
        app.queue_render_command(super::render::RenderCommand::FileListDialog(
            super::render::RenderFileListDialogCommand::ConfirmOverwrite,
        ));
    } else if cancel_overwrite {
        app.queue_render_command(super::render::RenderCommand::FileListDialog(
            super::render::RenderFileListDialogCommand::CancelOverwrite,
        ));
    }

    let mut confirm_ancestor = false;
    let mut current_root_only = false;
    let mut cancel_ancestor = false;
    if app
        .shell
        .features
        .filelist
        .workflow
        .pending_ancestor_confirmation
        .as_ref()
        .is_some_and(|pending| pending.tab_id == current_tab_id)
    {
        app.sync_filelist_dialog_selection(FileListDialogKind::Ancestor);
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
                    if app
                        .dialog_button(
                            ui,
                            "Continue",
                            app.shell.features.filelist.workflow.active_dialog_button == 0,
                        )
                        .clicked()
                    {
                        confirm_ancestor = true;
                    }
                    if app
                        .dialog_button(
                            ui,
                            "Current Root Only",
                            app.shell.features.filelist.workflow.active_dialog_button == 1,
                        )
                        .clicked()
                    {
                        current_root_only = true;
                    }
                    if app
                        .dialog_button(
                            ui,
                            "Cancel",
                            app.shell.features.filelist.workflow.active_dialog_button == 2,
                        )
                        .clicked()
                    {
                        cancel_ancestor = true;
                    }
                });
            });
    }
    if confirm_ancestor {
        app.queue_render_command(super::render::RenderCommand::FileListDialog(
            super::render::RenderFileListDialogCommand::ConfirmAncestorPropagation,
        ));
    } else if current_root_only {
        app.queue_render_command(super::render::RenderCommand::FileListDialog(
            super::render::RenderFileListDialogCommand::SkipAncestorPropagation,
        ));
    } else if cancel_ancestor {
        app.queue_render_command(super::render::RenderCommand::FileListDialog(
            super::render::RenderFileListDialogCommand::CancelAncestorConfirmation,
        ));
    }

    let mut confirm_walker = false;
    let mut cancel_walker = false;
    if app
        .shell
        .features
        .filelist
        .workflow
        .pending_use_walker_confirmation
        .as_ref()
        .is_some_and(|pending| pending.source_tab_id == current_tab_id)
    {
        let [line1, line2] = FlistWalkerApp::filelist_use_walker_dialog_lines();
        app.sync_filelist_dialog_selection(FileListDialogKind::UseWalker);
        egui::Window::new("Create File List?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(line1);
                ui.label(line2);
                ui.horizontal(|ui| {
                    if app
                        .dialog_button(
                            ui,
                            "Continue",
                            app.shell.features.filelist.workflow.active_dialog_button == 0,
                        )
                        .clicked()
                    {
                        confirm_walker = true;
                    }
                    if app
                        .dialog_button(
                            ui,
                            "Cancel",
                            app.shell.features.filelist.workflow.active_dialog_button == 1,
                        )
                        .clicked()
                    {
                        cancel_walker = true;
                    }
                });
            });
    }
    if confirm_walker {
        app.queue_render_command(super::render::RenderCommand::FileListDialog(
            super::render::RenderFileListDialogCommand::ConfirmUseWalker,
        ));
    } else if cancel_walker {
        app.queue_render_command(super::render::RenderCommand::FileListDialog(
            super::render::RenderFileListDialogCommand::CancelUseWalker,
        ));
    }
    if app.current_filelist_dialog_kind().is_none() {
        app.clear_filelist_dialog_selection();
    }
}

pub(super) fn render_update_dialog(app: &mut FlistWalkerApp, ctx: &egui::Context) {
    if let Some(prompt) = app.shell.features.update.state.prompt.as_ref().cloned() {
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
                                .add_enabled(!prompt.install_started, egui::Button::new("Later"))
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

        app.shell
            .features
            .update
            .set_prompt_skip_until_next_version(skip_until_next_version);

        if confirm {
            app.queue_render_command(super::render::RenderCommand::UpdateDialog(
                super::render::RenderUpdateDialogCommand::StartInstall,
            ));
        } else if later {
            if skip_until_next_version {
                app.queue_render_command(super::render::RenderCommand::UpdateDialog(
                    super::render::RenderUpdateDialogCommand::SkipPromptUntilNextVersion,
                ));
            } else {
                app.queue_render_command(super::render::RenderCommand::UpdateDialog(
                    super::render::RenderUpdateDialogCommand::DismissPrompt,
                ));
            }
        }
    }

    if let Some(failure) = app
        .shell
        .features
        .update
        .state
        .check_failure
        .as_ref()
        .cloned()
    {
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

        app.shell
            .features
            .update
            .set_check_failure_suppress_future_errors(suppress_future_errors);

        if close {
            if suppress_future_errors {
                app.queue_render_command(super::render::RenderCommand::UpdateDialog(
                    super::render::RenderUpdateDialogCommand::SuppressCheckFailures,
                ));
            } else {
                app.queue_render_command(super::render::RenderCommand::UpdateDialog(
                    super::render::RenderUpdateDialogCommand::DismissCheckFailure,
                ));
            }
        }
    }
}

pub(super) fn render_manage_root_list_dialog(app: &mut FlistWalkerApp, ctx: &egui::Context) {
    if !app.shell.features.root_browser.manage_list.open {
        return;
    }

    let mut add_input = false;
    let mut browse_and_add = false;
    let mut start_edit = false;
    let mut save_edit = false;
    let mut cancel_edit = false;
    let mut enter_remove_mode = false;
    let mut remove_selected = false;
    let mut cancel_remove_mode = false;
    let mut apply = false;
    let mut ok = false;
    let mut cancel = false;
    let viewport_id = FlistWalkerApp::manage_root_list_viewport_id();
    let parent_rect = ctx.input(|input| input.viewport().outer_rect);
    let viewport_builder = FlistWalkerApp::manage_root_list_viewport_builder(parent_rect);

    ctx.show_viewport_immediate(viewport_id, viewport_builder, |ui, _class| {
        if ui.input(|input| {
            input
                .viewport()
                .events
                .contains(&egui::ViewportEvent::Close)
        }) {
            cancel = true;
        }

        egui::CentralPanel::default().show_inside(ui, |ui| {
            let mut add_response = None;
            ui.horizontal(|ui| {
                let row_height = ui.spacing().interact_size.y;
                let browse_width = 84.0;
                let add_width = 52.0;
                let spacing = ui.spacing().item_spacing.x * 2.0;
                let input_width =
                    (ui.available_width() - browse_width - add_width - spacing).max(160.0);
                let has_error = !app
                    .shell
                    .features
                    .root_browser
                    .manage_list
                    .add_error
                    .is_empty();
                let response = FlistWalkerApp::manage_root_list_text_edit(
                    ui,
                    &mut app.shell.features.root_browser.manage_list.input_path,
                    input_width,
                    has_error,
                    Some("Folder path"),
                );
                if response.changed() {
                    app.clear_manage_root_list_add_error();
                }
                add_response = Some(response);
                if ui
                    .add_sized([browse_width, row_height], egui::Button::new("Browse..."))
                    .clicked()
                {
                    browse_and_add = true;
                }
                if ui
                    .add_sized([add_width, row_height], egui::Button::new("Add"))
                    .clicked()
                {
                    add_input = true;
                }
            });
            if let Some(response) = add_response {
                let manage = &mut app.shell.features.root_browser.manage_list;
                let text = manage.input_path.clone();
                FlistWalkerApp::apply_manage_root_list_text_edit_focus(
                    &response,
                    &text,
                    &mut manage.add_focus_requested,
                    &mut manage.add_select_all_requested,
                );
            }
            let add_error = app
                .shell
                .features
                .root_browser
                .manage_list
                .add_error
                .clone();
            if !add_error.is_empty() {
                FlistWalkerApp::manage_root_list_error_label(ui, &add_error);
            }

            let notice = app.shell.features.root_browser.manage_list.notice.clone();
            if !notice.is_empty() {
                ui.label(notice);
            }

            ui.separator();
            ui.horizontal(|ui| {
                let manage = &app.shell.features.root_browser.manage_list;
                ui.heading(if manage.remove_mode {
                    "Select roots to remove"
                } else {
                    "Saved roots"
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if manage.remove_mode {
                        if ui.button("Cancel").clicked() {
                            cancel_remove_mode = true;
                        }
                        if ui
                            .add_enabled(
                                !manage.selected_indices.is_empty(),
                                egui::Button::new("Remove selected"),
                            )
                            .clicked()
                        {
                            remove_selected = true;
                        }
                    } else {
                        if ui
                            .add_enabled(
                                manage.editing_index.is_none() && !manage.draft_roots.is_empty(),
                                egui::Button::new("Remove..."),
                            )
                            .clicked()
                        {
                            enter_remove_mode = true;
                        }
                        if ui
                            .add_enabled(
                                manage.selected_index.is_some() && manage.editing_index.is_none(),
                                egui::Button::new("Edit"),
                            )
                            .clicked()
                        {
                            start_edit = true;
                        }
                    }
                });
            });

            let button_row_height = ui.spacing().interact_size.y + ui.spacing().item_spacing.y;
            let list_height = (ui.available_height() - button_row_height - 8.0).max(80.0);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(list_height)
                .show(ui, |ui| {
                    let roots = app
                        .shell
                        .features
                        .root_browser
                        .manage_list
                        .draft_roots
                        .clone();
                    if roots.is_empty() {
                        ui.label("No saved roots");
                    }
                    for (index, root) in roots.iter().enumerate() {
                        let label = root.to_string_lossy().to_string();
                        let remove_mode = app.shell.features.root_browser.manage_list.remove_mode;
                        let editing_index =
                            app.shell.features.root_browser.manage_list.editing_index;
                        if remove_mode {
                            let mut selected = app
                                .shell
                                .features
                                .root_browser
                                .manage_list
                                .selected_indices
                                .contains(&index);
                            if ui.checkbox(&mut selected, label).changed() {
                                let selected_indices = &mut app
                                    .shell
                                    .features
                                    .root_browser
                                    .manage_list
                                    .selected_indices;
                                if selected {
                                    selected_indices.insert(index);
                                } else {
                                    selected_indices.remove(&index);
                                }
                            }
                        } else if editing_index == Some(index) {
                            ui.vertical(|ui| {
                                let mut edit_response = None;
                                ui.horizontal(|ui| {
                                    let available = (ui.available_width() - 124.0).max(160.0);
                                    let has_error = !app
                                        .shell
                                        .features
                                        .root_browser
                                        .manage_list
                                        .edit_error
                                        .is_empty();
                                    let response = FlistWalkerApp::manage_root_list_text_edit(
                                        ui,
                                        &mut app.shell.features.root_browser.manage_list.edit_path,
                                        available,
                                        has_error,
                                        None,
                                    );
                                    if response.changed() {
                                        app.clear_manage_root_list_edit_error();
                                    }
                                    if response.has_focus() {
                                        if ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                                            save_edit = true;
                                        }
                                        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                                            cancel_edit = true;
                                        }
                                    }
                                    if ui.button("Save").clicked() {
                                        save_edit = true;
                                    }
                                    if ui.button("Cancel").clicked() {
                                        cancel_edit = true;
                                    }
                                    edit_response = Some(response);
                                });
                                if let Some(response) = edit_response {
                                    let manage = &mut app.shell.features.root_browser.manage_list;
                                    let text = manage.edit_path.clone();
                                    FlistWalkerApp::apply_manage_root_list_text_edit_focus(
                                        &response,
                                        &text,
                                        &mut manage.edit_focus_requested,
                                        &mut manage.edit_select_all_requested,
                                    );
                                }
                                let edit_error = app
                                    .shell
                                    .features
                                    .root_browser
                                    .manage_list
                                    .edit_error
                                    .clone();
                                if !edit_error.is_empty() {
                                    FlistWalkerApp::manage_root_list_error_label(ui, &edit_error);
                                }
                            });
                        } else {
                            let selected =
                                app.shell.features.root_browser.manage_list.selected_index
                                    == Some(index);
                            let response = FlistWalkerApp::manage_root_list_selectable_row(
                                ui, selected, &label,
                            );
                            if response.double_clicked() {
                                if app.select_manage_root_list_item(index) {
                                    start_edit = true;
                                }
                            } else if response.clicked() {
                                app.select_manage_root_list_item(index);
                            }
                        }
                    }
                });

            ui.separator();
            let action_height = ui.spacing().interact_size.y.round();
            let (row_rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), action_height),
                egui::Sense::hover(),
            );
            let [apply_rect, ok_rect, cancel_rect] =
                FlistWalkerApp::manage_root_list_action_button_rects(
                    row_rect,
                    action_height,
                    ui.spacing().item_spacing.x,
                );
            ui.scope(|ui| {
                let mut style = (**ui.style()).clone();
                style.visuals.widgets.hovered.expansion = 0.0;
                style.visuals.widgets.active.expansion = 0.0;
                style.visuals.widgets.open.expansion = 0.0;
                ui.set_style(style);
                if ui.put(apply_rect, egui::Button::new("Apply")).clicked() {
                    apply = true;
                }
                if ui.put(ok_rect, egui::Button::new("OK")).clicked() {
                    ok = true;
                }
                if ui.put(cancel_rect, egui::Button::new("Cancel")).clicked() {
                    cancel = true;
                }
            });
        });
    });

    if browse_and_add {
        app.queue_render_command(super::render::RenderCommand::RootListDialog(
            super::render::RenderRootListDialogCommand::BrowseAndAdd,
        ));
    }
    if add_input {
        app.queue_render_command(super::render::RenderCommand::RootListDialog(
            super::render::RenderRootListDialogCommand::AddInput,
        ));
    }
    if start_edit {
        app.queue_render_command(super::render::RenderCommand::RootListDialog(
            super::render::RenderRootListDialogCommand::StartEdit,
        ));
    }
    if save_edit {
        app.queue_render_command(super::render::RenderCommand::RootListDialog(
            super::render::RenderRootListDialogCommand::SaveEdit,
        ));
    }
    if cancel_edit {
        app.queue_render_command(super::render::RenderCommand::RootListDialog(
            super::render::RenderRootListDialogCommand::CancelEdit,
        ));
    }
    if enter_remove_mode {
        app.queue_render_command(super::render::RenderCommand::RootListDialog(
            super::render::RenderRootListDialogCommand::EnterRemoveMode,
        ));
    }
    if remove_selected {
        app.queue_render_command(super::render::RenderCommand::RootListDialog(
            super::render::RenderRootListDialogCommand::RemoveSelected,
        ));
    }
    if cancel_remove_mode {
        app.queue_render_command(super::render::RenderCommand::RootListDialog(
            super::render::RenderRootListDialogCommand::CancelRemoveMode,
        ));
    }
    if apply {
        app.queue_render_command(super::render::RenderCommand::RootListDialog(
            super::render::RenderRootListDialogCommand::Apply,
        ));
    }
    if ok {
        app.queue_render_command(super::render::RenderCommand::RootListDialog(
            super::render::RenderRootListDialogCommand::Ok,
        ));
    }
    if cancel {
        app.queue_render_command(super::render::RenderCommand::RootListDialog(
            super::render::RenderRootListDialogCommand::Cancel,
        ));
    }
}

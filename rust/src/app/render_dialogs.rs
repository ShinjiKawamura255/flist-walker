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

use crate::app::{FileListDialogKind, FlistWalkerApp};
use eframe::egui;

pub(in crate::app) fn overwrite_command(
    overwrite: bool,
    cancel: bool,
) -> Option<crate::app::render::RenderFileListDialogCommand> {
    if overwrite {
        Some(crate::app::render::RenderFileListDialogCommand::ConfirmOverwrite)
    } else if cancel {
        Some(crate::app::render::RenderFileListDialogCommand::CancelOverwrite)
    } else {
        None
    }
}

pub(in crate::app) fn ancestor_command(
    confirm: bool,
    current_root_only: bool,
    cancel: bool,
) -> Option<crate::app::render::RenderFileListDialogCommand> {
    if confirm {
        Some(crate::app::render::RenderFileListDialogCommand::ConfirmAncestorPropagation)
    } else if current_root_only {
        Some(crate::app::render::RenderFileListDialogCommand::SkipAncestorPropagation)
    } else if cancel {
        Some(crate::app::render::RenderFileListDialogCommand::CancelAncestorConfirmation)
    } else {
        None
    }
}

pub(in crate::app) fn use_walker_command(
    confirm: bool,
    cancel: bool,
) -> Option<crate::app::render::RenderFileListDialogCommand> {
    if confirm {
        Some(crate::app::render::RenderFileListDialogCommand::ConfirmUseWalker)
    } else if cancel {
        Some(crate::app::render::RenderFileListDialogCommand::CancelUseWalker)
    } else {
        None
    }
}

pub(super) fn render(app: &mut FlistWalkerApp, ctx: &egui::Context) {
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
    if let Some(command) = overwrite_command(overwrite, cancel_overwrite) {
        app.queue_render_command(crate::app::render::RenderCommand::FileListDialog(command));
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
    if let Some(command) = ancestor_command(confirm_ancestor, current_root_only, cancel_ancestor) {
        app.queue_render_command(crate::app::render::RenderCommand::FileListDialog(command));
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
    if let Some(command) = use_walker_command(confirm_walker, cancel_walker) {
        app.queue_render_command(crate::app::render::RenderCommand::FileListDialog(command));
    }
    if app.current_filelist_dialog_kind().is_none() {
        app.clear_filelist_dialog_selection();
    }
}

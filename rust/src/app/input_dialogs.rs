use super::{FileListDialogKind, FlistWalkerApp};
use eframe::egui;

fn filelist_dialog_button_count(kind: FileListDialogKind) -> usize {
    match kind {
        FileListDialogKind::Overwrite => 2,
        FileListDialogKind::Ancestor => 3,
        FileListDialogKind::UseWalker => 2,
    }
}

pub(super) fn current_filelist_dialog_kind(app: &FlistWalkerApp) -> Option<FileListDialogKind> {
    let Some(current_tab_id) = app.current_tab_id() else {
        return None;
    };
    if app
        .shell
        .features
        .filelist
        .workflow.pending_confirmation
        .as_ref()
        .is_some_and(|pending| pending.tab_id == current_tab_id)
    {
        return Some(FileListDialogKind::Overwrite);
    }
    if app
        .shell
        .features
        .filelist
        .workflow.pending_ancestor_confirmation
        .as_ref()
        .is_some_and(|pending| pending.tab_id == current_tab_id)
    {
        return Some(FileListDialogKind::Ancestor);
    }
    if app
        .shell
        .features
        .filelist
        .workflow.pending_use_walker_confirmation
        .as_ref()
        .is_some_and(|pending| pending.source_tab_id == current_tab_id)
    {
        return Some(FileListDialogKind::UseWalker);
    }
    None
}

pub(super) fn sync_filelist_dialog_selection(app: &mut FlistWalkerApp, kind: FileListDialogKind) {
    let button_count = filelist_dialog_button_count(kind);
    if app.shell.features.filelist.workflow.active_dialog != Some(kind) {
        app.shell.features.filelist.workflow.active_dialog = Some(kind);
        app.shell.features.filelist.workflow.active_dialog_button = 0;
        return;
    }
    app.shell.features.filelist.workflow.active_dialog_button %= button_count;
}

pub(super) fn clear_filelist_dialog_selection(app: &mut FlistWalkerApp) {
    app.shell.features.filelist.workflow.active_dialog = None;
    app.shell.features.filelist.workflow.active_dialog_button = 0;
}

pub(super) fn activate_selected_filelist_dialog_button(app: &mut FlistWalkerApp) {
    match (
        app.shell.features.filelist.workflow.active_dialog,
        app.shell.features.filelist.workflow.active_dialog_button,
    ) {
        (Some(FileListDialogKind::Overwrite), 0) => app.confirm_pending_filelist_overwrite(),
        (Some(FileListDialogKind::Overwrite), _) => app.cancel_pending_filelist_overwrite(),
        (Some(FileListDialogKind::Ancestor), 0) => {
            app.confirm_pending_filelist_ancestor_propagation()
        }
        (Some(FileListDialogKind::Ancestor), 1) => {
            app.skip_pending_filelist_ancestor_propagation()
        }
        (Some(FileListDialogKind::Ancestor), _) => {
            app.cancel_pending_filelist_ancestor_confirmation()
        }
        (Some(FileListDialogKind::UseWalker), 0) => app.confirm_pending_filelist_use_walker(),
        (Some(FileListDialogKind::UseWalker), _) => app.cancel_pending_filelist_use_walker(),
        (None, _) => {}
    }
}

pub(super) fn cancel_active_filelist_dialog(app: &mut FlistWalkerApp) {
    match app.shell.features.filelist.workflow.active_dialog {
        Some(FileListDialogKind::Overwrite) => app.cancel_pending_filelist_overwrite(),
        Some(FileListDialogKind::Ancestor) => app.cancel_pending_filelist_ancestor_confirmation(),
        Some(FileListDialogKind::UseWalker) => app.cancel_pending_filelist_use_walker(),
        None => {}
    }
}

pub(super) fn move_filelist_dialog_selection(app: &mut FlistWalkerApp, delta: isize) {
    let Some(kind) = app.shell.features.filelist.workflow.active_dialog else {
        return;
    };
    let count = filelist_dialog_button_count(kind) as isize;
    let current = app.shell.features.filelist.workflow.active_dialog_button as isize;
    app.shell.features.filelist.workflow.active_dialog_button =
        (current + delta).rem_euclid(count) as usize;
}

pub(super) fn handle_filelist_dialog_shortcuts(
    app: &mut FlistWalkerApp,
    ctx: &egui::Context,
) -> bool {
    let Some(kind) = current_filelist_dialog_kind(app) else {
        clear_filelist_dialog_selection(app);
        return false;
    };
    sync_filelist_dialog_selection(app, kind);

    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        cancel_active_filelist_dialog(app);
        return true;
    }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft))
        || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp))
        || ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab))
    {
        move_filelist_dialog_selection(app, -1);
        return true;
    }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight))
        || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown))
        || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab))
    {
        move_filelist_dialog_selection(app, 1);
        return true;
    }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
        || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space))
    {
        activate_selected_filelist_dialog_button(app);
        return true;
    }
    true
}

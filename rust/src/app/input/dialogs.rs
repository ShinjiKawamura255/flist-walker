use super::super::{FileListDialogKind, FlistWalkerApp};
use eframe::egui;

fn filelist_dialog_button_count(kind: FileListDialogKind) -> usize {
    match kind {
        FileListDialogKind::Overwrite => 2,
        FileListDialogKind::Ancestor => 3,
        FileListDialogKind::UseWalker => 2,
    }
}

impl FlistWalkerApp {
    pub(in crate::app) fn current_filelist_dialog_kind(&self) -> Option<FileListDialogKind> {
        let current_tab_id = self.current_tab_id()?;
        if self
            .shell
            .features
            .filelist
            .workflow
            .pending_confirmation
            .as_ref()
            .is_some_and(|pending| pending.tab_id == current_tab_id)
        {
            return Some(FileListDialogKind::Overwrite);
        }
        if self
            .shell
            .features
            .filelist
            .workflow
            .pending_ancestor_confirmation
            .as_ref()
            .is_some_and(|pending| pending.tab_id == current_tab_id)
        {
            return Some(FileListDialogKind::Ancestor);
        }
        if self
            .shell
            .features
            .filelist
            .workflow
            .pending_use_walker_confirmation
            .as_ref()
            .is_some_and(|pending| pending.source_tab_id == current_tab_id)
        {
            return Some(FileListDialogKind::UseWalker);
        }
        None
    }

    pub(in crate::app) fn sync_filelist_dialog_selection(&mut self, kind: FileListDialogKind) {
        let button_count = filelist_dialog_button_count(kind);
        if self.shell.features.filelist.workflow.active_dialog != Some(kind) {
            self.shell.features.filelist.workflow.active_dialog = Some(kind);
            self.shell.features.filelist.workflow.active_dialog_button = 0;
            return;
        }
        self.shell.features.filelist.workflow.active_dialog_button %= button_count;
    }

    pub(in crate::app) fn clear_filelist_dialog_selection(&mut self) {
        self.shell.features.filelist.workflow.active_dialog = None;
        self.shell.features.filelist.workflow.active_dialog_button = 0;
    }

    fn activate_selected_filelist_dialog_button(&mut self) {
        match (
            self.shell.features.filelist.workflow.active_dialog,
            self.shell.features.filelist.workflow.active_dialog_button,
        ) {
            (Some(FileListDialogKind::Overwrite), 0) => self.confirm_pending_filelist_overwrite(),
            (Some(FileListDialogKind::Overwrite), _) => self.cancel_pending_filelist_overwrite(),
            (Some(FileListDialogKind::Ancestor), 0) => {
                self.confirm_pending_filelist_ancestor_propagation()
            }
            (Some(FileListDialogKind::Ancestor), 1) => {
                self.skip_pending_filelist_ancestor_propagation()
            }
            (Some(FileListDialogKind::Ancestor), _) => {
                self.cancel_pending_filelist_ancestor_confirmation()
            }
            (Some(FileListDialogKind::UseWalker), 0) => self.confirm_pending_filelist_use_walker(),
            (Some(FileListDialogKind::UseWalker), _) => self.cancel_pending_filelist_use_walker(),
            (None, _) => {}
        }
    }

    fn cancel_active_filelist_dialog(&mut self) {
        match self.shell.features.filelist.workflow.active_dialog {
            Some(FileListDialogKind::Overwrite) => self.cancel_pending_filelist_overwrite(),
            Some(FileListDialogKind::Ancestor) => {
                self.cancel_pending_filelist_ancestor_confirmation()
            }
            Some(FileListDialogKind::UseWalker) => self.cancel_pending_filelist_use_walker(),
            None => {}
        }
    }

    fn move_filelist_dialog_selection(&mut self, delta: isize) {
        let Some(kind) = self.shell.features.filelist.workflow.active_dialog else {
            return;
        };
        let count = filelist_dialog_button_count(kind) as isize;
        let current = self.shell.features.filelist.workflow.active_dialog_button as isize;
        self.shell.features.filelist.workflow.active_dialog_button =
            (current + delta).rem_euclid(count) as usize;
    }

    pub(in crate::app) fn handle_filelist_dialog_shortcuts(&mut self, ctx: &egui::Context) -> bool {
        let Some(kind) = self.current_filelist_dialog_kind() else {
            self.clear_filelist_dialog_selection();
            return false;
        };
        self.sync_filelist_dialog_selection(kind);

        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.cancel_active_filelist_dialog();
            return true;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft))
            || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp))
            || ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab))
        {
            self.move_filelist_dialog_selection(-1);
            return true;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight))
            || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown))
            || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab))
        {
            self.move_filelist_dialog_selection(1);
            return true;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
            || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space))
        {
            self.activate_selected_filelist_dialog_button();
            return true;
        }
        true
    }
}

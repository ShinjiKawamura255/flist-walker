use super::super::{input_dialogs, FileListDialogKind, FlistWalkerApp};

impl FlistWalkerApp {
    pub(in crate::app) fn current_filelist_dialog_kind(&self) -> Option<FileListDialogKind> {
        input_dialogs::current_filelist_dialog_kind(self)
    }

    pub(in crate::app) fn sync_filelist_dialog_selection(&mut self, kind: FileListDialogKind) {
        input_dialogs::sync_filelist_dialog_selection(self, kind);
    }

    pub(in crate::app) fn clear_filelist_dialog_selection(&mut self) {
        input_dialogs::clear_filelist_dialog_selection(self);
    }
}

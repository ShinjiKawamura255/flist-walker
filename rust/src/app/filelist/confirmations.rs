use super::super::{FlistWalkerApp, PendingFileListAfterIndex};
use crate::path_utils::path_key;
use std::sync::atomic::Ordering;
impl FlistWalkerApp {
    fn cancel_stale_pending_filelist_confirmation(&mut self) {
        let current_tab_id = self.current_tab_id().unwrap_or_default();
        let current_root_key = path_key(&self.shell.runtime.root);
        let should_cancel = self
            .shell
            .features
            .filelist
            .cancel_stale_pending_confirmation(current_tab_id, current_root_key.as_ref());
        if should_cancel {
            self.set_notice("Pending FileList overwrite canceled because root changed");
        }
    }

    fn cancel_stale_pending_filelist_ancestor_confirmation(&mut self) {
        let current_tab_id = self.current_tab_id().unwrap_or_default();
        let current_root_key = path_key(&self.shell.runtime.root);
        let should_cancel = self
            .shell
            .features
            .filelist
            .cancel_stale_pending_ancestor_confirmation(current_tab_id, current_root_key.as_ref());
        if should_cancel {
            self.set_notice(
                "Pending Create File List ancestor update canceled because root changed",
            );
        }
    }

    fn cancel_stale_pending_filelist_use_walker_confirmation(&mut self) {
        let current_tab_id = self.current_tab_id().unwrap_or_default();
        let current_root_key = path_key(&self.shell.runtime.root);
        let should_cancel = self
            .shell
            .features
            .filelist
            .cancel_stale_pending_use_walker_confirmation(
                current_tab_id,
                current_root_key.as_ref(),
            );
        if should_cancel {
            self.set_notice("Pending Create File List confirmation canceled because root changed");
        }
    }

    pub(in crate::app) fn cancel_stale_pending_filelist_confirmations_for_active_root(&mut self) {
        self.cancel_stale_pending_filelist_confirmation();
        self.cancel_stale_pending_filelist_ancestor_confirmation();
        self.cancel_stale_pending_filelist_use_walker_confirmation();
    }

    pub(in crate::app) fn confirm_pending_filelist_overwrite(&mut self) {
        let Some(pending) = self
            .shell
            .features
            .filelist
            .workflow
            .pending_confirmation
            .take()
        else {
            return;
        };
        self.request_filelist_creation_after_overwrite_check(
            pending.tab_id,
            pending.root,
            pending.entries,
        );
    }

    pub(in crate::app) fn confirm_pending_filelist_ancestor_propagation(&mut self) {
        let Some(pending) = self
            .shell
            .features
            .filelist
            .workflow
            .pending_ancestor_confirmation
            .take()
        else {
            return;
        };
        self.start_filelist_creation(pending.tab_id, pending.root, pending.entries, true);
    }

    pub(in crate::app) fn skip_pending_filelist_ancestor_propagation(&mut self) {
        let Some(pending) = self
            .shell
            .features
            .filelist
            .workflow
            .pending_ancestor_confirmation
            .take()
        else {
            return;
        };
        self.start_filelist_creation(pending.tab_id, pending.root, pending.entries, false);
    }

    pub(in crate::app) fn confirm_pending_filelist_use_walker(&mut self) {
        let Some(pending) = self
            .shell
            .features
            .filelist
            .workflow
            .pending_use_walker_confirmation
            .take()
        else {
            return;
        };
        self.shell.features.filelist.workflow.pending_after_index =
            Some(PendingFileListAfterIndex {
                tab_id: pending.source_tab_id,
                root: pending.root,
            });
        if !self.shell.runtime.include_files || !self.shell.runtime.include_dirs {
            self.shell.runtime.include_files = true;
            self.shell.runtime.include_dirs = true;
        }
        self.request_create_filelist_walker_refresh();
        self.set_notice("Preparing background Walker index for Create File List");
    }

    pub(in crate::app) fn cancel_pending_filelist_overwrite(&mut self) {
        if self
            .shell
            .features
            .filelist
            .workflow
            .pending_confirmation
            .take()
            .is_some()
        {
            self.set_notice("Create File List canceled");
        }
    }

    pub(in crate::app) fn cancel_pending_filelist_ancestor_confirmation(&mut self) {
        if self
            .shell
            .features
            .filelist
            .workflow
            .pending_ancestor_confirmation
            .take()
            .is_some()
        {
            self.set_notice("Create File List canceled");
        }
    }

    pub(in crate::app) fn cancel_pending_filelist_use_walker(&mut self) {
        if self
            .shell
            .features
            .filelist
            .workflow
            .pending_use_walker_confirmation
            .take()
            .is_some()
        {
            self.set_notice("Create File List canceled");
        }
    }

    pub(in crate::app) fn can_cancel_create_filelist(&self) -> bool {
        self.shell
            .features
            .filelist
            .workflow
            .pending_after_index
            .is_some()
            || self
                .shell
                .features
                .filelist
                .workflow
                .pending_confirmation
                .is_some()
            || self
                .shell
                .features
                .filelist
                .workflow
                .pending_ancestor_confirmation
                .is_some()
            || self
                .shell
                .features
                .filelist
                .workflow
                .pending_use_walker_confirmation
                .is_some()
            || self.shell.features.filelist.workflow.in_progress
    }

    pub(in crate::app) fn cancel_create_filelist(&mut self) {
        if self
            .shell
            .features
            .filelist
            .workflow
            .pending_confirmation
            .is_some()
        {
            self.cancel_pending_filelist_overwrite();
            return;
        }
        if self
            .shell
            .features
            .filelist
            .workflow
            .pending_ancestor_confirmation
            .is_some()
        {
            self.cancel_pending_filelist_ancestor_confirmation();
            return;
        }
        if self
            .shell
            .features
            .filelist
            .workflow
            .pending_use_walker_confirmation
            .is_some()
        {
            self.cancel_pending_filelist_use_walker();
            return;
        }
        if self
            .shell
            .features
            .filelist
            .workflow
            .pending_after_index
            .take()
            .is_some()
        {
            self.set_notice("Create File List canceled");
            return;
        }
        if let Some(cancel) = self.shell.features.filelist.request_cancel() {
            cancel.store(true, Ordering::Relaxed);
            self.refresh_status_line();
            self.set_notice("Canceling Create File List...");
        }
    }
}

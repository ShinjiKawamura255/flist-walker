use super::super::{
    FlistWalkerApp, PendingFileListAfterIndex, PendingFileListAncestorConfirmation,
    PendingFileListConfirmation, PendingFileListUseWalkerConfirmation,
};
use std::path::PathBuf;
impl FlistWalkerApp {
    pub(in crate::app) fn discard_prepared_filelist_snapshot(&mut self, request_id: u64) {
        let commands = self
            .shell
            .features
            .filelist
            .discard_prepared_commands(request_id);
        self.dispatch_filelist_commands(commands);
    }

    pub(in crate::app) fn start_filelist_creation(
        &mut self,
        tab_id: u64,
        root: PathBuf,
        prepared_request_id: u64,
        propagate_to_ancestors: bool,
    ) {
        let commands = self.shell.features.filelist.start_request_commands(
            tab_id,
            root,
            prepared_request_id,
            propagate_to_ancestors,
        );
        self.dispatch_filelist_commands(commands);
    }

    pub(in crate::app) fn request_filelist_creation(
        &mut self,
        tab_id: u64,
        root: PathBuf,
        entries: Vec<PathBuf>,
    ) {
        let commands = self
            .shell
            .features
            .filelist
            .start_preflight_commands(tab_id, root, entries);
        self.dispatch_filelist_commands(commands);
    }

    pub(in crate::app) fn complete_filelist_preflight(
        &mut self,
        tab_id: u64,
        root: PathBuf,
        prepared_request_id: u64,
        existing_path: Option<PathBuf>,
        ancestor_confirmation_needed: bool,
    ) {
        if let Some(existing_path) = existing_path {
            self.shell.features.filelist.workflow.pending_confirmation =
                Some(PendingFileListConfirmation {
                    tab_id,
                    root,
                    prepared_request_id,
                    existing_path: existing_path.clone(),
                    ancestor_confirmation_needed,
                });
            self.set_notice(format!(
                "{} already exists. Choose overwrite or cancel.",
                existing_path.display()
            ));
            return;
        }
        self.request_filelist_creation_after_overwrite_check(
            tab_id,
            root,
            prepared_request_id,
            ancestor_confirmation_needed,
        );
    }

    pub(in crate::app) fn request_filelist_creation_after_overwrite_check(
        &mut self,
        tab_id: u64,
        root: PathBuf,
        prepared_request_id: u64,
        ancestor_confirmation_needed: bool,
    ) {
        if ancestor_confirmation_needed {
            self.shell
                .features
                .filelist
                .workflow
                .pending_ancestor_confirmation = Some(PendingFileListAncestorConfirmation {
                tab_id,
                root,
                prepared_request_id,
            });
            self.set_notice(
                "Create File List will also update parent FileList entries. Continue or choose current root only.",
            );
            return;
        }
        self.start_filelist_creation(tab_id, root, prepared_request_id, false);
    }

    pub(in crate::app) fn create_filelist(&mut self) {
        if self.shell.features.filelist.workflow.in_progress {
            self.set_notice("Create File List is already running");
            return;
        }
        if self
            .shell
            .features
            .filelist
            .workflow
            .pending_confirmation
            .is_some()
        {
            self.set_notice("Confirm overwrite or cancel first");
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
            self.set_notice("Confirm ancestor FileList update choice or cancel first");
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
            self.set_notice("Confirm Create File List action or cancel first");
            return;
        }
        let Some(tab_id) = self.current_tab_id() else {
            self.set_notice("Create File List is unavailable without an active tab");
            return;
        };
        if self.use_filelist_requires_locked_filters() {
            self.shell
                .features
                .filelist
                .workflow
                .pending_use_walker_confirmation = Some(PendingFileListUseWalkerConfirmation {
                source_tab_id: tab_id,
                root: self.shell.runtime.root.clone(),
            });
            self.set_notice("Confirmation required: Create File List needs Walker indexing");
            return;
        }

        let filters_adjusted =
            !self.shell.runtime.include_files || !self.shell.runtime.include_dirs;
        if !self.shell.runtime.include_files || !self.shell.runtime.include_dirs {
            self.shell.runtime.include_files = true;
            self.shell.runtime.include_dirs = true;
        }

        self.shell.features.filelist.workflow.pending_after_index =
            Some(PendingFileListAfterIndex {
                tab_id,
                root: self.shell.runtime.root.clone(),
                index_request_id: None,
            });
        self.request_create_filelist_walker_refresh();
        self.set_notice(if filters_adjusted {
            "Preparing complete Walker snapshot with files/folders enabled before Create File List"
        } else {
            "Preparing fresh complete Walker snapshot before Create File List"
        });
    }
}

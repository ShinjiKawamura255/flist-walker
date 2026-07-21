use super::super::{
    FlistWalkerApp, IndexSource, PendingFileListAfterIndex, PendingFileListAncestorConfirmation,
    PendingFileListConfirmation, PendingFileListUseWalkerConfirmation,
};
use crate::indexer::{ancestor_filelist_propagation_needed, find_filelist_in_first_level};
use std::path::PathBuf;
impl FlistWalkerApp {
    pub(in crate::app) fn filelist_entries_snapshot(&self) -> Vec<PathBuf> {
        let compiled_ignore_terms = self.shell.ui.ignore_list_enabled.then(|| {
            crate::query::CompiledIgnoreTerms::compile(
                self.shell.runtime.ignore_list_terms.as_slice(),
                self.shell.runtime.ignore_case,
            )
        });
        self.shell
            .runtime
            .all_entries
            .iter()
            .filter(|entry| {
                self.is_entry_visible_for_current_filter(entry, compiled_ignore_terms.as_ref())
            })
            .map(|entry| entry.path.clone())
            .collect()
    }

    pub(in crate::app) fn start_filelist_creation(
        &mut self,
        tab_id: u64,
        root: PathBuf,
        entries: Vec<PathBuf>,
        propagate_to_ancestors: bool,
    ) {
        let commands = self.shell.features.filelist.start_request_commands(
            tab_id,
            root,
            entries,
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
        if let Some(existing_path) = find_filelist_in_first_level(&root) {
            self.shell.features.filelist.workflow.pending_confirmation =
                Some(PendingFileListConfirmation {
                    tab_id,
                    root,
                    entries,
                    existing_path: existing_path.clone(),
                });
            self.set_notice(format!(
                "{} already exists. Choose overwrite or cancel.",
                existing_path.display()
            ));
            return;
        }
        self.request_filelist_creation_after_overwrite_check(tab_id, root, entries);
    }

    pub(in crate::app) fn request_filelist_creation_after_overwrite_check(
        &mut self,
        tab_id: u64,
        root: PathBuf,
        entries: Vec<PathBuf>,
    ) {
        if ancestor_filelist_propagation_needed(&root) {
            self.shell
                .features
                .filelist
                .workflow
                .pending_ancestor_confirmation = Some(PendingFileListAncestorConfirmation {
                tab_id,
                root,
                entries,
            });
            self.set_notice(
                "Create File List will also update parent FileList entries. Continue or choose current root only.",
            );
            return;
        }
        self.start_filelist_creation(tab_id, root, entries, false);
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

        let mut needs_reindex = false;
        if !self.shell.runtime.include_files || !self.shell.runtime.include_dirs {
            self.shell.runtime.include_files = true;
            self.shell.runtime.include_dirs = true;
            needs_reindex = true;
        }
        if !matches!(self.shell.runtime.index.source, IndexSource::Walker) {
            needs_reindex = true;
        }
        if self.shell.indexing.in_progress || self.shell.indexing.pending_finish.is_some() {
            self.shell.features.filelist.workflow.pending_after_index =
                Some(PendingFileListAfterIndex {
                    tab_id,
                    root: self.shell.runtime.root.clone(),
                });
            if needs_reindex {
                if self.shell.runtime.use_filelist {
                    self.request_create_filelist_walker_refresh();
                    self.set_notice(
                        "Preparing background Walker index with files/folders enabled before Create File List",
                    );
                } else {
                    self.request_index_refresh();
                    self.set_notice(
                        "Preparing Walker index with files/folders enabled before Create File List",
                    );
                }
            } else {
                self.set_notice("Waiting for current indexing to finish before Create File List");
            }
            return;
        }

        if needs_reindex {
            self.shell.features.filelist.workflow.pending_after_index =
                Some(PendingFileListAfterIndex {
                    tab_id,
                    root: self.shell.runtime.root.clone(),
                });
            if self.shell.runtime.use_filelist {
                self.request_create_filelist_walker_refresh();
                self.set_notice(
                    "Preparing background Walker index with files/folders enabled before Create File List",
                );
            } else {
                self.request_index_refresh();
                self.set_notice(
                    "Preparing Walker index with files/folders enabled before Create File List",
                );
            }
            return;
        }

        let entries = self.filelist_entries_snapshot();
        self.request_filelist_creation(tab_id, self.shell.runtime.root.clone(), entries);
    }
}

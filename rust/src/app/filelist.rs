use super::*;
use crate::app::state::{FileListResponseContext, FileListResponseScope};
use crate::path_utils::path_key;

// FileList reducer command surface. FileListManager owns the workflow state,
// and this module bridges those commands back into FlistWalkerApp.
#[allow(dead_code)]
pub(super) enum FileListUiCommand {
    RefreshStatusLine,
    SetNotice(String),
}

#[allow(dead_code)]
pub(super) enum FileListWorkerCommand {
    Start(FileListRequest),
}

#[allow(dead_code)]
pub(super) enum FileListAppCommand {
    SetPendingAfterIndex(Option<PendingFileListAfterIndex>),
    SetIncludeFilesAndDirs {
        include_files: bool,
        include_dirs: bool,
    },
    RequestIndexRefresh,
    RequestCreateFileListWalkerRefresh,
    RequestBackgroundIndexRefreshForTab(usize),
    SetUseFileListForTab {
        tab_index: usize,
        use_filelist: bool,
    },
}

#[allow(dead_code)]
pub(super) enum FileListCommand {
    Ui(FileListUiCommand),
    Worker(FileListWorkerCommand),
    App(FileListAppCommand),
}

impl FlistWalkerApp {
    fn dispatch_filelist_commands(&mut self, commands: Vec<FileListCommand>) {
        for command in commands {
            match command {
                FileListCommand::Ui(FileListUiCommand::RefreshStatusLine) => {
                    self.refresh_status_line();
                }
                FileListCommand::Ui(FileListUiCommand::SetNotice(notice)) => {
                    self.set_notice(notice);
                }
                FileListCommand::Worker(FileListWorkerCommand::Start(req)) => {
                    if self.shell.worker_bus.filelist.tx.send(req).is_err() {
                        let fallback = self.shell.features.filelist.send_failure_commands();
                        self.dispatch_filelist_commands(fallback);
                    }
                }
                FileListCommand::App(FileListAppCommand::SetPendingAfterIndex(pending)) => {
                    self.shell.features.filelist.workflow.pending_after_index = pending;
                }
                FileListCommand::App(FileListAppCommand::SetIncludeFilesAndDirs {
                    include_files,
                    include_dirs,
                }) => {
                    self.shell.runtime.include_files = include_files;
                    self.shell.runtime.include_dirs = include_dirs;
                }
                FileListCommand::App(FileListAppCommand::RequestIndexRefresh) => {
                    self.request_index_refresh();
                }
                FileListCommand::App(FileListAppCommand::RequestCreateFileListWalkerRefresh) => {
                    self.request_create_filelist_walker_refresh();
                }
                FileListCommand::App(FileListAppCommand::RequestBackgroundIndexRefreshForTab(
                    tab_index,
                )) => {
                    self.request_background_index_refresh_for_tab(tab_index);
                }
                FileListCommand::App(FileListAppCommand::SetUseFileListForTab {
                    tab_index,
                    use_filelist,
                }) => {
                    if let Some(tab) = self.shell.tabs.get_mut(tab_index) {
                        tab.use_filelist = use_filelist;
                    }
                    if tab_index == self.shell.tabs.active_tab_index() {
                        self.shell.runtime.use_filelist = use_filelist;
                    }
                }
            }
        }
    }

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

    pub(super) fn cancel_stale_pending_filelist_confirmations_for_active_root(&mut self) {
        self.cancel_stale_pending_filelist_confirmation();
        self.cancel_stale_pending_filelist_ancestor_confirmation();
        self.cancel_stale_pending_filelist_use_walker_confirmation();
    }

    fn resolve_filelist_target_tab_index(&self, tab_id: Option<u64>, root: &Path) -> Option<usize> {
        let tab_id = tab_id?;
        let tab_index = self.find_tab_index_by_id(tab_id)?;
        let tab_matches_root = self
            .shell
            .tabs
            .get(tab_index)
            .is_some_and(|tab| path_key(&tab.root) == path_key(root));
        tab_matches_root.then_some(tab_index)
    }

    fn handle_filelist_finished_response(
        &mut self,
        context: FileListResponseContext,
        root: PathBuf,
        path: PathBuf,
        count: usize,
    ) {
        if matches!(
            context.root_scope,
            FileListResponseScope::StaleRequestedRoot
        ) {
            return;
        }
        let target_tab_index = self.resolve_filelist_target_tab_index(context.tab_id, &root);
        if let Some(tab_index) = target_tab_index {
            self.dispatch_filelist_commands(vec![FileListCommand::App(
                FileListAppCommand::SetUseFileListForTab {
                    tab_index,
                    use_filelist: true,
                },
            )]);
        }

        match context.root_scope {
            FileListResponseScope::PreviousRoot => {
                self.set_notice(format!(
                    "Created {}: {} entries (previous root)",
                    path.display(),
                    count
                ));
                if let Some(tab_index) =
                    target_tab_index.filter(|index| *index != self.shell.tabs.active_tab_index())
                {
                    self.dispatch_filelist_commands(vec![FileListCommand::App(
                        FileListAppCommand::RequestBackgroundIndexRefreshForTab(tab_index),
                    )]);
                }
            }
            FileListResponseScope::CurrentRoot => {
                self.set_notice(format!("Created {}: {} entries", path.display(), count));
                if let Some(tab_index) = target_tab_index {
                    if tab_index == self.shell.tabs.active_tab_index()
                        && self.shell.runtime.use_filelist
                    {
                        self.dispatch_filelist_commands(vec![FileListCommand::App(
                            FileListAppCommand::RequestIndexRefresh,
                        )]);
                    } else if tab_index != self.shell.tabs.active_tab_index() {
                        self.dispatch_filelist_commands(vec![FileListCommand::App(
                            FileListAppCommand::RequestBackgroundIndexRefreshForTab(tab_index),
                        )]);
                    }
                }
            }
            FileListResponseScope::StaleRequestedRoot => {}
        }
    }

    fn handle_filelist_failed_response(&mut self, context: FileListResponseContext, error: String) {
        match context.root_scope {
            FileListResponseScope::StaleRequestedRoot => {}
            FileListResponseScope::PreviousRoot => {
                self.set_notice(format!(
                    "Create File List failed for previous root: {}",
                    error
                ));
            }
            FileListResponseScope::CurrentRoot => {
                self.set_notice(format!("Create File List failed: {}", error));
            }
        }
    }

    fn handle_filelist_canceled_response(&mut self, context: FileListResponseContext) {
        if !matches!(
            context.root_scope,
            FileListResponseScope::StaleRequestedRoot
        ) {
            self.set_notice("Create File List canceled");
        }
    }

    pub(super) fn filelist_entries_snapshot(&self) -> Vec<PathBuf> {
        self.shell
            .runtime
            .all_entries
            .iter()
            .filter(|entry| self.is_entry_visible_for_current_filter(entry))
            .map(|entry| entry.path.clone())
            .collect()
    }

    pub(super) fn start_filelist_creation(
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

    pub(super) fn request_filelist_creation(
        &mut self,
        tab_id: u64,
        root: PathBuf,
        entries: Vec<PathBuf>,
    ) {
        if let Some(existing_path) = find_filelist_in_first_level(&root) {
            self.shell.features.filelist.workflow.pending_confirmation = Some(
                PendingFileListConfirmation {
                tab_id,
                root,
                entries,
                existing_path: existing_path.clone(),
            },
            );
            self.set_notice(format!(
                "{} already exists. Choose overwrite or cancel.",
                existing_path.display()
            ));
            return;
        }
        self.request_filelist_creation_after_overwrite_check(tab_id, root, entries);
    }

    pub(super) fn request_filelist_creation_after_overwrite_check(
        &mut self,
        tab_id: u64,
        root: PathBuf,
        entries: Vec<PathBuf>,
    ) {
        if has_ancestor_filelists(&root) {
            self.shell.features.filelist.workflow.pending_ancestor_confirmation = Some(
                PendingFileListAncestorConfirmation {
                    tab_id,
                    root,
                    entries,
                },
            );
            self.set_notice(
                "Create File List will also update parent FileList entries. Continue or choose current root only.",
            );
            return;
        }
        self.start_filelist_creation(tab_id, root, entries, false);
    }

    pub(super) fn confirm_pending_filelist_overwrite(&mut self) {
        let Some(pending) = self
            .shell
            .features
            .filelist
            .workflow
            .pending_confirmation
            .take() else {
            return;
        };
        self.request_filelist_creation_after_overwrite_check(
            pending.tab_id,
            pending.root,
            pending.entries,
        );
    }

    pub(super) fn confirm_pending_filelist_ancestor_propagation(&mut self) {
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

    pub(super) fn skip_pending_filelist_ancestor_propagation(&mut self) {
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

    pub(super) fn confirm_pending_filelist_use_walker(&mut self) {
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

    pub(super) fn cancel_pending_filelist_overwrite(&mut self) {
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

    pub(super) fn cancel_pending_filelist_ancestor_confirmation(&mut self) {
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

    pub(super) fn cancel_pending_filelist_use_walker(&mut self) {
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

    pub(super) fn can_cancel_create_filelist(&self) -> bool {
        self.shell.features.filelist.workflow.pending_after_index.is_some()
            || self.shell.features.filelist.workflow.pending_confirmation.is_some()
            || self
                .shell
                .features
                .filelist
                .pending_ancestor_confirmation
                .is_some()
            || self
                .shell
                .features
                .filelist
                .pending_use_walker_confirmation
                .is_some()
            || self.shell.features.filelist.workflow.in_progress
    }

    pub(super) fn cancel_create_filelist(&mut self) {
        if self.shell.features.filelist.workflow.pending_confirmation.is_some() {
            self.cancel_pending_filelist_overwrite();
            return;
        }
        if self
            .shell
            .features
            .filelist
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

    pub(super) fn create_filelist(&mut self) {
        if self.shell.features.filelist.workflow.in_progress {
            self.set_notice("Create File List is already running");
            return;
        }
        if self.shell.features.filelist.workflow.pending_confirmation.is_some() {
            self.set_notice("Confirm overwrite or cancel first");
            return;
        }
        if self
            .shell
            .features
            .filelist
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
            self.shell.features.filelist.workflow.pending_use_walker_confirmation =
                Some(PendingFileListUseWalkerConfirmation {
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
        if self.shell.indexing.in_progress {
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

    pub(super) fn poll_filelist_response(&mut self) {
        let current_root = self.shell.runtime.root.clone();
        while let Ok(response) = self.shell.worker_bus.filelist.rx.try_recv() {
            match response {
                FileListResponse::Finished {
                    request_id,
                    root,
                    path,
                    count,
                } => {
                    let Some((context, commands)) = self
                        .shell
                        .features
                        .filelist
                        .settle_response_context_commands(request_id, &root, &current_root)
                    else {
                        continue;
                    };
                    self.dispatch_filelist_commands(commands);
                    self.handle_filelist_finished_response(context, root, path, count);
                }
                FileListResponse::Failed {
                    request_id,
                    root,
                    error,
                } => {
                    let Some((context, commands)) = self
                        .shell
                        .features
                        .filelist
                        .settle_response_context_commands(request_id, &root, &current_root)
                    else {
                        continue;
                    };
                    self.dispatch_filelist_commands(commands);
                    self.handle_filelist_failed_response(context, error);
                }
                FileListResponse::Canceled { request_id, root } => {
                    let Some((context, commands)) = self
                        .shell
                        .features
                        .filelist
                        .settle_response_context_commands(request_id, &root, &current_root)
                    else {
                        continue;
                    };
                    self.dispatch_filelist_commands(commands);
                    self.handle_filelist_canceled_response(context);
                }
            }
        }
    }
}

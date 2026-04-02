use super::*;

impl FlistWalkerApp {
    pub(super) fn cancel_stale_pending_filelist_confirmation(&mut self) {
        let current_tab_id = self.current_tab_id().unwrap_or_default();
        let should_cancel = self
            .filelist_state
            .pending_confirmation
            .as_ref()
            .is_some_and(|pending| {
                pending.tab_id == current_tab_id
                    && Self::path_key(&pending.root) != Self::path_key(&self.root)
            });
        if should_cancel {
            self.filelist_state.pending_confirmation = None;
            self.set_notice("Pending FileList overwrite canceled because root changed");
        }
    }

    pub(super) fn cancel_stale_pending_filelist_ancestor_confirmation(&mut self) {
        let current_tab_id = self.current_tab_id().unwrap_or_default();
        let should_cancel = self
            .filelist_state
            .pending_ancestor_confirmation
            .as_ref()
            .is_some_and(|pending| {
                pending.tab_id == current_tab_id
                    && Self::path_key(&pending.root) != Self::path_key(&self.root)
            });
        if should_cancel {
            self.filelist_state.pending_ancestor_confirmation = None;
            self.set_notice(
                "Pending Create File List ancestor update canceled because root changed",
            );
        }
    }

    pub(super) fn cancel_stale_pending_filelist_use_walker_confirmation(&mut self) {
        let current_tab_id = self.current_tab_id().unwrap_or_default();
        let should_cancel = self
            .filelist_state
            .pending_use_walker_confirmation
            .as_ref()
            .is_some_and(|pending| {
                pending.source_tab_id == current_tab_id
                    && Self::path_key(&pending.root) != Self::path_key(&self.root)
            });
        if should_cancel {
            self.filelist_state.pending_use_walker_confirmation = None;
            self.set_notice("Pending Create File List confirmation canceled because root changed");
        }
    }

    pub(super) fn filelist_entries_snapshot(&self) -> Vec<PathBuf> {
        self.all_entries
            .iter()
            .filter(|path| self.is_entry_visible_for_current_filter(path))
            .cloned()
            .collect()
    }

    pub(super) fn start_filelist_creation(
        &mut self,
        tab_id: u64,
        root: PathBuf,
        entries: Vec<PathBuf>,
        propagate_to_ancestors: bool,
    ) {
        self.filelist_state.pending_after_index = None;
        let cancel = Arc::new(AtomicBool::new(false));
        let request_id = self.filelist_state.next_request_id;
        self.filelist_state.next_request_id = self.filelist_state.next_request_id.saturating_add(1);
        self.filelist_state.pending_request_id = Some(request_id);
        self.filelist_state.pending_request_tab_id = Some(tab_id);
        self.filelist_state.pending_root = Some(root.clone());
        self.filelist_state.pending_cancel = Some(Arc::clone(&cancel));
        self.filelist_state.in_progress = true;
        self.filelist_state.cancel_requested = false;
        self.refresh_status_line();

        let req = FileListRequest {
            request_id,
            tab_id,
            root,
            entries,
            propagate_to_ancestors,
            cancel,
        };
        if self.worker_bus.filelist.tx.send(req).is_err() {
            self.filelist_state.pending_request_id = None;
            self.filelist_state.pending_request_tab_id = None;
            self.filelist_state.pending_root = None;
            self.filelist_state.pending_cancel = None;
            self.filelist_state.in_progress = false;
            self.filelist_state.cancel_requested = false;
            self.refresh_status_line();
            self.set_notice("Create File List worker is unavailable");
        }
    }

    pub(super) fn request_filelist_creation(
        &mut self,
        tab_id: u64,
        root: PathBuf,
        entries: Vec<PathBuf>,
    ) {
        if let Some(existing_path) = find_filelist_in_first_level(&root) {
            self.filelist_state.pending_confirmation = Some(PendingFileListConfirmation {
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

    pub(super) fn request_filelist_creation_after_overwrite_check(
        &mut self,
        tab_id: u64,
        root: PathBuf,
        entries: Vec<PathBuf>,
    ) {
        if has_ancestor_filelists(&root) {
            self.filelist_state.pending_ancestor_confirmation =
                Some(PendingFileListAncestorConfirmation {
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

    pub(super) fn confirm_pending_filelist_overwrite(&mut self) {
        let Some(pending) = self.filelist_state.pending_confirmation.take() else {
            return;
        };
        self.request_filelist_creation_after_overwrite_check(
            pending.tab_id,
            pending.root,
            pending.entries,
        );
    }

    pub(super) fn confirm_pending_filelist_ancestor_propagation(&mut self) {
        let Some(pending) = self.filelist_state.pending_ancestor_confirmation.take() else {
            return;
        };
        self.start_filelist_creation(pending.tab_id, pending.root, pending.entries, true);
    }

    pub(super) fn skip_pending_filelist_ancestor_propagation(&mut self) {
        let Some(pending) = self.filelist_state.pending_ancestor_confirmation.take() else {
            return;
        };
        self.start_filelist_creation(pending.tab_id, pending.root, pending.entries, false);
    }

    pub(super) fn confirm_pending_filelist_use_walker(&mut self) {
        let Some(pending) = self.filelist_state.pending_use_walker_confirmation.take() else {
            return;
        };
        self.filelist_state.pending_after_index = Some(PendingFileListAfterIndex {
            tab_id: pending.source_tab_id,
            root: pending.root,
        });
        if !self.include_files || !self.include_dirs {
            self.include_files = true;
            self.include_dirs = true;
        }
        self.request_create_filelist_walker_refresh();
        self.set_notice("Preparing background Walker index for Create File List");
    }

    pub(super) fn cancel_pending_filelist_overwrite(&mut self) {
        if self.filelist_state.pending_confirmation.take().is_some() {
            self.set_notice("Create File List canceled");
        }
    }

    pub(super) fn cancel_pending_filelist_ancestor_confirmation(&mut self) {
        if self
            .filelist_state
            .pending_ancestor_confirmation
            .take()
            .is_some()
        {
            self.set_notice("Create File List canceled");
        }
    }

    pub(super) fn cancel_pending_filelist_use_walker(&mut self) {
        if self
            .filelist_state
            .pending_use_walker_confirmation
            .take()
            .is_some()
        {
            self.set_notice("Create File List canceled");
        }
    }

    pub(super) fn can_cancel_create_filelist(&self) -> bool {
        self.filelist_state.pending_after_index.is_some()
            || self.filelist_state.pending_confirmation.is_some()
            || self.filelist_state.pending_ancestor_confirmation.is_some()
            || self
                .filelist_state
                .pending_use_walker_confirmation
                .is_some()
            || self.filelist_state.in_progress
    }

    pub(super) fn cancel_create_filelist(&mut self) {
        if self.filelist_state.pending_confirmation.is_some() {
            self.cancel_pending_filelist_overwrite();
            return;
        }
        if self.filelist_state.pending_ancestor_confirmation.is_some() {
            self.cancel_pending_filelist_ancestor_confirmation();
            return;
        }
        if self
            .filelist_state
            .pending_use_walker_confirmation
            .is_some()
        {
            self.cancel_pending_filelist_use_walker();
            return;
        }
        if self.filelist_state.pending_after_index.take().is_some() {
            self.set_notice("Create File List canceled");
            return;
        }
        if self.filelist_state.in_progress && !self.filelist_state.cancel_requested {
            if let Some(cancel) = self.filelist_state.pending_cancel.as_ref() {
                cancel.store(true, Ordering::Relaxed);
            }
            self.filelist_state.cancel_requested = true;
            self.refresh_status_line();
            self.set_notice("Canceling Create File List...");
        }
    }

    pub(super) fn create_filelist(&mut self) {
        if self.filelist_state.in_progress {
            self.set_notice("Create File List is already running");
            return;
        }
        if self.filelist_state.pending_confirmation.is_some() {
            self.set_notice("Confirm overwrite or cancel first");
            return;
        }
        if self.filelist_state.pending_ancestor_confirmation.is_some() {
            self.set_notice("Confirm ancestor FileList update choice or cancel first");
            return;
        }
        if self
            .filelist_state
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
            self.filelist_state.pending_use_walker_confirmation =
                Some(PendingFileListUseWalkerConfirmation {
                    source_tab_id: tab_id,
                    root: self.root.clone(),
                });
            self.set_notice("Confirmation required: Create File List needs Walker indexing");
            return;
        }

        let mut needs_reindex = false;
        if !self.include_files || !self.include_dirs {
            self.include_files = true;
            self.include_dirs = true;
            needs_reindex = true;
        }
        if !matches!(self.index.source, IndexSource::Walker) {
            needs_reindex = true;
        }
        if self.indexing.in_progress {
            self.filelist_state.pending_after_index = Some(PendingFileListAfterIndex {
                tab_id,
                root: self.root.clone(),
            });
            if needs_reindex {
                if self.use_filelist {
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
            self.filelist_state.pending_after_index = Some(PendingFileListAfterIndex {
                tab_id,
                root: self.root.clone(),
            });
            if self.use_filelist {
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
        self.request_filelist_creation(tab_id, self.root.clone(), entries);
    }

    pub(super) fn poll_filelist_response(&mut self) {
        while let Ok(response) = self.worker_bus.filelist.rx.try_recv() {
            let Some(pending) = self.filelist_state.pending_request_id else {
                continue;
            };
            let requested_root = self.filelist_state.pending_root.clone();
            let requested_tab_id = self.filelist_state.pending_request_tab_id;
            match response {
                FileListResponse::Finished {
                    request_id,
                    root,
                    path,
                    count,
                } => {
                    if request_id != pending {
                        continue;
                    }
                    self.filelist_state.pending_request_id = None;
                    self.filelist_state.pending_request_tab_id = None;
                    self.filelist_state.pending_root = None;
                    self.filelist_state.pending_cancel = None;
                    self.filelist_state.in_progress = false;
                    self.filelist_state.cancel_requested = false;
                    self.refresh_status_line();

                    let same_requested_root = requested_root
                        .as_ref()
                        .map(|r| Self::path_key(r) == Self::path_key(&root))
                        .unwrap_or(true);
                    let same_current_root = Self::path_key(&self.root) == Self::path_key(&root);

                    if !same_requested_root {
                        continue;
                    }
                    let mut target_tab_index = None;
                    if let Some(tab_id) = requested_tab_id {
                        if let Some(tab_index) = self.find_tab_index_by_id(tab_id) {
                            let tab_matches_root = self.tabs.get(tab_index).is_some_and(|tab| {
                                Self::path_key(&tab.root) == Self::path_key(&root)
                            });
                            if tab_matches_root {
                                if let Some(tab) = self.tabs.get_mut(tab_index) {
                                    tab.use_filelist = true;
                                }
                                if tab_index == self.active_tab {
                                    self.use_filelist = true;
                                }
                                target_tab_index = Some(tab_index);
                            }
                        }
                    }
                    if !same_current_root {
                        self.set_notice(format!(
                            "Created {}: {} entries (previous root)",
                            path.display(),
                            count
                        ));
                        if let Some(tab_index) = target_tab_index {
                            if tab_index != self.active_tab {
                                self.request_background_index_refresh_for_tab(tab_index);
                            }
                        }
                        continue;
                    }

                    self.set_notice(format!("Created {}: {} entries", path.display(), count));
                    if let Some(tab_index) = target_tab_index {
                        if tab_index == self.active_tab && self.use_filelist {
                            self.request_index_refresh();
                        } else if tab_index != self.active_tab {
                            self.request_background_index_refresh_for_tab(tab_index);
                        }
                    }
                }
                FileListResponse::Failed {
                    request_id,
                    root,
                    error,
                } => {
                    if request_id != pending {
                        continue;
                    }
                    self.filelist_state.pending_request_id = None;
                    self.filelist_state.pending_request_tab_id = None;
                    self.filelist_state.pending_root = None;
                    self.filelist_state.pending_cancel = None;
                    self.filelist_state.in_progress = false;
                    self.filelist_state.cancel_requested = false;
                    self.refresh_status_line();

                    let same_requested_root = requested_root
                        .as_ref()
                        .map(|r| Self::path_key(r) == Self::path_key(&root))
                        .unwrap_or(true);
                    let same_current_root = Self::path_key(&self.root) == Self::path_key(&root);
                    if !same_requested_root || !same_current_root {
                        self.set_notice(format!(
                            "Create File List failed for previous root: {}",
                            error
                        ));
                        continue;
                    }

                    self.set_notice(format!("Create File List failed: {}", error));
                }
                FileListResponse::Canceled { request_id, root } => {
                    if request_id != pending {
                        continue;
                    }
                    self.filelist_state.pending_request_id = None;
                    self.filelist_state.pending_request_tab_id = None;
                    self.filelist_state.pending_root = None;
                    self.filelist_state.pending_cancel = None;
                    self.filelist_state.in_progress = false;
                    self.filelist_state.cancel_requested = false;
                    self.refresh_status_line();

                    let same_requested_root = requested_root
                        .as_ref()
                        .map(|r| Self::path_key(r) == Self::path_key(&root))
                        .unwrap_or(true);
                    if !same_requested_root {
                        continue;
                    }

                    self.set_notice("Create File List canceled");
                }
            }
        }
    }
}

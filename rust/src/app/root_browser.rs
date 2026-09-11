#![allow(deprecated)]

use super::session::{enqueue_settings_commit, SettingsCommitRequest, UiStatePatch};
use super::{FlistWalkerApp, PendingSettingsCommit, PendingSettingsOperation};
use crate::path_utils::normalize_windows_path_buf;
use crate::path_utils::path_key;
use eframe::egui;
use std::path::{Path, PathBuf};
use std::sync::mpsc::TryRecvError;

impl FlistWalkerApp {
    /// ダイアログで選んだ root を現在 tab に適用する。
    pub(super) fn browse_for_root(&mut self) {
        let dialog_root = Self::browse_dialog_start_location(&self.shell.runtime.root);
        match self.select_root_via_dialog(&dialog_root) {
            Ok(Some(dir)) => self.apply_root_change(dir),
            Ok(None) => {}
            Err(err) => self.set_notice(format!("Browse failed: {}", err)),
        }
    }

    /// ダイアログで選んだ root を新規 tab として開く。
    pub(super) fn browse_for_root_in_new_tab(&mut self) {
        let dialog_root = Self::browse_dialog_start_location(&self.shell.runtime.root);
        match self.select_root_via_dialog(&dialog_root) {
            Ok(Some(dir)) => {
                self.create_new_tab();
                self.apply_root_change(dir);
            }
            Ok(None) => {}
            Err(err) => self.set_notice(format!("Browse failed: {}", err)),
        }
    }

    pub(super) fn select_root_for_path_input(
        &mut self,
        input: &str,
    ) -> Result<Option<PathBuf>, String> {
        let input = input.trim();
        let start = if input.is_empty() {
            Self::browse_dialog_start_location(&self.shell.runtime.root)
        } else {
            Self::browse_dialog_start_location(Path::new(input))
        };
        self.select_root_via_dialog(&start)
    }

    pub(super) fn open_manage_root_list(&mut self) {
        let root_browser = &mut self.shell.features.root_browser;
        root_browser.manage_list.dialog_generation =
            root_browser.manage_list.dialog_generation.saturating_add(1);
        root_browser.manage_list.open = true;
        root_browser.manage_list.draft_roots = root_browser.saved_roots.clone();
        root_browser.manage_list.draft_default_root = root_browser.default_root.clone();
        root_browser.manage_list.selected_index = None;
        root_browser.manage_list.selected_indices.clear();
        root_browser.manage_list.remove_mode = false;
        root_browser.manage_list.editing_index = None;
        root_browser.manage_list.edit_path.clear();
        root_browser.manage_list.edit_error.clear();
        root_browser.manage_list.edit_focus_requested = false;
        root_browser.manage_list.edit_select_all_requested = false;
        root_browser.manage_list.input_path =
            normalize_windows_path_buf(self.shell.runtime.root.clone())
                .to_string_lossy()
                .to_string();
        root_browser.manage_list.add_error.clear();
        root_browser.manage_list.add_focus_requested = false;
        root_browser.manage_list.add_select_all_requested = false;
        root_browser.manage_list.notice.clear();
        root_browser.manage_list.pending_validation_intent = None;
        self.shell.worker_bus.root_validation.clear_request();
        self.clear_focus_query_request();
        self.request_unfocus_query();
    }

    pub(super) fn add_manage_root_list_input(&mut self) {
        if self.reject_manage_root_list_mutation_while_saving() {
            return;
        }
        let input = self
            .shell
            .features
            .root_browser
            .manage_list
            .input_path
            .trim()
            .to_string();
        self.request_manage_root_validation(super::RootValidationIntent::Add, input);
    }

    pub(super) fn clear_manage_root_list_add_error(&mut self) {
        self.cancel_manage_root_validation();
        let manage = &mut self.shell.features.root_browser.manage_list;
        manage.add_error.clear();
        manage.add_focus_requested = false;
        manage.add_select_all_requested = false;
    }

    pub(super) fn clear_manage_root_list_edit_error(&mut self) {
        self.cancel_manage_root_validation();
        let manage = &mut self.shell.features.root_browser.manage_list;
        manage.edit_error.clear();
        manage.edit_focus_requested = false;
        manage.edit_select_all_requested = false;
    }

    pub(super) fn select_manage_root_list_item(&mut self, index: usize) -> bool {
        if self.reject_manage_root_list_mutation_while_saving() {
            return false;
        }
        self.cancel_manage_root_validation();
        let manage = &mut self.shell.features.root_browser.manage_list;
        if manage.remove_mode || index >= manage.draft_roots.len() {
            return false;
        }
        if manage.editing_index.is_some() && manage.editing_index != Some(index) {
            let edit_is_dirty = match manage.editing_index {
                Some(editing_index) => match manage.draft_roots.get(editing_index) {
                    Some(root) => manage.edit_path != root.to_string_lossy(),
                    None => true,
                },
                None => false,
            };
            if edit_is_dirty {
                manage.notice =
                    "Save or Cancel the current edit before selecting another root".to_string();
                return false;
            }
            manage.editing_index = None;
            manage.edit_path.clear();
            manage.edit_error.clear();
            manage.edit_focus_requested = false;
            manage.edit_select_all_requested = false;
        }
        manage.selected_index = Some(index);
        manage.notice.clear();
        true
    }

    pub(super) fn start_editing_manage_root_list_item(&mut self) {
        if self.reject_manage_root_list_mutation_while_saving() {
            return;
        }
        self.cancel_manage_root_validation();
        let manage = &mut self.shell.features.root_browser.manage_list;
        let Some(index) = manage.selected_index else {
            manage.notice = "Select a root to edit".to_string();
            return;
        };
        let Some(root) = manage.draft_roots.get(index) else {
            manage.selected_index = None;
            manage.notice = "Select a root to edit".to_string();
            return;
        };
        manage.edit_path = root.to_string_lossy().to_string();
        manage.editing_index = Some(index);
        manage.edit_error.clear();
        manage.edit_focus_requested = true;
        manage.edit_select_all_requested = true;
        manage.notice.clear();
    }

    pub(super) fn cancel_manage_root_list_edit(&mut self) {
        if self.reject_manage_root_list_mutation_while_saving() {
            return;
        }
        self.cancel_manage_root_validation();
        let manage = &mut self.shell.features.root_browser.manage_list;
        manage.editing_index = None;
        manage.edit_path.clear();
        manage.edit_error.clear();
        manage.edit_focus_requested = false;
        manage.edit_select_all_requested = false;
        manage.notice.clear();
    }

    pub(super) fn save_manage_root_list_edit(&mut self) {
        if self.reject_manage_root_list_mutation_while_saving() {
            return;
        }
        let (index, input) = {
            let manage = &self.shell.features.root_browser.manage_list;
            let Some(index) = manage.editing_index else {
                return;
            };
            (index, manage.edit_path.trim().to_string())
        };
        self.request_manage_root_validation(super::RootValidationIntent::Edit { index }, input);
    }

    fn apply_validated_manage_root_edit(
        &mut self,
        index: usize,
        replacement: PathBuf,
        replacement_key: String,
    ) {
        let manage = &mut self.shell.features.root_browser.manage_list;
        let Some(original) = manage.draft_roots.get(index).cloned() else {
            manage.editing_index = None;
            manage.edit_path.clear();
            manage.edit_error.clear();
            manage.edit_focus_requested = false;
            manage.edit_select_all_requested = false;
            manage.selected_index = None;
            manage.notice = "The selected root is no longer available".to_string();
            return;
        };
        if manage
            .draft_default_root
            .as_ref()
            .is_some_and(|default_root| {
                Self::manage_root_list_path_key(default_root)
                    == Self::manage_root_list_path_key(&original)
            })
        {
            manage.draft_default_root = Some(replacement.clone());
        }
        manage.draft_roots[index] = replacement.clone();
        manage
            .draft_roots
            .sort_by_key(|p| p.to_string_lossy().to_string().to_ascii_lowercase());
        manage.selected_index = manage
            .draft_roots
            .iter()
            .position(|candidate| Self::manage_root_list_path_key(candidate) == replacement_key);
        manage.editing_index = None;
        manage.edit_path.clear();
        manage.edit_error.clear();
        manage.edit_focus_requested = false;
        manage.edit_select_all_requested = false;
        manage.notice = format!("Updated root in draft list: {}", replacement.display());
    }

    pub(super) fn enter_manage_root_list_remove_mode(&mut self) {
        if self.reject_manage_root_list_mutation_while_saving() {
            return;
        }
        self.cancel_manage_root_validation();
        let manage = &mut self.shell.features.root_browser.manage_list;
        if manage.draft_roots.is_empty() {
            manage.notice = "There are no roots to remove".to_string();
            return;
        }
        manage.remove_mode = true;
        manage.selected_index = None;
        manage.selected_indices.clear();
        manage.editing_index = None;
        manage.edit_path.clear();
        manage.edit_error.clear();
        manage.edit_focus_requested = false;
        manage.edit_select_all_requested = false;
        manage.notice = "Select one or more roots to remove".to_string();
    }

    pub(super) fn cancel_manage_root_list_remove_mode(&mut self) {
        if self.reject_manage_root_list_mutation_while_saving() {
            return;
        }
        let manage = &mut self.shell.features.root_browser.manage_list;
        manage.remove_mode = false;
        manage.selected_indices.clear();
        manage.notice.clear();
    }

    pub(super) fn browse_for_manage_root_list(&mut self) {
        if self.reject_manage_root_list_mutation_while_saving() {
            return;
        }
        let input = self
            .shell
            .features
            .root_browser
            .manage_list
            .input_path
            .clone();
        match self.select_root_for_path_input(&input) {
            Ok(Some(dir)) => {
                let root = normalize_windows_path_buf(dir);
                self.shell.features.root_browser.manage_list.input_path =
                    root.to_string_lossy().to_string();
                self.request_manage_root_validation(
                    super::RootValidationIntent::Add,
                    root.to_string_lossy().to_string(),
                );
            }
            Ok(None) => {}
            Err(err) => {
                self.shell.features.root_browser.manage_list.notice =
                    format!("Browse failed: {}", err);
            }
        }
    }

    pub(super) fn remove_selected_manage_root_list_items(&mut self) {
        if self.reject_manage_root_list_mutation_while_saving() {
            return;
        }
        self.cancel_manage_root_validation();
        let manage = &mut self.shell.features.root_browser.manage_list;
        if manage.selected_indices.is_empty() {
            manage.notice = "Select one or more roots to remove".to_string();
            return;
        }
        let selected = &manage.selected_indices;
        if manage
            .draft_default_root
            .as_ref()
            .is_some_and(|default_root| {
                let default_key = Self::manage_root_list_path_key(default_root);
                manage.draft_roots.iter().enumerate().any(|(index, root)| {
                    selected.contains(&index)
                        && Self::manage_root_list_path_key(root) == default_key
                })
            })
        {
            manage.draft_default_root = None;
        }
        manage.draft_roots = manage
            .draft_roots
            .iter()
            .cloned()
            .enumerate()
            .filter_map(|(index, root)| (!selected.contains(&index)).then_some(root))
            .collect();
        manage.selected_indices.clear();
        manage.selected_index = None;
        manage.remove_mode = false;
        manage.notice = "Removed selected roots from the draft list".to_string();
    }

    pub(super) fn apply_manage_root_list_changes(&mut self) {
        if self.shell.worker_bus.root_validation.in_progress {
            self.shell.features.root_browser.manage_list.notice =
                "Wait for folder validation to finish".to_string();
            return;
        }
        if self
            .shell
            .features
            .root_browser
            .pending_settings_commit
            .is_some()
        {
            self.shell.features.root_browser.manage_list.notice =
                "Wait for settings save to finish".to_string();
            return;
        }
        let (draft_roots, draft_default_root) = {
            let manage = &self.shell.features.root_browser.manage_list;
            (
                manage.draft_roots.clone(),
                manage.draft_default_root.clone(),
            )
        };
        self.start_root_list_settings_commit(draft_roots, draft_default_root, false);
    }

    pub(super) fn confirm_manage_root_list_changes(&mut self) {
        if self.shell.worker_bus.root_validation.in_progress {
            self.shell.features.root_browser.manage_list.notice =
                "Wait for folder validation to finish".to_string();
            return;
        }
        if self
            .shell
            .features
            .root_browser
            .pending_settings_commit
            .is_some()
        {
            self.shell.features.root_browser.manage_list.notice =
                "Wait for settings save to finish".to_string();
            return;
        }
        let (draft_roots, draft_default_root) = {
            let manage = &self.shell.features.root_browser.manage_list;
            (
                manage.draft_roots.clone(),
                manage.draft_default_root.clone(),
            )
        };
        self.start_root_list_settings_commit(draft_roots, draft_default_root, true);
    }

    pub(super) fn cancel_manage_root_list(&mut self) {
        if matches!(
            self.shell
                .features
                .root_browser
                .pending_settings_commit
                .as_ref()
                .map(|pending| &pending.operation),
            Some(PendingSettingsOperation::RootList { .. })
        ) {
            self.shell.features.root_browser.manage_list.notice =
                "Wait for settings save to finish".to_string();
            return;
        }
        self.close_manage_root_list();
        self.set_notice("Canceled saved roots list changes");
    }

    fn reject_manage_root_list_mutation_while_saving(&mut self) -> bool {
        if !self.settings_commit_in_progress() {
            return false;
        }
        self.shell.features.root_browser.manage_list.notice =
            "Wait for settings save to finish".to_string();
        true
    }

    fn allocate_settings_request_id(&mut self) -> u64 {
        let request_id = self.shell.features.root_browser.next_settings_request_id;
        self.shell.features.root_browser.next_settings_request_id = request_id.saturating_add(1);
        request_id
    }

    fn settings_patch_for_default_root(&self, default_root: Option<&Path>) -> UiStatePatch {
        let default_text = default_root.map(|path| path.to_string_lossy().to_string());
        let last_root = if Self::restore_tabs_enabled() {
            &self.shell.runtime.root
        } else {
            default_root.unwrap_or(&self.shell.runtime.root)
        };
        UiStatePatch::from_json(serde_json::json!({
            "default_root": default_text,
            "last_root": last_root.to_string_lossy().to_string(),
        }))
    }

    fn saved_roots_text(roots: &[PathBuf]) -> String {
        let text = roots
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("\n");
        if text.is_empty() {
            String::new()
        } else {
            format!("{text}\n")
        }
    }

    fn start_root_list_settings_commit(
        &mut self,
        roots: Vec<PathBuf>,
        default_root: Option<PathBuf>,
        close_on_success: bool,
    ) {
        let Some(ui_state_path) = Self::ui_state_file_path() else {
            self.apply_settings_commit_start_error("Settings path is unavailable");
            return;
        };
        let Some(saved_roots_path) = Self::saved_roots_file_path() else {
            self.apply_settings_commit_start_error("Saved-roots path is unavailable");
            return;
        };
        let request_id = self.allocate_settings_request_id();
        let request = SettingsCommitRequest {
            request_id,
            patch: self.settings_patch_for_default_root(default_root.as_deref()),
            saved_roots: Some((saved_roots_path, Self::saved_roots_text(&roots))),
        };
        match enqueue_settings_commit(ui_state_path, Self::history_persist_disabled(), request) {
            Ok(response) => {
                self.shell.features.root_browser.pending_settings_commit =
                    Some(PendingSettingsCommit {
                        request_id,
                        response,
                        operation: PendingSettingsOperation::RootList {
                            roots,
                            default_root,
                            close_on_success,
                        },
                    });
                self.shell.features.root_browser.manage_list.notice =
                    "Saving saved roots...".to_string();
                self.set_notice("Saving saved roots...");
            }
            Err(error) => self.apply_settings_commit_start_error(&error),
        }
    }

    fn apply_settings_commit_start_error(&mut self, error: &str) {
        let notice = format!("Couldn't save settings. {error}");
        if self.shell.features.root_browser.manage_list.open {
            self.shell.features.root_browser.manage_list.notice = notice.clone();
        }
        self.set_notice(notice);
    }

    pub(super) fn start_default_root_settings_commit(&mut self, root: PathBuf) {
        if self
            .shell
            .features
            .root_browser
            .pending_settings_commit
            .is_some()
        {
            self.set_notice("Wait for settings save to finish");
            return;
        }
        let Some(ui_state_path) = Self::ui_state_file_path() else {
            self.apply_settings_commit_start_error("Settings path is unavailable");
            return;
        };
        let request_id = self.allocate_settings_request_id();
        let request = SettingsCommitRequest {
            request_id,
            patch: self.settings_patch_for_default_root(Some(&root)),
            saved_roots: None,
        };
        match enqueue_settings_commit(ui_state_path, Self::history_persist_disabled(), request) {
            Ok(response) => {
                self.shell.features.root_browser.pending_settings_commit =
                    Some(PendingSettingsCommit {
                        request_id,
                        response,
                        operation: PendingSettingsOperation::DefaultRoot,
                    });
                self.set_notice("Saving default root...");
            }
            Err(error) => self.apply_settings_commit_start_error(&error),
        }
    }

    pub(super) fn settings_commit_in_progress(&self) -> bool {
        self.shell
            .features
            .root_browser
            .pending_settings_commit
            .is_some()
    }

    pub(super) fn poll_settings_commit_response(&mut self) {
        let result = {
            let Some(pending) = self
                .shell
                .features
                .root_browser
                .pending_settings_commit
                .as_ref()
            else {
                return;
            };
            match pending.response.try_recv() {
                Ok(response) => Some(Ok(response)),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err(
                    "Settings persistence worker disconnected unexpectedly".to_string(),
                )),
            }
        };
        let Some(result) = result else {
            return;
        };
        let pending = self
            .shell
            .features
            .root_browser
            .pending_settings_commit
            .take()
            .expect("pending settings commit");
        let response_result = match result {
            Ok(response) if response.request_id == pending.request_id => response.result,
            Ok(response) => Err(format!(
                "Settings persistence response mismatch: expected request {}, received {}",
                pending.request_id, response.request_id
            )),
            Err(error) => Err(error),
        };
        match (pending.operation, response_result) {
            (
                PendingSettingsOperation::RootList {
                    roots,
                    default_root,
                    close_on_success,
                },
                Ok(receipt),
            ) => {
                self.shell.features.root_browser.saved_roots = roots;
                self.shell.features.root_browser.default_root = if default_root.is_some() {
                    receipt.canonical_default_root.or(default_root)
                } else {
                    None
                };
                self.shell.ui.set_root_dropdown_highlight(None);
                self.shell.features.root_browser.manage_list.notice =
                    "Applied saved roots list".to_string();
                self.set_notice("Applied saved roots list");
                if close_on_success {
                    self.close_manage_root_list();
                }
            }
            (PendingSettingsOperation::DefaultRoot, Ok(receipt)) => {
                if let Some(root) = receipt.canonical_default_root {
                    self.shell.features.root_browser.default_root = Some(root.clone());
                    self.set_notice(format!("Set default root: {}", root.display()));
                } else {
                    self.set_notice("Couldn't save settings. Default root was not persisted");
                }
            }
            (PendingSettingsOperation::RootList { .. }, Err(error)) => {
                let notice = format!("Couldn't save saved roots. {error}");
                self.shell.features.root_browser.manage_list.notice = notice.clone();
                self.set_notice(notice);
            }
            (PendingSettingsOperation::DefaultRoot, Err(error)) => {
                self.set_notice(format!("Couldn't save default root. {error}"));
            }
        }
        if self.shell.ui.ui_state_dirty {
            self.persist_ui_state_now();
        }
    }

    fn close_manage_root_list(&mut self) {
        let manage = &mut self.shell.features.root_browser.manage_list;
        manage.open = false;
        manage.input_path.clear();
        manage.add_error.clear();
        manage.add_focus_requested = false;
        manage.add_select_all_requested = false;
        manage.draft_roots.clear();
        manage.draft_default_root = None;
        manage.selected_index = None;
        manage.selected_indices.clear();
        manage.remove_mode = false;
        manage.editing_index = None;
        manage.edit_path.clear();
        manage.edit_error.clear();
        manage.edit_focus_requested = false;
        manage.edit_select_all_requested = false;
        manage.notice.clear();
        manage.dialog_generation = manage.dialog_generation.saturating_add(1);
        manage.pending_validation_intent = None;
        self.shell.worker_bus.root_validation.clear_request();
    }

    fn manage_root_list_path_key(path: &Path) -> String {
        path_key(&normalize_windows_path_buf(path.to_path_buf()))
    }

    fn add_manage_root_list_path(&mut self, root: PathBuf, key: String) {
        let root = normalize_windows_path_buf(root);
        let manage = &mut self.shell.features.root_browser.manage_list;
        if manage
            .draft_roots
            .iter()
            .any(|candidate| Self::manage_root_list_path_key(candidate) == key)
        {
            manage.add_error =
                "Couldn't add the root. This folder is already in the list.".to_string();
            manage.add_focus_requested = true;
            manage.add_select_all_requested = true;
            manage.notice.clear();
            return;
        }
        manage.draft_roots.push(root.clone());
        manage
            .draft_roots
            .sort_by_key(|p| p.to_string_lossy().to_string().to_ascii_lowercase());
        manage.selected_index = manage
            .draft_roots
            .iter()
            .position(|candidate| Self::manage_root_list_path_key(candidate) == key);
        manage.add_error.clear();
        manage.add_focus_requested = false;
        manage.add_select_all_requested = false;
        manage.notice = format!("Added root to draft list: {}", root.display());
    }

    fn request_manage_root_validation(
        &mut self,
        intent: super::RootValidationIntent,
        input: String,
    ) {
        if self.reject_manage_root_list_mutation_while_saving() {
            return;
        }
        let (dialog_generation, draft_roots) = {
            let manage = &self.shell.features.root_browser.manage_list;
            (manage.dialog_generation, manage.draft_roots.clone())
        };
        let request_id = self.shell.worker_bus.root_validation.begin_request();
        self.shell
            .features
            .root_browser
            .manage_list
            .pending_validation_intent = Some(intent);
        let request = super::RootValidationRequest {
            request_id,
            dialog_generation,
            intent,
            input,
            draft_roots,
        };
        if self
            .shell
            .worker_bus
            .root_validation
            .tx
            .send(request)
            .is_err()
        {
            self.shell.worker_bus.root_validation.clear_request();
            self.shell
                .features
                .root_browser
                .manage_list
                .pending_validation_intent = None;
            self.apply_root_validation_error(
                intent,
                "Validation worker is unavailable".to_string(),
            );
            return;
        }
        self.shell.features.root_browser.manage_list.notice = "Validating folder...".to_string();
    }

    fn cancel_manage_root_validation(&mut self) {
        if self.shell.worker_bus.root_validation.in_progress {
            self.shell.worker_bus.root_validation.clear_request();
            let manage = &mut self.shell.features.root_browser.manage_list;
            manage.pending_validation_intent = None;
            if manage.notice == "Validating folder..." {
                manage.notice.clear();
            }
        }
    }

    fn apply_root_validation_error(
        &mut self,
        intent: super::RootValidationIntent,
        message: String,
    ) {
        let manage = &mut self.shell.features.root_browser.manage_list;
        manage.notice.clear();
        match intent {
            super::RootValidationIntent::Add => {
                manage.add_error = format!("Couldn't add the root. {message}");
                manage.add_focus_requested = true;
                manage.add_select_all_requested = true;
            }
            super::RootValidationIntent::Edit { .. } => {
                manage.edit_error = format!("Couldn't update the root. {message}");
                manage.edit_focus_requested = true;
                manage.edit_select_all_requested = true;
            }
        }
    }

    pub(super) fn poll_root_validation_response(&mut self) {
        loop {
            let response = match self.shell.worker_bus.root_validation.rx.try_recv() {
                Ok(response) => response,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if self.shell.worker_bus.root_validation.in_progress {
                        let intent = self
                            .shell
                            .features
                            .root_browser
                            .manage_list
                            .pending_validation_intent;
                        self.shell.worker_bus.root_validation.clear_request();
                        self.shell
                            .features
                            .root_browser
                            .manage_list
                            .pending_validation_intent = None;
                        if let Some(intent) = intent {
                            self.apply_root_validation_error(
                                intent,
                                "Validation worker disconnected unexpectedly".to_string(),
                            );
                        }
                    }
                    break;
                }
            };
            let manage = &self.shell.features.root_browser.manage_list;
            if !manage.open
                || self.shell.worker_bus.root_validation.pending_request_id
                    != Some(response.request_id)
                || manage.dialog_generation != response.dialog_generation
                || manage.pending_validation_intent != Some(response.intent)
            {
                continue;
            }
            self.shell.worker_bus.root_validation.clear_request();
            self.shell
                .features
                .root_browser
                .manage_list
                .pending_validation_intent = None;
            match response.result {
                Ok(validated) => match response.intent {
                    super::RootValidationIntent::Add => {
                        self.add_manage_root_list_path(validated.path, validated.key)
                    }
                    super::RootValidationIntent::Edit { index } => {
                        self.apply_validated_manage_root_edit(index, validated.path, validated.key)
                    }
                },
                Err(message) => self.apply_root_validation_error(response.intent, message),
            }
        }
    }

    fn browse_dialog_start_location(root: &Path) -> PathBuf {
        // This runs only at the explicit native-picker command boundary, not during frame
        // rendering. Preserve the established fallback so a stale saved path cannot make the
        // platform picker fail before it opens.
        let normalized = normalize_windows_path_buf(root.to_path_buf());
        if normalized.is_dir() {
            return normalized;
        }
        if let Some(ancestor) = normalized.ancestors().find(|ancestor| ancestor.is_dir()) {
            return ancestor.to_path_buf();
        }
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }

    #[cfg(test)]
    fn select_root_via_dialog(&mut self, dialog_root: &Path) -> Result<Option<PathBuf>, String> {
        self.shell.features.root_browser.last_browse_dialog_root = Some(dialog_root.to_path_buf());
        self.shell
            .features
            .root_browser
            .browse_dialog_result
            .take()
            .unwrap_or(Ok(None))
    }

    #[cfg(not(test))]
    fn select_root_via_dialog(&mut self, dialog_root: &Path) -> Result<Option<PathBuf>, String> {
        native_dialog::DialogBuilder::file()
            .set_location(dialog_root)
            .open_single_dir()
            .show()
            .map_err(|err| err.to_string())
    }

    /// root selector popup の stable id を返す。
    pub(super) fn root_selector_popup_id() -> egui::Id {
        egui::Id::new(Self::ROOT_SELECTOR_POPUP_ID)
    }

    pub(super) fn is_root_dropdown_open(&self, ctx: &egui::Context) -> bool {
        egui::Popup::is_id_open(ctx, Self::root_selector_popup_id())
    }

    fn current_root_dropdown_index(&self) -> Option<usize> {
        let current_key = path_key(&self.shell.runtime.root);
        self.shell
            .features
            .root_browser
            .saved_roots()
            .iter()
            .position(|path| path_key(path) == current_key)
    }

    /// dropdown のハイライト位置を保存済み root 一覧に同期する。
    pub(super) fn sync_root_dropdown_highlight(&mut self) {
        let max_index = self
            .shell
            .features
            .root_browser
            .saved_roots()
            .len()
            .checked_sub(1);
        let next = match (self.shell.ui.root_dropdown_highlight(), max_index) {
            (_, None) => None,
            (Some(index), Some(max)) => Some(index.min(max)),
            (None, Some(_)) => self.current_root_dropdown_index().or(Some(0usize)),
        };
        self.shell.ui.set_root_dropdown_highlight(next);
    }

    /// root dropdown を開き、入力 focus を切り替える。
    pub(super) fn open_root_dropdown(&mut self, ctx: &egui::Context) {
        self.sync_root_dropdown_highlight();
        egui::Popup::open_id(ctx, Self::root_selector_popup_id());
        self.clear_focus_query_request();
        self.request_unfocus_query();
    }

    /// root dropdown を閉じる。
    pub(super) fn close_root_dropdown(&mut self, ctx: &egui::Context) {
        egui::Popup::close_id(ctx, Self::root_selector_popup_id());
    }

    /// root dropdown 内の候補選択を上下へ移動する。
    pub(super) fn move_root_dropdown_selection(&mut self, delta: isize) {
        let Some(max_index) = self
            .shell
            .features
            .root_browser
            .saved_roots()
            .len()
            .checked_sub(1)
        else {
            self.shell.ui.set_root_dropdown_highlight(None);
            return;
        };
        let current = self
            .shell
            .ui
            .root_dropdown_highlight()
            .or_else(|| self.current_root_dropdown_index())
            .unwrap_or(0) as isize;
        let next = (current + delta).clamp(0, max_index as isize) as usize;
        self.shell.ui.set_root_dropdown_highlight(Some(next));
    }

    /// dropdown で確定した root を現在 tab に反映する。
    pub(super) fn apply_root_dropdown_selection(&mut self, ctx: &egui::Context) {
        let selected = self.shell.ui.root_dropdown_highlight().and_then(|index| {
            self.shell
                .features
                .root_browser
                .saved_roots()
                .get(index)
                .cloned()
        });
        self.close_root_dropdown(ctx);
        if let Some(root) = selected {
            self.apply_root_change(root);
        }
    }
}

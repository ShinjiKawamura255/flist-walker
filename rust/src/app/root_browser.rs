#![allow(deprecated)]

use super::FlistWalkerApp;
use crate::path_utils::normalize_windows_path_buf;
use crate::path_utils::path_key;
use eframe::egui;
use std::path::{Path, PathBuf};

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

    pub(super) fn open_manage_root_list(&mut self) {
        let root_browser = &mut self.shell.features.root_browser;
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
        self.clear_focus_query_request();
        self.request_unfocus_query();
    }

    pub(super) fn add_manage_root_list_input(&mut self) {
        let input = self
            .shell
            .features
            .root_browser
            .manage_list
            .input_path
            .trim()
            .to_string();
        match Self::normalize_manage_root_list_path(&input) {
            Ok(root) => self.add_manage_root_list_path(root),
            Err(message) => {
                let manage = &mut self.shell.features.root_browser.manage_list;
                manage.add_error = format!("Couldn't add the root. {}", message);
                manage.add_focus_requested = true;
                manage.add_select_all_requested = true;
                manage.notice.clear();
            }
        }
    }

    pub(super) fn clear_manage_root_list_add_error(&mut self) {
        let manage = &mut self.shell.features.root_browser.manage_list;
        manage.add_error.clear();
        manage.add_focus_requested = false;
        manage.add_select_all_requested = false;
    }

    pub(super) fn clear_manage_root_list_edit_error(&mut self) {
        let manage = &mut self.shell.features.root_browser.manage_list;
        manage.edit_error.clear();
        manage.edit_focus_requested = false;
        manage.edit_select_all_requested = false;
    }

    pub(super) fn select_manage_root_list_item(&mut self, index: usize) {
        let manage = &mut self.shell.features.root_browser.manage_list;
        if !manage.remove_mode && manage.editing_index.is_none() && index < manage.draft_roots.len()
        {
            manage.selected_index = Some(index);
            manage.notice.clear();
        }
    }

    pub(super) fn start_editing_manage_root_list_item(&mut self) {
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
        manage.edit_select_all_requested = false;
        manage.notice.clear();
    }

    pub(super) fn cancel_manage_root_list_edit(&mut self) {
        let manage = &mut self.shell.features.root_browser.manage_list;
        manage.editing_index = None;
        manage.edit_path.clear();
        manage.edit_error.clear();
        manage.edit_focus_requested = false;
        manage.edit_select_all_requested = false;
        manage.notice.clear();
    }

    pub(super) fn save_manage_root_list_edit(&mut self) {
        let (index, input) = {
            let manage = &self.shell.features.root_browser.manage_list;
            let Some(index) = manage.editing_index else {
                return;
            };
            (index, manage.edit_path.trim().to_string())
        };
        let replacement = match Self::normalize_manage_root_list_path(&input) {
            Ok(root) => root,
            Err(message) => {
                let manage = &mut self.shell.features.root_browser.manage_list;
                manage.edit_error = format!("Couldn't update the root. {}", message);
                manage.edit_focus_requested = true;
                manage.edit_select_all_requested = true;
                manage.notice.clear();
                return;
            }
        };
        let replacement_key = Self::manage_root_list_path_key(&replacement);
        let manage = &mut self.shell.features.root_browser.manage_list;
        if manage
            .draft_roots
            .iter()
            .enumerate()
            .any(|(candidate_index, candidate)| {
                candidate_index != index
                    && Self::manage_root_list_path_key(candidate) == replacement_key
            })
        {
            manage.edit_error =
                "Couldn't update the root. This folder is already in the list.".to_string();
            manage.edit_focus_requested = true;
            manage.edit_select_all_requested = true;
            manage.notice.clear();
            return;
        }
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
        let manage = &mut self.shell.features.root_browser.manage_list;
        manage.remove_mode = false;
        manage.selected_indices.clear();
        manage.notice.clear();
    }

    pub(super) fn browse_for_manage_root_list(&mut self) {
        let input = self
            .shell
            .features
            .root_browser
            .manage_list
            .input_path
            .trim();
        let start = if input.is_empty() {
            Self::browse_dialog_start_location(&self.shell.runtime.root)
        } else {
            Self::browse_dialog_start_location(Path::new(input))
        };
        match self.select_root_via_dialog(&start) {
            Ok(Some(dir)) => {
                let root = normalize_windows_path_buf(dir);
                self.shell.features.root_browser.manage_list.input_path =
                    root.to_string_lossy().to_string();
                self.add_manage_root_list_path(root);
            }
            Ok(None) => {}
            Err(err) => {
                self.shell.features.root_browser.manage_list.notice =
                    format!("Browse failed: {}", err);
            }
        }
    }

    pub(super) fn remove_selected_manage_root_list_items(&mut self) {
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
        let (draft_roots, draft_default_root) = {
            let manage = &self.shell.features.root_browser.manage_list;
            (
                manage.draft_roots.clone(),
                manage.draft_default_root.clone(),
            )
        };
        let previous_default_key = self
            .shell
            .features
            .root_browser
            .default_root
            .as_ref()
            .map(|root| Self::manage_root_list_path_key(root));
        let draft_default_key = draft_default_root
            .as_ref()
            .map(|root| Self::manage_root_list_path_key(root));
        self.shell.features.root_browser.saved_roots = draft_roots;
        self.shell.features.root_browser.default_root = draft_default_root;
        self.shell.ui.set_root_dropdown_highlight(None);
        if previous_default_key != draft_default_key {
            self.mark_ui_state_dirty();
            self.persist_ui_state_now();
        }
        self.save_saved_roots();
        self.shell.features.root_browser.manage_list.notice =
            "Applied saved roots list".to_string();
        self.set_notice("Applied saved roots list");
    }

    pub(super) fn confirm_manage_root_list_changes(&mut self) {
        self.apply_manage_root_list_changes();
        self.close_manage_root_list();
    }

    pub(super) fn cancel_manage_root_list(&mut self) {
        self.close_manage_root_list();
        self.set_notice("Canceled saved roots list changes");
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
    }

    fn normalize_manage_root_list_path(input: &str) -> Result<PathBuf, String> {
        if input.is_empty() {
            return Err("Enter a folder path.".to_string());
        }
        let path = normalize_windows_path_buf(PathBuf::from(input));
        if !path.is_dir() {
            return Err(format!("Folder not found: {}", path.display()));
        }
        Ok(normalize_windows_path_buf(
            path.canonicalize().unwrap_or(path),
        ))
    }

    fn manage_root_list_path_key(path: &Path) -> String {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        path_key(&normalize_windows_path_buf(canonical))
    }

    fn add_manage_root_list_path(&mut self, root: PathBuf) {
        let root = normalize_windows_path_buf(root);
        let key = Self::manage_root_list_path_key(&root);
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

    fn browse_dialog_start_location(root: &Path) -> PathBuf {
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
        native_dialog::FileDialog::new()
            .set_location(dialog_root)
            .show_open_single_dir()
            .map_err(|err| err.to_string())
    }

    /// root selector popup の stable id を返す。
    pub(super) fn root_selector_popup_id() -> egui::Id {
        egui::Id::new(Self::ROOT_SELECTOR_POPUP_ID)
    }

    pub(super) fn is_root_dropdown_open(&self, ctx: &egui::Context) -> bool {
        ctx.memory(|mem| mem.is_popup_open(Self::root_selector_popup_id()))
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
        ctx.memory_mut(|mem| mem.open_popup(Self::root_selector_popup_id()));
        self.clear_focus_query_request();
        self.request_unfocus_query();
    }

    /// root dropdown を閉じる。
    pub(super) fn close_root_dropdown(&mut self, ctx: &egui::Context) {
        ctx.memory_mut(|mem| mem.close_popup(Self::root_selector_popup_id()));
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

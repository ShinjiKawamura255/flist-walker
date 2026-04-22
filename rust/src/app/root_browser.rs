use super::FlistWalkerApp;
use crate::path_utils::normalize_windows_path_buf;
use crate::path_utils::path_key;
use eframe::egui;
use std::path::{Path, PathBuf};

impl FlistWalkerApp {
    /// ダイアログで選んだ root を現在 tab に適用する。
    pub(super) fn browse_for_root(&mut self) {
        let dialog_root = normalize_windows_path_buf(self.shell.runtime.root.clone());
        match self.select_root_via_dialog(&dialog_root) {
            Ok(Some(dir)) => self.apply_root_change(dir),
            Ok(None) => {}
            Err(err) => self.set_notice(format!("Browse failed: {}", err)),
        }
    }

    /// ダイアログで選んだ root を新規 tab として開く。
    pub(super) fn browse_for_root_in_new_tab(&mut self) {
        let dialog_root = normalize_windows_path_buf(self.shell.runtime.root.clone());
        match self.select_root_via_dialog(&dialog_root) {
            Ok(Some(dir)) => {
                self.create_new_tab();
                self.apply_root_change(dir);
            }
            Ok(None) => {}
            Err(err) => self.set_notice(format!("Browse failed: {}", err)),
        }
    }

    #[cfg(test)]
    fn select_root_via_dialog(&mut self, _dialog_root: &Path) -> Result<Option<PathBuf>, String> {
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
        ctx.memory_mut(|mem| mem.close_popup());
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

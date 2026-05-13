use super::super::{normalize_path_for_display, ActionRequest, FlistWalkerApp};
use eframe::egui;
use std::path::PathBuf;

impl FlistWalkerApp {
    /// pinned selection 優先で action 対象 path を列挙する。
    fn selected_paths(&self) -> Vec<PathBuf> {
        if !self.shell.runtime.pinned_paths.is_empty() {
            let mut out: Vec<PathBuf> = self.shell.runtime.pinned_paths.iter().cloned().collect();
            out.sort();
            return out;
        }
        self.shell
            .runtime
            .current_row
            .and_then(|row| {
                self.shell
                    .runtime
                    .results
                    .get(row)
                    .map(|(p, _)| vec![p.clone()])
            })
            .unwrap_or_default()
    }

    /// 既定動作で選択 path を実行またはオープンする。
    pub(in crate::app) fn execute_selected(&mut self) {
        self.execute_selected_with_options(false);
    }

    /// Enter 系アクション用に file は親フォルダオープンへ切り替えられる実行入口。
    pub(in crate::app) fn execute_selected_for_activation(&mut self, open_parent_for_files: bool) {
        self.execute_selected_with_options(open_parent_for_files);
    }

    /// 選択項目の格納フォルダを開く。
    pub(in crate::app) fn execute_selected_open_folder(&mut self) {
        self.execute_selected_for_activation(true);
    }

    /// worker dispatch と root 外 path ガードを含めて action を起動する。
    pub(in crate::app) fn execute_selected_with_options(&mut self, open_parent_for_files: bool) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }
        if let Some(blocked) = self.first_action_path_outside_root(&paths) {
            self.shell.worker_bus.action.clear_request();
            self.set_notice(format!(
                "Action blocked: path is outside current root: {}",
                normalize_path_for_display(&blocked)
            ));
            return;
        }

        let request_id = self.shell.worker_bus.action.begin_request();
        self.bind_action_request_to_current_tab(request_id);

        if paths.len() == 1 {
            if open_parent_for_files {
                self.set_notice(format!(
                    "Action: open containing folder for {}",
                    normalize_path_for_display(&paths[0])
                ));
            } else {
                self.set_notice(format!("Action: {}", normalize_path_for_display(&paths[0])));
            }
        } else if open_parent_for_files {
            self.set_notice(format!(
                "Action: launched {} containing folder items",
                paths.len()
            ));
        } else {
            self.set_notice(format!("Action: launched {} items", paths.len()));
        }

        let req = ActionRequest {
            request_id,
            paths,
            open_parent_for_files,
        };
        if self.shell.worker_bus.action.tx.send(req).is_err() {
            self.shell.worker_bus.action.clear_request();
            self.set_notice("Action worker is unavailable");
        }
    }

    /// 選択 path を clipboard 用文字列へ変換して UI 出力へ流す。
    pub(in crate::app) fn copy_selected_paths(&mut self, ctx: &egui::Context) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }
        let text = Self::clipboard_paths_text(&paths);
        ctx.copy_text(text);
        if paths.len() == 1 {
            self.set_notice(format!(
                "Copied path: {}",
                normalize_path_for_display(&paths[0])
            ));
        } else {
            self.set_notice(format!("Copied {} paths to clipboard", paths.len()));
        }
    }

    /// pinned selection を全解除する。
    pub(in crate::app) fn clear_pinned(&mut self) {
        self.shell.runtime.pinned_paths.clear();
        self.set_notice("Cleared pinned selections");
    }

    pub(in crate::app) fn run_deferred_shortcuts(&mut self, ctx: &egui::Context) {
        if !self.shell.ui.pending_copy_shortcut {
            return;
        }
        self.shell.ui.pending_copy_shortcut = false;
        self.copy_selected_paths(ctx);
        self.request_focus_query();
    }
}

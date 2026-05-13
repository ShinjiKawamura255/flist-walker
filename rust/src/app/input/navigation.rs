use super::super::FlistWalkerApp;

impl FlistWalkerApp {
    /// ページ単位のカーソル移動を行う。
    pub(in crate::app) fn move_page(&mut self, direction: isize) {
        self.move_row(direction.saturating_mul(Self::PAGE_MOVE_ROWS));
    }

    /// 結果一覧内の current row を相対移動する。
    pub(in crate::app) fn move_row(&mut self, delta: isize) {
        self.commit_query_history_if_needed(true);
        if self.shell.runtime.results.is_empty() {
            return;
        }
        let row = self.shell.runtime.current_row.unwrap_or(0) as isize;
        let next = (row + delta).clamp(0, self.shell.runtime.results.len() as isize - 1) as usize;
        self.set_current_row(Some(next));
        self.request_scroll_to_current();
        self.request_preview_for_current();
        self.refresh_status_line();
    }

    /// current row を pinned selection に追加または解除する。
    pub(in crate::app) fn toggle_pin_current(&mut self) {
        if let Some(row) = self.shell.runtime.current_row {
            if let Some((path, _)) = self.shell.runtime.results.get(row) {
                let path = path.clone();
                if self.shell.runtime.pinned_paths.contains(&path) {
                    self.shell.runtime.pinned_paths.remove(&path);
                } else {
                    self.shell.runtime.pinned_paths.insert(path);
                }
                self.refresh_status_line();
            }
        }
    }

    /// 先頭行へ移動し preview を更新する。
    pub(in crate::app) fn move_to_first_row(&mut self) {
        self.commit_query_history_if_needed(true);
        if self.shell.runtime.results.is_empty() {
            return;
        }
        self.set_current_row(Some(0));
        self.request_scroll_to_current();
        self.request_preview_for_current();
        self.refresh_status_line();
    }

    /// 末尾行へ移動し preview を更新する。
    pub(in crate::app) fn move_to_last_row(&mut self) {
        self.commit_query_history_if_needed(true);
        if self.shell.runtime.results.is_empty() {
            return;
        }
        self.set_current_row(Some(self.shell.runtime.results.len().saturating_sub(1)));
        self.request_scroll_to_current();
        self.request_preview_for_current();
        self.refresh_status_line();
    }

    /// query と選択状態を初期化し一覧表示へ戻す。
    pub(in crate::app) fn clear_query_and_selection(&mut self) {
        self.shell.runtime.query_state.query.clear();
        self.reset_query_history_navigation();
        self.reset_history_search_state();
        self.set_query_history_dirty_since(None);
        self.shell.runtime.pinned_paths.clear();
        // Keep the list visible after Esc/Ctrl+G by restoring the default row selection.
        self.set_current_row(Some(0));
        self.shell.runtime.preview.clear();
        self.update_results();
        self.ensure_results_cursor_visible();
        self.request_focus_query();
        self.set_notice("Cleared selection and query");
    }
}

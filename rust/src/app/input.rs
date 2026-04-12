use super::*;

impl FlistWalkerApp {
    /// pinned selection 優先で action 対象 path を列挙する。
    fn selected_paths(&self) -> Vec<PathBuf> {
        if !self.shell.runtime.pinned_paths.is_empty() {
            let mut out: Vec<PathBuf> = self.shell.runtime.pinned_paths.iter().cloned().collect();
            out.sort();
            return out;
        }
        self.shell.runtime.current_row
            .and_then(|row| self.shell.runtime.results.get(row).map(|(p, _)| vec![p.clone()]))
            .unwrap_or_default()
    }

    /// 既定動作で選択 path を実行またはオープンする。
    pub(super) fn execute_selected(&mut self) {
        self.execute_selected_with_options(false);
    }

    /// Enter 系アクション用に file は親フォルダオープンへ切り替えられる実行入口。
    pub(super) fn execute_selected_for_activation(&mut self, open_parent_for_files: bool) {
        self.execute_selected_with_options(open_parent_for_files);
    }

    /// 選択項目の格納フォルダを開く。
    pub(super) fn execute_selected_open_folder(&mut self) {
        self.execute_selected_for_activation(true);
    }

    /// worker dispatch と root 外 path ガードを含めて action を起動する。
    pub(super) fn execute_selected_with_options(&mut self, open_parent_for_files: bool) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }
        if let Some(blocked) = self.first_action_path_outside_root(&paths) {
            self.shell.worker_bus.action.pending_request_id = None;
            self.shell.worker_bus.action.in_progress = false;
            self.set_notice(format!(
                "Action blocked: path is outside current root: {}",
                normalize_path_for_display(&blocked)
            ));
            return;
        }

        let request_id = self.shell.worker_bus.action.next_request_id;
        self.shell.worker_bus.action.next_request_id =
            self.shell.worker_bus.action.next_request_id.saturating_add(1);
        self.shell.worker_bus.action.pending_request_id = Some(request_id);
        self.shell.worker_bus.action.in_progress = true;
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
            self.shell.worker_bus.action.pending_request_id = None;
            self.shell.worker_bus.action.in_progress = false;
            self.set_notice("Action worker is unavailable");
        }
    }

    /// 選択 path を clipboard 用文字列へ変換して UI 出力へ流す。
    pub(super) fn copy_selected_paths(&mut self, ctx: &egui::Context) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }
        let text = Self::clipboard_paths_text(&paths);
        ctx.output_mut(|o| o.copied_text = text);
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
    pub(super) fn clear_pinned(&mut self) {
        self.shell.runtime.pinned_paths.clear();
        self.set_notice("Cleared pinned selections");
    }

    /// ページ単位のカーソル移動を行う。
    pub(super) fn move_page(&mut self, direction: isize) {
        self.move_row(direction.saturating_mul(Self::PAGE_MOVE_ROWS));
    }

    /// 結果一覧内の current row を相対移動する。
    pub(super) fn move_row(&mut self, delta: isize) {
        self.commit_query_history_if_needed(true);
        if self.shell.runtime.results.is_empty() {
            return;
        }
        let row = self.shell.runtime.current_row.unwrap_or(0) as isize;
        let next = (row + delta).clamp(0, self.shell.runtime.results.len() as isize - 1) as usize;
        self.shell.runtime.current_row = Some(next);
        self.shell.ui.scroll_to_current = true;
        self.request_preview_for_current();
        self.refresh_status_line();
    }

    /// current row を pinned selection に追加または解除する。
    pub(super) fn toggle_pin_current(&mut self) {
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
    pub(super) fn move_to_first_row(&mut self) {
        self.commit_query_history_if_needed(true);
        if self.shell.runtime.results.is_empty() {
            return;
        }
        self.shell.runtime.current_row = Some(0);
        self.shell.ui.scroll_to_current = true;
        self.request_preview_for_current();
        self.refresh_status_line();
    }

    /// 末尾行へ移動し preview を更新する。
    pub(super) fn move_to_last_row(&mut self) {
        self.commit_query_history_if_needed(true);
        if self.shell.runtime.results.is_empty() {
            return;
        }
        self.shell.runtime.current_row = Some(self.shell.runtime.results.len().saturating_sub(1));
        self.shell.ui.scroll_to_current = true;
        self.request_preview_for_current();
        self.refresh_status_line();
    }

    /// query と選択状態を初期化し一覧表示へ戻す。
    pub(super) fn clear_query_and_selection(&mut self) {
        self.shell.runtime.query_state.query.clear();
        self.reset_query_history_navigation();
        self.reset_history_search_state();
        self.shell.runtime.query_state.query_history_dirty_since = None;
        self.shell.runtime.pinned_paths.clear();
        // Keep the list visible after Esc/Ctrl+G by restoring the default row selection.
        self.shell.runtime.current_row = Some(0);
        self.shell.runtime.preview.clear();
        self.update_results();
        self.shell.ui.focus_query_requested = true;
        self.set_notice("Cleared selection and query");
    }

    pub(super) fn normalize_singleline_input(text: &mut String) -> bool {
        let original = text.as_str();
        let mut normalized = String::with_capacity(original.len());
        let mut at_line_start = true;

        for ch in original.chars() {
            if matches!(
                ch,
                '\u{00ad}'
                    | '\u{200b}'
                    | '\u{200c}'
                    | '\u{200d}'
                    | '\u{200e}'
                    | '\u{200f}'
                    | '\u{202a}'
                    | '\u{202b}'
                    | '\u{202c}'
                    | '\u{202d}'
                    | '\u{202e}'
                    | '\u{2060}'
                    | '\u{2066}'
                    | '\u{2067}'
                    | '\u{2068}'
                    | '\u{2069}'
                    | '\u{feff}'
            ) {
                continue;
            }

            match ch {
                '\r' | '\n' => {
                    if !normalized.ends_with(' ') && !normalized.is_empty() {
                        normalized.push(' ');
                    }
                    at_line_start = true;
                }
                '\t' if at_line_start => {}
                '\t' => {
                    normalized.push(' ');
                    at_line_start = false;
                }
                _ => {
                    normalized.push(ch);
                    at_line_start = false;
                }
            }
        }

        if normalized != original {
            *text = normalized;
            return true;
        }

        false
    }

    pub(super) fn char_count(text: &str) -> usize {
        text.chars().count()
    }

    pub(super) fn byte_index_at_char(text: &str, char_index: usize) -> usize {
        if char_index == 0 {
            return 0;
        }
        text.char_indices()
            .nth(char_index)
            .map(|(idx, _)| idx)
            .unwrap_or(text.len())
    }

    pub(super) fn remove_char_range(text: &mut String, start: usize, end: usize) -> String {
        if start >= end {
            return String::new();
        }
        let start_byte = Self::byte_index_at_char(text, start);
        let end_byte = Self::byte_index_at_char(text, end);
        let removed = text[start_byte..end_byte].to_string();
        text.replace_range(start_byte..end_byte, "");
        removed
    }

    pub(super) fn insert_at_char(text: &mut String, pos: usize, value: &str) {
        let byte_pos = Self::byte_index_at_char(text, pos);
        text.insert_str(byte_pos, value);
    }

    pub(super) fn is_word_char(ch: char) -> bool {
        ch.is_alphanumeric() || ch == '_' || ch == '-'
    }

    pub(super) fn selection_range(cursor: usize, anchor: usize) -> Option<(usize, usize)> {
        if cursor == anchor {
            None
        } else {
            Some((cursor.min(anchor), cursor.max(anchor)))
        }
    }

    pub(super) fn apply_ctrl_h_delete(
        &mut self,
        cursor: &mut usize,
        anchor: &mut usize,
        text_already_changed: bool,
    ) -> (bool, bool) {
        // Some backends map Ctrl+H to Backspace at the widget level.
        // Avoid applying our delete logic twice in the same frame.
        if text_already_changed {
            return (false, false);
        }

        if let Some((start, end)) = Self::selection_range(*cursor, *anchor) {
            Self::remove_char_range(&mut self.shell.runtime.query_state.query, start, end);
            *cursor = start;
            *anchor = start;
            return (true, true);
        }

        if *cursor > 0 {
            let start = *cursor - 1;
            Self::remove_char_range(&mut self.shell.runtime.query_state.query, start, *cursor);
            *cursor = start;
            *anchor = start;
            return (true, true);
        }

        (false, false)
    }

    pub(super) fn apply_emacs_query_shortcuts(
        &mut self,
        ctx: &egui::Context,
        output: &mut egui::text_edit::TextEditOutput,
    ) -> bool {
        if self.shell.ui.ime_composition_active {
            return false;
        }
        if !output.response.has_focus() {
            return false;
        }

        let emacs_mods = egui::Modifiers {
            command: true,
            ..Default::default()
        };
        let pressed = |key: egui::Key| ctx.input_mut(|i| i.consume_key(emacs_mods, key));

        let mut text_changed = false;
        let mut cursor_changed = false;
        let char_len = Self::char_count(&self.shell.runtime.query_state.query);
        let ccursor =
            output.state.cursor.char_range().unwrap_or_else(|| {
                egui::text::CCursorRange::one(egui::text::CCursor::new(char_len))
            });
        let mut cursor = ccursor.primary.index.min(char_len);
        let mut anchor = ccursor.secondary.index.min(char_len);

        if pressed(egui::Key::A) {
            cursor = 0;
            anchor = 0;
            cursor_changed = true;
        } else if pressed(egui::Key::E) {
            let end = Self::char_count(&self.shell.runtime.query_state.query);
            cursor = end;
            anchor = end;
            cursor_changed = true;
        } else if pressed(egui::Key::B) {
            let next = cursor.saturating_sub(1);
            if next != cursor {
                cursor = next;
                anchor = next;
                cursor_changed = true;
            }
        } else if pressed(egui::Key::F) {
            let end = Self::char_count(&self.shell.runtime.query_state.query);
            let next = (cursor + 1).min(end);
            if next != cursor {
                cursor = next;
                anchor = next;
                cursor_changed = true;
            }
        } else if pressed(egui::Key::H) {
            let (changed, moved) =
                self.apply_ctrl_h_delete(&mut cursor, &mut anchor, output.response.changed());
            text_changed |= changed;
            cursor_changed |= moved;
        } else if pressed(egui::Key::D) {
            if let Some((start, end)) = Self::selection_range(cursor, anchor) {
                Self::remove_char_range(&mut self.shell.runtime.query_state.query, start, end);
                cursor = start;
                anchor = start;
                text_changed = true;
                cursor_changed = true;
            } else {
                let end = Self::char_count(&self.shell.runtime.query_state.query);
                if cursor < end {
                    Self::remove_char_range(&mut self.shell.runtime.query_state.query, cursor, cursor + 1);
                    text_changed = true;
                    cursor_changed = true;
                }
            }
        } else if pressed(egui::Key::W) {
            if let Some((start, end)) = Self::selection_range(cursor, anchor) {
                self.shell.runtime.query_state.kill_buffer =
                    Self::remove_char_range(&mut self.shell.runtime.query_state.query, start, end);
                cursor = start;
                anchor = start;
                text_changed = true;
                cursor_changed = true;
            } else if cursor > 0 {
                let chars: Vec<char> = self.shell.runtime.query_state.query.chars().collect();
                let mut start = cursor;
                while start > 0 && chars[start - 1].is_whitespace() {
                    start -= 1;
                }
                while start > 0 && Self::is_word_char(chars[start - 1]) {
                    start -= 1;
                }
                if start < cursor {
                    self.shell.runtime.query_state.kill_buffer =
                        Self::remove_char_range(&mut self.shell.runtime.query_state.query, start, cursor);
                    cursor = start;
                    anchor = start;
                    text_changed = true;
                    cursor_changed = true;
                }
            }
        } else if pressed(egui::Key::K) {
            let end = Self::char_count(&self.shell.runtime.query_state.query);
            if cursor < end {
                self.shell.runtime.query_state.kill_buffer =
                    Self::remove_char_range(&mut self.shell.runtime.query_state.query, cursor, end);
                anchor = cursor;
                text_changed = true;
                cursor_changed = true;
            }
        } else if pressed(egui::Key::Y) {
            if !self.shell.runtime.query_state.kill_buffer.is_empty() {
                let kill_buffer = self.shell.runtime.query_state.kill_buffer.clone();
                if let Some((start, end)) = Self::selection_range(cursor, anchor) {
                    Self::remove_char_range(&mut self.shell.runtime.query_state.query, start, end);
                    cursor = start;
                }
                Self::insert_at_char(&mut self.shell.runtime.query_state.query, cursor, &kill_buffer);
                cursor += Self::char_count(&kill_buffer);
                anchor = cursor;
                text_changed = true;
                cursor_changed = true;
            }
        } else if pressed(egui::Key::U) && cursor > 0 {
            Self::remove_char_range(&mut self.shell.runtime.query_state.query, 0, cursor);
            cursor = 0;
            anchor = 0;
            text_changed = true;
            cursor_changed = true;
        }

        if cursor_changed {
            output
                .state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(anchor),
                    egui::text::CCursor::new(cursor),
                )));
            output.state.clone().store(ctx, output.response.id);
            ctx.request_repaint();
        }

        text_changed
    }

    pub(super) fn primary_shortcut_label() -> &'static str {
        #[cfg(target_os = "macos")]
        {
            "Cmd"
        }
        #[cfg(not(target_os = "macos"))]
        {
            "Ctrl"
        }
    }

    pub(super) fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.handle_filelist_dialog_shortcuts(ctx) {
            return;
        }
        let query_focused = ctx.memory(|m| m.has_focus(self.shell.ui.query_input_id));
        self.handle_shortcuts_with_focus(ctx, query_focused);
    }

    pub(super) fn consume_gui_shortcut(ctx: &egui::Context, key: egui::Key, shift: bool) -> bool {
        #[cfg(target_os = "macos")]
        {
            let primary = egui::Modifiers {
                mac_cmd: true,
                shift,
                ..Default::default()
            };
            if ctx.input_mut(|i| i.consume_key(primary, key)) {
                return true;
            }
            let fallback = egui::Modifiers {
                command: true,
                shift,
                ..Default::default()
            };
            return ctx.input_mut(|i| i.consume_key(fallback, key));
        }
        #[cfg(not(target_os = "macos"))]
        {
            let mods = egui::Modifiers {
                ctrl: true,
                shift,
                ..Default::default()
            };
            ctx.input_mut(|i| i.consume_key(mods, key))
        }
    }

    pub(super) fn consume_tab_switch_shortcut(
        ctx: &egui::Context,
        key: egui::Key,
        shift: bool,
    ) -> bool {
        let mods = egui::Modifiers {
            ctrl: true,
            shift,
            ..Default::default()
        };
        ctx.input_mut(|i| i.consume_key(mods, key))
    }

    pub(super) fn consume_emacs_shortcut(ctx: &egui::Context, key: egui::Key, shift: bool) -> bool {
        let mods = egui::Modifiers {
            ctrl: true,
            shift,
            ..Default::default()
        };
        if ctx.input_mut(|i| i.consume_key(mods, key)) {
            return true;
        }
        #[cfg(target_os = "macos")]
        {
            // Some backends may surface ctrl chords via command bit on macOS.
            let fallback = egui::Modifiers {
                command: true,
                ctrl: true,
                shift,
                ..Default::default()
            };
            return ctx.input_mut(|i| i.consume_key(fallback, key));
        }
        #[cfg(not(target_os = "macos"))]
        false
    }

    pub(super) fn current_filelist_dialog_kind(&self) -> Option<FileListDialogKind> {
        let current_tab_id = self.current_tab_id().unwrap_or_default();
        if self.shell.features
            .filelist
            .pending_confirmation
            .as_ref()
            .is_some_and(|pending| pending.tab_id == current_tab_id)
        {
            return Some(FileListDialogKind::Overwrite);
        }
        if self.shell.features
            .filelist
            .pending_ancestor_confirmation
            .as_ref()
            .is_some_and(|pending| pending.tab_id == current_tab_id)
        {
            return Some(FileListDialogKind::Ancestor);
        }
        if self.shell.features
            .filelist
            .pending_use_walker_confirmation
            .as_ref()
            .is_some_and(|pending| pending.source_tab_id == current_tab_id)
        {
            return Some(FileListDialogKind::UseWalker);
        }
        None
    }

    fn filelist_dialog_button_count(kind: FileListDialogKind) -> usize {
        match kind {
            FileListDialogKind::Overwrite => 2,
            FileListDialogKind::Ancestor => 3,
            FileListDialogKind::UseWalker => 2,
        }
    }

    pub(super) fn sync_filelist_dialog_selection(&mut self, kind: FileListDialogKind) {
        let button_count = Self::filelist_dialog_button_count(kind);
        if self.shell.features.filelist.active_dialog != Some(kind) {
            self.shell.features.filelist.active_dialog = Some(kind);
            self.shell.features.filelist.active_dialog_button = 0;
            return;
        }
        self.shell.features.filelist.active_dialog_button %= button_count;
    }

    pub(super) fn clear_filelist_dialog_selection(&mut self) {
        self.shell.features.filelist.active_dialog = None;
        self.shell.features.filelist.active_dialog_button = 0;
    }

    fn activate_selected_filelist_dialog_button(&mut self) {
        match (
            self.shell.features.filelist.active_dialog,
            self.shell.features.filelist.active_dialog_button,
        ) {
            (Some(FileListDialogKind::Overwrite), 0) => self.confirm_pending_filelist_overwrite(),
            (Some(FileListDialogKind::Overwrite), _) => self.cancel_pending_filelist_overwrite(),
            (Some(FileListDialogKind::Ancestor), 0) => {
                self.confirm_pending_filelist_ancestor_propagation()
            }
            (Some(FileListDialogKind::Ancestor), 1) => {
                self.skip_pending_filelist_ancestor_propagation()
            }
            (Some(FileListDialogKind::Ancestor), _) => {
                self.cancel_pending_filelist_ancestor_confirmation()
            }
            (Some(FileListDialogKind::UseWalker), 0) => self.confirm_pending_filelist_use_walker(),
            (Some(FileListDialogKind::UseWalker), _) => self.cancel_pending_filelist_use_walker(),
            (None, _) => {}
        }
    }

    fn cancel_active_filelist_dialog(&mut self) {
        match self.shell.features.filelist.active_dialog {
            Some(FileListDialogKind::Overwrite) => self.cancel_pending_filelist_overwrite(),
            Some(FileListDialogKind::Ancestor) => {
                self.cancel_pending_filelist_ancestor_confirmation()
            }
            Some(FileListDialogKind::UseWalker) => self.cancel_pending_filelist_use_walker(),
            None => {}
        }
    }

    fn move_filelist_dialog_selection(&mut self, delta: isize) {
        let Some(kind) = self.shell.features.filelist.active_dialog else {
            return;
        };
        let count = Self::filelist_dialog_button_count(kind) as isize;
        let current = self.shell.features.filelist.active_dialog_button as isize;
        self.shell.features.filelist.active_dialog_button = (current + delta).rem_euclid(count) as usize;
    }

    fn handle_filelist_dialog_shortcuts(&mut self, ctx: &egui::Context) -> bool {
        let Some(kind) = self.current_filelist_dialog_kind() else {
            self.clear_filelist_dialog_selection();
            return false;
        };
        self.sync_filelist_dialog_selection(kind);

        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.cancel_active_filelist_dialog();
            return true;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft))
            || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp))
            || ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab))
        {
            self.move_filelist_dialog_selection(-1);
            return true;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight))
            || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown))
            || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab))
        {
            self.move_filelist_dialog_selection(1);
            return true;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
            || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space))
        {
            self.activate_selected_filelist_dialog_button();
            return true;
        }
        true
    }

    pub(super) fn handle_shortcuts_with_focus(&mut self, ctx: &egui::Context, query_focused: bool) {
        if Self::consume_gui_shortcut(ctx, egui::Key::R, true) {
            self.open_root_dropdown(ctx);
            return;
        }
        if self.is_root_dropdown_open(ctx) {
            if Self::consume_emacs_shortcut(ctx, egui::Key::N, false)
                || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown))
            {
                self.move_root_dropdown_selection(1);
                return;
            }
            if Self::consume_emacs_shortcut(ctx, egui::Key::P, false)
                || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp))
            {
                self.move_root_dropdown_selection(-1);
                return;
            }
            if Self::consume_emacs_shortcut(ctx, egui::Key::J, false)
                || Self::consume_emacs_shortcut(ctx, egui::Key::M, false)
                || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
            {
                self.apply_root_dropdown_selection(ctx);
                return;
            }
            if Self::consume_emacs_shortcut(ctx, egui::Key::G, false)
                || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
            {
                self.close_root_dropdown(ctx);
                return;
            }
        }

        if Self::consume_gui_shortcut(ctx, egui::Key::T, false) {
            self.create_new_tab();
            return;
        }
        if Self::consume_gui_shortcut(ctx, egui::Key::W, false) {
            self.close_active_tab();
            return;
        }
        if Self::consume_tab_switch_shortcut(ctx, egui::Key::Tab, true) {
            self.activate_previous_tab();
            return;
        }
        if Self::consume_tab_switch_shortcut(ctx, egui::Key::Tab, false) {
            self.activate_next_tab();
            return;
        }
        for (shortcut_number, key) in [
            (1, egui::Key::Num1),
            (2, egui::Key::Num2),
            (3, egui::Key::Num3),
            (4, egui::Key::Num4),
            (5, egui::Key::Num5),
            (6, egui::Key::Num6),
            (7, egui::Key::Num7),
            (8, egui::Key::Num8),
            (9, egui::Key::Num9),
        ] {
            if Self::consume_gui_shortcut(ctx, key, false) {
                self.activate_tab_shortcut(shortcut_number);
                return;
            }
        }
        if Self::consume_gui_shortcut(ctx, egui::Key::L, false) {
            if query_focused {
                self.shell.ui.focus_query_requested = false;
                self.shell.ui.unfocus_query_requested = true;
            } else {
                self.shell.ui.focus_query_requested = true;
                self.shell.ui.unfocus_query_requested = false;
            }
            return;
        }
        if Self::consume_gui_shortcut(ctx, egui::Key::O, true) {
            self.browse_for_root_in_new_tab();
            return;
        }
        if Self::consume_gui_shortcut(ctx, egui::Key::O, false) {
            self.browse_for_root();
            return;
        }

        if self.shell.runtime.query_state.history_search_active {
            if Self::consume_emacs_shortcut(ctx, egui::Key::N, false) {
                self.move_history_search_selection(1);
            }
            if Self::consume_emacs_shortcut(ctx, egui::Key::P, false) {
                self.move_history_search_selection(-1);
            }
            if Self::consume_emacs_shortcut(ctx, egui::Key::G, false)
                || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
            {
                self.cancel_history_search();
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)) {
                self.move_history_search_selection(1);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)) {
                self.move_history_search_selection(-1);
            }
            if Self::consume_emacs_shortcut(ctx, egui::Key::J, false)
                || Self::consume_emacs_shortcut(ctx, egui::Key::M, false)
                || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
            {
                self.accept_history_search();
            }
            if query_focused {
                ctx.memory_mut(|m| m.request_focus(self.shell.ui.query_input_id));
            }
            return;
        }

        if Self::consume_emacs_shortcut(ctx, egui::Key::N, false) {
            self.move_row(1);
        }
        if Self::consume_emacs_shortcut(ctx, egui::Key::P, false) {
            self.move_row(-1);
        }
        if Self::consume_emacs_shortcut(ctx, egui::Key::R, false) {
            self.start_history_search();
            if query_focused {
                ctx.memory_mut(|m| m.request_focus(self.shell.ui.query_input_id));
            }
        }
        if Self::consume_gui_shortcut(ctx, egui::Key::C, true) {
            // Keep this deferred until after TextEdit processing so query-focus copy
            // cannot overwrite the intended "copy selected path(s)" shortcut result.
            self.shell.ui.pending_copy_shortcut = true;
        }
        if Self::consume_emacs_shortcut(ctx, egui::Key::G, false)
            || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.clear_query_and_selection();
        }
        let tab_forward = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab));
        if tab_forward {
            self.toggle_pin_current();
            // Keep Tab dedicated to pin toggle without changing query focus active/inactive state.
            if query_focused {
                ctx.memory_mut(|m| m.request_focus(self.shell.ui.query_input_id));
            } else {
                ctx.memory_mut(|m| m.stop_text_input());
            }
        }
        let tab_backward = ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab));
        if tab_backward {
            self.toggle_pin_current();
            // Keep Shift+Tab dedicated to pin toggle without changing query focus active/inactive state.
            if query_focused {
                ctx.memory_mut(|m| m.request_focus(self.shell.ui.query_input_id));
            } else {
                ctx.memory_mut(|m| m.stop_text_input());
            }
        }
        if Self::consume_emacs_shortcut(ctx, egui::Key::I, false) {
            self.toggle_pin_current();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)) {
            self.move_row(1);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)) {
            self.move_row(-1);
        }
        if Self::consume_emacs_shortcut(ctx, egui::Key::J, false)
            || Self::consume_emacs_shortcut(ctx, egui::Key::M, false)
        {
            self.execute_selected();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Enter)) {
            self.execute_selected_open_folder();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
            self.execute_selected();
        }

        if self.shell.ui.ime_composition_active {
            return;
        }
        // Regression guard: query focus must not disable row movement/pin toggle/execute shortcuts.
        if query_focused {
            return;
        }

        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Home)) {
            self.move_to_first_row();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::End)) {
            self.move_to_last_row();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::PageUp)) {
            self.move_page(-1);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::PageDown)) {
            self.move_page(1);
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::V)) {
            self.move_page(1);
        }
        if ctx.input(|i| i.modifiers.alt && i.key_pressed(egui::Key::V)) {
            self.move_page(-1);
        }
    }

    pub(super) fn run_deferred_shortcuts(&mut self, ctx: &egui::Context) {
        if !self.shell.ui.pending_copy_shortcut {
            return;
        }
        self.shell.ui.pending_copy_shortcut = false;
        self.copy_selected_paths(ctx);
        self.shell.ui.focus_query_requested = true;
    }

    pub(super) fn reset_query_history_navigation(&mut self) {
        self.shell.runtime.query_state.query_history_cursor = None;
        self.shell.runtime.query_state.query_history_draft = None;
    }

    pub(super) fn reset_history_search_state(&mut self) {
        self.shell.runtime.query_state.history_search_active = false;
        self.shell.runtime.query_state.history_search_query.clear();
        self.shell.runtime.query_state.history_search_original_query.clear();
        self.shell.runtime.query_state.history_search_results.clear();
        self.shell.runtime.query_state.history_search_current = None;
    }

    fn history_search_score(query: &str, candidate: &str, recency_rank: usize) -> Option<i64> {
        if query.trim().is_empty() {
            return Some(recency_rank as i64);
        }

        let matcher = SkimMatcherV2::default();
        matcher.fuzzy_match(candidate, query).or_else(|| {
            let query_lower = query.to_ascii_lowercase();
            let candidate_lower = candidate.to_ascii_lowercase();
            if candidate_lower.contains(&query_lower) {
                Some((query_lower.len() as i64) * 100 + recency_rank as i64)
            } else {
                None
            }
        })
    }

    pub(super) fn refresh_history_search_results(&mut self) {
        if !self.shell.runtime.query_state.history_search_active {
            self.shell.runtime.query_state.history_search_results.clear();
            self.shell.runtime.query_state.history_search_current = None;
            self.refresh_status_line();
            return;
        }

        let query = self.shell.runtime.query_state.history_search_query.trim();
        let mut scored = self.shell.runtime.query_state
            .query_history
            .iter()
            .rev()
            .enumerate()
            .filter_map(|(idx, entry)| {
                Self::history_search_score(query, entry, Self::QUERY_HISTORY_MAX - idx)
                    .map(|score| (entry.clone(), score, idx))
            })
            .collect::<Vec<_>>();
        scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.2.cmp(&b.2)));
        self.shell.runtime.query_state.history_search_results =
            scored.into_iter().map(|(entry, _, _)| entry).collect();
        self.shell.runtime.query_state.history_search_current =
            (!self.shell.runtime.query_state.history_search_results.is_empty()).then_some(0);
        self.refresh_status_line();
    }

    pub(super) fn start_history_search(&mut self) {
        self.commit_query_history_if_needed(true);
        self.shell.runtime.query_state.history_search_active = true;
        self.shell.runtime.query_state.history_search_query.clear();
        self.shell.runtime.query_state.history_search_original_query = self.shell.runtime.query_state.query.clone();
        self.refresh_history_search_results();
        self.shell.ui.focus_query_requested = true;
        self.shell.ui.unfocus_query_requested = false;
    }

    pub(super) fn cancel_history_search(&mut self) {
        if !self.shell.runtime.query_state.history_search_active {
            return;
        }
        self.shell.runtime.query_state.query = self.shell.runtime.query_state.history_search_original_query.clone();
        self.reset_history_search_state();
        self.update_results();
        self.shell.ui.focus_query_requested = true;
        self.set_notice("Canceled history search");
    }

    pub(super) fn accept_history_search(&mut self) {
        if !self.shell.runtime.query_state.history_search_active {
            return;
        }
        let Some(index) = self.shell.runtime.query_state.history_search_current else {
            return;
        };
        let Some(selected) = self.shell.runtime.query_state.history_search_results.get(index).cloned() else {
            return;
        };
        self.shell.runtime.query_state.query = selected;
        self.reset_query_history_navigation();
        self.shell.runtime.query_state.query_history_dirty_since = None;
        self.reset_history_search_state();
        self.update_results();
        self.shell.ui.focus_query_requested = true;
        self.set_notice("Loaded query from history");
    }

    pub(super) fn move_history_search_selection(&mut self, delta: isize) {
        if !self.shell.runtime.query_state.history_search_active
            || self.shell.runtime.query_state.history_search_results.is_empty()
        {
            return;
        }
        let current = self.shell.runtime.query_state.history_search_current.unwrap_or(0) as isize;
        let next = (current + delta).clamp(
            0,
            self.shell.runtime.query_state.history_search_results.len() as isize - 1,
        );
        self.shell.runtime.query_state.history_search_current = Some(next as usize);
    }

    pub(super) fn mark_query_edited(&mut self) {
        self.reset_query_history_navigation();
        self.shell.runtime.query_state.query_history_dirty_since = Some(Instant::now());
        self.invalidate_result_sort(true);
    }

    pub(super) fn push_query_history(history: &mut VecDeque<String>, query: &str) {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return;
        }
        if history.back().is_some_and(|entry| entry == trimmed) {
            return;
        }
        history.push_back(trimmed.to_string());
        while history.len() > Self::QUERY_HISTORY_MAX {
            history.pop_front();
        }
    }

    pub(super) fn sync_shared_query_history_to_tabs(&mut self) {
        let history = self.shell.runtime.query_state.query_history.clone();
        for tab in &mut self.shell.tabs {
            tab.query_state.query_history = history.clone();
        }
    }

    pub(super) fn commit_query_history_if_needed(&mut self, force: bool) {
        if self.shell.ui.ime_composition_active {
            return;
        }
        let should_commit = self.shell.runtime.query_state
            .query_history_dirty_since
            .is_some_and(|since| force || since.elapsed() >= Self::QUERY_HISTORY_IDLE_DELAY);
        if !should_commit || self.shell.runtime.query_state.query_history_cursor.is_some() {
            return;
        }
        let before_len = self.shell.runtime.query_state.query_history.len();
        let query = self.shell.runtime.query_state.query.clone();
        Self::push_query_history(&mut self.shell.runtime.query_state.query_history, &query);
        self.shell.runtime.query_state.query_history_dirty_since = None;
        if self.shell.runtime.query_state.query_history.len() != before_len
            || self.shell.runtime.query_state
                .query_history
                .back()
                .is_some_and(|entry| entry == query.trim())
        {
            self.sync_shared_query_history_to_tabs();
            self.mark_ui_state_dirty();
        }
    }

    pub(super) fn process_query_input_events(
        &mut self,
        ctx: &egui::Context,
        events: &[egui::Event],
        query_focused: bool,
        text_changed_by_widget: bool,
        cursor_range: Option<egui::text::CCursorRange>,
    ) -> (bool, Option<usize>) {
        let mut changed = false;
        let mut saw_text_space = false;
        let mut saw_composition_update = false;
        let mut fallback_space: Option<char> = None;
        let mut saw_space_key = false;
        let mut composition_commit_text: Option<String> = None;
        let mut requested_full_space = false;
        let mut cursor_changed = false;
        let initial_cursor = cursor_range
            .map(|range| range.primary.index)
            .unwrap_or_else(|| Self::char_count(&self.shell.runtime.query_state.query));
        let initial_anchor = cursor_range
            .map(|range| range.secondary.index)
            .unwrap_or(initial_cursor);
        let mut cursor = initial_cursor.min(Self::char_count(&self.shell.runtime.query_state.query));
        let mut anchor = initial_anchor.min(Self::char_count(&self.shell.runtime.query_state.query));

        for event in events {
            match event {
                egui::Event::Ime(egui::ImeEvent::Enabled) => {
                    self.shell.ui.ime_composition_active = true;
                    Self::append_window_trace("ime_composition_start", "active=true");
                }
                egui::Event::Ime(egui::ImeEvent::Preedit(text)) => {
                    self.shell.ui.ime_composition_active = true;
                    if !text.is_empty() {
                        saw_composition_update = true;
                        Self::append_window_trace(
                            "ime_composition_update",
                            &format!("chars={}", text.chars().count()),
                        );
                    }
                }
                egui::Event::Ime(egui::ImeEvent::Commit(text)) => {
                    self.shell.ui.ime_composition_active = false;
                    Self::append_window_trace(
                        "ime_composition_end",
                        &format!(
                            "chars={} has_half={} has_full={}",
                            text.chars().count(),
                            text.contains(' '),
                            text.contains('\u{3000}')
                        ),
                    );
                    if !text.is_empty() {
                        composition_commit_text = Some(text.clone());
                        if text.contains(' ') || text.contains('\u{3000}') {
                            saw_text_space = true;
                        }
                    }
                }
                egui::Event::Ime(egui::ImeEvent::Disabled) => {
                    self.shell.ui.ime_composition_active = false;
                    Self::append_window_trace("ime_composition_disabled", "active=false");
                }
                egui::Event::Text(text) => {
                    if text.contains(' ') || text.contains('\u{3000}') {
                        saw_text_space = true;
                        Self::append_window_trace(
                            "ime_text_space_seen",
                            &format!(
                                "has_half={} has_full={} chars={}",
                                text.contains(' '),
                                text.contains('\u{3000}'),
                                text.chars().count()
                            ),
                        );
                    }
                }
                egui::Event::Key {
                    key: egui::Key::Space,
                    pressed: true,
                    modifiers,
                    ..
                } if query_focused
                    && !modifiers.ctrl
                    && !modifiers.alt
                    && !modifiers.command
                    && !modifiers.mac_cmd =>
                {
                    saw_space_key = true;
                    requested_full_space = modifiers.shift;
                    fallback_space = Some(' ');
                }
                _ => {}
            }
        }

        let space_down_now = ctx.input(|i| i.key_down(egui::Key::Space));
        let shift_down_now = ctx.input(|i| i.modifiers.shift);
        if query_focused && space_down_now && !self.shell.ui.prev_space_down && fallback_space.is_none() {
            requested_full_space = shift_down_now;
            fallback_space = Some(' ');
            saw_space_key = true;
            Self::append_window_trace(
                "ime_space_keydown_edge",
                &format!("shift={}", shift_down_now),
            );
        }
        self.shell.ui.prev_space_down = space_down_now;

        if let Some(commit_text) = composition_commit_text {
            if query_focused && !text_changed_by_widget {
                if let Some((start, end)) = Self::selection_range(cursor, anchor) {
                    Self::remove_char_range(&mut self.shell.runtime.query_state.query, start, end);
                    cursor = start;
                }
                Self::insert_at_char(&mut self.shell.runtime.query_state.query, cursor, &commit_text);
                cursor += Self::char_count(&commit_text);
                anchor = cursor;
                changed = true;
                cursor_changed = true;
                Self::append_window_trace(
                    "ime_composition_commit_fallback",
                    &format!(
                        "chars={} query_chars_after={}",
                        commit_text.chars().count(),
                        self.shell.runtime.query_state.query.chars().count()
                    ),
                );
            }
        } else if query_focused && text_changed_by_widget {
            Self::append_window_trace(
                "ime_composition_commit_widget_owned",
                &format!(
                    "query_chars_after={}",
                    self.shell.runtime.query_state.query.chars().count()
                ),
            );
        }

        if query_focused && !saw_text_space && !saw_composition_update {
            if let Some(space) = fallback_space {
                if let Some((start, end)) = Self::selection_range(cursor, anchor) {
                    Self::remove_char_range(&mut self.shell.runtime.query_state.query, start, end);
                    cursor = start;
                }
                // Keep IME fallback insertion at the caret instead of forcing tail append.
                Self::insert_at_char(&mut self.shell.runtime.query_state.query, cursor, &space.to_string());
                cursor += 1;
                changed = true;
                cursor_changed = true;
                Self::append_window_trace(
                    "ime_space_fallback_inserted",
                    &format!("kind={}", if space == '\u{3000}' { "full" } else { "half" }),
                );
            }
        } else if saw_space_key {
            Self::append_window_trace(
                "ime_space_fallback_skipped",
                &format!(
                    "focused={} widget_changed={} comp_active={} text_space={} comp_update={} requested_full={} fallback_present={}",
                    query_focused,
                    text_changed_by_widget,
                    self.shell.ui.ime_composition_active,
                    saw_text_space,
                    saw_composition_update,
                    requested_full_space,
                    fallback_space.is_some()
                ),
            );
        }

        (changed, cursor_changed.then_some(cursor))
    }
}

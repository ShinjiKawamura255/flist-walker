use super::super::FlistWalkerApp;
use eframe::egui;

impl FlistWalkerApp {
    pub(in crate::app) fn normalize_singleline_input(text: &mut String) -> bool {
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

    pub(in crate::app) fn char_count(text: &str) -> usize {
        text.chars().count()
    }

    pub(in crate::app) fn byte_index_at_char(text: &str, char_index: usize) -> usize {
        if char_index == 0 {
            return 0;
        }
        text.char_indices()
            .nth(char_index)
            .map(|(idx, _)| idx)
            .unwrap_or(text.len())
    }

    pub(in crate::app) fn remove_char_range(text: &mut String, start: usize, end: usize) -> String {
        if start >= end {
            return String::new();
        }
        let start_byte = Self::byte_index_at_char(text, start);
        let end_byte = Self::byte_index_at_char(text, end);
        let removed = text[start_byte..end_byte].to_string();
        text.replace_range(start_byte..end_byte, "");
        removed
    }

    pub(in crate::app) fn insert_at_char(text: &mut String, pos: usize, value: &str) {
        let byte_pos = Self::byte_index_at_char(text, pos);
        text.insert_str(byte_pos, value);
    }

    pub(in crate::app) fn is_word_char(ch: char) -> bool {
        ch.is_alphanumeric() || ch == '_' || ch == '-'
    }

    pub(in crate::app) fn selection_range(cursor: usize, anchor: usize) -> Option<(usize, usize)> {
        if cursor == anchor {
            None
        } else {
            Some((cursor.min(anchor), cursor.max(anchor)))
        }
    }

    pub(in crate::app) fn apply_ctrl_h_delete(
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

    pub(in crate::app) fn apply_emacs_query_shortcuts(
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
                    Self::remove_char_range(
                        &mut self.shell.runtime.query_state.query,
                        cursor,
                        cursor + 1,
                    );
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
                    self.shell.runtime.query_state.kill_buffer = Self::remove_char_range(
                        &mut self.shell.runtime.query_state.query,
                        start,
                        cursor,
                    );
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
                Self::insert_at_char(
                    &mut self.shell.runtime.query_state.query,
                    cursor,
                    &kill_buffer,
                );
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
}

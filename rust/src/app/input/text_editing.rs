use super::super::FlistWalkerApp;
use crate::text_editing::{apply_emacs_edit, CursorRange, EditOutcome, EmacsEdit};
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

    pub(in crate::app) fn apply_ctrl_h_delete(
        text: &mut String,
        kill_buffer: &mut String,
        cursor: &mut usize,
        anchor: &mut usize,
        text_already_changed: bool,
    ) -> (bool, bool) {
        // Some backends map Ctrl+H to Backspace at the widget level.
        // Avoid applying our delete logic twice in the same frame.
        if text_already_changed {
            return (false, false);
        }

        let mut range = CursorRange {
            primary: *cursor,
            anchor: *anchor,
        };
        let outcome = apply_emacs_edit(text, &mut range, kill_buffer, EmacsEdit::DeleteBackward);
        *cursor = range.primary;
        *anchor = range.anchor;
        (outcome.text_changed, outcome.cursor_changed)
    }

    pub(in crate::app) fn apply_emacs_query_shortcuts(
        &mut self,
        ctx: &egui::Context,
        output: &mut egui::text_edit::TextEditOutput,
        text_before_widget: &str,
    ) -> bool {
        let enabled = self.shell.runtime.emacs_keybindings_enabled;
        let ime_composition_active = self.shell.ui.ime_composition_active;
        let query_state = &mut self.shell.runtime.query_state;
        Self::apply_emacs_text_edit_shortcuts(
            ctx,
            output,
            &mut query_state.query,
            &mut query_state.kill_buffer,
            enabled,
            ime_composition_active,
            text_before_widget,
        )
    }

    pub(in crate::app) fn apply_emacs_history_search_shortcuts(
        &mut self,
        ctx: &egui::Context,
        output: &mut egui::text_edit::TextEditOutput,
        text_before_widget: &str,
    ) -> bool {
        let enabled = self.shell.runtime.emacs_keybindings_enabled;
        let ime_composition_active = self.shell.ui.ime_composition_active;
        let query_state = &mut self.shell.runtime.query_state;
        Self::apply_emacs_text_edit_shortcuts(
            ctx,
            output,
            &mut query_state.history_search_query,
            &mut query_state.kill_buffer,
            enabled,
            ime_composition_active,
            text_before_widget,
        )
    }

    // Regression guard: every application-owned single-line TextEdit uses this
    // adapter so Emacs editing chords do not stop at the main query field. Keep
    // preset/root editors and future text inputs paired with regression_emacs_ctrl_*.
    pub(in crate::app) fn apply_emacs_text_edit_shortcuts(
        ctx: &egui::Context,
        output: &mut egui::text_edit::TextEditOutput,
        text: &mut String,
        kill_buffer: &mut String,
        enabled: bool,
        ime_composition_active: bool,
        text_before_widget: &str,
    ) -> bool {
        if !enabled || ime_composition_active || !output.response.has_focus() {
            return false;
        }

        let emacs_mods = egui::Modifiers {
            ctrl: true,
            ..Default::default()
        };
        let pressed = |key: egui::Key| ctx.input_mut(|i| i.consume_key(emacs_mods, key));

        let char_len = text.chars().count();
        let ccursor =
            output.state.cursor.char_range().unwrap_or_else(|| {
                egui::text::CCursorRange::one(egui::text::CCursor::new(char_len))
            });
        let mut cursor = CursorRange {
            primary: ccursor.primary.index.0.min(char_len),
            anchor: ccursor.secondary.index.0.min(char_len),
        };

        let edit = if pressed(egui::Key::A) {
            Some(EmacsEdit::MoveToStart)
        } else if pressed(egui::Key::E) {
            Some(EmacsEdit::MoveToEnd)
        } else if pressed(egui::Key::B) {
            Some(EmacsEdit::MoveBackward)
        } else if pressed(egui::Key::F) {
            Some(EmacsEdit::MoveForward)
        } else if pressed(egui::Key::H) {
            Some(EmacsEdit::DeleteBackward)
        } else if pressed(egui::Key::D) {
            Some(EmacsEdit::DeleteForward)
        } else if pressed(egui::Key::K) {
            Some(EmacsEdit::KillToEnd)
        } else if pressed(egui::Key::Y) {
            Some(EmacsEdit::Yank)
        } else if pressed(egui::Key::U) {
            Some(EmacsEdit::KillToStart)
        } else {
            None
        };
        let Some(edit) = edit else {
            return false;
        };

        let widget_changed_text = output.response.changed();
        let outcome = if matches!(edit, EmacsEdit::KillToEnd | EmacsEdit::KillToStart)
            && widget_changed_text
        {
            // egui currently handles Ctrl+K/U before this adapter. Recover the
            // deleted span so every field still shares the same yank buffer.
            let removed = widget_kill_segment(text_before_widget, text, edit);
            if !removed.is_empty() {
                *kill_buffer = removed;
            }
            EditOutcome {
                text_changed: true,
                cursor_changed: false,
            }
        } else if edit == EmacsEdit::DeleteBackward {
            let mut primary = cursor.primary;
            let mut anchor = cursor.anchor;
            let (text_changed, cursor_changed) = Self::apply_ctrl_h_delete(
                text,
                kill_buffer,
                &mut primary,
                &mut anchor,
                widget_changed_text,
            );
            cursor = CursorRange { primary, anchor };
            EditOutcome {
                text_changed,
                cursor_changed,
            }
        } else {
            apply_emacs_edit(text, &mut cursor, kill_buffer, edit)
        };

        if outcome.cursor_changed || outcome.text_changed {
            output
                .state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(cursor.anchor),
                    egui::text::CCursor::new(cursor.primary),
                )));
            output.state.clone().store(ctx, output.response.id);
            ctx.request_repaint();
        }

        outcome.text_changed
    }

    pub(in crate::app) fn consume_ctrl_w_search_edit(
        &mut self,
        ctx: &egui::Context,
        query_focused: bool,
    ) -> bool {
        if !self.shell.runtime.emacs_keybindings_enabled
            || !self.shell.runtime.ctrl_w_deletes_word_in_query
            || !query_focused
        {
            return false;
        }
        let ctrl_mods = egui::Modifiers {
            ctrl: true,
            ..Default::default()
        };
        if !ctx.input_mut(|input| input.consume_key(ctrl_mods, egui::Key::W)) {
            return false;
        }
        if self.shell.ui.ime_composition_active {
            return true;
        }

        let editing_history_search = self.shell.runtime.query_state.history_search_active;
        let query_input_id = self.shell.ui.query_input_id();
        let mut text_state =
            egui::widgets::text_edit::TextEditState::load(ctx, query_input_id).unwrap_or_default();
        let char_len = if editing_history_search {
            self.shell
                .runtime
                .query_state
                .history_search_query
                .chars()
                .count()
        } else {
            self.shell.runtime.query_state.query.chars().count()
        };
        let ccursor = text_state
            .cursor
            .char_range()
            .unwrap_or_else(|| egui::text::CCursorRange::one(egui::text::CCursor::new(char_len)));
        let mut cursor = CursorRange {
            primary: ccursor.primary.index.0.min(char_len),
            anchor: ccursor.secondary.index.0.min(char_len),
        };
        let query_state = &mut self.shell.runtime.query_state;
        let outcome = if editing_history_search {
            apply_emacs_edit(
                &mut query_state.history_search_query,
                &mut cursor,
                &mut query_state.kill_buffer,
                EmacsEdit::KillBackwardWord,
            )
        } else {
            apply_emacs_edit(
                &mut query_state.query,
                &mut cursor,
                &mut query_state.kill_buffer,
                EmacsEdit::KillBackwardWord,
            )
        };
        if outcome.cursor_changed || outcome.text_changed {
            text_state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(cursor.anchor),
                    egui::text::CCursor::new(cursor.primary),
                )));
            text_state.store(ctx, query_input_id);
            ctx.request_repaint();
        }
        if outcome.text_changed {
            if editing_history_search {
                self.refresh_history_search_results();
            } else {
                self.mark_query_edited();
                self.update_results();
            }
        }
        true
    }

    pub(in crate::app) fn consume_disabled_emacs_text_edit_shortcuts(
        ctx: &egui::Context,
        input_focused: bool,
        emacs_enabled: bool,
    ) {
        if emacs_enabled || !input_focused {
            return;
        }

        let keys = [
            egui::Key::A,
            egui::Key::E,
            egui::Key::B,
            egui::Key::F,
            egui::Key::H,
            egui::Key::D,
            egui::Key::W,
            egui::Key::K,
            egui::Key::Y,
            egui::Key::U,
        ];
        for key in keys {
            let ctrl_mods = egui::Modifiers {
                ctrl: true,
                ..Default::default()
            };
            let command_mods = egui::Modifiers {
                command: true,
                ..Default::default()
            };
            let _ = ctx.input_mut(|i| i.consume_key(ctrl_mods, key));
            let _ = ctx.input_mut(|i| i.consume_key(command_mods, key));
        }
    }
}

fn widget_kill_segment(before: &str, after: &str, edit: EmacsEdit) -> String {
    let before = before.chars().collect::<Vec<_>>();
    let after = after.chars().collect::<Vec<_>>();
    if after.len() >= before.len() {
        return String::new();
    }

    let removed_len = before.len() - after.len();
    match edit {
        EmacsEdit::KillToEnd => before[after.len()..].iter().collect(),
        EmacsEdit::KillToStart => before[..removed_len].iter().collect(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widget_kill_segment_preserves_unicode_and_repeated_text_direction() {
        assert_eq!(
            widget_kill_segment("界a界bc", "界a", EmacsEdit::KillToEnd),
            "界bc"
        );
        assert_eq!(
            widget_kill_segment("界a界bc", "界bc", EmacsEdit::KillToStart),
            "界a"
        );
    }
}

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

        let mut range = CursorRange {
            primary: *cursor,
            anchor: *anchor,
        };
        let query_state = &mut self.shell.runtime.query_state;
        let outcome = apply_emacs_edit(
            &mut query_state.query,
            &mut range,
            &mut query_state.kill_buffer,
            EmacsEdit::DeleteBackward,
        );
        *cursor = range.primary;
        *anchor = range.anchor;
        (outcome.text_changed, outcome.cursor_changed)
    }

    pub(in crate::app) fn apply_emacs_query_shortcuts(
        &mut self,
        ctx: &egui::Context,
        output: &mut egui::text_edit::TextEditOutput,
    ) -> bool {
        if !self.shell.runtime.emacs_keybindings_enabled {
            return false;
        }
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

        let char_len = self.shell.runtime.query_state.query.chars().count();
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
        } else if pressed(egui::Key::W) {
            Some(EmacsEdit::KillBackwardWord)
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

        let outcome = if edit == EmacsEdit::DeleteBackward {
            let mut primary = cursor.primary;
            let mut anchor = cursor.anchor;
            let (text_changed, cursor_changed) =
                self.apply_ctrl_h_delete(&mut primary, &mut anchor, output.response.changed());
            cursor = CursorRange { primary, anchor };
            EditOutcome {
                text_changed,
                cursor_changed,
            }
        } else {
            let query_state = &mut self.shell.runtime.query_state;
            apply_emacs_edit(
                &mut query_state.query,
                &mut cursor,
                &mut query_state.kill_buffer,
                edit,
            )
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

    pub(in crate::app) fn consume_disabled_emacs_query_edit_shortcuts(
        &self,
        ctx: &egui::Context,
        query_focused: bool,
    ) {
        if self.shell.runtime.emacs_keybindings_enabled || !query_focused {
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

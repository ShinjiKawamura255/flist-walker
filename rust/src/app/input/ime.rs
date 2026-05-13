use super::super::FlistWalkerApp;
use eframe::egui;

impl FlistWalkerApp {
    pub(in crate::app) fn process_query_input_events(
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
        let mut cursor =
            initial_cursor.min(Self::char_count(&self.shell.runtime.query_state.query));
        let mut anchor =
            initial_anchor.min(Self::char_count(&self.shell.runtime.query_state.query));

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
                egui::Event::Text(text) if text.contains(' ') || text.contains('\u{3000}') => {
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
                egui::Event::Text(_) => {}
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
        if query_focused
            && space_down_now
            && !self.shell.ui.prev_space_down
            && fallback_space.is_none()
        {
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
                Self::insert_at_char(
                    &mut self.shell.runtime.query_state.query,
                    cursor,
                    &commit_text,
                );
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
                Self::insert_at_char(
                    &mut self.shell.runtime.query_state.query,
                    cursor,
                    &space.to_string(),
                );
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

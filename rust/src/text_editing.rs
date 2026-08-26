#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EmacsEdit {
    MoveToStart,
    MoveToEnd,
    MoveBackward,
    MoveForward,
    DeleteBackward,
    DeleteForward,
    KillBackwardWord,
    KillToEnd,
    Yank,
    KillToStart,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CursorRange {
    pub(crate) primary: usize,
    pub(crate) anchor: usize,
}

impl CursorRange {
    pub(crate) const fn collapsed(position: usize) -> Self {
        Self {
            primary: position,
            anchor: position,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct EditOutcome {
    pub(crate) text_changed: bool,
    pub(crate) cursor_changed: bool,
}

pub(crate) fn apply_emacs_edit(
    text: &mut String,
    cursor: &mut CursorRange,
    kill_buffer: &mut String,
    edit: EmacsEdit,
) -> EditOutcome {
    let char_len = text.chars().count();
    cursor.primary = cursor.primary.min(char_len);
    cursor.anchor = cursor.anchor.min(char_len);
    let original_cursor = *cursor;
    let mut text_changed = false;

    match edit {
        EmacsEdit::MoveToStart => *cursor = CursorRange::collapsed(0),
        EmacsEdit::MoveToEnd => *cursor = CursorRange::collapsed(char_len),
        EmacsEdit::MoveBackward => {
            *cursor = CursorRange::collapsed(cursor.primary.saturating_sub(1));
        }
        EmacsEdit::MoveForward => {
            *cursor = CursorRange::collapsed((cursor.primary + 1).min(char_len));
        }
        EmacsEdit::DeleteBackward => {
            let range = cursor_selection_range(*cursor)
                .or_else(|| (cursor.primary > 0).then_some((cursor.primary - 1, cursor.primary)));
            if let Some((start, end)) = range {
                remove_char_range(text, start, end);
                *cursor = CursorRange::collapsed(start);
                text_changed = true;
            }
        }
        EmacsEdit::DeleteForward => {
            let range = cursor_selection_range(*cursor).or_else(|| {
                (cursor.primary < char_len).then_some((cursor.primary, cursor.primary + 1))
            });
            if let Some((start, end)) = range {
                remove_char_range(text, start, end);
                *cursor = CursorRange::collapsed(start);
                text_changed = true;
            }
        }
        EmacsEdit::KillBackwardWord => {
            let range = cursor_selection_range(*cursor).or_else(|| {
                let chars = text.chars().collect::<Vec<_>>();
                let mut start = cursor.primary;
                while start > 0 && chars[start - 1].is_whitespace() {
                    start -= 1;
                }
                while start > 0 && is_word_char(chars[start - 1]) {
                    start -= 1;
                }
                (start < cursor.primary).then_some((start, cursor.primary))
            });
            if let Some((start, end)) = range {
                *kill_buffer = remove_char_range(text, start, end);
                *cursor = CursorRange::collapsed(start);
                text_changed = true;
            }
        }
        EmacsEdit::KillToEnd if cursor.primary < char_len => {
            *kill_buffer = remove_char_range(text, cursor.primary, char_len);
            *cursor = CursorRange::collapsed(cursor.primary);
            text_changed = true;
        }
        EmacsEdit::Yank if !kill_buffer.is_empty() => {
            let insertion_point = if let Some((start, end)) = cursor_selection_range(*cursor) {
                remove_char_range(text, start, end);
                start
            } else {
                cursor.primary
            };
            let inserted_chars = kill_buffer.chars().count();
            insert_at_char(text, insertion_point, kill_buffer);
            *cursor = CursorRange::collapsed(insertion_point + inserted_chars);
            text_changed = true;
        }
        EmacsEdit::KillToStart if cursor.primary > 0 => {
            remove_char_range(text, 0, cursor.primary);
            *cursor = CursorRange::collapsed(0);
            text_changed = true;
        }
        EmacsEdit::KillToEnd | EmacsEdit::Yank | EmacsEdit::KillToStart => {}
    }

    EditOutcome {
        text_changed,
        cursor_changed: *cursor != original_cursor,
    }
}

pub(crate) fn char_to_byte_index(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

pub(crate) fn char_count(text: &str) -> usize {
    text.chars().count()
}

pub(crate) fn selected_char_range(primary: usize, anchor: usize) -> Option<(usize, usize)> {
    (primary != anchor).then_some((primary.min(anchor), primary.max(anchor)))
}

fn cursor_selection_range(cursor: CursorRange) -> Option<(usize, usize)> {
    selected_char_range(cursor.primary, cursor.anchor)
}

pub(crate) fn remove_char_range(text: &mut String, start: usize, end: usize) -> String {
    if start >= end {
        return String::new();
    }
    let start_byte = char_to_byte_index(text, start);
    let end_byte = char_to_byte_index(text, end);
    let removed = text[start_byte..end_byte].to_string();
    text.replace_range(start_byte..end_byte, "");
    removed
}

pub(crate) fn insert_at_char(text: &mut String, position: usize, value: &str) {
    let byte_index = char_to_byte_index(text, position);
    text.insert_str(byte_index, value);
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '-'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backward_delete_removes_a_unicode_scalar_and_collapses_selection() {
        let mut text = "a界b".to_string();
        let mut cursor = CursorRange::collapsed(2);
        let mut kill_buffer = String::new();

        let outcome = apply_emacs_edit(
            &mut text,
            &mut cursor,
            &mut kill_buffer,
            EmacsEdit::DeleteBackward,
        );

        assert_eq!(text, "ab");
        assert_eq!(cursor, CursorRange::collapsed(1));
        assert_eq!(
            outcome,
            EditOutcome {
                text_changed: true,
                cursor_changed: true
            }
        );
    }

    #[test]
    fn backward_word_uses_the_gui_word_boundary_contract() {
        let mut text = "alpha/beta".to_string();
        let mut cursor = CursorRange::collapsed(text.chars().count());
        let mut kill_buffer = String::new();

        apply_emacs_edit(
            &mut text,
            &mut cursor,
            &mut kill_buffer,
            EmacsEdit::KillBackwardWord,
        );

        assert_eq!(text, "alpha/");
        assert_eq!(kill_buffer, "beta");
        assert_eq!(cursor, CursorRange::collapsed(6));
    }

    #[test]
    fn yank_replaces_the_selected_range() {
        let mut text = "alpha beta".to_string();
        let mut cursor = CursorRange {
            primary: 10,
            anchor: 6,
        };
        let mut kill_buffer = "gamma".to_string();

        apply_emacs_edit(&mut text, &mut cursor, &mut kill_buffer, EmacsEdit::Yank);

        assert_eq!(text, "alpha gamma");
        assert_eq!(cursor, CursorRange::collapsed(11));
    }
}

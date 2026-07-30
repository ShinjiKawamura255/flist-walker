use super::protocol::{KeyAction, TuiRuntimeOptions};
use super::state::{
    refresh_history_results, FileListConfirmation, HistoryOverlay, OptionsOverlay, TuiState,
    SORT_MODES,
};
use crate::actions::AuthorizedActionMode;
use crate::persistence::AsyncHistoryPersistence;
use crate::search::SearchSortMode;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;
use std::time::Instant;

pub(super) fn history_move_selection(
    history: &mut HistoryOverlay,
    delta: isize,
    viewport_rows: usize,
) {
    if history.results.is_empty() {
        history.selected = 0;
        history.offset = 0;
        return;
    }
    history.selected = if delta.is_negative() {
        history.selected.saturating_sub(delta.unsigned_abs())
    } else {
        history
            .selected
            .saturating_add(delta as usize)
            .min(history.results.len() - 1)
    };
    let viewport_rows = viewport_rows.max(1);
    if history.selected < history.offset {
        history.offset = history.selected;
    } else if history.selected >= history.offset + viewport_rows {
        history.offset = history.selected + 1 - viewport_rows;
    }
    history.offset = history
        .offset
        .min(history.results.len().saturating_sub(viewport_rows));
}

pub(super) fn edit_history_filter(
    history: &mut HistoryOverlay,
    entries: &[String],
    key: KeyCode,
) -> bool {
    match key {
        KeyCode::Backspace if history.filter_cursor > 0 => {
            let start = char_to_byte_index(&history.filter, history.filter_cursor - 1);
            let end = char_to_byte_index(&history.filter, history.filter_cursor);
            history.filter.replace_range(start..end, "");
            history.filter_cursor -= 1;
        }
        KeyCode::Delete if history.filter_cursor < history.filter.chars().count() => {
            let start = char_to_byte_index(&history.filter, history.filter_cursor);
            let end = char_to_byte_index(&history.filter, history.filter_cursor + 1);
            history.filter.replace_range(start..end, "");
        }
        KeyCode::Left => history.filter_cursor = history.filter_cursor.saturating_sub(1),
        KeyCode::Right => {
            history.filter_cursor = (history.filter_cursor + 1).min(history.filter.chars().count())
        }
        KeyCode::Home => history.filter_cursor = 0,
        KeyCode::End => history.filter_cursor = history.filter.chars().count(),
        KeyCode::Char(ch) if !ch.is_control() => {
            let byte_index = char_to_byte_index(&history.filter, history.filter_cursor);
            history.filter.insert(byte_index, ch);
            history.filter_cursor += 1;
        }
        _ => return false,
    }
    refresh_history_results(history, entries);
    true
}

pub(super) fn enqueue_history_delta(
    persistence: Option<&AsyncHistoryPersistence>,
    query: &str,
) -> Result<(), String> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(());
    }
    if let Some(persistence) = persistence {
        persistence.enqueue_history(vec![query.to_string()])?;
    }
    Ok(())
}

pub(super) fn selected_paths(state: &TuiState) -> Vec<PathBuf> {
    if !state.pinned.is_empty() {
        return state.pinned.clone();
    }
    state
        .results
        .get(state.selected)
        .map(|(path, _)| vec![path.clone()])
        .unwrap_or_default()
}

pub(super) fn move_overlay_selection(selected: &mut usize, delta: isize, len: usize) {
    if delta.is_negative() {
        *selected = selected.saturating_sub(delta.unsigned_abs());
    } else {
        *selected = selected
            .saturating_add(delta as usize)
            .min(len.saturating_sub(1));
    }
}

pub(super) fn toggle_option(overlay: &mut OptionsOverlay) {
    match overlay.selected {
        0 if overlay.draft.include_dirs => {
            overlay.draft.include_files = !overlay.draft.include_files
        }
        1 if overlay.draft.include_files => {
            overlay.draft.include_dirs = !overlay.draft.include_dirs
        }
        2 => overlay.draft.regex = !overlay.draft.regex,
        3 => overlay.draft.ignore_case = !overlay.draft.ignore_case,
        4 => overlay.draft.ignore_enabled = !overlay.draft.ignore_enabled,
        5 => overlay.draft.source = overlay.draft.source.next(),
        _ => {}
    }
}

pub(super) fn option_change_requires_reindex(
    before: TuiRuntimeOptions,
    after: TuiRuntimeOptions,
) -> bool {
    before.include_files != after.include_files
        || before.include_dirs != after.include_dirs
        || before.source != after.source
}

pub(super) fn handle_options_key(state: &mut TuiState, key: KeyEvent) -> KeyAction {
    if matches!(
        (key.code, key.modifiers),
        (KeyCode::Char('c'), KeyModifiers::CONTROL)
    ) {
        return KeyAction::Cancel;
    }
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) | (KeyCode::Char('g'), KeyModifiers::CONTROL) => {
            state.options_overlay = None;
        }
        (KeyCode::Enter, _) => {
            let Some(overlay) = state.options_overlay.take() else {
                return KeyAction::Continue;
            };
            let previous = state.runtime_options;
            let changed = previous != overlay.draft;
            let reindex = option_change_requires_reindex(previous, overlay.draft);
            let source_changed = previous.source != overlay.draft.source;
            state.runtime_options = overlay.draft;
            if changed {
                state.sort_mode = SearchSortMode::Score;
                state.active_search_request_id = None;
                state.source_changed_on_apply = source_changed;
            }
            if reindex {
                state.status = "Reindexing...".to_string();
                state.dirty = true;
                return KeyAction::Reindex;
            }
            state.status = "Options applied".to_string();
            state.last_query_change = Some(Instant::now());
        }
        (KeyCode::Up, _) => {
            if let Some(overlay) = state.options_overlay.as_mut() {
                move_overlay_selection(&mut overlay.selected, -1, 6);
            }
        }
        (KeyCode::Down, _) => {
            if let Some(overlay) = state.options_overlay.as_mut() {
                move_overlay_selection(&mut overlay.selected, 1, 6);
            }
        }
        (KeyCode::Char(' '), _) | (KeyCode::Left, _) | (KeyCode::Right, _) => {
            if let Some(overlay) = state.options_overlay.as_mut() {
                toggle_option(overlay);
            }
        }
        _ => {}
    }
    state.dirty = true;
    KeyAction::Continue
}

pub(super) fn handle_sort_key(state: &mut TuiState, key: KeyEvent) -> KeyAction {
    if matches!(
        (key.code, key.modifiers),
        (KeyCode::Char('c'), KeyModifiers::CONTROL)
    ) {
        return KeyAction::Cancel;
    }
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) | (KeyCode::Char('g'), KeyModifiers::CONTROL) => state.sort_picker = None,
        (KeyCode::Enter, _) => {
            if let Some(picker) = state.sort_picker.take() {
                state.sort_mode = SORT_MODES[picker.selected];
                state.status = format!("Sorting by {}...", state.sort_mode.label());
                state.last_query_change = Some(Instant::now());
            }
        }
        (KeyCode::Up, _) => {
            if let Some(picker) = state.sort_picker.as_mut() {
                move_overlay_selection(&mut picker.selected, -1, SORT_MODES.len());
            }
        }
        (KeyCode::Down, _) => {
            if let Some(picker) = state.sort_picker.as_mut() {
                move_overlay_selection(&mut picker.selected, 1, SORT_MODES.len());
            }
        }
        _ => {}
    }
    state.dirty = true;
    KeyAction::Continue
}

pub(super) fn handle_root_picker_key(state: &mut TuiState, key: KeyEvent) -> KeyAction {
    if matches!(
        (key.code, key.modifiers),
        (KeyCode::Char('c'), KeyModifiers::CONTROL)
    ) {
        return KeyAction::Cancel;
    }
    let Some(picker) = state.root_picker.as_mut() else {
        return KeyAction::Continue;
    };
    if state.saved_roots.is_empty() {
        if matches!(
            (key.code, key.modifiers),
            (KeyCode::Enter, _) | (KeyCode::Esc, _) | (KeyCode::Char('g'), KeyModifiers::CONTROL)
        ) {
            state.root_picker = None;
        }
        state.dirty = true;
        return KeyAction::Continue;
    }
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) | (KeyCode::Char('g'), KeyModifiers::CONTROL) => state.root_picker = None,
        (KeyCode::Enter, _) => {
            let root = state.saved_roots[picker.selected].clone();
            state.root_picker = None;
            state.dirty = true;
            return KeyAction::SwitchRoot(root);
        }
        (KeyCode::Up, _) => {
            move_overlay_selection(&mut picker.selected, -1, state.saved_roots.len())
        }
        (KeyCode::Down, _) => {
            move_overlay_selection(&mut picker.selected, 1, state.saved_roots.len())
        }
        (KeyCode::PageUp, _) => move_overlay_selection(
            &mut picker.selected,
            -(state.viewport_rows.max(1) as isize),
            state.saved_roots.len(),
        ),
        (KeyCode::PageDown, _) => move_overlay_selection(
            &mut picker.selected,
            state.viewport_rows.max(1) as isize,
            state.saved_roots.len(),
        ),
        _ => {}
    }
    state.dirty = true;
    KeyAction::Continue
}

pub(super) fn handle_filelist_confirmation_key(state: &mut TuiState, key: KeyEvent) -> KeyAction {
    let Some(confirmation) = state.filelist_confirmation.as_mut() else {
        return KeyAction::Continue;
    };
    if matches!(
        (key.code, key.modifiers),
        (KeyCode::Char('c'), KeyModifiers::CONTROL)
    ) {
        return KeyAction::Cancel;
    }
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) | (KeyCode::Char('g'), KeyModifiers::CONTROL) => {
            state.filelist_confirmation = None;
        }
        (KeyCode::Enter, _) => match confirmation {
            FileListConfirmation::Mode {
                propagate_to_ancestors,
            } if state.root_filelist_known && state.root_filelist_exists => {
                let propagate_to_ancestors = *propagate_to_ancestors;
                state.filelist_confirmation = Some(FileListConfirmation::Overwrite {
                    propagate_to_ancestors,
                });
            }
            FileListConfirmation::Mode {
                propagate_to_ancestors,
            } => {
                let propagate_to_ancestors = *propagate_to_ancestors;
                state.filelist_confirmation = None;
                state.dirty = true;
                return KeyAction::StartFileList {
                    propagate_to_ancestors,
                    allow_root_overwrite: false,
                };
            }
            FileListConfirmation::Overwrite {
                propagate_to_ancestors,
            } => {
                let propagate_to_ancestors = *propagate_to_ancestors;
                state.filelist_confirmation = None;
                state.dirty = true;
                return KeyAction::StartFileList {
                    propagate_to_ancestors,
                    allow_root_overwrite: true,
                };
            }
        },
        (KeyCode::Up, _) | (KeyCode::Down, _) | (KeyCode::Char(' '), _) => {
            if let FileListConfirmation::Mode {
                propagate_to_ancestors,
            } = confirmation
            {
                *propagate_to_ancestors = !*propagate_to_ancestors;
            }
        }
        _ => {}
    }
    state.dirty = true;
    KeyAction::Continue
}

pub(super) fn handle_active_filelist_key(state: &mut TuiState, key: KeyEvent) -> KeyAction {
    match (key.code, key.modifiers) {
        (KeyCode::Char('c'), KeyModifiers::CONTROL)
        | (KeyCode::Char('g'), KeyModifiers::CONTROL)
        | (KeyCode::Esc, _) => KeyAction::Cancel,
        (KeyCode::F(1), _) => {
            state.open_help();
            state.dirty = true;
            KeyAction::Continue
        }
        (KeyCode::F(4), _) => {
            state.open_root_picker();
            state.dirty = true;
            KeyAction::Continue
        }
        (KeyCode::Enter, _) => KeyAction::Select,
        (KeyCode::Up, _) => {
            state.move_selection(-1);
            state.dirty = true;
            KeyAction::Continue
        }
        (KeyCode::Down, _) => {
            state.move_selection(1);
            state.dirty = true;
            KeyAction::Continue
        }
        (KeyCode::PageUp, _) => {
            state.move_selection(-(state.viewport_rows.max(1) as isize));
            state.dirty = true;
            KeyAction::Continue
        }
        (KeyCode::PageDown, _) => {
            state.move_selection(state.viewport_rows.max(1) as isize);
            state.dirty = true;
            KeyAction::Continue
        }
        _ => KeyAction::Continue,
    }
}

pub(super) fn is_emacs_shortcut(key: KeyEvent) -> bool {
    match (key.code, key.modifiers) {
        (KeyCode::Char(ch), KeyModifiers::CONTROL) => matches!(
            ch.to_ascii_lowercase(),
            'a' | 'b'
                | 'd'
                | 'e'
                | 'f'
                | 'g'
                | 'h'
                | 'i'
                | 'j'
                | 'k'
                | 'm'
                | 'n'
                | 'p'
                | 'r'
                | 'u'
                | 'v'
                | 'w'
                | 'y'
        ),
        (KeyCode::Char(ch), KeyModifiers::ALT) => ch.eq_ignore_ascii_case(&'v'),
        _ => false,
    }
}

pub(super) fn normalize_emacs_shortcut(key: KeyEvent) -> KeyEvent {
    let code = match (key.code, key.modifiers) {
        (KeyCode::Char(ch), KeyModifiers::CONTROL) => match ch.to_ascii_lowercase() {
            'n' => Some(KeyCode::Down),
            'p' => Some(KeyCode::Up),
            'v' => Some(KeyCode::PageDown),
            'i' => Some(KeyCode::Tab),
            'j' | 'm' => Some(KeyCode::Enter),
            _ => None,
        },
        (KeyCode::Char(ch), KeyModifiers::ALT) if ch.eq_ignore_ascii_case(&'v') => {
            Some(KeyCode::PageUp)
        }
        _ => None,
    };
    code.map_or(key, |code| KeyEvent::new(code, KeyModifiers::NONE))
}

pub(super) fn apply_emacs_text_editing(
    text: &mut String,
    cursor: &mut usize,
    kill_buffer: &mut String,
    key: KeyEvent,
) -> Option<bool> {
    let (KeyCode::Char(ch), KeyModifiers::CONTROL) = (key.code, key.modifiers) else {
        return None;
    };
    let char_len = text.chars().count();
    let mut changed = false;
    match ch.to_ascii_lowercase() {
        'a' => *cursor = 0,
        'e' => *cursor = char_len,
        'b' => *cursor = cursor.saturating_sub(1),
        'f' => *cursor = (*cursor + 1).min(char_len),
        'h' if *cursor > 0 => {
            let start = char_to_byte_index(text, *cursor - 1);
            let end = char_to_byte_index(text, *cursor);
            text.replace_range(start..end, "");
            *cursor -= 1;
            changed = true;
        }
        'd' if *cursor < char_len => {
            let start = char_to_byte_index(text, *cursor);
            let end = char_to_byte_index(text, *cursor + 1);
            text.replace_range(start..end, "");
            changed = true;
        }
        'w' if *cursor > 0 => {
            let chars: Vec<char> = text.chars().collect();
            let mut start = *cursor;
            while start > 0 && chars[start - 1].is_whitespace() {
                start -= 1;
            }
            while start > 0 && !chars[start - 1].is_whitespace() {
                start -= 1;
            }
            let start_byte = char_to_byte_index(text, start);
            let end_byte = char_to_byte_index(text, *cursor);
            *kill_buffer = text[start_byte..end_byte].to_string();
            text.replace_range(start_byte..end_byte, "");
            *cursor = start;
            changed = true;
        }
        'k' if *cursor < char_len => {
            let start = char_to_byte_index(text, *cursor);
            *kill_buffer = text[start..].to_string();
            text.truncate(start);
            changed = true;
        }
        'y' if !kill_buffer.is_empty() => {
            let byte_index = char_to_byte_index(text, *cursor);
            text.insert_str(byte_index, kill_buffer);
            *cursor += kill_buffer.chars().count();
            changed = true;
        }
        'u' if *cursor > 0 => {
            let end = char_to_byte_index(text, *cursor);
            text.replace_range(..end, "");
            *cursor = 0;
            changed = true;
        }
        'd' | 'h' | 'k' | 'u' | 'w' | 'y' => {}
        _ => return None,
    }
    Some(changed)
}

pub(super) fn apply_emacs_query_editing(state: &mut TuiState, key: KeyEvent) -> bool {
    let Some(changed) = apply_emacs_text_editing(
        &mut state.query,
        &mut state.query_cursor,
        &mut state.kill_buffer,
        key,
    ) else {
        return false;
    };
    if changed {
        state.mark_query_changed();
    }
    true
}

pub(super) fn toggle_pin_current(state: &mut TuiState) {
    let Some(path) = state
        .results
        .get(state.selected)
        .map(|(path, _)| path.clone())
    else {
        return;
    };
    if let Some(index) = state.pinned.iter().position(|pinned| pinned == &path) {
        state.pinned.remove(index);
    } else {
        state.pinned.push(path);
    }
    if state.tab_pin_moves_to_next_row {
        state.move_selection(1);
    }
}

pub(super) fn handle_key(state: &mut TuiState, key: KeyEvent) -> KeyAction {
    if !state.emacs_keybindings_enabled && is_emacs_shortcut(key) {
        return KeyAction::Continue;
    }
    let original_key = key;
    let key = normalize_emacs_shortcut(key);
    if state.help.is_some() {
        match (key.code, key.modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => return KeyAction::Cancel,
            (KeyCode::Enter, _)
            | (KeyCode::Esc, _)
            | (KeyCode::Char('g'), KeyModifiers::CONTROL) => state.close_help(),
            _ => {}
        }
        state.dirty = true;
        return KeyAction::Continue;
    }
    if state.options_overlay.is_some() {
        return handle_options_key(state, key);
    }
    if state.sort_picker.is_some() {
        return handle_sort_key(state, key);
    }
    if state.root_picker.is_some() {
        return handle_root_picker_key(state, key);
    }
    if state.filelist_confirmation.is_some() {
        return handle_filelist_confirmation_key(state, key);
    }
    if state.active_filelist.is_some() {
        return handle_active_filelist_key(state, key);
    }
    if matches!(key.code, KeyCode::F(1)) {
        state.open_help();
        state.dirty = true;
        return KeyAction::Continue;
    }
    if state.history.is_some() {
        if matches!(
            (key.code, key.modifiers),
            (KeyCode::Char('c'), KeyModifiers::CONTROL)
        ) {
            return KeyAction::Cancel;
        }
        let emacs_edit_handled = {
            let history = state.history.as_mut().expect("history overlay checked");
            match apply_emacs_text_editing(
                &mut history.filter,
                &mut history.filter_cursor,
                &mut state.kill_buffer,
                original_key,
            ) {
                Some(changed) => {
                    if changed {
                        refresh_history_results(history, &state.history_entries);
                    }
                    true
                }
                None => false,
            }
        };
        if emacs_edit_handled {
            state.dirty = true;
            return KeyAction::Continue;
        }
        let viewport_rows = state.viewport_rows;
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) | (KeyCode::Char('g'), KeyModifiers::CONTROL) => {
                state.cancel_history();
            }
            (KeyCode::Enter, _) if state.accept_history().is_some() => {
                state.dirty = true;
                return KeyAction::HistoryApplied;
            }
            (KeyCode::Up, _) => history_move_selection(
                state.history.as_mut().expect("history overlay checked"),
                -1,
                viewport_rows,
            ),
            (KeyCode::Down, _) => history_move_selection(
                state.history.as_mut().expect("history overlay checked"),
                1,
                viewport_rows,
            ),
            (KeyCode::PageUp, _) => history_move_selection(
                state.history.as_mut().expect("history overlay checked"),
                -(viewport_rows.max(1) as isize),
                viewport_rows,
            ),
            (KeyCode::PageDown, _) => history_move_selection(
                state.history.as_mut().expect("history overlay checked"),
                viewport_rows.max(1) as isize,
                viewport_rows,
            ),
            _ if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT => {
                let entries = state.history_entries.clone();
                let _ = edit_history_filter(
                    state.history.as_mut().expect("history overlay checked"),
                    &entries,
                    key.code,
                );
            }
            _ => {}
        }
        state.dirty = true;
        return KeyAction::Continue;
    }
    if matches!(key.code, KeyCode::F(2)) {
        state.open_options();
        state.dirty = true;
        return KeyAction::Continue;
    }
    if matches!(key.code, KeyCode::F(3)) {
        state.open_sort_picker();
        state.dirty = true;
        return KeyAction::Continue;
    }
    if matches!(key.code, KeyCode::F(4)) {
        state.open_root_picker();
        state.dirty = true;
        return KeyAction::Continue;
    }
    if matches!(key.code, KeyCode::F(5)) {
        return KeyAction::Refresh;
    }
    if matches!(key.code, KeyCode::F(6)) {
        return KeyAction::OpenFileList;
    }
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
            return KeyAction::Cancel;
        }
        (KeyCode::Char('g'), KeyModifiers::CONTROL) => {
            state.query.clear();
            state.query_cursor = 0;
            state.pinned.clear();
            state.status = "Query and pins cleared".to_string();
            state.mark_query_changed();
        }
        (KeyCode::Char('r'), KeyModifiers::CONTROL) => {
            if state.history_enabled {
                let query = state.commit_query_to_history();
                state.begin_history();
                state.dirty = true;
                return KeyAction::HistoryOpened(query);
            } else {
                return KeyAction::Continue;
            }
        }
        (KeyCode::Char('p'), KeyModifiers::ALT) | (KeyCode::Char('P'), KeyModifiers::ALT) => {
            state.preview_preferred = !state.preview_preferred;
            if !state.preview_preferred {
                state.preview_visible = false;
                state.clear_preview();
                state.status = "Preview hidden".to_string();
            } else {
                state.status = "Preview enabled".to_string();
            }
        }
        (KeyCode::Char('o'), KeyModifiers::CONTROL) => {
            return KeyAction::DispatchAction(AuthorizedActionMode::ExecuteOrOpen);
        }
        (KeyCode::Enter, KeyModifiers::SHIFT) => {
            return KeyAction::DispatchAction(AuthorizedActionMode::Reveal);
        }
        (KeyCode::Enter, _) => {
            if selected_paths(state).is_empty() {
                state.status = "No selection".to_string();
            } else {
                return KeyAction::Select;
            }
        }
        (KeyCode::Tab, _) | (KeyCode::BackTab, _) => toggle_pin_current(state),
        _ if apply_emacs_query_editing(state, original_key) => {}
        (KeyCode::Backspace, _) if state.query_cursor > 0 => {
            let start = char_to_byte_index(&state.query, state.query_cursor - 1);
            let end = char_to_byte_index(&state.query, state.query_cursor);
            state.query.replace_range(start..end, "");
            state.query_cursor -= 1;
            state.mark_query_changed();
        }
        (KeyCode::Delete, _) if state.query_cursor < state.query.chars().count() => {
            let start = char_to_byte_index(&state.query, state.query_cursor);
            let end = char_to_byte_index(&state.query, state.query_cursor + 1);
            state.query.replace_range(start..end, "");
            state.mark_query_changed();
        }
        (KeyCode::Left, _) => state.query_cursor = state.query_cursor.saturating_sub(1),
        (KeyCode::Right, _) => {
            state.query_cursor = (state.query_cursor + 1).min(state.query.chars().count())
        }
        (KeyCode::Home, _) => state.query_cursor = 0,
        (KeyCode::End, _) => state.query_cursor = state.query.chars().count(),
        (KeyCode::Char(ch), KeyModifiers::NONE) | (KeyCode::Char(ch), KeyModifiers::SHIFT) => {
            let byte_index = char_to_byte_index(&state.query, state.query_cursor);
            state.query.insert(byte_index, ch);
            state.query_cursor += 1;
            state.mark_query_changed();
        }
        (KeyCode::Up, _) => state.move_selection(-1),
        (KeyCode::Down, _) => state.move_selection(1),
        (KeyCode::PageUp, _) => state.move_selection(-(state.viewport_rows.max(1) as isize)),
        (KeyCode::PageDown, _) => state.move_selection(state.viewport_rows.max(1) as isize),
        _ => {}
    }
    state.dirty = true;
    KeyAction::Continue
}

pub(super) fn char_to_byte_index(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

pub(super) fn insert_paste(state: &mut TuiState, pasted: &str) {
    if pasted.is_empty()
        || state.help.is_some()
        || state.options_overlay.is_some()
        || state.sort_picker.is_some()
        || state.root_picker.is_some()
        || state.filelist_confirmation.is_some()
        || state.active_filelist.is_some()
    {
        return;
    }
    if let Some(history) = state.history.as_mut() {
        let byte_index = char_to_byte_index(&history.filter, history.filter_cursor);
        history.filter.insert_str(byte_index, pasted);
        history.filter_cursor += pasted.chars().count();
        refresh_history_results(history, &state.history_entries);
        state.dirty = true;
        return;
    }
    let byte_index = char_to_byte_index(&state.query, state.query_cursor);
    state.query.insert_str(byte_index, pasted);
    state.query_cursor += pasted.chars().count();
    state.mark_query_changed();
    state.dirty = true;
}

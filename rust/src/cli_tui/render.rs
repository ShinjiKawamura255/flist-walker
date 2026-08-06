use super::state::{
    FileListConfirmation, HelpContext, HistoryOverlay, OptionsOverlay, RootPicker, SortPicker,
    TuiState, SORT_MODES,
};
use super::{preview_visible_for_size, tui_path_label, CliTuiOptions};
use crate::query::{CompiledQuery, QueryOptions};
use crate::ui_model::display_path_with_mode;
use anyhow::Result;
use crossterm::cursor::MoveTo;
use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor};
use crossterm::terminal::{self, BeginSynchronizedUpdate, Clear, ClearType, EndSynchronizedUpdate};
use crossterm::{execute, queue};
use std::collections::HashSet;
use std::io::{self, Write};
use std::path::PathBuf;
use unicode_width::UnicodeWidthChar;

pub(super) fn draw<W: Write>(
    terminal_output: &mut W,
    state: &mut TuiState,
    options: &CliTuiOptions,
) -> Result<()> {
    let mut frame = Vec::new();
    render_frame(&mut frame, state, options)?;
    write_synchronized_frame(terminal_output, &frame)?;
    Ok(())
}

pub(super) fn write_synchronized_frame<W: Write>(
    terminal_output: &mut W,
    frame: &[u8],
) -> io::Result<()> {
    queue!(terminal_output, BeginSynchronizedUpdate)?;
    let frame_result = terminal_output.write_all(frame);
    let end_result = queue!(terminal_output, EndSynchronizedUpdate);
    let flush_result = terminal_output.flush();
    frame_result?;
    end_result?;
    flush_result
}

pub(super) fn render_frame<W: Write>(
    terminal_output: &mut W,
    state: &mut TuiState,
    options: &CliTuiOptions,
) -> Result<()> {
    let (width, height) = terminal::size()?;
    let preview_visible = preview_visible_for_size(state.preview_preferred, width, height);
    state.preview_visible = preview_visible;
    if !preview_visible {
        state.clear_preview();
    }
    let list_width = if preview_visible {
        width.saturating_mul(3).saturating_div(5).max(1)
    } else {
        width
    };
    let visible = if state.history.is_some() {
        height.saturating_sub(3) as usize
    } else {
        height.saturating_sub(4) as usize
    };
    state.viewport_rows = visible.max(1);
    state.ensure_selection_visible();
    let start = state.offset.min(state.results.len());
    execute!(
        terminal_output,
        Clear(ClearType::All),
        MoveTo(0, 0),
        SetAttribute(Attribute::Bold),
        Print(clip_to_width("FlistWalker CLI", list_width as usize)),
        SetAttribute(Attribute::Reset),
    )?;
    if height > 1 {
        execute!(
            terminal_output,
            MoveTo(0, 1),
            Print(query_line_for_width(state, list_width as usize))
        )?;
    }
    if height > 2 {
        let status = clip_to_width(&state.status_line(), list_width as usize);
        if options.color_enabled {
            execute!(
                terminal_output,
                MoveTo(0, 2),
                SetForegroundColor(Color::DarkGrey),
                Print(status),
                ResetColor
            )?;
        } else {
            execute!(terminal_output, MoveTo(0, 2), Print(status))?;
        }
    }
    if height > 3 {
        let help = clip_to_width(
            &format!(
                "Enter select | F2 options | F3 {} | Alt+P preview | Esc cancel",
                state.sort_mode.label()
            ),
            list_width as usize,
        );
        if options.color_enabled {
            execute!(
                terminal_output,
                MoveTo(0, 3),
                SetForegroundColor(Color::DarkGrey),
                Print(help),
                ResetColor
            )?;
        } else {
            execute!(terminal_output, MoveTo(0, 3), Print(help))?;
        }
    }
    let compiled = (!state.query.trim().is_empty()).then(|| {
        CompiledQuery::compile(
            &state.query,
            QueryOptions {
                use_regex: state.runtime_options.regex,
                ignore_case: state.runtime_options.ignore_case,
            },
        )
    });
    for (row, (path, _score)) in state.results.iter().skip(start).take(visible).enumerate() {
        let is_selected = start + row == state.selected;
        let is_pinned = state.pinned.contains(path);
        let marker = match (is_selected, is_pinned) {
            (true, true) => "*>",
            (true, false) => "> ",
            (false, true) => "* ",
            (false, false) => "  ",
        };
        let display = display_path_with_mode(path, &state.root, true);
        let positions = compiled
            .as_ref()
            .and_then(|query| query.as_ref().ok())
            .map(|query| {
                crate::ui_model::match_positions_for_path_with_compiled(
                    path,
                    &state.root,
                    query,
                    true,
                )
            })
            .unwrap_or_default();
        print_highlighted(
            terminal_output,
            (row + 4) as u16,
            marker,
            &display,
            &positions,
            list_width,
            options.color_enabled,
        )?;
    }
    if preview_visible {
        render_preview_pane(terminal_output, state, list_width, width, height)?;
    }
    if let Some(context) = state.help {
        render_help_overlay(
            terminal_output,
            context,
            state.emacs_keybindings_enabled,
            width,
            height,
        )?;
    } else if let Some(options_overlay) = state.options_overlay.as_ref() {
        render_options_overlay(
            terminal_output,
            options_overlay,
            state.emacs_keybindings_enabled,
            width,
            height,
        )?;
    } else if let Some(sort_picker) = state.sort_picker.as_ref() {
        render_sort_picker(
            terminal_output,
            sort_picker,
            state.emacs_keybindings_enabled,
            width,
            height,
        )?;
    } else if let Some(root_picker) = state.root_picker.as_ref() {
        render_root_picker(
            terminal_output,
            root_picker,
            &state.saved_roots,
            state.emacs_keybindings_enabled,
            width,
            height,
        )?;
    } else if let Some(confirmation) = state.filelist_confirmation.as_ref() {
        render_filelist_confirmation(
            terminal_output,
            confirmation,
            state.emacs_keybindings_enabled,
            width,
            height,
        )?;
    } else if let Some(history) = state.history.as_ref() {
        render_history_overlay(
            terminal_output,
            history,
            state.emacs_keybindings_enabled,
            width,
            height,
            options.color_enabled,
        )?;
    }
    Ok(())
}

pub(super) fn render_filelist_confirmation<W: Write>(
    terminal_output: &mut W,
    confirmation: &FileListConfirmation,
    emacs_keybindings_enabled: bool,
    width: u16,
    height: u16,
) -> Result<()> {
    execute!(terminal_output, Clear(ClearType::All))?;
    let cancel_keys = overlay_cancel_keys(emacs_keybindings_enabled);
    let lines = match confirmation {
        FileListConfirmation::Mode {
            propagate_to_ancestors,
        } => vec![
            "Create FileList".to_string(),
            format!(
                "Up/Down/Space choose scope | Enter continue | {cancel_keys} cancel | Ctrl+C exit"
            ),
            format!(
                "> Scope: {}",
                if *propagate_to_ancestors {
                    "root and ancestors"
                } else {
                    "root only"
                }
            ),
            "No files are written until this confirmation is accepted.".to_string(),
        ],
        FileListConfirmation::Overwrite {
            propagate_to_ancestors,
        } => vec![
            "Overwrite existing root FileList?".to_string(),
            format!("Enter overwrite | {cancel_keys} cancel | Ctrl+C exit"),
            format!(
                "Scope: {}",
                if *propagate_to_ancestors {
                    "root and ancestors"
                } else {
                    "root only"
                }
            ),
            "This is the final write confirmation.".to_string(),
        ],
    };
    for (row, line) in lines.into_iter().take(height as usize).enumerate() {
        execute!(
            terminal_output,
            MoveTo(0, row as u16),
            Print(clip_to_width(&line, width as usize)),
        )?;
    }
    Ok(())
}

pub(super) fn render_root_picker<W: Write>(
    terminal_output: &mut W,
    picker: &RootPicker,
    roots: &[PathBuf],
    emacs_keybindings_enabled: bool,
    width: u16,
    height: u16,
) -> Result<()> {
    execute!(terminal_output, Clear(ClearType::All))?;
    let cancel_keys = overlay_cancel_keys(emacs_keybindings_enabled);
    let lines = [
        "Saved roots".to_string(),
        format!("Enter switch | {cancel_keys} cancel | Ctrl+C exit | arrows/Page move"),
    ];
    for (row, line) in lines.iter().enumerate().take(height as usize) {
        execute!(
            terminal_output,
            MoveTo(0, row as u16),
            Print(clip_to_width(line, width as usize)),
        )?;
    }
    if roots.is_empty() {
        if height > 2 {
            execute!(
                terminal_output,
                MoveTo(0, 2),
                Print(clip_to_width(
                    "No saved roots are available.",
                    width as usize
                )),
            )?;
        }
        return Ok(());
    }
    let visible = height.saturating_sub(2) as usize;
    let start = overlay_window_start(picker.selected, roots.len(), visible);
    for (row, (index, root)) in roots
        .iter()
        .enumerate()
        .skip(start)
        .take(visible)
        .enumerate()
    {
        let marker = if index == picker.selected { "> " } else { "  " };
        execute!(
            terminal_output,
            MoveTo(0, (row + 2) as u16),
            Print(clip_to_width(
                &format!("{marker}{}", tui_path_label(root)),
                width as usize
            )),
        )?;
    }
    Ok(())
}

pub(super) fn render_options_overlay<W: Write>(
    terminal_output: &mut W,
    overlay: &OptionsOverlay,
    emacs_keybindings_enabled: bool,
    width: u16,
    height: u16,
) -> Result<()> {
    let on_off = |value| if value { "on" } else { "off" };
    let cancel_keys = overlay_cancel_keys(emacs_keybindings_enabled);
    let lines = [
        "Options".to_string(),
        format!("Enter apply | {cancel_keys} cancel | Ctrl+C exit | arrows + Space change"),
        format!("Files: {}", on_off(overlay.draft.include_files)),
        format!("Folders: {}", on_off(overlay.draft.include_dirs)),
        format!("Regex: {}", on_off(overlay.draft.regex)),
        format!("Ignore Case: {}", on_off(overlay.draft.ignore_case)),
        format!("Ignore: {}", on_off(overlay.draft.ignore_enabled)),
        format!("Source: {}", overlay.draft.source.label()),
    ];
    execute!(terminal_output, Clear(ClearType::All))?;
    for (row, line) in lines.iter().take(2).enumerate().take(height as usize) {
        execute!(
            terminal_output,
            MoveTo(0, row as u16),
            Print(clip_to_width(line, width as usize)),
        )?;
    }
    let option_rows = &lines[2..];
    let visible = height.saturating_sub(2) as usize;
    let start = overlay_window_start(overlay.selected, option_rows.len(), visible);
    for (row, (index, line)) in option_rows
        .iter()
        .enumerate()
        .skip(start)
        .take(visible)
        .enumerate()
    {
        let marker = if index == overlay.selected {
            "> "
        } else {
            "  "
        };
        execute!(
            terminal_output,
            MoveTo(0, (row + 2) as u16),
            Print(clip_to_width(&format!("{marker}{line}"), width as usize)),
        )?;
    }
    Ok(())
}

pub(super) fn render_sort_picker<W: Write>(
    terminal_output: &mut W,
    picker: &SortPicker,
    emacs_keybindings_enabled: bool,
    width: u16,
    height: u16,
) -> Result<()> {
    execute!(terminal_output, Clear(ClearType::All))?;
    let cancel_keys = overlay_cancel_keys(emacs_keybindings_enabled);
    let heading = [
        "Sort results".to_string(),
        format!("Enter apply | {cancel_keys} cancel | Ctrl+C exit | arrows move"),
    ];
    for (row, line) in heading.into_iter().enumerate() {
        if row < height as usize {
            execute!(
                terminal_output,
                MoveTo(0, row as u16),
                Print(clip_to_width(&line, width as usize)),
            )?;
        }
    }
    let visible = height.saturating_sub(2) as usize;
    let start = overlay_window_start(picker.selected, SORT_MODES.len(), visible);
    for (row, (index, mode)) in SORT_MODES
        .iter()
        .enumerate()
        .skip(start)
        .take(visible)
        .enumerate()
    {
        let marker = if index == picker.selected { "> " } else { "  " };
        execute!(
            terminal_output,
            MoveTo(0, (row + 2) as u16),
            Print(clip_to_width(
                &format!("{marker}{}", mode.label()),
                width as usize
            )),
        )?;
    }
    Ok(())
}

pub(super) fn overlay_window_start(selected: usize, total: usize, visible: usize) -> usize {
    if visible >= total {
        return 0;
    }
    let visible = visible.max(1);
    let before = visible / 2;
    selected
        .saturating_sub(before)
        .min(total.saturating_sub(visible))
}

pub(super) fn render_history_overlay<W: Write>(
    terminal_output: &mut W,
    history: &HistoryOverlay,
    emacs_keybindings_enabled: bool,
    width: u16,
    height: u16,
    color_enabled: bool,
) -> Result<()> {
    execute!(
        terminal_output,
        Clear(ClearType::All),
        MoveTo(0, 0),
        SetAttribute(Attribute::Bold),
        Print(clip_to_width("History", width as usize)),
        SetAttribute(Attribute::Reset),
    )?;
    if height > 1 {
        execute!(
            terminal_output,
            MoveTo(0, 1),
            Print(clip_to_width(
                &format!("Filter: {}", history.filter),
                width as usize,
            )),
        )?;
    }
    if height > 2 {
        let help = clip_to_width(
            &format!(
                "Enter apply | {} cancel | Ctrl+C exit | arrows/Page move",
                overlay_cancel_keys(emacs_keybindings_enabled)
            ),
            width as usize,
        );
        if color_enabled {
            execute!(
                terminal_output,
                MoveTo(0, 2),
                SetForegroundColor(Color::DarkGrey),
                Print(help),
                ResetColor,
            )?;
        } else {
            execute!(terminal_output, MoveTo(0, 2), Print(help))?;
        }
    }
    let visible = height.saturating_sub(3) as usize;
    for (row, entry) in history
        .results
        .iter()
        .skip(history.offset)
        .take(visible)
        .enumerate()
    {
        let marker = if history.offset + row == history.selected {
            "> "
        } else {
            "  "
        };
        execute!(
            terminal_output,
            MoveTo(0, (row + 3) as u16),
            Print(clip_to_width(marker, width as usize)),
            Print(clip_to_width(entry, width.saturating_sub(2) as usize)),
        )?;
    }
    Ok(())
}

pub(super) fn overlay_cancel_keys(emacs_keybindings_enabled: bool) -> &'static str {
    if emacs_keybindings_enabled {
        "Esc/Ctrl+G"
    } else {
        "Esc"
    }
}

pub(super) fn render_help_overlay<W: Write>(
    terminal_output: &mut W,
    context: HelpContext,
    emacs_keybindings_enabled: bool,
    width: u16,
    height: u16,
) -> Result<()> {
    let close_help = if emacs_keybindings_enabled {
        "Enter / Esc / Ctrl+G close help | Ctrl+C exit"
    } else {
        "Enter / Esc close help | Ctrl+C exit"
    };
    let mut lines = vec!["Help".to_string(), close_help.to_string()];
    match context {
        HelpContext::Normal if emacs_keybindings_enabled => lines.extend([
            "Enter/Ctrl+J/Ctrl+M output | Tab/Shift+Tab/Ctrl+I pin".to_string(),
            "arrows/Ctrl+P/Ctrl+N move | PageUp/Alt+V and PageDown/Ctrl+V".to_string(),
            "Ctrl+O open current | Shift+Enter reveal current".to_string(),
            "Ctrl+G clear query and pins | Ctrl+R search history".to_string(),
            "F2 options | F3 sort | F4 roots | F5 refresh | F6 FileList | Alt+P preview | F1 help".to_string(),
        ]),
        HelpContext::Normal => lines.extend([
            "Enter output selection | Tab/Shift+Tab pin | arrows/Page move".to_string(),
            "Emacs shortcuts disabled by runtime config".to_string(),
            "Ctrl+O open current | Shift+Enter reveal current".to_string(),
            "F2 options | F3 sort | F4 roots | F5 refresh | F6 FileList | Alt+P preview | F1 help".to_string(),
        ]),
        HelpContext::History => lines.extend([
            "History search is paused while help is open.".to_string(),
            if emacs_keybindings_enabled {
                "Close help to use Enter, Esc/Ctrl+G, edit, or navigation."
            } else {
                "Close help to use Enter, Esc, edit, or navigation."
            }
            .to_string(),
        ]),
        HelpContext::FileList => lines.extend([
            "FileList creation is settling; no result is accepted before it finishes.".to_string(),
            if emacs_keybindings_enabled {
                "Enter selects after cancellation, F4 chooses a root, Esc/Ctrl+G/Ctrl+C exits after settlement."
            } else {
                "Enter selects after cancellation, F4 chooses a root, Esc/Ctrl+C exits after settlement."
            }
            .to_string(),
        ]),
    }
    execute!(terminal_output, Clear(ClearType::All))?;
    for (row, line) in lines.into_iter().take(height as usize).enumerate() {
        execute!(
            terminal_output,
            MoveTo(0, row as u16),
            Print(clip_to_width(&line, width as usize)),
        )?;
    }
    Ok(())
}

pub(super) fn render_preview_pane<W: Write>(
    terminal_output: &mut W,
    state: &TuiState,
    list_width: u16,
    terminal_width: u16,
    terminal_height: u16,
) -> Result<()> {
    let x = list_width.saturating_add(1);
    let pane_width = terminal_width.saturating_sub(x);
    if pane_width == 0 {
        return Ok(());
    }
    execute!(
        terminal_output,
        MoveTo(x, 0),
        SetAttribute(Attribute::Bold),
        Print(clip_to_width("Preview", pane_width as usize)),
        SetAttribute(Attribute::Reset),
    )?;
    for (index, line) in state
        .preview
        .lines()
        .take(terminal_height.saturating_sub(1) as usize)
        .enumerate()
    {
        execute!(
            terminal_output,
            MoveTo(x, (index + 1) as u16),
            Print(clip_to_width(line, pane_width as usize)),
        )?;
    }
    Ok(())
}

pub(super) fn print_highlighted<W: Write>(
    terminal_output: &mut W,
    row: u16,
    marker: &str,
    text: &str,
    positions: &HashSet<usize>,
    width: u16,
    color_enabled: bool,
) -> Result<()> {
    execute!(
        terminal_output,
        MoveTo(0, row),
        Print(clip_to_width(marker, width as usize))
    )?;
    let mut highlighted = false;
    let mut chunk = String::new();
    let available = width.saturating_sub(2) as usize;
    let mut used = 0;
    for (index, ch) in text.chars().enumerate() {
        let display_char = terminal_safe_char(ch);
        let char_width = UnicodeWidthChar::width(display_char).unwrap_or(0);
        if used + char_width > available {
            break;
        }
        used += char_width;
        let next = positions.contains(&index);
        if next != highlighted {
            if !chunk.is_empty() {
                execute!(terminal_output, Print(std::mem::take(&mut chunk)))?;
            }
            if color_enabled {
                if next {
                    execute!(terminal_output, SetForegroundColor(Color::Yellow))?;
                } else {
                    execute!(terminal_output, ResetColor)?;
                }
            }
            highlighted = next;
        }
        chunk.push(display_char);
    }
    if !chunk.is_empty() {
        execute!(terminal_output, Print(chunk))?;
    }
    if color_enabled {
        execute!(terminal_output, ResetColor)?;
    }
    Ok(())
}

pub(super) fn clip_to_width(text: &str, width: usize) -> String {
    let mut used = 0;
    text.chars()
        .map(terminal_safe_char)
        .take_while(|ch| {
            let char_width = UnicodeWidthChar::width(*ch).unwrap_or(0);
            if used + char_width > width {
                false
            } else {
                used += char_width;
                true
            }
        })
        .collect()
}

pub(super) fn terminal_safe_char(ch: char) -> char {
    if ch.is_control() {
        '�'
    } else {
        ch
    }
}

pub(super) fn query_line_for_width(state: &TuiState, width: usize) -> String {
    let prefix = clip_to_width("> ", width);
    let prefix_width = prefix
        .chars()
        .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
        .sum::<usize>();
    let available = width.saturating_sub(prefix_width);
    if available == 0 {
        return prefix;
    }

    let chars = state.query.chars().collect::<Vec<_>>();
    let cursor = state.query_cursor.min(chars.len());
    let left_budget = available.saturating_sub(1);
    let mut left = Vec::new();
    let mut left_width = 0;
    for ch in chars[..cursor].iter().rev().copied() {
        let safe = terminal_safe_char(ch);
        let char_width = UnicodeWidthChar::width(safe).unwrap_or(0);
        if left_width + char_width > left_budget {
            break;
        }
        left.push(safe);
        left_width += char_width;
    }
    left.reverse();

    let mut line = prefix;
    line.extend(left);
    line.push('│');
    let mut used = left_width + 1;
    for ch in chars[cursor..].iter().copied() {
        let safe = terminal_safe_char(ch);
        let char_width = UnicodeWidthChar::width(safe).unwrap_or(0);
        if used + char_width > available {
            break;
        }
        line.push(safe);
        used += char_width;
    }
    line
}

use super::protocol::{KeyAction, TuiSource, WORKER_JOIN_TIMEOUT};
use super::retirement::RetirementSender;
use super::state::TuiState;
use crate::search::SearchSortMode;
use crate::search_catalog::{
    load_search_catalog_from_path, search_catalog_file_path, update_search_catalog,
    PresetEntryType, PresetSortMode, PresetSource, SearchCatalog, SearchPreset,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

#[derive(Clone)]
pub(super) enum PresetOperation {
    Load,
    Create(SearchPreset),
    Delete(String),
}
pub(super) struct PresetRequest {
    pub(super) request_id: u64,
    pub(super) operation: PresetOperation,
    pub(super) cancel: Arc<AtomicBool>,
}
pub(super) struct PresetResponse {
    pub(super) request_id: u64,
    pub(super) result: Result<Arc<SearchCatalog>, String>,
}
pub(super) struct PresetPending {
    pub(super) request_id: u64,
    pub(super) cancel: Arc<AtomicBool>,
}
#[derive(Clone, Debug)]
pub(super) enum PresetPhase {
    List,
    Name,
    Delete(String),
}
pub(super) struct PresetModal {
    pub(super) phase: PresetPhase,
    pub(super) catalog: Option<Arc<SearchCatalog>>,
    pub(super) selected: usize,
    pub(super) name: String,
    pub(super) cursor: usize,
    pub(super) error: Option<String>,
    pub(super) snapshot: SearchPreset,
}

pub(super) struct PresetWorker {
    tx: Option<mpsc::SyncSender<PresetRequest>>,
    rx: mpsc::Receiver<PresetResponse>,
    shutdown: Arc<AtomicBool>,
    done: mpsc::Receiver<()>,
    handle: Option<thread::JoinHandle<()>>,
}
impl PresetWorker {
    pub(super) fn start(retirement: RetirementSender) -> std::io::Result<Self> {
        let (tx, requests) = mpsc::sync_channel::<PresetRequest>(1);
        let (responses, rx) = mpsc::sync_channel(1);
        let (done_tx, done) = mpsc::channel();
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_shutdown = Arc::clone(&shutdown);
        let handle = thread::Builder::new()
            .name("flistwalker-cli-presets".into())
            .spawn(move || {
                while !worker_shutdown.load(Ordering::Acquire) {
                    let request = match requests.recv_timeout(Duration::from_millis(25)) {
                        Ok(request) => request,
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    };
                    let canceled = || {
                        worker_shutdown.load(Ordering::Acquire)
                            || request.cancel.load(Ordering::Acquire)
                    };
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let path = search_catalog_file_path().map_err(|error| error.to_string())?;
                        execute_preset_operation(&path, &request.operation, &canceled)
                    }))
                    .unwrap_or_else(|_| Err("Preset worker failed".into()))
                    .map(Arc::new);
                    // Catalog snapshots use the same producer-owned retirement guard.
                    let result = match result {
                        Ok(catalog)
                            if retirement
                                .retain(&catalog, || worker_shutdown.load(Ordering::Acquire)) =>
                        {
                            Ok(catalog)
                        }
                        Ok(_) => break,
                        Err(error) => Err(error),
                    };
                    let mut response = PresetResponse {
                        request_id: request.request_id,
                        result,
                    };
                    loop {
                        if worker_shutdown.load(Ordering::Acquire) {
                            break;
                        }
                        match responses.try_send(response) {
                            Ok(()) => break,
                            Err(mpsc::TrySendError::Full(returned)) => {
                                response = returned;
                                thread::sleep(Duration::from_millis(10));
                            }
                            Err(mpsc::TrySendError::Disconnected(_)) => return,
                        }
                    }
                }
                let _ = done_tx.send(());
            })?;
        Ok(Self {
            tx: Some(tx),
            rx,
            shutdown,
            done,
            handle: Some(handle),
        })
    }
    pub(super) fn send(&self, request: PresetRequest) -> Result<(), String> {
        self.tx
            .as_ref()
            .ok_or("Preset worker unavailable")?
            .try_send(request)
            .map_err(|_| "Preset worker busy or unavailable".into())
    }
    pub(super) fn poll(&self) -> Result<PresetResponse, mpsc::TryRecvError> {
        self.rx.try_recv()
    }
}
impl Drop for PresetWorker {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        self.tx.take();
        if self.done.recv_timeout(WORKER_JOIN_TIMEOUT).is_ok() {
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }
}

fn execute_preset_operation(
    path: &Path,
    operation: &PresetOperation,
    canceled: &impl Fn() -> bool,
) -> Result<SearchCatalog, String> {
    if canceled() {
        return Err("Preset operation canceled".into());
    }
    let result = match operation {
        PresetOperation::Load => load_search_catalog_from_path(path),
        PresetOperation::Create(snapshot) => update_search_catalog(path, |catalog| {
            if canceled() {
                anyhow::bail!("Preset operation canceled");
            }
            catalog.add_preset(snapshot.clone())
        }),
        PresetOperation::Delete(name) => update_search_catalog(path, |catalog| {
            if canceled() {
                anyhow::bail!("Preset operation canceled");
            }
            catalog.remove_preset(name)
        }),
    };
    // Once the locked mutation begins, its atomic commit settles normally. A later
    // cancel closes the UI; it does not claim to undo a completed catalog write.
    result.map_err(|error| crate::path_utils::normalize_text_for_display(&error.to_string()))
}

impl TuiState {
    pub(super) fn open_presets(&mut self) {
        if self.pending_preset.is_some() {
            return;
        }
        self.preset_modal = Some(PresetModal {
            phase: PresetPhase::List,
            catalog: None,
            selected: 0,
            name: String::new(),
            cursor: 0,
            error: None,
            snapshot: self.preset_snapshot(),
        });
        self.dirty = true;
    }
    fn preset_snapshot(&self) -> SearchPreset {
        SearchPreset {
            name: String::new(),
            root_name: None,
            root_path: self.root.clone(),
            query: self.query.clone(),
            entry_type: match (
                self.runtime_options.include_files,
                self.runtime_options.include_dirs,
            ) {
                (true, false) => PresetEntryType::File,
                (false, true) => PresetEntryType::Folder,
                _ => PresetEntryType::All,
            },
            source: match self.runtime_options.source {
                TuiSource::Auto => PresetSource::Auto,
                TuiSource::FileList => PresetSource::Filelist,
                TuiSource::Walker => PresetSource::Walker,
            },
            regex: self.runtime_options.regex,
            ignore_case: self.runtime_options.ignore_case,
            ignore_enabled: self.runtime_options.ignore_enabled,
            sort: match self.sort_mode {
                SearchSortMode::Score => PresetSortMode::Score,
                SearchSortMode::NameAsc => PresetSortMode::NameAsc,
                SearchSortMode::NameDesc => PresetSortMode::NameDesc,
                SearchSortMode::ModifiedAsc => PresetSortMode::ModifiedAsc,
                SearchSortMode::ModifiedDesc => PresetSortMode::ModifiedDesc,
                SearchSortMode::CreatedAsc => PresetSortMode::CreatedAsc,
                SearchSortMode::CreatedDesc => PresetSortMode::CreatedDesc,
                SearchSortMode::SizeAsc => PresetSortMode::SizeAsc,
                SearchSortMode::SizeDesc => PresetSortMode::SizeDesc,
            },
            max_depth: self.max_depth,
            extra: BTreeMap::new(),
        }
    }
    pub(super) fn next_preset_request(
        &mut self,
        operation: PresetOperation,
    ) -> Option<PresetRequest> {
        if self.pending_preset.is_some() {
            return None;
        }
        self.next_preset_request_id = self.next_preset_request_id.wrapping_add(1);
        let cancel = Arc::new(AtomicBool::new(false));
        self.pending_preset = Some(PresetPending {
            request_id: self.next_preset_request_id,
            cancel: Arc::clone(&cancel),
        });
        if let Some(modal) = &mut self.preset_modal {
            modal.error = None;
        }
        self.dirty = true;
        Some(PresetRequest {
            request_id: self.next_preset_request_id,
            operation,
            cancel,
        })
    }
    pub(super) fn cancel_preset_request(&mut self) {
        if let Some(pending) = &self.pending_preset {
            pending.cancel.store(true, Ordering::Release);
        }
    }
}

pub(super) fn dispatch_preset(
    state: &mut TuiState,
    worker: &PresetWorker,
    operation: PresetOperation,
) {
    if let Some(request) = state.next_preset_request(operation) {
        let id = request.request_id;
        if let Err(error) = worker.send(request) {
            apply_preset_response(
                state,
                PresetResponse {
                    request_id: id,
                    result: Err(error),
                },
            );
        }
    }
}

pub(super) fn apply_preset_response(state: &mut TuiState, response: PresetResponse) {
    if state
        .pending_preset
        .as_ref()
        .is_none_or(|pending| pending.request_id != response.request_id)
    {
        return;
    }
    state.pending_preset = None;
    if let Some(modal) = &mut state.preset_modal {
        match response.result {
            Ok(catalog) => {
                modal.selected = modal.selected.min(catalog.presets.len().saturating_sub(1));
                modal.catalog = Some(catalog);
                modal.phase = PresetPhase::List;
                modal.name.clear();
                modal.cursor = 0;
                modal.error = None;
            }
            Err(error) => modal.error = Some(error),
        }
    } else {
        state.status = match response.result {
            Ok(_) => "Preset operation completed".into(),
            Err(error) => error,
        };
    }
    state.dirty = true;
}

pub(super) fn handle_preset_key(state: &mut TuiState, key: KeyEvent) -> KeyAction {
    if key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL {
        return KeyAction::Cancel;
    }
    if key.code == KeyCode::Esc
        || (key.code == KeyCode::Char('g') && key.modifiers == KeyModifiers::CONTROL)
    {
        state.cancel_preset_request();
        state.preset_modal = None;
        state.dirty = true;
        return KeyAction::Continue;
    }
    if state.pending_preset.is_some() {
        return KeyAction::Continue;
    }
    let Some(modal) = &mut state.preset_modal else {
        return KeyAction::Continue;
    };
    match &modal.phase {
        PresetPhase::List => match key.code {
            KeyCode::Char('n') | KeyCode::Char('N')
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                modal.phase = PresetPhase::Name;
                modal.error = None;
            }
            KeyCode::Up => modal.selected = modal.selected.saturating_sub(1),
            KeyCode::Down => {
                modal.selected = (modal.selected + 1).min(
                    modal
                        .catalog
                        .as_ref()
                        .map_or(0, |c| c.presets.len().saturating_sub(1)),
                )
            }
            KeyCode::Delete => {
                if let Some(preset) = modal
                    .catalog
                    .as_ref()
                    .and_then(|c| c.presets.get(modal.selected))
                {
                    modal.phase = PresetPhase::Delete(preset.name.clone());
                    modal.error = None;
                }
            }
            KeyCode::Char('r') if key.modifiers.is_empty() => {
                return KeyAction::PresetOperation(PresetOperation::Load)
            }
            _ => {}
        },
        PresetPhase::Name => match key.code {
            KeyCode::Enter => {
                if !modal.snapshot.root_path.is_absolute()
                    || (!state.runtime_options.include_files && !state.runtime_options.include_dirs)
                {
                    modal.error = Some(
                        "Choose an absolute root and at least one entry type before saving".into(),
                    );
                } else {
                    let mut snapshot = modal.snapshot.clone();
                    snapshot.name = modal.name.clone();
                    return KeyAction::PresetOperation(PresetOperation::Create(snapshot));
                }
            }
            KeyCode::Left => modal.cursor = modal.cursor.saturating_sub(1),
            KeyCode::Right => modal.cursor = (modal.cursor + 1).min(modal.name.chars().count()),
            KeyCode::Home => modal.cursor = 0,
            KeyCode::End => modal.cursor = modal.name.chars().count(),
            KeyCode::Backspace if modal.cursor > 0 => {
                let start = crate::text_editing::char_to_byte_index(&modal.name, modal.cursor - 1);
                let end = crate::text_editing::char_to_byte_index(&modal.name, modal.cursor);
                modal.name.replace_range(start..end, "");
                modal.cursor -= 1;
            }
            KeyCode::Delete if modal.cursor < modal.name.chars().count() => {
                let start = crate::text_editing::char_to_byte_index(&modal.name, modal.cursor);
                let end = crate::text_editing::char_to_byte_index(&modal.name, modal.cursor + 1);
                modal.name.replace_range(start..end, "");
            }
            KeyCode::Char(ch)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                let byte = crate::text_editing::char_to_byte_index(&modal.name, modal.cursor);
                modal.name.insert(byte, ch);
                modal.cursor += 1;
            }
            _ => {}
        },
        PresetPhase::Delete(name) => {
            if key.code == KeyCode::Enter {
                return KeyAction::PresetOperation(PresetOperation::Delete(name.clone()));
            }
        }
    }
    state.dirty = true;
    KeyAction::Continue
}

pub(super) fn render_presets<W: std::io::Write>(
    output: &mut W,
    modal: &PresetModal,
    busy: bool,
    emacs: bool,
    width: u16,
    height: u16,
) -> std::io::Result<()> {
    use super::render::{clip_to_width, overlay_window_start};
    use crossterm::{
        cursor::MoveTo,
        execute,
        style::Print,
        terminal::{Clear, ClearType},
    };
    let mut lines = vec!["Presets — save current search / delete".to_string()];
    let enter = if emacs { "Enter/Ctrl+J/M" } else { "Enter" };
    let escape = if emacs { "Esc/Ctrl+G" } else { "Esc" };
    lines.push(if busy {
        "Working… Esc close | Ctrl+C exit after settlement".into()
    } else {
        match &modal.phase {
            PresetPhase::List => format!("N save | Delete remove | R retry | {escape} close"),
            PresetPhase::Name => format!("{enter} save | {escape} discard | Ctrl+C exit"),
            PresetPhase::Delete(_) => {
                format!("{enter} confirm delete | {escape} cancel | Ctrl+C exit")
            }
        }
    });
    if emacs && matches!(modal.phase, PresetPhase::List) {
        lines.push("Up/Down or Ctrl+P/N move | Ctrl+C exit".into());
    }
    if let Some(error) = &modal.error {
        lines.push(format!("Error: {error}"));
    }
    match &modal.phase {
        PresetPhase::List => {
            if let Some(catalog) = &modal.catalog {
                if catalog.presets.is_empty() {
                    lines.push("No presets — press N to save the current search".into());
                }
                let visible = (height as usize).saturating_sub(lines.len());
                let start = overlay_window_start(modal.selected, catalog.presets.len(), visible);
                lines.extend(
                    catalog
                        .presets
                        .iter()
                        .enumerate()
                        .skip(start)
                        .take(visible)
                        .map(|(index, preset)| {
                            format!(
                                "{}{}",
                                if index == modal.selected { "> " } else { "  " },
                                preset.name
                            )
                        }),
                );
            } else if !busy && modal.error.is_none() {
                lines.push("No catalog loaded — press R to retry".into());
            }
        }
        PresetPhase::Name => {
            let byte = crate::text_editing::char_to_byte_index(&modal.name, modal.cursor);
            lines.push(format!(
                "Name: {}▏{}",
                &modal.name[..byte],
                &modal.name[byte..]
            ));
            lines.push(format!(
                "Root: {}",
                super::tui_path_label(&modal.snapshot.root_path)
            ));
            lines.push(format!("Query: {}", modal.snapshot.query));
        }
        PresetPhase::Delete(name) => lines.push(format!("Delete preset '{name}'?")),
    }
    execute!(output, Clear(ClearType::All))?;
    for (row, line) in lines.iter().take(height as usize).enumerate() {
        execute!(
            output,
            MoveTo(0, row as u16),
            Print(clip_to_width(line, width as usize))
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    #[test]
    fn alignment_preset_modal_consumes_actions_and_keeps_failed_draft() {
        let mut state = TuiState::new("main");
        state.open_presets();
        for key in [KeyCode::F(4), KeyCode::F(5), KeyCode::Enter] {
            assert!(matches!(
                handle_preset_key(&mut state, KeyEvent::new(key, KeyModifiers::NONE)),
                KeyAction::Continue
            ));
        }
        handle_preset_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE),
        );
        handle_preset_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
        );
        let request = state
            .next_preset_request(PresetOperation::Create(
                state.preset_modal.as_ref().unwrap().snapshot.clone(),
            ))
            .unwrap();
        assert!(state.next_preset_request(PresetOperation::Load).is_none());
        apply_preset_response(
            &mut state,
            PresetResponse {
                request_id: request.request_id,
                result: Err("disk full".into()),
            },
        );
        assert_eq!(state.preset_modal.as_ref().unwrap().name, "x");
        assert_eq!(state.query, "main");
    }

    #[test]
    fn alignment_preset_modal_esc_and_delayed_response_never_revert_search_state() {
        let mut state = TuiState::new("saved-query");
        state.root = std::path::PathBuf::from("/tmp/saved-root");
        state.open_presets();
        let snapshot = state.preset_modal.as_ref().unwrap().snapshot.clone();
        let request = state
            .next_preset_request(PresetOperation::Create(snapshot.clone()))
            .unwrap();
        assert!(matches!(
            handle_preset_key(&mut state, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            KeyAction::Continue
        ));
        assert!(request.cancel.load(Ordering::Acquire));
        assert!(state.preset_modal.is_none());
        state.root = std::path::PathBuf::from("/tmp/new-root");
        state.query = "new-query".into();
        let mut catalog = SearchCatalog::default();
        catalog.presets.push(snapshot);
        apply_preset_response(
            &mut state,
            PresetResponse {
                request_id: request.request_id + 1,
                result: Ok(Arc::new(catalog.clone())),
            },
        );
        assert!(state.pending_preset.is_some());
        apply_preset_response(
            &mut state,
            PresetResponse {
                request_id: request.request_id,
                result: Ok(Arc::new(catalog)),
            },
        );
        assert!(state.pending_preset.is_none());
        assert_eq!(state.query, "new-query");
        assert_eq!(state.root, std::path::PathBuf::from("/tmp/new-root"));
    }

    #[test]
    fn alignment_preset_exit_settlement_blocks_all_further_actions() {
        let mut state = TuiState::new("draft");
        state.preset_exit_pending = Some(super::super::protocol::TuiExit::Cancelled);
        for code in [
            KeyCode::F(4),
            KeyCode::F(5),
            KeyCode::F(6),
            KeyCode::F(7),
            KeyCode::Enter,
            KeyCode::Char('x'),
        ] {
            assert!(matches!(
                super::super::input::handle_key(
                    &mut state,
                    KeyEvent::new(code, KeyModifiers::NONE)
                ),
                KeyAction::Continue
            ));
        }
        assert_eq!(state.query, "draft");
        assert!(matches!(
            super::super::input::handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
            ),
            KeyAction::Cancel
        ));
    }

    #[test]
    fn alignment_preset_atomic_create_delete_preserve_unknown_fields_and_cancel() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("flist-tui-preset-{nonce}"));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("catalog.json");
        let mut state = TuiState::new("captured-query");
        state.root = dir.clone();
        let mut existing = state.preset_snapshot();
        existing.name = "existing".into();
        existing
            .extra
            .insert("future".into(), serde_json::json!({"kept":true}));
        let mut catalog = SearchCatalog::default();
        catalog
            .extra
            .insert("future-catalog".into(), serde_json::json!(17));
        catalog.add_preset(existing).unwrap();
        std::fs::write(&path, serde_json::to_vec(&catalog).unwrap()).unwrap();
        let mut snapshot = state.preset_snapshot();
        snapshot.name = "created".into();
        let created =
            execute_preset_operation(&path, &PresetOperation::Create(snapshot.clone()), &|| false)
                .unwrap();
        assert_eq!(
            created.preset("existing").unwrap().extra["future"]["kept"],
            true
        );
        assert_eq!(created.extra["future-catalog"], 17);
        let before = std::fs::read(&path).unwrap();
        assert!(
            execute_preset_operation(&path, &PresetOperation::Create(snapshot), &|| false).is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(execute_preset_operation(
            &path,
            &PresetOperation::Delete("created".into()),
            &|| true
        )
        .is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let calls = std::cell::Cell::new(0usize);
        assert!(execute_preset_operation(
            &path,
            &PresetOperation::Delete("created".into()),
            &|| {
                let count = calls.get();
                calls.set(count + 1);
                count > 0
            }
        )
        .is_err());
        assert_eq!(
            calls.get(),
            2,
            "cancellation is rechecked inside the acquired catalog lock"
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let deleted =
            execute_preset_operation(&path, &PresetOperation::Delete("created".into()), &|| false)
                .unwrap();
        assert!(deleted.preset("created").is_none());
        assert_eq!(
            deleted.preset("existing").unwrap().extra["future"]["kept"],
            true
        );
        assert_eq!(deleted.extra["future-catalog"], 17);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn alignment_preset_delete_requires_second_enter_and_error_keeps_confirmation() {
        let mut state = TuiState::new("query");
        state.open_presets();
        let mut preset = state.preset_snapshot();
        preset.name = "remove-me".into();
        let mut catalog = SearchCatalog::default();
        catalog.add_preset(preset).unwrap();
        state.preset_modal.as_mut().unwrap().catalog = Some(Arc::new(catalog));
        assert!(matches!(
            handle_preset_key(
                &mut state,
                KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE)
            ),
            KeyAction::Continue
        ));
        assert!(
            matches!(&state.preset_modal.as_ref().unwrap().phase, PresetPhase::Delete(name) if name == "remove-me")
        );
        let KeyAction::PresetOperation(operation) = handle_preset_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        ) else {
            panic!("missing delete request")
        };
        let request = state.next_preset_request(operation).unwrap();
        assert!(matches!(
            handle_preset_key(
                &mut state,
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
            ),
            KeyAction::Continue
        ));
        apply_preset_response(
            &mut state,
            PresetResponse {
                request_id: request.request_id,
                result: Err("storage unavailable".into()),
            },
        );
        assert!(matches!(
            &state.preset_modal.as_ref().unwrap().phase,
            PresetPhase::Delete(_)
        ));
    }

    #[test]
    fn alignment_preset_worker_drop_uses_a_bounded_join_budget() {
        let (tx, _requests) = mpsc::sync_channel(1);
        let (_responses, rx) = mpsc::channel();
        let (done_tx, done) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let (finished_tx, finished) = mpsc::channel();
        let handle = thread::spawn(move || {
            released.recv().unwrap();
            let _ = done_tx.send(());
            let _ = finished_tx.send(());
        });
        let worker = PresetWorker {
            tx: Some(tx),
            rx,
            shutdown: Arc::new(AtomicBool::new(false)),
            done,
            handle: Some(handle),
        };
        let started = std::time::Instant::now();
        drop(worker);
        assert!(started.elapsed() < Duration::from_millis(650));
        release.send(()).unwrap();
        finished.recv_timeout(Duration::from_secs(2)).unwrap();
    }

    #[test]
    fn alignment_preset_render_scrolls_selected_entry_and_keeps_name_draft_visible() {
        let mut state = TuiState::new("query");
        state.open_presets();
        let mut catalog = SearchCatalog::default();
        for index in 0..30 {
            let mut preset = state.preset_snapshot();
            preset.name = format!("preset-{index:02}");
            catalog.add_preset(preset).unwrap();
        }
        let modal = state.preset_modal.as_mut().unwrap();
        modal.catalog = Some(Arc::new(catalog));
        modal.selected = 29;
        let mut output = Vec::new();
        render_presets(&mut output, modal, false, false, 40, 6).unwrap();
        let rendered = String::from_utf8(output).unwrap();
        assert!(rendered.contains("> preset-29"));
        assert!(!rendered.contains("preset-00"));
        modal.phase = PresetPhase::Name;
        modal.name = "日本語".into();
        modal.cursor = 3;
        let mut output = Vec::new();
        render_presets(&mut output, modal, false, false, 40, 6).unwrap();
        assert!(String::from_utf8(output).unwrap().contains("Name: 日本語"));
    }
}

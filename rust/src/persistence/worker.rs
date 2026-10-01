//! Owns bounded admission, ordered barriers, retry and persistence status.
mod document;
mod settings;

use super::history_persist_disabled;
use super::paths::ui_state_file_path;
use super::schema::UiState;
#[cfg(test)]
pub(crate) use document::canonicalize_last_root_for_persistence;
#[cfg(test)]
use document::{append_history_delta, build_ui_state_document};
use document::{history_delta_from_snapshot, normalize_history_recency, write_pending_ui_state};
use serde_json::Value;
use settings::commit_settings;
#[cfg(test)]
use settings::commit_settings_with_writer;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

static UI_STATE_PERSISTENCE: OnceLock<Mutex<UiStatePersistenceRegistry>> = OnceLock::new();

// Permits cover queued AND retrying autosaves. Extra channel slots admit ordered
// barriers even when all autosave permits are occupied.
const MAX_PENDING_UI_STATE_WRITES: usize = 64;
const UI_STATE_COMMAND_CAPACITY: usize = MAX_PENDING_UI_STATE_WRITES + 8;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct UiStatePersistenceStatus {
    pub(crate) accepted_generation: u64,
    pub(crate) persisted_generation: u64,
    pub(crate) last_error: Option<String>,
    pub(crate) startup_protected: bool,
}

#[derive(Default)]
struct AdmissionState {
    status: UiStatePersistenceStatus,
    outstanding: usize,
}

#[derive(Clone)]
struct PersistenceSender {
    tx: SyncSender<UiStatePersistenceCommand>,
    state: Arc<Mutex<AdmissionState>>,
}

impl PersistenceSender {
    fn enqueue(&self, patch: UiStatePatch, history_delta: Vec<String>) -> Result<u64, String> {
        let history_delta = normalize_history_recency(history_delta);
        let mut state = self
            .state
            .lock()
            .map_err(|_| "UI-state persistence status is unavailable".to_string())?;
        if state.status.startup_protected {
            return Err(startup_protection_message());
        }
        if state.outstanding >= MAX_PENDING_UI_STATE_WRITES {
            let error = "UI-state persistence queue is full; retry after pending changes are saved"
                .to_string();
            state.status.last_error = Some(error.clone());
            return Err(error);
        }
        let generation = state.status.accepted_generation.saturating_add(1);
        if let Err(error) = self.tx.try_send(UiStatePersistenceCommand::Enqueue {
            generation,
            patch,
            history_delta,
        }) {
            let error = admission_error(error);
            state.status.last_error = Some(error.clone());
            return Err(error);
        }
        state.outstanding += 1;
        state.status.accepted_generation = generation;
        Ok(generation)
    }

    fn send_control(&self, command: UiStatePersistenceCommand) -> Result<(), String> {
        self.tx.try_send(command).map_err(admission_error)
    }
}

fn admission_error(error: TrySendError<UiStatePersistenceCommand>) -> String {
    match error {
        TrySendError::Full(_) => {
            "UI-state persistence queue is full; retry after pending changes are saved".to_string()
        }
        TrySendError::Disconnected(_) => "UI-state persistence worker is unavailable".to_string(),
    }
}

fn startup_protection_message() -> String {
    "UI-state could not be loaded at startup; repair the settings file and restart before saving"
        .to_string()
}

const UI_STATE_PERSISTENCE_RETRY_DELAY: Duration = Duration::from_millis(50);
const UI_STATE_PERSISTENCE_LOCK_TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Clone, Debug)]
pub(crate) struct UiStatePatch(Value);

impl Default for UiStatePatch {
    fn default() -> Self {
        Self(Value::Object(Default::default()))
    }
}

impl UiStatePatch {
    pub(crate) fn from_json(value: Value) -> Self {
        Self(if value.is_object() {
            value
        } else {
            Value::Object(Default::default())
        })
    }

    pub(crate) fn from_ui_state(state: &UiState) -> Self {
        Self::from_json(
            serde_json::to_value(state).unwrap_or_else(|_| Value::Object(Default::default())),
        )
    }

    fn without_history(mut self) -> Self {
        if let Value::Object(map) = &mut self.0 {
            map.remove("query_history");
        }
        self
    }
}

#[derive(Clone)]
struct PendingUiStateWrite {
    generation: u64,
    patch: UiStatePatch,
    history_delta: Vec<String>,
}

enum UiStatePersistenceCommand {
    Enqueue {
        generation: u64,
        patch: UiStatePatch,
        history_delta: Vec<String>,
    },
    CommitSettings {
        request: SettingsCommitRequest,
        response: Sender<SettingsCommitResponse>,
    },
    Flush(Sender<Result<(), String>>),
    Shutdown(Sender<Result<(), String>>),
}

pub(crate) struct SettingsCommitRequest {
    pub(crate) request_id: u64,
    pub(crate) patch: UiStatePatch,
    pub(crate) saved_roots: Option<(PathBuf, String)>,
}

pub(crate) struct SettingsCommitReceipt {
    pub(crate) canonical_default_root: Option<PathBuf>,
}

pub(crate) struct SettingsCommitResponse {
    pub(crate) request_id: u64,
    pub(crate) result: Result<SettingsCommitReceipt, String>,
}

pub struct AsyncHistoryPersistence {
    sender: PersistenceSender,
    history_persist_disabled: bool,
    handle: Mutex<Option<thread::JoinHandle<()>>>,
}

impl AsyncHistoryPersistence {
    pub fn new_default() -> Option<Self> {
        let path = ui_state_file_path()?;
        Some(Self::new(path, history_persist_disabled()))
    }
    pub fn new(path: PathBuf, history_persist_disabled: bool) -> Self {
        Self::new_with_lock_timeout(
            path,
            history_persist_disabled,
            UI_STATE_PERSISTENCE_LOCK_TIMEOUT,
        )
    }
    fn new_with_lock_timeout(
        path: PathBuf,
        history_persist_disabled: bool,
        lock_timeout: Duration,
    ) -> Self {
        let (sender, handle) =
            spawn_ui_state_persistence_worker(path, history_persist_disabled, lock_timeout, false);
        Self {
            sender,
            history_persist_disabled,
            handle: Mutex::new(Some(handle)),
        }
    }
    pub fn enqueue_history(&self, history_delta: Vec<String>) -> Result<(), String> {
        if self.history_persist_disabled {
            return Ok(());
        }
        self.sender
            .enqueue(UiStatePatch::default(), history_delta)
            .map(|_| ())
    }
    pub fn flush(&self, timeout: Duration) -> Result<(), String> {
        flush_sender(&self.sender, timeout)
    }
    pub fn shutdown(self, timeout: Duration) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.sender
            .send_control(UiStatePersistenceCommand::Shutdown(tx))?;
        let result = rx
            .recv_timeout(timeout)
            .map_err(|_| "UI-state persistence shutdown timed out".to_string())?;
        result?;
        let Some(handle) = self.handle.lock().ok().and_then(|mut handle| handle.take()) else {
            return Ok(());
        };
        let (joined_tx, joined_rx) = mpsc::channel();
        thread::spawn(move || {
            let _ =
                joined_tx.send(handle.join().map_err(|_| {
                    "UI-state persistence worker panicked during shutdown".to_string()
                }));
        });
        joined_rx
            .recv_timeout(timeout)
            .map_err(|_| "UI-state persistence worker join timed out".to_string())?
    }
    #[cfg(test)]
    fn enqueue_patch_for_test(&self, patch: UiStatePatch, history_delta: Vec<String>) {
        self.sender
            .enqueue(patch, history_delta)
            .expect("admit test patch");
    }
}

#[derive(Default)]
struct UiStatePersistenceRegistry {
    senders: std::collections::HashMap<PathBuf, PersistenceSender>,
    history_snapshots: std::collections::HashMap<PathBuf, Vec<String>>,
    startup_failures: std::collections::HashSet<PathBuf>,
}
fn ui_state_persistence_registry() -> &'static Mutex<UiStatePersistenceRegistry> {
    UI_STATE_PERSISTENCE.get_or_init(|| Mutex::new(UiStatePersistenceRegistry::default()))
}
pub(super) fn protect_failed_startup_read(path: &Path) {
    if let Ok(mut registry) = ui_state_persistence_registry().lock() {
        registry.startup_failures.insert(path.to_path_buf());
        if let Some(sender) = registry.senders.get(path) {
            if let Ok(mut state) = sender.state.lock() {
                state.status.startup_protected = true;
                state.status.last_error = Some(startup_protection_message());
            }
        }
    }
}
pub(crate) fn ui_state_persistence_status(path: &Path) -> UiStatePersistenceStatus {
    let Ok(registry) = ui_state_persistence_registry().lock() else {
        return UiStatePersistenceStatus {
            last_error: Some("UI-state persistence registry is unavailable".into()),
            ..Default::default()
        };
    };
    if let Some(sender) = registry.senders.get(path) {
        return sender
            .state
            .lock()
            .map(|state| state.status.clone())
            .unwrap_or_else(|_| UiStatePersistenceStatus {
                last_error: Some("UI-state persistence status is unavailable".into()),
                ..Default::default()
            });
    }
    let startup_protected = registry.startup_failures.contains(path);
    UiStatePersistenceStatus {
        startup_protected,
        last_error: startup_protected.then(startup_protection_message),
        ..Default::default()
    }
}
fn spawn_ui_state_persistence_worker(
    path: PathBuf,
    history_persist_disabled: bool,
    lock_timeout: Duration,
    startup_protected: bool,
) -> (PersistenceSender, thread::JoinHandle<()>) {
    let (tx, rx) = mpsc::sync_channel(UI_STATE_COMMAND_CAPACITY);
    let state = Arc::new(Mutex::new(AdmissionState {
        status: UiStatePersistenceStatus {
            startup_protected,
            last_error: startup_protected.then(startup_protection_message),
            ..Default::default()
        },
        outstanding: 0,
    }));
    let worker_state = Arc::clone(&state);
    let handle = thread::spawn(move || {
        run_ui_state_persistence_worker(
            rx,
            path,
            history_persist_disabled,
            lock_timeout,
            worker_state,
        )
    });
    (PersistenceSender { tx, state }, handle)
}
fn registry_sender(
    registry: &mut UiStatePersistenceRegistry,
    path: PathBuf,
    history_persist_disabled: bool,
) -> PersistenceSender {
    let startup_protected = registry.startup_failures.contains(&path);
    registry
        .senders
        .entry(path.clone())
        .or_insert_with(|| {
            spawn_ui_state_persistence_worker(
                path,
                history_persist_disabled,
                UI_STATE_PERSISTENCE_LOCK_TIMEOUT,
                startup_protected,
            )
            .0
        })
        .clone()
}
pub(crate) fn enqueue_ui_state_patch(
    path: PathBuf,
    patch: UiStatePatch,
    history_snapshot: Vec<String>,
    history_persist_disabled: bool,
) -> Result<u64, String> {
    let history_snapshot = normalize_history_recency(history_snapshot);
    let mut registry = ui_state_persistence_registry()
        .lock()
        .map_err(|_| "UI-state persistence registry is unavailable".to_string())?;
    let history_delta = if history_persist_disabled {
        Vec::new()
    } else {
        let previous = registry
            .history_snapshots
            .get(&path)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        history_delta_from_snapshot(previous, &history_snapshot)
    };
    let sender = registry_sender(&mut registry, path.clone(), history_persist_disabled);
    let generation = sender.enqueue(patch.without_history(), history_delta)?;
    // Rejection must not advance this baseline: the next retry still owns its delta.
    if !history_persist_disabled {
        registry.history_snapshots.insert(path, history_snapshot);
    }
    Ok(generation)
}
fn persistence_sender_for_path(
    path: PathBuf,
    history_persist_disabled: bool,
) -> Result<PersistenceSender, String> {
    let mut registry = ui_state_persistence_registry()
        .lock()
        .map_err(|_| "UI-state persistence registry is unavailable".to_string())?;
    Ok(registry_sender(
        &mut registry,
        path,
        history_persist_disabled,
    ))
}
pub(crate) fn enqueue_settings_commit(
    ui_state_path: PathBuf,
    history_persist_disabled: bool,
    request: SettingsCommitRequest,
) -> Result<mpsc::Receiver<SettingsCommitResponse>, String> {
    let sender = persistence_sender_for_path(ui_state_path, history_persist_disabled)?;
    let state = sender
        .state
        .lock()
        .map_err(|_| "UI-state persistence status is unavailable".to_string())?;
    if state.status.startup_protected {
        return Err(startup_protection_message());
    }
    let (response_tx, response_rx) = mpsc::channel();
    sender.send_control(UiStatePersistenceCommand::CommitSettings {
        request,
        response: response_tx,
    })?;
    Ok(response_rx)
}
fn flush_sender(sender: &PersistenceSender, timeout: Duration) -> Result<(), String> {
    let (tx, rx) = mpsc::channel();
    sender.send_control(UiStatePersistenceCommand::Flush(tx))?;
    rx.recv_timeout(timeout)
        .map_err(|_| "UI-state persistence flush timed out".to_string())?
}
pub(crate) fn flush_ui_state_persistence(path: &Path, timeout: Duration) -> Result<(), String> {
    let registry = ui_state_persistence_registry()
        .lock()
        .map_err(|_| "UI-state persistence registry is unavailable".to_string())?;
    if registry.startup_failures.contains(path) {
        return Err(startup_protection_message());
    }
    let sender = registry.senders.get(path).cloned();
    drop(registry);
    match sender {
        Some(sender) => flush_sender(&sender, timeout),
        None => Ok(()),
    }
}
#[cfg(test)]
pub(crate) fn shutdown_ui_state_persistence_for_test(path: &Path, timeout: Duration) {
    let sender = ui_state_persistence_registry()
        .lock()
        .ok()
        .and_then(|mut registry| {
            registry.history_snapshots.remove(path);
            registry.startup_failures.remove(path);
            registry.senders.remove(path)
        });
    if let Some(sender) = sender {
        let (tx, rx) = mpsc::channel();
        if sender
            .send_control(UiStatePersistenceCommand::Shutdown(tx))
            .is_ok()
        {
            let _ = rx.recv_timeout(timeout);
        }
    }
}
fn run_ui_state_persistence_worker(
    rx: mpsc::Receiver<UiStatePersistenceCommand>,
    path: PathBuf,
    history_persist_disabled: bool,
    lock_timeout: Duration,
    state: Arc<Mutex<AdmissionState>>,
) {
    let mut pending = Vec::<PendingUiStateWrite>::new();
    loop {
        let command = rx.recv_timeout(UI_STATE_PERSISTENCE_RETRY_DELAY);
        let mut flush_reply = None;
        let mut shutdown_reply = None;
        let mut settings_commit = None;
        let mut disconnected = false;
        match command {
            Ok(UiStatePersistenceCommand::Enqueue {
                generation,
                patch,
                history_delta,
            }) => pending.push(PendingUiStateWrite {
                generation,
                patch,
                history_delta,
            }),
            Ok(UiStatePersistenceCommand::CommitSettings { request, response }) => {
                settings_commit = Some((request, response))
            }
            Ok(UiStatePersistenceCommand::Flush(reply)) => flush_reply = Some(reply),
            Ok(UiStatePersistenceCommand::Shutdown(reply)) => shutdown_reply = Some(reply),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => disconnected = true,
        }
        if flush_reply.is_none()
            && shutdown_reply.is_none()
            && settings_commit.is_none()
            && !disconnected
        {
            while let Ok(command) = rx.try_recv() {
                match command {
                    UiStatePersistenceCommand::Enqueue {
                        generation,
                        patch,
                        history_delta,
                    } => pending.push(PendingUiStateWrite {
                        generation,
                        patch,
                        history_delta,
                    }),
                    UiStatePersistenceCommand::CommitSettings { request, response } => {
                        settings_commit = Some((request, response));
                        break;
                    }
                    UiStatePersistenceCommand::Flush(reply) => {
                        flush_reply = Some(reply);
                        break;
                    }
                    UiStatePersistenceCommand::Shutdown(reply) => {
                        shutdown_reply = Some(reply);
                        break;
                    }
                }
            }
        }
        debug_assert!(pending.len() <= MAX_PENDING_UI_STATE_WRITES);
        let attempted = settings_commit.is_some() || !pending.is_empty();
        let protected = state
            .lock()
            .map(|state| state.status.startup_protected)
            .unwrap_or(true);
        let result = if let Some((request, response)) = settings_commit {
            let request_id = request.request_id;
            let result = if protected {
                Err(startup_protection_message())
            } else {
                commit_settings(
                    &path,
                    &pending,
                    history_persist_disabled,
                    lock_timeout,
                    request,
                )
                .map_err(|error| error.to_string())
            };
            let outcome = result.as_ref().map(|_| ()).map_err(Clone::clone);
            publish_write_result(&state, &mut pending, &outcome, true);
            let _ = response.send(SettingsCommitResponse { request_id, result });
            outcome
        } else {
            let result = if protected {
                Err(startup_protection_message())
            } else if pending.is_empty() {
                Ok(())
            } else {
                write_pending_ui_state(&path, &pending, history_persist_disabled, lock_timeout)
                    .map_err(|error| error.to_string())
            };
            publish_write_result(&state, &mut pending, &result, attempted);
            result
        };
        if let Some(reply) = flush_reply {
            let _ = reply.send(result.clone());
        }
        if let Some(reply) = shutdown_reply {
            let _ = reply.send(result);
            break;
        }
        if disconnected {
            break;
        }
    }
}
fn publish_write_result(
    state: &Mutex<AdmissionState>,
    pending: &mut Vec<PendingUiStateWrite>,
    result: &Result<(), String>,
    attempted: bool,
) {
    if !attempted {
        return;
    }
    if let Ok(mut state) = state.lock() {
        match result {
            Ok(()) => {
                if let Some(last) = pending.last() {
                    state.status.persisted_generation = last.generation;
                }
                state.outstanding -= pending.len();
                if state.status.persisted_generation == state.status.accepted_generation
                    && !state.status.startup_protected
                {
                    state.status.last_error = None;
                }
            }
            Err(error) => state.status.last_error = Some(error.clone()),
        }
    }
    // Potentially large payloads are released by this worker, outside the lock.
    if result.is_ok() {
        pending.clear();
    }
}

pub(crate) fn seed_persisted_history_snapshot(path: PathBuf, history: &[String]) {
    if let Ok(mut registry) = ui_state_persistence_registry().lock() {
        registry
            .history_snapshots
            .insert(path, normalize_history_recency(history.to_vec()));
    }
}

#[cfg(test)]
use crate::fs_atomic::write_bytes_atomic;
#[cfg(test)]
use crate::path_utils::normalize_windows_path_buf;
#[cfg(test)]
use crate::query_history::MAX_QUERY_HISTORY_ENTRIES;
#[cfg(test)]
mod tests;

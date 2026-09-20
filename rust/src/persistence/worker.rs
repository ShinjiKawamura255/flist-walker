//! Serializes patch and history updates under the shared document lock.
use super::history_persist_disabled;
use super::paths::ui_state_file_path;
use super::schema::UiState;
use crate::fs_atomic::{
    acquire_sidecar_lock, atomic_write_replaced_destination, write_bytes_atomic, write_text_atomic,
};
use crate::path_utils::normalize_windows_path_buf;
use crate::query_history::MAX_QUERY_HISTORY_ENTRIES;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

static UI_STATE_PERSISTENCE: OnceLock<Mutex<UiStatePersistenceRegistry>> = OnceLock::new();

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
    tx: Sender<UiStatePersistenceCommand>,
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
        let (tx, rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            run_ui_state_persistence_worker(rx, path, history_persist_disabled, lock_timeout)
        });
        Self {
            tx,
            history_persist_disabled,
            handle: Mutex::new(Some(handle)),
        }
    }

    pub fn enqueue_history(&self, history_delta: Vec<String>) -> Result<(), String> {
        if self.history_persist_disabled {
            return Ok(());
        }
        self.tx
            .send(UiStatePersistenceCommand::Enqueue {
                patch: UiStatePatch::default(),
                history_delta: normalize_history_delta(history_delta),
            })
            .map_err(|_| "UI-state persistence worker is unavailable".to_string())
    }

    pub fn flush(&self, timeout: Duration) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.tx
            .send(UiStatePersistenceCommand::Flush(tx))
            .map_err(|_| "UI-state persistence worker is unavailable".to_string())?;
        rx.recv_timeout(timeout)
            .map_err(|_| "UI-state persistence flush timed out".to_string())?
    }

    pub fn shutdown(self, timeout: Duration) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.tx
            .send(UiStatePersistenceCommand::Shutdown(tx))
            .map_err(|_| "UI-state persistence worker is unavailable".to_string())?;
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
        let _ = self.tx.send(UiStatePersistenceCommand::Enqueue {
            patch,
            history_delta: normalize_history_delta(history_delta),
        });
    }
}

#[derive(Default)]
struct UiStatePersistenceRegistry {
    senders: std::collections::HashMap<PathBuf, Sender<UiStatePersistenceCommand>>,
    history_snapshots: std::collections::HashMap<PathBuf, Vec<String>>,
}

fn ui_state_persistence_registry() -> &'static Mutex<UiStatePersistenceRegistry> {
    UI_STATE_PERSISTENCE.get_or_init(|| Mutex::new(UiStatePersistenceRegistry::default()))
}

fn spawn_detached_ui_state_persistence_worker(
    path: PathBuf,
    history_persist_disabled: bool,
) -> Sender<UiStatePersistenceCommand> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        run_ui_state_persistence_worker(
            rx,
            path,
            history_persist_disabled,
            UI_STATE_PERSISTENCE_LOCK_TIMEOUT,
        )
    });
    tx
}

pub(crate) fn enqueue_ui_state_patch(
    path: PathBuf,
    patch: UiStatePatch,
    history_snapshot: Vec<String>,
    history_persist_disabled: bool,
) {
    let Ok(mut registry) = ui_state_persistence_registry().lock() else {
        return;
    };
    let history_delta = if history_persist_disabled {
        Vec::new()
    } else {
        let history_snapshot = normalize_history_recency(history_snapshot);
        let previous = registry.history_snapshots.entry(path.clone()).or_default();
        let delta = history_delta_from_snapshot(previous, &history_snapshot);
        *previous = history_snapshot;
        delta
    };
    let sender = registry
        .senders
        .entry(path.clone())
        .or_insert_with(|| {
            spawn_detached_ui_state_persistence_worker(path.clone(), history_persist_disabled)
        })
        .clone();
    drop(registry);
    let _ = sender.send(UiStatePersistenceCommand::Enqueue {
        patch: patch.without_history(),
        history_delta,
    });
}

fn persistence_sender_for_path(
    path: PathBuf,
    history_persist_disabled: bool,
) -> Result<Sender<UiStatePersistenceCommand>, String> {
    let mut registry = ui_state_persistence_registry()
        .lock()
        .map_err(|_| "UI-state persistence registry is unavailable".to_string())?;
    Ok(registry
        .senders
        .entry(path.clone())
        .or_insert_with(|| {
            spawn_detached_ui_state_persistence_worker(path, history_persist_disabled)
        })
        .clone())
}

pub(crate) fn enqueue_settings_commit(
    ui_state_path: PathBuf,
    history_persist_disabled: bool,
    request: SettingsCommitRequest,
) -> Result<mpsc::Receiver<SettingsCommitResponse>, String> {
    let sender = persistence_sender_for_path(ui_state_path, history_persist_disabled)?;
    let (response_tx, response_rx) = mpsc::channel();
    sender
        .send(UiStatePersistenceCommand::CommitSettings {
            request,
            response: response_tx,
        })
        .map_err(|_| "UI-state persistence worker is unavailable".to_string())?;
    Ok(response_rx)
}

pub(crate) fn flush_ui_state_persistence(path: &Path, timeout: Duration) {
    let sender = ui_state_persistence_registry()
        .lock()
        .ok()
        .and_then(|registry| registry.senders.get(path).cloned());
    let Some(sender) = sender else {
        return;
    };
    let (tx, rx) = mpsc::channel();
    if sender.send(UiStatePersistenceCommand::Flush(tx)).is_ok() {
        let _ = rx.recv_timeout(timeout);
    }
}

#[cfg(test)]
pub(crate) fn shutdown_ui_state_persistence_for_test(path: &Path, timeout: Duration) {
    let sender = ui_state_persistence_registry()
        .lock()
        .ok()
        .and_then(|mut registry| {
            registry.history_snapshots.remove(path);
            registry.senders.remove(path)
        });
    let Some(sender) = sender else {
        return;
    };
    let (tx, rx) = mpsc::channel();
    if sender.send(UiStatePersistenceCommand::Shutdown(tx)).is_ok() {
        let _ = rx.recv_timeout(timeout);
    }
}

fn run_ui_state_persistence_worker(
    rx: mpsc::Receiver<UiStatePersistenceCommand>,
    path: PathBuf,
    history_persist_disabled: bool,
    lock_timeout: Duration,
) {
    let mut next_generation = 1u64;
    let mut pending = Vec::<PendingUiStateWrite>::new();
    loop {
        let command = rx.recv_timeout(UI_STATE_PERSISTENCE_RETRY_DELAY);
        let mut flush_reply = None;
        let mut shutdown_reply = None;
        let mut settings_commit = None;
        let mut disconnected = false;
        match command {
            Ok(UiStatePersistenceCommand::Enqueue {
                patch,
                history_delta,
            }) => {
                push_pending_ui_state_write(
                    &mut pending,
                    &mut next_generation,
                    patch,
                    history_delta,
                );
            }
            Ok(UiStatePersistenceCommand::CommitSettings { request, response }) => {
                settings_commit = Some((request, response));
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
                        patch,
                        history_delta,
                    } => push_pending_ui_state_write(
                        &mut pending,
                        &mut next_generation,
                        patch,
                        history_delta,
                    ),
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
        let result = if let Some((request, response)) = settings_commit {
            let request_id = request.request_id;
            let result = commit_settings(
                &path,
                &pending,
                history_persist_disabled,
                lock_timeout,
                request,
            )
            .map_err(|error| error.to_string());
            if result.is_ok() {
                pending.clear();
            }
            let _ = response.send(SettingsCommitResponse { request_id, result });
            Ok(())
        } else if pending.is_empty() {
            Ok(())
        } else {
            write_pending_ui_state(&path, &pending, history_persist_disabled, lock_timeout)
                .map(|_| pending.clear())
                .map_err(|error| error.to_string())
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

fn build_ui_state_document(
    path: &Path,
    pending: &[PendingUiStateWrite],
    extra_patch: Option<&UiStatePatch>,
    history_persist_disabled: bool,
) -> std::io::Result<Value> {
    // Startup may fall back to defaults, but a writer must not turn a failed
    // read into permission to replace existing data. Only absence permits seed.
    let mut document = match fs::read_to_string(path) {
        Ok(text) => {
            let document = serde_json::from_str::<Value>(&text).map_err(|error| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("UI-state JSON is invalid; existing file was not changed: {error}"),
                )
            })?;
            if !document.is_object() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "UI-state JSON must be an object; existing file was not changed",
                ));
            }
            document
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Value::Object(Default::default())
        }
        Err(error) => {
            return Err(std::io::Error::new(
                error.kind(),
                format!("UI-state read failed; existing file was not changed: {error}"),
            ));
        }
    };
    for write in pending {
        merge_json_leaves(&mut document, &write.patch.0);
    }
    if let Some(patch) = extra_patch {
        merge_json_leaves(&mut document, &patch.0);
    }
    if !history_persist_disabled {
        let history = document
            .get("query_history")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut history = normalize_history_recency(history);
        for delta in pending.iter().flat_map(|write| write.history_delta.iter()) {
            append_history_delta(&mut history, delta.clone());
        }
        if let Value::Object(map) = &mut document {
            map.insert(
                "query_history".to_string(),
                Value::Array(history.into_iter().map(Value::String).collect()),
            );
        }
    }
    canonicalize_last_root_for_persistence(&mut document);
    canonicalize_default_root_for_persistence(&mut document);
    Ok(document)
}

fn commit_settings(
    ui_state_path: &Path,
    pending: &[PendingUiStateWrite],
    history_persist_disabled: bool,
    lock_timeout: Duration,
    request: SettingsCommitRequest,
) -> std::io::Result<SettingsCommitReceipt> {
    commit_settings_with_writer(
        ui_state_path,
        pending,
        history_persist_disabled,
        lock_timeout,
        request,
        write_bytes_atomic,
    )
}

fn commit_settings_with_writer<W>(
    ui_state_path: &Path,
    pending: &[PendingUiStateWrite],
    history_persist_disabled: bool,
    lock_timeout: Duration,
    request: SettingsCommitRequest,
    mut write_atomic: W,
) -> std::io::Result<SettingsCommitReceipt>
where
    W: FnMut(&Path, &[u8]) -> std::io::Result<()>,
{
    let _lock = acquire_sidecar_lock(ui_state_path, lock_timeout)?;
    let ui_state_previous = read_optional_file(ui_state_path)?;
    let document = build_ui_state_document(
        ui_state_path,
        pending,
        Some(&request.patch),
        history_persist_disabled,
    )?;
    let serialized = serde_json::to_string_pretty(&document).map_err(std::io::Error::other)?;
    let canonical_default_root = document
        .get("default_root")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(normalize_windows_path_buf);

    let mut roots_rollback = None;
    if let Some((roots_path, roots_text)) = request.saved_roots.as_ref() {
        let previous = read_optional_file(roots_path)?;
        if let Err(error) = write_atomic(roots_path, roots_text.as_bytes()) {
            if atomic_write_replaced_destination(&error) {
                if let Err(rollback_error) =
                    restore_atomic_target(roots_path, previous.as_deref(), &mut write_atomic)
                {
                    return Err(std::io::Error::other(format!(
                        "{error}; saved-roots rollback failed: {rollback_error}"
                    )));
                }
            }
            return Err(error);
        }
        roots_rollback = Some((roots_path, previous));
    }

    if let Err(error) = write_atomic(ui_state_path, serialized.as_bytes()) {
        let mut rollback_errors = Vec::new();
        if atomic_write_replaced_destination(&error) {
            if let Err(rollback_error) = restore_atomic_target(
                ui_state_path,
                ui_state_previous.as_deref(),
                &mut write_atomic,
            ) {
                rollback_errors.push(format!("UI-state rollback failed: {rollback_error}"));
            }
        }
        if let Some((roots_path, previous)) = roots_rollback {
            if let Err(rollback_error) =
                restore_atomic_target(roots_path, previous.as_deref(), &mut write_atomic)
            {
                rollback_errors.push(format!("saved-roots rollback failed: {rollback_error}"));
            }
        }
        if !rollback_errors.is_empty() {
            return Err(std::io::Error::other(format!(
                "{error}; {}",
                rollback_errors.join("; ")
            )));
        }
        return Err(error);
    }

    Ok(SettingsCommitReceipt {
        canonical_default_root,
    })
}

fn read_optional_file(path: &Path) -> std::io::Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn restore_atomic_target<W>(
    path: &Path,
    previous: Option<&[u8]>,
    write_atomic: &mut W,
) -> std::io::Result<()>
where
    W: FnMut(&Path, &[u8]) -> std::io::Result<()>,
{
    match previous {
        Some(bytes) => write_atomic(path, bytes),
        None => fs::remove_file(path).or_else(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(error)
            }
        }),
    }
}

fn push_pending_ui_state_write(
    pending: &mut Vec<PendingUiStateWrite>,
    next_generation: &mut u64,
    patch: UiStatePatch,
    history_delta: Vec<String>,
) {
    pending.push(PendingUiStateWrite {
        generation: *next_generation,
        patch,
        history_delta,
    });
    *next_generation = next_generation.saturating_add(1);
}

fn write_pending_ui_state(
    path: &Path,
    pending: &[PendingUiStateWrite],
    history_persist_disabled: bool,
    lock_timeout: Duration,
) -> std::io::Result<()> {
    debug_assert!(pending
        .windows(2)
        .all(|writes| writes[0].generation < writes[1].generation));
    let _lock = acquire_sidecar_lock(path, lock_timeout)?;
    let document = build_ui_state_document(path, pending, None, history_persist_disabled)?;
    let text = serde_json::to_string_pretty(&document).map_err(std::io::Error::other)?;
    write_text_atomic(path, &text)
}

pub(crate) fn canonicalize_last_root_for_persistence(document: &mut Value) {
    let Some(last_root) = document
        .get("last_root")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    let canonical = PathBuf::from(last_root)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(last_root));
    if let Value::Object(map) = document {
        map.insert(
            "last_root".to_string(),
            Value::String(canonical.to_string_lossy().to_string()),
        );
    }
}

fn canonicalize_default_root_for_persistence(document: &mut Value) {
    let Some(default_root) = document
        .get("default_root")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    let canonical = PathBuf::from(default_root)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(default_root));
    if let Value::Object(map) = document {
        map.insert(
            "default_root".to_string(),
            Value::String(canonical.to_string_lossy().to_string()),
        );
    }
}

fn merge_json_leaves(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(target), Value::Object(patch)) => {
            for (key, value) in patch {
                match target.get_mut(key) {
                    Some(existing) => merge_json_leaves(existing, value),
                    None => {
                        target.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (Value::Array(target), Value::Array(patch)) => {
            for (index, value) in patch.iter().enumerate() {
                if let Some(existing) = target.get_mut(index) {
                    merge_json_leaves(existing, value);
                } else {
                    target.push(value.clone());
                }
            }
            target.truncate(patch.len());
        }
        (target, patch) => *target = patch.clone(),
    }
}

fn normalize_history_delta(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

fn normalize_history_recency(values: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for value in normalize_history_delta(values) {
        append_history_delta(&mut normalized, value);
    }
    normalized
}

fn append_history_delta(history: &mut Vec<String>, delta: String) {
    history.retain(|existing| existing != &delta);
    history.push(delta);
    if history.len() > MAX_QUERY_HISTORY_ENTRIES {
        let trim = history.len() - MAX_QUERY_HISTORY_ENTRIES;
        history.drain(..trim);
    }
}

fn history_delta_from_snapshot(previous: &[String], current: &[String]) -> Vec<String> {
    let current = normalize_history_recency(current.to_vec());
    for start in (0..=current.len()).rev() {
        let mut replayed = normalize_history_recency(previous.to_vec());
        for delta in &current[start..] {
            append_history_delta(&mut replayed, delta.clone());
        }
        if replayed == current {
            return current[start..].to_vec();
        }
    }
    current
}

pub(crate) fn seed_persisted_history_snapshot(path: PathBuf, history: &[String]) {
    if let Ok(mut registry) = ui_state_persistence_registry().lock() {
        registry
            .history_snapshots
            .insert(path, normalize_history_recency(history.to_vec()));
    }
}

#[cfg(test)]
mod tests;

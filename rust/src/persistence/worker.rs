//! Owns bounded admission, ordered barriers, retry and persistence status.
#[cfg(test)]
mod diagnostic;
mod document;
mod settings;
#[cfg(test)]
use diagnostic::FlushDiagnostic;

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
    #[cfg(test)]
    progress: Arc<TestWorkerProgress>,
}

#[cfg(test)]
struct TestWorkerProgress {
    started: std::time::Instant,
    phase: std::sync::atomic::AtomicU8,
    last_timeout: Mutex<Option<Value>>,
    diagnostic: Option<Arc<FlushDiagnostic>>,
}

#[cfg(test)]
impl TestWorkerProgress {
    fn new(diagnostic: Option<Arc<FlushDiagnostic>>) -> Self {
        Self {
            diagnostic,
            started: std::time::Instant::now(),
            phase: std::sync::atomic::AtomicU8::new(0),
            last_timeout: Mutex::new(None),
        }
    }
    fn set_phase(&self, phase: u8) {
        self.phase
            .store(phase, std::sync::atomic::Ordering::Relaxed);
    }
    fn phase(&self) -> &'static str {
        match self.phase.load(std::sync::atomic::Ordering::Relaxed) {
            0 => "starting",
            1 => "waiting-command",
            2 => "pre-io-gate",
            3 => "write-result",
            4 => "result-published",
            5 => "returning",
            _ => "worker-body-returned",
        }
    }
}

/// Observational only: atomics/status may advance between reads. Never wait for
/// diagnostic locks, and never replace is_finished/join with a phase label.
#[cfg(test)]
fn record_test_worker_timeout(
    sender: Option<&PersistenceSender>,
    started: std::time::Instant,
    registry_elapsed: Duration,
    ownership_elapsed: Duration,
    stage: &str,
    shutdown_sent: Option<bool>,
) {
    let Some(sender) = sender else {
        eprintln!("TEST_WRITER_TIMEOUT sender_missing=true stage={stage}");
        return;
    };
    let status = sender.state.try_lock().ok().map(|state| {
        (
            state.status.accepted_generation,
            state.status.persisted_generation,
            state.outstanding,
            state.status.last_error.is_some(),
        )
    });
    let diagnostic = serde_json::json!({
        "stage": stage,
        "phase": sender.progress.phase(),
        "worker_age_ms": sender.progress.started.elapsed().as_secs_f64() * 1000.0,
        "elapsed_ms": started.elapsed().as_secs_f64() * 1000.0,
        "registry_acquisition_ms": registry_elapsed.as_secs_f64() * 1000.0,
        "ownership_acquisition_ms": ownership_elapsed.as_secs_f64() * 1000.0,
        "shutdown_sent": shutdown_sent,
        "physical_finished": (stage == "physical-stop").then_some(false),
        "status_available": status.is_some(),
        "accepted_generation": status.map(|status| status.0),
        "persisted_generation": status.map(|status| status.1),
        "outstanding": status.map(|status| status.2),
        "last_error_present": status.map(|status| status.3),
    });
    eprintln!("TEST_WRITER_TIMEOUT {diagnostic}");
    if let Ok(mut last) = sender.progress.last_timeout.try_lock() {
        *last = Some(diagnostic);
    }
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
        #[cfg(test)]
        if let Some(trace) = &self.progress.diagnostic {
            trace.record("enqueue-accepted", Some(generation));
        }
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
        let (sender, handle) = spawn_ui_state_persistence_worker(
            path,
            history_persist_disabled,
            lock_timeout,
            false,
            #[cfg(test)]
            None,
        );
        Self {
            sender,
            history_persist_disabled,
            handle: Mutex::new(Some(handle)),
        }
    }
    #[cfg(test)]
    fn new_with_diagnostic(
        path: PathBuf,
        disabled: bool,
        lock_timeout: Duration,
        label: &'static str,
    ) -> Self {
        let trace = Arc::new(FlushDiagnostic::new(label));
        trace.record("spawn-requested", None);
        let (sender, handle) =
            spawn_ui_state_persistence_worker(path, disabled, lock_timeout, false, Some(trace));
        Self {
            sender,
            history_persist_disabled: disabled,
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
    #[cfg(test)]
    test_workers: std::collections::HashMap<PathBuf, Arc<Mutex<TestWorkerOwnership>>>,
}

#[cfg(test)]
struct TestWorkerOwnership {
    handle: Option<thread::JoinHandle<()>>,
    shutdown_sent: bool,
    completed: Option<Result<(), String>>,
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
    #[cfg(test)] diagnostic: Option<Arc<FlushDiagnostic>>,
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
    #[cfg(test)]
    let progress = Arc::new(TestWorkerProgress::new(diagnostic));
    #[cfg(test)]
    let worker_progress = Arc::clone(&progress);
    let handle = thread::spawn(move || {
        run_ui_state_persistence_worker(
            rx,
            path,
            history_persist_disabled,
            lock_timeout,
            worker_state,
            #[cfg(test)]
            worker_progress,
        )
    });
    (
        PersistenceSender {
            tx,
            state,
            #[cfg(test)]
            progress,
        },
        handle,
    )
}
fn registry_sender(
    registry: &mut UiStatePersistenceRegistry,
    path: PathBuf,
    history_persist_disabled: bool,
) -> PersistenceSender {
    let startup_protected = registry.startup_failures.contains(&path);
    #[cfg(not(test))]
    {
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
    #[cfg(test)]
    {
        if let Some(sender) = registry.senders.get(&path) {
            return sender.clone();
        }
        let (sender, handle) = spawn_ui_state_persistence_worker(
            path.clone(),
            history_persist_disabled,
            UI_STATE_PERSISTENCE_LOCK_TIMEOUT,
            startup_protected,
            None,
        );
        registry.test_workers.insert(
            path.clone(),
            Arc::new(Mutex::new(TestWorkerOwnership {
                handle: Some(handle),
                shutdown_sent: false,
                completed: None,
            })),
        );
        registry.senders.insert(path, sender.clone());
        sender
    }
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
    #[cfg(test)]
    if let Some(trace) = &sender.progress.diagnostic {
        trace.record("flush-send-enter", None);
    }
    let admitted = sender.send_control(UiStatePersistenceCommand::Flush(tx));
    #[cfg(test)]
    if let Some(trace) = &sender.progress.diagnostic {
        trace.record(
            if admitted.is_ok() {
                "flush-send-accepted"
            } else {
                "flush-send-error"
            },
            None,
        );
    }
    admitted?;
    #[cfg(test)]
    if let Some(trace) = &sender.progress.diagnostic {
        trace.record("flush-recv-enter", None);
        trace.begin_receive();
    }
    let received = rx.recv_timeout(timeout);
    #[cfg(test)]
    if let Some(trace) = &sender.progress.diagnostic {
        trace.finish_receive(&received);
        trace.record(
            match &received {
                Ok(Ok(())) => "flush-recv-Ok",
                Ok(Err(_)) => "flush-recv-WorkerError",
                Err(mpsc::RecvTimeoutError::Timeout) => "flush-recv-Timeout",
                Err(mpsc::RecvTimeoutError::Disconnected) => "flush-recv-Disconnected",
            },
            None,
        );
    }
    received.map_err(|_| "UI-state persistence flush timed out".to_string())?
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
    let _ = finish_ui_state_persistence_for_test(path, timeout);
}

/// A test fixture may be deleted only after its writer has physically returned.
/// On timeout keep both sender and JoinHandle registered for a later bounded wait.
#[cfg(test)]
pub(crate) fn finish_ui_state_persistence_for_test(
    path: &Path,
    timeout: Duration,
) -> Result<(), String> {
    let started = std::time::Instant::now();
    let deadline = started + timeout;
    let (sender, ownership) = {
        let mut registry = ui_state_persistence_registry()
            .lock()
            .map_err(|_| "UI-state persistence registry is unavailable".to_string())?;
        let Some(ownership) = registry.test_workers.get(path).cloned() else {
            registry.history_snapshots.remove(path);
            registry.startup_failures.remove(path);
            return Ok(());
        };
        (registry.senders.get(path).cloned(), ownership)
    };
    let registry_elapsed = started.elapsed();
    let ownership_started = std::time::Instant::now();
    // Never wait while holding the global registry lock. Serialize only this path.
    let mut worker = loop {
        match ownership.try_lock() {
            Ok(worker) => break worker,
            Err(std::sync::TryLockError::Poisoned(_)) => {
                return Err("test writer ownership poisoned".into())
            }
            Err(std::sync::TryLockError::WouldBlock) => {
                if std::time::Instant::now() >= deadline {
                    record_test_worker_timeout(
                        sender.as_ref(),
                        started,
                        registry_elapsed,
                        ownership_started.elapsed(),
                        "ownership",
                        None,
                    );
                    return Err("test writer ownership wait timed out".into());
                }
                thread::sleep(Duration::from_millis(1));
            }
        }
    };
    let ownership_elapsed = ownership_started.elapsed();
    if worker.completed.is_none() {
        if !worker.shutdown_sent {
            let (tx, _rx) = mpsc::channel();
            let sent = sender
                .as_ref()
                .ok_or("test writer sender missing")?
                .send_control(UiStatePersistenceCommand::Shutdown(tx));
            if let Err(error) = sent {
                if !worker
                    .handle
                    .as_ref()
                    .is_some_and(thread::JoinHandle::is_finished)
                {
                    return Err(error);
                }
            } else {
                worker.shutdown_sent = true;
            }
        }
        while !worker
            .handle
            .as_ref()
            .is_some_and(thread::JoinHandle::is_finished)
        {
            if std::time::Instant::now() >= deadline {
                record_test_worker_timeout(
                    sender.as_ref(),
                    started,
                    registry_elapsed,
                    ownership_elapsed,
                    "physical-stop",
                    Some(worker.shutdown_sent),
                );
                return Err("test settings writer did not physically stop before deadline".into());
            }
            thread::sleep(Duration::from_millis(1));
        }
        worker.completed = Some(
            worker
                .handle
                .take()
                .expect("owned writer")
                .join()
                .map_err(|_| "test settings writer panicked".to_string()),
        );
    }
    worker.completed.as_ref().expect("joined writer").clone()?;
    drop(worker);
    let mut registry = ui_state_persistence_registry()
        .lock()
        .map_err(|_| "UI-state persistence registry is unavailable".to_string())?;
    if registry
        .test_workers
        .get(path)
        .is_some_and(|current| Arc::ptr_eq(current, &ownership))
    {
        registry.test_workers.remove(path);
        registry.senders.remove(path);
        registry.history_snapshots.remove(path);
        registry.startup_failures.remove(path);
    }
    Ok(())
}

fn run_ui_state_persistence_worker(
    rx: mpsc::Receiver<UiStatePersistenceCommand>,
    path: PathBuf,
    history_persist_disabled: bool,
    lock_timeout: Duration,
    state: Arc<Mutex<AdmissionState>>,
    #[cfg(test)] progress: Arc<TestWorkerProgress>,
) {
    #[cfg(test)]
    let _trace_lifetime = diagnostic::WorkerLifetime::new(progress.diagnostic.clone());
    let mut pending = Vec::<PendingUiStateWrite>::new();
    loop {
        #[cfg(test)]
        progress.set_phase(1);
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
            }) => {
                #[cfg(test)]
                if let Some(trace) = &progress.diagnostic {
                    trace.record("worker-received-enqueue", Some(generation));
                }
                pending.push(PendingUiStateWrite {
                    generation,
                    patch,
                    history_delta,
                });
            }
            Ok(UiStatePersistenceCommand::CommitSettings { request, response }) => {
                settings_commit = Some((request, response))
            }
            Ok(UiStatePersistenceCommand::Flush(reply)) => {
                #[cfg(test)]
                if let Some(trace) = &progress.diagnostic {
                    trace.record("worker-received-flush", None);
                }
                flush_reply = Some(reply);
            }
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
                    } => {
                        #[cfg(test)]
                        if let Some(trace) = &progress.diagnostic {
                            trace.record("worker-received-enqueue", Some(generation));
                        }
                        pending.push(PendingUiStateWrite {
                            generation,
                            patch,
                            history_delta,
                        });
                    }
                    UiStatePersistenceCommand::CommitSettings { request, response } => {
                        settings_commit = Some((request, response));
                        break;
                    }
                    UiStatePersistenceCommand::Flush(reply) => {
                        #[cfg(test)]
                        if let Some(trace) = &progress.diagnostic {
                            trace.record("worker-received-flush", None);
                        }
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
        #[cfg(test)]
        if let Some(trace) = &progress.diagnostic {
            trace.record("status-read-enter", None);
        }
        let protected = state
            .lock()
            .map(|state| state.status.startup_protected)
            .unwrap_or(true);
        #[cfg(test)]
        progress.set_phase(2);
        #[cfg(test)]
        if let Some(trace) = &progress.diagnostic {
            trace.record("status-read-return", None);
            trace.record("gate-enter", None);
        }
        #[cfg(test)]
        let after_write_gate = if attempted && !protected {
            tests::pause_before_write(&path)
        } else {
            None
        };
        #[cfg(test)]
        progress.set_phase(3);
        #[cfg(test)]
        if let Some(trace) = &progress.diagnostic {
            trace.record("gate-return", None);
        }
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
                write_pending_ui_state(
                    &path,
                    &pending,
                    history_persist_disabled,
                    lock_timeout,
                    #[cfg(test)]
                    progress.diagnostic.as_deref(),
                )
                .map_err(|error| error.to_string())
            };
            #[cfg(test)]
            if let Some(trace) = &progress.diagnostic {
                trace.record("publish-enter", None);
            }
            publish_write_result(&state, &mut pending, &result, attempted);
            #[cfg(test)]
            if let Some(trace) = &progress.diagnostic {
                trace.record("publish-return", None);
            }
            result
        };
        #[cfg(test)]
        progress.set_phase(4);
        #[cfg(test)]
        tests::pause_after_write(after_write_gate);
        if let Some(reply) = flush_reply {
            #[cfg(test)]
            if let Some(trace) = &progress.diagnostic {
                trace.record("flush-reply-enter", None);
            }
            let sent = reply.send(result.clone());
            #[cfg(test)]
            if let Some(trace) = &progress.diagnostic {
                trace.record(
                    if sent.is_ok() {
                        "flush-reply-sent"
                    } else {
                        "flush-reply-disconnected"
                    },
                    None,
                );
            }
            let _ = sent;
        }
        if let Some(reply) = shutdown_reply {
            let _ = reply.send(result);
            #[cfg(test)]
            progress.set_phase(5);
            break;
        }
        if disconnected {
            #[cfg(test)]
            progress.set_phase(5);
            break;
        }
    }
    #[cfg(test)]
    progress.set_phase(6);
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
#[cfg(test)]
pub(crate) use tests::WriteGate;

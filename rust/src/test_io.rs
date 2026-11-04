//! Test-only operation boundaries. These are not OS stack samples.
//! No callback, lock, allocation or clock read is added to non-test builds.
mod history;
pub(crate) use history::Kind;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum Stage {
    Idle,
    SidecarLock,
    CreateParent,
    SidecarOpen,
    SidecarTryLock,
    SidecarRetryWait,
    WriteDocument,
    CommitSettings,
    BuildDocument,
    ReadDocument,
    CanonicalizeLastRoot,
    CanonicalizeDefaultRoot,
    SerializeDocument,
    AtomicWrite,
    DirectoryPlan,
    OpenTemp,
    WriteTemp,
    SyncTemp,
    CloseTemp,
    BeforeReplace,
    ReplaceFile,
    SyncDirectory,
    CleanupTemp,
    PublishResult,
    PublishStateLock,
    DropPending,
    ReceiveCommand,
    StartupStateLock,
    WaitFlush,
    WaitOwnership,
    WaitPhysical,
}

fn label(code: u8) -> &'static str {
    const LABELS: &[&str] = &[
        "idle",
        "sidecar-lock",
        "create-parent",
        "sidecar-open",
        "sidecar-try-lock",
        "sidecar-retry-wait",
        "write-document",
        "commit-settings",
        "build-document",
        "read-document",
        "canonicalize-last-root",
        "canonicalize-default-root",
        "serialize-document",
        "atomic-write",
        "directory-plan",
        "open-temp",
        "write-temp",
        "sync-temp",
        "close-temp",
        "before-replace",
        "replace-file",
        "sync-directory",
        "cleanup-temp",
        "publish-result",
        "publish-state-lock",
        "drop-pending",
        "receive-command",
        "startup-state-lock",
        "wait-flush",
        "wait-ownership",
        "wait-physical",
    ];
    LABELS.get(usize::from(code)).copied().unwrap_or("unknown")
}

pub(crate) type Observer = Arc<dyn Fn(Stage) + Send + Sync>;

pub(crate) struct Progress {
    worker_id: u64,
    started: Instant,
    // One observation: low8 current stage, next8 parent, upper48 start micros.
    recorded: AtomicU64,
    native_tid: AtomicU32,
    observer_injected: AtomicBool,
    history: history::History,
    next_command: AtomicU64,
    next_operation: AtomicU64,
    command: AtomicU64,
    generation: AtomicU64,
    operation: AtomicU64,
}

impl Progress {
    pub(crate) fn new() -> Arc<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Arc::new(Self {
            worker_id: NEXT.fetch_add(1, Ordering::Relaxed),
            started: Instant::now(),
            recorded: AtomicU64::new(Stage::Idle as u64),
            native_tid: AtomicU32::new(0),
            observer_injected: AtomicBool::new(false),
            history: history::History::new(),
            next_command: AtomicU64::new(0),
            next_operation: AtomicU64::new(0),
            command: AtomicU64::new(0),
            generation: AtomicU64::new(0),
            operation: AtomicU64::new(0),
        })
    }

    pub(crate) fn event(
        &self,
        kind: Kind,
        command: u64,
        generation: u64,
        operation: u64,
        stage: Stage,
        detail: u8,
    ) {
        self.history.record(history::Record {
            at_us: self.micros(),
            kind,
            command,
            generation,
            operation,
            stage: stage as u8,
            detail,
        });
    }
    pub(crate) fn command_attempt(&self, detail: u8, generation: u64) -> u64 {
        let id = self.next_command.fetch_add(1, Ordering::Relaxed) + 1;
        self.event(Kind::SendAttempt, id, generation, 0, Stage::Idle, detail);
        id
    }
    pub(crate) fn dequeued(&self, id: u64, detail: u8, generation: u64) {
        self.command.store(id, Ordering::Relaxed);
        self.generation.store(generation, Ordering::Relaxed);
        self.event(Kind::Dequeued, id, generation, 0, Stage::Idle, detail);
    }
    pub(crate) fn batch(&self, generation: u64, kind: Kind) {
        self.generation.store(generation, Ordering::Relaxed);
        self.event(
            kind,
            self.command.load(Ordering::Relaxed),
            generation,
            0,
            Stage::Idle,
            0,
        );
    }
    pub(crate) fn lifecycle(&self, kind: Kind, detail: u8) {
        self.event(
            kind,
            self.command.load(Ordering::Relaxed),
            self.generation.load(Ordering::Relaxed),
            0,
            Stage::Idle,
            detail,
        );
    }
    pub(crate) fn wait_begin(&self, stage: Stage, command: u64) -> u64 {
        let id = self.next_operation.fetch_add(1, Ordering::Relaxed) + 1;
        self.event(Kind::WaitBegin, command, 0, id, stage, 0);
        id
    }
    pub(crate) fn wait_end(&self, stage: Stage, command: u64, id: u64, kind: Kind) {
        self.event(kind, command, 0, id, stage, 0);
    }

    fn micros(&self) -> u64 {
        self.started
            .elapsed()
            .as_micros()
            .min(u128::from(u64::MAX >> 16)) as u64
    }

    pub(crate) fn snapshot(&self) -> serde_json::Value {
        let value = self.recorded.load(Ordering::Relaxed);
        let current = value as u8;
        let tid = self.native_tid.load(Ordering::Relaxed);
        serde_json::json!({
            "worker_id": self.worker_id,
            "process_id": std::process::id(),
            "native_thread_id": (tid != 0).then_some(tid),
            "stage": label(current),
            "parent": label((value >> 8) as u8),
            "stage_elapsed_ms": (current != 0).then(|| self.micros().saturating_sub(value >> 16) as f64 / 1000.0),
            "observer_injected": self.observer_injected.load(Ordering::Relaxed),
            "command_id":self.command.load(Ordering::Relaxed),
            "generation":self.generation.load(Ordering::Relaxed),
            "operation_id":self.operation.load(Ordering::Relaxed),
            "history":self.history.snapshot(),
        })
    }
}

#[derive(Clone)]
struct Context {
    progress: Arc<Progress>,
    extra: Option<Observer>,
}

thread_local! {
    static CURRENT: RefCell<Option<Context>> = const { RefCell::new(None) };
    // Only explicit test controls propagate, never an installed worker recorder.
    static EXTRA: RefCell<Option<Observer>> = const { RefCell::new(None) };
}

pub(crate) struct Scope {
    previous: Option<Context>,
}

impl Drop for Scope {
    fn drop(&mut self) {
        CURRENT.with(|slot| {
            *slot.borrow_mut() = self.previous.take();
        });
    }
}

pub(crate) fn install(progress: Arc<Progress>, extra: Option<Observer>) -> Scope {
    progress
        .native_tid
        .store(native_thread_id(), Ordering::Relaxed);
    progress
        .observer_injected
        .store(extra.is_some(), Ordering::Relaxed);
    progress.event(Kind::Started, 0, 0, 0, Stage::Idle, 0);
    let previous = CURRENT.with(|slot| slot.replace(Some(Context { progress, extra })));
    Scope { previous }
}

#[cfg(windows)]
fn native_thread_id() -> u32 {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThreadId() -> u32;
    }
    // This read-only API has no preconditions and returns the calling OS thread.
    unsafe { GetCurrentThreadId() }
}

#[cfg(not(windows))]
fn native_thread_id() -> u32 {
    0
}

pub(crate) fn inherited_observer() -> Option<Observer> {
    EXTRA.with(|slot| slot.borrow().clone())
}

pub(crate) struct Capture {
    previous: Option<Observer>,
}

impl Drop for Capture {
    fn drop(&mut self) {
        EXTRA.with(|slot| {
            *slot.borrow_mut() = self.previous.take();
        });
    }
}

pub(crate) fn capture_on_spawn(observer: Observer) -> Capture {
    Capture {
        previous: EXTRA.with(|slot| slot.replace(Some(observer))),
    }
}

pub(crate) struct Operation {
    progress: Option<Arc<Progress>>,
    previous: u64,
    previous_operation: u64,
    id: u64,
    command: u64,
    generation: u64,
    stage: Stage,
}

impl Drop for Operation {
    fn drop(&mut self) {
        if let Some(progress) = &self.progress {
            // No callback/lock/allocation, including unwind. Timestamp is a monotonic read.
            progress.recorded.store(self.previous, Ordering::Relaxed);
            progress
                .operation
                .store(self.previous_operation, Ordering::Relaxed);
            progress.event(
                if std::thread::panicking() {
                    Kind::IoUnwind
                } else {
                    Kind::IoEnd
                },
                self.command,
                self.generation,
                self.id,
                self.stage,
                0,
            );
        }
    }
}

pub(crate) fn enter(stage: Stage) -> Operation {
    // End the TLS borrow before an optional controlled test callback runs.
    let context = CURRENT.with(|slot| slot.borrow().clone());
    let Some(context) = context else {
        return Operation {
            progress: None,
            previous: 0,
            previous_operation: 0,
            id: 0,
            command: 0,
            generation: 0,
            stage,
        };
    };
    let previous = context.progress.recorded.load(Ordering::Relaxed);
    let id = context
        .progress
        .next_operation
        .fetch_add(1, Ordering::Relaxed)
        + 1;
    let command = context.progress.command.load(Ordering::Relaxed);
    let generation = context.progress.generation.load(Ordering::Relaxed);
    let previous_operation = context.progress.operation.swap(id, Ordering::Relaxed);
    // Establish restoration before publishing or invoking a possibly-panicking control.
    let guard = Operation {
        progress: Some(Arc::clone(&context.progress)),
        previous,
        previous_operation,
        id,
        command,
        generation,
        stage,
    };
    context.progress.recorded.store(
        (context.progress.micros() << 16) | ((previous & 0xff) << 8) | stage as u64,
        Ordering::Relaxed,
    );
    context
        .progress
        .event(Kind::IoBegin, command, generation, id, stage, 0);
    if let Some(extra) = &context.extra {
        extra(stage);
    }
    guard
}

pub(crate) fn valid_history_label(kind: &str, stage: &str) -> bool {
    history::valid_kind(kind) && (0..=Stage::WaitPhysical as u8).any(|code| label(code) == stage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_error_and_callback_unwind_restore_original_span_and_scope() {
        let outer = Progress::new();
        let _scope = install(Arc::clone(&outer), None);
        let _parent = enter(Stage::WriteDocument);
        let original = outer.recorded.load(Ordering::Relaxed);
        let fail_in_child_scope = || -> Result<(), ()> {
            let _child = enter(Stage::OpenTemp);
            Err(())
        };
        let result = fail_in_child_scope();
        assert!(result.is_err());
        assert_eq!(outer.recorded.load(Ordering::Relaxed), original);
        let inner = Progress::new();
        let outcome = std::panic::catch_unwind(|| {
            let _nested = install(
                Arc::clone(&inner),
                Some(Arc::new(|_| panic!("controlled observer panic"))),
            );
            let _operation = enter(Stage::ReplaceFile);
        });
        assert!(outcome.is_err());
        assert_eq!(inner.recorded.load(Ordering::Relaxed), 0);
        let inner_history = inner.history.snapshot();
        assert!(inner_history["records"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["kind"] == "io-unwind" && r["stage"] == "replace-file"));
        assert!(outer.history.snapshot()["records"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["kind"] == "io-end" && r["stage"] == "open-temp"));
        assert_eq!(outer.recorded.load(Ordering::Relaxed), original);
        assert!(inherited_observer().is_none());
        let _next = enter(Stage::SerializeDocument);
        assert_eq!(outer.snapshot()["parent"], "write-document");
    }

    #[test]
    fn ordinary_worker_observer_is_not_inherited_by_another_thread() {
        let progress = Progress::new();
        let _scope = install(progress, Some(Arc::new(|_| {})));
        assert!(inherited_observer().is_none());
        std::thread::spawn(|| {
            assert!(inherited_observer().is_none());
            let _operation = enter(Stage::OpenTemp);
            assert!(CURRENT.with(|slot| slot.borrow().is_none()));
        })
        .join()
        .unwrap();
    }
}

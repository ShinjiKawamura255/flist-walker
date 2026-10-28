//! Diagnostic-carrier-only test observations. A checkpoint is the last observed
//! before-call boundary, never a kernel stack or a transactional state sample.
use super::TestWorkerProgress;
use std::cell::Cell;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};

thread_local! {
    static CURRENT: Cell<Option<Weak<TestWorkerProgress>>> = const { Cell::new(None) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum WriterIoCheckpoint {
    SidecarParent = 1,
    SidecarOpen,
    SidecarLock,
    SidecarRetry,
    DocumentRead,
    DocumentDecode,
    CanonicalLastRoot,
    CanonicalDefaultRoot,
    DocumentSerialize,
    AtomicAncestors,
    AtomicParent,
    AtomicOpen,
    AtomicWrite,
    AtomicPermissions,
    AtomicTimes,
    AtomicFileSync,
    AtomicBeforeReplace,
    AtomicClose,
    AtomicReplace,
    AtomicParentSync,
    PublishResult,
}

impl WriterIoCheckpoint {
    const ALL: [Self; 21] = [
        Self::SidecarParent,
        Self::SidecarOpen,
        Self::SidecarLock,
        Self::SidecarRetry,
        Self::DocumentRead,
        Self::DocumentDecode,
        Self::CanonicalLastRoot,
        Self::CanonicalDefaultRoot,
        Self::DocumentSerialize,
        Self::AtomicAncestors,
        Self::AtomicParent,
        Self::AtomicOpen,
        Self::AtomicWrite,
        Self::AtomicPermissions,
        Self::AtomicTimes,
        Self::AtomicFileSync,
        Self::AtomicBeforeReplace,
        Self::AtomicClose,
        Self::AtomicReplace,
        Self::AtomicParentSync,
        Self::PublishResult,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::SidecarParent => "before-sidecar-parent-create",
            Self::SidecarOpen => "before-sidecar-open",
            Self::SidecarLock => "before-sidecar-try-lock",
            Self::SidecarRetry => "before-sidecar-retry-delay",
            Self::DocumentRead => "before-document-read",
            Self::DocumentDecode => "before-document-decode",
            Self::CanonicalLastRoot => "before-last-root-canonicalize",
            Self::CanonicalDefaultRoot => "before-default-root-canonicalize",
            Self::DocumentSerialize => "before-document-serialize",
            Self::AtomicAncestors => "before-atomic-ancestor-probes",
            Self::AtomicParent => "before-atomic-parent-create",
            Self::AtomicOpen => "before-atomic-temp-open",
            Self::AtomicWrite => "before-atomic-file-write",
            Self::AtomicPermissions => "before-atomic-permissions",
            Self::AtomicTimes => "before-atomic-file-times",
            Self::AtomicFileSync => "before-atomic-file-sync",
            Self::AtomicBeforeReplace => "before-atomic-replace-hook",
            Self::AtomicClose => "before-atomic-file-close",
            Self::AtomicReplace => "before-atomic-replace",
            Self::AtomicParentSync => "before-atomic-parent-sync",
            Self::PublishResult => "before-result-publication",
        }
    }
}

pub(super) fn checkpoint_name(value: u8) -> Option<&'static str> {
    WriterIoCheckpoint::ALL
        .into_iter()
        .find(|phase| *phase as u8 == value)
        .map(WriterIoCheckpoint::label)
}

pub(super) fn path_fingerprint(path: &Path) -> u64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hash);
    hash.finish()
}

fn thread_fingerprint() -> u64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    std::thread::current().id().hash(&mut hash);
    hash.finish()
}

#[must_use]
pub(super) struct WriterIoScope {
    previous: Option<Weak<TestWorkerProgress>>,
    registered: bool,
    // Restoring a different thread's TLS would misattribute observations.
    _same_thread: PhantomData<Rc<()>>,
}

impl WriterIoScope {
    pub(super) fn bind(progress: &Arc<TestWorkerProgress>) -> Self {
        progress
            .worker_thread_fingerprint
            .store(thread_fingerprint(), Ordering::Relaxed);
        let previous = CURRENT.try_with(|slot| slot.replace(Some(Arc::downgrade(progress))));
        let (previous, registered) = match previous {
            Ok(value) => (value, true),
            Err(_) => (None, false),
        };
        Self {
            previous,
            registered,
            _same_thread: PhantomData,
        }
    }
}

impl Drop for WriterIoScope {
    fn drop(&mut self) {
        if self.registered {
            let previous = self.previous.take();
            let _ = CURRENT.try_with(|slot| slot.set(previous));
        }
    }
}

pub(crate) fn record_test_writer_io_checkpoint(phase: WriterIoCheckpoint) {
    // No I/O, locking, callback, or registry access while the TLS slot is taken.
    // An expired owner or TLS teardown simply leaves the observation missing.
    let _ = CURRENT.try_with(|slot| {
        let context = slot.take();
        if let Some(progress) = context.as_ref().and_then(Weak::upgrade) {
            progress
                .last_io_checkpoint
                .store(phase as u8, Ordering::Relaxed);
        }
        slot.set(context);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn observed(progress: &TestWorkerProgress) -> Option<&'static str> {
        checkpoint_name(progress.last_io_checkpoint.load(Ordering::Relaxed))
    }
    #[test]
    fn writer_io_checkpoint_records_only_its_registered_owner() {
        let first = Arc::new(TestWorkerProgress::new());
        let second = Arc::new(TestWorkerProgress::new());
        let _outer = WriterIoScope::bind(&first);
        record_test_writer_io_checkpoint(WriterIoCheckpoint::DocumentRead);
        {
            let _inner = WriterIoScope::bind(&second);
            record_test_writer_io_checkpoint(WriterIoCheckpoint::AtomicFileSync);
            assert_eq!(observed(&first), Some("before-document-read"));
            assert_eq!(observed(&second), Some("before-atomic-file-sync"));
        }
        record_test_writer_io_checkpoint(WriterIoCheckpoint::PublishResult);
        assert_eq!(observed(&first), Some("before-result-publication"));
        assert_eq!(observed(&second), Some("before-atomic-file-sync"));
    }
    #[test]
    fn writer_io_context_restores_after_unwind() {
        let first = Arc::new(TestWorkerProgress::new());
        let second = Arc::new(TestWorkerProgress::new());
        let _outer = WriterIoScope::bind(&first);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _inner = WriterIoScope::bind(&second);
            record_test_writer_io_checkpoint(WriterIoCheckpoint::SidecarOpen);
            panic!("controlled diagnostic unwind");
        }));
        assert!(result.is_err());
        record_test_writer_io_checkpoint(WriterIoCheckpoint::DocumentRead);
        assert_eq!(observed(&first), Some("before-document-read"));
        assert_eq!(observed(&second), Some("before-sidecar-open"));
    }
    #[test]
    fn writer_io_missing_and_expired_context_are_safe() {
        record_test_writer_io_checkpoint(WriterIoCheckpoint::AtomicWrite);
        let first = Arc::new(TestWorkerProgress::new());
        let outer = WriterIoScope::bind(&first);
        let second = Arc::new(TestWorkerProgress::new());
        {
            let _expired = WriterIoScope::bind(&second);
            drop(second);
            record_test_writer_io_checkpoint(WriterIoCheckpoint::AtomicWrite);
            assert_eq!(observed(&first), None);
        }
        record_test_writer_io_checkpoint(WriterIoCheckpoint::PublishResult);
        assert_eq!(observed(&first), Some("before-result-publication"));
        drop(outer);
        record_test_writer_io_checkpoint(WriterIoCheckpoint::AtomicWrite);
        assert_eq!(observed(&first), Some("before-result-publication"));
    }
    #[test]
    fn writer_io_context_is_thread_local_and_returns_physically() {
        let first = Arc::new(TestWorkerProgress::new());
        let second = Arc::new(TestWorkerProgress::new());
        let _outer = WriterIoScope::bind(&first);
        let child_progress = Arc::clone(&second);
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let child = std::thread::spawn(move || {
            let _scope = WriterIoScope::bind(&child_progress);
            record_test_writer_io_checkpoint(WriterIoCheckpoint::AtomicFileSync);
            assert_eq!(
                child_progress
                    .worker_thread_fingerprint
                    .load(Ordering::Relaxed),
                thread_fingerprint()
            );
            ready_tx.send(()).unwrap();
        });
        ready_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        while !child.is_finished() && std::time::Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert!(
            child.is_finished(),
            "diagnostic-only child did not physically return"
        );
        child.join().unwrap();
        record_test_writer_io_checkpoint(WriterIoCheckpoint::DocumentRead);
        assert_eq!(observed(&first), Some("before-document-read"));
        assert_eq!(observed(&second), Some("before-atomic-file-sync"));
    }
    #[test]
    fn writer_io_checkpoint_names_do_not_infer_missing_or_unknown_steps() {
        assert_eq!(checkpoint_name(0), None);
        assert_eq!(checkpoint_name(255), None);
        let labels = WriterIoCheckpoint::ALL
            .into_iter()
            .map(WriterIoCheckpoint::label)
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(labels.len(), 21);
        assert!(labels.iter().all(|name| name.starts_with("before-")));
    }
}

pub(super) const NATIVE_EVENT_PREFIX: &str = "NATIVE_WRITER_OBSERVATION ";
const NATIVE_LEAF: &str = "app::tests::indexing_perf::harness::extensions::tc_229_all_tab_profile_controls_preserve_own_expected_state";

pub(super) fn native_observation_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        native_scope_valid(
            &std::env::var("FW_INDEX_PERF_NORMAL_CHILD").unwrap_or_default(),
            &std::env::var("FW_INDEX_PERF_NORMAL_CHILD_ROLE").unwrap_or_default(),
            &std::env::args_os()
                .skip(1)
                .map(|value| value.into_string().unwrap_or_default())
                .collect::<Vec<_>>(),
        )
    })
}
fn native_scope_valid(child: &str, role: &str, arguments: &[String]) -> bool {
    child == NATIVE_LEAF
        && role == "normal"
        && arguments
            .windows(2)
            .any(|pair| pair[0] == "--exact" && pair[1] == NATIVE_LEAF)
        && arguments.iter().any(|arg| arg == "--test-threads=1")
}

#[derive(Clone, Debug, serde::Serialize)]
pub(super) struct NativeThreadSample {
    platform: &'static str,
    cpu_100ns: Option<u64>,
    io_pending: Option<bool>,
    cpu_error: Option<u32>,
    io_error: Option<u32>,
}

pub(super) fn native_thread_sample(
    handle: Option<&std::thread::JoinHandle<()>>,
) -> NativeThreadSample {
    #[cfg(not(windows))]
    {
        let _ = handle;
        NativeThreadSample {
            platform: "unsupported",
            cpu_100ns: None,
            io_pending: None,
            cpu_error: None,
            io_error: None,
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        #[repr(C)]
        #[derive(Default)]
        struct FileTime {
            low: u32,
            high: u32,
        }
        impl FileTime {
            fn ticks(&self) -> u64 {
                (u64::from(self.high) << 32) | u64::from(self.low)
            }
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetThreadTimes(
                handle: *mut std::ffi::c_void,
                created: *mut FileTime,
                exited: *mut FileTime,
                kernel: *mut FileTime,
                user: *mut FileTime,
            ) -> i32;
            fn GetThreadIOPendingFlag(handle: *mut std::ffi::c_void, pending: *mut i32) -> i32;
            fn GetLastError() -> u32;
        }
        let Some(handle) = handle else {
            return NativeThreadSample {
                platform: "missing-handle",
                cpu_100ns: None,
                io_pending: None,
                cpu_error: None,
                io_error: None,
            };
        };
        let (mut created, mut exited, mut kernel, mut user) = (
            FileTime::default(),
            FileTime::default(),
            FileTime::default(),
            FileTime::default(),
        );
        // Borrow this exact owned handle only; never open, duplicate, suspend or close it.
        let times_ok = unsafe {
            GetThreadTimes(
                handle.as_raw_handle(),
                &mut created,
                &mut exited,
                &mut kernel,
                &mut user,
            )
        } != 0;
        let cpu_error = (!times_ok).then(|| unsafe { GetLastError() });
        let mut pending = 0;
        let io_ok = unsafe { GetThreadIOPendingFlag(handle.as_raw_handle(), &mut pending) } != 0;
        let io_error = (!io_ok).then(|| unsafe { GetLastError() });
        NativeThreadSample {
            platform: "windows",
            cpu_100ns: times_ok
                .then(|| kernel.ticks().checked_add(user.ticks()))
                .flatten(),
            io_pending: io_ok.then_some(pending != 0),
            cpu_error,
            io_error,
        }
    }
}

fn native_class(before: &NativeThreadSample, after: &NativeThreadSample) -> &'static str {
    if before.platform == "unsupported" || after.platform == "unsupported" {
        return "unsupported";
    }
    if before.cpu_error.is_some()
        || before.io_error.is_some()
        || after.cpu_error.is_some()
        || after.io_error.is_some()
    {
        return "query-incomplete";
    }
    let (Some(a), Some(b), Some(io_a), Some(io_b)) = (
        before.cpu_100ns,
        after.cpu_100ns,
        before.io_pending,
        after.io_pending,
    ) else {
        return "query-incomplete";
    };
    if b < a {
        "counter-inconsistent"
    } else if io_a || io_b {
        "io-pending-observed"
    } else if b > a {
        "cpu-progress-no-positive-io-sample"
    } else {
        "no-io-sample-no-cpu-progress"
    }
}

pub(super) fn native_wait_event(
    progress: &TestWorkerProgress,
    before: &(u64, NativeThreadSample, u64),
    after: NativeThreadSample,
    wall_after_us: u64,
    decision: &'static str,
) -> serde_json::Value {
    serde_json::json!({
        "schema": 1, "kind": "owned-thread-samples", "wait_decision": decision, "wait_sequence": before.2,
        "wall_before_us": before.0, "wall_after_us": wall_after_us,
        "worker_path_fingerprint": progress.path_fingerprint.map(|v| format!("{v:016x}")),
        "worker_rust_thread_fingerprint": progress.worker_thread_fingerprint.load(Ordering::Relaxed),
        "phase": progress.phase(),
        "last_io_checkpoint": checkpoint_name(progress.last_io_checkpoint.load(Ordering::Relaxed)),
        "sample_before": before.1, "sample_after": after,
        "classification": native_class(&before.1, &after),
    })
}

pub(super) fn emit_native_wait_event(event: &serde_json::Value) {
    use std::io::Write;
    // Caller has released worker ownership and registry locks. The child output
    // is an owned drained file; the parent exports only finite validated fields.
    if let Ok(line) = serde_json::to_string(event) {
        let _ = writeln!(std::io::stderr().lock(), "{NATIVE_EVENT_PREFIX}{line}");
    }
}

#[cfg(test)]
mod native_tests {
    use super::*;
    #[test]
    fn native_diag_classifier_preserves_unknown_and_instantaneous_limits() {
        let args = ["--exact", NATIVE_LEAF, "--test-threads=1"].map(str::to_owned);
        assert!(native_scope_valid(NATIVE_LEAF, "normal", &args));
        assert!(!native_scope_valid(NATIVE_LEAF, "perf", &args));
        assert!(!native_scope_valid("other-leaf", "normal", &args));
        assert!(!native_scope_valid(
            NATIVE_LEAF,
            "normal",
            &["--ignored".into()]
        ));
        let sample = |cpu, io| NativeThreadSample {
            platform: "windows",
            cpu_100ns: cpu,
            io_pending: io,
            cpu_error: None,
            io_error: None,
        };
        assert_eq!(
            native_class(&sample(Some(1), Some(false)), &sample(Some(1), Some(true))),
            "io-pending-observed"
        );
        assert_eq!(
            native_class(&sample(Some(1), Some(false)), &sample(Some(2), Some(false))),
            "cpu-progress-no-positive-io-sample"
        );
        assert_eq!(
            native_class(&sample(Some(1), Some(false)), &sample(Some(1), Some(false))),
            "no-io-sample-no-cpu-progress"
        );
        assert_eq!(
            native_class(&sample(Some(2), Some(false)), &sample(Some(1), Some(false))),
            "counter-inconsistent"
        );
        assert_eq!(
            native_class(&sample(None, Some(false)), &sample(Some(1), Some(false))),
            "query-incomplete"
        );
        let mut denied = sample(None, None);
        denied.io_error = Some(5);
        assert_eq!(
            native_class(&denied, &sample(Some(1), Some(true))),
            "query-incomplete"
        );
    }
    #[cfg(windows)]
    #[test]
    fn native_diag_queries_only_a_borrowed_owned_live_thread() {
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let child = std::thread::spawn(move || {
            let _ = ready_tx.send(());
            let _ = release_rx.recv_timeout(std::time::Duration::from_secs(2));
        });
        let ready = ready_rx.recv_timeout(std::time::Duration::from_secs(2));
        let sample = native_thread_sample(Some(&child));
        let _ = release_tx.send(());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !child.is_finished() && std::time::Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert!(
            child.is_finished(),
            "native query fixture did not physically return"
        );
        let joined = child.join();
        ready.unwrap();
        joined.unwrap();
        assert_eq!(sample.platform, "windows");
        assert!(sample.cpu_100ns.is_some() && sample.io_pending.is_some());
        assert!(sample.cpu_error.is_none() && sample.io_error.is_none());
    }
}

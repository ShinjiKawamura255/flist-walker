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

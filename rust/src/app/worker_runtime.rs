use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

pub(super) struct WorkerRuntime {
    shutdown: Arc<AtomicBool>,
    handles: Vec<NamedWorkerHandle>,
}

struct NamedWorkerHandle {
    name: String,
    handle: thread::JoinHandle<()>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WorkerJoinSummary {
    pub(super) joined: usize,
    pub(super) total: usize,
    pub(super) pending: Vec<String>,
}

impl WorkerRuntime {
    pub(super) fn new(shutdown: Arc<AtomicBool>) -> Self {
        Self {
            shutdown,
            handles: Vec::new(),
        }
    }

    pub(super) fn push(&mut self, name: impl Into<String>, handle: thread::JoinHandle<()>) {
        self.handles.push(NamedWorkerHandle {
            name: name.into(),
            handle,
        });
    }

    pub(super) fn request_shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }

    pub(super) fn join_all_with_timeout(mut self, timeout: Duration) -> WorkerJoinSummary {
        let total = self.handles.len();
        if total == 0 {
            return WorkerJoinSummary {
                joined: 0,
                total: 0,
                pending: Vec::new(),
            };
        }

        let (tx, rx) = mpsc::channel::<String>();
        let mut pending = self
            .handles
            .iter()
            .map(|handle| handle.name.clone())
            .collect::<Vec<_>>();
        for named_handle in self.handles.drain(..) {
            let tx_done = tx.clone();
            let name = named_handle.name;
            let handle = named_handle.handle;
            thread::spawn(move || {
                let _ = handle.join();
                let _ = tx_done.send(name);
            });
        }
        drop(tx);

        let deadline = Instant::now() + timeout;
        let mut joined = 0usize;
        while joined < total {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let remain = deadline.saturating_duration_since(now);
            match rx.recv_timeout(remain) {
                Ok(name) => {
                    joined = joined.saturating_add(1);
                    pending.retain(|pending_name| pending_name != &name);
                }
                Err(_) => break,
            }
        }

        WorkerJoinSummary {
            joined,
            total,
            pending,
        }
    }
}

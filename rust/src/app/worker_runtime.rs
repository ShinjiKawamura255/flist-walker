use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use super::*;

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

impl FlistWalkerApp {
    /// worker request sender を dummy channel へ差し替えて shutdown を開始する。
    fn disconnect_worker_channels(&mut self) {
        let (dummy_search_tx, _) = mpsc::channel::<SearchRequest>();
        let (dummy_preview_tx, _) = mpsc::channel::<PreviewRequest>();
        let (dummy_action_tx, _) = mpsc::channel::<ActionRequest>();
        let (dummy_sort_tx, _) = mpsc::channel::<SortMetadataRequest>();
        let (dummy_kind_tx, _) = mpsc::channel::<KindResolveRequest>();
        let (dummy_filelist_tx, _) = mpsc::channel::<FileListRequest>();
        let (dummy_update_tx, _) = mpsc::channel::<UpdateRequest>();
        let (dummy_index_tx, _) = mpsc::channel::<IndexRequest>();
        let old_search_tx = std::mem::replace(&mut self.shell.search.tx, dummy_search_tx);
        let old_preview_tx = std::mem::replace(&mut self.shell.worker_bus.preview.tx, dummy_preview_tx);
        let old_action_tx = std::mem::replace(&mut self.shell.worker_bus.action.tx, dummy_action_tx);
        let old_sort_tx = std::mem::replace(&mut self.shell.worker_bus.sort.tx, dummy_sort_tx);
        let old_kind_tx = std::mem::replace(&mut self.shell.worker_bus.kind.tx, dummy_kind_tx);
        let old_filelist_tx =
            std::mem::replace(&mut self.shell.worker_bus.filelist.tx, dummy_filelist_tx);
        let old_update_tx = std::mem::replace(&mut self.shell.worker_bus.update.tx, dummy_update_tx);
        let old_index_tx = std::mem::replace(&mut self.shell.indexing.tx, dummy_index_tx);
        drop(old_search_tx);
        drop(old_preview_tx);
        drop(old_action_tx);
        drop(old_sort_tx);
        drop(old_kind_tx);
        drop(old_filelist_tx);
        drop(old_update_tx);
        drop(old_index_tx);
    }

    /// worker 群へ shutdown を通知し、短い timeout で join を待つ。
    pub(super) fn shutdown_workers_with_timeout(
        &mut self,
        timeout: Duration,
        phase: &str,
    ) -> Option<WorkerJoinSummary> {
        let runtime = self.shell.worker_runtime.as_ref()?;
        runtime.request_shutdown();
        self.disconnect_worker_channels();
        let runtime = self.shell.worker_runtime.take()?;
        let summary = runtime.join_all_with_timeout(timeout);
        if summary.joined < summary.total {
            let pending = if summary.pending.is_empty() {
                "unknown".to_string()
            } else {
                summary.pending.join(", ")
            };
            eprintln!(
                "Worker shutdown timeout during {phase}: joined {}/{} threads within {:?}; pending: {pending}",
                summary.joined, summary.total, timeout
            );
        }
        Some(summary)
    }

    pub(super) fn request_viewport_close_if_needed(&mut self, ctx: &egui::Context) -> bool {
        if process_shutdown_requested() {
            self.set_notice("Shutdown requested by signal");
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return true;
        }
        if self.shell.features.update.close_requested_for_install {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return true;
        }
        false
    }

    pub(super) fn poll_runtime_events(&mut self) {
        self.poll_index_response();
        self.poll_search_response();
        self.poll_routed_worker_responses();
        self.poll_kind_response();
        self.pump_kind_resolution_requests();
        self.poll_filelist_response();
        self.poll_update_response();
    }
}

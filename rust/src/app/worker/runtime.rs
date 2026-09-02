use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use super::protocol::{
    ActionRequest, CatalogRequest, FileListRequest, IndexRequest, KindResolveRequest,
    PreviewRequest, RootValidationRequest, SearchRequest, SortMetadataRequest, UpdateRequest,
};
use crate::app::{process_shutdown_requested, FlistWalkerApp};
use eframe::egui;
use tracing::{info, warn};

pub(in crate::app) struct WorkerRuntime {
    shutdown: Arc<AtomicBool>,
    handles: Vec<NamedWorkerHandle>,
}

struct NamedWorkerHandle {
    name: String,
    handle: thread::JoinHandle<()>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::app) struct WorkerJoinSummary {
    pub(in crate::app) joined: usize,
    pub(in crate::app) total: usize,
    pub(in crate::app) pending: Vec<String>,
}

impl WorkerRuntime {
    pub(in crate::app) fn new(shutdown: Arc<AtomicBool>) -> Self {
        Self {
            shutdown,
            handles: Vec::new(),
        }
    }

    pub(in crate::app) fn push(&mut self, name: impl Into<String>, handle: thread::JoinHandle<()>) {
        self.handles.push(NamedWorkerHandle {
            name: name.into(),
            handle,
        });
    }

    pub(in crate::app) fn request_shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }

    #[cfg(test)]
    pub(in crate::app) fn worker_names(&self) -> Vec<String> {
        self.handles
            .iter()
            .map(|handle| handle.name.clone())
            .collect()
    }

    pub(in crate::app) fn join_all_with_timeout(mut self, timeout: Duration) -> WorkerJoinSummary {
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
    /// Move tab-owned snapshots and in-progress index buffers to a worker before
    /// the native UI starts waiting for shutdown. Dropping a large `Vec<Entry>`
    /// can otherwise consume the whole close-frame budget.
    fn offload_tab_resources_for_shutdown(&mut self, runtime: &mut WorkerRuntime) {
        let active_committed = self.take_active_committed_resources();
        let active_build = self.take_active_index_build_resources();
        let background_states = std::mem::take(&mut self.shell.indexing.background_states);
        let background_finalizations =
            std::mem::take(&mut self.shell.indexing.background_finalizations);
        let pending_stale_build_reclaim = self.shell.indexing.pending_stale_build_reclaim.take();
        let pending_replace_all = self.shell.indexing.pending_replace_all.take();
        let index_mailboxes = self.shell.indexing.take_all_mailboxes_for_shutdown();
        let tab_resources = self.shell.tabs.take_all_heavy_resources_for_shutdown();

        let handle = thread::Builder::new()
            .name("flistwalker-tab-shutdown-drain".to_string())
            .spawn(move || {
                drop((
                    active_committed,
                    active_build,
                    background_states,
                    background_finalizations,
                    pending_stale_build_reclaim,
                    pending_replace_all,
                    index_mailboxes,
                    tab_resources,
                ));
            })
            .expect("spawn tab shutdown drain");
        runtime.push("tab-shutdown-drain", handle);
        self.shell.tabs.disconnect_resource_reclaimer();
    }

    /// worker request sender を dummy channel へ差し替えて shutdown を開始する。
    fn disconnect_worker_channels(&mut self) {
        let (dummy_search_tx, _) = mpsc::channel::<SearchRequest>();
        let (dummy_preview_tx, _) = mpsc::channel::<PreviewRequest>();
        let (dummy_action_tx, _) = super::channel::bounded_request_channel::<ActionRequest>(1);
        let (dummy_sort_tx, _) = mpsc::channel::<SortMetadataRequest>();
        let (dummy_kind_tx, _) = super::channel::bounded_request_channel::<KindResolveRequest>(1);
        let (dummy_filelist_tx, _) = mpsc::channel::<FileListRequest>();
        let (dummy_update_tx, _) = mpsc::channel::<UpdateRequest>();
        let (dummy_catalog_tx, _) = mpsc::channel::<CatalogRequest>();
        let (dummy_root_validation_tx, _) = mpsc::channel::<RootValidationRequest>();
        let (dummy_index_tx, _) = super::channel::bounded_request_channel::<IndexRequest>(1);
        let old_search_tx = std::mem::replace(&mut self.shell.search.tx, dummy_search_tx);
        let old_preview_tx =
            std::mem::replace(&mut self.shell.worker_bus.preview.tx, dummy_preview_tx);
        let old_action_tx =
            std::mem::replace(&mut self.shell.worker_bus.action.tx, dummy_action_tx);
        let old_sort_tx = std::mem::replace(&mut self.shell.worker_bus.sort.tx, dummy_sort_tx);
        let old_kind_tx = std::mem::replace(&mut self.shell.worker_bus.kind.tx, dummy_kind_tx);
        let old_filelist_tx =
            std::mem::replace(&mut self.shell.worker_bus.filelist.tx, dummy_filelist_tx);
        let old_update_tx =
            std::mem::replace(&mut self.shell.worker_bus.update.tx, dummy_update_tx);
        let old_catalog_tx =
            std::mem::replace(&mut self.shell.worker_bus.catalog.tx, dummy_catalog_tx);
        let old_root_validation_tx = std::mem::replace(
            &mut self.shell.worker_bus.root_validation.tx,
            dummy_root_validation_tx,
        );
        let old_index_tx = std::mem::replace(&mut self.shell.indexing.tx, dummy_index_tx);
        drop(old_search_tx);
        drop(old_preview_tx);
        drop(old_action_tx);
        drop(old_sort_tx);
        drop(old_kind_tx);
        drop(old_filelist_tx);
        drop(old_update_tx);
        drop(old_catalog_tx);
        drop(old_root_validation_tx);
        drop(old_index_tx);
    }

    /// worker 群へ shutdown を通知し、短い timeout で join を待つ。
    pub(in crate::app) fn shutdown_workers_with_timeout(
        &mut self,
        timeout: Duration,
        phase: &str,
    ) -> Option<WorkerJoinSummary> {
        self.shell.worker_runtime.as_ref()?.request_shutdown();
        let action_load = self.shell.worker_bus.action.tx.load_observer();
        let kind_load = self.shell.worker_bus.kind.tx.load_observer();
        let index_load = self.shell.indexing.tx.load_observer();
        let mut runtime = self.shell.worker_runtime.take()?;
        self.offload_tab_resources_for_shutdown(&mut runtime);
        self.disconnect_worker_channels();
        let summary = runtime.join_all_with_timeout(timeout);
        if summary.joined < summary.total {
            let pending_names = if summary.pending.is_empty() {
                "unknown".to_string()
            } else {
                summary.pending.join(", ")
            };
            eprintln!(
                "Worker shutdown timeout during {phase}: joined {}/{} threads within {:?}; pending: {pending_names}",
                summary.joined, summary.total, timeout
            );
            for pending_worker in &summary.pending {
                let (worker_family, load, load_known) = if pending_worker.starts_with("action-") {
                    ("action", action_load.load(), true)
                } else if pending_worker == "kind-resolver" {
                    ("kind_resolver", kind_load.load(), true)
                } else if pending_worker.starts_with("index-") {
                    ("index", index_load.load(), true)
                } else {
                    (
                        "other",
                        super::channel::WorkerLoadSnapshot {
                            queued: 0,
                            inflight: 0,
                            capacity: 0,
                        },
                        false,
                    )
                };
                let record = super::channel::worker_trace_record(
                    load,
                    worker_family,
                    "shutdown_timeout",
                    super::channel::WorkerTraceContext {
                        worker_id: pending_worker,
                        request_id: None,
                        tab_id: None,
                        epoch: None,
                        outcome: "shutdown_timeout",
                    },
                );
                warn!(
                    flow = "worker_runtime",
                    worker_family = record.worker_family,
                    event = record.event,
                    worker_id = record.worker_id,
                    phase,
                    joined = summary.joined,
                    total = summary.total,
                    pending = pending_names,
                    timeout_ms = timeout.as_millis() as u64,
                    queue_depth = record.queue_depth,
                    in_flight = record.in_flight,
                    capacity = record.capacity,
                    load_scope = "family",
                    load_known,
                    outcome = record.outcome,
                    "worker shutdown exceeded its join budget"
                );
            }
        } else {
            info!(
                flow = "worker_runtime",
                event = "shutdown_complete",
                worker_id = "runtime",
                phase,
                joined = summary.joined,
                total = summary.total,
                timeout_ms = timeout.as_millis() as u64,
                outcome = "completed",
                "worker shutdown completed within its join budget"
            );
        }
        Some(summary)
    }

    pub(in crate::app) fn request_viewport_close_if_needed(&mut self, ctx: &egui::Context) -> bool {
        let signal_close = process_shutdown_requested();
        let native_close = ctx.input(|input| input.viewport().close_requested());
        if signal_close || native_close {
            if self.shell.features.update.defer_close_and_cancel() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.set_notice(if signal_close {
                    "Canceling update before signal shutdown..."
                } else {
                    "Canceling update before closing..."
                });
                ctx.request_repaint();
                return false;
            }
            if signal_close {
                self.set_notice("Shutdown requested by signal");
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return true;
            }
        }
        if self.shell.features.update.state.close_requested_for_install {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return true;
        }
        false
    }

    pub(in crate::app) fn poll_runtime_events(&mut self) {
        self.poll_index_response();
        self.poll_search_response();
        self.poll_routed_worker_responses();
        self.poll_kind_response();
        self.pump_kind_resolution_requests();
        self.poll_filelist_response();
        self.poll_update_response();
        self.poll_catalog_response();
        self.poll_root_validation_response();
    }
}

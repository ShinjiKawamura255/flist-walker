//! One filesystem probe worker and one physical request, including timed-out I/O.
use super::{observe_file, FileListChange, FileObservation, SnapshotFreshness};
use crate::indexer::IndexSource;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const CHECK_INTERVAL: Duration = Duration::from_secs(5);
const CHECK_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::app) struct CheckIdentity {
    serial: u64,
    pub(in crate::app) tab_id: u64,
    pub(in crate::app) root: PathBuf,
    snapshot_request_id: u64,
    path: PathBuf,
}

impl CheckIdentity {
    pub(in crate::app) fn matches(
        &self,
        snapshot: &SnapshotFreshness,
        tab_id: Option<u64>,
        root: &Path,
    ) -> bool {
        tab_id == Some(self.tab_id)
            && root == self.root
            && snapshot.root == self.root
            && snapshot.request_id == self.snapshot_request_id
            && matches!(&snapshot.source, IndexSource::FileList(path) if path == &self.path)
    }

    pub(in crate::app) fn suspend(
        &self,
        snapshot: &mut SnapshotFreshness,
        tab_id: Option<u64>,
        root: &Path,
    ) {
        if self.matches(snapshot, tab_id, root) {
            snapshot.monitor_suspended = true;
            if snapshot.change != FileListChange::Changed {
                snapshot.change = FileListChange::Unavailable;
            }
        }
    }
}

struct CheckResponse {
    identity: CheckIdentity,
    observation: FileObservation,
    completed_at: Instant,
}

struct PendingCheck {
    identity: CheckIdentity,
    started_at: Instant,
    timed_out: bool,
}

#[derive(Default)]
pub(in crate::app) struct MonitorTick {
    pub(in crate::app) repaint_after: Option<Duration>,
    pub(in crate::app) timeout: Option<CheckIdentity>,
}

pub(in crate::app) struct FreshnessMonitor {
    tx: Option<SyncSender<CheckIdentity>>,
    rx: Receiver<CheckResponse>,
    pending: Option<PendingCheck>,
    next_serial: u64,
}

impl FreshnessMonitor {
    pub(in crate::app) fn new(shutdown: Arc<AtomicBool>) -> (Self, JoinHandle<()>) {
        Self::spawn_with(shutdown, observe_file)
    }

    pub(in crate::app) fn spawn_with(
        shutdown: Arc<AtomicBool>,
        mut observe: impl FnMut(&Path) -> FileObservation + Send + 'static,
    ) -> (Self, JoinHandle<()>) {
        let (tx, requests) = mpsc::sync_channel::<CheckIdentity>(1);
        let (responses, rx) = mpsc::sync_channel(1);
        let handle = thread::Builder::new()
            .name("flistwalker-filelist-freshness".into())
            .spawn(move || {
                while let Ok(identity) = requests.recv() {
                    if shutdown.load(Ordering::Acquire) {
                        break;
                    }
                    let observation = observe(&identity.path);
                    if shutdown.load(Ordering::Acquire) {
                        break;
                    }
                    let response = CheckResponse {
                        identity,
                        observation,
                        completed_at: Instant::now(),
                    };
                    if responses.try_send(response).is_err() {
                        break;
                    }
                }
            })
            .expect("spawn FileList freshness worker");
        (
            Self {
                tx: Some(tx),
                rx,
                pending: None,
                next_serial: 0,
            },
            handle,
        )
    }

    pub(in crate::app) fn disconnect(&mut self) {
        self.tx = None;
    }

    #[cfg(test)]
    pub(in crate::app) fn in_progress(&self) -> bool {
        self.pending.is_some()
    }

    /// Poll even when scheduling/application is disabled, so stale responses do
    /// not retain physical capacity. No filesystem operations run here.
    pub(in crate::app) fn tick(
        &mut self,
        mut snapshot: Option<&mut SnapshotFreshness>,
        tab_id: Option<u64>,
        root: &Path,
        enabled: bool,
        foreground: bool,
        now: Instant,
    ) -> MonitorTick {
        let mut result = MonitorTick::default();
        match self.rx.try_recv() {
            Ok(response) => {
                if self
                    .pending
                    .as_ref()
                    .is_some_and(|pending| pending.identity == response.identity)
                {
                    let pending = self.pending.take().expect("matched physical request");
                    let expired = pending.timed_out
                        || response
                            .completed_at
                            .saturating_duration_since(pending.started_at)
                            >= CHECK_TIMEOUT;
                    if expired {
                        if !pending.timed_out {
                            result.timeout = Some(pending.identity.clone());
                        }
                        if enabled && foreground {
                            if let Some(snapshot) = snapshot.as_deref_mut() {
                                pending.identity.suspend(snapshot, tab_id, root);
                            }
                        }
                    } else if enabled && foreground {
                        if let Some(snapshot) = snapshot.as_deref_mut() {
                            if response.identity.matches(snapshot, tab_id, root)
                                && !snapshot.monitor_suspended
                                && snapshot.change != FileListChange::Changed
                            {
                                snapshot.change = FileListChange::compare(
                                    &snapshot.baseline,
                                    &response.observation,
                                );
                            }
                        }
                    }
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.tx = None;
                if let Some(pending) = self.pending.take() {
                    if !pending.timed_out {
                        result.timeout = Some(pending.identity.clone());
                    }
                    if enabled && foreground {
                        if let Some(snapshot) = snapshot.as_deref_mut() {
                            pending.identity.suspend(snapshot, tab_id, root);
                        }
                    }
                }
            }
        }
        if let Some(pending) = &mut self.pending {
            if !pending.timed_out
                && now.saturating_duration_since(pending.started_at) >= CHECK_TIMEOUT
            {
                pending.timed_out = true;
                result.timeout = Some(pending.identity.clone());
                if enabled && foreground {
                    if let Some(snapshot) = snapshot.as_deref_mut() {
                        pending.identity.suspend(snapshot, tab_id, root);
                    }
                }
            }
            // Timeout is logical only: retain occupancy until the physical
            // worker answers or disconnects; never spawn replacement workers.
            result.repaint_after = Some(if pending.timed_out {
                Duration::from_secs(1)
            } else {
                Duration::from_millis(100)
            });
            return result;
        }
        if !enabled || !foreground {
            return result;
        }
        let (Some(snapshot), Some(tab_id)) = (snapshot, tab_id) else {
            return result;
        };
        if snapshot.root != root
            || snapshot.monitor_suspended
            || snapshot.change == FileListChange::Changed
        {
            return result;
        }
        let IndexSource::FileList(path) = &snapshot.source else {
            return result;
        };
        // The status explicitly covers the loaded root FileList only.
        if path.parent() != Some(root) {
            return result;
        }
        let elapsed = now.saturating_duration_since(snapshot.last_checked_at);
        if elapsed < CHECK_INTERVAL {
            result.repaint_after = Some(CHECK_INTERVAL - elapsed);
            return result;
        }
        let Some(next_serial) = self.next_serial.checked_add(1) else {
            snapshot.change = FileListChange::Unavailable;
            snapshot.monitor_suspended = true;
            return result;
        };
        let identity = CheckIdentity {
            serial: next_serial,
            tab_id,
            root: root.to_path_buf(),
            snapshot_request_id: snapshot.request_id,
            path: path.clone(),
        };
        let sent = self
            .tx
            .as_ref()
            .is_some_and(|tx| tx.try_send(identity.clone()).is_ok());
        if sent {
            self.next_serial = next_serial;
            snapshot.last_checked_at = now;
            self.pending = Some(PendingCheck {
                identity,
                started_at: now,
                timed_out: false,
            });
            result.repaint_after = Some(Duration::from_millis(100));
        } else {
            snapshot.change = FileListChange::Unavailable;
            snapshot.monitor_suspended = true;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freshness_ui_routes_inactive_timeout_then_rejects_late_response_on_return() {
        use crate::app::tests::{egui, test_root, test_settings_scope, FlistWalkerApp};
        let scope = test_settings_scope("freshness-inactive-timeout");
        let root_a = test_root("freshness-timeout-root-a");
        let root_b = test_root("freshness-timeout-root-b");
        for root in [&root_a, &root_b] {
            std::fs::create_dir_all(root).unwrap();
        }
        std::fs::write(root_a.join("FileList.txt"), "alpha.txt\n").unwrap();
        let mut app = scope.app(root_a.clone(), 50, String::new());
        let settle = |app: &mut FlistWalkerApp| {
            let deadline = Instant::now() + Duration::from_secs(5);
            while app.shell.indexing.in_progress || app.shell.indexing.pending_finish.is_some() {
                app.poll_index_response();
                app.poll_search_response();
                assert!(Instant::now() < deadline);
                thread::yield_now();
            }
        };
        settle(&mut app);
        let tab_a = app.current_tab_id().unwrap();
        let generation_a = app.shell.runtime.freshness.as_ref().unwrap().request_id;
        let acquired_a = app.shell.runtime.freshness.as_ref().unwrap().acquired_at;
        app.create_new_tab();
        app.apply_root_change(root_b.clone());
        settle(&mut app);
        assert_eq!(
            app.shell.runtime.freshness.as_ref().unwrap().source,
            IndexSource::Walker
        );
        let acquired_b = app.shell.runtime.freshness.as_ref().unwrap().acquired_at;
        app.switch_to_tab_index(0);
        app.filelist_auto_check_enabled = true;
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (monitor, handle) =
            FreshnessMonitor::spawn_with(Arc::new(AtomicBool::new(false)), move |_| {
                let _ = entered_tx.send(());
                let _ = release_rx.recv();
                FileObservation::Missing
            });
        app.freshness_monitor = monitor;
        app.shell
            .runtime
            .snapshot_freshness_mut()
            .unwrap()
            .last_checked_at = Instant::now() - CHECK_INTERVAL;
        let ctx = egui::Context::default();
        let frame = |app: &mut FlistWalkerApp| {
            ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000.0, 700.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    app.run_update_cycle(ui);
                },
            )
        };
        frame(&mut app);
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        app.switch_to_tab_index(1);
        // Advance the test-owned pending clock without adding production timing hooks.
        app.freshness_monitor.pending.as_mut().unwrap().started_at = Instant::now() - CHECK_TIMEOUT;
        frame(&mut app);
        let inactive = app.shell.tabs.iter().find(|tab| tab.id == tab_a).unwrap();
        let snapshot = inactive.result_state.committed.freshness.as_ref().unwrap();
        assert!(
            snapshot.monitor_suspended,
            "timeout must reach its inactive owner"
        );
        assert_eq!(snapshot.change, FileListChange::Unchanged);
        assert_eq!(snapshot.acquired_at, acquired_a);
        assert_eq!(
            app.shell.runtime.freshness.as_ref().unwrap().acquired_at,
            acquired_b
        );
        assert!(
            app.freshness_monitor.in_progress(),
            "physical I/O still owns capacity"
        );
        app.switch_to_tab_index(0);
        let output = frame(&mut app);
        let snapshot = app.shell.runtime.freshness.as_ref().unwrap();
        assert!(snapshot.monitor_suspended);
        assert_eq!(snapshot.change, FileListChange::Unavailable);
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::epaint::Shape::Text(text) if text.galley.text() == "FileList change check unavailable")));
        release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while app.freshness_monitor.in_progress() {
            frame(&mut app);
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        app.shell
            .runtime
            .snapshot_freshness_mut()
            .unwrap()
            .last_checked_at = Instant::now() - CHECK_INTERVAL;
        frame(&mut app);
        let snapshot = app.shell.runtime.freshness.as_ref().unwrap();
        assert_eq!(
            snapshot.change,
            FileListChange::Unavailable,
            "late Missing must not become Changed"
        );
        assert_eq!(snapshot.request_id, generation_a);
        assert_eq!(snapshot.acquired_at, acquired_a);
        assert!(
            !app.freshness_monitor.in_progress(),
            "suspended owner must not recheck"
        );
        assert!(entered_rx.try_recv().is_err(), "no replacement probe");
        app.freshness_monitor.disconnect();
        handle.join().unwrap();
        drop(app);
        for root in [root_a, root_b] {
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn serial_mismatch_keeps_physical_capacity_and_cannot_apply() {
        let (tx, requests) = mpsc::sync_channel(1);
        let (responses, rx) = mpsc::sync_channel(1);
        let mut monitor = FreshnessMonitor {
            tx: Some(tx),
            rx,
            pending: None,
            next_serial: 0,
        };
        let root = PathBuf::from("root");
        let mut snapshot = SnapshotFreshness::acquired(
            1,
            root.clone(),
            IndexSource::FileList(root.join("FileList.txt")),
            FileObservation::Unavailable,
            FileObservation::Unavailable,
        );
        snapshot.change = FileListChange::Unchanged;
        let now = Instant::now();
        snapshot.last_checked_at = now - CHECK_INTERVAL;
        monitor.tick(Some(&mut snapshot), Some(1), &root, true, true, now);
        let identity = requests.try_recv().unwrap();
        let mut stale = identity.clone();
        stale.serial += 1;
        responses
            .try_send(CheckResponse {
                identity: stale,
                observation: FileObservation::Missing,
                completed_at: now,
            })
            .unwrap();
        monitor.tick(Some(&mut snapshot), Some(1), &root, true, true, now);
        assert!(monitor.in_progress());
        assert_eq!(snapshot.change, FileListChange::Unchanged);
        assert!(requests.try_recv().is_err());
        responses
            .try_send(CheckResponse {
                identity,
                observation: FileObservation::Missing,
                completed_at: now,
            })
            .unwrap();
        monitor.tick(Some(&mut snapshot), Some(1), &root, true, true, now);
        assert!(!monitor.in_progress());
        assert_eq!(snapshot.change, FileListChange::Unavailable);
    }

    #[test]
    fn disconnected_or_shutdown_worker_settles_without_replacement_thread() {
        let (mut monitor, handle) = FreshnessMonitor::new(Arc::new(AtomicBool::new(true)));
        let root = PathBuf::from("root");
        let mut snapshot = SnapshotFreshness::acquired(
            1,
            root.clone(),
            IndexSource::FileList(root.join("FileList.txt")),
            FileObservation::Unavailable,
            FileObservation::Unavailable,
        );
        let now = Instant::now();
        snapshot.last_checked_at = now - CHECK_INTERVAL;
        monitor.tick(Some(&mut snapshot), Some(1), &root, true, true, now);
        handle.join().unwrap();
        monitor.tick(Some(&mut snapshot), Some(1), &root, true, true, now);
        assert!(!monitor.in_progress());
        assert!(snapshot.monitor_suspended);
        assert_eq!(snapshot.change, FileListChange::Unavailable);
        monitor.disconnect();
    }
}

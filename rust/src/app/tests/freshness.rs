use super::*;

fn indexed_filelist_app(name: &str) -> (FlistWalkerApp, PathBuf) {
    let root = test_root(name);
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("FileList.txt"), "alpha.txt\n").unwrap();
    fs::write(root.join("alpha.txt"), "alpha\n").unwrap();
    let scope = test_settings_scope(name);
    let mut app = scope.app(root.clone(), 50, String::new());
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.shell.indexing.in_progress || app.shell.indexing.pending_finish.is_some() {
        app.poll_index_response();
        app.poll_search_response();
        assert!(Instant::now() < deadline, "index did not settle");
        thread::yield_now();
    }
    (app, root)
}

#[test]
fn freshness_display_is_acquired_snapshot_bound() {
    let (mut app, root) = indexed_filelist_app("freshness-source");
    assert!(
        app.source_text().contains("Loaded"),
        "{}",
        app.source_text()
    );
    let old_source = app.source_text();
    app.shell.runtime.use_filelist = false;
    app.request_index_refresh();
    // A new request must not relabel the last committed snapshot as Walker.
    assert!(app.source_text().contains("FileList"));
    assert!(app.source_text().contains("Refreshing"));
    assert!(old_source.contains("FileList"));
    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn freshness_new_session_starts_without_acquisition_time() {
    let scope = test_settings_scope("freshness-not-loaded");
    let mut app = scope.app(test_root("freshness-missing"), 50, String::new());
    reset_index_request_state_for_test(&mut app);
    assert!(
        app.source_text().contains("Not loaded"),
        "{}",
        app.source_text()
    );
}

use crate::app::freshness::worker::FreshnessMonitor;
use crate::app::freshness::{observe_file, FileListChange, FileObservation, SnapshotFreshness};
use std::sync::atomic::AtomicBool;
use std::sync::{mpsc, Arc};

fn monitor_fixture(name: &str) -> (PathBuf, SnapshotFreshness) {
    let root = test_root(name);
    fs::create_dir_all(&root).unwrap();
    let path = root.join("FileList.txt");
    fs::write(&path, "alpha.txt\n").unwrap();
    let observation = observe_file(&path);
    let mut snapshot = SnapshotFreshness::acquired(
        17,
        root.clone(),
        IndexSource::FileList(path),
        observation.clone(),
        observation,
    );
    snapshot.last_checked_at = Instant::now() - Duration::from_secs(6);
    (root, snapshot)
}

fn settle_monitor(
    monitor: &mut FreshnessMonitor,
    snapshot: &mut SnapshotFreshness,
    tab_id: u64,
    root: &Path,
    now: Instant,
) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while monitor.in_progress() {
        monitor.tick(Some(snapshot), Some(tab_id), root, true, true, now);
        assert!(Instant::now() < deadline, "monitor did not settle");
        thread::yield_now();
    }
}

#[test]
fn freshness_monitor_checks_only_enabled_foreground_filelist_after_five_seconds() {
    let (root, mut snapshot) = monitor_fixture("freshness-monitor-schedule");
    let (mut monitor, handle) = FreshnessMonitor::new(Arc::new(AtomicBool::new(false)));
    let now = Instant::now();
    monitor.tick(Some(&mut snapshot), Some(1), &root, false, true, now);
    assert!(!monitor.in_progress());
    monitor.tick(Some(&mut snapshot), Some(1), &root, true, false, now);
    assert!(!monitor.in_progress());
    snapshot.source = IndexSource::Walker;
    monitor.tick(Some(&mut snapshot), Some(1), &root, true, true, now);
    assert!(!monitor.in_progress());
    snapshot.source = IndexSource::None;
    monitor.tick(Some(&mut snapshot), Some(1), &root, true, true, now);
    assert!(!monitor.in_progress());
    snapshot.source = IndexSource::FileList(root.join("FileList.txt"));
    monitor.tick(
        Some(&mut snapshot),
        Some(1),
        &root.join("other"),
        true,
        true,
        now,
    );
    assert!(!monitor.in_progress());
    snapshot.last_checked_at = now;
    monitor.tick(
        Some(&mut snapshot),
        Some(1),
        &root,
        true,
        true,
        now + Duration::from_secs(4),
    );
    assert!(!monitor.in_progress());
    monitor.tick(
        Some(&mut snapshot),
        Some(1),
        &root,
        true,
        true,
        now + Duration::from_secs(5),
    );
    assert!(monitor.in_progress());
    settle_monitor(
        &mut monitor,
        &mut snapshot,
        1,
        &root,
        now + Duration::from_secs(5),
    );
    assert_eq!(snapshot.change, FileListChange::Unchanged);
    monitor.disconnect();
    handle.join().unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn freshness_monitor_latches_deletion_or_replacement_without_changing_acquired_time() {
    for replace in [false, true] {
        let (root, mut snapshot) = monitor_fixture(if replace {
            "freshness-replaced"
        } else {
            "freshness-deleted"
        });
        let path = root.join("FileList.txt");
        let acquired_at = snapshot.acquired_at;
        if replace {
            let replacement = root.join("replacement");
            fs::write(&replacement, "alpha.txt\n").unwrap();
            fs::remove_file(&path).unwrap();
            fs::rename(replacement, &path).unwrap();
        } else {
            fs::remove_file(&path).unwrap();
        }
        let (mut monitor, handle) = FreshnessMonitor::new(Arc::new(AtomicBool::new(false)));
        let now = Instant::now();
        monitor.tick(Some(&mut snapshot), Some(1), &root, true, true, now);
        settle_monitor(&mut monitor, &mut snapshot, 1, &root, now);
        assert_eq!(snapshot.change, FileListChange::Changed);
        assert_eq!(snapshot.acquired_at, acquired_at);
        monitor.tick(
            Some(&mut snapshot),
            Some(1),
            &root,
            true,
            true,
            now + Duration::from_secs(10),
        );
        assert!(
            !monitor.in_progress(),
            "Changed is latched until a new snapshot"
        );
        monitor.disconnect();
        handle.join().unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn freshness_monitor_unavailable_retries_but_unknown_baseline_cannot_become_unchanged() {
    let (root, mut snapshot) = monitor_fixture("freshness-unavailable-retry");
    let baseline = snapshot.baseline.clone();
    let mut first = true;
    let (mut monitor, handle) =
        FreshnessMonitor::spawn_with(Arc::new(AtomicBool::new(false)), move |_| {
            if std::mem::take(&mut first) {
                FileObservation::Unavailable
            } else {
                baseline.clone()
            }
        });
    let now = Instant::now();
    monitor.tick(Some(&mut snapshot), Some(1), &root, true, true, now);
    settle_monitor(&mut monitor, &mut snapshot, 1, &root, now);
    assert_eq!(snapshot.change, FileListChange::Unavailable);
    assert!(!snapshot.monitor_suspended);
    monitor.tick(
        Some(&mut snapshot),
        Some(1),
        &root,
        true,
        true,
        now + Duration::from_secs(5),
    );
    settle_monitor(
        &mut monitor,
        &mut snapshot,
        1,
        &root,
        now + Duration::from_secs(5),
    );
    assert_eq!(snapshot.change, FileListChange::Unchanged);
    snapshot.baseline = FileObservation::Unavailable;
    monitor.tick(
        Some(&mut snapshot),
        Some(1),
        &root,
        true,
        true,
        now + Duration::from_secs(10),
    );
    settle_monitor(
        &mut monitor,
        &mut snapshot,
        1,
        &root,
        now + Duration::from_secs(10),
    );
    assert_eq!(snapshot.change, FileListChange::Unavailable);
    monitor.disconnect();
    handle.join().unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn freshness_monitor_rejects_old_tab_root_generation_path_and_inactive_results() {
    for mismatch in [
        "tab",
        "root",
        "generation",
        "path",
        "foreground",
        "disabled",
        "no_snapshot",
    ] {
        let (root, mut snapshot) = monitor_fixture(&format!("freshness-stale-{mismatch}"));
        let (release_tx, release_rx) = mpsc::channel();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (mut monitor, handle) =
            FreshnessMonitor::spawn_with(Arc::new(AtomicBool::new(false)), move |_| {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                FileObservation::Missing
            });
        let now = Instant::now();
        monitor.tick(Some(&mut snapshot), Some(1), &root, true, true, now);
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let tab_id = if mismatch == "tab" { 2 } else { 1 };
        let active_root = if mismatch == "root" {
            root.join("other")
        } else {
            root.clone()
        };
        if mismatch == "generation" {
            snapshot.request_id += 1;
        }
        if mismatch == "path" {
            snapshot.source = IndexSource::FileList(root.join("filelist.txt"));
        }
        release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while monitor.in_progress() {
            let view = if mismatch == "no_snapshot" {
                None
            } else {
                Some(&mut snapshot)
            };
            monitor.tick(
                view,
                Some(tab_id),
                &active_root,
                mismatch != "disabled",
                mismatch != "foreground",
                now,
            );
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        assert_eq!(snapshot.change, FileListChange::Unchanged, "{mismatch}");
        monitor.disconnect();
        handle.join().unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn freshness_monitor_timeout_keeps_physical_slot_busy_and_discards_late_response() {
    let (root, mut snapshot) = monitor_fixture("freshness-monitor-timeout");
    let (release_tx, release_rx) = mpsc::channel();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (mut monitor, handle) =
        FreshnessMonitor::spawn_with(Arc::new(AtomicBool::new(false)), move |_| {
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            FileObservation::Missing
        });
    let now = Instant::now();
    monitor.tick(Some(&mut snapshot), Some(1), &root, true, true, now);
    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    monitor.tick(
        Some(&mut snapshot),
        Some(1),
        &root,
        true,
        true,
        now + Duration::from_secs(2),
    );
    assert_eq!(snapshot.change, FileListChange::Unavailable);
    assert!(snapshot.monitor_suspended);
    assert!(
        monitor.in_progress(),
        "logical timeout must not free physical capacity"
    );
    let mut next = snapshot.clone();
    next.request_id += 1;
    next.monitor_suspended = false;
    next.change = FileListChange::Unchanged;
    next.last_checked_at = now - Duration::from_secs(6);
    monitor.tick(
        Some(&mut next),
        Some(2),
        &root,
        true,
        true,
        now + Duration::from_secs(10),
    );
    assert!(
        entered_rx.try_recv().is_err(),
        "no second physical probe may queue"
    );
    release_tx.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    // Disable new scheduling while draining the late response.
    while monitor.in_progress() {
        monitor.tick(
            Some(&mut snapshot),
            Some(1),
            &root,
            false,
            true,
            now + Duration::from_secs(10),
        );
        assert!(Instant::now() < deadline);
        thread::yield_now();
    }
    assert_eq!(snapshot.change, FileListChange::Unavailable);
    monitor.tick(
        Some(&mut snapshot),
        Some(1),
        &root,
        true,
        true,
        now + Duration::from_secs(20),
    );
    assert!(
        !monitor.in_progress(),
        "timed-out snapshot remains suspended"
    );
    monitor.disconnect();
    handle.join().unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn freshness_monitor_ui_tick_does_not_schedule_or_apply_while_index_refreshes() {
    let (mut app, root) = indexed_filelist_app("freshness-refresh-gate");
    app.filelist_auto_check_enabled = true;
    let (release_tx, release_rx) = mpsc::channel();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (monitor, handle) =
        FreshnessMonitor::spawn_with(Arc::new(AtomicBool::new(false)), move |_| {
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            FileObservation::Missing
        });
    app.freshness_monitor = monitor;
    app.shell
        .runtime
        .snapshot_freshness_mut()
        .unwrap()
        .last_checked_at = Instant::now() - Duration::from_secs(6);
    let acquired_at = app.shell.runtime.freshness.as_ref().unwrap().acquired_at;
    let ctx = egui::Context::default();
    app.shell.indexing.in_progress = true;
    let _ = ctx.run_ui(
        egui::RawInput {
            focused: true,
            ..Default::default()
        },
        |_| app.tick_freshness(&ctx),
    );
    assert!(!app.freshness_monitor.in_progress());
    assert!(entered_rx.try_recv().is_err());
    app.shell.indexing.in_progress = false;
    let _ = ctx.run_ui(
        egui::RawInput {
            focused: true,
            ..Default::default()
        },
        |_| app.tick_freshness(&ctx),
    );
    assert!(app.freshness_monitor.in_progress(),
        "eligible UI frame must schedule: focused={}, tab={:?}, root={:?}, pending_finish={}, snapshot={:?}",
        ctx.input(|input| input.focused), app.current_tab_id(), app.shell.runtime.root,
        app.shell.indexing.pending_finish.is_some(), app.shell.runtime.freshness);
    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    app.shell.indexing.in_progress = true;
    release_tx.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while app.freshness_monitor.in_progress() {
        let _ = ctx.run_ui(
            egui::RawInput {
                focused: true,
                ..Default::default()
            },
            |_| app.tick_freshness(&ctx),
        );
        assert!(Instant::now() < deadline);
        thread::yield_now();
    }
    assert_eq!(
        app.shell.runtime.freshness.as_ref().unwrap().change,
        FileListChange::Unchanged
    );
    assert_eq!(
        app.shell.runtime.freshness.as_ref().unwrap().acquired_at,
        acquired_at
    );
    app.shell.indexing.in_progress = false;
    app.freshness_monitor.disconnect();
    handle.join().unwrap();
    drop(app);
    fs::remove_dir_all(root).unwrap();
}

//! Freshness describes the acquired snapshot, never a claim that the tree is current.
use super::{FlistWalkerApp, IndexSource, TabResourceLifecycle};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

pub(super) mod worker;
pub(super) use worker::FreshnessMonitor;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FileFingerprint {
    size: u64,
    modified: SystemTime,
    #[cfg(unix)]
    identity: (u64, u64),
    #[cfg(windows)]
    identity: (u32, u64),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum FileObservation {
    Present(FileFingerprint),
    Missing,
    Unavailable,
}

pub(super) fn observe_file(path: &Path) -> FileObservation {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return FileObservation::Missing;
        }
        Err(_) => return FileObservation::Unavailable,
    };
    let Ok(metadata) = file.metadata() else {
        return FileObservation::Unavailable;
    };
    if !metadata.is_file() {
        return FileObservation::Unavailable;
    }
    let Ok(modified) = metadata.modified() else {
        return FileObservation::Unavailable;
    };
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;
    #[cfg(windows)]
    let Some(identity) = crate::ui_model::windows_file_identity(&file) else {
        return FileObservation::Unavailable;
    };
    FileObservation::Present(FileFingerprint {
        size: metadata.len(),
        modified,
        #[cfg(unix)]
        identity: (metadata.dev(), metadata.ino()),
        #[cfg(windows)]
        identity,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FileListChange {
    Unchanged,
    Changed,
    Unavailable,
}

impl FileListChange {
    pub(super) fn compare(baseline: &FileObservation, current: &FileObservation) -> Self {
        match (baseline, current) {
            (FileObservation::Present(left), FileObservation::Present(right)) => {
                if left == right {
                    Self::Unchanged
                } else {
                    Self::Changed
                }
            }
            (FileObservation::Present(_), FileObservation::Missing) => Self::Changed,
            _ => Self::Unavailable,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct SnapshotFreshness {
    pub(super) request_id: u64,
    pub(super) root: PathBuf,
    pub(super) source: IndexSource,
    pub(super) acquired_at: SystemTime,
    pub(super) baseline: FileObservation,
    pub(super) change: FileListChange,
    pub(super) last_checked_at: Instant,
    pub(super) monitor_suspended: bool,
}

impl SnapshotFreshness {
    pub(super) fn acquired(
        request_id: u64,
        root: PathBuf,
        source: IndexSource,
        baseline: FileObservation,
        current: FileObservation,
    ) -> Self {
        let change = FileListChange::compare(&baseline, &current);
        Self {
            request_id,
            root,
            source,
            acquired_at: SystemTime::now(),
            baseline,
            change,
            last_checked_at: Instant::now(),
            monitor_suspended: false,
        }
    }
}

fn age_label(age: Duration) -> String {
    if age.as_secs() < 60 {
        "just now".into()
    } else if age.as_secs() < 3600 {
        format!("{} min ago", age.as_secs() / 60)
    } else if age.as_secs() < 86_400 {
        format!("{} h ago", age.as_secs() / 3600)
    } else {
        format!("{} d ago", age.as_secs() / 86_400)
    }
}

impl FlistWalkerApp {
    pub(super) fn tick_freshness(&mut self, ctx: &eframe::egui::Context) {
        let tab_id = self.current_tab_id();
        let root = self.shell.runtime.root.clone();
        let foreground = ctx.input(|input| input.focused);
        let can_observe = foreground
            && !self.shell.indexing.in_progress
            && self.shell.indexing.pending_finish.is_none();
        if self.filelist_auto_check_enabled && can_observe {
            if let Some(snapshot) = self.shell.runtime.snapshot_freshness_mut() {
                if snapshot.monitor_suspended && snapshot.change != FileListChange::Changed {
                    snapshot.change = FileListChange::Unavailable;
                }
            }
        }
        let tick = self.freshness_monitor.tick(
            self.shell.runtime.snapshot_freshness_mut(),
            tab_id,
            &root,
            self.filelist_auto_check_enabled,
            can_observe,
            Instant::now(),
        );
        if let Some(timeout) = tick.timeout {
            // Logical timeout belongs to the originating snapshot even if its
            // tab was deactivated while the physical probe remained blocked.
            if Some(timeout.tab_id) == tab_id {
                if let Some(snapshot) = self.shell.runtime.snapshot_freshness_mut() {
                    let previous_change = snapshot.change;
                    timeout.suspend(snapshot, tab_id, &root);
                    if !self.filelist_auto_check_enabled || !can_observe {
                        snapshot.change = previous_change;
                    }
                }
            } else if let Some(index) = self.find_tab_index_by_id(timeout.tab_id) {
                if let Some(tab) = self.shell.tabs.get_mut(index) {
                    if let Some(snapshot) = tab.result_state.committed.freshness.as_mut() {
                        let previous_change = snapshot.change;
                        timeout.suspend(snapshot, Some(tab.id), &tab.root);
                        snapshot.change = previous_change;
                    }
                }
            }
        }
        if let Some(delay) = tick.repaint_after {
            ctx.request_repaint_after(delay);
        }
    }
    pub(super) fn acquired_index_snapshot(
        &self,
        request_id: u64,
        source: IndexSource,
        root: PathBuf,
    ) -> SnapshotFreshness {
        self.shell
            .indexing
            .response_mailboxes
            .lock()
            .ok()
            .and_then(|mailboxes| {
                mailboxes
                    .get(&request_id)
                    .and_then(|mailbox| mailbox.snapshot())
            })
            .unwrap_or_else(|| {
                SnapshotFreshness::acquired(
                    request_id,
                    root,
                    source,
                    FileObservation::Unavailable,
                    FileObservation::Unavailable,
                )
            })
    }

    pub(super) fn freshness_source_text(&self) -> String {
        let Some(snapshot) = self.shell.runtime.freshness.as_ref() else {
            return "Source: None · Not loaded".into();
        };
        let age = age_label(snapshot.acquired_at.elapsed().unwrap_or_default());
        let text = match &snapshot.source {
            IndexSource::FileList(path) => format!(
                "Source: FileList ({}) · Loaded {age}",
                path.file_name().unwrap_or_default().to_string_lossy()
            ),
            IndexSource::Walker => format!("Source: Walker · Indexed {age}"),
            IndexSource::None => format!("Source: None · Acquired {age}"),
        };
        match self.shell.indexing.lifecycle() {
            TabResourceLifecycle::Refreshing => {
                format!("{text} · Refreshing… · Last successful acquisition")
            }
            TabResourceLifecycle::Failed => {
                format!("{text} · Refresh failed · Last successful acquisition")
            }
            _ => text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::test_root;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    fn check_isolated_filelist_change(component: &str) {
        let root = test_root(&format!("freshness-fingerprint-{component}"));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("FileList.txt");
        let modified = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        let set_modified = |path: &Path, time| {
            File::options()
                .write(true)
                .open(path)
                .unwrap()
                .set_times(std::fs::FileTimes::new().set_modified(time))
                .unwrap();
        };
        std::fs::write(&path, "alpha.txt\n").unwrap();
        set_modified(&path, modified);
        let baseline = observe_file(&path);
        let FileObservation::Present(before) = &baseline else {
            panic!("fixture fingerprint must be available");
        };
        match component {
            "size" => {
                std::fs::write(&path, "alpha.txt\nbeta.txt\n").unwrap();
                set_modified(&path, before.modified);
            }
            "mtime" => {
                std::fs::write(&path, "bravo.txt\n").unwrap();
                set_modified(&path, modified + Duration::from_secs(10));
            }
            "identity" => {
                let replacement = root.join("replacement");
                std::fs::write(&replacement, "alpha.txt\n").unwrap();
                set_modified(&replacement, before.modified);
                std::fs::remove_file(&path).unwrap();
                std::fs::rename(replacement, &path).unwrap();
            }
            _ => unreachable!(),
        }
        let FileObservation::Present(after) = observe_file(&path) else {
            panic!("changed fixture fingerprint must be available");
        };
        assert_eq!(
            before.size == after.size,
            component != "size",
            "{component}"
        );
        assert_eq!(
            before.modified == after.modified,
            component != "mtime",
            "{component}"
        );
        #[cfg(any(unix, windows))]
        assert_eq!(
            before.identity == after.identity,
            component != "identity",
            "{component}"
        );

        let mut snapshot = SnapshotFreshness::acquired(
            1,
            root.clone(),
            IndexSource::FileList(path),
            baseline.clone(),
            baseline,
        );
        let acquired_at = snapshot.acquired_at;
        let now = Instant::now();
        snapshot.last_checked_at = now - Duration::from_secs(6);
        let (mut monitor, handle) = FreshnessMonitor::new(Arc::new(AtomicBool::new(false)));
        monitor.tick(Some(&mut snapshot), Some(1), &root, true, true, now);
        assert!(monitor.in_progress());
        let deadline = Instant::now() + Duration::from_secs(2);
        while monitor.in_progress() {
            monitor.tick(Some(&mut snapshot), Some(1), &root, true, true, now);
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert_eq!(snapshot.change, FileListChange::Changed, "{component}");
        assert_eq!(snapshot.acquired_at, acquired_at);
        monitor.disconnect();
        handle.join().unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn freshness_monitor_detects_size_only_in_place_update() {
        check_isolated_filelist_change("size");
    }

    #[test]
    fn freshness_monitor_detects_mtime_only_in_place_update() {
        check_isolated_filelist_change("mtime");
    }

    #[test]
    fn freshness_monitor_detects_identity_only_replacement() {
        check_isolated_filelist_change("identity");
    }
}

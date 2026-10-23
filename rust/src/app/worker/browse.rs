//! Filesystem preparation for the UI-owned native folder picker.
use crate::path_utils::normalize_windows_path_buf;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::sync::Arc;
use std::thread;

pub(in crate::app) struct BrowseService {
    tx: Option<SyncSender<(u64, PathBuf)>>,
    rx: Receiver<(u64, PathBuf)>,
    pending: Option<u64>,
    next_id: u64,
    current: Arc<AtomicU64>,
}

#[cfg(test)]
pub(in crate::app) type ControlledBrowseService = (
    BrowseService,
    Receiver<(u64, PathBuf)>,
    SyncSender<(u64, PathBuf)>,
);

pub(in crate::app) fn picker_start_location(root: &Path) -> PathBuf {
    let normalized = normalize_windows_path_buf(root.to_path_buf());
    normalized
        .ancestors()
        .find(|ancestor| ancestor.is_dir())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

impl BrowseService {
    #[cfg(test)]
    pub(in crate::app) fn controlled_for_test() -> ControlledBrowseService {
        let (tx, requests) = mpsc::sync_channel(1);
        let (responses, rx) = mpsc::sync_channel(1);
        (
            Self {
                tx: Some(tx),
                rx,
                pending: None,
                next_id: 1,
                current: Arc::new(AtomicU64::new(0)),
            },
            requests,
            responses,
        )
    }

    pub(in crate::app) fn spawn_with(
        shutdown: Arc<AtomicBool>,
        mut resolve: impl FnMut(&Path) -> PathBuf + Send + 'static,
    ) -> (Self, thread::JoinHandle<()>) {
        let (tx, requests) = mpsc::sync_channel::<(u64, PathBuf)>(1);
        let (responses, rx) = mpsc::sync_channel(1);
        let current = Arc::new(AtomicU64::new(0));
        let worker_current = Arc::clone(&current);
        let handle = thread::Builder::new()
            .name("flistwalker-browse-location".into())
            .spawn(move || {
                while let Ok((id, root)) = requests.recv() {
                    if shutdown.load(Ordering::Acquire) {
                        break;
                    }
                    let location = if worker_current.load(Ordering::Acquire) == id {
                        resolve(&root)
                    } else {
                        root
                    };
                    if shutdown.load(Ordering::Acquire)
                        || responses.try_send((id, location)).is_err()
                    {
                        break;
                    }
                }
            })
            .expect("spawn Browse location worker");
        (
            Self {
                tx: Some(tx),
                rx,
                pending: None,
                next_id: 1,
                current,
            },
            handle,
        )
    }

    pub(in crate::app) fn start(&mut self, root: PathBuf) -> Result<u64, &'static str> {
        if self.pending.is_some() {
            return Err("Folder picker preparation is already in progress");
        }
        let id = self.next_id;
        self.current.store(id, Ordering::Release);
        self.tx
            .as_ref()
            .ok_or("Folder picker worker is unavailable")?
            .try_send((id, root))
            .map_err(|_| "Folder picker worker is unavailable")?;
        self.next_id += 1;
        self.pending = Some(id);
        Ok(id)
    }

    pub(in crate::app) fn poll(&mut self) -> Option<(u64, Result<PathBuf, &'static str>)> {
        let expected = self.pending?;
        let result = match self.rx.try_recv() {
            Ok((id, location)) if id == expected => Ok(location),
            Ok(_) => Err("Folder picker response identity mismatch"),
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Err("Folder picker worker is unavailable"),
        };
        self.pending = None;
        Some((expected, result))
    }

    pub(in crate::app) fn in_progress(&self) -> bool {
        self.pending.is_some()
    }

    pub(in crate::app) fn cancel(&self) {
        self.current.store(0, Ordering::Release);
    }

    pub(in crate::app) fn disconnect(&mut self) {
        self.tx = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn browse_worker_stays_bounded_while_filesystem_probe_is_blocked() {
        let (entered, started) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let caller = thread::current().id();
        let (mut service, handle) =
            BrowseService::spawn_with(Arc::new(AtomicBool::new(false)), move |path| {
                entered.send(thread::current().id()).unwrap();
                let _ = gate.recv_timeout(Duration::from_secs(5));
                path.to_path_buf()
            });
        let id = service.start("first".into()).unwrap();
        assert_ne!(
            started.recv_timeout(Duration::from_secs(5)).unwrap(),
            caller
        );
        for _ in 0..100 {
            assert!(service.start("duplicate".into()).is_err());
        }
        assert!(service.poll().is_none());
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let (completed, location) = loop {
            if let Some(response) = service.poll() {
                break response;
            }
            assert!(Instant::now() < deadline, "probe response missing");
            thread::yield_now();
        };
        assert_eq!(completed, id);
        assert_eq!(location.unwrap(), PathBuf::from("first"));
        service.disconnect();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !handle.is_finished() {
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        handle.join().unwrap();
        assert!(service.start("after shutdown".into()).is_err());
    }

    #[test]
    fn browse_worker_shutdown_settles_without_starting_queued_io() {
        let (mut service, handle) =
            BrowseService::spawn_with(Arc::new(AtomicBool::new(true)), |_| {
                panic!("I/O after shutdown")
            });
        service.start("root".into()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !handle.is_finished() {
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        handle.join().unwrap();
        assert!(service.poll().unwrap().1.is_err());
        assert!(!service.in_progress());
    }
}

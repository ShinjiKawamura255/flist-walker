use anyhow::Result;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::sync::Arc;
use std::thread;

type Completion = std::result::Result<PathBuf, String>;

// One accepted request remains owned until its response is consumed. Both
// mailboxes are bounded, and repeat clicks never create threads or queued work.
pub(in crate::app) struct ConfigOpenService {
    tx: Option<SyncSender<()>>,
    rx: Receiver<Completion>,
    pending_tab: Option<u64>,
}

impl ConfigOpenService {
    pub(in crate::app) fn spawn_with(
        shutdown: Arc<AtomicBool>,
        mut open: impl FnMut() -> Result<PathBuf> + Send + 'static,
    ) -> (Self, thread::JoinHandle<()>) {
        let (tx, requests) = mpsc::sync_channel(1);
        let (responses, rx) = mpsc::sync_channel(1);
        let handle = thread::Builder::new()
            .name("flistwalker-config-open".into())
            .spawn(move || {
                while requests.recv().is_ok() {
                    if shutdown.load(Ordering::Acquire) {
                        break;
                    }
                    let result = open().map_err(|error| format!("{error:#}"));
                    if responses.try_send(result).is_err() {
                        break;
                    }
                }
            })
            .expect("spawn config open worker");
        (
            Self {
                tx: Some(tx),
                rx,
                pending_tab: None,
            },
            handle,
        )
    }

    pub(in crate::app) fn start(&mut self, tab_id: u64) -> std::result::Result<(), &'static str> {
        if self.pending_tab.is_some() {
            return Err("Config file is already opening");
        }
        self.tx
            .as_ref()
            .ok_or("Config worker is unavailable")?
            .try_send(())
            .map_err(|_| "Config worker is unavailable")?;
        self.pending_tab = Some(tab_id);
        Ok(())
    }

    pub(in crate::app) fn in_progress(&self) -> bool {
        self.pending_tab.is_some()
    }

    pub(in crate::app) fn poll(&mut self) -> Option<(u64, Completion)> {
        let tab = self.pending_tab?;
        let result = match self.rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Err("Config worker is unavailable".into()),
        };
        self.pending_tab = None;
        Some((tab, result))
    }

    pub(in crate::app) fn disconnect(&mut self) {
        self.tx = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    #[test]
    fn config_open_dispatch_is_nonblocking_and_duplicate_requests_are_suppressed() {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let shutdown = Arc::new(AtomicBool::new(false));
        let caller = thread::current().id();
        let (mut service, handle) = ConfigOpenService::spawn_with(shutdown, move || {
            entered_tx.send(thread::current().id()).unwrap();
            release_rx.recv().unwrap();
            Ok(PathBuf::from("config.toml"))
        });
        let start = Instant::now();
        assert!(service.start(7).is_ok());
        assert!(start.elapsed() < Duration::from_millis(100));
        assert_ne!(
            entered_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            caller
        );
        assert!(service.start(8).is_err());
        assert!(service.poll().is_none());
        release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        let completion = loop {
            if let Some(completion) = service.poll() {
                break completion;
            }
            assert!(Instant::now() < deadline);
            thread::yield_now();
        };
        assert_eq!(completion.0, 7);
        assert_eq!(completion.1.unwrap(), PathBuf::from("config.toml"));
        assert!(!service.in_progress());
        service.disconnect();
        handle.join().unwrap();
        assert!(service.start(9).is_err());
    }

    #[test]
    fn config_open_failure_settles_and_shutdown_skips_queued_io() {
        let shutdown = Arc::new(AtomicBool::new(true));
        let (mut service, handle) = ConfigOpenService::spawn_with(shutdown, || {
            panic!("shutdown must reject I/O before execution")
        });
        service.start(1).unwrap();
        handle.join().unwrap();
        let (_, result) = service
            .poll()
            .expect("disconnected settles pending request");
        assert!(result.is_err());
        assert!(!service.in_progress());
    }
}

#[cfg(test)]
mod error_tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn config_open_error_allows_retry_with_new_owner() {
        let mut calls = 0;
        let (mut service, handle) =
            ConfigOpenService::spawn_with(Arc::new(AtomicBool::new(false)), move || {
                calls += 1;
                if calls == 1 {
                    anyhow::bail!("editor unavailable");
                }
                Ok("config.json".into())
            });
        for owner in [11, 12] {
            service.start(owner).unwrap();
            let deadline = Instant::now() + Duration::from_secs(1);
            let (completed_owner, result) = loop {
                if let Some(result) = service.poll() {
                    break result;
                }
                assert!(Instant::now() < deadline);
                thread::yield_now();
            };
            assert_eq!(completed_owner, owner);
            assert_eq!(result.is_ok(), owner == 12);
        }
        service.disconnect();
        handle.join().unwrap();
    }
}

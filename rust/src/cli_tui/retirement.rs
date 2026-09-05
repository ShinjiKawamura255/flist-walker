use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

const CAPACITY: usize = 4;
const POLL: Duration = Duration::from_millis(10);

trait Retained: Send {
    fn released(&self) -> bool;
}
struct RetainedArc<T: Send + Sync>(Arc<T>);
impl<T: Send + Sync> Retained for RetainedArc<T> {
    fn released(&self) -> bool {
        Arc::strong_count(&self.0) == 1
    }
}

#[derive(Clone)]
pub(super) struct RetirementSender(mpsc::SyncSender<Box<dyn Retained>>);
pub(super) struct RetirementWorker {
    pub(super) sender: RetirementSender,
    done: mpsc::Receiver<()>,
    handle: thread::JoinHandle<()>,
}
impl RetirementWorker {
    pub(super) fn start() -> std::io::Result<Self> {
        let (tx, rx) = mpsc::sync_channel::<Box<dyn Retained>>(CAPACITY);
        let (done_tx, done) = mpsc::channel();
        let handle = thread::Builder::new()
            .name("flistwalker-cli-retirement".into())
            .spawn(move || {
                let mut retained = Vec::<Box<dyn Retained>>::new();
                let mut disconnected = false;
                loop {
                    retained.retain(|payload| !payload.released());
                    if disconnected && retained.is_empty() {
                        break;
                    }
                    if retained.len() < CAPACITY && !disconnected {
                        match rx.recv_timeout(POLL) {
                            Ok(payload) => retained.push(payload),
                            Err(mpsc::RecvTimeoutError::Disconnected) => disconnected = true,
                            Err(mpsc::RecvTimeoutError::Timeout) => {}
                        }
                    } else {
                        thread::sleep(POLL);
                    }
                }
                let _ = done_tx.send(());
            })?;
        Ok(Self {
            sender: RetirementSender(tx),
            done,
            handle,
        })
    }
    pub(super) fn shutdown(self) {
        drop(self.sender);
        super::workers::finish_worker(self.handle, self.done);
    }
}
impl RetirementSender {
    // Called only by producers. No UI-visible payload is published without a guard.
    pub(super) fn retain<T: Send + Sync + 'static>(
        &self,
        payload: &Arc<T>,
        cancelled: impl Fn() -> bool,
    ) -> bool {
        let mut guard: Box<dyn Retained> = Box::new(RetainedArc(Arc::clone(payload)));
        loop {
            if cancelled() {
                return false;
            }
            match self.0.try_send(guard) {
                Ok(()) => return true,
                Err(mpsc::TrySendError::Full(returned)) => {
                    guard = returned;
                    thread::sleep(POLL);
                }
                Err(mpsc::TrySendError::Disconnected(_)) => return false,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    struct Probe(mpsc::Sender<thread::ThreadId>);
    impl Drop for Probe {
        fn drop(&mut self) {
            let _ = self.0.send(thread::current().id());
        }
    }
    #[test]
    fn alignment_retirement_drops_final_payload_off_consumer_thread() {
        let worker = RetirementWorker::start().unwrap();
        let (tx, rx) = mpsc::channel();
        let payload = Arc::new(Probe(tx));
        assert!(worker.sender.retain(&payload, || false));
        drop(payload);
        assert_ne!(
            rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            thread::current().id()
        );
        worker.shutdown();
    }
    #[test]
    fn alignment_retirement_full_queue_cancels_producer_without_leaking_guards() {
        let worker = RetirementWorker::start().unwrap();
        let (tx, rx) = mpsc::channel();
        let held = (0..CAPACITY * 2)
            .map(|_| Arc::new(Probe(tx.clone())))
            .collect::<Vec<_>>();
        for payload in &held {
            assert!(worker.sender.retain(payload, || false));
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let sender = worker.sender.clone();
        let producer_cancel = Arc::clone(&cancelled);
        let producer = thread::spawn(move || {
            let payload = Arc::new(Probe(tx));
            sender.retain(&payload, || producer_cancel.load(Ordering::Acquire))
        });
        cancelled.store(true, Ordering::Release);
        assert!(!producer.join().unwrap());
        drop(held);
        worker.shutdown();
        assert_eq!(rx.try_iter().count(), CAPACITY * 2 + 1);
    }

    #[test]
    fn alignment_retirement_disconnected_rejects_publication_and_keeps_producer_owner() {
        let (tx, rx) = mpsc::sync_channel::<Box<dyn Retained>>(1);
        drop(rx);
        let sender = RetirementSender(tx);
        let (drop_tx, drop_rx) = mpsc::channel();
        let payload = Arc::new(Probe(drop_tx));
        assert!(!sender.retain(&payload, || false));
        assert_eq!(Arc::strong_count(&payload), 1);
        assert!(drop_rx.try_recv().is_err());
        drop(payload);
        assert_eq!(drop_rx.recv().unwrap(), thread::current().id());
    }
}

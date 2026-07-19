use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvError, SyncSender, TrySendError};
#[cfg(test)]
use std::sync::mpsc::{SendError, TryRecvError};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct WorkerLoadSnapshot {
    pub(super) queued: usize,
    pub(super) inflight: usize,
    pub(super) capacity: usize,
}

pub(super) struct WorkerTraceContext<'a> {
    pub(super) worker_id: &'a str,
    pub(super) request_id: Option<u64>,
    pub(super) tab_id: Option<u64>,
    pub(super) epoch: Option<u64>,
    pub(super) outcome: &'static str,
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct WorkerTraceRecord<'a> {
    pub(super) worker_family: &'static str,
    pub(super) event: &'static str,
    pub(super) worker_id: &'a str,
    pub(super) request_id: Option<u64>,
    pub(super) tab_id: Option<u64>,
    pub(super) epoch: Option<u64>,
    pub(super) outcome: &'static str,
    pub(super) queue_depth: usize,
    pub(super) in_flight: usize,
    pub(super) capacity: usize,
}

struct WorkerLoad {
    queued: AtomicUsize,
    inflight: AtomicUsize,
    capacity: usize,
}

#[derive(Clone)]
pub(super) struct WorkerLoadObserver {
    load: Arc<WorkerLoad>,
}

impl WorkerLoadObserver {
    pub(super) fn load(&self) -> WorkerLoadSnapshot {
        WorkerLoadSnapshot {
            queued: self.load.queued.load(Ordering::Acquire),
            inflight: self.load.inflight.load(Ordering::Acquire),
            capacity: self.load.capacity,
        }
    }
}

fn decrement_saturating(counter: &AtomicUsize) {
    let _ = counter.fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
        Some(value.saturating_sub(1))
    });
}

pub(super) struct BoundedSender<T> {
    inner: SyncSender<T>,
    load: Arc<WorkerLoad>,
}

impl<T> Clone for BoundedSender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            load: Arc::clone(&self.load),
        }
    }
}

impl<T> BoundedSender<T> {
    #[cfg(test)]
    pub(super) fn send(&self, value: T) -> Result<(), SendError<T>> {
        self.load.queued.fetch_add(1, Ordering::AcqRel);
        self.inner.send(value).inspect_err(|_| {
            decrement_saturating(&self.load.queued);
        })
    }

    pub(super) fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        self.load.queued.fetch_add(1, Ordering::AcqRel);
        self.inner.try_send(value).inspect_err(|_| {
            decrement_saturating(&self.load.queued);
        })
    }

    pub(super) fn load(&self) -> WorkerLoadSnapshot {
        WorkerLoadSnapshot {
            queued: self.load.queued.load(Ordering::Acquire),
            inflight: self.load.inflight.load(Ordering::Acquire),
            capacity: self.load.capacity,
        }
    }

    pub(super) fn load_observer(&self) -> WorkerLoadObserver {
        WorkerLoadObserver {
            load: Arc::clone(&self.load),
        }
    }
}

pub(super) struct BoundedReceiver<T> {
    inner: Receiver<T>,
    load: Arc<WorkerLoad>,
}

impl<T> BoundedReceiver<T> {
    pub(super) fn recv(&self) -> Result<T, RecvError> {
        let value = self.inner.recv()?;
        decrement_saturating(&self.load.queued);
        Ok(value)
    }

    #[cfg(test)]
    pub(super) fn try_recv(&self) -> Result<T, TryRecvError> {
        let value = self.inner.try_recv()?;
        decrement_saturating(&self.load.queued);
        Ok(value)
    }

    pub(super) fn recv_tracked(&self) -> Result<(T, InflightGuard), RecvError> {
        let value = self.recv()?;
        self.load.inflight.fetch_add(1, Ordering::AcqRel);
        Ok((
            value,
            InflightGuard {
                load: Arc::clone(&self.load),
            },
        ))
    }
}

impl<T> Drop for BoundedReceiver<T> {
    fn drop(&mut self) {
        self.load.queued.store(0, Ordering::Release);
    }
}

pub(super) struct InflightGuard {
    load: Arc<WorkerLoad>,
}

impl Drop for InflightGuard {
    fn drop(&mut self) {
        decrement_saturating(&self.load.inflight);
    }
}

impl InflightGuard {
    pub(super) fn load(&self) -> WorkerLoadSnapshot {
        WorkerLoadSnapshot {
            queued: self.load.queued.load(Ordering::Acquire),
            inflight: self.load.inflight.load(Ordering::Acquire),
            capacity: self.load.capacity,
        }
    }
}

pub(super) fn bounded_request_channel<T>(
    capacity: usize,
) -> (BoundedSender<T>, BoundedReceiver<T>) {
    let (tx, rx) = mpsc::sync_channel(capacity);
    let load = Arc::new(WorkerLoad {
        queued: AtomicUsize::new(0),
        inflight: AtomicUsize::new(0),
        capacity,
    });
    (
        BoundedSender {
            inner: tx,
            load: Arc::clone(&load),
        },
        BoundedReceiver { inner: rx, load },
    )
}

pub(super) fn trace_worker_load<T>(
    sender: &BoundedSender<T>,
    flow: &'static str,
    event: &'static str,
    context: WorkerTraceContext<'_>,
) {
    trace_worker_snapshot(sender.load(), flow, event, context);
}

pub(super) fn trace_worker_snapshot(
    load: WorkerLoadSnapshot,
    flow: &'static str,
    event: &'static str,
    context: WorkerTraceContext<'_>,
) {
    let record = worker_trace_record(load, flow, event, context);
    tracing::debug!(
        flow = record.worker_family,
        worker_family = record.worker_family,
        event = record.event,
        worker_id = record.worker_id,
        request_id = record.request_id.unwrap_or_default(),
        request_id_present = record.request_id.is_some(),
        tab_id = record.tab_id.unwrap_or_default(),
        tab_id_present = record.tab_id.is_some(),
        epoch = record.epoch.unwrap_or_default(),
        epoch_present = record.epoch.is_some(),
        outcome = record.outcome,
        queue_depth = record.queue_depth,
        in_flight = record.in_flight,
        capacity = record.capacity,
        "bounded worker load"
    );
}

pub(super) fn worker_trace_record<'a>(
    load: WorkerLoadSnapshot,
    worker_family: &'static str,
    event: &'static str,
    context: WorkerTraceContext<'a>,
) -> WorkerTraceRecord<'a> {
    WorkerTraceRecord {
        worker_family,
        event,
        worker_id: context.worker_id,
        request_id: context.request_id,
        tab_id: context.tab_id,
        epoch: context.epoch,
        outcome: context.outcome,
        queue_depth: load.queued,
        in_flight: load.inflight,
        capacity: load.capacity,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tc_153_load_counters_settle_after_full_receive_and_disconnect() {
        let (tx, rx) = bounded_request_channel(1);
        tx.send(1).expect("fill queue");
        assert!(matches!(tx.try_send(2), Err(TrySendError::Full(2))));
        assert_eq!(
            tx.load(),
            WorkerLoadSnapshot {
                queued: 1,
                inflight: 0,
                capacity: 1,
            }
        );

        let (value, guard) = rx.recv_tracked().expect("receive tracked value");
        assert_eq!(value, 1);
        assert_eq!(tx.load().queued, 0);
        assert_eq!(tx.load().inflight, 1);
        drop(guard);
        assert_eq!(tx.load().inflight, 0);

        drop(rx);
        assert!(matches!(tx.try_send(3), Err(TrySendError::Disconnected(3))));
        assert_eq!(tx.load().queued, 0);
        assert_eq!(tx.load().inflight, 0);
    }

    #[test]
    fn tc_153_inflight_guard_cleans_up_during_unwind() {
        let (tx, rx) = bounded_request_channel(1);
        tx.send(1).expect("send value");
        let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let (_value, _guard) = rx.recv_tracked().expect("receive tracked value");
            panic!("exercise unwind cleanup");
        }));
        assert!(unwind.is_err());
        assert_eq!(tx.load().queued, 0);
        assert_eq!(tx.load().inflight, 0);
    }

    #[test]
    fn tc_153_disconnect_race_cannot_underflow_queued_load() {
        let (tx, rx) = bounded_request_channel(1);
        let sender = tx.clone();
        let (started_tx, started_rx) = mpsc::channel();
        let handle = std::thread::spawn(move || {
            sender.send(1).expect("fill queue");
            started_tx.send(()).expect("signal queue full");
            loop {
                match sender.try_send(2) {
                    Err(TrySendError::Full(_)) => std::thread::yield_now(),
                    Err(TrySendError::Disconnected(_)) => break,
                    Ok(()) => panic!("queue must remain full until disconnect"),
                }
            }
        });
        started_rx.recv().expect("queue filled");
        drop(rx);
        handle.join().expect("join racing sender");
        assert_eq!(tx.load().queued, 0);
        assert_eq!(tx.load().inflight, 0);
    }

    #[test]
    fn tc_153_trace_record_preserves_correlation_load_and_terminal_outcome() {
        let record = worker_trace_record(
            WorkerLoadSnapshot {
                queued: 3,
                inflight: 2,
                capacity: 8,
            },
            "action",
            "terminal",
            WorkerTraceContext {
                worker_id: "flistwalker-action-1",
                request_id: Some(41),
                tab_id: Some(7),
                epoch: None,
                outcome: "failed",
            },
        );
        assert_eq!(record.worker_family, "action");
        assert_eq!(record.event, "terminal");
        assert_eq!(record.worker_id, "flistwalker-action-1");
        assert_eq!(record.request_id, Some(41));
        assert_eq!(record.tab_id, Some(7));
        assert_eq!(record.epoch, None);
        assert_eq!(record.outcome, "failed");
        assert_eq!(record.queue_depth, 3);
        assert_eq!(record.in_flight, 2);
        assert_eq!(record.capacity, 8);
    }

    #[test]
    fn tc_153_load_observer_reports_timeout_point_residual_without_holding_sender_open() {
        let (tx, rx) = bounded_request_channel(1);
        let observer = tx.load_observer();
        tx.send(1).expect("send observed request");
        let (_request, guard) = rx.recv_tracked().expect("receive observed request");
        drop(tx);

        let record = worker_trace_record(
            observer.load(),
            "index",
            "shutdown_timeout",
            WorkerTraceContext {
                worker_id: "index-0",
                request_id: None,
                tab_id: None,
                epoch: None,
                outcome: "shutdown_timeout",
            },
        );
        assert_eq!(record.queue_depth, 0);
        assert_eq!(record.in_flight, 1);
        assert_eq!(record.outcome, "shutdown_timeout");

        drop(guard);
        assert_eq!(observer.load().inflight, 0);
        assert!(
            rx.recv().is_err(),
            "observer must not keep request sender open"
        );
    }
}

//! Test-only opt-in trace. No paths, document contents or new worker waits.
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicU64, AtomicU8, AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::Instant;
const CAPACITY: usize = 96;
#[derive(Clone, Serialize)]
struct Event {
    at_ns: u64,
    event: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    io_error_kind: Option<String>,
}
pub(super) struct FlushDiagnostic {
    label: &'static str,
    origin: Instant,
    events: Mutex<Vec<Event>>,
    dropped: AtomicUsize,
    receive_begin: AtomicU64,
    receive_end: AtomicU64,
    receive_outcome: AtomicU8,
}
impl FlushDiagnostic {
    pub(super) fn new(label: &'static str) -> Self {
        Self {
            label,
            origin: Instant::now(),
            events: Mutex::new(Vec::with_capacity(CAPACITY)),
            dropped: AtomicUsize::new(0),
            receive_begin: AtomicU64::new(0),
            receive_end: AtomicU64::new(0),
            receive_outcome: AtomicU8::new(0),
        }
    }
    fn push(&self, event: &'static str, generation: Option<u64>, io_error_kind: Option<String>) {
        let Ok(at_ns) = u64::try_from(self.origin.elapsed().as_nanos()) else {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        };
        // Diagnostic contention/overflow is visible missing evidence, never a worker wait.
        if let Ok(mut events) = self.events.try_lock() {
            if events.len() < CAPACITY {
                events.push(Event {
                    at_ns,
                    event,
                    generation,
                    io_error_kind,
                });
                return;
            }
        }
        self.dropped.fetch_add(1, Ordering::Relaxed);
    }
    pub(super) fn record(&self, event: &'static str, generation: Option<u64>) {
        self.push(event, generation, None);
    }
    pub(super) fn record_io<T>(&self, event: &'static str, result: &std::io::Result<T>) {
        self.push(
            event,
            None,
            result.as_ref().err().map(|e| format!("{:?}", e.kind())),
        );
    }
    pub(super) fn begin_receive(&self) {
        self.receive_outcome.store(0, Ordering::Release);
        self.receive_begin.store(
            u64::try_from(self.origin.elapsed().as_nanos()).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
    }
    pub(super) fn finish_receive(
        &self,
        received: &Result<Result<(), String>, std::sync::mpsc::RecvTimeoutError>,
    ) {
        self.receive_end.store(
            u64::try_from(self.origin.elapsed().as_nanos()).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        let code = match received {
            Ok(Ok(())) => 1,
            Ok(Err(_)) => 2,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => 3,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => 4,
        };
        self.receive_outcome.store(code, Ordering::Release);
    }
    pub(super) fn snapshot(&self) -> Value {
        let events = self.events.try_lock().ok().map(|events| events.clone());
        let dropped = self.dropped.load(Ordering::Relaxed);
        let code = self.receive_outcome.load(Ordering::Acquire);
        let begin = self.receive_begin.load(Ordering::Relaxed);
        let end = self.receive_end.load(Ordering::Relaxed);
        let outcome = match code {
            1 => Some("Ok"),
            2 => Some("WorkerError"),
            3 => Some("Timeout"),
            4 => Some("Disconnected"),
            _ => None,
        };
        let elapsed = (code != 0 && begin != u64::MAX && end != u64::MAX)
            .then(|| end.checked_sub(begin))
            .flatten();
        json!({"last_receive":{"outcome":outcome,"begin_ns":begin,"end_ns":end,"elapsed_ns":elapsed},"schema":"tc167-flush-diagnostic-v1", "label":self.label, "clock":"one writer std::time::Instant since opt-in construction", "capacity":CAPACITY, "dropped":dropped, "snapshot_available":events.is_some(), "complete_snapshot":events.is_some() && dropped==0, "events":events})
    }
}
// Unwind is observed without catching it or replacing physical handle/join evidence.
pub(super) struct WorkerLifetime(Option<Arc<FlushDiagnostic>>);
impl WorkerLifetime {
    pub(super) fn new(trace: Option<Arc<FlushDiagnostic>>) -> Self {
        if let Some(t) = &trace {
            t.record("worker-enter", None);
        }
        Self(trace)
    }
}
impl Drop for WorkerLifetime {
    fn drop(&mut self) {
        if let Some(t) = &self.0 {
            t.record(
                if std::thread::panicking() {
                    "worker-unwind"
                } else {
                    "worker-return"
                },
                None,
            );
        }
    }
}

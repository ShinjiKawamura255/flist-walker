//! Bounded per-owner observations. No blocking or allocation on the record path.
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::SeqCst};
pub(super) const CAPACITY: usize = 64;
#[derive(Clone, Copy)]
#[repr(u8)]
pub(crate) enum Kind {
    Started,
    SendAttempt,
    SendAccepted,
    SendFull,
    SendDisconnected,
    Dequeued,
    BatchBegin,
    BatchOk,
    BatchError,
    IoBegin,
    IoEnd,
    IoUnwind,
    WaitBegin,
    WaitOk,
    WaitTimeout,
    WaitDisconnected,
    WaitError,
    BodyReturned,
    Joined,
}
const KINDS: &[&str] = &[
    "started",
    "send-attempt",
    "send-accepted",
    "send-full",
    "send-disconnected",
    "dequeued",
    "batch-begin",
    "batch-ok",
    "batch-error",
    "io-begin",
    "io-end",
    "io-unwind",
    "wait-begin",
    "wait-ok",
    "wait-timeout",
    "wait-disconnected",
    "wait-error",
    "body-returned",
    "joined",
];
pub(super) fn valid_kind(label: &str) -> bool {
    KINDS.contains(&label)
}
pub(super) struct Record {
    pub at_us: u64,
    pub kind: Kind,
    pub command: u64,
    pub generation: u64,
    pub operation: u64,
    pub stage: u8,
    pub detail: u8,
}
struct Slot {
    busy: AtomicBool,
    stamp: AtomicU64,
    time: AtomicU64,
    command: AtomicU64,
    generation: AtomicU64,
    operation: AtomicU64,
    meta: AtomicU64,
}
impl Slot {
    fn new() -> Self {
        Self {
            busy: AtomicBool::new(false),
            stamp: AtomicU64::new(0),
            time: AtomicU64::new(0),
            command: AtomicU64::new(0),
            generation: AtomicU64::new(0),
            operation: AtomicU64::new(0),
            meta: AtomicU64::new(0),
        }
    }
}
pub(super) struct History {
    next: AtomicU64,
    dropped: AtomicU64,
    stale: AtomicU64,
    slots: [Slot; CAPACITY],
}
impl History {
    pub fn new() -> Self {
        Self {
            next: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
            stale: AtomicU64::new(0),
            slots: std::array::from_fn(|_| Slot::new()),
        }
    }
    pub fn record(&self, r: Record) {
        let ticket = self.next.fetch_add(1, SeqCst) + 1;
        let slot = &self.slots[((ticket - 1) % CAPACITY as u64) as usize];
        // A stalled previous writer cannot corrupt a newer writer. Never retry.
        if slot
            .busy
            .compare_exchange(false, true, SeqCst, SeqCst)
            .is_err()
        {
            self.dropped.fetch_add(1, SeqCst);
            return;
        }
        if slot.stamp.load(SeqCst) >= ticket {
            self.stale.fetch_add(1, SeqCst);
            slot.busy.store(false, SeqCst);
            return;
        }
        slot.stamp.store(0, SeqCst);
        slot.time.store(r.at_us, SeqCst);
        slot.command.store(r.command, SeqCst);
        slot.generation.store(r.generation, SeqCst);
        slot.operation.store(r.operation, SeqCst);
        slot.meta.store(
            r.kind as u64 | ((r.stage as u64) << 8) | ((r.detail as u64) << 16),
            SeqCst,
        );
        slot.stamp.store(ticket, SeqCst);
        slot.busy.store(false, SeqCst);
    }
    pub fn snapshot(&self) -> serde_json::Value {
        let end = self.next.load(SeqCst);
        let begin = end.saturating_sub(CAPACITY as u64) + 1;
        let mut rows = Vec::new();
        for ticket in begin..=end {
            let slot = &self.slots[((ticket - 1) % CAPACITY as u64) as usize];
            if slot.stamp.load(SeqCst) != ticket {
                continue;
            }
            let (time, command, generation, operation, meta) = (
                slot.time.load(SeqCst),
                slot.command.load(SeqCst),
                slot.generation.load(SeqCst),
                slot.operation.load(SeqCst),
                slot.meta.load(SeqCst),
            );
            if slot.stamp.load(SeqCst) != ticket {
                continue;
            }
            rows.push(serde_json::json!({"sequence":ticket,"at_us":time,"kind":KINDS.get((meta&255)as usize).copied().unwrap_or("unknown"),"command_id":command,"generation":generation,"operation_id":operation,"stage":super::label((meta>>8)as u8),"detail":(meta>>16)&255}));
        }
        serde_json::json!({"capacity":CAPACITY,"reserved_records":end,"reserved_before_window":end.saturating_sub(CAPACITY as u64),"dropped_busy":self.dropped.load(SeqCst),"dropped_stale":self.stale.load(SeqCst),"unavailable_in_window":end.min(CAPACITY as u64)-rows.len()as u64,"records":rows,"scope":"bounded best-effort observations; sequence is reservation order"})
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn item(id: u64) -> Record {
        Record {
            at_us: id,
            kind: Kind::IoBegin,
            command: id,
            generation: id * 3,
            operation: id * 7,
            stage: 0,
            detail: 0,
        }
    }
    #[test]
    fn wraps_latest_window_and_reports_loss() {
        let h = History::new();
        for id in 1..=96 {
            h.record(item(id));
        }
        let s = h.snapshot();
        let rows = s["records"].as_array().unwrap();
        assert_eq!(rows.len(), 64);
        assert_eq!(rows[0]["sequence"], 33);
        assert_eq!(s["reserved_before_window"], 32);
        assert_eq!(s["unavailable_in_window"], 0);
    }
    #[test]
    fn busy_slot_is_dropped_without_waiting_and_owners_are_independent() {
        let h = History::new();
        h.slots[0].busy.store(true, SeqCst);
        h.record(item(1));
        assert_eq!(h.snapshot()["dropped_busy"], 1);
        assert_eq!(h.snapshot()["unavailable_in_window"], 1);
        let other = History::new();
        other.record(item(2));
        assert_eq!(other.snapshot()["records"][0]["command_id"], 2);
    }
    #[test]
    fn stale_ticket_never_overwrites_a_newer_committed_slot() {
        let h = History::new();
        h.slots[0].stamp.store(65, SeqCst);
        h.record(item(1));
        assert_eq!(h.slots[0].stamp.load(SeqCst), 65);
        assert_eq!(h.snapshot()["dropped_stale"], 1);
        assert_eq!(h.snapshot()["unavailable_in_window"], 1);
    }
    #[test]
    fn concurrent_snapshot_never_combines_different_records() {
        let h = std::sync::Arc::new(History::new());
        let threads = (0..4)
            .map(|n| {
                let h = h.clone();
                std::thread::spawn(move || {
                    for i in 1..=1024 {
                        h.record(item(n * 1024 + i));
                    }
                })
            })
            .collect::<Vec<_>>();
        let mut consistent = true;
        for _ in 0..128 {
            for row in h.snapshot()["records"].as_array().unwrap() {
                let id = row["command_id"].as_u64().unwrap();
                consistent &= row["generation"].as_u64() == Some(id * 3)
                    && row["operation_id"].as_u64() == Some(id * 7);
            }
        }
        for t in threads {
            t.join().unwrap();
        }
        assert!(consistent);
    }
}

//! Scoped physical worker timings for the isolated settings diagnostics.
use std::cell::RefCell;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

#[derive(Clone, Copy)]
pub(crate) enum Milestone {
    Pickup,
    LockAcquired,
    WriteComplete,
    NotifyAttempt,
    NotifySucceeded,
}

const MILESTONES: [(Milestone, &str); 5] = [
    (Milestone::Pickup, "pickup"),
    (Milestone::LockAcquired, "lock_acquired"),
    (Milestone::WriteComplete, "write_complete"),
    (Milestone::NotifyAttempt, "response_notify_attempt"),
    (Milestone::NotifySucceeded, "response_send_returned_ok"),
];

#[derive(Clone)]
pub(crate) struct SaveTiming {
    origin: Instant,
    marks: Arc<[OnceLock<Instant>; MILESTONES.len()]>,
}

impl SaveTiming {
    pub(crate) fn new() -> Self {
        Self {
            origin: Instant::now(),
            marks: Arc::new(std::array::from_fn(|_| OnceLock::new())),
        }
    }

    pub(crate) fn mark(&self, milestone: Milestone) {
        let _ = self.marks[milestone as usize].set(Instant::now());
    }

    pub(crate) fn describe(&self, deadline: Instant) -> String {
        let deadline_us = deadline.saturating_duration_since(self.origin).as_micros();
        let mut report = format!("deadline_us_from_submit={deadline_us}");
        // Read later stages first so we do not miss an earlier checkpoint of a
        // send that completes while this status is being captured.
        let snapshot: [Option<Instant>; MILESTONES.len()] =
            std::array::from_fn(|i| self.marks[MILESTONES.len() - 1 - i].get().copied());
        for (stage, name) in MILESTONES {
            let value = snapshot[MILESTONES.len() - 1 - stage as usize].map_or_else(
                || "unobserved".to_owned(),
                |at| {
                    let elapsed_us = at.saturating_duration_since(self.origin).as_micros();
                    let relative_to_deadline = if at < deadline {
                        "before"
                    } else {
                        "at_or_after"
                    };
                    format!("{elapsed_us}us:{relative_to_deadline}")
                },
            );
            report.push_str(&format!(" {name}={value}"));
        }
        report
    }
}

thread_local! {
    static ACTIVE_SAVE: RefCell<Option<SaveTiming>> = const { RefCell::new(None) };
}

pub(crate) fn with_save_timing<T>(timing: Option<&SaveTiming>, save: impl FnOnce() -> T) -> T {
    if let Some(timing) = timing {
        struct Restore(Option<SaveTiming>);
        impl Drop for Restore {
            fn drop(&mut self) {
                ACTIVE_SAVE.with(|active| active.replace(self.0.take()));
            }
        }
        let _restore = Restore(ACTIVE_SAVE.with(|active| active.replace(Some(timing.clone()))));
        save()
    } else {
        save()
    }
}

pub(crate) fn mark_save_milestone(milestone: Milestone) {
    ACTIVE_SAVE.with(|active| {
        if let Some(timing) = active.borrow().as_ref() {
            timing.mark(milestone);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn deadline_classification_uses_monotonic_observations_not_millisecond_rounding() {
        let timing = SaveTiming::new();
        let deadline = timing.origin + Duration::from_micros(2_000_000);
        for (milestone, at) in [
            (
                Milestone::Pickup,
                timing.origin + Duration::from_micros(999),
            ),
            (Milestone::LockAcquired, deadline - Duration::from_nanos(1)),
            (Milestone::WriteComplete, deadline),
            (Milestone::NotifyAttempt, deadline + Duration::from_nanos(1)),
        ] {
            timing.marks[milestone as usize].set(at).unwrap();
        }
        let report = timing.describe(deadline);
        assert_eq!(
            report,
            "deadline_us_from_submit=2000000 pickup=999us:before lock_acquired=1999999us:before write_complete=2000000us:at_or_after response_notify_attempt=2000000us:at_or_after response_send_returned_ok=unobserved"
        );
    }
}

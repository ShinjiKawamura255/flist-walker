//! Request-local observations of the real search worker for indexing perf tests.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Instant;

#[derive(Clone, Debug, Default)]
pub(in crate::app) struct SearchPerfObservation {
    pub(in crate::app) started_at: Option<Instant>,
    pub(in crate::app) evaluation_completed_at: Option<Instant>,
    pub(in crate::app) completed_at: Option<Instant>,
    pub(in crate::app) canceled_at: Option<Instant>,
    pub(in crate::app) candidates: usize,
    pub(in crate::app) evaluated_candidates: usize,
    pub(in crate::app) skipped_canceled: bool,
}

impl SearchPerfObservation {
    pub(super) fn begin(&mut self, candidates: usize, canceled: bool) {
        self.candidates = candidates;
        if canceled {
            self.skipped_canceled = true;
            self.canceled_at = Some(Instant::now());
        } else if candidates > 0 {
            self.started_at = Some(Instant::now());
        }
    }

    pub(super) fn evaluated(&mut self, evaluated_candidates: usize) {
        self.evaluated_candidates = evaluated_candidates;
        self.evaluation_completed_at = Some(Instant::now());
    }

    pub(super) fn finish(&mut self, canceled: bool) {
        if canceled {
            self.canceled_at.get_or_insert_with(Instant::now);
        } else {
            self.completed_at = Some(Instant::now());
        }
    }
}

struct Registration {
    // Retaining a weak cancellation identity also prevents its allocation address
    // from being reused while an unclaimed observation is registered.
    cancel: Weak<AtomicBool>,
    observation: Weak<Mutex<SearchPerfObservation>>,
}

fn registry() -> &'static Mutex<HashMap<usize, Registration>> {
    static REGISTRY: OnceLock<Mutex<HashMap<usize, Registration>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(in crate::app) fn register(cancel: &Arc<AtomicBool>) -> Arc<Mutex<SearchPerfObservation>> {
    let observation = Arc::new(Mutex::new(SearchPerfObservation::default()));
    let mut registrations = registry().lock().expect("search perf registry");
    registrations.retain(|_, registered| {
        registered.cancel.strong_count() > 0 && registered.observation.strong_count() > 0
    });
    registrations.insert(
        Arc::as_ptr(cancel) as usize,
        Registration {
            cancel: Arc::downgrade(cancel),
            observation: Arc::downgrade(&observation),
        },
    );
    observation
}

pub(super) fn take(cancel: &Arc<AtomicBool>) -> Option<Arc<Mutex<SearchPerfObservation>>> {
    let registered = registry()
        .lock()
        .expect("search perf registry")
        .remove(&(Arc::as_ptr(cancel) as usize))?;
    let identity = registered.cancel.upgrade()?;
    if !Arc::ptr_eq(cancel, &identity) {
        return None;
    }
    registered.observation.upgrade()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    #[test]
    fn observations_are_isolated_by_cancellation_identity_and_taken_once() {
        let first = Arc::new(AtomicBool::new(false));
        let second = Arc::new(AtomicBool::new(false));
        let first_observation = register(&first);
        let second_observation = register(&second);
        assert!(Arc::ptr_eq(
            &take(&first).expect("first registered"),
            &first_observation
        ));
        assert!(take(&first).is_none());
        assert!(Arc::ptr_eq(
            &take(&second).expect("second registered"),
            &second_observation
        ));
        assert!(take(&second).is_none());
    }

    #[test]
    fn default_observation_does_not_claim_execution() {
        let observation = SearchPerfObservation::default();
        assert!(observation.started_at.is_none());
        assert!(observation.completed_at.is_none());
        assert!(observation.canceled_at.is_none());
        assert_eq!(observation.candidates, 0);
        assert_eq!(observation.evaluated_candidates, 0);
        assert!(observation.evaluation_completed_at.is_none());
        assert!(!observation.skipped_canceled);
    }

    #[test]
    fn substantive_execution_records_candidates_and_completion() {
        let mut observation = SearchPerfObservation::default();
        observation.begin(23, false);
        observation.evaluated(17);
        observation.finish(false);
        assert_eq!(observation.candidates, 23);
        assert_eq!(observation.evaluated_candidates, 17);
        assert!(observation.evaluation_completed_at >= observation.started_at);
        assert!(observation.completed_at >= observation.evaluation_completed_at);
        assert!(observation.started_at.is_some());
        assert!(observation.completed_at >= observation.started_at);
        assert!(observation.canceled_at.is_none());
        assert!(!observation.skipped_canceled);
    }

    #[test]
    fn canceled_before_execution_is_distinct_from_cancellation_during_execution() {
        let mut skipped = SearchPerfObservation::default();
        skipped.begin(23, true);
        let canceled_at = skipped.canceled_at;
        skipped.finish(true);
        assert!(skipped.started_at.is_none());
        assert!(skipped.completed_at.is_none());
        assert!(skipped.canceled_at.is_some());
        assert_eq!(skipped.canceled_at, canceled_at);
        assert!(skipped.skipped_canceled);

        let mut running = SearchPerfObservation::default();
        running.begin(23, false);
        running.finish(true);
        assert!(running.started_at.is_some());
        assert!(running.completed_at.is_none());
        assert!(running.canceled_at >= running.started_at);
        assert!(!running.skipped_canceled);
    }

    #[test]
    fn cancellation_after_evaluation_retains_completed_evaluation_evidence() {
        let mut observation = SearchPerfObservation::default();
        observation.begin(23, false);
        observation.evaluated(17);
        observation.finish(true);
        assert_eq!(observation.evaluated_candidates, 17);
        assert!(observation.evaluation_completed_at >= observation.started_at);
        assert!(observation.canceled_at >= observation.evaluation_completed_at);
        assert!(observation.completed_at.is_none());
        assert!(!observation.skipped_canceled);
    }

    #[test]
    fn empty_candidates_do_not_claim_substantive_execution() {
        let mut observation = SearchPerfObservation::default();
        observation.begin(0, false);
        observation.finish(false);
        assert!(observation.started_at.is_none());
        assert!(observation.completed_at.is_some());
        assert_eq!(observation.candidates, 0);
    }

    #[test]
    fn unregistered_or_dropped_observation_cannot_be_taken() {
        let missing = Arc::new(AtomicBool::new(false));
        assert!(take(&missing).is_none());
        let registered = Arc::new(AtomicBool::new(false));
        drop(register(&registered));
        assert!(take(&registered).is_none());
    }

    #[test]
    fn registration_prunes_dead_cancellation_identity_with_live_observation() {
        let original = Arc::new(AtomicBool::new(false));
        let original_key = Arc::as_ptr(&original) as usize;
        let weak_identity = Arc::downgrade(&original);
        let observation = register(&original);
        drop(original);
        let next = Arc::new(AtomicBool::new(false));
        let next_observation = register(&next);
        assert!(!registry()
            .lock()
            .expect("registry")
            .contains_key(&original_key));
        assert!(weak_identity.upgrade().is_none());
        assert!(observation
            .lock()
            .expect("observation")
            .started_at
            .is_none());
        assert!(Arc::ptr_eq(
            &take(&next).expect("next registered"),
            &next_observation
        ));
    }

    #[test]
    fn mismatched_identity_is_rejected_even_with_matching_registry_key() {
        let original = Arc::new(AtomicBool::new(false));
        let unrelated = Arc::new(AtomicBool::new(false));
        let observation = Arc::new(Mutex::new(SearchPerfObservation::default()));
        registry().lock().expect("registry").insert(
            Arc::as_ptr(&unrelated) as usize,
            Registration {
                cancel: Arc::downgrade(&original),
                observation: Arc::downgrade(&observation),
            },
        );
        assert!(take(&unrelated).is_none());
    }
}

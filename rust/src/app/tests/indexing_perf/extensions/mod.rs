//! Additional actual-worker profiles; the cold basic trace remains independent.
use super::*;
mod cases;
mod driver;
mod fixture;
mod oracle;
mod runner;
mod supplementary;

#[test]
fn tc_229_real_worker_extended_profiles_smoke() {
    if crate::app::tests::indexing_perf::harness::child_process::isolate(
        module_path!(),
        "tc_229_real_worker_extended_profiles_smoke",
    ) {
        return;
    }
    use cases::Profile;
    use fixture::{ExtendedFixture, Shape};
    let mut fixtures = Vec::new();
    for profile in Profile::ALL
        .into_iter()
        .filter(|p| !p.parser() && *p != Profile::Truncated)
    {
        if !fixtures.iter().any(|(shape, _)| *shape == profile.shape()) {
            fixtures.push((profile.shape(), ExtendedFixture::new(4096, profile.shape())));
        }
    }
    let b = ExtendedFixture::new(4096, Shape::FlatFiles);
    let c = ExtendedFixture::new(4096, Shape::FlatFiles);
    for profile in Profile::ALL
        .into_iter()
        .filter(|p| !p.parser() && *p != Profile::Truncated)
    {
        let f = &fixtures
            .iter()
            .find(|(shape, _)| *shape == profile.shape())
            .unwrap()
            .1;
        let others = if profile == Profile::TabChain {
            vec![&b, &c]
        } else if profile.tabs() {
            vec![&b]
        } else {
            vec![]
        };
        for source in profile.sources() {
            eprintln!("EXTENSION_SMOKE {} {}", profile.name(), source.name());
            let row = driver::run(f, &others, profile, source, true, false);
            assert_eq!(row["correct"], true);
        }
    }
}

#[derive(Default)]
struct ExtendedTruth {
    successful_terminal: bool,
    owned_physical_complete: bool,
    owned_request_released: bool,
    snapshot_valid: bool,
    debt: bool,
    latest_results_valid: bool,
}
impl ExtendedTruth {
    fn index_ready(&self) -> bool {
        self.successful_terminal
            && self.owned_physical_complete
            && self.owned_request_released
            && self.snapshot_valid
            && !self.debt
    }
    fn results_ready(&self) -> bool {
        self.index_ready() && self.latest_results_valid
    }
}

struct DeliveredProof {
    dispatched: Instant,
    delivered: Instant,
    owned_identity: bool,
    successful: bool,
    count: usize,
}
impl DeliveredProof {
    fn completed_while_unsettled(&self, start: Instant, cutoff: Instant) -> bool {
        self.owned_identity
            && self.successful
            && self.count > 0
            && self.dispatched >= start
            && self.delivered >= self.dispatched
            && self.delivered <= cutoff
    }
}

fn owned_background_ingested(_active_count: usize, background_count: usize) -> usize {
    background_count
}

#[test]
fn tc_229_failed_or_stale_terminal_and_debt_cannot_complete() {
    let mut truth = ExtendedTruth {
        successful_terminal: true,
        owned_physical_complete: true,
        owned_request_released: true,
        snapshot_valid: true,
        latest_results_valid: true,
        ..Default::default()
    };
    assert!(truth.results_ready());
    truth.successful_terminal = false;
    assert!(
        !truth.index_ready(),
        "Failed/Canceled cannot certify successful completion"
    );
    truth.successful_terminal = true;
    truth.owned_request_released = false;
    assert!(
        !truth.index_ready(),
        "bookkeeping release belongs to own request"
    );
    truth.owned_request_released = true;
    truth.debt = true;
    assert!(
        !truth.results_ready(),
        "background finalizer/reclaim debt remains"
    );
}
#[test]
fn tc_229_successful_rx_requires_owned_identity_and_interval() {
    let start = Instant::now();
    let mut proof = DeliveredProof {
        dispatched: start + Duration::from_millis(1),
        delivered: start + Duration::from_millis(2),
        owned_identity: true,
        successful: true,
        count: 1,
    };
    let cutoff = start + Duration::from_millis(10);
    assert!(proof.completed_while_unsettled(start, cutoff));
    proof.owned_identity = false;
    assert!(
        !proof.completed_while_unsettled(start, cutoff),
        "stale id/epoch/path cannot prove work"
    );
    proof.owned_identity = true;
    proof.successful = false;
    assert!(!proof.completed_while_unsettled(start, cutoff));
    proof.successful = true;
    proof.count = 0;
    assert!(!proof.completed_while_unsettled(start, cutoff));
    proof.count = 1;
    proof.dispatched = start - Duration::from_millis(1);
    assert!(!proof.completed_while_unsettled(start, cutoff));
    proof.dispatched = start + Duration::from_millis(1);
    proof.delivered = cutoff + Duration::from_millis(1);
    assert!(!proof.completed_while_unsettled(start, cutoff));
}
#[test]
fn tc_229_warm_checkpoints_do_not_use_active_committed_count() {
    assert_eq!(owned_background_ingested(100_000, 10_000), 10_000);
}

#[test]
fn tc_229_revoked_closed_mailbox_preserves_physical_terminal_offer() {
    let mailbox = IndexResponseMailbox::new();
    mailbox.enable_perf_observation();
    mailbox.close();
    mailbox.record_terminal_offer(
        &crate::app::worker::protocol::IndexResponse::Failed {
            request_id: 7,
            error: "index receiver closed".into(),
        },
        false,
    );
    mailbox.record_terminal_send_returned();
    let observation = mailbox.perf_observation();
    assert_eq!(
        observation.terminal_kind, None,
        "closed mailbox did not publish success"
    );
    assert_eq!(observation.terminal_offer_kind, Some("failed"));
    assert_eq!(observation.terminal_offer_current, Some(false));
    assert!(observation.terminal_send_returned >= observation.terminal_offered);
}

fn physical_request_complete(
    observation: &crate::app::index_mailbox::IndexPerfObservation,
) -> bool {
    observation.request_processing_returned.is_some()
        && !observation.stale_full_data_abort_duplicate
}
#[test]
fn tc_229_cleanup_closed_without_physical_return_cannot_complete() {
    let mut observation = crate::app::index_mailbox::IndexPerfObservation {
        mailbox_closed: true,
        terminal_offer_kind: Some("canceled"),
        ..Default::default()
    };
    assert!(!physical_request_complete(&observation));
    observation.request_processing_returned = Some(Instant::now());
    assert!(physical_request_complete(&observation));
}

#[test]
fn tc_229_terminal_cleanup_without_actual_return_is_not_t2() {
    let truth = ExtendedTruth {
        successful_terminal: true,
        owned_request_released: true,
        snapshot_valid: true,
        latest_results_valid: true,
        owned_physical_complete: false,
        ..Default::default()
    };
    assert!(
        !truth.index_ready(),
        "successful terminal and bookkeeping cleanup do not prove worker return"
    );
}

#[test]
fn tc_229_empty_query_all_matches_sort_proves_successful_worker_response() {
    if crate::app::tests::indexing_perf::harness::child_process::isolate(
        module_path!(),
        "tc_229_empty_query_all_matches_sort_proves_successful_worker_response",
    ) {
        return;
    }
    let fixture = fixture::ExtendedFixture::new(4096, fixture::Shape::FlatMixed);
    let row = driver::run(
        &fixture,
        &[],
        cases::Profile::NameAll,
        Source::FileList,
        true,
        false,
    );
    assert!(
        row["aux_observations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["flow"] == "search-sort" && o["successful"] == true),
        "empty-query sort needs owned successful worker RX rather than fuzzy evaluation count: aux={} bindings={} workers={}",
        row["aux_observations"],
        row["search_dispatch_bindings"],
        row["worker_observations"]
    );
}

#[test]
fn tc_229_search_sort_rejects_type_mode_stale_error_and_zero_candidates() {
    if crate::app::tests::indexing_perf::harness::child_process::isolate(
        module_path!(),
        "tc_229_search_sort_rejects_type_mode_stale_error_and_zero_candidates",
    ) {
        return;
    }
    let mut driver = Driver::new();
    driver.settle_startup();
    let root = driver.app.shell.runtime.root.clone();
    let i = &mut driver.app.shell.indexing;
    i.perf_search_bindings
        .push(crate::app::index_coordinator::SearchDispatchBinding {
            request_id: 7,
            tab_id: 1,
            epoch: 9,
            root: root.clone(),
            query: String::new(),
            candidates: 1000,
            candidate_ptr: 11,
            sort_mode: ResultSortMode::NameAsc,
            sort_scope: ResultSortScope::AllMatches,
            at: Instant::now(),
        });
    let accepts = |i: &crate::app::index_coordinator::IndexCoordinator,
                   id,
                   tab,
                   epoch,
                   root: &std::path::Path,
                   mode,
                   scope,
                   nonempty: bool,
                   error: bool| {
        let response = crate::app::SearchResponse {
            request_id: id,
            results: if nonempty {
                vec![(root.join("item"), 0.0)]
            } else {
                vec![]
            },
            total_match_count: usize::from(nonempty),
            sort_mode: mode,
            sort_scope: scope,
            error: error.then(|| "test search error".to_owned()),
        };
        i.perf_search_sort_owned(
            &response,
            crate::app::index_coordinator::PerfSearchSortIdentity {
                tab,
                epoch,
                root,
                query: "",
            },
        )
    };
    assert!(accepts(
        i,
        7,
        1,
        9,
        &root,
        ResultSortMode::NameAsc,
        ResultSortScope::AllMatches,
        true,
        false
    ));
    for (id, tab, epoch, mode, scope, nonempty, error) in [
        (
            8,
            1,
            9,
            ResultSortMode::NameAsc,
            ResultSortScope::AllMatches,
            true,
            false,
        ),
        (
            7,
            2,
            9,
            ResultSortMode::NameAsc,
            ResultSortScope::AllMatches,
            true,
            false,
        ),
        (
            7,
            1,
            10,
            ResultSortMode::NameAsc,
            ResultSortScope::AllMatches,
            true,
            false,
        ),
        (
            7,
            1,
            9,
            ResultSortMode::Score,
            ResultSortScope::AllMatches,
            true,
            false,
        ),
        (
            7,
            1,
            9,
            ResultSortMode::NameAsc,
            ResultSortScope::ShownResults,
            true,
            false,
        ),
        (
            7,
            1,
            9,
            ResultSortMode::NameAsc,
            ResultSortScope::AllMatches,
            false,
            false,
        ),
        (
            7,
            1,
            9,
            ResultSortMode::NameAsc,
            ResultSortScope::AllMatches,
            true,
            true,
        ),
    ] {
        assert!(!accepts(
            i, id, tab, epoch, &root, mode, scope, nonempty, error
        ));
    }
    assert!(!accepts(
        i,
        7,
        1,
        9,
        &root.join("wrong-root"),
        ResultSortMode::NameAsc,
        ResultSortScope::AllMatches,
        true,
        false
    ));
    i.perf_search_bindings[0].candidates = 0;
    assert!(!accepts(
        i,
        7,
        1,
        9,
        &root,
        ResultSortMode::NameAsc,
        ResultSortScope::AllMatches,
        true,
        false
    ));
}

fn completed_search_sort(
    stats: &crate::app::worker::search_perf::SearchPerfObservation,
    start: Instant,
    dispatch: Instant,
    delivered: Instant,
    cutoff: Instant,
    candidates: usize,
) -> bool {
    !stats.skipped_canceled
        && stats.canceled_at.is_none()
        && candidates > 0
        && stats.candidates == candidates
        && dispatch >= start
        && delivered <= cutoff
        && stats
            .started_at
            .zip(stats.completed_at)
            .is_some_and(|(begin, end)| begin >= dispatch && end >= begin && end <= delivered)
}
#[test]
fn tc_229_empty_sort_queued_canceled_or_late_work_cannot_prove_overlap() {
    use crate::app::worker::search_perf::SearchPerfObservation;
    let start = Instant::now();
    let dispatch = start + Duration::from_millis(1);
    let delivered = start + Duration::from_millis(4);
    let cutoff = start + Duration::from_millis(5);
    let mut stats = SearchPerfObservation::default();
    assert!(!completed_search_sort(
        &stats, start, dispatch, delivered, cutoff, 1000
    ));
    stats.started_at = Some(start + Duration::from_millis(2));
    stats.completed_at = Some(start + Duration::from_millis(3));
    stats.candidates = 1000;
    assert!(completed_search_sort(
        &stats, start, dispatch, delivered, cutoff, 1000
    ));
    stats.skipped_canceled = true;
    assert!(!completed_search_sort(
        &stats, start, dispatch, delivered, cutoff, 1000
    ));
    stats.skipped_canceled = false;
    stats.canceled_at = Some(start + Duration::from_millis(3));
    assert!(!completed_search_sort(
        &stats, start, dispatch, delivered, cutoff, 1000
    ));
    stats.canceled_at = None;
    assert!(!completed_search_sort(
        &stats, start, dispatch, delivered, cutoff, 0
    ));
    assert!(!completed_search_sort(
        &stats, start, dispatch, delivered, cutoff, 999
    ));
    assert!(!completed_search_sort(
        &stats, start, dispatch, delivered, dispatch, 1000
    ));
    stats.completed_at = None;
    assert!(!completed_search_sort(
        &stats, start, dispatch, delivered, cutoff, 1000
    ));
}

#[derive(Clone, Default)]
struct PlannedRequest {
    id: u64,
    tab: u64,
    revoked_by: Option<u64>,
    completed_predecessor: bool,
    completed_predecessor_observed_at: Option<Instant>,
    permission_at: Option<Instant>,
    permission_reason: Option<&'static str>,
    victim: Option<VictimDeclaration>,
}
#[derive(Clone)]
struct RequestProof {
    id: u64,
    tab: u64,
    finished: bool,
    physical_complete: bool,
    source_root_correct: bool,
    revoked_abort: bool,
    last_good_victim: bool,
}
fn planned_requests_valid(planned: &[PlannedRequest], actual: &[RequestProof]) -> bool {
    if planned.is_empty() || planned.len() != actual.len() {
        return false;
    }
    let mut ids = std::collections::HashSet::new();
    if !planned.iter().all(|p| ids.insert(p.id)) {
        return false;
    }
    ids.clear();
    if !actual.iter().all(|p| ids.insert(p.id)) {
        return false;
    }
    planned.iter().all(|p| {
        let Some(a) = actual.iter().find(|a| a.id == p.id && a.tab == p.tab) else {
            return false;
        };
        if !a.physical_complete {
            return false;
        }
        if let Some(next) = p.revoked_by {
            if !planned
                .iter()
                .any(|n| n.id == next && n.tab == p.tab && n.id > p.id)
            {
                return false;
            }
            (a.finished && a.source_root_correct) || a.revoked_abort
        } else {
            (p.completed_predecessor || !planned.iter().any(|n| n.tab == p.tab && n.id > p.id))
                && ((a.finished && a.source_root_correct)
                    || (!a.finished
                        && a.last_good_victim
                        && p.victim.is_some()
                        && !planned.iter().any(|n| n.tab == p.tab && n.id > p.id)))
        }
    })
}
#[test]
fn tc_229_planned_requests_reject_secondary_cancel_extra_and_unplanned_revocation() {
    let plans = vec![
        PlannedRequest {
            id: 1,
            tab: 1,
            revoked_by: None,
            ..Default::default()
        },
        PlannedRequest {
            id: 2,
            tab: 2,
            revoked_by: None,
            ..Default::default()
        },
    ];
    let proof = |id, tab, finished| RequestProof {
        id,
        tab,
        finished,
        physical_complete: true,
        source_root_correct: true,
        revoked_abort: !finished,
        last_good_victim: false,
    };
    assert!(planned_requests_valid(
        &plans,
        &[proof(1, 1, true), proof(2, 2, true)]
    ));
    assert!(
        !planned_requests_valid(&plans, &[proof(1, 1, true), proof(2, 2, false)]),
        "secondary latest canceled cannot use old snapshot"
    );
    assert!(
        !planned_requests_valid(
            &plans,
            &[proof(1, 1, true), proof(2, 2, true), proof(3, 1, true)]
        ),
        "unexpected extra work changes the scenario"
    );
    let unplanned = vec![
        plans[0].clone(),
        PlannedRequest {
            id: 3,
            tab: 1,
            revoked_by: None,
            ..Default::default()
        },
    ];
    assert!(
        !planned_requests_valid(&unplanned, &[proof(1, 1, false), proof(3, 1, true)]),
        "later unplanned request does not authorize earlier failure"
    );
    let mut authorized = unplanned;
    authorized[0].revoked_by = Some(3);
    assert!(planned_requests_valid(
        &authorized,
        &[proof(1, 1, false), proof(3, 1, true)]
    ));
}

fn abort_after_permission(abort: Option<Instant>, permission: Option<Instant>) -> bool {
    matches!((abort, permission), (Some(a), Some(p)) if a >= p)
}
#[test]
fn tc_229_later_operation_cannot_authorize_earlier_abort() {
    let first = Instant::now();
    let later = first + Duration::from_millis(1);
    assert!(!abort_after_permission(Some(first), Some(later)));
    assert!(!abort_after_permission(Some(first), None));
    assert!(!abort_after_permission(None, Some(first)));
    assert!(abort_after_permission(Some(later), Some(first)));
}

fn dependent_input_allowed(requested: u64, active: Option<u64>, pending: Option<u64>) -> bool {
    active == Some(requested) && pending.is_none()
}
#[test]
fn tc_229_dependent_input_waits_for_actual_target_activation() {
    assert!(!dependent_input_allowed(2, Some(1), Some(2)));
    assert!(!dependent_input_allowed(2, Some(2), Some(2)));
    assert!(!dependent_input_allowed(2, Some(1), None));
    assert!(dependent_input_allowed(2, Some(2), None));
}

fn planned_unsent_discard(
    o: &crate::app::index_mailbox::IndexPerfObservation,
    permission: Option<Instant>,
    released: bool,
) -> bool {
    o.allocation_observed
        && o.admitted_at.is_none()
        && o.mailbox_closed
        && released
        && abort_after_permission(o.mailbox_closed_at, permission)
}
#[test]
fn tc_229_unsent_discard_never_substitutes_admitted_worker_return() {
    let at = Instant::now();
    let mut o = crate::app::index_mailbox::IndexPerfObservation {
        allocation_observed: true,
        mailbox_closed: true,
        mailbox_closed_at: Some(at + Duration::from_millis(1)),
        ..Default::default()
    };
    assert!(!planned_unsent_discard(&o, None, true));
    assert!(!planned_unsent_discard(&o, Some(at), false));
    assert!(planned_unsent_discard(&o, Some(at), true));
    o.admitted_at = Some(at);
    assert!(!planned_unsent_discard(&o, Some(at), true));
    assert!(!physical_request_complete(&o));
    o.request_processing_returned = Some(at + Duration::from_millis(2));
    assert!(physical_request_complete(&o));
}

fn requires_latest_generation(plans: &[PlannedRequest], id: u64, tab: u64) -> bool {
    !plans.iter().any(|p| p.tab == tab && p.id > id)
}
#[test]
fn tc_229_completed_latest_cannot_bypass_owned_generation() {
    let mut plans = vec![PlannedRequest {
        id: 1,
        tab: 1,
        completed_predecessor: true,
        ..Default::default()
    }];
    assert!(requires_latest_generation(&plans, 1, 1));
    plans.push(PlannedRequest {
        id: 2,
        tab: 2,
        ..Default::default()
    });
    assert!(requires_latest_generation(&plans, 1, 1));
    plans.push(PlannedRequest {
        id: 3,
        tab: 1,
        ..Default::default()
    });
    assert!(!requires_latest_generation(&plans, 1, 1));
    assert!(requires_latest_generation(&plans, 3, 1));
}
fn request_acquisition_owned(
    o: &crate::app::index_mailbox::IndexPerfObservation,
    root: &std::path::Path,
    source: Source,
) -> bool {
    (o.admitted_at.is_none() || o.admitted_root.as_deref() == Some(root))
        && (o.started_source.is_none() && o.started_root.is_none() && o.started_published.is_none()
            || o.started_source == Some(source.name()) && o.started_root.as_deref() == Some(root))
}
#[test]
fn tc_229_revoked_abort_rejects_wrong_actual_root_or_source() {
    let root = std::path::Path::new("owned-root");
    let mut o = crate::app::index_mailbox::IndexPerfObservation {
        admitted_at: Some(Instant::now()),
        admitted_root: Some("wrong-root".into()),
        ..Default::default()
    };
    assert!(!request_acquisition_owned(&o, root, Source::Walker));
    o.admitted_root = Some(root.into());
    o.started_source = Some("Walker");
    o.started_root = Some("wrong-root".into());
    assert!(!request_acquisition_owned(&o, root, Source::Walker));
    o.started_root = Some(root.into());
    o.started_source = Some("FileList");
    assert!(!request_acquisition_owned(&o, root, Source::Walker));
    o.started_source = Some("Walker");
    assert!(request_acquisition_owned(&o, root, Source::Walker));
    o.started_source = None;
    o.started_root = None;
    assert!(request_acquisition_owned(&o, root, Source::Walker));
    o.admitted_root = None;
    assert!(!request_acquisition_owned(&o, root, Source::Walker));
    o.admitted_at = None;
    assert!(request_acquisition_owned(&o, root, Source::Walker));
}

#[test]
fn tc_229_unmeasured_promotion_control_is_empty_and_quiescent() {
    if crate::app::tests::indexing_perf::harness::child_process::isolate(
        module_path!(),
        "tc_229_unmeasured_promotion_control_is_empty_and_quiescent",
    ) {
        return;
    }
    use cases::Profile;
    use fixture::{ExtendedFixture, Shape};
    let a = ExtendedFixture::new(4096, Shape::FlatFiles);
    let b = ExtendedFixture::new(4096, Shape::FlatFiles);
    let row = driver::run(
        &a,
        &[&b],
        Profile::Promotion,
        Source::FileList,
        false,
        false,
    );
    assert_eq!(row["correct"], true);
    assert_eq!(row["index_requests"].as_array().unwrap().len(), 1);
    let untouched = &row["unmeasured_tab_observations"].as_array().unwrap()[0];
    assert_eq!(untouched["all_entries"], 0);
    assert_eq!(untouched["visible_entries"], 0);
    assert_eq!(untouched["measured_allocation_count"], 0);
    assert_eq!(untouched["index_debt"], false);
}

#[test]
fn tc_229_all_tab_profile_controls_preserve_own_expected_state() {
    if crate::app::tests::indexing_perf::harness::child_process::isolate(
        module_path!(),
        "tc_229_all_tab_profile_controls_preserve_own_expected_state",
    ) {
        return;
    }
    use cases::Profile;
    use fixture::{ExtendedFixture, Shape};
    let a = ExtendedFixture::new(4096, Shape::FlatFiles);
    let b = ExtendedFixture::new(4096, Shape::FlatFiles);
    let c = ExtendedFixture::new(4096, Shape::FlatFiles);
    for profile in Profile::ALL.into_iter().filter(|p| p.tabs()) {
        let companions = if profile == Profile::TabChain {
            vec![&b, &c]
        } else {
            vec![&b]
        };
        for source in profile.sources() {
            eprintln!(
                "EXTENSION_CONTROL_SMOKE {} {}",
                profile.name(),
                source.name()
            );
            let row = driver::run(&a, &companions, profile, source, false, false);
            assert_eq!(row["correct"], true);
            assert_eq!(row["contention_eligible"], true);
        }
    }
}

#[derive(Clone, Default)]
struct MeasuredOwnerProof {
    id: u64,
    allocation_count: usize,
    allocation_tab_correct: bool,
    ledger_count: usize,
    ledger_tab_correct: bool,
    logically_released: bool,
    route_present: bool,
    finished_preemption_cause: bool,
    warm_removal_identity: bool,
}
fn select_measured_request(
    tab: u64,
    live_latest: Option<u64>,
    planned: &[PlannedRequest],
    proof: &MeasuredOwnerProof,
) -> Result<u64, &'static str> {
    let owner = planned
        .iter()
        .filter(|p| p.tab == tab)
        .max_by_key(|p| p.id)
        .ok_or("no measured planned owner")?;
    if owner.id == 0
        || proof.id != owner.id
        || proof.allocation_count != 1
        || !proof.allocation_tab_correct
        || proof.ledger_count != 1
        || !proof.ledger_tab_correct
    {
        return Err("latest planned owner lacks exact allocation and ledger identity");
    }
    match live_latest {
        Some(0) => {
            if !proof.finished_preemption_cause
                && !proof.warm_removal_identity
                && (owner.permission_at.is_none()
                    || owner.permission_reason != Some("switch-evicts-previous-Warm"))
            {
                return Err(
                    "cancellation marker lacks prior latest-owner Warm eviction permission",
                );
            }
        }
        Some(id) if id != owner.id => return Err("live ID is not latest measured planned owner"),
        None if !proof.warm_removal_identity
            && (!proof.logically_released || proof.route_present) =>
        {
            return Err("missing live ID lacks exact latest logical retirement");
        }
        _ => {}
    }
    // This selects the completion owner only. Logical retirement and cancellation
    // permission do not replace successful terminal/source/root or physical debt truth.
    Ok(owner.id)
}
#[test]
fn tc_229_measured_owner_rejects_cancellation_tombstone_as_request() {
    let plans = [PlannedRequest {
        id: 5,
        tab: 1,
        permission_at: Some(Instant::now()),
        permission_reason: Some("switch-evicts-previous-Warm"),
        ..Default::default()
    }];
    let proof = MeasuredOwnerProof {
        id: 5,
        allocation_count: 1,
        allocation_tab_correct: true,
        ledger_count: 1,
        ledger_tab_correct: true,
        logically_released: false,
        route_present: true,
        finished_preemption_cause: false,
        warm_removal_identity: false,
    };
    assert_eq!(select_measured_request(1, Some(0), &plans, &proof), Ok(5));
}

#[test]
fn tc_229_measured_owner_requires_latest_planned_allocation_and_ledger_identity() {
    let plans = [
        PlannedRequest {
            id: 5,
            tab: 1,
            permission_at: Some(Instant::now()),
            permission_reason: Some("switch-evicts-previous-Warm"),
            ..Default::default()
        },
        PlannedRequest {
            id: 8,
            tab: 1,
            ..Default::default()
        },
    ];
    let proof = MeasuredOwnerProof {
        id: 8,
        allocation_count: 1,
        allocation_tab_correct: true,
        ledger_count: 1,
        ledger_tab_correct: true,
        logically_released: false,
        route_present: true,
        finished_preemption_cause: false,
        warm_removal_identity: false,
    };
    assert_eq!(select_measured_request(1, Some(8), &plans, &proof), Ok(8));
    for live in [0, 4, 5, 6, 99] {
        assert!(
            select_measured_request(1, Some(live), &plans, &proof).is_err(),
            "older/setup/foreign/unplanned ID or predecessor-only permission: {live}"
        );
    }
    assert!(select_measured_request(2, Some(8), &plans, &proof).is_err());
    for bad in 0..7 {
        let mut p = proof.clone();
        match bad {
            0 => p.id = 5,
            1 => p.allocation_count = 0,
            2 => p.allocation_count = 2,
            3 => p.allocation_tab_correct = false,
            4 => p.ledger_count = 0,
            5 => p.ledger_count = 2,
            _ => p.ledger_tab_correct = false,
        }
        assert!(
            select_measured_request(1, Some(8), &plans, &p).is_err(),
            "identity fault {bad}"
        );
    }
}
#[test]
fn tc_229_measured_owner_none_requires_exact_latest_logical_retirement() {
    let mut plans = [
        PlannedRequest {
            id: 5,
            tab: 1,
            ..Default::default()
        },
        PlannedRequest {
            id: 8,
            tab: 1,
            ..Default::default()
        },
    ];
    let proof = MeasuredOwnerProof {
        id: 8,
        allocation_count: 1,
        allocation_tab_correct: true,
        ledger_count: 1,
        ledger_tab_correct: true,
        logically_released: true,
        route_present: false,
        finished_preemption_cause: false,
        warm_removal_identity: false,
    };
    assert_eq!(select_measured_request(1, None, &plans, &proof), Ok(8));
    let now = Instant::now();
    let mut observation = crate::app::index_mailbox::IndexPerfObservation {
        admitted_at: Some(now),
        admitted_root: Some(std::path::PathBuf::from("owned-test-root")),
        started_root: Some(std::path::PathBuf::from("owned-test-root")),
        started_source: Some("FileList"),
        terminal_kind: Some("finished"),
        terminal_source: Some("FileList"),
        terminal_published: Some(now),
        ..Default::default()
    };
    let mut truth = ExtendedTruth {
        successful_terminal: observation.terminal_kind == Some("finished"),
        owned_physical_complete: physical_request_complete(&observation),
        owned_request_released: proof.logically_released,
        snapshot_valid: true,
        debt: false,
        latest_results_valid: true,
    };
    assert!(
        !truth.index_ready() && !truth.results_ready(),
        "selected logically retired owner with successful terminal still awaits actual worker return"
    );
    observation.request_processing_returned = Some(now + Duration::from_millis(1));
    truth.owned_physical_complete = physical_request_complete(&observation);
    assert!(truth.index_ready() && truth.results_ready());
    let mut p = proof.clone();
    p.logically_released = false;
    assert!(select_measured_request(1, None, &plans, &p).is_err());
    p = proof.clone();
    p.route_present = true;
    assert!(select_measured_request(1, None, &plans, &p).is_err());
    p = proof.clone();
    p.id = 5;
    assert!(
        select_measured_request(1, None, &plans, &p).is_err(),
        "older release is insufficient"
    );
    assert!(select_measured_request(1, Some(0), &plans, &proof).is_err());
    plans[1].permission_at = Some(Instant::now());
    plans[1].permission_reason = Some("explicit-TabChain-refresh");
    assert!(
        select_measured_request(1, Some(0), &plans, &proof).is_err(),
        "refresh permission does not authorize eviction marker"
    );
    plans[1].permission_reason = Some("switch-evicts-previous-Warm");
    assert_eq!(select_measured_request(1, Some(0), &plans, &proof), Ok(8));
}

#[derive(Clone)]
struct SeedWitness {
    verified: bool,
    id: u64,
    root: std::path::PathBuf,
    source: Source,
    all_count: usize,
    visible_count: usize,
    all_signature: String,
    visible_signature: String,
}
#[derive(Clone)]
struct SwitchAcknowledgement {
    at: Instant,
    tab: u64,
    root: std::path::PathBuf,
    pending_activation: Option<u64>,
}
#[derive(Clone)]
struct VictimDeclaration {
    profile: cases::Profile,
    old_warm_tab: Option<u64>,
    trace_tabs: [u64; 3],
    stage: usize,
    declared_at: Instant,
    closed_at: Option<Instant>,
    victim_id: u64,
    victim_tab: u64,
    active_tab: u64,
    current_warm_tab: u64,
    incoming_ids: Vec<u64>,
    active_root: std::path::PathBuf,
    switch_ack: Option<SwitchAcknowledgement>,
    seed: SeedWitness,
}
#[derive(Clone)]
struct VictimFastState {
    seed_id: Option<u64>,
    root_source_match: bool,
    counts_match: bool,
    latest_without_successor: bool,
    released: bool,
    route_absent: bool,
    worker_load_zero: bool,
    index_debt_zero: bool,
    result_debt_zero: bool,
}
fn declared_preemption_cause(
    planned: &PlannedRequest,
    event: &crate::app::index_coordinator::IndexPerfPreemption,
    observation: &crate::app::index_mailbox::IndexPerfObservation,
) -> bool {
    let Some(c) = &planned.victim else {
        return false;
    };
    let authorized = match (planned.permission_at, planned.permission_reason) {
        (Some(permission), Some("switch-evicts-previous-Warm")) => {
            c.declared_at <= permission && permission <= event.at
        }
        (None, None) => match (
            planned.completed_predecessor_observed_at,
            observation.terminal_published,
        ) {
            (Some(observed), Some(published)) => {
                planned.completed_predecessor
                    && published <= observed
                    && observed <= event.at
                    && observation.terminal_kind == Some("finished")
                    && observation.terminal_offer_kind == Some("finished")
                    && observation.started_source == Some(c.seed.source.name())
                    && observation.terminal_source == Some(c.seed.source.name())
                    && observation.started_root.as_ref() == Some(&c.seed.root)
                    && request_acquisition_owned(observation, &c.seed.root, c.seed.source)
            }
            _ => false,
        },
        _ => false,
    };
    let Some(incoming) = event.pending_active_id else {
        return false;
    };
    let (expected_victim, expected_active, expected_warm) = match c.stage {
        1 => (c.trace_tabs[0], c.trace_tabs[2], c.trace_tabs[1]),
        2 => (c.trace_tabs[1], c.trace_tabs[0], c.trace_tabs[2]),
        _ => return false,
    };
    let unique = event
        .queued_active_ids
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    c.profile == cases::Profile::TabChain
        && (c.victim_tab, c.active_tab, c.current_warm_tab)
            == (expected_victim, expected_active, expected_warm)
        && event.inflight_count >= 2
        && event.queued_active_ids.len() <= 128
        && c.old_warm_tab == Some(c.victim_tab)
        && c.victim_id == planned.id
        && c.victim_tab == planned.tab
        && c.active_tab != c.victim_tab
        && c.current_warm_tab != c.victim_tab
        && c.current_warm_tab != c.active_tab
        && authorized
        && c.declared_at <= event.at
        && c.closed_at.is_none_or(|end| event.at <= end)
        && event.victim_id == planned.id
        && event.victim_tab == planned.tab
        && event.prior_latest == Some(planned.id)
        && event.replacement_id == 0
        && event.active_tab == c.active_tab
        && event.warm_tab == Some(c.current_warm_tab)
        && event.latest_active_id == Some(incoming)
        && c.incoming_ids.contains(&incoming)
        && event.queued_active_ids.contains(&incoming)
        && unique.len() == event.queued_active_ids.len()
        && event
            .queued_active_ids
            .iter()
            .all(|id| c.incoming_ids.contains(id))
}

fn declared_victim_execution(
    planned: &PlannedRequest,
    event: &crate::app::index_coordinator::IndexPerfPreemption,
    observation: &crate::app::index_mailbox::IndexPerfObservation,
) -> bool {
    let Some(c) = &planned.victim else {
        return false;
    };
    let (Some(admitted), Some(started), Some(offered), Some(published), Some(returned)) = (
        observation.admitted_at,
        observation.started_published,
        observation.terminal_offered,
        observation.terminal_published,
        observation.request_processing_returned,
    ) else {
        return false;
    };
    declared_preemption_cause(planned, event, observation)
        && planned.permission_at.is_some()
        && planned.permission_reason == Some("switch-evicts-previous-Warm")
        && admitted <= started
        && started <= event.at
        && event.at <= offered
        && offered <= published
        && published <= returned
        && observation.entries_emitted > 0
        && !observation.skipped_closed_before_start
        && declared_victim_abort_terminal(planned, observation, event.at)
        && request_acquisition_owned(observation, &c.seed.root, c.seed.source)
}

fn declared_preemption_set_valid(
    events: &[crate::app::index_coordinator::IndexPerfPreemption],
    planned: &[PlannedRequest],
    overflow: bool,
    observe: impl Fn(u64, u64) -> Option<crate::app::index_mailbox::IndexPerfObservation>,
) -> bool {
    !overflow
        && events.len() <= 128
        && events.iter().all(|e| {
            let Some(incoming) = e.pending_active_id else {
                return false;
            };
            planned
                .iter()
                .any(|p| p.id == incoming && p.tab == e.active_tab)
                && observe(incoming, e.active_tab).is_some()
                && events
                    .iter()
                    .filter(|other| other.victim_id == e.victim_id)
                    .count()
                    == 1
                && planned
                    .iter()
                    .filter(|p| {
                        p.id == e.victim_id
                            && p.tab == e.victim_tab
                            && observe(p.id, p.tab)
                                .is_some_and(|o| declared_preemption_cause(p, e, &o))
                    })
                    .count()
                    == 1
        })
}
fn last_good_victim_fast(
    planned: &PlannedRequest,
    event: &crate::app::index_coordinator::IndexPerfPreemption,
    observation: &crate::app::index_mailbox::IndexPerfObservation,
    state: &VictimFastState,
) -> bool {
    declared_victim_execution(planned, event, observation) && victim_seed_fast(planned, state)
}
fn victim_seed_fast(planned: &PlannedRequest, state: &VictimFastState) -> bool {
    let Some(c) = &planned.victim else {
        return false;
    };
    c.seed.verified
        && c.seed.id > 0
        && c.seed.id != planned.id
        && state.seed_id == Some(c.seed.id)
        && state.root_source_match
        && state.counts_match
        && state.latest_without_successor
        && planned.revoked_by.is_none()
        && !planned.completed_predecessor
        && state.released
        && state.route_absent
        && state.worker_load_zero
        && state.index_debt_zero
        && state.result_debt_zero
}
fn warm_removal_cause(
    p: &PlannedRequest,
    m: &crate::app::index_coordinator::IndexPerfWarmRemoval,
    o: &crate::app::index_mailbox::IndexPerfObservation,
) -> bool {
    let Some(c) = &p.victim else { return false };
    let Some(ack) = &c.switch_ack else {
        return false;
    };
    let actors = match c.stage {
        1 => (c.trace_tabs[0], c.trace_tabs[2], c.trace_tabs[1]),
        2 => (c.trace_tabs[1], c.trace_tabs[0], c.trace_tabs[2]),
        _ => return false,
    };
    let permission = match (p.permission_at, p.permission_reason) {
        (Some(at), Some("switch-evicts-previous-Warm")) => c.declared_at <= at && at <= m.at,
        (None, None) => {
            p.completed_predecessor
                && o.terminal_kind == Some("finished")
                && o.terminal_offer_kind == Some("finished")
                && o.terminal_source == Some(c.seed.source.name())
                && o.started_source == Some(c.seed.source.name())
                && o.started_root.as_ref() == Some(&c.seed.root)
                && o.terminal_published
                    .zip(p.completed_predecessor_observed_at)
                    .is_some_and(|(pub_at, observed)| pub_at <= observed && observed <= m.at)
        }
        _ => false,
    };
    c.profile == cases::Profile::TabChain
        && (c.victim_tab, c.active_tab, c.current_warm_tab) == actors
        && c.victim_id == p.id
        && c.victim_tab == p.tab
        && c.old_warm_tab == Some(p.tab)
        && p.id > 0
        && m.removed_request_id == p.id
        && m.previous_warm_tab == p.tab
        && m.route_tab == p.tab
        && m.replacement_warm_tab == Some(c.current_warm_tab)
        && c.victim_tab != c.active_tab
        && c.victim_tab != c.current_warm_tab
        && c.active_tab != c.current_warm_tab
        && permission
        && c.declared_at <= m.at
        && m.at <= ack.at
        && ack.tab == c.active_tab
        && ack.root == c.active_root
        && ack.pending_activation.is_none()
        && c.closed_at.is_none_or(|end| ack.at <= end)
        && request_acquisition_owned(o, &c.seed.root, c.seed.source)
}
fn declared_victim_abort_terminal(
    p: &PlannedRequest,
    o: &crate::app::index_mailbox::IndexPerfObservation,
    earliest: Instant,
) -> bool {
    if o.stale_full_data_abort_duplicate {
        return false;
    }
    if o.terminal_kind == Some("canceled") && o.terminal_offer_kind == Some("canceled") {
        return o.terminal_offer_current == Some(false);
    }
    let Some(c) = &p.victim else { return false };
    let Some(a) = &o.stale_full_data_abort else {
        return false;
    };
    c.profile == cases::Profile::TabChain
        && c.stage == 2
        && c.victim_id == p.id
        && c.victim_tab == p.tab
        && c.trace_tabs[1] == p.tab
        && p.revoked_by.is_none()
        && !p.completed_predecessor
        && !o.stale_full_data_abort_duplicate
        && a.request_id == p.id
        && a.tab_id == p.tab
        && a.response_request_id == p.id
        && matches!(a.data_kind, "batch" | "replace-all")
        && a.latest_lookup_succeeded
        && !a.shutdown
        && a.latest_id != Some(p.id)
        && earliest <= a.at
        && o.terminal_offered.is_some_and(|offer| a.at <= offer)
        && o.terminal_kind == Some("failed")
        && o.terminal_offer_kind == Some("failed")
        && o.terminal_offer_error.as_deref() == Some("index receiver closed")
        && o.terminal_offer_current == Some(false)
        && o.data_publish_end.is_none()
}
fn warm_victim_execution(
    p: &PlannedRequest,
    m: &crate::app::index_coordinator::IndexPerfWarmRemoval,
    o: &crate::app::index_mailbox::IndexPerfObservation,
) -> bool {
    warm_removal_cause(p, m, o)
        && p.permission_at.is_some()
        && p.permission_reason == Some("switch-evicts-previous-Warm")
        && o.admitted_at
            .zip(o.started_published)
            .is_some_and(|(a, b)| a <= b)
        && o.started_published.is_some_and(|at| at <= m.at)
        && o.terminal_offered.is_some_and(|at| m.at <= at)
        && o.terminal_offered
            .zip(o.terminal_published)
            .is_some_and(|(a, b)| a <= b)
        && o.terminal_published
            .zip(o.request_processing_returned)
            .is_some_and(|(a, b)| a <= b)
        && o.entries_emitted > 0
        && !o.skipped_closed_before_start
        && declared_victim_abort_terminal(p, o, m.at)
}
fn warm_last_good_victim_fast(
    p: &PlannedRequest,
    m: &crate::app::index_coordinator::IndexPerfWarmRemoval,
    o: &crate::app::index_mailbox::IndexPerfObservation,
    s: &VictimFastState,
) -> bool {
    warm_victim_execution(p, m, o) && victim_seed_fast(p, s)
}
fn warm_preempt_followup(
    p: &PlannedRequest,
    m: &crate::app::index_coordinator::IndexPerfWarmRemoval,
    e: &crate::app::index_coordinator::IndexPerfPreemption,
    o: &crate::app::index_mailbox::IndexPerfObservation,
) -> bool {
    let Some(c) = &p.victim else { return false };
    let Some(incoming) = e.pending_active_id else {
        return false;
    };
    let ids = e
        .queued_active_ids
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    warm_removal_cause(p, m, o)
        && m.at <= e.at
        && c.closed_at.is_none_or(|end| e.at <= end)
        && e.prior_latest.is_none()
        && e.victim_id == p.id
        && e.victim_tab == p.tab
        && e.replacement_id == 0
        && e.active_tab == c.active_tab
        && e.warm_tab == Some(c.current_warm_tab)
        && e.inflight_count >= 2
        && e.queued_active_ids.len() <= 128
        && ids.len() == e.queued_active_ids.len()
        && e.latest_active_id == Some(incoming)
        && e.queued_active_ids.contains(&incoming)
        && c.incoming_ids.contains(&incoming)
        && e.queued_active_ids
            .iter()
            .all(|id| c.incoming_ids.contains(id))
}
fn seed_entry_signature(root: &std::path::Path, entries: &[Entry]) -> String {
    format!("{:016x}", signature(root, entries.iter().map(|e| &e.path)))
}
fn full_retained_seed_valid(
    seed: &SeedWitness,
    oracle: &oracle::ExtendedOracle,
    all: &[Entry],
    visible: &[Entry],
) -> bool {
    seed.verified
        && all.len() == seed.all_count
        && visible.len() == seed.visible_count
        && oracle.valid_snapshot(all, visible)
        && seed_entry_signature(&seed.root, all) == seed.all_signature
        && seed_entry_signature(&seed.root, visible) == seed.visible_signature
}

fn victim_test_inputs() -> (
    PlannedRequest,
    crate::app::index_coordinator::IndexPerfPreemption,
    crate::app::index_mailbox::IndexPerfObservation,
    VictimFastState,
) {
    let t = Instant::now();
    let root = std::path::PathBuf::from("owned-victim-root");
    let planned = PlannedRequest {
        id: 6,
        tab: 2,
        permission_at: Some(t + Duration::from_millis(3)),
        permission_reason: Some("switch-evicts-previous-Warm"),
        victim: Some(VictimDeclaration {
            profile: cases::Profile::TabChain,
            old_warm_tab: Some(2),
            trace_tabs: [1, 2, 3],
            stage: 2,
            declared_at: t + Duration::from_millis(2),
            closed_at: None,
            victim_id: 6,
            victim_tab: 2,
            active_tab: 1,
            current_warm_tab: 3,
            incoming_ids: vec![8],
            active_root: "owned-active-root".into(),
            switch_ack: None,
            seed: SeedWitness {
                verified: true,
                id: 3,
                root: root.clone(),
                source: Source::FileList,
                all_count: 100000,
                visible_count: 100000,
                all_signature: "seed-all".into(),
                visible_signature: "seed-visible".into(),
            },
        }),
        ..Default::default()
    };
    let event = crate::app::index_coordinator::IndexPerfPreemption {
        at: t + Duration::from_millis(5),
        victim_id: 6,
        victim_tab: 2,
        prior_latest: Some(6),
        replacement_id: 0,
        active_tab: 1,
        warm_tab: Some(3),
        pending_active_id: Some(8),
        latest_active_id: Some(8),
        queued_active_ids: vec![8],
        inflight_count: 2,
    };
    let observation = crate::app::index_mailbox::IndexPerfObservation {
        entries_emitted: 58368,
        admitted_at: Some(t),
        admitted_root: Some(root.clone()),
        started_source: Some("FileList"),
        started_root: Some(root),
        started_published: Some(t + Duration::from_millis(1)),
        terminal_kind: Some("canceled"),
        terminal_published: Some(t + Duration::from_millis(6)),
        terminal_offer_kind: Some("canceled"),
        terminal_offer_current: Some(false),
        terminal_offered: Some(t + Duration::from_millis(6)),
        request_processing_returned: Some(t + Duration::from_millis(7)),
        ..Default::default()
    };
    let state = VictimFastState {
        seed_id: Some(3),
        root_source_match: true,
        counts_match: true,
        latest_without_successor: true,
        released: true,
        route_absent: true,
        worker_load_zero: true,
        index_debt_zero: true,
        result_debt_zero: true,
    };
    (planned, event, observation, state)
}
#[test]
fn tc_229_finished_observation_before_mutation_needs_no_cancel_permission() {
    let (mut p, e, mut o, _) = victim_test_inputs();
    p.permission_at = None;
    p.permission_reason = None;
    p.completed_predecessor = true;
    p.completed_predecessor_observed_at = Some(e.at - Duration::from_micros(500));
    o.terminal_kind = Some("finished");
    o.terminal_offer_kind = Some("finished");
    o.terminal_source = Some("FileList");
    o.terminal_offered = Some(e.at - Duration::from_millis(1));
    o.terminal_published = o.terminal_offered;
    o.request_processing_returned = Some(e.at + Duration::from_millis(1));
    let incoming = PlannedRequest {
        id: 8,
        tab: 1,
        ..Default::default()
    };
    let valid = |p: &PlannedRequest,
                 o: &crate::app::index_mailbox::IndexPerfObservation,
                 e: &crate::app::index_coordinator::IndexPerfPreemption| {
        declared_preemption_set_valid(
            std::slice::from_ref(e),
            &[p.clone(), incoming.clone()],
            false,
            |id, _| {
                Some(if id == 6 {
                    o.clone()
                } else {
                    Default::default()
                })
            },
        )
    };
    assert!(
        valid(&p, &o, &e),
        "published Finished after declaration and observed before mutation needs no Cancel permission"
    );
    let mut early = o.clone();
    early.terminal_offered =
        Some(p.victim.as_ref().unwrap().declared_at - Duration::from_micros(500));
    early.terminal_published = early.terminal_offered;
    assert!(
        valid(&p, &early, &e),
        "already Finished before declaration also remains valid"
    );
    let mut unfinished = o.clone();
    unfinished.request_processing_returned = None;
    assert!(
        valid(&p, &unfinished, &e),
        "typed Finished mutation cause precedes physical settlement"
    );
    let mut latest = [RequestProof {
        id: p.id,
        tab: p.tab,
        finished: true,
        physical_complete: physical_request_complete(&unfinished),
        source_root_correct: true,
        revoked_abort: false,
        last_good_victim: false,
    }];
    assert!(!planned_requests_valid(std::slice::from_ref(&p), &latest));
    latest[0].physical_complete = physical_request_complete(&o);
    assert!(planned_requests_valid(std::slice::from_ref(&p), &latest));
    for mutation in 0..10 {
        let (mut p, mut o, mut e) = (p.clone(), o.clone(), e.clone());
        match mutation {
            0 => p.completed_predecessor = false,
            1 => p.completed_predecessor_observed_at = None,
            2 => o.terminal_published = None,
            3 => o.terminal_published = Some(e.at),
            4 => p.completed_predecessor_observed_at = Some(e.at + Duration::from_micros(1)),
            5 => o.terminal_kind = Some("canceled"),
            6 => o.started_root = Some("wrong-root".into()),
            7 => o.terminal_source = Some("Walker"),
            8 => e.pending_active_id = Some(99),
            9 => p.victim.as_mut().unwrap().stage = 1,
            _ => unreachable!(),
        }
        assert!(
            !valid(&p, &o, &e),
            "invalid Finished observation mutation {mutation}"
        );
    }
}

#[test]
fn tc_229_preemption_finished_predecessor_race_preserves_composed_completion() {
    let (mut b, mut eb, mut ob, sb) = victim_test_inputs();
    let mut a = b.clone();
    a.id = 5;
    a.tab = 1;
    a.revoked_by = Some(8);
    let ca = a.victim.as_mut().unwrap();
    ca.stage = 1;
    ca.old_warm_tab = Some(1);
    ca.victim_id = 5;
    ca.victim_tab = 1;
    ca.active_tab = 3;
    ca.current_warm_tab = 2;
    ca.incoming_ids = vec![7];
    ca.closed_at = Some(eb.at + Duration::from_millis(1));
    let mut ea = eb.clone();
    ea.victim_id = 5;
    ea.victim_tab = 1;
    ea.prior_latest = Some(5);
    ea.active_tab = 3;
    ea.warm_tab = Some(2);
    ea.pending_active_id = Some(7);
    ea.latest_active_id = Some(7);
    ea.queued_active_ids = vec![7];
    let mut oa = ob.clone();
    oa.terminal_kind = Some("finished");
    oa.terminal_offer_kind = Some("finished");
    oa.terminal_offer_current = Some(true);
    oa.terminal_source = Some("FileList");
    // Finished and even physical return can race ahead of the scheduler mutation.
    oa.terminal_offered = Some(ea.at - Duration::from_millis(1));
    oa.terminal_published = oa.terminal_offered;
    oa.request_processing_returned = oa.terminal_offered;
    let later = Duration::from_millis(10);
    b.permission_at = b.permission_at.map(|t| t + later);
    b.victim.as_mut().unwrap().declared_at += later;
    eb.at += later;
    ob.terminal_offered = ob.terminal_offered.map(|t| t + later);
    ob.terminal_published = ob.terminal_published.map(|t| t + later);
    ob.request_processing_returned = ob.request_processing_returned.map(|t| t + later);
    let plans = [
        a,
        b,
        PlannedRequest {
            id: 7,
            tab: 3,
            ..Default::default()
        },
        PlannedRequest {
            id: 8,
            tab: 1,
            ..Default::default()
        },
    ];
    let mut proofs = [
        RequestProof {
            id: 5,
            tab: 1,
            finished: true,
            physical_complete: true,
            source_root_correct: true,
            revoked_abort: false,
            last_good_victim: false,
        },
        RequestProof {
            id: 6,
            tab: 2,
            finished: false,
            physical_complete: true,
            source_root_correct: false,
            revoked_abort: false,
            last_good_victim: last_good_victim_fast(&plans[1], &eb, &ob, &sb),
        },
        RequestProof {
            id: 7,
            tab: 3,
            finished: true,
            physical_complete: true,
            source_root_correct: true,
            revoked_abort: false,
            last_good_victim: false,
        },
        RequestProof {
            id: 8,
            tab: 1,
            finished: true,
            physical_complete: true,
            source_root_correct: true,
            revoked_abort: false,
            last_good_victim: false,
        },
    ];
    let observe = |id, _tab| {
        Some(if id == 5 {
            oa.clone()
        } else if id == 6 {
            ob.clone()
        } else {
            Default::default()
        })
    };
    let events = [ea, eb];
    let composed = |ps: &[RequestProof],
                    es: &[crate::app::index_coordinator::IndexPerfPreemption]| {
        let mut actual_proofs = ps.to_vec();
        let victim_events = es.iter().filter(|e| e.victim_id == 6).collect::<Vec<_>>();
        actual_proofs[1].last_good_victim &= victim_events.len() == 1
            && last_good_victim_fast(&plans[1], victim_events[0], &ob, &sb);
        declared_preemption_set_valid(es, &plans, false, observe)
            && planned_requests_valid(&plans, &actual_proofs)
    };
    assert!(
        composed(&proofs, &events),
        "owned Finished predecessor must not be forced into Canceled chronology"
    );
    let mut wrong = events.clone();
    wrong[0].pending_active_id = Some(8);
    assert!(!composed(&proofs, &wrong));
    wrong = events.clone();
    wrong[0].victim_id = 99;
    assert!(!composed(&proofs, &wrong));
    assert!(
        !composed(&proofs, &events[..1]),
        "latest canceled B cannot retain last-good without its actual event"
    );
    for required in [2, 3] {
        proofs[required].finished = false;
        assert!(!composed(&proofs, &events));
        proofs[required].finished = true;
    }
    proofs[0].finished = false;
    assert!(
        !composed(&proofs, &events),
        "unproven predecessor abort cannot hide behind actual mutation"
    );
    proofs[0].revoked_abort = true;
    assert!(
        composed(&proofs, &events),
        "strict existing authorized predecessor abort remains valid"
    );
}

#[test]
fn tc_229_declared_tabchain_victim_preserves_required_latest_success() {
    let (victim, event, observation, state) = victim_test_inputs();
    let plans = [
        victim,
        PlannedRequest {
            id: 7,
            tab: 3,
            ..Default::default()
        },
        PlannedRequest {
            id: 8,
            tab: 1,
            ..Default::default()
        },
    ];
    let mut proofs = [
        RequestProof {
            id: 6,
            tab: 2,
            finished: false,
            physical_complete: true,
            source_root_correct: false,
            revoked_abort: false,
            last_good_victim: last_good_victim_fast(&plans[0], &event, &observation, &state),
        },
        RequestProof {
            id: 7,
            tab: 3,
            finished: true,
            physical_complete: true,
            source_root_correct: true,
            revoked_abort: false,
            last_good_victim: false,
        },
        RequestProof {
            id: 8,
            tab: 1,
            finished: true,
            physical_complete: true,
            source_root_correct: true,
            revoked_abort: false,
            last_good_victim: false,
        },
    ];
    assert!(planned_requests_valid(&plans, &proofs));
    for required in [1, 2] {
        proofs[required].finished = false;
        assert!(!planned_requests_valid(&plans, &proofs));
        proofs[required].last_good_victim = true;
        assert!(!planned_requests_valid(&plans, &proofs));
        proofs[required].finished = true;
        proofs[required].last_good_victim = false;
    }
    proofs[0].finished = true;
    assert!(
        !planned_requests_valid(&plans, &proofs),
        "Finished B cannot use seed fallback even if victim flag supplied"
    );
    proofs[0].source_root_correct = true;
    assert!(planned_requests_valid(&plans, &proofs));
}
#[test]
fn tc_229_tabchain_victim_rejects_unowned_cause_chronology_and_terminal() {
    let (p, e, o, s) = victim_test_inputs();
    assert!(last_good_victim_fast(&p, &e, &o, &s));
    let owned = [
        p.clone(),
        PlannedRequest {
            id: 8,
            tab: 1,
            ..Default::default()
        },
    ];
    let observe = |id, tab| {
        if (id, tab) == (6, 2) {
            Some(o.clone())
        } else if (id, tab) == (8, 1) {
            Some(Default::default())
        } else {
            None
        }
    };
    assert!(declared_preemption_set_valid(
        std::slice::from_ref(&e),
        &owned,
        false,
        observe
    ));
    assert!(!declared_preemption_set_valid(
        &[e.clone(), e.clone()],
        &owned,
        false,
        observe
    ));
    assert!(!declared_preemption_set_valid(
        std::slice::from_ref(&e),
        &owned,
        true,
        observe
    ));
    assert!(
        !declared_preemption_set_valid(
            std::slice::from_ref(&e),
            std::slice::from_ref(&p),
            false,
            observe
        ),
        "unplanned incoming request"
    );
    let mut foreign = e.clone();
    foreign.victim_id = 99;
    assert!(!declared_preemption_set_valid(
        &[foreign],
        &owned,
        false,
        observe
    ));
    for bad in 0..22 {
        let mut p = p.clone();
        let mut e = e.clone();
        let mut o = o.clone();
        match bad {
            0 => p.victim = None,
            1 => p.victim.as_mut().unwrap().profile = cases::Profile::Warm,
            2 => p.victim.as_mut().unwrap().stage = 1,
            3 => p.victim.as_mut().unwrap().old_warm_tab = Some(99),
            4 => e.victim_id = 99,
            5 => e.victim_tab = 99,
            6 => e.prior_latest = Some(5),
            7 => e.replacement_id = 9,
            8 => e.active_tab = 99,
            9 => e.warm_tab = Some(2),
            10 => e.pending_active_id = Some(99),
            11 => e.latest_active_id = Some(99),
            12 => e.queued_active_ids = vec![99],
            13 => e.queued_active_ids = vec![8, 8],
            14 => p.permission_at = None,
            15 => p.permission_at = Some(e.at + Duration::from_secs(1)),
            16 => p.permission_reason = Some("explicit-TabChain-refresh"),
            17 => p.victim.as_mut().unwrap().closed_at = Some(e.at - Duration::from_millis(1)),
            18 => o.terminal_kind = Some("failed"),
            19 => o.terminal_offer_current = Some(true),
            20 => o.terminal_offered = Some(e.at - Duration::from_millis(1)),
            _ => o.request_processing_returned = None,
        }
        assert!(
            !last_good_victim_fast(&p, &e, &o, &s),
            "cause/chronology fault {bad}"
        );
    }
}
#[test]
fn tc_229_tabchain_victim_fast_rejects_wrong_seed_and_unfinished_debt() {
    let (p, e, o, s) = victim_test_inputs();
    assert!(last_good_victim_fast(&p, &e, &o, &s));
    for bad in 0..18 {
        let mut p = p.clone();
        let mut o = o.clone();
        let mut s = s.clone();
        match bad {
            0 => s.seed_id = Some(6),
            1 => s.root_source_match = false,
            2 => s.counts_match = false,
            3 => s.latest_without_successor = false,
            4 => s.released = false,
            5 => s.route_absent = false,
            6 => s.worker_load_zero = false,
            7 => s.index_debt_zero = false,
            8 => s.result_debt_zero = false,
            9 => p.victim.as_mut().unwrap().seed.verified = false,
            10 => o.started_source = Some("Walker"),
            11 => o.started_root = Some(std::path::PathBuf::from("foreign")),
            12 => o.admitted_root = Some(std::path::PathBuf::from("foreign")),
            13 => o.started_published = None,
            14 => o.entries_emitted = 0,
            15 => o.terminal_kind = Some("finished"),
            16 => o.skipped_closed_before_start = true,
            _ => o.request_processing_returned = Some(e.at - Duration::from_millis(1)),
        }
        assert!(
            !last_good_victim_fast(&p, &e, &o, &s),
            "seed/debt fault {bad}"
        );
    }
}

#[test]
fn tc_229_scalar_fast_ready_corrupt_full_seed_cannot_emit_correct_sample() {
    let fixture = fixture::ExtendedFixture::new(16, fixture::Shape::FlatFiles);
    let oracle = oracle::ExtendedOracle::new(
        &fixture,
        Source::FileList,
        driver::default_filter(),
        "",
        ResultSortMode::Score,
        ResultSortScope::ShownResults,
        1000,
    );
    let all = fixture
        .expected
        .iter()
        .map(|r| Entry::file(r.path.clone()))
        .collect::<Vec<_>>();
    let mut inputs = victim_test_inputs();
    let seed = &mut inputs.0.victim.as_mut().unwrap().seed;
    seed.root = fixture.root.clone();
    seed.all_count = all.len();
    seed.visible_count = all.len();
    seed.all_signature = seed_entry_signature(&fixture.root, &all);
    seed.visible_signature = seed.all_signature.clone();
    inputs.2.admitted_root = Some(fixture.root.clone());
    inputs.2.started_root = Some(fixture.root.clone());
    assert!(last_good_victim_fast(
        &inputs.0, &inputs.1, &inputs.2, &inputs.3
    ));
    let removal = crate::app::index_coordinator::IndexPerfWarmRemoval {
        at: inputs.1.at - Duration::from_micros(500),
        removed_request_id: inputs.0.id,
        previous_warm_tab: inputs.0.tab,
        replacement_warm_tab: Some(3),
        route_tab: inputs.0.tab,
    };
    let c = inputs.0.victim.as_mut().unwrap();
    c.switch_ack = Some(SwitchAcknowledgement {
        at: inputs.1.at,
        tab: c.active_tab,
        root: c.active_root.clone(),
        pending_activation: None,
    });
    assert!(warm_last_good_victim_fast(
        &inputs.0, &removal, &inputs.2, &inputs.3
    ));
    let seed = &inputs.0.victim.as_ref().unwrap().seed;
    assert!(full_retained_seed_valid(seed, &oracle, &all, &all));
    for bad in 0..5 {
        let mut corrupt = all.clone();
        match bad {
            0 => corrupt[0] = Entry::file(fixture.root.join("foreign")),
            1 => corrupt[0] = Entry::dir(corrupt[0].path.clone()),
            2 => corrupt.swap(0, 1),
            3 => {
                corrupt.pop();
            }
            _ => corrupt[0] = corrupt[1].clone(),
        }
        let can_emit_correct = last_good_victim_fast(&inputs.0, &inputs.1, &inputs.2, &inputs.3)
            && full_retained_seed_valid(seed, &oracle, &corrupt, &corrupt);
        assert!(
            !(warm_last_good_victim_fast(&inputs.0, &removal, &inputs.2, &inputs.3)
                && full_retained_seed_valid(seed, &oracle, &corrupt, &corrupt)),
            "Warm first-cause scalar readiness cannot substitute full seed oracle {bad}"
        );
        assert!(
            !can_emit_correct,
            "scalar readiness never substitutes the independent post-t3 seed oracle fault {bad}"
        );
    }
}

#[test]
fn tc_229_warm_first_cause_requires_actual_execution_and_seed_truth() {
    let (mut p, e, o, s) = victim_test_inputs();
    let m = crate::app::index_coordinator::IndexPerfWarmRemoval {
        at: e.at - Duration::from_micros(500),
        removed_request_id: p.id,
        previous_warm_tab: p.tab,
        replacement_warm_tab: Some(3),
        route_tab: p.tab,
    };
    let c = p.victim.as_mut().unwrap();
    c.switch_ack = Some(SwitchAcknowledgement {
        at: e.at,
        tab: c.active_tab,
        root: c.active_root.clone(),
        pending_activation: None,
    });
    assert!(warm_last_good_victim_fast(&p, &m, &o, &s));
    for bad in 0..18 {
        let mut p = p.clone();
        let mut o = o.clone();
        let mut s = s.clone();
        match bad {
            0 => o.terminal_offered = Some(m.at - Duration::from_micros(1)),
            1 => o.request_processing_returned = None,
            2 => o.terminal_published = None,
            3 => o.terminal_kind = Some("finished"),
            4 => o.terminal_offer_kind = Some("finished"),
            5 => o.terminal_offer_current = Some(true),
            6 => o.entries_emitted = 0,
            7 => o.started_published = None,
            8 => o.admitted_root = Some("foreign".into()),
            9 => p.completed_predecessor = true,
            10 => p.revoked_by = Some(8),
            11 => s.seed_id = Some(p.id),
            12 => s.root_source_match = false,
            13 => s.counts_match = false,
            14 => s.latest_without_successor = false,
            15 => s.route_absent = false,
            16 => s.index_debt_zero = false,
            _ => s.released = false,
        }
        assert!(
            !warm_last_good_victim_fast(&p, &m, &o, &s),
            "first cause/physical/latest seed fault {bad}"
        );
    }
    let mut finished = p.clone();
    finished.permission_at = None;
    finished.permission_reason = None;
    finished.completed_predecessor = true;
    finished.completed_predecessor_observed_at = Some(m.at - Duration::from_micros(1));
    let mut actual = o.clone();
    actual.terminal_kind = Some("finished");
    actual.terminal_offer_kind = Some("finished");
    actual.terminal_source = Some("FileList");
    actual.terminal_published = Some(m.at - Duration::from_micros(2));
    assert!(warm_removal_cause(&finished, &m, &actual));
    for bad in 0..5 {
        let mut p = finished.clone();
        let mut o = actual.clone();
        match bad {
            0 => o.terminal_published = None,
            1 => p.completed_predecessor_observed_at = None,
            2 => o.terminal_published = Some(m.at),
            3 => p.completed_predecessor_observed_at = Some(m.at + Duration::from_micros(1)),
            _ => p.completed_predecessor = false,
        }
        assert!(
            !warm_removal_cause(&p, &m, &o),
            "unobserved Finished cause fault {bad}"
        );
    }
    assert!(
        !warm_last_good_victim_fast(&finished, &m, &actual, &s),
        "Finished B cannot use old seed fallback"
    );
}

#[test]
fn tc_229_stale_full_failed_victim_rejects_unproven_abort() {
    let (p, e, mut o, s) = victim_test_inputs();
    o.terminal_kind = Some("failed");
    o.terminal_offer_kind = Some("failed");
    o.terminal_offer_error = Some("index receiver closed".into());
    o.stale_full_data_abort = Some(crate::app::index_mailbox::IndexPerfStaleFullDataAbort {
        request_id: p.id,
        tab_id: p.tab,
        response_request_id: p.id,
        data_kind: "batch",
        at: e.at,
        latest_id: None,
        latest_lookup_succeeded: true,
        shutdown: false,
    });
    assert!(last_good_victim_fast(&p, &e, &o, &s));
    for mutation in 0..32 {
        let (mut p, mut e, mut o, mut s) = (p.clone(), e.clone(), o.clone(), s.clone());
        match mutation {
            0 => o.stale_full_data_abort = None,
            1 => o.stale_full_data_abort_duplicate = true,
            2 => o.stale_full_data_abort.as_mut().unwrap().request_id += 1,
            3 => o.stale_full_data_abort.as_mut().unwrap().tab_id += 1,
            4 => {
                o.stale_full_data_abort
                    .as_mut()
                    .unwrap()
                    .response_request_id += 1
            }
            5 => o.stale_full_data_abort.as_mut().unwrap().data_kind = "terminal",
            6 => o.stale_full_data_abort.as_mut().unwrap().shutdown = true,
            7 => {
                o.stale_full_data_abort
                    .as_mut()
                    .unwrap()
                    .latest_lookup_succeeded = false
            }
            8 => o.stale_full_data_abort.as_mut().unwrap().latest_id = Some(p.id),
            9 => o.stale_full_data_abort.as_mut().unwrap().at = e.at - Duration::from_micros(1),
            10 => {
                o.stale_full_data_abort.as_mut().unwrap().at =
                    o.terminal_offered.unwrap() + Duration::from_micros(1)
            }
            11 => o.terminal_offer_error = Some("other error".into()),
            12 => o.terminal_offer_current = Some(true),
            13 => o.terminal_kind = Some("finished"),
            14 => o.terminal_offer_kind = Some("canceled"),
            15 => o.terminal_published = None,
            16 => o.request_processing_returned = None,
            17 => o.entries_emitted = 0,
            18 => o.skipped_closed_before_start = true,
            19 => s.released = false,
            20 => s.index_debt_zero = false,
            21 => s.result_debt_zero = false,
            22 => s.route_absent = false,
            23 => s.worker_load_zero = false,
            24 => p.revoked_by = Some(8),
            25 => p.victim.as_mut().unwrap().stage = 1,
            26 => p.permission_at = Some(e.at + Duration::from_micros(1)),
            27 => o.started_root = Some("foreign".into()),
            28 => o.started_source = Some("Walker"),
            29 => p.completed_predecessor = true,
            30 => o.data_publish_end = Some(e.at),
            31 => e.at += Duration::from_micros(1),
            _ => unreachable!(),
        }
        assert!(
            !last_good_victim_fast(&p, &e, &o, &s),
            "invalid stale Full abort mutation {mutation}"
        );
    }
    let mut proofs = [
        RequestProof {
            id: 6,
            tab: 2,
            finished: false,
            physical_complete: true,
            source_root_correct: false,
            revoked_abort: false,
            last_good_victim: true,
        },
        RequestProof {
            id: 8,
            tab: 1,
            finished: false,
            physical_complete: true,
            source_root_correct: false,
            revoked_abort: false,
            last_good_victim: false,
        },
    ];
    let plans = [
        p.clone(),
        PlannedRequest {
            id: 8,
            tab: 1,
            ..Default::default()
        },
    ];
    assert!(
        !planned_requests_valid(&plans, &proofs),
        "required A Failed cannot borrow B abort exception"
    );
    let mut c_plans = plans.clone();
    c_plans[1].id = 7;
    c_plans[1].tab = 3;
    let mut c_proofs = proofs.clone();
    c_proofs[1].id = 7;
    c_proofs[1].tab = 3;
    assert!(
        !planned_requests_valid(&c_plans, &c_proofs),
        "required C Failed cannot borrow B abort exception"
    );
    proofs[1].finished = true;
    proofs[1].source_root_correct = true;
    assert!(planned_requests_valid(&plans, &proofs));
    let mut o = o.clone();
    o.terminal_kind = Some("finished");
    o.terminal_offer_kind = Some("finished");
    assert!(
        !last_good_victim_fast(&p, &e, &o, &s),
        "Finished B must use newest generation, never seed fallback"
    );
}

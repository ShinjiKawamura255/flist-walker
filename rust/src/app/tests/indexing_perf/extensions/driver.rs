use super::cases::Profile;
use super::fixture::{EmptyRootPublication, ExtendedFixture};
use super::oracle::{ExtendedOracle, Filter};
use super::*;

struct OwnedRequest {
    id: u64,
    tab: u64,
    allocated_at: Instant,
    observation: crate::app::index_mailbox::IndexPerfHandle,
}
impl OwnedRequest {
    fn observation(&self) -> crate::app::index_mailbox::IndexPerfObservation {
        self.observation.lock().expect("index observation").clone()
    }
}
fn collect_requests(driver: &Driver, ledger: &mut Vec<OwnedRequest>) {
    for a in &driver.app.shell.indexing.perf_allocations {
        if !ledger.iter().any(|r| r.id == a.id) {
            ledger.push(OwnedRequest {
                id: a.id,
                tab: a.tab.expect("owned tab"),
                allocated_at: a.at,
                observation: Arc::clone(&a.observation),
            });
        }
    }
}
fn record_planned_new(driver: &Driver, plans: &mut Vec<PlannedRequest>, tab: u64, count: usize) {
    let new = driver
        .app
        .shell
        .indexing
        .perf_allocations
        .iter()
        .filter(|a| !plans.iter().any(|p| p.id == a.id))
        .collect::<Vec<_>>();
    assert_eq!(
        new.len(),
        count,
        "unexpected index allocation count for owned operation/tab={tab}"
    );
    for a in new {
        assert_eq!(a.tab, Some(tab), "request allocated for unexpected tab");
        for declaration in plans.iter_mut().filter_map(|p| p.victim.as_mut()) {
            if declaration.closed_at.is_none()
                && declaration.active_tab == tab
                && a.at >= declaration.declared_at
            {
                assert!(!declaration.incoming_ids.contains(&a.id));
                declaration.incoming_ids.push(a.id);
            }
        }
        if let Some(previous) = plans.iter_mut().rev().find(|p| p.tab == tab) {
            let observed = driver
                .app
                .shell
                .indexing
                .perf_allocations
                .iter()
                .find(|r| r.id == previous.id)
                .unwrap()
                .observation
                .lock()
                .unwrap();
            if observed.terminal_kind == Some("finished") {
                previous.completed_predecessor = true;
            } else {
                assert!(
                    previous.permission_at.is_some(),
                    "no planned revocation permission for predecessor {}",
                    previous.id
                );
                assert!(previous.revoked_by.is_none());
                previous.revoked_by = Some(a.id);
            }
        }
        plans.push(PlannedRequest {
            id: a.id,
            tab,
            revoked_by: None,
            ..Default::default()
        });
    }
}
fn permit_revocation(
    driver: &Driver,
    plans: &mut [PlannedRequest],
    tab: u64,
    reason: &'static str,
) {
    let Some(p) = plans.iter_mut().rev().find(|p| p.tab == tab) else {
        return;
    };
    if p.permission_at.is_some() {
        return;
    }
    let a = driver
        .app
        .shell
        .indexing
        .perf_allocations
        .iter()
        .find(|a| a.id == p.id)
        .unwrap();
    let o = a.observation.lock().unwrap();
    if o.terminal_kind == Some("finished") {
        let _published = o.terminal_published.expect("observed Finished publication");
        p.completed_predecessor = true;
        p.completed_predecessor_observed_at
            .get_or_insert_with(Instant::now);
        return;
    }
    assert!(
        !o.mailbox_closed
            && !matches!(o.terminal_kind, Some("canceled" | "failed"))
            && !matches!(o.terminal_offer_kind, Some("canceled" | "failed")),
        "operation cannot retroactively authorize an aborted request {}: {o:?}",
        p.id
    );
    p.permission_at = Some(Instant::now());
    p.permission_reason = Some(reason);
}
fn refresh_allocation(
    driver: &Driver,
    plans: &mut Vec<PlannedRequest>,
    deferred: &mut HashMap<u64, usize>,
    tab: u64,
) {
    let count = driver
        .app
        .shell
        .indexing
        .perf_allocations
        .iter()
        .filter(|a| !plans.iter().any(|p| p.id == a.id))
        .count();
    if count == 0 {
        assert_eq!(driver.app.current_tab_id(), Some(tab));
        assert!(
            driver
                .app
                .shell
                .indexing
                .refresh_after_pending_finish
                .is_some(),
            "missing immediate or planned deferred refresh"
        );
        // A second click while the same production deferred refresh flag is set
        // coalesces, rather than authorizing an extra request.
        deferred.insert(tab, 1);
    } else {
        record_planned_new(driver, plans, tab, 1);
    }
}
fn capture_deferred(
    driver: &Driver,
    plans: &mut Vec<PlannedRequest>,
    deferred: &mut HashMap<u64, usize>,
) {
    let arrivals = driver
        .app
        .shell
        .indexing
        .perf_allocations
        .iter()
        .filter(|a| !plans.iter().any(|p| p.id == a.id))
        .map(|a| a.tab.unwrap())
        .collect::<Vec<_>>();
    for tab in arrivals {
        assert_eq!(deferred.remove(&tab), Some(1), "unplanned frame allocation");
        record_planned_new(driver, plans, tab, 1);
    }
}
#[derive(Debug, PartialEq)]
enum SwitchAdmission {
    Activated(bool),
    WaitForFinishedWarm(u64),
}
#[derive(Clone, Copy, PartialEq)]
enum WarmSwitchPolicy {
    Unrestricted,
    ObservedCompletion,
    CommittedSmall,
}
pub(super) fn tabchain_input_policy(full: bool) -> &'static str {
    if full {
        "observed-completion (data-end/Finished-offer/publication) waits for own committed snapshot before eviction; t0-included production frames; final unobserved publication/removal race remains strict failure"
    } else {
        "sub100k correctness/diagnostic trace waits for old-Warm own committed snapshot before eviction; t0-included production frames; not full contention or abort pressure evidence"
    }
}
#[test]
fn tc_229_finished_warm_switch_waits_for_own_commit_before_eviction() {
    assert_warm_admission_waits("published");
}

#[test]
fn tc_229_completion_offer_warm_switch_waits_for_own_commit() {
    for stage in ["offered", "data-complete"] {
        assert_warm_admission_waits(stage);
    }
}

#[test]
fn tc_229_small_smoke_waits_for_unfinished_warm_commit() {
    assert_warm_admission_waits("unfinished");
}

fn assert_warm_admission_waits(stage: &str) {
    let mut driver = Driver::new();
    driver.settle_startup();
    driver.app.create_new_tab();
    driver.app.create_new_tab();
    driver.settle_startup();
    let warm = driver.app.shell.tabs.get(1).unwrap().id;
    let active = driver.app.current_tab_id();
    let root = driver.app.shell.tabs.get(1).unwrap().root.clone();
    let now = Instant::now();
    let mut observation = crate::app::index_mailbox::IndexPerfObservation {
        terminal_kind: Some("finished"),
        terminal_offer_kind: Some("finished"),
        terminal_published: Some(now),
        started_root: Some(root),
        started_source: Some("Walker"),
        terminal_source: Some("Walker"),
        request_processing_returned: Some(now),
        entries_emitted: 8,
        ..Default::default()
    };
    if stage != "published" {
        observation.terminal_kind = None;
        observation.terminal_published = None;
        observation.terminal_source = None;
        observation.request_processing_returned = None;
        observation.data_publish_end = Some(now);
        if stage == "data-complete" {
            observation.terminal_offer_kind = None;
        } else if stage == "unfinished" {
            observation.terminal_offer_kind = None;
            observation.data_publish_end = None;
        }
    }
    let handle = Arc::new(std::sync::Mutex::new(observation));
    let i = &mut driver.app.shell.indexing;
    i.warm_tab_id = Some(warm);
    i.latest_request_ids.lock().unwrap().insert(warm, 777);
    i.request_tabs.insert(777, warm);
    i.perf_allocations
        .push(crate::app::index_coordinator::IndexPerfAllocation {
            id: 777,
            tab: Some(warm),
            at: now,
            observation: handle,
        });
    let mut plans = vec![PlannedRequest {
        id: 777,
        tab: warm,
        ..Default::default()
    }];
    let mut future = HashMap::new();
    // Finished can arrive after an outer checkpoint. Admission itself must
    // notice it; old equal-count data is not this request's committed snapshot.
    assert_eq!(
        planned_switch_guarded(
            &mut driver,
            0,
            &mut plans,
            &mut future,
            if stage == "unfinished" {
                WarmSwitchPolicy::CommittedSmall
            } else {
                WarmSwitchPolicy::ObservedCompletion
            },
        ),
        SwitchAdmission::WaitForFinishedWarm(777)
    );
    assert_eq!(driver.app.current_tab_id(), active);
    assert_eq!(
        driver.app.shell.indexing.latest_request_for_tab(warm),
        Some(777)
    );
    assert_eq!(
        driver.app.shell.indexing.request_tabs.get(&777),
        Some(&warm)
    );
    assert!(plans[0].permission_at.is_none());
    assert!(future.is_empty());
}
fn finished_warm_commit_pending(
    driver: &Driver,
    plans: &[PlannedRequest],
    warm: u64,
) -> Option<u64> {
    warm_commit_pending(driver, plans, warm, WarmSwitchPolicy::ObservedCompletion)
}
fn warm_commit_pending(
    driver: &Driver,
    plans: &[PlannedRequest],
    warm: u64,
    policy: WarmSwitchPolicy,
) -> Option<u64> {
    let p = plans
        .iter()
        .filter(|p| p.tab == warm)
        .max_by_key(|p| p.id)?;
    let a = driver
        .app
        .shell
        .indexing
        .perf_allocations
        .iter()
        .find(|a| a.id == p.id)?;
    let o = a.observation.lock().unwrap().clone();
    if policy != WarmSwitchPolicy::CommittedSmall
        && o.terminal_kind != Some("finished")
        && o.terminal_offer_kind != Some("finished")
        && o.data_publish_end.is_none()
    {
        return None;
    }
    let t = driver
        .app
        .shell
        .tabs
        .iter()
        .find(|t| t.id == warm)
        .expect("owned Warm tab");
    let snapshot = &t.result_state.committed;
    let own_commit = snapshot.freshness.as_ref().is_some_and(|f| {
        f.request_id == p.id
            && f.root == t.root
            && o.started_root.as_ref() == Some(&t.root)
            && o.started_source == o.terminal_source
            && match (&f.source, o.terminal_source) {
                (IndexSource::Walker, Some("Walker")) => true,
                (IndexSource::FileList(path), Some("FileList")) => {
                    path == &t.root.join("FileList.txt")
                }
                _ => false,
            }
    });
    let settled = own_commit
        && snapshot.all_entries.len() == o.entries_emitted
        && snapshot.entries.len() == o.entries_emitted
        && !tab_debt(t)
        && !driver
            .app
            .shell
            .indexing
            .background_finalizations
            .contains_key(&p.id)
        && !driver.app.shell.indexing.request_tabs.contains_key(&p.id)
        && driver
            .app
            .shell
            .indexing
            .perf_released_requests
            .contains_key(&p.id)
        && physical_request_complete(&o);
    (!settled).then_some(p.id)
}
#[test]
fn tc_229_finished_warm_admission_accepts_actual_commit_and_rejects_stale_identity() {
    if super::super::child_process::isolate(
        module_path!(),
        "tc_229_finished_warm_admission_accepts_actual_commit_and_rejects_stale_identity",
    ) {
        return;
    }
    let fixture = ExtendedFixture::new(32, super::fixture::Shape::FlatFiles);
    let mut driver = Driver::new();
    driver.settle_startup();
    configure(
        &mut driver,
        &fixture,
        Source::Walker,
        default_filter(),
        false,
    );
    driver.app.shell.indexing.perf_observe_requests = true;
    driver.app.shell.indexing.perf_observe_history = true;
    driver.app.request_index_refresh();
    let id = driver.app.shell.indexing.pending_request_id.unwrap();
    let warm = driver.app.current_tab_id().unwrap();
    settle_setup(&mut driver);
    driver.app.create_new_tab();
    settle_setup(&mut driver);
    let plans = vec![PlannedRequest {
        id,
        tab: warm,
        ..Default::default()
    }];
    let n = driver
        .app
        .shell
        .tabs
        .iter()
        .position(|t| t.id == warm)
        .unwrap();
    assert_eq!(finished_warm_commit_pending(&driver, &plans, warm), None);
    let tab = driver.app.shell.tabs.get_mut(n).unwrap();
    tab.result_state
        .committed
        .freshness
        .as_mut()
        .unwrap()
        .request_id = id + 1;
    assert_eq!(
        finished_warm_commit_pending(&driver, &plans, warm),
        Some(id),
        "same cardinality with another generation must defer"
    );
    driver
        .app
        .shell
        .tabs
        .get_mut(n)
        .unwrap()
        .result_state
        .committed
        .freshness
        .as_mut()
        .unwrap()
        .request_id = id;
    driver
        .app
        .shell
        .tabs
        .get_mut(n)
        .unwrap()
        .index_state
        .kind_resolution_in_progress = true;
    assert_eq!(
        finished_warm_commit_pending(&driver, &plans, warm),
        Some(id)
    );
    driver
        .app
        .shell
        .tabs
        .get_mut(n)
        .unwrap()
        .index_state
        .kind_resolution_in_progress = false;
    assert_eq!(finished_warm_commit_pending(&driver, &plans, warm), None);
}
fn planned_switch(
    driver: &mut Driver,
    n: usize,
    plans: &mut Vec<PlannedRequest>,
    future: &mut HashMap<u64, usize>,
) -> bool {
    match planned_switch_guarded(driver, n, plans, future, WarmSwitchPolicy::Unrestricted) {
        SwitchAdmission::Activated(active) => active,
        SwitchAdmission::WaitForFinishedWarm(_) => unreachable!("unguarded switch"),
    }
}
fn planned_switch_guarded(
    driver: &mut Driver,
    n: usize,
    plans: &mut Vec<PlannedRequest>,
    future: &mut HashMap<u64, usize>,
    policy: WarmSwitchPolicy,
) -> SwitchAdmission {
    let tab = driver.app.shell.tabs.get(n).expect("switch target");
    let id = tab.id;
    let lifecycle = tab.index_state.lifecycle();
    let pending = tab.index_state.pending_index_request_id;
    let deferred = pending.is_none()
        && tab.index_state.pending_index_finish.is_none()
        && (tab.index_state.root_after_pending_finish.is_some()
            || tab.index_state.refresh_after_pending_finish.is_some());
    let auto = driver.app.shell.tabs.active_tab_index() != n
        && (deferred
            || pending.is_none()
                && matches!(
                    lifecycle,
                    crate::app::TabResourceLifecycle::Dormant
                        | crate::app::TabResourceLifecycle::Evicted
                ));
    if let Some(warm) = driver.app.shell.indexing.warm_tab_id {
        if warm != id && Some(warm) != driver.app.current_tab_id() {
            if policy != WarmSwitchPolicy::Unrestricted {
                if let Some(id) = warm_commit_pending(driver, plans, warm, policy) {
                    return SwitchAdmission::WaitForFinishedWarm(id);
                }
            }
            let before_permission =
                (policy != WarmSwitchPolicy::Unrestricted).then(|| plans.clone());
            permit_revocation(driver, plans, warm, "switch-evicts-previous-Warm");
            // Publication may happen after the outer input checkpoint. Recheck
            // the actual classification before any replacement mutation.
            if policy != WarmSwitchPolicy::Unrestricted {
                if let Some(id) = warm_commit_pending(driver, plans, warm, policy) {
                    *plans = before_permission.unwrap();
                    return SwitchAdmission::WaitForFinishedWarm(id);
                }
            }
        }
    }
    if auto {
        permit_revocation(driver, plans, id, "lifecycle-activation-refresh");
    }
    driver.app.switch_to_tab_index(n);
    let active = driver.app.current_tab_id() == Some(id);
    if active {
        record_planned_new(driver, plans, id, usize::from(auto));
    } else {
        assert_eq!(
            driver.app.shell.tabs.pending_activation_tab_id,
            Some(id),
            "unowned deferred activation"
        );
        record_planned_new(driver, plans, id, 0);
        if auto {
            assert!(
                future.insert(id, 1).is_none(),
                "duplicate planned deferred activation"
            );
        }
    }
    SwitchAdmission::Activated(active)
}
fn declare_tabchain_victim(
    driver: &Driver,
    plans: &mut [PlannedRequest],
    stage: usize,
    new_active: u64,
    seeds: &[SeedWitness],
    tabs: &[u64],
) {
    if stage == 0 {
        return;
    }
    assert!(matches!(stage, 1 | 2));
    let declared_at = Instant::now();
    for d in plans.iter_mut().filter_map(|p| p.victim.as_mut()) {
        if d.closed_at.is_none() {
            d.closed_at = Some(declared_at);
        }
    }
    let victim_tab = tabs[stage - 1];
    if driver.app.shell.indexing.warm_tab_id != Some(victim_tab) {
        return;
    }
    let outgoing = driver.app.current_tab_id().unwrap();
    let active_root = seeds[tabs.iter().position(|id| *id == new_active).unwrap()]
        .root
        .clone();
    let p = plans
        .iter_mut()
        .filter(|p| p.tab == victim_tab)
        .max_by_key(|p| p.id)
        .expect("owned old Warm request");
    assert!(p.victim.is_none());
    p.victim = Some(VictimDeclaration {
        profile: Profile::TabChain,
        old_warm_tab: Some(victim_tab),
        trace_tabs: [tabs[0], tabs[1], tabs[2]],
        stage,
        declared_at,
        closed_at: None,
        victim_id: p.id,
        victim_tab,
        active_tab: new_active,
        current_warm_tab: outgoing,
        incoming_ids: Vec::new(),
        active_root,
        switch_ack: None,
        seed: seeds[stage - 1].clone(),
    });
}
// Fixed TabChain switch only: actual state is read after activation, before
// dependent refresh/selection. This is not active-at-removal instrumentation.
fn acknowledge_tabchain_switch(
    driver: &Driver,
    plans: &mut [PlannedRequest],
    stage: usize,
    expected_tab: u64,
    expected_root: &std::path::Path,
) -> bool {
    if stage > 2
        || !dependent_input_allowed(
            expected_tab,
            driver.app.current_tab_id(),
            driver.app.shell.tabs.pending_activation_tab_id,
        )
        || driver.app.shell.runtime.root != expected_root
    {
        return false;
    }
    let Some(p) = plans.iter_mut().find(|p| {
        p.victim
            .as_ref()
            .is_some_and(|c| c.stage == stage && c.closed_at.is_none())
    }) else {
        // A fast successful old tab may already have no Warm request. This
        // acknowledges the real switch only, creating no invalidation proof.
        return true;
    };
    let c = p.victim.as_mut().unwrap();
    if c.active_tab != expected_tab || c.active_root != expected_root {
        return false;
    }
    if c.switch_ack.is_none() {
        c.switch_ack = Some(SwitchAcknowledgement {
            at: Instant::now(),
            tab: driver.app.current_tab_id().unwrap(),
            root: driver.app.shell.runtime.root.clone(),
            pending_activation: driver.app.shell.tabs.pending_activation_tab_id,
        });
    }
    true
}
fn preemption_events_valid(
    driver: &Driver,
    plans: &[PlannedRequest],
    ledger: &[OwnedRequest],
) -> bool {
    let i = &driver.app.shell.indexing;
    if i.perf_preemption_overflow
        || i.perf_warm_removal_overflow
        || i.perf_preemptions.len() > 128
        || i.perf_warm_removals.len() > 128
    {
        return false;
    }
    let observe = |id, tab| {
        ledger
            .iter()
            .find(|r| r.id == id && r.tab == tab)
            .map(OwnedRequest::observation)
    };
    i.perf_warm_removals.iter().all(|m| {
        i.perf_warm_removals
            .iter()
            .filter(|other| other.removed_request_id == m.removed_request_id)
            .count()
            == 1
            && plans
                .iter()
                .filter(|p| {
                    p.id == m.removed_request_id
                        && p.tab == m.route_tab
                        && observe(p.id, p.tab).is_some_and(|o| warm_removal_cause(p, m, &o))
                })
                .count()
                == 1
    }) && i.perf_preemptions.iter().all(|e| {
        i.perf_preemptions
            .iter()
            .filter(|other| other.victim_id == e.victim_id)
            .count()
            == 1
            && e.pending_active_id.is_some_and(|incoming| {
                plans
                    .iter()
                    .any(|p| p.id == incoming && p.tab == e.active_tab)
                    && observe(incoming, e.active_tab).is_some()
            })
            && plans
                .iter()
                .filter(|p| {
                    p.id == e.victim_id
                        && p.tab == e.victim_tab
                        && observe(p.id, p.tab).is_some_and(|o| {
                            if e.prior_latest == Some(p.id) {
                                !i.perf_warm_removals
                                    .iter()
                                    .any(|m| m.removed_request_id == p.id)
                                    && declared_preemption_cause(p, e, &o)
                            } else if e.prior_latest.is_none() {
                                let matches = i
                                    .perf_warm_removals
                                    .iter()
                                    .filter(|m| m.removed_request_id == p.id)
                                    .collect::<Vec<_>>();
                                matches.len() == 1 && warm_preempt_followup(p, matches[0], e, &o)
                            } else {
                                false
                            }
                        })
                })
                .count()
                == 1
    })
}
fn request_complete(driver: &Driver, r: &OwnedRequest, plans: &[PlannedRequest]) -> bool {
    let o = r.observation();
    physical_request_complete(&o)
        || (plans.iter().any(|p| p.id == r.id && p.revoked_by.is_some())
            && planned_unsent_discard(
                &o,
                plans
                    .iter()
                    .find(|p| p.id == r.id)
                    .and_then(|p| p.permission_at),
                driver
                    .app
                    .shell
                    .indexing
                    .perf_released_requests
                    .contains_key(&r.id),
            ))
}
fn proofs(
    driver: &Driver,
    ledger: &[OwnedRequest],
    plans: &[PlannedRequest],
    roots: &[&ExtendedFixture],
    tabs: &[u64],
    source: Source,
) -> Vec<RequestProof> {
    ledger
        .iter()
        .map(|r| {
            let o = r.observation();
            let n = tabs.iter().position(|id| *id == r.tab).expect("owned tab");
            let fresh = if driver.app.current_tab_id() == Some(r.tab) {
                driver.app.shell.runtime.freshness.as_ref()
            } else {
                driver
                    .app
                    .shell
                    .tabs
                    .get(n)
                    .unwrap()
                    .result_state
                    .committed
                    .freshness
                    .as_ref()
            };
            let latest_identity = fresh.is_some_and(|f| {
                f.request_id == r.id
                    && f.root == roots[n].root
                    && match (source, &f.source) {
                        (Source::Walker, IndexSource::Walker) => true,
                        (Source::FileList, IndexSource::FileList(path)) => {
                            path == &roots[n].root.join("FileList.txt")
                        }
                        _ => false,
                    }
            });
            let source_root = o.started_source == Some(source.name())
                && o.terminal_source == Some(source.name())
                && o.started_root.as_ref() == Some(&roots[n].root);
            let revoked = plans.iter().any(|p| p.id == r.id && p.revoked_by.is_some());
            let last_good_victim = plans.iter().find(|p| p.id == r.id).is_some_and(|p| {
                let Some(c) = &p.victim else { return false };
                let t = driver.app.shell.tabs.get(n).unwrap();
                let all_count = if driver.app.current_tab_id() == Some(r.tab) {
                    driver.app.shell.runtime.all_entries.len()
                } else {
                    t.result_state.committed.all_entries.len()
                };
                let visible_count = if driver.app.current_tab_id() == Some(r.tab) {
                    driver.app.shell.runtime.entries.len()
                } else {
                    t.result_state.committed.entries.len()
                };
                let state = VictimFastState {
                    seed_id: fresh.map(|f| f.request_id),
                    root_source_match: fresh.is_some_and(|f| {
                        f.root == c.seed.root
                            && match (c.seed.source, &f.source) {
                                (Source::Walker, IndexSource::Walker) => true,
                                (Source::FileList, IndexSource::FileList(path)) => {
                                    path == &c.seed.root.join("FileList.txt")
                                }
                                _ => false,
                            }
                    }),
                    counts_match: all_count == c.seed.all_count
                        && visible_count == c.seed.visible_count,
                    latest_without_successor: requires_latest_generation(plans, r.id, r.tab),
                    released: driver
                        .app
                        .shell
                        .indexing
                        .perf_released_requests
                        .contains_key(&r.id),
                    route_absent: !driver.app.shell.indexing.request_tabs.contains_key(&r.id),
                    worker_load_zero: {
                        let load = driver.app.shell.indexing.tx.load();
                        load.queued == 0 && load.inflight == 0
                    },
                    index_debt_zero: !all_index_debt(driver),
                    result_debt_zero: !result_debt(driver),
                };
                let events = driver
                    .app
                    .shell
                    .indexing
                    .perf_preemptions
                    .iter()
                    .filter(|e| e.victim_id == r.id)
                    .collect::<Vec<_>>();
                let removals = driver
                    .app
                    .shell
                    .indexing
                    .perf_warm_removals
                    .iter()
                    .filter(|m| m.removed_request_id == r.id)
                    .collect::<Vec<_>>();
                if removals.len() == 1 {
                    warm_last_good_victim_fast(p, removals[0], &o, &state)
                } else {
                    removals.is_empty()
                        && events.len() == 1
                        && last_good_victim_fast(p, events[0], &o, &state)
                }
            });
            RequestProof {
                id: r.id,
                tab: r.tab,
                finished: o.terminal_kind == Some("finished"),
                physical_complete: request_complete(driver, r, plans),
                source_root_correct: source_root
                    && request_acquisition_owned(&o, &roots[n].root, source)
                    && (!requires_latest_generation(plans, r.id, r.tab) || latest_identity),
                last_good_victim,
                revoked_abort: revoked
                    && request_acquisition_owned(&o, &roots[n].root, source)
                    && abort_after_permission(
                        o.mailbox_closed_at
                            .into_iter()
                            .chain(
                                if matches!(o.terminal_offer_kind, Some("canceled" | "failed")) {
                                    o.terminal_offered
                                } else {
                                    None
                                },
                            )
                            .min(),
                        plans
                            .iter()
                            .find(|p| p.id == r.id)
                            .and_then(|p| p.permission_at),
                    )
                    && (o.terminal_kind == Some("canceled")
                        || o.mailbox_closed
                            && o.terminal_offer_current == Some(false)
                            && (matches!(o.terminal_offer_kind, Some("finished" | "canceled"))
                                || o.terminal_offer_kind == Some("failed")
                                    && o.terminal_offer_error.as_deref()
                                        == Some("index receiver closed"))
                        || planned_unsent_discard(
                            &o,
                            plans
                                .iter()
                                .find(|p| p.id == r.id)
                                .and_then(|p| p.permission_at),
                            driver
                                .app
                                .shell
                                .indexing
                                .perf_released_requests
                                .contains_key(&r.id),
                        )
                        || o.mailbox_closed
                            && o.skipped_closed_before_start
                            && o.request_processing_returned.is_some()),
            }
        })
        .collect()
}
pub(super) fn configure(
    driver: &mut Driver,
    fixture: &ExtendedFixture,
    source: Source,
    filter: Filter,
    links: bool,
) {
    let app = &mut driver.app;
    app.shell.runtime.root = fixture.root.clone();
    app.shell.runtime.use_filelist = source == Source::FileList;
    app.shell.runtime.include_files = filter.files;
    app.shell.runtime.include_dirs = filter.dirs;
    app.shell.runtime.ignore_case = filter.ignore_case;
    app.shell.ui.ignore_list_enabled = filter.ignore_enabled;
    app.shell.runtime.ignore_list_terms = Arc::new(vec!["SKIP".into()]);
    app.shell.runtime.follow_links = links;
    app.sync_active_tab_state();
}
pub(super) fn default_filter() -> Filter {
    Filter {
        files: true,
        dirs: true,
        ignore_enabled: false,
        ignore_case: true,
    }
}
fn tab_debt(tab: &crate::app::AppTabState) -> bool {
    let i = &tab.index_state;
    i.index_in_progress
        || i.pending_index_request_id.is_some()
        || i.pending_index_entries_request_id.is_some()
        || i.pending_index_finish.is_some()
        || i.build_reclaim_pending
        || i.refresh_after_pending_finish.is_some()
        || i.root_after_pending_finish.is_some()
        || !i.build.pending_entries.is_empty()
        || i.build.active_filter.is_some()
        || i.kind_resolution_in_progress
        || !i.build.pending_kind_paths.is_empty()
        || !i.build.in_flight_kind_paths.is_empty()
        || i.search_resume_pending
        || i.search_rerun_pending
}
pub(super) fn all_index_debt(driver: &Driver) -> bool {
    let i = &driver.app.shell.indexing;
    i.tx.load().queued != 0
        || i.tx.load().inflight != 0
        || driver.index_debt()
        || !i.pending_queue.is_empty()
        || !i.inflight_requests.is_empty()
        || !i.request_tabs.is_empty()
        || !i.background_states.is_empty()
        || i.background_finalizations.keys().next().is_some()
        || i.deferred_response.is_some()
        || !i.deferred_non_active_responses.is_empty()
        || driver.app.shell.tabs.pending_activation_tab_id.is_some()
        || driver
            .app
            .shell
            .tabs
            .iter()
            .enumerate()
            .any(|(n, t)| n != driver.app.shell.tabs.active_tab_index() && tab_debt(t))
}
pub(super) fn result_debt(driver: &Driver) -> bool {
    let app = &driver.app;
    app.shell.search.in_progress()
        || app.shell.search.pending_request_id().is_some()
        || app.shell.indexing.search_resume_pending
        || app.shell.indexing.search_rerun_pending
        || app.shell.worker_bus.sort.in_progress
        || app.shell.worker_bus.sort.pending_request_id.is_some()
        || app.shell.worker_bus.preview.in_progress
        || app.shell.worker_bus.preview.pending_request_id.is_some()
        || app
            .shell
            .worker_bus
            .preview
            .worker_inflight_request_id
            .is_some()
        || app.shell.worker_bus.preview.latest_request.is_some()
        || app.parked_preview_request.is_some()
        || app.deferred_latest_preview_request.is_some()
        || app.deferred_preview_response.is_some()
        || app.shell.tabs.iter().enumerate().any(|(n, t)| {
            n != app.shell.tabs.active_tab_index()
                && (t.search_in_progress
                    || t.preview_in_progress
                    || t.result_state.sort_in_progress
                    || t.pending_request_id.is_some()
                    || t.pending_preview_request_id.is_some()
                    || t.result_state.pending_sort_request_id.is_some())
        })
}
pub(super) fn settle_setup(driver: &mut Driver) {
    let begin = Instant::now();
    loop {
        driver.paced_frame();
        if !all_index_debt(driver) && !result_debt(driver) {
            break;
        }
        assert!(
            begin.elapsed() < Duration::from_secs(120),
            "setup did not settle"
        );
    }
}
fn owned_count(driver: &Driver, tab: u64, request: Option<u64>) -> usize {
    let app = &driver.app;
    if app.current_tab_id() == Some(tab) {
        return driver.ingested();
    }
    let t = app.shell.tabs.iter().find(|t| t.id == tab).unwrap();
    if let Some(id) = request {
        if let Some(f) = app.shell.indexing.background_finalizations.get(&id) {
            return f.completed_entries.len();
        }
        if let Some(s) = app.shell.indexing.background_states.get(&id) {
            return s.entries.len() + t.index_state.build.index.entries.len();
        }
        if t.index_state.pending_index_request_id == Some(id) {
            return t.index_state.build.index.entries.len();
        }
    }
    t.result_state.committed.all_entries.len()
}
fn request_for(driver: &Driver, tab: u64) -> Option<u64> {
    driver.app.shell.indexing.latest_request_for_tab(tab)
}
fn measured_request_for(
    driver: &Driver,
    plans: &[PlannedRequest],
    ledger: &[OwnedRequest],
    tab: u64,
) -> u64 {
    let id = plans
        .iter()
        .filter(|p| p.tab == tab)
        .map(|p| p.id)
        .max()
        .expect("measured planned owner");
    let i = &driver.app.shell.indexing;
    let allocations = i.perf_allocations.iter().filter(|a| a.id == id);
    let owned = ledger.iter().filter(|r| r.id == id);
    let proof = MeasuredOwnerProof {
        id,
        allocation_count: allocations.clone().count(),
        allocation_tab_correct: allocations.clone().all(|a| a.tab == Some(tab)),
        ledger_count: owned.clone().count(),
        ledger_tab_correct: owned.clone().all(|r| r.tab == tab),
        logically_released: i.perf_released_requests.contains_key(&id),
        route_present: i.request_tabs.contains_key(&id),
        warm_removal_identity: {
            let matching = i
                .perf_warm_removals
                .iter()
                .filter(|m| m.removed_request_id == id && m.route_tab == tab)
                .collect::<Vec<_>>();
            matching.len() == 1
                && preemption_events_valid(driver, plans, ledger)
                && plans
                    .iter()
                    .find(|p| p.id == id && p.tab == tab)
                    .is_some_and(|p| {
                        owned
                            .clone()
                            .find(|r| r.tab == tab)
                            .is_some_and(|r| warm_removal_cause(p, matching[0], &r.observation()))
                    })
        },
        finished_preemption_cause: {
            let events = i
                .perf_preemptions
                .iter()
                .filter(|e| e.victim_id == id && e.victim_tab == tab)
                .collect::<Vec<_>>();
            !i.perf_preemption_overflow
                && events.len() == 1
                && preemption_events_valid(driver, plans, ledger)
                && plans
                    .iter()
                    .find(|p| p.id == id && p.tab == tab)
                    .is_some_and(|p| {
                        p.permission_at.is_none()
                            && p.permission_reason.is_none()
                            && owned.clone().find(|r| r.tab == tab).is_some_and(|r| {
                                declared_preemption_cause(p, events[0], &r.observation())
                            })
                    })
        },
    };
    select_measured_request(tab, request_for(driver, tab), plans, &proof).unwrap_or_else(|reason| {
        panic!("measured owner identity rejected tab={tab} id={id}: {reason}")
    })
}
fn actual_signature(root: &std::path::Path, entries: &[Entry]) -> String {
    let mut p = entries.iter().map(|e| e.path.clone()).collect::<Vec<_>>();
    p.sort();
    format!("{:016x}", signature(root, p.iter()))
}

fn fixture_logical_count(fixture: &ExtendedFixture, filter: Filter, scans: &mut usize) -> usize {
    *scans += 1;
    fixture
        .expected
        .iter()
        .filter(|record| {
            if record.is_dir {
                filter.dirs
            } else {
                filter.files
            }
        })
        .count()
}

const FRAME_DIAGNOSTIC_LIMIT: usize = 256;

#[derive(serde::Serialize)]
struct FrameState {
    // Active-filter owner identity is not exposed. These are separate actual
    // shell identities, not a claim that the pending request owns the filter.
    active_filter_cursor: Option<usize>,
    // Producer state is read after ended_ms; never backdate asynchronous state.
    observed_ms: f64,
    active_tab: Option<u64>,
    pending_request_id: Option<u64>,
    primary_request_id: u64,
    ingested: usize,
    index_debt: bool,
    result_debt: bool,
    producer_data_end_ms: Option<f64>,
    producer_terminal_ms: Option<f64>,
}

#[derive(serde::Serialize)]
struct FrameObservation {
    frame: usize,
    started_ms: f64,
    ended_ms: f64,
    #[serde(flatten)]
    state: FrameState,
}

struct FrameDiagnostics {
    records: Vec<FrameObservation>,
    total: usize,
    previous_start: Option<Duration>,
    max_start_gap: Duration,
}
impl FrameDiagnostics {
    // Construct before t0; push never grows the allocation or touches payloads.
    fn new() -> Self {
        Self {
            records: Vec::with_capacity(FRAME_DIAGNOSTIC_LIMIT),
            total: 0,
            previous_start: None,
            max_start_gap: Duration::ZERO,
        }
    }
    fn push(&mut self, start: Duration, end: Duration, state: FrameState) {
        assert!(end >= start);
        if let Some(previous) = self.previous_start {
            self.max_start_gap = self
                .max_start_gap
                .max(start.checked_sub(previous).expect("monotonic frame starts"));
        }
        self.previous_start = Some(start);
        self.total += 1;
        if self.records.len() < FRAME_DIAGNOSTIC_LIMIT {
            self.records.push(FrameObservation {
                frame: self.total,
                started_ms: ms(start),
                ended_ms: ms(end),
                state,
            });
        }
    }
    // Serialization belongs after tentative t3, with all other raw reporting.
    fn report(&self) -> serde_json::Value {
        serde_json::json!({
            "policy":"bounded-scalars; state observed after frame; active-filter owner NOT_OBSERVED",
            "limit":FRAME_DIAGNOSTIC_LIMIT,
            "total_frames":self.total,
            "truncated_frames":self.total-self.records.len(),
            "max_frame_start_gap_ms":ms(self.max_start_gap),
            "records":self.records,
        })
    }
}

pub(super) fn run(
    fixture: &ExtendedFixture,
    companions: &[&ExtendedFixture],
    profile: Profile,
    source: Source,
    condition: bool,
    full: bool,
) -> serde_json::Value {
    assert!(!profile.parser());
    fixture.prepare_source(source);
    for f in companions {
        f.prepare_source(source);
    }
    let mut driver = Driver::new();
    driver.settle_startup();
    let mut tabs = vec![driver.app.current_tab_id().unwrap()];
    if profile.tabs() {
        for _ in 0..if profile == Profile::TabChain { 2 } else { 1 } {
            driver.app.create_new_tab();
            tabs.push(driver.app.current_tab_id().unwrap());
        }
    }
    let mut setup_roots = vec![fixture];
    setup_roots.extend_from_slice(companions);
    for n in (0..tabs.len()).rev() {
        driver.app.switch_to_tab_index(n);
        configure(
            &mut driver,
            setup_roots[n],
            source,
            profile.filter(false),
            profile == Profile::Links,
        );
    }
    settle_setup(&mut driver);
    assert!(driver.app.shell.runtime.all_entries.is_empty());
    let mut empty_publication = profile
        .stable()
        .then(|| EmptyRootPublication::new(setup_roots[1], source));
    // The final App owner is declared after the publication guard: workers stop
    // before its rollback on every panic path.
    let mut driver = driver;
    let mut initial_b = serde_json::Value::Null;
    if profile.stable() {
        driver.app.switch_to_tab_index(1);
        settle_setup(&mut driver);
        driver.app.shell.indexing.perf_observe_requests = true;
        driver.app.request_index_refresh();
        let empty_id = driver
            .app
            .shell
            .indexing
            .pending_request_id
            .expect("empty B request");
        let empty_observation = driver
            .app
            .shell
            .indexing
            .response_mailboxes
            .lock()
            .unwrap()
            .get(&empty_id)
            .expect("empty B mailbox")
            .perf_handle();
        settle_setup(&mut driver);
        assert!(driver.app.shell.runtime.all_entries.is_empty());
        assert!(driver.app.shell.runtime.entries.is_empty());
        let observed = empty_observation.lock().unwrap().clone();
        assert_eq!(observed.terminal_kind, Some("finished"));
        assert_eq!(observed.started_source, Some(source.name()));
        assert_eq!(observed.terminal_source, Some(source.name()));
        let f = driver
            .app
            .shell
            .runtime
            .freshness
            .as_ref()
            .expect("actual empty B freshness");
        assert_eq!(f.request_id, empty_id);
        assert_eq!(f.root, setup_roots[1].root);
        assert!(match (&source, &f.source) {
            (Source::FileList, IndexSource::FileList(path)) =>
                path == &setup_roots[1].root.join("FileList.txt"),
            (Source::Walker, IndexSource::Walker) => true,
            _ => false,
        });
        initial_b = serde_json::json!({"request_id":empty_id,"root":f.root,"actual_source":format!("{:?}",f.source),"all_entries":0,"visible_entries":0,"generation":f.request_id,"actual_success":true});
        driver.app.switch_to_tab_index(0);
        settle_setup(&mut driver);
    }
    let seed = profile.stable()
        || matches!(
            profile,
            Profile::Reclaim | Profile::WarmReclaim | Profile::TabChain
        );
    if seed {
        let seed_tabs = if profile.stable() { 1 } else { tabs.len() };
        for (n, setup_root) in setup_roots.iter().enumerate().take(seed_tabs) {
            driver.app.switch_to_tab_index(n);
            configure(
                &mut driver,
                setup_root,
                source,
                profile.filter(false),
                false,
            );
            driver.app.request_index_refresh();
            settle_setup(&mut driver);
            assert_eq!(
                driver.app.shell.runtime.all_entries.len(),
                setup_root.expected.len()
            );
        }
        driver.app.switch_to_tab_index(0);
        settle_setup(&mut driver);
    }
    // Real old -> distinct new membership in the same roots, before t0.
    let generations = if matches!(profile, Profile::Reclaim | Profile::WarmReclaim) {
        setup_roots
            .iter()
            .take(if profile == Profile::WarmReclaim && !condition {
                1
            } else {
                tabs.len()
            })
            .map(|&f| f.next_generation())
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let mut driver = driver;
    let roots = setup_roots
        .iter()
        .enumerate()
        .map(|(n, f)| generations.get(n).map_or(*f, |g| g.fixture()))
        .collect::<Vec<_>>();
    if profile.stable() {
        driver.app.switch_to_tab_index(1);
        settle_setup(&mut driver);
        assert!(
            driver.app.shell.runtime.all_entries.is_empty(),
            "B must have an empty committed prestate"
        );
    }
    let target = if profile.stable() { 1 } else { 0 };
    let filter = profile.filter(condition);
    let (mode, scope) = profile.sort(condition);
    let expected = ExtendedOracle::new(
        roots[0],
        source,
        filter,
        profile.query(condition),
        mode,
        scope,
        driver.app.shell.runtime.limit,
    );
    let quiet_oracles = roots
        .iter()
        .take(tabs.len())
        .enumerate()
        .map(|(n, f)| {
            ExtendedOracle::new(
                if profile == Profile::WarmReclaim && !condition && n == 1 {
                    setup_roots[n]
                } else {
                    f
                },
                source,
                default_filter(),
                "",
                ResultSortMode::Score,
                ResultSortScope::ShownResults,
                driver.app.shell.runtime.limit,
            )
        })
        .collect::<Vec<_>>();
    let seed_witnesses = if profile == Profile::TabChain {
        tabs.iter()
            .enumerate()
            .map(|(n, id)| {
                let t = driver.app.shell.tabs.get(n).unwrap();
                let (all, visible, fresh) = if driver.app.current_tab_id() == Some(*id) {
                    let r = &driver.app.shell.runtime;
                    (&r.all_entries, &r.entries, r.freshness.as_ref())
                } else {
                    let r = &t.result_state.committed;
                    (&r.all_entries, &r.entries, r.freshness.as_ref())
                };
                assert!(
                    quiet_oracles[n].valid_snapshot(all, visible),
                    "untimed independent seeded tab oracle"
                );
                let f = fresh.expect("seed actual freshness");
                assert_eq!(f.root, roots[n].root);
                assert!(match (source, &f.source) {
                    (Source::Walker, IndexSource::Walker) => true,
                    (Source::FileList, IndexSource::FileList(path)) =>
                        path == &roots[n].root.join("FileList.txt"),
                    _ => false,
                });
                SeedWitness {
                    verified: true,
                    id: f.request_id,
                    root: f.root.clone(),
                    source,
                    all_count: all.len(),
                    visible_count: visible.len(),
                    all_signature: seed_entry_signature(&f.root, all),
                    visible_signature: seed_entry_signature(&f.root, visible),
                }
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let stable_ptr = if profile.stable() {
        let tab = driver.app.shell.tabs.get(0).unwrap();
        assert!(
            quiet_oracles[0].valid_snapshot(
                &tab.result_state.committed.all_entries,
                &tab.result_state.committed.entries
            ),
            "untimed initial A fixture membership"
        );
        Some(Arc::as_ptr(&tab.result_state.committed.entries) as usize)
    } else {
        None
    };
    // Static filter profiles use the same emitted logical workload in both halves.
    if !matches!(profile, Profile::IgnoreCase | Profile::MidIgnore) {
        configure(
            &mut driver,
            roots[target],
            source,
            profile.filter(condition),
            profile == Profile::Links,
        );
    }
    if profile.stable() {
        assert!(!all_index_debt(&driver) && !result_debt(&driver));
        let f = driver
            .app
            .shell
            .runtime
            .freshness
            .as_ref()
            .expect("B freshness before publication");
        assert_eq!(f.request_id, initial_b["request_id"].as_u64().unwrap());
        assert_eq!(f.root, roots[1].root);
        empty_publication
            .take()
            .unwrap()
            .restore()
            .expect("restore full B fixture");
    }
    driver.app.shell.indexing.perf_observe_requests = true;
    driver.app.shell.indexing.perf_observe_history = true;
    driver.app.shell.indexing.perf_observe_aux = true;
    driver.app.shell.search.perf_enabled = true;
    let initial = serde_json::json!({"active_tab":tabs[target],"active_snapshot_entries":driver.app.shell.runtime.all_entries.len(),"stable_A_entries":if profile.stable(){fixture.expected.len()}else{0},"source":source.name(),"depth":"unlimited","seeded_old_snapshot":seed,"B_committed_empty_setup":initial_b});
    // Fixed roots/filter membership is setup work, not per-frame observer work.
    let mut fixture_scan_passes = 0;
    let logical = fixture_logical_count(roots[target], filter, &mut fixture_scan_passes);
    let fixture_scans_before_t0 = fixture_scan_passes;
    let mut frame_diagnostics = FrameDiagnostics::new();
    let start = Instant::now();
    driver.app.request_index_refresh();
    let mut primary = driver.app.shell.indexing.pending_request_id.unwrap();
    let mut ledger = Vec::new();
    let mut plans = Vec::new();
    let mut deferred_allocations = HashMap::new();
    record_planned_new(&driver, &mut plans, tabs[target], 1);
    collect_requests(&driver, &mut ledger);
    if profile.stable() {
        assert!(
            planned_switch(&mut driver, 0, &mut plans, &mut deferred_allocations),
            "stable A switch must activate its settled snapshot"
        );
        assert_eq!(driver.app.shell.indexing.warm_tab_id, Some(tabs[1]));
        assert!(
            driver.app.shell.indexing.pending_request_id.is_none(),
            "A must remain settled without refreshing"
        );
    } else if condition
        && matches!(
            profile,
            Profile::Warm | Profile::Promotion | Profile::WarmReclaim
        )
    {
        driver.app.request_background_index_refresh_for_tab(1);
        record_planned_new(&driver, &mut plans, tabs[1], 1);
        collect_requests(&driver, &mut ledger);
    }
    let mut events = Vec::<serde_json::Value>::new();
    let mut transition_events = Vec::<serde_json::Value>::new();
    let mut finished_warm_waits = Vec::<serde_json::Value>::new();
    let mut pending_transition: Option<(usize, usize)> = None;
    let mut gui_sort_completions = Vec::new();
    let mut driver_overhead = Duration::ZERO;
    let mut stable_previous_input = None;
    let mut stable_input_admissions = Vec::new();
    let mut stage = 0;
    let mut done_inputs = !condition
        || !matches!(
            profile,
            Profile::IgnoreCase
                | Profile::MidIgnore
                | Profile::SearchIgnore
                | Profile::EditFiles
                | Profile::NameShown
                | Profile::ModifiedShown
                | Profile::NameAll
                | Profile::ModifiedAll
                | Profile::Promotion
                | Profile::TabChain
                | Profile::StableSelective
                | Profile::StableDense
                | Profile::StableEdit
                | Profile::Preview
        );
    let mut t2 = None;
    let mut t2_load = None;
    let mut cutoff = Duration::ZERO;
    let mut frame_max = Duration::ZERO;
    let mut frames = 0;
    let mut high_water = HashMap::<(u64, u8, usize), usize>::new();
    let mut previous = Vec::new();
    let mut last_progress = Duration::ZERO;
    let mut max_gap = Duration::ZERO;
    let mut previous_ingested = 0;
    let mut last_ingest = Duration::ZERO;
    let mut max_ingest_gap = Duration::ZERO;
    let mut deadline = ProgressDeadline::new(Duration::from_secs(30));
    let t3;
    loop {
        let loop_begin = Instant::now();
        collect_requests(&driver, &mut ledger);
        if all_index_debt(&driver) || ledger.iter().any(|r| !request_complete(&driver, r, &plans)) {
            cutoff = start.elapsed();
        }
        let primary_tab = tabs[target];
        let ingested = owned_background_ingested(
            driver.app.shell.runtime.all_entries.len(),
            owned_count(&driver, primary_tab, Some(primary)),
        );
        let frame_begin = Instant::now();
        let mut completed_transition_this_frame = false;
        if let Some((n, input_stage)) = pending_transition {
            if dependent_input_allowed(
                tabs[n],
                driver.app.current_tab_id(),
                driver.app.shell.tabs.pending_activation_tab_id,
            ) {
                completed_transition_this_frame = true;
                transition_events.push(serde_json::json!({"stage":input_stage,"actual_activated_ms":ms(start.elapsed()),"tab_id":tabs[n],"root":roots[n].root}));
                if profile == Profile::TabChain {
                    assert!(acknowledge_tabchain_switch(
                        &driver,
                        &mut plans,
                        input_stage,
                        tabs[n],
                        &roots[n].root
                    ));
                    permit_revocation(
                        &driver,
                        &mut plans,
                        tabs[n],
                        "explicit-TabChain-refresh-after-activation",
                    );
                    driver.app.request_index_refresh();
                    refresh_allocation(&driver, &mut plans, &mut deferred_allocations, tabs[n]);
                    collect_requests(&driver, &mut ledger);
                    transition_events.push(serde_json::json!({"stage":input_stage,"explicit_refresh_ms":ms(start.elapsed()),"tab_id":tabs[n],"root":roots[n].root}));
                    if input_stage == 2 {
                        primary = driver
                            .app
                            .shell
                            .indexing
                            .pending_request_id
                            .unwrap_or(primary);
                        done_inputs = true;
                    }
                } else if input_stage == 1 {
                    done_inputs = true;
                }
                pending_transition = None;
                stage = input_stage + 1;
            }
        }
        if condition
            && !done_inputs
            && pending_transition.is_none()
            && !completed_transition_this_frame
        {
            let threshold = match stage {
                0 => logical / 10,
                1 => logical / 4,
                _ => logical / 2,
            };
            let checkpoint = if profile == Profile::TabChain && stage > 0 {
                owned_count(
                    &driver,
                    tabs[stage.min(2)],
                    Some(measured_request_for(
                        &driver,
                        &plans,
                        &ledger,
                        tabs[stage.min(2)],
                    )),
                )
            } else {
                ingested
            };
            let threshold = if profile == Profile::TabChain {
                logical / 10
            } else {
                threshold
            };
            if checkpoint >= threshold.max(1) {
                let input_at = Instant::now();
                let mut input_admitted = true;
                match profile {
                    Profile::IgnoreCase => {
                        driver.app.set_ignore_case(true);
                        done_inputs = true;
                    }
                    Profile::MidIgnore => {
                        permit_revocation(
                            &driver,
                            &mut plans,
                            tabs[0],
                            "IgnoreList-PreserveSort-refresh",
                        );
                        driver.app.shell.ui.ignore_list_enabled = true;
                        driver
                            .app
                            .maybe_reindex_from_filter_toggles(false, false, false, true);
                        refresh_allocation(&driver, &mut plans, &mut deferred_allocations, tabs[0]);
                        collect_requests(&driver, &mut ledger);
                        primary = driver
                            .app
                            .shell
                            .indexing
                            .pending_request_id
                            .unwrap_or(primary);
                        done_inputs = true;
                    }
                    Profile::SearchIgnore | Profile::StableSelective | Profile::StableDense => {
                        driver.query(profile.query(true));
                        done_inputs = true;
                    }
                    Profile::EditFiles | Profile::StableEdit => {
                        // Observe only our latest preceding A dispatch. Clone the scalar
                        // observation under its short lock, then drive the normal frame
                        // even when input admission is still pending.
                        let preceding = if full && profile == Profile::StableEdit && stage > 0 {
                            let expected = StableQueryIdentity {
                                tab: tabs[0],
                                root: &fixture.root,
                                query: if stage == 1 { "item" } else { "needle" },
                                candidates: fixture.expected.len(),
                                candidate_ptr: stable_ptr.unwrap(),
                                epoch: driver.app.shell.indexing.kind_resolution_epoch,
                                input_at: stable_previous_input
                                    .expect("preceding admitted stable input"),
                            };
                            driver.app.shell.indexing.perf_search_bindings.iter().rev()
                                .find(|b| b.tab_id == expected.tab && b.at >= expected.input_at)
                                .and_then(|b| driver.app.shell.search.perf_workers.get(&b.request_id)
                                    .map(|w| (b, w.lock().expect("own search observation").clone())))
                                .filter(|(b, w)| stable_query_evaluated(b, w, &expected)
                                    && w.evaluation_completed_at.unwrap() <= input_at)
                                .map(|(b, w)| serde_json::json!({
                                    "previous_stage":stage-1,"next_stage":stage,"previous_query":expected.query,
                                    "previous_input_ms":ms(expected.input_at.duration_since(start)),
                                    "input_admitted_ms":ms(input_at.duration_since(start)),
                                    "request_id":b.request_id,"tab_id":b.tab_id,"root":b.root,"epoch":b.epoch,
                                    "candidate_count":b.candidates,"evaluated_candidates":w.evaluated_candidates,
                                    "dispatched_ms":ms(b.at.duration_since(start)),
                                    "started_ms":ms(w.started_at.unwrap().duration_since(start)),
                                    "evaluation_completed_ms":ms(w.evaluation_completed_at.unwrap().duration_since(start))
                                }))
                        } else {
                            None
                        };
                        if stable_edit_input_allowed(full, profile, stage, preceding.is_some()) {
                            if let Some(proof) = preceding {
                                stable_input_admissions.push(proof);
                            }
                            driver.query(match stage {
                                0 => "item",
                                1 => "needle",
                                _ => "",
                            });
                            if full && profile == Profile::StableEdit {
                                stable_previous_input = Some(input_at);
                            }
                            if stage == 2 {
                                done_inputs = true;
                            }
                        } else {
                            input_admitted = false;
                        }
                    }
                    Profile::NameShown
                    | Profile::ModifiedShown
                    | Profile::NameAll
                    | Profile::ModifiedAll => {
                        let has_rows = !driver.app.shell.runtime.base_results.is_empty();
                        driver.app.set_result_sort_scope(scope);
                        driver.app.set_result_sort_mode(mode);
                        if profile == Profile::NameShown && has_rows {
                            gui_sort_completions.push(Instant::now());
                        }
                        done_inputs = true;
                    }
                    Profile::Promotion => {
                        let n = if stage == 0 { 1 } else { 0 };
                        if planned_switch(&mut driver, n, &mut plans, &mut deferred_allocations) {
                            transition_events.push(serde_json::json!({"stage":stage,"actual_activated_ms":ms(start.elapsed()),"tab_id":tabs[n],"root":roots[n].root}));
                            if stage == 1 {
                                done_inputs = true;
                            }
                        } else {
                            pending_transition = Some((n, stage));
                        }
                        collect_requests(&driver, &mut ledger);
                    }
                    Profile::TabChain => {
                        let n = if stage == 2 { 0 } else { stage + 1 };
                        let before_declaration = plans.clone();
                        declare_tabchain_victim(
                            &driver,
                            &mut plans,
                            stage,
                            tabs[n],
                            &seed_witnesses,
                            &tabs,
                        );
                        let admission = planned_switch_guarded(
                            &mut driver,
                            n,
                            &mut plans,
                            &mut deferred_allocations,
                            if full {
                                WarmSwitchPolicy::ObservedCompletion
                            } else {
                                WarmSwitchPolicy::CommittedSmall
                            },
                        );
                        if let SwitchAdmission::WaitForFinishedWarm(id) = admission {
                            plans = before_declaration;
                            input_admitted = false;
                            if !finished_warm_waits.iter().any(|w| w["stage"] == stage) {
                                assert!(finished_warm_waits.len() < 3, "bounded fixed trace waits");
                                finished_warm_waits.push(serde_json::json!({"stage":stage,"request_id":id,"checkpoint":checkpoint,"start_ms":ms(input_at.duration_since(start)),"end_ms":null,"reason":if full{"observed-completion-own-commit-before-Warm-eviction"}else{"sub100k-correctness-old-Warm-own-commit-before-eviction"}}));
                            }
                        } else if admission == SwitchAdmission::Activated(true) {
                            assert!(acknowledge_tabchain_switch(
                                &driver,
                                &mut plans,
                                stage,
                                tabs[n],
                                &roots[n].root
                            ));
                            transition_events.push(serde_json::json!({"stage":stage,"actual_activated_ms":ms(start.elapsed()),"tab_id":driver.app.current_tab_id(),"root":driver.app.shell.runtime.root}));
                            permit_revocation(
                                &driver,
                                &mut plans,
                                tabs[n],
                                "explicit-TabChain-refresh",
                            );
                            driver.app.request_index_refresh();
                            refresh_allocation(
                                &driver,
                                &mut plans,
                                &mut deferred_allocations,
                                tabs[n],
                            );
                            transition_events.push(serde_json::json!({"stage":stage,"explicit_refresh_ms":ms(start.elapsed()),"tab_id":tabs[n],"root":roots[n].root}));
                            if stage == 2 {
                                primary = driver
                                    .app
                                    .shell
                                    .indexing
                                    .pending_request_id
                                    .unwrap_or(primary);
                                done_inputs = true;
                            }
                        } else {
                            pending_transition = Some((n, stage));
                        }
                        collect_requests(&driver, &mut ledger);
                    }
                    Profile::Preview => {
                        driver.app.shell.ui.show_preview = true;
                        assert!(driver.app.shell.runtime.results.len() > stage);
                        driver.app.shell.runtime.set_current_row(Some(stage));
                        driver.app.request_preview_for_current();
                        if stage == 2 {
                            done_inputs = true;
                        }
                    }
                    _ => {
                        done_inputs = true;
                    }
                }
                if input_admitted {
                    if let Some(wait) = finished_warm_waits.iter_mut().find(|w| w["stage"] == stage)
                    {
                        wait["end_ms"] = ms(input_at.duration_since(start)).into();
                    }
                    events.push(serde_json::json!({"stage":stage,"at_ms":ms(input_at.duration_since(start)),"GUI_ingested":checkpoint,"query":match profile{Profile::EditFiles|Profile::StableEdit=>match stage{0=>"item",1=>"needle",_=>""},_=>profile.query(true)},"active_tab":driver.app.current_tab_id(),"input":"production-handler","requested_tab":pending_transition.map(|(n,_)|tabs[n]).or(driver.app.current_tab_id()),"requested_root":pending_transition.map(|(n,_)|&roots[n].root).unwrap_or(&driver.app.shell.runtime.root)}));
                    if pending_transition.is_none() {
                        stage += 1;
                    }
                }
            }
        }
        if all_index_debt(&driver) || ledger.iter().any(|r| !request_complete(&driver, r, &plans)) {
            cutoff = start.elapsed();
        }
        driver.frame();
        frame_max = frame_max.max(driver.last_frame_end.duration_since(frame_begin));
        frames += 1;
        capture_deferred(&driver, &mut plans, &mut deferred_allocations);
        record_planned_new(&driver, &mut plans, primary_tab, 0);
        let now = driver.last_frame_end.duration_since(start);
        collect_requests(&driver, &mut ledger);
        if profile == Profile::TabChain {
            if let Some((n, input_stage)) = pending_transition {
                // Activation may finish inside this frame: acknowledge now, before
                // this frame's owner selection; dependent input remains next frame.
                acknowledge_tabchain_switch(
                    &driver,
                    &mut plans,
                    input_stage,
                    tabs[n],
                    &roots[n].root,
                );
            }
        }
        let primary_before_latest_map = primary;
        if matches!(profile, Profile::MidIgnore | Profile::TabChain) {
            primary = measured_request_for(&driver, &plans, &ledger, tabs[target]);
        }
        if profile.stable() {
            assert_eq!(driver.app.current_tab_id(), Some(tabs[0]));
            assert_eq!(driver.app.shell.runtime.root, roots[0].root);
            assert_eq!(
                driver.app.shell.runtime.entries.len(),
                fixture.expected.len(),
                "A candidate cardinality remains fixed"
            );
            assert_eq!(
                Arc::as_ptr(&driver.app.shell.runtime.entries) as usize,
                stable_ptr.unwrap(),
                "A candidate Arc must remain unchanged without pinning"
            );
            assert!(
                request_for(&driver, tabs[0]).is_none(),
                "A does not refresh"
            );
        }
        let i = &driver.app.shell.indexing;
        for r in &ledger {
            let count = owned_count(&driver, r.tab, Some(r.id));
            let f = i.background_finalizations.get(&r.id);
            for (phase, value) in [
                (0, count),
                (1, f.map_or(0, |f| f.filter_cursor)),
                (2, f.map_or(0, |f| f.kind_cursor)),
            ] {
                let mark = high_water.entry((r.id, phase, stage)).or_default();
                *mark = (*mark).max(value);
            }
        }
        if let Some(f) = &i.build.active_filter {
            let mark = high_water.entry((primary, 3, stage)).or_default();
            *mark = (*mark).max(f.cursor);
        }
        let evaluated = i
            .perf_aux
            .iter()
            .filter(|o| o.successful && o.delivered_at.is_some())
            .count()
            + driver
                .app
                .shell
                .search
                .perf_workers
                .values()
                .map(|w| worker_progress(&w.lock().unwrap()).0)
                .sum::<usize>();
        let progress = vec![
            high_water.values().sum(),
            ledger.iter().map(|r| r.observation().entries_emitted).sum(),
            evaluated,
        ];
        max_gap = max_gap.max(now.saturating_sub(last_progress));
        if progress != previous {
            last_progress = now;
            previous = progress;
            assert!(
                !deadline.observe(now, true),
                "constructive progress deadline {}",
                profile.name()
            );
        } else {
            let stalled = deadline.observe(now, false);
            if stalled {
                let observed = ledger
                    .iter()
                    .map(|r| {
                        serde_json::json!({
                            "id":r.id,"tab":r.tab,"observation":format!("{:?}",r.observation()),
                            "physical_complete":request_complete(&driver,r,&plans),
                            "released":i.perf_released_requests.contains_key(&r.id)
                        })
                    })
                    .collect::<Vec<_>>();
                let planned = plans.iter().map(|p| serde_json::json!({
                    "id":p.id,"tab":p.tab,"revoked_by":p.revoked_by,"completed_predecessor":p.completed_predecessor,
                    "permission_at":format!("{:?}",p.permission_at),"permission_reason":p.permission_reason,
                    "requires_latest_generation":requires_latest_generation(&plans,p.id,p.tab)
                })).collect::<Vec<_>>();
                let request_proofs = proofs(&driver, &ledger, &plans, &roots, &tabs, source);
                let proof_rows = request_proofs.iter().map(|p| serde_json::json!({
                    "id":p.id,"tab":p.tab,"finished":p.finished,"physical_complete":p.physical_complete,
                    "source_root_correct":p.source_root_correct,"revoked_abort":p.revoked_abort,
                    "last_good_victim":p.last_good_victim
                })).collect::<Vec<_>>();
                let snapshots = tabs.iter().enumerate().map(|(n,id)| {
                    let t=driver.app.shell.tabs.get(n).unwrap();
                    let active=driver.app.current_tab_id()==Some(*id);
                    let (all,visible,fresh)=if active {
                        let r=&driver.app.shell.runtime;(&r.all_entries,&r.entries,r.freshness.as_ref())
                    } else {
                        let r=&t.result_state.committed;(&r.all_entries,&r.entries,r.freshness.as_ref())
                    };
                    serde_json::json!({"tab":id,"active":active,"expected_root":roots[n].root,
                        "all_count":all.len(),"visible_count":visible.len(),"oracle_valid":quiet_oracles[n].valid_snapshot(all,visible),
                        "signature":actual_signature(&roots[n].root,all),"lifecycle":format!("{:?}",t.index_state.lifecycle()),
                        "freshness_request":fresh.map(|f|f.request_id),"freshness_root":fresh.map(|f|&f.root),"freshness_source":fresh.map(|f|format!("{:?}",f.source))})
                }).collect::<Vec<_>>();
                let victim_diagnostics = plans.iter().filter_map(|p| {
                    let c = p.victim.as_ref()?;
                    let r = ledger.iter().find(|r| r.id == p.id && r.tab == p.tab)?;
                    let o = r.observation();
                    let n = tabs.iter().position(|id| *id == p.tab).unwrap();
                    let t = driver.app.shell.tabs.get(n).unwrap();
                    let (all, visible, fresh) = if driver.app.current_tab_id() == Some(p.tab) {
                        let r = &driver.app.shell.runtime;
                        (&r.all_entries, &r.entries, r.freshness.as_ref())
                    } else {
                        let r = &t.result_state.committed;
                        (&r.all_entries, &r.entries, r.freshness.as_ref())
                    };
                    let load = i.tx.load();
                    let state = VictimFastState {
                        seed_id: fresh.map(|f| f.request_id),
                        root_source_match: fresh.is_some_and(|f| f.root == c.seed.root && match (c.seed.source, &f.source) {
                            (Source::Walker, IndexSource::Walker) => true,
                            (Source::FileList, IndexSource::FileList(path)) => path == &c.seed.root.join("FileList.txt"),
                            _ => false,
                        }),
                        counts_match: all.len() == c.seed.all_count && visible.len() == c.seed.visible_count,
                        latest_without_successor: requires_latest_generation(&plans, p.id, p.tab),
                        released: i.perf_released_requests.contains_key(&p.id),
                        route_absent: !i.request_tabs.contains_key(&p.id),
                        worker_load_zero: load.queued == 0 && load.inflight == 0,
                        index_debt_zero: !all_index_debt(&driver),
                        result_debt_zero: !result_debt(&driver),
                    };
                    let matching = i.perf_preemptions.iter().filter(|e| e.victim_id == p.id).take(128).map(|e| {
                        serde_json::json!({
                            "event": {"at":format!("{:?}",e.at),"victim_id":e.victim_id,"victim_tab":e.victim_tab,
                                "prior_latest":e.prior_latest,"replacement_id":e.replacement_id,"active_tab":e.active_tab,"warm_tab":e.warm_tab,
                                "pending_active_id":e.pending_active_id,"latest_active_id":e.latest_active_id,
                                "queued_active_ids":e.queued_active_ids.iter().take(128).collect::<Vec<_>>(),"inflight_count":e.inflight_count},
                            "cause_valid":declared_preemption_cause(p,e,&o),"execution_valid":declared_victim_execution(p,e,&o),
                            "last_good_fast_valid":last_good_victim_fast(p,e,&o,&state),
                            "chronology":{"admitted_before_started":o.admitted_at.zip(o.started_published).is_some_and(|(a,b)|a<=b),
                                "started_before_mutation":o.started_published.is_some_and(|a|a<=e.at),
                                "mutation_before_offer":o.terminal_offered.is_some_and(|a|e.at<=a),
                                "offer_before_published":o.terminal_offered.zip(o.terminal_published).is_some_and(|(a,b)|a<=b),
                                "published_before_return":o.terminal_published.zip(o.request_processing_returned).is_some_and(|(a,b)|a<=b)}
                        })
                    }).collect::<Vec<_>>();
                    Some(serde_json::json!({"id":p.id,"tab":p.tab,
                        "declaration":{"profile":c.profile.name(),"old_warm_tab":c.old_warm_tab,"trace_tabs":c.trace_tabs,"stage":c.stage,
                            "declared_at":format!("{:?}",c.declared_at),"closed_at":format!("{:?}",c.closed_at),
                            "victim_id":c.victim_id,"victim_tab":c.victim_tab,"active_tab":c.active_tab,"current_warm_tab":c.current_warm_tab,"incoming_ids":c.incoming_ids},
                        "seed":{"verified":c.seed.verified,"id":c.seed.id,"root":c.seed.root,"source":c.seed.source.name(),
                            "all_count":c.seed.all_count,"visible_count":c.seed.visible_count,"all_signature":c.seed.all_signature,"visible_signature":c.seed.visible_signature},
                        "fast_state":{"seed_id":state.seed_id,"root_source_match":state.root_source_match,"counts_match":state.counts_match,
                            "latest_without_successor":state.latest_without_successor,"released":state.released,"route_absent":state.route_absent,
                            "worker_load_zero":state.worker_load_zero,"index_debt_zero":state.index_debt_zero,"result_debt_zero":state.result_debt_zero},
                        "completed_predecessor_observed_at":format!("{:?}",p.completed_predecessor_observed_at),
                        "matching_events":matching,"observation":format!("{:?}",o)}))
                }).collect::<Vec<_>>();
                let preemption_diagnostics = i.perf_preemptions.iter().take(128).map(|e| serde_json::json!({
                    "at":format!("{:?}",e.at),"victim_id":e.victim_id,"victim_tab":e.victim_tab,"prior_latest":e.prior_latest,
                    "replacement_id":e.replacement_id,"active_tab":e.active_tab,"warm_tab":e.warm_tab,"pending_active_id":e.pending_active_id,
                    "latest_active_id":e.latest_active_id,"queued_active_ids":e.queued_active_ids.iter().take(128).collect::<Vec<_>>(),"inflight_count":e.inflight_count
                })).collect::<Vec<_>>();
                let actual_switch_acks = plans.iter().filter_map(|p|p.victim.as_ref().map(|c|serde_json::json!({"victim_id":p.id,"stage":c.stage,"ack":c.switch_ack.as_ref().map(|a|serde_json::json!({"at":format!("{:?}",a.at),"tab":a.tab,"root":a.root,"pending_activation":a.pending_activation}))}))).collect::<Vec<_>>();
                let load = i.tx.load();
                eprintln!(
                    "INDEX_PERF_STALL {}",
                    serde_json::json!({
                        "case":profile.name(),"source":source.name(),"condition":condition,"stage":stage,"done_inputs":done_inputs,
                        "frame":frames,"elapsed_ms":ms(now),"last_progress_ms":ms(last_progress),"primary":primary,
                        "current_tab":driver.app.current_tab_id(),"pending_request":i.pending_request_id,
                        "latest_map":format!("{:?}",i.latest_request_ids.lock().unwrap()),"request_tabs":i.request_tabs,
                        "pending_transition":pending_transition,"deferred_allocations":deferred_allocations,
                        "queue":i.pending_queue.iter().map(|r|(r.request_id,r.tab_id)).collect::<Vec<_>>(),"inflight":i.inflight_requests,
                        "load":{"queued":load.queued,"inflight":load.inflight,"capacity":load.capacity},
                        "index_debt":all_index_debt(&driver),"result_debt":result_debt(&driver),
                        "plans_valid":planned_requests_valid(&plans,&request_proofs),"planned":planned,"requests":observed,
                        "proofs":proof_rows,"snapshots":snapshots,"input_trace":events,"transition_trace":transition_events,
                        "victim_diagnostics":victim_diagnostics,"preemption_diagnostics":preemption_diagnostics,
                        "preemption_overflow":i.perf_preemption_overflow,"preemption_set_valid":preemption_events_valid(&driver,&plans,&ledger),
                        "warm_removal_overflow":i.perf_warm_removal_overflow,"warm_removals":format!("{:?}",i.perf_warm_removals),
                        "actual_switch_acks":actual_switch_acks
                    })
                );
            }
            assert!(!stalled, "constructive stall {}", profile.name());
        }
        let current_ingested = owned_count(&driver, tabs[target], Some(primary));
        if current_ingested > previous_ingested {
            max_ingest_gap = max_ingest_gap.max(now.saturating_sub(last_ingest));
            last_ingest = now;
            previous_ingested = current_ingested;
        }
        let primary_observation = ledger
            .iter()
            .find(|r| r.id == primary)
            .unwrap_or_else(|| {
                let allocations = i.perf_allocations.iter().map(|a| {
                    serde_json::json!({"id":a.id,"tab":a.tab,"observation":format!("{:?}",a.observation.lock().unwrap())})
                }).collect::<Vec<_>>();
                let planned = plans.iter().map(|p| {
                    serde_json::json!({"id":p.id,"tab":p.tab,"revoked_by":p.revoked_by,"completed_predecessor":p.completed_predecessor,"permission_reason":p.permission_reason,"permission_at":format!("{:?}",p.permission_at)})
                }).collect::<Vec<_>>();
                eprintln!("INDEX_PERF_MISSING_PRIMARY {}", serde_json::json!({
                    "case":profile.name(),"source":source.name(),"condition":condition,
                    "stage":stage,"done_inputs":done_inputs,"frame":frames,"elapsed_ms":ms(now),
                    "primary":primary,"primary_before_latest_map":primary_before_latest_map,
                    "target_tab":tabs[target],"target_root":roots[target].root,
                    "current_tab":driver.app.current_tab_id(),"current_root":driver.app.shell.runtime.root,
                    "latest_map":format!("{:?}",i.latest_request_ids.lock().unwrap()),
                    "pending_request":i.pending_request_id,"pending_activation":driver.app.shell.tabs.pending_activation_tab_id,
                    "pending_transition":pending_transition,"queue":i.pending_queue.iter().map(|r| (r.request_id,r.tab_id)).collect::<Vec<_>>(),
                    "inflight":i.inflight_requests,"request_tabs":i.request_tabs,
                    "released":i.perf_released_requests.keys().collect::<Vec<_>>(),
                    "ledger":ledger.iter().map(|r| (r.id,r.tab)).collect::<Vec<_>>(),
                    "planned":planned,"allocations":allocations
                }));
                panic!("primary request {primary} is absent from the measured allocation ledger");
            })
            .observation();
        let released = ledger
            .iter()
            .all(|r| i.perf_released_requests.contains_key(&r.id));
        let debt =
            all_index_debt(&driver) || ledger.iter().any(|r| !request_complete(&driver, r, &plans));
        let pending_results = result_debt(&driver);
        frame_diagnostics.push(
            frame_begin.duration_since(start),
            now,
            FrameState {
                active_filter_cursor: i.build.active_filter.as_ref().map(|filter| filter.cursor),
                observed_ms: ms(start.elapsed()),
                active_tab: driver.app.current_tab_id(),
                pending_request_id: i.pending_request_id,
                primary_request_id: primary,
                ingested: current_ingested,
                index_debt: debt
                    || !deferred_allocations.is_empty()
                    || pending_transition.is_some(),
                result_debt: pending_results,
                producer_data_end_ms: primary_observation
                    .data_publish_end
                    .map(|at| ms(at.duration_since(start))),
                producer_terminal_ms: primary_observation
                    .terminal_published
                    .map(|at| ms(at.duration_since(start))),
            },
        );
        let snapshot_valid = if profile.stable() {
            driver
                .app
                .shell
                .tabs
                .get(1)
                .unwrap()
                .result_state
                .committed
                .all_entries
                .len()
                == logical
        } else {
            driver.app.shell.runtime.all_entries.len() == logical
        };
        let truth = ExtendedTruth {
            successful_terminal: primary_observation.terminal_kind == Some("finished")
                && preemption_events_valid(&driver, &plans, &ledger)
                && planned_requests_valid(
                    &plans,
                    &proofs(&driver, &ledger, &plans, &roots, &tabs, source),
                ),
            owned_physical_complete: ledger.iter().all(|r| request_complete(&driver, r, &plans)),
            owned_request_released: released,
            snapshot_valid,
            debt: debt || !deferred_allocations.is_empty() || pending_transition.is_some(),
            latest_results_valid: driver.app.shell.runtime.query_state.query
                == profile.query(condition)
                && driver.app.shell.runtime.result_sort_mode == mode
                && driver.app.shell.runtime.result_sort_scope == scope
                && (if scope == ResultSortScope::AllMatches {
                    expected.valid_results(
                        &driver.app.shell.runtime.results,
                        driver.app.shell.runtime.total_match_count,
                    )
                } else {
                    expected.valid_shown_results(
                        &driver.app.shell.runtime.results,
                        driver.app.shell.runtime.total_match_count,
                        &driver.app.shell.runtime.base_results,
                    )
                })
                && !pending_results,
        };
        if truth.index_ready() && done_inputs && t2.is_none() {
            let candidate_t2_load = driver.app.shell.indexing.tx.load();
            for p in plans
                .iter()
                .filter(|p| requires_latest_generation(&plans, p.id, p.tab))
            {
                let n = tabs.iter().position(|id| *id == p.tab).unwrap();
                if proofs(&driver, &ledger, &plans, &roots, &tabs, source)
                    .iter()
                    .any(|proof| proof.id == p.id && proof.last_good_victim)
                {
                    continue; // Full retained-seed oracle runs after tentative t3, before any output.
                }
                if driver.app.current_tab_id() == Some(p.tab) {
                    if !expected.valid_snapshot(
                        &driver.app.shell.runtime.all_entries,
                        &driver.app.shell.runtime.entries,
                    ) {
                        let runtime = &driver.app.shell.runtime;
                        let filter = profile.filter(condition);
                        let records = roots[0]
                            .expected
                            .iter()
                            .filter(|r| if r.is_dir { filter.dirs } else { filter.files })
                            .collect::<Vec<_>>();
                        let paths = records.iter().map(|r| &r.path).collect::<HashSet<_>>();
                        let actual = runtime
                            .all_entries
                            .iter()
                            .map(|e| &e.path)
                            .collect::<HashSet<_>>();
                        let first = runtime
                            .all_entries
                            .iter()
                            .zip(&records)
                            .enumerate()
                            .find(|(_, (e, r))| e.path != r.path);
                        let freshness = runtime.freshness.as_ref();
                        let by_path = records
                            .iter()
                            .map(|r| (&r.path, r.is_dir))
                            .collect::<HashMap<_, _>>();
                        eprintln!(
                            "INDEX_PERF_FAILURE {}",
                            serde_json::json!({
                                "profile":profile.name(),"source":source.name(),"condition":condition,"stage":stage,"done_inputs":done_inputs,"pending_transition":pending_transition,
                                "current_tab":driver.app.current_tab_id(),"latest_planned_tab":p.tab,"latest_planned_request":p.id,"active_root":runtime.root,"expected_root":roots[0].root,
                                "freshness_root":freshness.map(|f| &f.root),"freshness_source":freshness.map(|f|format!("{:?}",f.source)),"freshness_request":freshness.map(|f|f.request_id),
                                "all_len":runtime.all_entries.len(),"visible_len":runtime.entries.len(),"expected_all_len":records.len(),"unique_paths":actual.len(),"missing_paths":paths.difference(&actual).count(),"extra_paths":actual.difference(&paths).count(),
                                "kind_mismatches":runtime.all_entries.iter().filter(|e| by_path.get(&e.path).is_some_and(|is_dir|e.kind.is_some_and(|k|k.is_dir!=Some(*is_dir)))).count(),
                                "first_order_mismatch":first.map(|(index,(entry,record))|serde_json::json!({"index":index,"actual_relative":entry.path.strip_prefix(&roots[0].root).ok(),"expected_relative":record.path.strip_prefix(&roots[0].root).ok(),"actual_kind":format!("{:?}",entry.kind),"expected_dir":record.is_dir})),
                                "quiet_oracle_matches":quiet_oracles.iter().map(|o|o.valid_snapshot(&runtime.all_entries,&runtime.entries)).collect::<Vec<_>>(),"input_trace":events,"transition_trace":transition_events
                            })
                        );
                        panic!("latest active request membership");
                    }
                } else {
                    let snapshot = &driver.app.shell.tabs.get(n).unwrap().result_state.committed;
                    assert!(
                        quiet_oracles[n].valid_snapshot(&snapshot.all_entries, &snapshot.entries),
                        "latest background request membership"
                    );
                }
            }
            t2 = Some(now);
            t2_load = Some(candidate_t2_load);
        }
        if truth.results_ready() && done_inputs {
            if profile == Profile::Preview && condition {
                let path = &driver.app.shell.runtime.results
                    [driver.app.shell.runtime.current_row.unwrap()]
                .0;
                let text = fs::read_to_string(path).unwrap();
                if driver.app.shell.runtime.preview.is_empty()
                    && driver.app.shell.runtime.preview_document.is_none()
                {
                    driver.paced_frame();
                    continue;
                }
                let shown = driver
                    .app
                    .shell
                    .runtime
                    .preview_document
                    .as_ref()
                    .map(|d| d.body())
                    .unwrap_or(&driver.app.shell.runtime.preview);
                assert!(
                    shown.contains(text.trim()),
                    "latest preview content must match own selected path"
                );
            }
            driver_overhead += loop_begin
                .elapsed()
                .saturating_sub(driver.last_frame_end.duration_since(frame_begin));
            t3 = now;
            break;
        }
        assert!(
            now < Duration::from_secs(120),
            "extension did not settle {:?}/{source:?}/condition={condition};stage={stage};query={};debt={debt};released={released};snapshot={snapshot_valid};all={};visible={};results={};total={};target={primary}",
            profile,
            driver.app.shell.runtime.query_state.query,
            driver.app.shell.runtime.all_entries.len(),
            driver.app.shell.runtime.entries.len(),
            driver.app.shell.runtime.results.len(),
            driver.app.shell.runtime.total_match_count
        );
        driver_overhead += loop_begin
            .elapsed()
            .saturating_sub(driver.last_frame_end.duration_since(frame_begin));
        let next = frame_begin + FRAME_PERIOD;
        if let Some(wait) = next.checked_duration_since(Instant::now()) {
            thread::sleep(wait);
        }
    }
    let final_proofs = proofs(&driver, &ledger, &plans, &roots, &tabs, source);
    assert!(preemption_events_valid(&driver, &plans, &ledger));
    let mut victim_rows = Vec::new();
    for proof in final_proofs.iter().filter(|p| p.last_good_victim) {
        let p = plans.iter().find(|p| p.id == proof.id).unwrap();
        let c = p.victim.as_ref().unwrap();
        let n = tabs.iter().position(|id| *id == p.tab).unwrap();
        assert_ne!(
            driver.app.current_tab_id(),
            Some(p.tab),
            "retained victim must be inactive"
        );
        let snapshot = &driver.app.shell.tabs.get(n).unwrap().result_state.committed;
        assert!(
            full_retained_seed_valid(
                &c.seed,
                &quiet_oracles[n],
                &snapshot.all_entries,
                &snapshot.entries
            ),
            "post-t3 full independent retained seed oracle"
        );
        let victim_observation = ledger.iter().find(|r| r.id == p.id).unwrap().observation();
        victim_rows.push(serde_json::json!({"request_id":p.id,"tab_id":p.tab,"role":retained_victim_role(&victim_observation),"stage":c.stage,"declared_ms":ms(c.declared_at.duration_since(start)),"closed_ms":c.closed_at.map(|at|ms(at.duration_since(start))),"incoming_request_ids":c.incoming_ids,"seed_request_id":c.seed.id,"seed_root":c.seed.root,"seed_source":c.seed.source.name(),"seed_all_count":c.seed.all_count,"seed_visible_count":c.seed.visible_count,"seed_all_signature":c.seed.all_signature,"seed_visible_signature":c.seed.visible_signature,"actual_seed_request_id":snapshot.freshness.as_ref().unwrap().request_id,"actual_seed_root":snapshot.freshness.as_ref().unwrap().root,"actual_seed_source":if matches!(&snapshot.freshness.as_ref().unwrap().source,IndexSource::Walker){"Walker"}else{"FileList"},"actual_all_count":snapshot.all_entries.len(),"actual_visible_count":snapshot.entries.len(),"actual_all_signature":seed_entry_signature(&c.seed.root,&snapshot.all_entries),"actual_visible_signature":seed_entry_signature(&c.seed.root,&snapshot.entries),"full_seed_oracle_after_t3":true,"partial_emitted_entries":ledger.iter().find(|r|r.id==p.id).unwrap().observation().entries_emitted,"workload_semantics":if victim_observation.terminal_kind==Some("failed"){"actual partial stale-full-failed work; not 100k indexing throughput"}else{"actual partial canceled work; not 100k indexing throughput"}}));
    }
    let t2 = t2.unwrap();
    if profile == Profile::TabChain && condition && !full {
        assert_eq!(ledger.len(), 4, "small TabChain owns exactly four requests");
        assert_eq!(events.len(), 3, "small TabChain admits all three inputs");
        for (event, tab) in events.iter().zip([tabs[1], tabs[2], tabs[0]]) {
            assert_eq!(event["requested_tab"], tab);
        }
        for (tab, count) in [(tabs[0], 2), (tabs[1], 1), (tabs[2], 1)] {
            assert_eq!(ledger.iter().filter(|r| r.tab == tab).count(), count);
        }
        for request in &ledger {
            let observation = request.observation();
            assert_eq!(observation.terminal_kind, Some("finished"));
            assert!(physical_request_complete(&observation));
            assert_eq!(observation.started_source, Some(source.name()));
            assert_eq!(observation.terminal_source, Some(source.name()));
            assert_eq!(
                observation.started_root.as_ref(),
                Some(&roots[tabs.iter().position(|tab| *tab == request.tab).unwrap()].root)
            );
        }
    }
    let allocated = driver
        .app
        .shell
        .indexing
        .perf_allocations
        .iter()
        .map(|a| a.id)
        .collect::<std::collections::BTreeSet<_>>();
    let recorded = ledger
        .iter()
        .map(|r| r.id)
        .collect::<std::collections::BTreeSet<_>>();
    let planned = plans
        .iter()
        .map(|p| p.id)
        .collect::<std::collections::BTreeSet<_>>();
    let released = driver
        .app
        .shell
        .indexing
        .perf_released_requests
        .iter()
        .filter(|(_, at)| **at >= start)
        .map(|(id, _)| *id)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(allocated, recorded, "complete allocation history");
    assert_eq!(recorded, planned, "exact operation-owned request manifest");
    assert_eq!(planned, released, "exact measured release history");
    assert!(
        expected.valid_snapshot(
            &driver.app.shell.runtime.all_entries,
            &driver.app.shell.runtime.entries
        ),
        "final active snapshot oracle {}",
        profile.name()
    );
    let mut unmeasured_tab_observations = Vec::new();
    for (n, oracle) in quiet_oracles.iter().enumerate() {
        if n == driver.app.shell.tabs.active_tab_index() {
            continue;
        }
        let tab = driver.app.shell.tabs.get(n).unwrap();
        let measured = plans.iter().any(|p| p.tab == tab.id);
        let seeded = matches!(profile, Profile::WarmReclaim | Profile::TabChain);
        if measured || seeded {
            assert!(
                oracle.valid_snapshot(
                    &tab.result_state.committed.all_entries,
                    &tab.result_state.committed.entries
                ),
                "background tab membership {} tab{n}",
                profile.name()
            );
        } else {
            assert!(
                tab.result_state.committed.all_entries.is_empty(),
                "unmeasured B all entries must remain empty"
            );
            assert!(
                tab.result_state.committed.entries.is_empty(),
                "unmeasured B visible entries must remain empty"
            );
            assert!(!tab_debt(tab), "unmeasured B must remain quiescent");
            assert!(!ledger.iter().any(|r| r.tab == tab.id));
            assert!(
                !driver
                    .app
                    .shell
                    .indexing
                    .perf_allocations
                    .iter()
                    .any(|a| a.tab == Some(tab.id)),
                "unmeasured B must have no index allocation"
            );
        }
        if !measured {
            let freshness = tab.result_state.committed.freshness.as_ref();
            unmeasured_tab_observations.push(serde_json::json!({"tab_id":tab.id,"prestate":if seeded{"untimed-seeded-owned-snapshot"}else{"configured-unmeasured-startup-empty"},"configured_root":tab.root,"actual_committed_root":freshness.map(|f|&f.root),"actual_committed_source":freshness.map(|f|format!("{:?}",f.source)),"actual_committed_generation":freshness.map(|f|f.request_id),"all_entries":tab.result_state.committed.all_entries.len(),"visible_entries":tab.result_state.committed.entries.len(),"index_debt":tab_debt(tab),"measured_allocation_count":0}));
        }
    }
    let quiesce = Instant::now();
    while driver.app.shell.search.perf_workers.values().any(|w| {
        let w = w.lock().unwrap();
        w.completed_at.is_none() && w.canceled_at.is_none()
    }) {
        assert!(quiesce.elapsed() < Duration::from_secs(5));
        thread::sleep(Duration::from_millis(1));
    }
    let workers = driver
        .app
        .shell
        .search
        .perf_workers
        .iter()
        .map(|(id, w)| (*id, w.lock().unwrap().clone()))
        .collect::<Vec<_>>();
    let genuine = workers
        .iter()
        .filter(|(_, w)| genuine_search_overlap(w, start, cutoff))
        .map(|(id, _)| *id)
        .collect::<HashSet<_>>();
    let bindings = &driver.app.shell.indexing.perf_search_bindings;
    let full_evaluations = workers
        .iter()
        .filter(|(id, w)| {
            genuine_search_overlap(w, start, cutoff)
                && w.evaluated_candidates == fixture.expected.len()
                && bindings.iter().any(|b| {
                    b.request_id == *id
                        && b.tab_id == tabs[0]
                        && b.root == fixture.root
                        && b.candidates == fixture.expected.len()
                        && Some(b.candidate_ptr) == stable_ptr
                        && b.at >= start
                })
        })
        .map(|(id, _)| *id)
        .collect::<HashSet<_>>();
    let query_executed = |query: &str| {
        bindings
            .iter()
            .any(|b| b.query == query && full_evaluations.contains(&b.request_id))
    };
    let aux = driver
        .app
        .shell
        .indexing
        .perf_aux
        .iter()
        .filter(|o| o.delivered_at.is_some())
        .map(|o| {
            let proof = DeliveredProof {
                dispatched: o.dispatched_at,
                delivered: o.delivered_at.unwrap(),
                owned_identity: tabs.contains(&o.tab_id),
                successful: o.successful,
                count: o.count,
            };
            (
                o.flow,
                proof.completed_while_unsettled(start, start + cutoff),
            )
        })
        .collect::<Vec<_>>();
    let aux_count = |flow| aux.iter().filter(|(f, ok)| *f == flow && *ok).count();
    let producers = ledger
        .iter()
        .filter_map(|r| {
            let o = r.observation();
            Some((
                r.tab,
                o.started_published?,
                o.data_publish_end?,
                o.entries_emitted,
            ))
        })
        .collect::<Vec<_>>();
    let concurrent_index_producers = producers.iter().enumerate().any(|(n, (tab, a, b, count))| {
        *count > 0
            && producers
                .iter()
                .skip(n + 1)
                .any(|(other, c, d, entries)| *entries > 0 && tab != other && a < d && c < b)
    });
    let index_input_progress = events
        .iter()
        .all(|e| e["GUI_ingested"].as_u64().unwrap_or(0) > 0);
    let sort_rx_completed = driver
        .app
        .shell
        .indexing
        .perf_aux
        .iter()
        .filter(|o| o.flow == "search-sort" && o.successful)
        .filter(|o| {
            o.delivered_at.is_some_and(|delivered| {
                workers.iter().any(|(id, w)| {
                    *id == o.request_id
                        && completed_search_sort(
                            w,
                            start,
                            o.dispatched_at,
                            delivered,
                            start + cutoff,
                            o.count,
                        )
                })
            })
        })
        .count();
    let eligible = !condition
        || match profile {
            Profile::StableSelective | Profile::StableDense => query_executed(profile.query(true)),
            Profile::StableEdit => {
                full_evaluations.len() >= 2 && query_executed("item") && query_executed("needle")
            }
            Profile::SearchIgnore => !genuine.is_empty(),
            Profile::EditFiles => genuine.len() >= 2,
            Profile::NameAll | Profile::ModifiedAll => sort_rx_completed > 0,
            Profile::ModifiedShown => aux_count("sort") > 0,
            Profile::Preview => aux_count("preview") > 0,
            Profile::Warm | Profile::WarmReclaim => concurrent_index_producers,
            Profile::Promotion => {
                concurrent_index_producers && events.len() == 2 && index_input_progress
            }
            Profile::TabChain => events.len() == 3 && index_input_progress && ledger.len() >= 4,
            Profile::NameShown => gui_sort_completions
                .iter()
                .any(|at| *at >= start && *at <= start + cutoff),
            _ => true,
        };
    if full {
        assert!(
            eligible,
            "actual operation must overlap GUI indexing {} {source:?}; workers={workers:?};aux={aux:?};cutoff={cutoff:?}",
            profile.name()
        );
    }
    let p = ledger.iter().find(|r| r.id == primary).unwrap();
    let observation = p.observation();
    assert_eq!(observation.terminal_kind, Some("finished"));
    assert_eq!(
        observation.started_source,
        Some(source.name()),
        "own successful request source"
    );
    assert_eq!(
        observation.terminal_source,
        Some(source.name()),
        "own finished source"
    );
    if matches!(profile, Profile::NestedEarly | Profile::NestedLate) {
        assert_eq!(
            observation.replacements, 1,
            "nested override must publish real ReplaceAll"
        );
        assert_eq!(
            observation.nested_input_reused,
            Some(profile == Profile::NestedEarly),
            "actual nested reuse/reread branch"
        );
    }
    let records=ledger.iter().map(|r|{let o=r.observation();let planned_revocation=plans.iter().any(|p|p.id==r.id&&p.revoked_by.is_some());let retained_victim=final_proofs.iter().any(|p|p.id==r.id&&p.last_good_victim);
        let removal=driver.app.shell.indexing.perf_warm_removals.iter().find(|m|m.removed_request_id==r.id);
        let direct=driver.app.shell.indexing.perf_preemptions.iter().find(|e|e.victim_id==r.id && e.prior_latest==Some(r.id));
        assert!(request_complete(&driver,r,&plans));
        assert!(planned_requests_valid(&plans, &proofs(&driver,&ledger,&plans,&roots,&tabs,source)), "unexpected index request graph/outcome");
        let abort=o.stale_full_data_abort.as_ref().map(|a|serde_json::json!({"reason":"stale-full-data","request_id":a.request_id,"tab_id":a.tab_id,"response_request_id":a.response_request_id,"data_kind":a.data_kind,"at_ms":ms(a.at.duration_since(start)),"latest_request_id":a.latest_id,"latest_lookup_succeeded":a.latest_lookup_succeeded,"shutdown":a.shutdown}));
        let mut record=serde_json::json!({"request_id":r.id,"tab_id":r.tab,"actual_cause_kind":if removal.is_some(){Some("warm-replacement-removal")}else if direct.is_some(){Some("direct-preempt")}else{None},"invalidating_mutation_ms":removal.map(|m|ms(m.at.duration_since(start))).or_else(||direct.map(|e|ms(e.at.duration_since(start)))),"followup_preempt_ms":driver.app.shell.indexing.perf_preemptions.iter().find(|e|e.victim_id==r.id && e.prior_latest.is_none()).map(|e|ms(e.at.duration_since(start))),"allocated_ms":ms(r.allocated_at.duration_since(start)),"terminal_kind":o.terminal_kind,"allocation_observed":o.allocation_observed,"admitted_ms":o.admitted_at.map(|at|ms(at.duration_since(start))),"admitted_root_matches":o.admitted_root.as_ref()==Some(&roots[tabs.iter().position(|t|*t==r.tab).unwrap()].root),"actual_admitted_root":o.admitted_root,"actual_started_root":o.started_root,"expected_root":roots[tabs.iter().position(|t|*t==r.tab).unwrap()].root,"latest_generation_required":requires_latest_generation(&plans,r.id,r.tab)&&!retained_victim,"latest_measured_request":requires_latest_generation(&plans,r.id,r.tab),"terminal_role":if retained_victim{retained_victim_role(&o)}else if planned_revocation{"revoked-predecessor"}else if requires_latest_generation(&plans,r.id,r.tab){"required-latest-success"}else{"completed-predecessor"},"emitted_workload_semantics":if o.terminal_kind==Some("canceled"){"actual partial canceled work; not full generation throughput"}else if o.terminal_kind==Some("failed"){"actual partial failed work; not full generation throughput"}else{"actual producer emitted entries"},"skipped_closed_before_start":o.skipped_closed_before_start,"unsent_planned_discard":planned_unsent_discard(&o,plans.iter().find(|p|p.id==r.id).and_then(|p|p.permission_at),driver.app.shell.indexing.perf_released_requests.contains_key(&r.id)),"mailbox_closed_ms":o.mailbox_closed_at.map(|at|ms(at.duration_since(start))),"planned_revocation":planned_revocation,"completed_predecessor":plans.iter().find(|p|p.id==r.id).unwrap().completed_predecessor,"completed_predecessor_observed_ms":plans.iter().find(|p|p.id==r.id).unwrap().completed_predecessor_observed_at.map(|at|ms(at.duration_since(start))),"permission_ms":plans.iter().find(|p|p.id==r.id).unwrap().permission_at.map(|at|ms(at.duration_since(start))),"permission_reason":plans.iter().find(|p|p.id==r.id).unwrap().permission_reason,"planned_revoked_by":plans.iter().find(|p|p.id==r.id).unwrap().revoked_by,"mailbox_closed":o.mailbox_closed,"actual_terminal_offer_kind":o.terminal_offer_kind,"terminal_offer_error":o.terminal_offer_error,"terminal_offer_current":o.terminal_offer_current,"terminal_offered_ms":o.terminal_offered.map(|at|ms(at.duration_since(start))),"request_processing_returned_ms":o.request_processing_returned.map(|at|ms(at.duration_since(start))),"started_source":o.started_source,"terminal_source":o.terminal_source,"started_ms":o.started_published.map(|x|ms(x.duration_since(start))),"entries_emitted":o.entries_emitted,"batches":o.batches,"replacements":o.replacements,"data_publish_end_ms":o.data_publish_end.map(|x|ms(x.duration_since(start))),"terminal_publish_ms":o.terminal_published.map(|x|ms(x.duration_since(start))),"bookkeeping_released_ms":ms(driver.app.shell.indexing.perf_released_requests[&r.id].duration_since(start))});
        record.as_object_mut().unwrap().extend(serde_json::json!({"stale_full_data_abort":abort,"stale_full_data_abort_limit":1,"stale_full_data_abort_duplicate":o.stale_full_data_abort_duplicate}).as_object().unwrap().clone());
        record}).collect::<Vec<_>>();
    let binding_rows=bindings.iter().map(|b|serde_json::json!({"request_id":b.request_id,"tab_id":b.tab_id,"root_is_stable_A":b.root==fixture.root,"query":b.query,"candidate_count":b.candidates,"sort_mode":format!("{:?}",b.sort_mode),"sort_scope":format!("{:?}",b.sort_scope),"epoch":b.epoch,"candidate_is_initial_A":Some(b.candidate_ptr)==stable_ptr,"dispatched_ms":ms(b.at.duration_since(start)),"candidate_signature":if Some(b.candidate_ptr)==stable_ptr{Some(quiet_oracles[0].signature())}else{None}})).collect::<Vec<_>>();
    let aux_rows=driver.app.shell.indexing.perf_aux.iter().map(|o|serde_json::json!({"flow":o.flow,"request_id":o.request_id,"tab_id":o.tab_id,"epoch":o.epoch,"owned_tab_root":tabs.iter().position(|t|*t==o.tab_id).map(|n|roots[n].signature()),"path":o.path.as_ref().map(|p|p.strip_prefix(&roots[0].root).unwrap_or(p).to_string_lossy()),"count":o.count,"enqueue_or_dispatch_ms":ms(o.dispatched_at.duration_since(start)),"delivered_ms":o.delivered_at.map(|at|ms(at.duration_since(start))),"successful":o.successful,"route_outcome":o.route,"received_kind_epoch":o.received_kind_epoch,"strict_current_kind_epoch_matches":o.received_kind_epoch.map(|e|e==o.epoch)})).collect::<Vec<_>>();
    let worker_rows=workers.iter().map(|(id,w)|serde_json::json!({"request_id":id,"candidates":w.candidates,"evaluated_candidates":w.evaluated_candidates,"started_ms":w.started_at.map(|x|ms(x.duration_since(start))),"evaluation_completed_ms":w.evaluation_completed_at.map(|x|ms(x.duration_since(start))),"completed_ms":w.completed_at.map(|x|ms(x.duration_since(start))),"canceled_ms":w.canceled_at.map(|x|ms(x.duration_since(start))),"skipped_canceled":w.skipped_canceled})).collect::<Vec<_>>();
    let source_matches = |s: &IndexSource| {
        matches!(
            (source, s),
            (Source::FileList, IndexSource::FileList(_)) | (Source::Walker, IndexSource::Walker)
        )
    };
    assert!(source_matches(
        &driver.app.shell.indexing.build.index.source
    ));
    assert!(driver.app.shell.runtime.query_state.search_error.is_none());
    let snapshot = if profile.stable() {
        &driver
            .app
            .shell
            .tabs
            .get(1)
            .unwrap()
            .result_state
            .committed
            .all_entries
    } else {
        &driver.app.shell.runtime.all_entries
    };
    let declared_edges=plans.iter().filter_map(|p|p.victim.as_ref().map(|c|serde_json::json!({"victim_request_id":p.id,"victim_tab":p.tab,"stage":c.stage,"old_warm_tab":c.old_warm_tab,"trace_tabs":c.trace_tabs,"expected_active_tab":c.active_tab,"expected_current_warm_tab":c.current_warm_tab,"declared_ms":ms(c.declared_at.duration_since(start)),"closed_ms":c.closed_at.map(|at|ms(at.duration_since(start))),"incoming_request_ids":c.incoming_ids,"permission_ms":p.permission_at.map(|at|ms(at.duration_since(start))),"permission_reason":p.permission_reason,"seed_request_id":c.seed.id,"expected_active_root":c.active_root,"actual_switch_ack":c.switch_ack.as_ref().map(|a|serde_json::json!({"at_ms":ms(a.at.duration_since(start)),"tab_id":a.tab,"root":a.root,"pending_activation_tab_id":a.pending_activation}))}))).collect::<Vec<_>>();
    let preemption_rows=driver.app.shell.indexing.perf_preemptions.iter().map(|e|serde_json::json!({"mutation_ms":ms(e.at.duration_since(start)),"warm_removal_mutation_ms":driver.app.shell.indexing.perf_warm_removals.iter().find(|m|m.removed_request_id==e.victim_id).map(|m|ms(m.at.duration_since(start))),"victim_request_id":e.victim_id,"victim_tab":e.victim_tab,"prior_latest_request_id":e.prior_latest,"replacement_request_id":e.replacement_id,"actual_active_tab":e.active_tab,"actual_warm_tab":e.warm_tab,"pending_active_request_id":e.pending_active_id,"latest_active_request_id":e.latest_active_id,"queued_active_request_ids":e.queued_active_ids,"actual_inflight_count":e.inflight_count})).collect::<Vec<_>>();
    let warm_removal_rows=driver.app.shell.indexing.perf_warm_removals.iter().map(|m|serde_json::json!({"mutation_ms":ms(m.at.duration_since(start)),"removed_request_id":m.removed_request_id,"previous_warm_tab":m.previous_warm_tab,"replacement_warm_tab":m.replacement_warm_tab,"route_tab":m.route_tab})).collect::<Vec<_>>();
    let mut row = serde_json::json!({"schema_version":1,"profile_family":"extension","source":source.name(),"comparison":profile.name(),"case":if condition{profile.name()}else{"B0"},"sample_entries":fixture.records.len(),"fixture_shape":format!("{:?}",fixture.shape),"fixture_signature":fixture.signature(),"request_id":primary,"index_ready_ms":ms(t2),"results_ready_ms":ms(t3),"data_publish_end_ms":ms(observation.data_publish_end.unwrap().duration_since(start)),"terminal_publish_ms":ms(observation.terminal_published.unwrap().duration_since(start)),"bookkeeping_released_ms":ms(driver.app.shell.indexing.perf_released_requests[&primary].duration_since(start)),"last_confirmed_snapshot_unsettled_ms":ms(cutoff),"max_no_work_progress_ms":ms(max_gap),"max_ingest_gap_ms":ms(max_ingest_gap),"max_frame_ms":ms(frame_max),"frames":frames,"correct":true,"contention_eligible":eligible,"snapshot_signature":actual_signature(&roots[target].root,snapshot),"results_signature":format!("{:016x}",signature(&roots[0].root,driver.app.shell.runtime.results.iter().map(|(p,_)|p))),"full_wait_ms":ms(observation.full_wait),"full_count":observation.full_retries,"blocked_batches":observation.blocked_batches,"batches":observation.batches,"entries_emitted":observation.entries_emitted});
    row["timed_fixture_scan_passes"] = (fixture_scan_passes - fixture_scans_before_t0).into();
    row["fixture_scans_before_t0"] = fixture_scans_before_t0.into();
    row["frame_diagnostics"] = frame_diagnostics.report();
    row.as_object_mut().unwrap().extend(serde_json::json!({"measurement_kind":"headless-GUI-actual-workers","comparison_kind":profile.comparison_kind(),"condition_description":profile.condition_description(condition),"driver_overhead_ms":ms(driver_overhead),"GUI_sort_completed_ms":gui_sort_completions.iter().map(|at|ms(at.duration_since(start))).collect::<Vec<_>>(),"search_dispatch_bindings":binding_rows,"nested_input_reused":observation.nested_input_reused,"initial_state":initial,"input_trace":events,"tab_transition_trace":transition_events,"finished_warm_commit_waits":finished_warm_waits,"tabchain_input_policy":tabchain_input_policy(full),"stable_edit_input_policy":if full{STABLE_EDIT_INPUT_POLICY}else{"sub100k diagnostic: checkpoint-only; not full-pressure evidence"},"stable_edit_input_admissions":stable_input_admissions,"index_requests":records,"unmeasured_tab_observations":unmeasured_tab_observations,"allocated_request_ids":allocated,"planned_request_ids":planned,"released_request_ids":released,"worker_observations":worker_rows,"overlap_executions":genuine.len(),"index_sender_load_at_t2":{"queued":t2_load.as_ref().unwrap().queued,"inflight":t2_load.as_ref().unwrap().inflight,"capacity":t2_load.as_ref().unwrap().capacity},"final_index_sender_load":{"queued":driver.app.shell.indexing.tx.load().queued,"inflight":driver.app.shell.indexing.tx.load().inflight,"capacity":driver.app.shell.indexing.tx.load().capacity},"search_sort_worker_completed_while_index_unsettled":sort_rx_completed,"full_candidate_evaluations":full_evaluations.len(),"actual_index_producer_interval_overlap":concurrent_index_producers,"aux_completed_while_index_unsettled":{"kind":aux_count("kind"),"sort":aux_count("sort"),"preview":aux_count("preview")},"aux_observations":aux_rows,"aux_observer_limit_per_flow":256,"aux_known_flows":["kind","sort","preview","search-sort"],"strict_kind_sort_preview_cpu_duration":"NOT_OBSERVED","aux_proof_counts":"sampled-owned-successful-received; search-sort may route Background/Stale","settings":{"files":filter.files,"folders":filter.dirs,"ignore_enabled":filter.ignore_enabled,"ignore_case":filter.ignore_case,"sort_mode":format!("{mode:?}"),"sort_scope":format!("{scope:?}"),"query":profile.query(condition),"follow_links":profile==Profile::Links},"expected_final_logical_entries":roots[target].expected.iter().filter(|r|if r.is_dir{filter.dirs}else{filter.files}).count(),"expected_query_match_count":expected.expected_count(),"intended_extra_index_requests":if condition&&matches!(profile,Profile::Warm|Profile::Promotion|Profile::WarmReclaim){1}else{0},"native":false}).as_object().unwrap().clone());
    row.as_object_mut().unwrap().extend(serde_json::json!({"declared_retained_victims":victim_rows,"declared_eviction_edges":declared_edges,"actual_preemption_events":preemption_rows,"actual_warm_removal_events":warm_removal_rows,"warm_removal_observer_limit":128,"warm_removal_observer_overflow":driver.app.shell.indexing.perf_warm_removal_overflow,"active_at_removal":"NOT_DIRECTLY_OBSERVED; fixed switch source ordering and actual switch acknowledgement binding only","preemption_observer_limit":128,"preemption_observer_overflow":driver.app.shell.indexing.perf_preemption_overflow,"retained_seed_validation_policy":"full independent oracle after tentative t3; no extra frame/input before output"}).as_object().unwrap().clone());
    drop(driver);
    drop(roots);
    drop(generations);
    row
}

#[test]
fn tc_229_observation_ledger_does_not_pin_closed_heavy_mailbox() {
    if crate::app::tests::indexing_perf::harness::child_process::isolate(
        module_path!(),
        "tc_229_observation_ledger_does_not_pin_closed_heavy_mailbox",
    ) {
        return;
    }
    let mut driver = Driver::new();
    driver.settle_startup();
    driver.app.shell.indexing.perf_observe_requests = true;
    driver.app.shell.indexing.perf_observe_history = true;
    let tab = driver.app.current_tab_id();
    let id = driver.app.shell.indexing.allocate_request_id(tab);
    let mailbox = driver.mailbox(id);
    mailbox
        .try_publish(crate::app::worker::protocol::IndexResponse::Batch {
            request_id: id,
            entries: vec![
                crate::app::worker::protocol::IndexEntry {
                    path: driver.app.shell.runtime.root.join("owned-heavy.txt"),
                    kind: crate::entry::EntryKind::file(),
                    kind_known: true
                };
                1024
            ],
        })
        .unwrap();
    let mut ledger = Vec::new();
    collect_requests(&driver, &mut ledger);
    let weak = Arc::downgrade(&mailbox);
    driver.app.shell.indexing.cleanup_request(id);
    drop(mailbox);
    assert!(
        weak.upgrade().is_none(),
        "observations must not retain unconsumed Batch payload through closed mailbox"
    );
    assert_eq!(ledger.len(), 1);
}

#[test]
fn tc_229_preemption_observer_is_opt_in_bounded_and_scalar() {
    if crate::app::tests::indexing_perf::harness::child_process::isolate(
        module_path!(),
        "tc_229_preemption_observer_is_opt_in_bounded_and_scalar",
    ) {
        return;
    }
    let mut driver = Driver::new();
    driver.settle_startup();
    driver.app.create_new_tab();
    driver.app.create_new_tab();
    settle_setup(&mut driver);
    assert_stale_full_abort_composition(&mut driver);
    assert_warm_removal_composition(&mut driver);
    assert_observed_finished_primary_selection(&mut driver);
    let active = driver.app.current_tab_id().unwrap();
    let victim = driver.app.shell.tabs.get(0).unwrap().id;
    let warm = driver.app.shell.tabs.get(1).unwrap().id;
    let root = driver.app.shell.runtime.root.clone();
    let (tx, _rx) = crate::app::worker::channel::bounded_request_channel::<
        crate::app::worker::protocol::IndexRequest,
    >(2);
    let i = &mut driver.app.shell.indexing;
    i.tx = tx;
    i.inflight_requests.clear();
    i.request_tabs.clear();
    i.pending_queue.clear();
    i.inflight_requests.extend([100, 101]);
    i.request_tabs.extend([(100, victim), (101, warm)]);
    i.warm_tab_id = Some(warm);
    i.pending_request_id = Some(102);
    i.pending_queue
        .push_back(crate::app::worker::protocol::IndexRequest {
            request_id: 102,
            tab_id: active,
            root: root.clone(),
            use_filelist: false,
            include_files: true,
            include_dirs: true,
            max_depth: crate::indexer::MaxDepth::unlimited(),
            follow_links: false,
            complete_walker_snapshot: false,
        });
    i.latest_request_ids
        .lock()
        .unwrap()
        .extend([(victim, 100), (warm, 101), (active, 102)]);
    assert!(!i.perf_observe_history);
    assert!(driver.app.preempt_background_for_active_request());
    assert!(driver.app.shell.indexing.perf_preemptions.is_empty());
    let i = &mut driver.app.shell.indexing;
    i.perf_observe_history = true;
    i.latest_request_ids.lock().unwrap().insert(victim, 100);
    let before = Instant::now();
    assert!(driver.app.preempt_background_for_active_request());
    let after = Instant::now();
    let i = &driver.app.shell.indexing;
    assert_eq!(i.perf_preemptions.len(), 1);
    let e = &i.perf_preemptions[0];
    assert!(before <= e.at && e.at <= after);
    assert_eq!(
        (e.victim_id, e.victim_tab, e.prior_latest, e.replacement_id),
        (100, victim, Some(100), 0)
    );
    assert_eq!(
        (
            e.active_tab,
            e.warm_tab,
            e.pending_active_id,
            e.latest_active_id
        ),
        (active, Some(warm), Some(102), Some(102))
    );
    assert_eq!(e.queued_active_ids, vec![102]);
    assert_eq!(e.inflight_count, 2);
    for _ in 1..128 {
        driver
            .app
            .shell
            .indexing
            .latest_request_ids
            .lock()
            .unwrap()
            .insert(victim, 100);
        assert!(driver.app.preempt_background_for_active_request());
    }
    driver
        .app
        .shell
        .indexing
        .latest_request_ids
        .lock()
        .unwrap()
        .insert(victim, 100);
    let overflow = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        driver.app.preempt_background_for_active_request()
    }));
    assert!(overflow.is_err());
    assert!(driver.app.shell.indexing.perf_preemption_overflow);
    assert_eq!(driver.app.shell.indexing.perf_preemptions.len(), 128);
    assert_eq!(
        driver
            .app
            .shell
            .indexing
            .latest_request_ids
            .lock()
            .unwrap()
            .get(&victim),
        Some(&100),
        "observer failure does not poison or mutate latest mutex"
    );
    // The invalid observation remains sticky while testing the independently bounded inner queue.
    driver.app.shell.indexing.perf_preemptions.clear();
    let prototype = driver.app.shell.indexing.pending_queue.front().unwrap();
    let root = prototype.root.clone();
    for request_id in 103..=230 {
        driver.app.shell.indexing.pending_queue.push_back(
            crate::app::worker::protocol::IndexRequest {
                request_id,
                tab_id: active,
                root: root.clone(),
                use_filelist: false,
                include_files: true,
                include_dirs: true,
                max_depth: crate::indexer::MaxDepth::unlimited(),
                follow_links: false,
                complete_walker_snapshot: false,
            },
        );
    }
    let inner_overflow = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        driver.app.preempt_background_for_active_request()
    }));
    assert!(inner_overflow.is_err());
    assert!(driver.app.shell.indexing.perf_preemption_overflow);
    assert!(driver.app.shell.indexing.perf_preemptions.is_empty());
    assert_eq!(
        driver
            .app
            .shell
            .indexing
            .latest_request_ids
            .lock()
            .unwrap()
            .get(&victim),
        Some(&100)
    );
}

// Exercise the actual driver derivation and selector with scalar phase inputs.
// The enclosing owned leaf also proves the real scheduler observation hook.
fn assert_observed_finished_primary_selection(driver: &mut Driver) {
    let tabs = driver
        .app
        .shell
        .tabs
        .iter()
        .map(|t| t.id)
        .collect::<Vec<_>>();
    let (mut p, mut e, mut o, _) = victim_test_inputs();
    p.id = 5;
    p.tab = tabs[0];
    p.permission_at = None;
    p.permission_reason = None;
    p.completed_predecessor = true;
    p.completed_predecessor_observed_at = Some(e.at - Duration::from_micros(500));
    let c = p.victim.as_mut().unwrap();
    c.stage = 1;
    c.trace_tabs = [tabs[0], tabs[1], tabs[2]];
    c.old_warm_tab = Some(tabs[0]);
    c.victim_id = 5;
    c.victim_tab = tabs[0];
    c.active_tab = tabs[2];
    c.current_warm_tab = tabs[1];
    c.incoming_ids = vec![7];
    e.victim_id = 5;
    e.victim_tab = tabs[0];
    e.prior_latest = Some(5);
    e.active_tab = tabs[2];
    e.warm_tab = Some(tabs[1]);
    e.pending_active_id = Some(7);
    e.latest_active_id = Some(7);
    e.queued_active_ids = vec![7];
    o.terminal_kind = Some("finished");
    o.terminal_offer_kind = Some("finished");
    o.terminal_source = Some("FileList");
    o.terminal_offered = Some(e.at - Duration::from_millis(1));
    o.terminal_published = o.terminal_offered;
    o.request_processing_returned = None;
    let handle = Arc::new(std::sync::Mutex::new(o.clone()));
    let mut ledger = vec![OwnedRequest {
        id: 5,
        tab: tabs[0],
        allocated_at: o.admitted_at.unwrap(),
        observation: Arc::clone(&handle),
    }];
    driver.app.shell.indexing.perf_allocations.push(
        crate::app::index_coordinator::IndexPerfAllocation {
            id: 5,
            tab: Some(tabs[0]),
            at: o.admitted_at.unwrap(),
            observation: Arc::clone(&handle),
        },
    );
    driver
        .app
        .shell
        .indexing
        .latest_request_ids
        .lock()
        .unwrap()
        .insert(tabs[0], 0);
    driver.app.shell.indexing.perf_preemptions = vec![e.clone()];
    let mut plans = vec![
        p.clone(),
        PlannedRequest {
            id: 7,
            tab: tabs[2],
            ..Default::default()
        },
    ];
    let incoming_observation = Arc::new(std::sync::Mutex::new(Default::default()));
    ledger.push(OwnedRequest {
        id: 7,
        tab: tabs[2],
        allocated_at: Instant::now(),
        observation: Arc::clone(&incoming_observation),
    });
    driver.app.shell.indexing.perf_allocations.push(
        crate::app::index_coordinator::IndexPerfAllocation {
            id: 7,
            tab: Some(tabs[2]),
            at: Instant::now(),
            observation: incoming_observation,
        },
    );
    for early in [false, true] {
        let mut actual = o.clone();
        if early {
            actual.terminal_offered =
                Some(p.victim.as_ref().unwrap().declared_at - Duration::from_micros(500));
            actual.terminal_published = actual.terminal_offered;
        }
        *handle.lock().unwrap() = actual.clone();
        assert_eq!(
            measured_request_for(driver, &plans, &ledger, tabs[0]),
            5,
            "stage1 latest A5 marker0 with actual observed Finished needs no Cancel permission"
        );
        let mut truth = ExtendedTruth {
            successful_terminal: true,
            owned_physical_complete: physical_request_complete(&actual),
            owned_request_released: true,
            snapshot_valid: true,
            debt: false,
            latest_results_valid: true,
        };
        assert!(!truth.index_ready() && !truth.results_ready());
        actual.request_processing_returned = Some(e.at + Duration::from_millis(1));
        truth.owned_physical_complete = physical_request_complete(&actual);
        assert!(truth.index_ready() && truth.results_ready());
    }
    for mutation in 0..19 {
        *handle.lock().unwrap() = o.clone();
        plans[0] = p.clone();
        ledger[0].id = 5;
        ledger[0].tab = tabs[0];
        ledger[1].id = 7;
        driver.app.shell.indexing.perf_allocations[0].tab = Some(tabs[0]);
        driver.app.shell.indexing.perf_preemptions = vec![e.clone()];
        match mutation {
            0 => driver.app.shell.indexing.perf_preemptions.clear(),
            1 => plans[0].completed_predecessor_observed_at = None,
            2 => handle.lock().unwrap().terminal_published = None,
            3 => handle.lock().unwrap().started_root = Some("wrong-root".into()),
            4 => plans[0].victim.as_mut().unwrap().stage = 2,
            5 => driver.app.shell.indexing.perf_preemptions[0].pending_active_id = Some(8),
            6 => driver.app.shell.indexing.perf_preemptions.push(e.clone()),
            7 => driver.app.shell.indexing.perf_preemption_overflow = true,
            8 => handle.lock().unwrap().terminal_kind = Some("canceled"),
            9 => plans[0].completed_predecessor_observed_at = Some(e.at + Duration::from_micros(1)),
            10 => handle.lock().unwrap().terminal_published = Some(e.at),
            11 => handle.lock().unwrap().terminal_source = Some("Walker"),
            12 => plans[0].completed_predecessor = false,
            13 => handle.lock().unwrap().terminal_kind = Some("failed"),
            14 => driver.app.shell.indexing.perf_allocations[0].tab = None,
            15 => ledger[0].id = 99,
            16 => ledger[0].tab = tabs[1],
            17 => ledger[1].id = 99,
            18 => plans[0].victim = None,
            _ => unreachable!(),
        }
        let rejected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            measured_request_for(driver, &plans, &ledger, tabs[0])
        }));
        assert!(
            rejected.is_err(),
            "unproven observed Finished selection mutation {mutation}"
        );
        driver.app.shell.indexing.perf_preemption_overflow = false;
    }
    plans[0] = p;
    ledger[0].id = 5;
    ledger[0].tab = tabs[0];
    ledger[1].id = 7;
    driver.app.shell.indexing.perf_allocations[0].tab = Some(tabs[0]);
    *handle.lock().unwrap() = o;
    driver.app.shell.indexing.perf_preemptions = vec![e];
    let successor = Arc::new(std::sync::Mutex::new(Default::default()));
    ledger.push(OwnedRequest {
        id: 8,
        tab: tabs[0],
        allocated_at: Instant::now(),
        observation: Arc::clone(&successor),
    });
    driver.app.shell.indexing.perf_allocations.push(
        crate::app::index_coordinator::IndexPerfAllocation {
            id: 8,
            tab: Some(tabs[0]),
            at: Instant::now(),
            observation: successor,
        },
    );
    plans.push(PlannedRequest {
        id: 8,
        tab: tabs[0],
        ..Default::default()
    });
    let rejected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        measured_request_for(driver, &plans, &ledger, tabs[0])
    }));
    assert!(
        rejected.is_err(),
        "new latest A8 cannot reuse old A5 cause for marker0"
    );
    driver
        .app
        .shell
        .indexing
        .latest_request_ids
        .lock()
        .unwrap()
        .insert(tabs[0], 8);
    assert_eq!(measured_request_for(driver, &plans, &ledger, tabs[0]), 8);
    driver.app.shell.indexing.perf_preemptions.clear();
    driver.app.shell.indexing.perf_allocations.clear();
}

// Real coordinator mutation and actual driver derivation, with explicit scalar
// worker phases. These are correctness controls, not measured worker timings.
fn assert_warm_removal_composition(driver: &mut Driver) {
    let tabs = driver
        .app
        .shell
        .tabs
        .iter()
        .map(|t| t.id)
        .collect::<Vec<_>>();
    let root = driver.app.shell.runtime.root.clone();
    let mut no_victim = Vec::new();
    assert!(
        acknowledge_tabchain_switch(driver, &mut no_victim, 1, tabs[2], &root),
        "legitimate noWarm switch creates no cause"
    );
    assert!(no_victim.is_empty());
    assert!(!acknowledge_tabchain_switch(
        driver,
        &mut no_victim,
        3,
        tabs[2],
        &root
    ));
    assert!(!acknowledge_tabchain_switch(
        driver,
        &mut no_victim,
        1,
        tabs[0],
        &root
    ));
    assert!(!acknowledge_tabchain_switch(
        driver,
        &mut no_victim,
        1,
        tabs[2],
        std::path::Path::new("foreign")
    ));
    driver.app.shell.tabs.pending_activation_tab_id = Some(tabs[2]);
    assert!(!acknowledge_tabchain_switch(
        driver,
        &mut no_victim,
        1,
        tabs[2],
        &root
    ));
    driver.app.shell.tabs.pending_activation_tab_id = None;
    assert!(driver.app.shell.indexing.perf_warm_removals.is_empty());
    let now = Instant::now();
    let (mut p, _, mut observation, _) = victim_test_inputs();
    p.id = 5;
    p.tab = tabs[0];
    p.permission_at = Some(now);
    p.permission_reason = Some("switch-evicts-previous-Warm");
    let c = p.victim.as_mut().unwrap();
    c.stage = 1;
    c.trace_tabs = [tabs[0], tabs[1], tabs[2]];
    c.old_warm_tab = Some(tabs[0]);
    c.victim_id = 5;
    c.victim_tab = tabs[0];
    c.active_tab = tabs[2];
    c.current_warm_tab = tabs[1];
    c.declared_at = now - Duration::from_millis(1);
    c.closed_at = None;
    c.active_root = root.clone();
    c.seed.root = root.clone();
    c.incoming_ids = vec![7];
    observation.admitted_root = Some(root.clone());
    observation.started_root = Some(root.clone());
    observation.admitted_at = Some(now - Duration::from_millis(3));
    observation.started_published = Some(now - Duration::from_millis(2));
    observation.request_processing_returned = None;
    let handle = Arc::new(std::sync::Mutex::new(observation.clone()));
    let incoming = Arc::new(std::sync::Mutex::new(Default::default()));
    let mut ledger = vec![
        OwnedRequest {
            id: 5,
            tab: tabs[0],
            allocated_at: now - Duration::from_millis(4),
            observation: Arc::clone(&handle),
        },
        OwnedRequest {
            id: 7,
            tab: tabs[2],
            allocated_at: now,
            observation: Arc::clone(&incoming),
        },
    ];
    let mut plans = vec![
        p.clone(),
        PlannedRequest {
            id: 7,
            tab: tabs[2],
            ..Default::default()
        },
    ];
    let i = &mut driver.app.shell.indexing;
    i.perf_allocations.clear();
    i.perf_preemptions.clear();
    i.perf_warm_removals.clear();
    i.perf_observe_history = false;
    i.warm_tab_id = Some(tabs[0]);
    i.request_tabs.insert(5, tabs[0]);
    i.latest_request_ids.lock().unwrap().insert(tabs[0], 5);
    i.replace_warm_tab(Some(tabs[1]));
    assert!(
        i.perf_warm_removals.is_empty(),
        "disabled removal observer adds no event"
    );
    i.perf_observe_history = true;
    i.warm_tab_id = Some(tabs[0]);
    i.latest_request_ids.lock().unwrap().insert(tabs[0], 5);
    for r in &ledger {
        i.perf_allocations
            .push(crate::app::index_coordinator::IndexPerfAllocation {
                id: r.id,
                tab: Some(r.tab),
                at: r.allocated_at,
                observation: Arc::clone(&r.observation),
            });
    }
    let before = Instant::now();
    i.replace_warm_tab(Some(tabs[1]));
    let after = Instant::now();
    assert_eq!(
        i.perf_warm_removals.len(),
        1,
        "actual routed Warm removal must be observed, not inferred from later marker"
    );
    let removal = i.perf_warm_removals[0].clone();
    assert!(before <= removal.at && removal.at <= after);
    assert_eq!(
        (
            removal.removed_request_id,
            removal.previous_warm_tab,
            removal.replacement_warm_tab,
            removal.route_tab
        ),
        (5, tabs[0], Some(tabs[1]), tabs[0])
    );
    assert_eq!(i.latest_request_for_tab(tabs[0]), None);
    driver.app.shell.tabs.pending_activation_tab_id = Some(tabs[2]);
    assert!(
        !acknowledge_tabchain_switch(driver, &mut plans, 1, tabs[2], &root),
        "pending activation cannot be acknowledged"
    );
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| measured_request_for(
            driver, &plans, &ledger, tabs[0]
        )))
        .is_err()
    );
    // Actual update-cycle acknowledgement must be available before this same
    // post-frame selector. No dependent refresh is performed by this guard.
    driver.frame();
    assert!(driver.app.shell.tabs.pending_activation_tab_id.is_none());
    assert!(acknowledge_tabchain_switch(
        driver, &mut plans, 1, tabs[2], &root
    ));
    assert_eq!(
        measured_request_for(driver, &plans, &ledger, tabs[0]),
        5,
        "real standalone removal, live route/body, no marker"
    );
    assert!(preemption_events_valid(driver, &plans, &ledger));
    let actual_ack = plans[0]
        .victim
        .as_ref()
        .unwrap()
        .switch_ack
        .clone()
        .unwrap();
    assert_eq!(
        (
            actual_ack.tab,
            actual_ack.root.clone(),
            actual_ack.pending_activation
        ),
        (driver.app.current_tab_id().unwrap(), root.clone(), None)
    );
    let mut truth = ExtendedTruth {
        successful_terminal: true,
        owned_physical_complete: physical_request_complete(&observation),
        owned_request_released: true,
        snapshot_valid: true,
        debt: false,
        latest_results_valid: true,
    };
    assert!(!truth.index_ready() && !truth.results_ready());
    observation.request_processing_returned = Some(Instant::now());
    truth.owned_physical_complete = physical_request_complete(&observation);
    assert!(truth.index_ready() && truth.results_ready());
    observation.request_processing_returned = None;
    // Optional real preempt follows the first mutation and records priorNone.
    let i = &mut driver.app.shell.indexing;
    i.inflight_requests.clear();
    i.inflight_requests.extend([5, 6]);
    i.request_tabs.extend([(5, tabs[0]), (6, tabs[1])]);
    i.warm_tab_id = Some(tabs[1]);
    i.pending_request_id = Some(7);
    i.pending_queue
        .push_back(crate::app::worker::protocol::IndexRequest {
            request_id: 7,
            tab_id: tabs[2],
            root: root.clone(),
            use_filelist: false,
            include_files: true,
            include_dirs: true,
            max_depth: crate::indexer::MaxDepth::unlimited(),
            follow_links: false,
            complete_walker_snapshot: false,
        });
    i.latest_request_ids
        .lock()
        .unwrap()
        .extend([(tabs[1], 6), (tabs[2], 7)]);
    assert!(driver.app.preempt_background_for_active_request());
    let event = driver.app.shell.indexing.perf_preemptions[0].clone();
    assert_eq!(event.prior_latest, None);
    for offered in [
        removal.at + (event.at - removal.at) / 2,
        event.at + Duration::from_micros(1),
    ] {
        observation.terminal_offered = Some(offered);
        observation.terminal_published = Some(offered);
        observation.request_processing_returned = Some(offered + Duration::from_micros(1));
        *handle.lock().unwrap() = observation.clone();
        assert!(warm_victim_execution(&plans[0], &removal, &observation));
        assert!(preemption_events_valid(driver, &plans, &ledger));
        assert_eq!(measured_request_for(driver, &plans, &ledger, tabs[0]), 5);
    }
    // Every malformed actual-cause/ack input remains fail closed.
    let good = plans[0].clone();
    let good_o = observation.clone();
    for bad in 0..24 {
        plans[0] = good.clone();
        *handle.lock().unwrap() = good_o.clone();
        driver.app.shell.indexing.perf_warm_removals = vec![removal.clone()];
        driver.app.shell.indexing.perf_preemptions = vec![event.clone()];
        let c = plans[0].victim.as_mut().unwrap();
        match bad {
            0 => driver.app.shell.indexing.perf_warm_removals.clear(),
            1 => driver
                .app
                .shell
                .indexing
                .perf_warm_removals
                .push(removal.clone()),
            2 => driver.app.shell.indexing.perf_warm_removal_overflow = true,
            3 => driver.app.shell.indexing.perf_warm_removals[0].route_tab = tabs[1],
            4 => driver.app.shell.indexing.perf_warm_removals[0].removed_request_id = 4,
            5 => {
                driver.app.shell.indexing.perf_warm_removals[0].replacement_warm_tab = Some(tabs[0])
            }
            6 => c.switch_ack = None,
            7 => c.switch_ack.as_mut().unwrap().tab = tabs[0],
            8 => c.switch_ack.as_mut().unwrap().root = "foreign".into(),
            9 => c.switch_ack.as_mut().unwrap().pending_activation = Some(tabs[2]),
            10 => c.switch_ack.as_mut().unwrap().at = removal.at - Duration::from_micros(1),
            11 => c.closed_at = Some(actual_ack.at - Duration::from_micros(1)),
            12 => plans[0].permission_at = None,
            13 => plans[0].permission_at = Some(removal.at + Duration::from_micros(1)),
            14 => handle.lock().unwrap().started_root = Some("foreign".into()),
            15 => handle.lock().unwrap().started_source = Some("Walker"),
            16 => c.stage = 2,
            17 => c.incoming_ids.clear(),
            18 => driver.app.shell.indexing.perf_preemptions[0].pending_active_id = Some(8),
            19 => driver
                .app
                .shell
                .indexing
                .perf_preemptions
                .push(event.clone()),
            20 => c.declared_at = removal.at + Duration::from_micros(1),
            21 => c.active_root = "foreign".into(),
            22 => driver.app.shell.indexing.perf_warm_removals[0].previous_warm_tab = tabs[1],
            _ => driver.app.shell.indexing.perf_preemptions[0].prior_latest = Some(5),
        }
        assert!(
            !preemption_events_valid(driver, &plans, &ledger),
            "invalid exact removal/ack/followup {bad}"
        );
        driver.app.shell.indexing.perf_warm_removal_overflow = false;
    }
    plans[0] = good;
    *handle.lock().unwrap() = good_o;
    driver.app.shell.indexing.perf_warm_removals = vec![removal.clone()];
    driver.app.shell.indexing.perf_preemptions.clear();
    // Actual observed Finished classification on either side of declaration,
    // followed by a new real removal and actual immediate acknowledgement.
    for before_declaration in [true, false] {
        let declared = Instant::now();
        let c = plans[0].victim.as_mut().unwrap();
        c.declared_at = declared;
        c.switch_ack = None;
        plans[0].permission_at = None;
        plans[0].permission_reason = None;
        plans[0].completed_predecessor = false;
        plans[0].completed_predecessor_observed_at = None;
        let mut finished = observation.clone();
        finished.terminal_kind = Some("finished");
        finished.terminal_offer_kind = Some("finished");
        finished.terminal_source = Some("FileList");
        finished.terminal_published = Some(if before_declaration {
            declared - Duration::from_micros(1)
        } else {
            Instant::now()
        });
        finished.terminal_offered = finished.terminal_published;
        finished.request_processing_returned = None;
        *handle.lock().unwrap() = finished.clone();
        permit_revocation(driver, &mut plans, tabs[0], "switch-evicts-previous-Warm");
        let i = &mut driver.app.shell.indexing;
        i.perf_warm_removals.clear();
        i.warm_tab_id = Some(tabs[0]);
        i.latest_request_ids.lock().unwrap().insert(tabs[0], 5);
        i.replace_warm_tab(Some(tabs[1]));
        assert!(acknowledge_tabchain_switch(
            driver, &mut plans, 1, tabs[2], &root
        ));
        assert!(plans[0].permission_at.is_none());
        let first_observed = plans[0].completed_predecessor_observed_at;
        permit_revocation(driver, &mut plans, tabs[0], "explicit-TabChain-refresh");
        assert_eq!(
            plans[0].completed_predecessor_observed_at, first_observed,
            "later Finished observation must not erase earlier actual first-cause evidence"
        );
        assert_eq!(measured_request_for(driver, &plans, &ledger, tabs[0]), 5);
        assert!(!physical_request_complete(&finished));
    }
    let latest = Arc::new(std::sync::Mutex::new(Default::default()));
    ledger.push(OwnedRequest {
        id: 8,
        tab: tabs[0],
        allocated_at: Instant::now(),
        observation: Arc::clone(&latest),
    });
    driver.app.shell.indexing.perf_allocations.push(
        crate::app::index_coordinator::IndexPerfAllocation {
            id: 8,
            tab: Some(tabs[0]),
            at: Instant::now(),
            observation: latest,
        },
    );
    plans.push(PlannedRequest {
        id: 8,
        tab: tabs[0],
        ..Default::default()
    });
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| measured_request_for(
            driver, &plans, &ledger, tabs[0]
        )))
        .is_err(),
        "new max8 cannot inherit removal5 proof"
    );
    driver
        .app
        .shell
        .indexing
        .latest_request_ids
        .lock()
        .unwrap()
        .insert(tabs[0], 8);
    assert_eq!(measured_request_for(driver, &plans, &ledger, tabs[0]), 8);
    // Unrouted setup/missing/zero IDs never create an eviction observation.
    let i = &mut driver.app.shell.indexing;
    i.perf_warm_removals.clear();
    i.request_tabs.clear();
    for id in [0, 9] {
        i.warm_tab_id = Some(tabs[0]);
        i.latest_request_ids.lock().unwrap().insert(tabs[0], id);
        i.replace_warm_tab(Some(tabs[1]));
    }
    assert!(i.perf_warm_removals.is_empty());
    i.request_tabs.insert(5, tabs[0]);
    for _ in 0..128 {
        i.warm_tab_id = Some(tabs[0]);
        i.latest_request_ids.lock().unwrap().insert(tabs[0], 5);
        i.replace_warm_tab(Some(tabs[1]));
    }
    assert_eq!(i.perf_warm_removals.len(), 128);
    i.warm_tab_id = Some(tabs[0]);
    i.latest_request_ids.lock().unwrap().insert(tabs[0], 5);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| driver
            .app
            .shell
            .indexing
            .replace_warm_tab(Some(tabs[1]))))
        .is_err()
    );
    assert!(driver.app.shell.indexing.perf_warm_removal_overflow);
    assert!(
        driver.app.shell.indexing.latest_request_ids.lock().is_ok(),
        "overflow fails after releasing latest mutex"
    );
    let i = &mut driver.app.shell.indexing;
    i.perf_observe_history = false;
    i.perf_allocations.clear();
    i.perf_warm_removals.clear();
    i.perf_warm_removal_overflow = false;
    i.perf_preemptions.clear();
    i.request_tabs.clear();
    i.inflight_requests.clear();
    i.pending_queue.clear();
    i.pending_request_id = None;
}

fn assert_stale_full_abort_composition(driver: &mut Driver) {
    use crate::app::index_mailbox::IndexResponseMailbox;
    use crate::app::worker::protocol::{IndexEntry, IndexRequest, IndexResponse};
    use std::sync::atomic::AtomicBool;
    let active = driver.app.current_tab_id().unwrap();
    let victim = driver
        .app
        .shell
        .tabs
        .iter()
        .find(|t| t.id != active)
        .unwrap()
        .id;
    let warm = driver
        .app
        .shell
        .tabs
        .iter()
        .find(|t| t.id != active && t.id != victim)
        .unwrap()
        .id;
    let root = driver.app.shell.runtime.root.clone();
    let req = IndexRequest {
        request_id: 6,
        tab_id: victim,
        root: root.clone(),
        use_filelist: false,
        include_files: true,
        include_dirs: true,
        max_depth: crate::indexer::MaxDepth::unlimited(),
        follow_links: false,
        complete_walker_snapshot: false,
    };
    let batch = || IndexResponse::Batch {
        request_id: 6,
        entries: vec![IndexEntry {
            path: root.join("controlled.txt"),
            kind: crate::entry::EntryKind::file(),
            kind_known: true,
        }],
    };
    let mailbox = Arc::new(IndexResponseMailbox::with_data_capacity(1));
    mailbox.enable_perf_observation();
    let shutdown = Arc::new(AtomicBool::new(false));
    let latest = Arc::clone(&driver.app.shell.indexing.latest_request_ids);
    latest.lock().unwrap().insert(victim, 6);
    let send = crate::app::index_worker::mailbox_response_sink_for_test(
        &req,
        Arc::clone(&mailbox),
        Arc::clone(&latest),
        shutdown,
    );
    let admitted = Instant::now();
    send(IndexResponse::Started {
        request_id: 6,
        source: crate::indexer::IndexSource::Walker,
    })
    .unwrap();
    send(batch()).unwrap();
    assert!(matches!(
        mailbox.try_publish(batch()),
        Err(crate::app::index_mailbox::IndexMailboxPublishError::Full(_))
    ));
    let i = &mut driver.app.shell.indexing;
    i.perf_observe_history = true;
    i.perf_warm_removals.clear();
    i.perf_warm_removal_overflow = false;
    i.warm_tab_id = Some(victim);
    i.request_tabs.insert(6, victim);
    let declared = Instant::now();
    i.replace_warm_tab(Some(warm));
    assert_eq!(i.perf_warm_removals.len(), 1);
    assert!(send(batch()).is_err());
    let o = mailbox.perf_observation();
    assert!(o.stale_full_data_abort.is_some(),"real Full stale DATA abort must be observed at the actual rejection, not inferred from Failed");
    let removal = driver.app.shell.indexing.perf_warm_removals[0].clone();
    let (mut p, _, _, state) = victim_test_inputs();
    p.tab = victim;
    p.permission_at = Some(declared);
    let c = p.victim.as_mut().unwrap();
    c.trace_tabs = [active, victim, warm];
    c.old_warm_tab = Some(victim);
    c.victim_tab = victim;
    c.active_tab = active;
    c.current_warm_tab = warm;
    c.declared_at = declared;
    c.active_root = root.clone();
    c.seed.root = root.clone();
    c.seed.source = Source::Walker;
    let mut plans = vec![p];
    assert!(acknowledge_tabchain_switch(
        driver, &mut plans, 2, active, &root
    ));
    // Failed offer/publication are real sink events. This control sends the
    // terminal deliberately; it does not claim real worker classification.
    send(IndexResponse::Failed {
        request_id: 6,
        error: "index receiver closed".into(),
    })
    .unwrap();
    let mut o = mailbox.perf_observation();
    o.admitted_at = Some(admitted);
    o.admitted_root = Some(root.clone());
    assert!(
        !warm_last_good_victim_fast(&plans[0], &removal, &o, &state),
        "real abort without body cannot settle"
    );
    // Body return and seed fast-state are explicit predicate models. No actual
    // worker-thread join/body timing is claimed by this controlled sink test.
    o.request_processing_returned = Some(Instant::now());
    assert!(warm_last_good_victim_fast(&plans[0],&removal,&o,&state),"exact real stale Full witness + Failed terminal should qualify the declared retained victim");
    let first = o.stale_full_data_abort.as_ref().unwrap().at;
    assert!(send(batch()).is_err());
    let repeated = mailbox.perf_observation();
    assert!(
        !repeated.stale_full_data_abort_duplicate,
        "terminal-published lane rejects Closed, not Full"
    );
    assert_eq!(repeated.stale_full_data_abort.as_ref().unwrap().at, first);
    let duplicate_box = Arc::new(IndexResponseMailbox::with_data_capacity(1));
    duplicate_box.enable_perf_observation();
    duplicate_box.try_publish(batch()).unwrap();
    let duplicate_send = crate::app::index_worker::mailbox_response_sink_for_test(
        &req,
        Arc::clone(&duplicate_box),
        Arc::clone(&latest),
        Arc::new(AtomicBool::new(false)),
    );
    assert!(duplicate_send(batch()).is_err());
    let duplicate_first = duplicate_box
        .perf_observation()
        .stale_full_data_abort
        .unwrap()
        .at;
    assert!(duplicate_send(batch()).is_err());
    let duplicate_observed = duplicate_box.perf_observation();
    assert!(duplicate_observed.stale_full_data_abort_duplicate);
    assert_eq!(
        duplicate_observed.stale_full_data_abort.unwrap().at,
        duplicate_first
    );
    let mut duplicate = o.clone();
    duplicate.stale_full_data_abort_duplicate = true;
    assert!(!warm_last_good_victim_fast(
        &plans[0], &removal, &duplicate, &state
    ));
    for case in 0..8 {
        let mailbox = Arc::new(IndexResponseMailbox::with_data_capacity(1));
        if case != 0 {
            mailbox.enable_perf_observation();
        }
        let latest = Arc::new(std::sync::Mutex::new(HashMap::from([(victim, 0)])));
        let shutdown = Arc::new(AtomicBool::new(case == 3));
        if case == 2 {
            latest.lock().unwrap().insert(victim, 6);
        }
        if case == 4 {
            assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _lock = latest.lock().unwrap();
                panic!("controlled latest poison");
            }))
            .is_err());
        }
        mailbox.try_publish(batch()).unwrap();
        if matches!(case, 1 | 2) {
            mailbox.close();
        }
        let send = crate::app::index_worker::mailbox_response_sink_for_test(
            &req,
            Arc::clone(&mailbox),
            latest,
            shutdown,
        );
        let response = match case {
            5 => {
                send(IndexResponse::Started {
                    request_id: 6,
                    source: crate::indexer::IndexSource::Walker,
                })
                .unwrap();
                IndexResponse::Started {
                    request_id: 6,
                    source: crate::indexer::IndexSource::Walker,
                }
            }
            6 => IndexResponse::Batch {
                request_id: 99,
                entries: Vec::new(),
            },
            7 => IndexResponse::ReplaceAll {
                request_id: 6,
                entries: vec![IndexEntry {
                    path: root.join("replace.txt"),
                    kind: crate::entry::EntryKind::file(),
                    kind_known: true,
                }],
            },
            _ => batch(),
        };
        assert!(send(response).is_err());
        let result = mailbox.perf_observation();
        if case == 7 {
            let a = result.stale_full_data_abort.unwrap();
            assert_eq!(a.data_kind, "replace-all");
            assert_eq!(a.latest_id, Some(0));
        } else {
            assert!(
                result.stale_full_data_abort.is_none(),
                "nonqualifying sink case {case}"
            );
            assert!(!result.stale_full_data_abort_duplicate);
        }
    }
    let i = &mut driver.app.shell.indexing;
    i.perf_observe_history = false;
    i.perf_warm_removals.clear();
    i.request_tabs.remove(&6);
}

fn retained_victim_role(o: &crate::app::index_mailbox::IndexPerfObservation) -> &'static str {
    if o.terminal_kind == Some("failed") {
        "declared-Warm-eviction-stale-Full-retained-last-good"
    } else {
        "declared-Warm-eviction-retained-last-good"
    }
}

#[test]
fn tc_233_fixture_membership_scan_is_untimed_in_actual_multiframe_driver() {
    if super::super::child_process::isolate(
        module_path!(),
        "tc_233_fixture_membership_scan_is_untimed_in_actual_multiframe_driver",
    ) {
        return;
    }
    let fixture = ExtendedFixture::new(16_384, super::fixture::Shape::FlatMixed);
    for (profile, source) in [
        (Profile::Files, Source::Walker),
        (Profile::Folders, Source::Walker),
        (Profile::IgnoreCase, Source::FileList),
    ] {
        for condition in [false, true] {
            let row = run(&fixture, &[], profile, source, condition, false);
            let filter = profile.filter(condition);
            let expected = fixture
                .expected
                .iter()
                .filter(|record| {
                    if record.is_dir {
                        filter.dirs
                    } else {
                        filter.files
                    }
                })
                .count();
            assert_eq!(row["expected_final_logical_entries"], expected);
            assert!(
                row["frames"].as_u64().unwrap() > 1,
                "exercise repeated actual frames"
            );
            assert_eq!(
                row["timed_fixture_scan_passes"], 0,
                "immutable fixture scans must precede t0"
            );
            assert_eq!(row["fixture_scans_before_t0"], 1);
            assert_eq!(row["correct"], true);
        }
    }
}

#[test]
fn tc_233_actual_driver_records_bounded_frame_progress_without_changing_endpoints() {
    if super::super::child_process::isolate(
        module_path!(),
        "tc_233_actual_driver_records_bounded_frame_progress_without_changing_endpoints",
    ) {
        return;
    }
    let fixture = ExtendedFixture::new(16_384, super::fixture::Shape::FlatMixed);
    let row = run(
        &fixture,
        &[],
        Profile::IgnoreCase,
        Source::FileList,
        false,
        false,
    );
    let trace = &row["frame_diagnostics"];
    assert_eq!(
        trace["limit"], 256,
        "bounded frame diagnostics must be present"
    );
    let total = row["frames"].as_u64().unwrap();
    assert_eq!(trace["total_frames"], total);
    let records = trace["records"].as_array().unwrap();
    assert_eq!(records.len() as u64, total.min(256));
    assert_eq!(trace["truncated_frames"], total.saturating_sub(256));
    assert!(total > 1);
    let mut previous_end = 0.0;
    for (index, record) in records.iter().enumerate() {
        assert_eq!(record["frame"], index + 1);
        let start = record["started_ms"].as_f64().unwrap();
        let end = record["ended_ms"].as_f64().unwrap();
        assert!(start >= previous_end && end >= start);
        assert!(record["observed_ms"].as_f64().unwrap() >= end);
        assert!(end <= row["results_ready_ms"].as_f64().unwrap());
        assert!(record["primary_request_id"].as_u64().unwrap() > 0);
        previous_end = end;
    }
    assert_eq!(row["timed_fixture_scan_passes"], 0);
    assert!(row["index_ready_ms"].as_f64().unwrap() <= row["results_ready_ms"].as_f64().unwrap());
    assert_eq!(row["correct"], true);
}

#[test]
fn tc_233_frame_diagnostics_keep_all_frame_maximum_after_bounded_overflow() {
    let mut trace = FrameDiagnostics::new();
    let capacity = trace.records.capacity();
    for index in 0..300 {
        let start = Duration::from_millis(index * 16 + if index >= 280 { 1000 } else { 0 });
        trace.push(
            start,
            start + Duration::from_millis(3),
            FrameState {
                active_filter_cursor: Some(index as usize),
                observed_ms: ms(start + Duration::from_millis(4)),
                active_tab: Some(2),
                pending_request_id: Some(3),
                primary_request_id: 4,
                ingested: index as usize,
                index_debt: true,
                result_debt: false,
                producer_data_end_ms: None,
                producer_terminal_ms: None,
            },
        );
        assert_eq!(trace.records.capacity(), capacity, "no timed reallocation");
    }
    assert_eq!(trace.total, 300);
    assert_eq!(trace.records.len(), FRAME_DIAGNOSTIC_LIMIT);
    assert_eq!(trace.max_start_gap, Duration::from_millis(1016));
    let report = trace.report();
    assert_eq!(report["truncated_frames"], 44);
    assert_eq!(report["max_frame_start_gap_ms"], 1016.0);
    assert_eq!(report["records"][255]["frame"], 256);
    assert_eq!(report["records"][255]["active_filter_cursor"], 255);
}

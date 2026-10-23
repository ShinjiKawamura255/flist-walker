//! A Finished offer samples freshness before attempting mailbox publication.
//! A permitted refresh can close that mailbox between those two operations.
use super::{request_acquisition_owned, requires_latest_generation, PlannedRequest, Source};
use crate::app::index_mailbox::IndexPerfObservation;
use std::path::Path;

pub(super) fn finished_offer_revocation(
    id: u64,
    tab: u64,
    plans: &[PlannedRequest],
    o: &IndexPerfObservation,
    root: &Path,
    source: Source,
    released: bool,
) -> bool {
    let mut owners = plans.iter().filter(|p| p.id == id && p.tab == tab);
    let Some(owner) = owners.next() else {
        return false;
    };
    if owners.next().is_some() {
        return false;
    }
    let Some(next) = owner.revoked_by.filter(|next| *next > id) else {
        return false;
    };
    if plans
        .iter()
        .filter(|p| p.id == next && p.tab == tab)
        .count()
        != 1
        || !requires_latest_generation(plans, next, tab)
        || owner.permission_reason != Some("IgnoreList-PreserveSort-refresh")
        || owner.completed_predecessor
        || owner.completed_predecessor_observed_at.is_some()
        || owner.victim.is_some()
    {
        return false;
    }
    let (
        Some(admitted),
        Some(started),
        Some(data),
        Some(offer),
        Some(permission),
        Some(closed),
        Some(sent),
        Some(returned),
    ) = (
        o.admitted_at,
        o.started_published,
        o.data_publish_end,
        o.terminal_offered,
        owner.permission_at,
        o.mailbox_closed_at,
        o.terminal_send_returned,
        o.request_processing_returned,
    )
    else {
        return false;
    };
    // This is a revoked predecessor, never a successful terminal. The caller
    // still requires the successor's actual success, final snapshot and no debt.
    released
        && o.allocation_observed
        && request_acquisition_owned(o, root, source)
        && o.started_source == Some(source.name())
        && o.entries_emitted > 0
        && o.batches > 0
        && !o.skipped_closed_before_start
        && o.stale_full_data_abort.is_none()
        && !o.stale_full_data_abort_duplicate
        && o.mailbox_closed
        && o.terminal_offer_kind == Some("finished")
        && o.terminal_offer_current == Some(true)
        && o.terminal_offer_error.is_none()
        && o.terminal_published.is_none()
        && o.terminal_kind.is_none()
        && o.terminal_source.is_none()
        && admitted <= started
        && started <= data
        && data <= offer
        && offer <= permission
        && permission <= closed
        && closed <= sent
        && sent <= returned
}

#[test]
fn tc_229_finished_offer_revocation_rejects_missing_wrong_or_reordered_evidence() {
    use std::time::{Duration, Instant};
    let t = Instant::now();
    let at = |n| t + Duration::from_millis(n);
    let root = std::path::PathBuf::from("owned-fixture");
    let plans = vec![
        PlannedRequest {
            id: 2,
            tab: 7,
            revoked_by: Some(3),
            permission_at: Some(at(4)),
            permission_reason: Some("IgnoreList-PreserveSort-refresh"),
            ..Default::default()
        },
        PlannedRequest {
            id: 3,
            tab: 7,
            ..Default::default()
        },
    ];
    let observation = IndexPerfObservation {
        allocation_observed: true,
        admitted_at: Some(at(0)),
        started_published: Some(at(1)),
        admitted_root: Some(root.clone()),
        started_root: Some(root.clone()),
        started_source: Some("FileList"),
        entries_emitted: 4096,
        batches: 4,
        data_publish_end: Some(at(2)),
        terminal_offered: Some(at(3)),
        terminal_offer_kind: Some("finished"),
        terminal_offer_current: Some(true),
        mailbox_closed: true,
        mailbox_closed_at: Some(at(5)),
        terminal_send_returned: Some(at(6)),
        request_processing_returned: Some(at(7)),
        ..Default::default()
    };
    assert!(finished_offer_revocation(
        2,
        7,
        &plans,
        &observation,
        &root,
        Source::FileList,
        true
    ));
    assert!(!finished_offer_revocation(
        2,
        7,
        &plans,
        &observation,
        &root,
        Source::FileList,
        false
    ));
    assert!(!finished_offer_revocation(
        2,
        7,
        &plans,
        &observation,
        &root,
        Source::Walker,
        true
    ));
    for invalid in 0..43 {
        let mut p = plans.clone();
        let mut o = observation.clone();
        match invalid {
            0 => p[0].permission_at = None,
            1 => p[0].permission_reason = None,
            2 => p[0].permission_reason = Some("unrelated-operation"),
            3 => p[0].permission_at = Some(at(6)),
            4 => p[0].permission_at = Some(at(2)),
            5 => p[0].revoked_by = None,
            6 => p[0].revoked_by = Some(1),
            7 => p[1].id = 4,
            8 => p[1].tab = 8,
            9 => p.push(PlannedRequest {
                id: 4,
                tab: 7,
                ..Default::default()
            }),
            10 => o.started_root = Some("wrong-root".into()),
            11 => o.admitted_root = Some("wrong-root".into()),
            12 => o.started_source = Some("Walker"),
            13 => o.admitted_at = None,
            14 => o.started_published = None,
            15 => o.data_publish_end = None,
            16 => o.data_publish_end = Some(at(4)),
            17 => o.terminal_offered = None,
            18 => o.mailbox_closed_at = None,
            19 => o.mailbox_closed_at = Some(at(3)),
            20 => o.mailbox_closed = false,
            21 => o.terminal_published = Some(at(3)),
            22 => o.terminal_kind = Some("finished"),
            23 => o.terminal_source = Some("FileList"),
            24 => o.terminal_offer_kind = Some("canceled"),
            25 => o.terminal_offer_error = Some("receiver error".into()),
            26 => o.terminal_offer_current = None,
            27 => o.terminal_offer_current = Some(false),
            28 => o.terminal_send_returned = None,
            29 => o.terminal_send_returned = Some(at(4)),
            30 => o.request_processing_returned = None,
            31 => o.request_processing_returned = Some(at(5)),
            32 => o.allocation_observed = false,
            33 => o.skipped_closed_before_start = true,
            34 => o.stale_full_data_abort_duplicate = true,
            35 => o.entries_emitted = 0,
            36 => o.batches = 0,
            37 => p[0].completed_predecessor = true,
            38 => p[0].completed_predecessor_observed_at = Some(at(3)),
            39 => o.admitted_at = Some(at(2)),
            40 => o.started_published = Some(at(3)),
            41 => p.push(p[0].clone()),
            42 => p.push(p[1].clone()),
            _ => unreachable!(),
        }
        assert!(
            !finished_offer_revocation(2, 7, &p, &o, &root, Source::FileList, true),
            "accepted invalid evidence {invalid}"
        );
    }
}

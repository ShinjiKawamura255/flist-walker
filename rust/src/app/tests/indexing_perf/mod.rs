//! Actual-worker indexing measurements. These are headless GUI observations.
use super::*;

#[derive(Default)]
struct CompletionTruth {
    terminal_published: bool,
    own_request_settled: bool,
    snapshot_valid: bool,
    pending_debt: bool,
    latest_results_valid: bool,
}
impl CompletionTruth {
    fn snapshot_settled(&self) -> bool {
        self.terminal_published
            && self.own_request_settled
            && self.snapshot_valid
            && !self.pending_debt
    }
    fn results_settled(&self) -> bool {
        self.snapshot_settled() && self.latest_results_valid
    }
}
#[test]
fn tc_228_terminal_truth_rejects_false_completion_and_debt() {
    let mut state = CompletionTruth {
        terminal_published: true,
        own_request_settled: true,
        snapshot_valid: true,
        latest_results_valid: true,
        ..Default::default()
    };
    assert!(state.results_settled());
    state.pending_debt = true;
    assert!(!state.snapshot_settled());
    state.pending_debt = false;
    state.terminal_published = false;
    assert!(!state.results_settled());
    state.terminal_published = true;
    state.own_request_settled = false;
    assert!(!state.snapshot_settled());
}

mod harness;
use harness::{run_sample, Case, Fixture, Source};

#[test]
fn tc_228_real_worker_headless_refresh_preserves_snapshot_and_query() {
    if crate::app::tests::indexing_perf::harness::child_process::isolate(
        module_path!(),
        "tc_228_real_worker_headless_refresh_preserves_snapshot_and_query",
    ) {
        return;
    }
    let fixture = Fixture::new(16_384);
    for source in [Source::FileList, Source::Walker] {
        for case in [Case::B0, Case::S1Selective, Case::S1Dense, Case::S2] {
            let sample = run_sample(&fixture, case, source);
            assert!(sample.t1 <= sample.terminal);
            assert!(sample.terminal <= sample.t2);
            assert!(sample.t2 <= sample.t3);
        }
    }
}
fn setting(name: &str, default: usize) -> usize {
    let value = std::env::var(name)
        .map(|v| {
            v.parse::<usize>()
                .expect("positive integer performance setting")
        })
        .unwrap_or(default);
    assert!(valid_setting_value(name, value), "{name} out of bounds");
    value
}
fn valid_setting_value(name: &str, value: usize) -> bool {
    value > 0 && value <= if name.ends_with("PAIRS") { 31 } else { 500_000 }
}
fn unique_selection(names: &[&str]) -> bool {
    !names.is_empty()
        && names.iter().all(|name| !name.is_empty())
        && names.iter().collect::<HashSet<_>>().len() == names.len()
}
#[test]
fn tc_229_config_rejects_zero_excessive_pairs_and_over_cap_entries() {
    assert!(valid_setting_value("FW_INDEX_PERF_ENTRIES", 100_000));
    assert!(valid_setting_value("FW_INDEX_PERF_PAIRS", 7));
    assert!(!valid_setting_value("FW_INDEX_PERF_PAIRS", 0));
    assert!(!valid_setting_value("FW_INDEX_PERF_PAIRS", 32));
    assert!(!valid_setting_value("FW_INDEX_PERF_ENTRIES", 500_001));
}
#[test]
fn tc_229_config_rejects_empty_or_duplicate_case_source_selection() {
    assert!(unique_selection(&["FileList", "Walker"]));
    assert!(!unique_selection(&["Walker", "Walker"]));
    assert!(!unique_selection(&[]));
    assert!(!unique_selection(&[""]));
}
fn require_release_profile() {
    // Reject an explicit debug measurement at runtime without preventing the
    // normal debug test binary from compiling.
    #[cfg(debug_assertions)]
    panic!("use cargo test --release");
}
fn ms(value: Duration) -> f64 {
    value.as_secs_f64() * 1000.0
}
#[test]
#[ignore = "actual-worker release paired indexing contention observations; run explicitly"]
fn perf_indexing_contention_paired() {
    require_release_profile();
    let count = setting("FW_INDEX_PERF_ENTRIES", 100_000);
    let pairs = setting("FW_INDEX_PERF_PAIRS", 7);
    let cases = std::env::var("FW_INDEX_PERF_CASES")
        .unwrap_or_else(|_| "S1-selective,S1-dense,S2".into())
        .split(',')
        .map(Case::parse)
        .collect::<Vec<_>>();
    let fixture = Fixture::new(count);
    let sources = std::env::var("FW_INDEX_PERF_SOURCES")
        .unwrap_or_else(|_| "FileList,Walker".into())
        .split(',')
        .map(Source::parse)
        .collect::<Vec<_>>();
    assert!(
        unique_selection(&cases.iter().map(|c| c.name()).collect::<Vec<_>>()),
        "duplicate case selection"
    );
    assert!(
        unique_selection(&sources.iter().map(|s| s.name()).collect::<Vec<_>>()),
        "duplicate source selection"
    );
    let full_profile = count == 100_000
        && pairs >= 7
        && [Case::S1Selective, Case::S1Dense, Case::S2]
            .iter()
            .all(|case| cases.contains(case))
        && [Source::FileList, Source::Walker]
            .iter()
            .all(|source| sources.contains(source));
    eprintln!(
        "INDEX_PERF_META {}",
        serde_json::json!({"schema_version":1,"sample_entries":count,"pairs":pairs,"sources":sources.iter().map(|s|s.name()).collect::<Vec<_>>(),"cases":cases.iter().map(|s|s.name()).collect::<Vec<_>>(),"frame_period_ms":16,"host":std::env::consts::OS,"arch":std::env::consts::ARCH,"cpu_parallelism":std::thread::available_parallelism().map_or(1,|v|v.get()),"package_version":env!("CARGO_PKG_VERSION"),"cargo_profile":"release","full_profile":full_profile,"fixture":"flat-real-files-ascii-japanese-v2","fixture_signature":fixture.manifest_signature(),"trace":"empty-startup-held10-edit25-clear50-v4","observer":"cfg-test-opt-in-batch-phase-v2","native":false})
    );
    for source in sources {
        for case in &cases {
            let case = *case;
            for warmup in [Case::B0, case] {
                let _ = run_sample(&fixture, warmup, source);
            }
            for pair in 0..pairs {
                let order = if pair % 2 == 0 {
                    [Case::B0, case]
                } else {
                    [case, Case::B0]
                };
                for (position, condition) in order.into_iter().enumerate() {
                    let s = run_sample(&fixture, condition, source);
                    let mut record = serde_json::json!({
                        "schema_version":1,"source":source.name(),"comparison":case.name(),"case":condition.name(),
                        "pair":pair,"order":if pair%2==0 {"AB"} else {"BA"},"position":position,"sample_entries":count,
                        "request_id":s.request,"coordinator_terminal_settled_ms":ms(s.own_request_settled),"last_confirmed_snapshot_unsettled_ms":ms(s.last_confirmed_unsettled),
                        "data_publish_end_ms":ms(s.t1),"terminal_publish_ms":ms(s.terminal),"index_ready_ms":ms(s.t2),"results_ready_ms":ms(s.t3),
                        "max_no_work_progress_ms":ms(s.max_progress_gap),"max_ingest_gap_ms":ms(s.max_ingest_gap),"max_frame_ms":ms(s.frame_max),"frames":s.frames,
                        "correct":true,"snapshot_signature":format!("{:016x}",s.snapshot_signature),"results_signature":format!("{:016x}",s.results_signature)
                    });
                    record.as_object_mut().unwrap().extend(serde_json::json!({
                        "batches":s.observation.batches,"entries_emitted":s.observation.entries_emitted,"replace_all":s.observation.replacements,
                        "full_count":s.observation.full_retries,"blocked_batches":s.observation.blocked_batches,"full_wait_ms":ms(s.observation.full_wait),
                        "overlap_dispatches":s.overlap_dispatches,"producer_overlap_dispatches":s.producer_overlap_dispatches,
                        "overlap_responses":s.overlap_responses,"cancel_requested":s.cancel_requested,"edit_checkpoints":s.edit_checkpoints,
                        "search_events":s.search_events,"contention_eligible":s.contention_eligible,"worker_observations":s.worker_observations,
                        "overlap_executions":s.overlap_executions,"completed_searches":s.completed_searches,"canceled_searches":s.canceled_searches,
                        "input_trace":s.input_trace,"phase_backtracks":s.phase_backtracks,"overlap_completed_or_superseded":s.overlap_completed_or_superseded,"query_edits":s.query_edits
                    }).as_object().unwrap().clone());
                    eprintln!("INDEX_PERF_SAMPLE {record}");
                }
            }
        }
    }
}

struct ProgressDeadline {
    last_progress: Duration,
    max_gap: Duration,
}
impl ProgressDeadline {
    fn new(max_gap: Duration) -> Self {
        Self {
            last_progress: Duration::ZERO,
            max_gap,
        }
    }
    fn observe(&mut self, now: Duration, progressed: bool) -> bool {
        if progressed {
            self.last_progress = now;
        }
        now.saturating_sub(self.last_progress) > self.max_gap
    }
}
#[test]
fn tc_228_frozen_progress_is_detected_without_wall_clock_sleep() {
    let mut guard = ProgressDeadline::new(Duration::from_millis(10));
    assert!(!guard.observe(Duration::from_millis(9), false));
    assert!(
        guard.observe(Duration::from_millis(11), false),
        "deliberately frozen update cycle"
    );
    let mut guard = ProgressDeadline::new(Duration::from_millis(10));
    assert!(!guard.observe(Duration::from_millis(9), true));
    assert!(!guard.observe(Duration::from_millis(18), false));
    assert!(guard.observe(Duration::from_millis(20), false));
}

#[test]
#[ignore = "release observer on/off overhead observation; run explicitly"]
fn perf_indexing_observer_overhead() {
    require_release_profile();
    let count = setting("FW_INDEX_PERF_ENTRIES", 100_000);
    let pairs = setting("FW_INDEX_PERF_PAIRS", 7);
    let fixture = Fixture::new(count);
    let _ = run_sample(&fixture, Case::B0, Source::FileList);
    let _ = harness::run_unobserved_b0(&fixture);
    for pair in 0..pairs {
        for enabled in if pair % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            let elapsed = if enabled {
                run_sample(&fixture, Case::B0, Source::FileList).t3
            } else {
                harness::run_unobserved_b0(&fixture)
            };
            eprintln!(
                "INDEX_PERF_OBSERVER {}",
                serde_json::json!({"schema_version":1,"pair":pair,"observer_enabled":enabled,"sample_entries":count,"results_ready_ms":ms(elapsed),"correct":true,"full_profile":count == 100_000 && pairs >= 7})
            );
        }
    }
}

fn genuine_search_overlap(
    stats: &crate::app::worker::search_perf::SearchPerfObservation,
    start: Instant,
    index_ready: Duration,
) -> bool {
    !stats.skipped_canceled
        && stats.evaluated_candidates > 0
        && stats
            .started_at
            .is_some_and(|began| began >= start && began.duration_since(start) < index_ready)
        && stats
            .evaluation_completed_at
            .is_some_and(|ended| ended >= start)
}
fn contention_eligible(
    case: Case,
    workers: &[(u64, crate::app::worker::search_perf::SearchPerfObservation)],
    start: Instant,
    cutoff: Duration,
) -> bool {
    let executions = workers
        .iter()
        .filter(|(_, stats)| genuine_search_overlap(stats, start, cutoff))
        .map(|(id, _)| *id)
        .collect::<HashSet<_>>();
    executions.len()
        >= match case {
            Case::B0 => 0,
            Case::S2 => 2,
            Case::S1Selective | Case::S1Dense => 1,
        }
}
fn worker_progress(
    stats: &crate::app::worker::search_perf::SearchPerfObservation,
) -> (usize, usize) {
    (
        stats.evaluated_candidates,
        usize::from(stats.completed_at.is_some() && stats.evaluated_candidates > 0),
    )
}
#[test]
fn tc_228_s2_requires_two_distinct_genuine_search_executions() {
    use crate::app::worker::search_perf::SearchPerfObservation;
    let start = Instant::now();
    let executed = SearchPerfObservation {
        started_at: Some(start + Duration::from_millis(1)),
        evaluation_completed_at: Some(start + Duration::from_millis(2)),
        candidates: 100,
        evaluated_candidates: 100,
        ..Default::default()
    };
    let pre_canceled = SearchPerfObservation {
        candidates: 100,
        canceled_at: Some(start + Duration::from_millis(2)),
        skipped_canceled: true,
        ..Default::default()
    };
    let cutoff = Duration::from_millis(10);
    assert!(contention_eligible(
        Case::S1Dense,
        &[(1, executed.clone())],
        start,
        cutoff
    ));
    assert!(
        !contention_eligible(
            Case::S2,
            &[(1, executed.clone()), (2, pre_canceled)],
            start,
            cutoff
        ),
        "two dispatched requests with only one evaluated request cannot cover S2"
    );
    assert!(
        !contention_eligible(
            Case::S2,
            &[(1, executed.clone()), (1, executed.clone())],
            start,
            cutoff
        ),
        "one request counted twice cannot cover S2"
    );
    assert!(contention_eligible(
        Case::S2,
        &[(1, executed.clone()), (2, executed)],
        start,
        cutoff
    ));
}
#[test]
fn tc_228_cancel_only_spin_does_not_advance_work_progress() {
    use crate::app::worker::search_perf::SearchPerfObservation;
    let start = Instant::now();
    let canceled = SearchPerfObservation {
        started_at: Some(start),
        canceled_at: Some(start + Duration::from_millis(1)),
        candidates: 100,
        ..Default::default()
    };
    assert_eq!(
        worker_progress(&canceled),
        (0, 0),
        "cancellation without evaluated candidates is diagnostic only"
    );
    let completed = SearchPerfObservation {
        completed_at: Some(start),
        evaluated_candidates: 100,
        ..Default::default()
    };
    assert_eq!(worker_progress(&completed), (100, 1));
    let evaluated_cancel = SearchPerfObservation {
        evaluated_candidates: 100,
        evaluation_completed_at: Some(start),
        ..canceled
    };
    assert_eq!(
        worker_progress(&evaluated_cancel).0,
        100,
        "actual evaluation remains constructive work"
    );
}
#[test]
fn tc_228_search_overlap_requires_actual_evaluation_and_request_interval() {
    use crate::app::worker::search_perf::SearchPerfObservation;
    let start = Instant::now();
    let mut stats = SearchPerfObservation {
        started_at: Some(start + Duration::from_millis(1)),
        evaluation_completed_at: Some(start + Duration::from_millis(15)),
        candidates: 100,
        evaluated_candidates: 100,
        ..Default::default()
    };
    assert!(
        genuine_search_overlap(&stats, start, Duration::from_millis(10)),
        "evaluation can finish after indexing but began during it"
    );
    stats.skipped_canceled = true;
    assert!(!genuine_search_overlap(
        &stats,
        start,
        Duration::from_millis(10)
    ));
    stats.skipped_canceled = false;
    stats.started_at = None;
    assert!(
        !genuine_search_overlap(&stats, start, Duration::from_millis(10)),
        "queued is not executed"
    );
    stats.started_at = Some(start + Duration::from_millis(11));
    assert!(
        !genuine_search_overlap(&stats, start, Duration::from_millis(10)),
        "late execution"
    );
    stats.started_at = Some(start + Duration::from_millis(1));
    stats.evaluated_candidates = 0;
    assert!(
        !genuine_search_overlap(&stats, start, Duration::from_millis(10)),
        "cached or empty is not evaluated work"
    );
    stats.evaluated_candidates = 100;
    stats.evaluation_completed_at = None;
    assert!(
        !genuine_search_overlap(&stats, start, Duration::from_millis(10)),
        "interrupted evaluation lacks an actual count"
    );
}

use crate::app::worker::channel::BoundedSender;
struct ParserWorkerCleanup {
    shutdown: Arc<AtomicBool>,
    tx: Option<BoundedSender<IndexRequest>>,
    handles: Vec<thread::JoinHandle<()>>,
}
impl ParserWorkerCleanup {
    fn stop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        self.tx.take();
        let begin = Instant::now();
        while self.handles.iter().any(|h| !h.is_finished())
            && begin.elapsed() < Duration::from_secs(5)
        {
            thread::sleep(Duration::from_millis(1));
        }
        let timed_out = self.handles.iter().any(|h| !h.is_finished());
        for h in self.handles.drain(..) {
            if h.is_finished() && h.join().is_err() && !thread::panicking() {
                panic!("parser worker failed");
            }
        }
        if timed_out {
            if thread::panicking() {
                eprintln!("parser worker shutdown timeout during unwind");
            } else {
                panic!("parser worker shutdown timeout");
            }
        }
    }
}
impl Drop for ParserWorkerCleanup {
    fn drop(&mut self) {
        self.stop();
    }
}
use super::cases::Profile;
use super::fixture::{ExtendedFixture, Shape};
use super::oracle::{ExtendedOracle, Filter};
use super::*;
use crate::app::worker::protocol::{IndexEntry, IndexRequest, IndexResponse};
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) fn parser_run(fixture: &ExtendedFixture, profile: Profile) -> serde_json::Value {
    assert!(profile.parser());
    fixture.prepare_source(Source::FileList);
    let filter = profile.filter(true);
    let expected = fixture
        .records
        .iter()
        .filter(|r| if r.is_dir { filter.dirs } else { filter.files })
        .collect::<Vec<_>>();
    let shutdown = Arc::new(AtomicBool::new(false));
    let latest = Arc::new(std::sync::Mutex::new(HashMap::from([(1, 1)])));
    let (tx, _rx, mailboxes, handles) =
        crate::app::index_worker::spawn_index_worker(Arc::clone(&shutdown), latest);
    let mut cleanup = ParserWorkerCleanup {
        shutdown: Arc::clone(&shutdown),
        tx: Some(tx),
        handles,
    };
    let mailbox = Arc::new(IndexResponseMailbox::new());
    mailbox.enable_perf_observation();
    mailboxes.lock().unwrap().insert(1, Arc::clone(&mailbox));
    let start = Instant::now();
    cleanup
        .tx
        .as_ref()
        .unwrap()
        .try_send(IndexRequest {
            request_id: 1,
            tab_id: 1,
            root: fixture.root.clone(),
            use_filelist: true,
            include_files: filter.files,
            include_dirs: filter.dirs,
            max_depth: crate::indexer::MaxDepth::unlimited(),
            follow_links: false,
            complete_walker_snapshot: false,
        })
        .unwrap_or_else(|_| panic!("owned parser dispatch"));
    let mut entries = Vec::<IndexEntry>::new();
    let mut terminal = false;
    let mut frames = 0;
    let mut max_poll = Duration::ZERO;
    let mut deadline = ProgressDeadline::new(Duration::from_secs(30));
    let mut last_progress = Duration::ZERO;
    let mut max_gap = Duration::ZERO;
    while !terminal {
        let begin = Instant::now();
        let old = entries.len();
        for _ in 0..8 {
            let Some(response) = mailbox.try_recv() else {
                break;
            };
            match response {
                IndexResponse::Started { request_id, source } => {
                    assert_eq!(request_id, 1);
                    assert!(matches!(source, IndexSource::FileList(_)));
                }
                IndexResponse::Batch {
                    request_id,
                    entries: batch,
                } => {
                    assert_eq!(request_id, 1);
                    entries.extend(batch);
                }
                IndexResponse::Finished { request_id, source } => {
                    assert_eq!(request_id, 1);
                    assert!(matches!(source, IndexSource::FileList(_)));
                    terminal = true;
                }
                _ => panic!("unexpected parser response"),
            }
        }
        let now = start.elapsed();
        max_gap = max_gap.max(now - last_progress);
        let progress = entries.len() > old;
        if progress {
            last_progress = now;
        }
        assert!(!deadline.observe(now, progress));
        assert!(now < Duration::from_secs(120));
        max_poll = max_poll.max(begin.elapsed());
        frames += 1;
        if !terminal {
            if let Some(wait) = (begin + FRAME_PERIOD).checked_duration_since(Instant::now()) {
                thread::sleep(wait);
            }
        }
    }
    let end = start.elapsed();
    cleanup.stop();
    assert_eq!(entries.len(), expected.len());
    for (actual, want) in entries.iter().zip(&expected) {
        assert_eq!(actual.path, want.path);
        assert!(actual.kind_known);
        assert_eq!(actual.kind.is_dir.expect("known kind"), want.is_dir);
    }
    let o = mailbox.perf_observation();
    assert_eq!(o.terminal_kind, Some("finished"));
    assert_eq!(o.terminal_source, Some("FileList"));
    serde_json::json!({"schema_version":1,"profile_family":"extension","comparison":profile.name(),"case":profile.name(),"source":"FileList","measurement_kind":"worker-only-parser","comparison_kind":"AA-variability","sample_entries":fixture.records.len(),"expected_final_logical_entries":expected.len(),"unresolved_kind_count":entries.iter().filter(|e|!e.kind_known).count(),"filtered_mask":{"files":filter.files,"folders":filter.dirs},"fixture_signature":fixture.signature(),"snapshot_signature":format!("{:016x}",signature(&fixture.root,entries.iter().map(|e|&e.path))),"index_ready_ms":null,"results_ready_ms":null,"worker_drained_ms":ms(end),"data_publish_end_ms":ms(o.data_publish_end.unwrap().duration_since(start)),"terminal_publish_ms":ms(o.terminal_published.unwrap().duration_since(start)),"full_wait_ms":ms(o.full_wait),"full_count":o.full_retries,"blocked_batches":o.blocked_batches,"entries_emitted":o.entries_emitted,"max_no_work_progress_ms":ms(max_gap),"max_ingest_gap_ms":ms(max_gap),"max_frame_ms":null,"max_mailbox_poll_ms":ms(max_poll),"frames":frames,"correct":true,"contention_eligible":true,"native":false})
}
#[test]
fn tc_229_parser_kind_masks_are_real_worker_contracts() {
    if crate::app::tests::indexing_perf::harness::child_process::isolate(
        module_path!(),
        "tc_229_parser_kind_masks_are_real_worker_contracts",
    ) {
        return;
    }
    let fixture = ExtendedFixture::new(4096, Shape::FlatMixed);
    for profile in [Profile::ParserFiles, Profile::ParserFolders] {
        assert_eq!(parser_run(&fixture, profile)["correct"], true);
    }
}
fn truncation_required(input: usize, cap: usize) -> bool {
    input > cap
}
#[test]
fn tc_229_truncation_requires_more_input_than_actual_limit() {
    assert!(!truncation_required(500000, 500000));
    assert!(truncation_required(500001, 500000));
}

#[test]
fn tc_229_controlled_full_blocks_then_resumes_with_fresh_deadline() {
    if crate::app::tests::indexing_perf::harness::child_process::isolate(
        module_path!(),
        "tc_229_controlled_full_blocks_then_resumes_with_fresh_deadline",
    ) {
        return;
    }
    let fixture = ExtendedFixture::new(4096, Shape::FlatFiles);
    fixture.prepare_source(Source::FileList);
    let filter = Filter {
        files: true,
        dirs: true,
        ignore_enabled: false,
        ignore_case: true,
    };
    let mut driver = Driver::new();
    driver.settle_startup();
    super::driver::configure(&mut driver, &fixture, Source::FileList, filter, false);
    driver.app.request_index_refresh();
    super::driver::settle_setup(&mut driver);
    let generation = fixture.next_generation();
    let mut driver = driver;
    driver.app.shell.indexing.perf_observe_requests = true;
    // Request admission occurs with the ordinary real reclaimer; pressure begins after dispatch.
    driver.app.request_index_refresh();
    let id = driver.app.shell.indexing.pending_request_id.unwrap();
    let mailbox = driver.mailbox(id);
    driver.app.shell.tabs.pause_resource_reclaimer();
    for n in 0..crate::app::tab_resources::TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut tab = driver.app.capture_active_tab_state(9000 + n as u64);
        driver
            .app
            .shell
            .tabs
            .retire_tab_resources_for_test(tab.take_heavy_resources())
            .unwrap();
    }
    let paused = Instant::now();
    let mut blocked = false;
    while paused.elapsed() < Duration::from_secs(5) {
        driver.paced_frame();
        if driver.app.shell.indexing.pending_finish.is_some()
            || driver.app.shell.indexing.build_reclaim_pending
        {
            blocked = true;
            break;
        }
    }
    assert!(
        blocked,
        "real index terminal commit must retain debt under Full"
    );
    assert!(driver.index_debt());
    driver.app.shell.tabs.resume_resource_reclaimer();
    let resume = Instant::now();
    let mut deadline = ProgressDeadline::new(Duration::from_secs(5));
    let mut prior = Vec::new();
    loop {
        driver.paced_frame();
        let progress = driver.progress(mailbox.perf_observation().batches);
        let changed = progress != prior;
        prior = progress;
        assert!(
            !deadline.observe(resume.elapsed(), changed),
            "post-resume deadline"
        );
        if !super::driver::all_index_debt(&driver) && !super::driver::result_debt(&driver) {
            break;
        }
        assert!(resume.elapsed() < Duration::from_secs(15));
    }
    assert_eq!(mailbox.perf_observation().terminal_kind, Some("finished"));
    let oracle = ExtendedOracle::new(
        generation.fixture(),
        Source::FileList,
        filter,
        "",
        ResultSortMode::Score,
        ResultSortScope::ShownResults,
        1000,
    );
    assert!(oracle.valid_snapshot(
        &driver.app.shell.runtime.all_entries,
        &driver.app.shell.runtime.entries
    ));
    // Frozen post-resume work cannot pass merely because the pause was released.
    let mut frozen = ProgressDeadline::new(Duration::from_secs(5));
    assert!(!frozen.observe(Duration::ZERO, false));
    assert!(frozen.observe(Duration::from_secs(6), false));
    drop(driver);
    drop(generation);
}

struct OwnedRuntimeConfig(crate::runtime_config::RuntimeConfig);
impl OwnedRuntimeConfig {
    fn cap(cap: usize) -> Self {
        let old = crate::runtime_config::current_runtime_config();
        let mut measured = old.clone();
        measured.walker_max_entries = cap;
        crate::runtime_config::set_process_runtime_config(measured);
        Self(old)
    }
}
impl Drop for OwnedRuntimeConfig {
    fn drop(&mut self) {
        crate::runtime_config::set_process_runtime_config(self.0.clone());
    }
}
pub(super) fn truncated_run(fixture: &ExtendedFixture, cap: usize) -> serde_json::Value {
    assert!(
        truncation_required(fixture.expected.len(), cap),
        "truncated input must exceed actual cap"
    );
    let _owned_config = OwnedRuntimeConfig::cap(cap);
    fixture.prepare_source(Source::Walker);
    let allowed = fixture
        .expected
        .iter()
        .map(|r| r.path.clone())
        .collect::<HashSet<_>>();
    let mut driver = Driver::new();
    driver.settle_startup();
    super::driver::configure(
        &mut driver,
        fixture,
        Source::Walker,
        Filter {
            files: true,
            dirs: true,
            ignore_enabled: false,
            ignore_case: true,
        },
        false,
    );
    driver.app.shell.indexing.perf_observe_requests = true;
    let start = Instant::now();
    driver.app.request_index_refresh();
    let id = driver.app.shell.indexing.pending_request_id.unwrap();
    let mailbox = driver.mailbox(id);
    let mut t2 = None;
    let mut max_frame = Duration::ZERO;
    let mut max_gap = Duration::ZERO;
    let mut last_progress = Duration::ZERO;
    let mut prior = Vec::new();
    let mut deadline = ProgressDeadline::new(Duration::from_secs(30));
    let t3;
    loop {
        let begin = Instant::now();
        max_frame = max_frame.max(driver.frame());
        let now = start.elapsed();
        let progress = driver.progress(mailbox.perf_observation().batches);
        max_gap = max_gap.max(now - last_progress);
        let changed = progress != prior;
        if changed {
            last_progress = now;
            prior = progress;
        }
        assert!(!deadline.observe(now, changed));
        assert!(now < Duration::from_secs(120));
        if !super::driver::all_index_debt(&driver)
            && driver.app.shell.runtime.all_entries.len() == cap
        {
            if t2.is_none() {
                t2 = Some(now);
            }
            if !super::driver::result_debt(&driver) {
                t3 = now;
                break;
            }
        }
        if let Some(wait) = (begin + FRAME_PERIOD).checked_duration_since(Instant::now()) {
            thread::sleep(wait);
        }
    }
    let o = mailbox.perf_observation();
    assert_eq!(o.terminal_kind, Some("finished"));
    assert_eq!(o.truncated_limit, Some(cap));
    assert_eq!(o.started_source, Some("Walker"));
    assert_eq!(o.terminal_source, Some("Walker"));
    let all = &driver.app.shell.runtime.all_entries;
    let paths = all.iter().map(|e| e.path.clone()).collect::<HashSet<_>>();
    assert_eq!(paths.len(), cap);
    assert!(paths.is_subset(&allowed));
    assert!(all
        .iter()
        .all(|e| e.kind.is_some_and(|kind| kind.is_dir == Some(false))));
    assert_eq!(driver.app.shell.runtime.entries.len(), cap);
    assert_eq!(driver.app.shell.runtime.total_match_count, cap);
    assert_eq!(
        driver.app.shell.runtime.results.len(),
        cap.min(driver.app.shell.runtime.limit)
    );
    assert!(driver
        .app
        .shell
        .runtime
        .results
        .iter()
        .all(|(p, s)| paths.contains(p) && *s == 0.0));
    assert!(driver.app.shell.runtime.query_state.search_error.is_none());
    let mut sorted = paths.into_iter().collect::<Vec<_>>();
    sorted.sort();
    serde_json::json!({"schema_version":1,"profile_family":"extension","comparison":"W1-truncated","case":"W1-truncated","source":"Walker","measurement_kind":"headless-GUI-actual-workers","comparison_kind":"AA-variability","sample_entries":fixture.records.len(),"actual_limit":cap,"expected_final_logical_entries":cap,"subset_order":"parallel walker first-N; only unique fixture membership required","fixture_signature":fixture.signature(),"snapshot_signature":format!("{:016x}",signature(&fixture.root,sorted.iter())),"index_ready_ms":ms(t2.unwrap()),"results_ready_ms":ms(t3),"data_publish_end_ms":ms(o.data_publish_end.unwrap().duration_since(start)),"terminal_publish_ms":ms(o.terminal_published.unwrap().duration_since(start)),"full_wait_ms":ms(o.full_wait),"full_count":o.full_retries,"entries_emitted":o.entries_emitted,"max_no_work_progress_ms":ms(max_gap),"max_ingest_gap_ms":ms(max_gap),"max_frame_ms":ms(max_frame),"correct":true,"contention_eligible":true,"native":false})
}

#[test]
fn tc_229_parser_worker_cleanup_runs_after_intentional_panic() {
    let shutdown = Arc::new(AtomicBool::new(false));
    let worker_shutdown = Arc::clone(&shutdown);
    let (tx, rx) = crate::app::worker::channel::bounded_request_channel::<IndexRequest>(2);
    let worker = thread::spawn(move || {
        while !worker_shutdown.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(1));
        }
        drop(rx);
    });
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _cleanup = ParserWorkerCleanup {
            shutdown: Arc::clone(&shutdown),
            tx: Some(tx),
            handles: vec![worker],
        };
        panic!("intentional owned parser cleanup");
    }));
    assert!(panic.is_err());
    assert!(shutdown.load(Ordering::Relaxed));
}

#[test]
fn tc_229_payload_free_registry_isolates_request_runtime_and_does_not_pin() {
    use crate::app::index_mailbox::{
        register_perf_request, take_perf_request, IndexPerfObservation,
    };
    let a = Arc::new(std::sync::Mutex::new(HashMap::<u64, u64>::new()));
    let b = Arc::new(std::sync::Mutex::new(HashMap::<u64, u64>::new()));
    let first = Arc::new(std::sync::Mutex::new(IndexPerfObservation::default()));
    let second = Arc::new(std::sync::Mutex::new(IndexPerfObservation::default()));
    register_perf_request(Arc::as_ptr(&a) as usize, 1, &first);
    register_perf_request(Arc::as_ptr(&b) as usize, 1, &second);
    assert_eq!(Arc::strong_count(&first), 1);
    assert_eq!(Arc::strong_count(&second), 1);
    assert!(Arc::ptr_eq(
        &take_perf_request(Arc::as_ptr(&a) as usize, 1).unwrap(),
        &first
    ));
    assert!(take_perf_request(Arc::as_ptr(&a) as usize, 1).is_none());
    assert!(Arc::ptr_eq(
        &take_perf_request(Arc::as_ptr(&b) as usize, 1).unwrap(),
        &second
    ));
    let weak = Arc::downgrade(&first);
    register_perf_request(Arc::as_ptr(&a) as usize, 2, &first);
    drop(first);
    assert!(weak.upgrade().is_none());
    assert!(take_perf_request(Arc::as_ptr(&a) as usize, 2).is_none());
}
#[test]
fn tc_229_admitted_closed_mailbox_records_actual_skipped_worker_return() {
    if crate::app::tests::indexing_perf::harness::child_process::isolate(
        module_path!(),
        "tc_229_admitted_closed_mailbox_records_actual_skipped_worker_return",
    ) {
        return;
    }
    use crate::app::index_mailbox::{register_perf_request, IndexPerfObservation};
    let shutdown = Arc::new(AtomicBool::new(false));
    let latest = Arc::new(std::sync::Mutex::new(HashMap::from([(1, 1)])));
    let observation = Arc::new(std::sync::Mutex::new(IndexPerfObservation {
        allocation_observed: true,
        ..Default::default()
    }));
    register_perf_request(Arc::as_ptr(&latest) as usize, 1, &observation);
    let (tx, _rx, mailboxes, handles) =
        crate::app::index_worker::spawn_index_worker(Arc::clone(&shutdown), latest);
    let mut cleanup = ParserWorkerCleanup {
        shutdown: Arc::clone(&shutdown),
        tx: Some(tx),
        handles,
    };
    // No mailbox at dequeue is deterministic; the worker must skip the body,
    // without fabricating Started/Finished or inferring return from cleanup.
    assert!(mailboxes.lock().unwrap().is_empty());
    cleanup
        .tx
        .as_ref()
        .unwrap()
        .try_send(IndexRequest {
            request_id: 1,
            tab_id: 1,
            root: std::env::temp_dir(),
            use_filelist: false,
            include_files: true,
            include_dirs: true,
            max_depth: crate::indexer::MaxDepth::unlimited(),
            follow_links: false,
            complete_walker_snapshot: false,
        })
        .unwrap_or_else(|_| panic!("admit skipped request"));
    observation.lock().unwrap().admitted_at = Some(Instant::now());
    let deadline = Instant::now() + Duration::from_secs(5);
    while observation
        .lock()
        .unwrap()
        .request_processing_returned
        .is_none()
    {
        assert!(
            Instant::now() < deadline,
            "skipped admitted worker did not actually return"
        );
        thread::sleep(Duration::from_millis(1));
    }
    let observed = observation.lock().unwrap().clone();
    assert!(observed.skipped_closed_before_start);
    assert!(observed.started_published.is_none());
    assert!(observed.terminal_kind.is_none());
    assert!(physical_request_complete(&observed));
    cleanup.stop();
}

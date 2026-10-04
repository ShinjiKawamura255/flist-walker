use super::*;
use crate::app::index_mailbox::{IndexPerfObservation, IndexResponseMailbox};
use std::collections::BTreeSet;
use std::io::Write;

#[path = "extensions/mod.rs"]
mod extensions;

#[path = "child_process.rs"]
pub(super) mod child_process;

pub(super) const FRAME_PERIOD: Duration = Duration::from_millis(16);
pub(super) struct Fixture {
    pub(super) root: PathBuf,
    pub(super) paths: Vec<PathBuf>,
    filelist: Vec<u8>,
}
impl Fixture {
    pub(super) fn new(count: usize) -> Self {
        let lexical = test_root("indexing-perf");
        fs::create_dir_all(&lexical).unwrap();
        let root = fs::canonicalize(lexical).unwrap();
        let mut list =
            std::io::BufWriter::new(fs::File::create(root.join("FileList.txt")).unwrap());
        let paths = (0..count)
            .map(|i| {
                let name = format!(
                    "item_{i:08}_{}_{}.txt",
                    if i % 100 == 0 { "needle" } else { "bulk" },
                    if i % 10 == 0 { "日本語" } else { "ascii" }
                );
                let path = root.join(&name);
                fs::write(&path, []).unwrap();
                writeln!(list, "{name}").unwrap();
                path
            })
            .collect();
        list.flush().unwrap();
        let filelist = fs::read(root.join("FileList.txt")).unwrap();
        Self {
            root,
            paths,
            filelist,
        }
    }
    pub(super) fn manifest_signature(&self) -> String {
        format!("{:016x}", signature(&self.root, self.paths.iter()))
    }
    pub(super) fn prepare_source(&self, source: Source) {
        if source == Source::FileList {
            fs::write(self.root.join("FileList.txt"), &self.filelist).unwrap();
        } else if self.root.join("FileList.txt").exists() {
            fs::remove_file(self.root.join("FileList.txt")).unwrap();
        }
    }
    fn oracle(&self, case: Case, limit: usize, source: Source) -> Oracle {
        let query = case.final_query();
        let count = if query == "needle" {
            self.paths
                .iter()
                .enumerate()
                .filter(|(i, _)| i % 100 == 0)
                .count()
        } else {
            self.paths.len()
        };
        if query.is_empty() {
            let ranked = self
                .paths
                .iter()
                .cloned()
                .map(|p| (p, 0.0))
                .collect::<Vec<_>>();
            return Oracle::new(ranked, count, limit, source, true);
        }
        let entries = Arc::new(self.paths.iter().cloned().map(Entry::unknown).collect());
        let (result, error) = crate::search::rank_search_results_uncached(
            &entries,
            query,
            &self.root,
            self.paths.len(),
            false,
            true,
            true,
            ResultSortMode::Score,
            ResultSortScope::ShownResults,
        );
        assert!(error.is_none());
        assert_eq!(
            result.total_match_count, count,
            "independent pattern membership count"
        );
        Oracle::new(result.results, count, limit, source, false)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Source {
    FileList,
    Walker,
}
impl Source {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::FileList => "FileList",
            Self::Walker => "Walker",
        }
    }
    pub(super) fn parse(name: &str) -> Self {
        match name {
            "FileList" => Self::FileList,
            "Walker" => Self::Walker,
            _ => panic!("unknown source {name}"),
        }
    }
}
struct Oracle {
    ranked: Vec<(PathBuf, f64)>,
    count: usize,
    scores: HashMap<PathBuf, f64>,
    required_above_cutoff: HashSet<PathBuf>,
    limit: usize,
    cutoff: f64,
    source: Source,
    empty: bool,
}
impl Oracle {
    fn new(
        mut ranked: Vec<(PathBuf, f64)>,
        count: usize,
        limit: usize,
        source: Source,
        empty: bool,
    ) -> Self {
        let scores: HashMap<PathBuf, f64> = ranked.iter().cloned().collect();
        ranked.truncate(limit);
        let cutoff = ranked.last().map_or(0.0, |(_, score)| *score);
        // Construct outside t0; validation checks at most limit required paths
        // instead of rescanning the full fixture on every measured frame.
        let required_above_cutoff = scores
            .iter()
            .filter(|(_, score)| **score > cutoff)
            .map(|(path, _)| path.clone())
            .collect();
        Self {
            ranked,
            count,
            scores,
            required_above_cutoff,
            limit,
            cutoff,
            source,
            empty,
        }
    }
    fn valid(&self, results: &[(PathBuf, f64)], count: usize) -> bool {
        if count != self.count || results.len() != self.count.min(self.limit) {
            return false;
        }
        if self.source == Source::FileList {
            return results == self.ranked;
        }
        // Walker traversal order is not a contract for tied scores. Check each
        // independently generated candidate's score and the top-score boundary.
        let paths = results.iter().map(|(path, _)| path).collect::<HashSet<_>>();
        paths.len() == results.len()
            && self
                .required_above_cutoff
                .iter()
                .all(|path| paths.contains(path))
            && results
                .iter()
                .all(|(path, score)| self.scores.get(path) == Some(score) && *score >= self.cutoff)
            && (self.empty || results.windows(2).all(|pair| pair[0].1 >= pair[1].1))
    }
}

#[test]
fn tc_228_walker_oracle_requires_all_candidates_above_cutoff() {
    let ranked = vec![
        (PathBuf::from("A"), 100.0),
        (PathBuf::from("B"), 90.0),
        (PathBuf::from("C"), 90.0),
    ];
    let oracle = Oracle::new(ranked.clone(), 3, 2, Source::Walker, false);
    assert!(
        !oracle.valid(&ranked[1..], 3),
        "B,C meet cutoff but omit mandatory higher-score A"
    );
    assert!(oracle.valid(&ranked[..2], 3));
    assert!(
        oracle.valid(&[ranked[0].clone(), ranked[2].clone()], 3),
        "only cutoff ties can substitute"
    );
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Case {
    B0,
    S1Selective,
    S1Dense,
    S2,
}
impl Case {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::B0 => "B0",
            Self::S1Selective => "S1-selective",
            Self::S1Dense => "S1-dense",
            Self::S2 => "S2",
        }
    }
    fn initial_query(self) -> &'static str {
        match self {
            Self::B0 => "",
            Self::S1Selective => "needle",
            Self::S1Dense => "item",
            Self::S2 => "",
        }
    }
    fn final_query(self) -> &'static str {
        if self == Self::S2 {
            ""
        } else {
            self.initial_query()
        }
    }
    pub(super) fn parse(name: &str) -> Self {
        match name {
            "B0" => Self::B0,
            "S1-selective" => Self::S1Selective,
            "S1-dense" => Self::S1Dense,
            "S2" => Self::S2,
            _ => panic!("unknown case {name}"),
        }
    }
}

pub(super) struct Driver {
    pub(super) app: FlistWalkerApp,
    _settings: super::super::support::TestSettingsScope,
    ctx: egui::Context,
    _startup_root: OwnedEmptyRoot,
    pending_query: Option<String>,
    input_events: Vec<(String, Instant, usize)>,
    progress_high_water: HashMap<(u64, u8, usize, usize), usize>,
    progress_last_marks: HashMap<(u64, u8, usize, usize), usize>,
    phase_backtracks: usize,
    previous_reclaim: usize,
    completed_reclaim: usize,
    last_frame_end: Instant,
}
impl Driver {
    pub(super) fn new() -> Self {
        let settings = test_settings_scope("indexing-perf");
        let startup_root = test_root("indexing-perf-empty");
        fs::create_dir_all(&startup_root).unwrap();
        let mut app = settings.app(startup_root.clone(), 1000, String::new());
        app.shell.ui.show_preview = false;
        app.shell.ui.ignore_list_enabled = false;
        app.shell.runtime.ignore_list_terms = Arc::new(Vec::new());
        app.filelist_auto_check_enabled = false;
        Self {
            app,
            _settings: settings,
            ctx: egui::Context::default(),
            _startup_root: OwnedEmptyRoot(startup_root),
            pending_query: None,
            input_events: Vec::new(),
            progress_high_water: HashMap::new(),
            progress_last_marks: HashMap::new(),
            phase_backtracks: 0,
            previous_reclaim: 0,
            completed_reclaim: 0,
            last_frame_end: Instant::now(),
        }
    }
    pub(super) fn frame(&mut self) -> Duration {
        let begin = Instant::now();
        if let Some(query) = self.pending_query.take() {
            if self.app.shell.runtime.query_state.query != query {
                self.input_events
                    .push((query.clone(), Instant::now(), self.ingested()));
            }
            if query.is_empty() {
                self.app.clear_query_and_selection();
            } else {
                self.app.shell.runtime.query_state.query = query;
                self.app.finish_programmatic_query_replacement();
                self.app.update_results();
            }
        }
        let _ = self.ctx.run_ui(
            egui::RawInput {
                focused: true,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1600.0, 900.0),
                )),
                ..Default::default()
            },
            |ui| {
                assert!(self.app.run_update_cycle(ui));
            },
        );
        self.last_frame_end = Instant::now();
        begin.elapsed()
    }
    pub(super) fn paced_frame(&mut self) -> Duration {
        let start = Instant::now();
        let elapsed = self.frame();
        if let Some(rest) = FRAME_PERIOD.checked_sub(start.elapsed()) {
            thread::sleep(rest);
        }
        elapsed
    }
    pub(super) fn settle_startup(&mut self) {
        let start = Instant::now();
        while self.app.shell.indexing.pending_request_id.is_some()
            || self.index_debt()
            || self.app.shell.search.in_progress()
        {
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "startup settlement timeout"
            );
            self.paced_frame();
        }
        assert!(
            self.app.shell.runtime.all_entries.is_empty(),
            "clean initial snapshot"
        );
    }
    pub(super) fn index_debt(&self) -> bool {
        let i = &self.app.shell.indexing;
        i.in_progress
            || !i.build.pending_entries.is_empty()
            || i.pending_entries_request_id.is_some()
            || i.refresh_after_pending_finish.is_some()
            || i.root_after_pending_finish.is_some()
            || i.pending_finish.is_some()
            || i.build_reclaim_pending
            || i.pending_stale_build_reclaim.is_some()
            || i.pending_replace_all.is_some()
            || i.background_finalizations.keys().next().is_some()
            || i.build.active_filter.is_some()
            || i.kind_resolution_in_progress
            || !i.build.pending_kind_paths.is_empty()
            || !i.build.in_flight_kind_paths.is_empty()
            || self.app.shell.tabs.reclaimer_pending() != 0
    }
    fn query(&mut self, query: &str) {
        self.pending_query = Some(query.into());
    }
    pub(super) fn progress(&mut self, batches: usize) -> Vec<usize> {
        let i = &self.app.shell.indexing;
        let request = i.pending_request_id.unwrap_or(0);
        let mut marks = vec![((request, 0, 0, 0), self.ingested())];
        if let Some(filter) = &i.build.active_filter {
            marks.push((
                (
                    request,
                    1,
                    i.build.index.entries.len(),
                    self.input_events.len(),
                ),
                filter.cursor,
            ));
        }
        for id in i.background_finalizations.keys() {
            let f = i.background_finalizations.get(id).unwrap();
            marks.extend([
                ((*id, 2, 0, 0), f.completed_entries.len()),
                ((*id, 3, 0, self.input_events.len()), f.filter_cursor),
                ((*id, 4, 0, 0), f.kind_cursor),
            ]);
        }
        for (key, value) in marks {
            let high_water = self.progress_high_water.entry(key).or_default();
            if self
                .progress_last_marks
                .insert(key, value)
                .is_some_and(|previous| value < previous)
            {
                self.phase_backtracks += 1;
            }
            *high_water = (*high_water).max(value);
        }
        let reclaim = self.app.shell.tabs.reclaimer_pending();
        self.completed_reclaim += self.previous_reclaim.saturating_sub(reclaim);
        self.previous_reclaim = reclaim;
        let mut evaluated = 0;
        let mut ended = 0;
        for stats in self.app.shell.search.perf_workers.values() {
            let stats = stats.lock().unwrap();
            let progress = worker_progress(&stats);
            evaluated += progress.0;
            ended += progress.1;
        }
        vec![
            batches,
            self.progress_high_water.values().sum(),
            evaluated,
            ended,
            self.completed_reclaim,
            usize::from(i.pending_request_id.is_none()),
        ]
    }
    pub(super) fn ingested(&self) -> usize {
        let i = &self.app.shell.indexing;
        if let Some(id) = i.pending_request_id {
            if let Some(f) = i.background_finalizations.get(&id) {
                return f.completed_entries.len();
            }
        }
        if i.pending_request_id.is_none() {
            self.app.shell.runtime.all_entries.len()
        } else {
            i.build.index.entries.len()
        }
    }
    pub(super) fn mailbox(&self, request: u64) -> Arc<IndexResponseMailbox> {
        self.app
            .shell
            .indexing
            .response_mailboxes
            .lock()
            .unwrap()
            .get(&request)
            .expect("actual request mailbox")
            .clone()
    }
}
struct OwnedEmptyRoot(PathBuf);
impl Drop for OwnedEmptyRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Debug)]
pub(super) struct Sample {
    pub(super) request: u64,
    pub(super) own_request_settled: Duration,
    pub(super) last_confirmed_unsettled: Duration,
    pub(super) producer_overlap_dispatches: usize,
    pub(super) overlap_responses: usize,
    pub(super) cancel_requested: usize,
    pub(super) edit_checkpoints: Vec<(usize, f64)>,
    pub(super) search_events: Vec<serde_json::Value>,
    pub(super) contention_eligible: bool,
    pub(super) worker_observations: Vec<serde_json::Value>,
    pub(super) overlap_executions: usize,
    pub(super) completed_searches: usize,
    pub(super) canceled_searches: usize,
    pub(super) input_trace: Vec<serde_json::Value>,
    pub(super) phase_backtracks: usize,
    pub(super) t1: Duration,
    pub(super) terminal: Duration,
    pub(super) t2: Duration,
    pub(super) t3: Duration,
    pub(super) max_progress_gap: Duration,
    pub(super) max_ingest_gap: Duration,
    pub(super) frame_max: Duration,
    pub(super) frames: usize,
    pub(super) observation: IndexPerfObservation,
    pub(super) overlap_dispatches: usize,
    pub(super) overlap_completed_or_superseded: usize,
    pub(super) query_edits: usize,
    pub(super) snapshot_signature: u64,
    pub(super) results_signature: u64,
}
fn signature<'a>(root: &Path, paths: impl Iterator<Item = &'a PathBuf>) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for path in paths {
        let path = path.strip_prefix(root).unwrap().to_string_lossy();
        for byte in (path.len() as u64)
            .to_le_bytes()
            .iter()
            .copied()
            .chain(path.as_bytes().iter().copied())
        {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    hash
}

pub(super) fn run_sample(fixture: &Fixture, case: Case, source: Source) -> Sample {
    run_sample_config(fixture, case, source, true)
}
fn run_sample_config(fixture: &Fixture, case: Case, source: Source, observe: bool) -> Sample {
    fixture.prepare_source(source);
    let mut driver = Driver::new();
    driver.settle_startup();
    let expected = fixture.oracle(case, driver.app.shell.runtime.limit, source);
    // Configure target outside timing; constructor startup was on an empty root.
    driver.app.shell.runtime.root = fixture.root.clone();
    driver.app.shell.runtime.use_filelist = source == Source::FileList;
    driver.app.shell.search.perf_enabled = observe;
    driver.app.shell.indexing.perf_observe_requests = observe;
    let dispatch_global_cap = crate::runtime_config::current_runtime_config().walker_max_entries;
    let start = Instant::now();
    driver.app.request_index_refresh();
    let request = driver
        .app
        .shell
        .indexing
        .pending_request_id
        .expect("refresh identity");
    let mailbox = driver.mailbox(request);
    driver.query("");
    let mut query_edits = 0;
    let mut dispatched = BTreeSet::new();
    let mut completed = BTreeSet::new();
    let mut prior_search = driver.app.shell.search.pending_request_id();
    let mut frames = 0;
    let mut frame_max = Duration::ZERO;
    let mut t2 = None;
    let mut last_confirmed_unsettled = Duration::ZERO;
    let mut t3 = None;
    let mut held_query_set = !matches!(case, Case::S1Selective | Case::S1Dense);
    let mut dense_started = case != Case::S2;
    let mut changed_query = case != Case::S2;
    let mut cleared = case != Case::S2;
    let mut last_progress = Duration::ZERO;
    let mut last_ingest = Duration::ZERO;
    let mut max_gap = Duration::ZERO;
    let mut max_ingest_gap = Duration::ZERO;
    let mut previous_progress = Vec::new();
    let mut prior_ingested = 0;
    let mut edit_checkpoints = Vec::new();
    let mut deadline = ProgressDeadline::new(Duration::from_secs(30));
    loop {
        if driver.app.shell.indexing.pending_request_id == Some(request)
            || driver
                .app
                .shell
                .indexing
                .request_tabs
                .contains_key(&request)
            || driver.index_debt()
            || driver.app.shell.runtime.all_entries.len() != fixture.paths.len()
        {
            last_confirmed_unsettled = start.elapsed();
        }
        if driver.app.shell.indexing.pending_request_id == Some(request) {
            if let Some(id) = driver.app.shell.search.pending_request_id() {
                dispatched.insert(id);
            }
            if let Some(id) = prior_search {
                if driver.app.shell.search.pending_request_id() != Some(id) {
                    completed.insert(id);
                }
            }
        }
        if !held_query_set && driver.ingested() >= fixture.paths.len() / 10 {
            driver.query(case.final_query());
            query_edits += 1;
            held_query_set = true;
            edit_checkpoints.push((driver.ingested(), ms(start.elapsed())));
        } else if case == Case::S2
            && !dense_started
            && driver.ingested() >= fixture.paths.len() / 10
        {
            driver.query("item");
            query_edits += 1;
            dense_started = true;
            edit_checkpoints.push((driver.ingested(), ms(start.elapsed())));
        } else if case == Case::S2 && !changed_query && driver.ingested() >= fixture.paths.len() / 4
        {
            driver.query("needle");
            query_edits += 1;
            changed_query = true;
            edit_checkpoints.push((driver.ingested(), ms(start.elapsed())));
        } else if case == Case::S2
            && changed_query
            && !cleared
            && driver.ingested() >= fixture.paths.len() / 2
        {
            driver.query("");
            query_edits += 1;
            cleared = true;
            edit_checkpoints.push((driver.ingested(), ms(start.elapsed())));
        }
        prior_search = driver.app.shell.search.pending_request_id();
        frame_max = frame_max.max(driver.paced_frame());
        frames += 1;
        let elapsed = driver.last_frame_end.duration_since(start);
        let observation = mailbox.perf_observation();
        let progress = driver.progress(observation.batches);
        let ingested = driver.ingested();
        if deadline.observe(elapsed, progress != previous_progress) {
            eprintln!(
                "INDEX_PERF_STALL {}",
                serde_json::json!({
                    "case":format!("{case:?}"),"source":source.name(),"dispatch_global_cap":dispatch_global_cap,
                    "current_global_cap":crate::runtime_config::current_runtime_config().walker_max_entries,
                    "request":request,"pending_request":driver.app.shell.indexing.pending_request_id,
                    "request_still_owned":driver.app.shell.indexing.request_tabs.contains_key(&request),
                    "index_in_progress":driver.app.shell.indexing.in_progress,"index_debt":driver.index_debt(),
                    "expected_entries":fixture.paths.len(),"all_entries":driver.app.shell.runtime.all_entries.len(),"visible_entries":driver.app.shell.runtime.entries.len(),
                    "actual_emitted":observation.entries_emitted,"actual_truncated_limit":observation.truncated_limit,"terminal_kind":observation.terminal_kind,
                    "terminal_source":observation.terminal_source,"terminal_published":observation.terminal_published.is_some(),"processing_returned":observation.request_processing_returned.is_some(),
                    "query":driver.app.shell.runtime.query_state.query,"search_in_progress":driver.app.shell.search.in_progress(),"search_pending":driver.app.shell.search.pending_request_id(),
                    "result_count":driver.app.shell.runtime.results.len(),"total_match_count":driver.app.shell.runtime.total_match_count,
                    "root":driver.app.shell.runtime.root,"frames":frames,"elapsed_ms":ms(elapsed),"last_progress_ms":ms(last_progress),"progress":progress
                })
            );
            panic!("no visible work progress for 30 seconds: {case:?}");
        }
        if progress != previous_progress {
            max_gap = max_gap.max(elapsed - last_progress);
            last_progress = elapsed;
        }
        if ingested > prior_ingested {
            max_ingest_gap = max_ingest_gap.max(elapsed - last_ingest);
            last_ingest = elapsed;
        }
        previous_progress = progress;
        prior_ingested = ingested;
        let truth = CompletionTruth {
            terminal_published: !observe
                || (observation.terminal_published.is_some()
                    && observation.data_publish_end.is_some()),
            own_request_settled: driver.app.shell.indexing.pending_request_id != Some(request)
                && !driver
                    .app
                    .shell
                    .indexing
                    .request_tabs
                    .contains_key(&request),
            snapshot_valid: driver.app.shell.runtime.all_entries.len() == fixture.paths.len(),
            pending_debt: driver.index_debt(),
            latest_results_valid: !driver.app.shell.search.in_progress()
                && !driver.app.shell.indexing.search_resume_pending
                && !driver.app.shell.indexing.search_rerun_pending
                && driver.app.shell.runtime.query_state.query == case.final_query()
                && !driver.app.shell.worker_bus.sort.in_progress
                && driver
                    .app
                    .shell
                    .worker_bus
                    .sort
                    .pending_request_id
                    .is_none()
                && expected.valid(
                    &driver.app.shell.runtime.results,
                    driver.app.shell.runtime.total_match_count,
                ),
        };
        if t2.is_none() && truth.snapshot_settled() {
            t2 = Some(elapsed);
        }
        if t3.is_none() && truth.results_settled() {
            t3 = Some(elapsed);
        }
        if t2.is_some() && t3.is_some() {
            break;
        }
        assert!(elapsed < Duration::from_secs(120), "measurement timeout: case={case:?},request={request},frames={frames},index={},debt={},query={:?},search={},dispatch={:?},clear={cleared},entries={},results={},matches={}", driver.app.shell.indexing.in_progress, driver.index_debt(), driver.app.shell.runtime.query_state.query, driver.app.shell.search.in_progress(), dispatched, driver.app.shell.runtime.all_entries.len(), driver.app.shell.runtime.results.len(), driver.app.shell.runtime.total_match_count);
    }
    let observation = mailbox.perf_observation();
    let t2 = t2.unwrap();
    let t3 = t3.unwrap();
    max_gap = max_gap.max(t3 - last_progress);
    max_ingest_gap = max_ingest_gap.max(t2 - last_ingest);
    let events = &driver.app.shell.search.perf_events;
    let dispatch_events = events
        .iter()
        .filter(|e| e.event == "dispatch" && e.candidates > 0)
        .collect::<Vec<_>>();
    let overlap_responses = events
        .iter()
        .filter(|e| {
            e.event.starts_with("response")
                && e.at.duration_since(start) <= last_confirmed_unsettled
                && dispatch_events.iter().any(|d| d.request_id == e.request_id)
        })
        .count();
    let overlap_dispatches = dispatch_events
        .iter()
        .filter(|e| e.at.duration_since(start) <= last_confirmed_unsettled)
        .count();
    let producer_overlap_dispatches = dispatch_events
        .iter()
        .filter(|e| observation.data_publish_end.is_some_and(|end| e.at <= end))
        .count();
    let cancel_requested = events
        .iter()
        .filter(|e| e.event == "cancel_requested")
        .count();
    // Read phase observations after the measured UI settles. RX polling can
    // happen after t2 even when search CPU work overlapped the indexing request.
    let phase_wait = Instant::now();
    while driver.app.shell.search.perf_workers.values().any(|stats| {
        let stats = stats.lock().unwrap();
        stats.completed_at.is_none() && stats.canceled_at.is_none()
    }) {
        assert!(
            phase_wait.elapsed() < Duration::from_secs(5),
            "test observer phase settlement timeout"
        );
        thread::sleep(Duration::from_millis(1));
    }
    let workers = driver
        .app
        .shell
        .search
        .perf_workers
        .iter()
        .map(|(id, stats)| (*id, stats.lock().unwrap().clone()))
        .collect::<Vec<_>>();
    let own_request_settled = driver
        .app
        .shell
        .indexing
        .perf_settled_requests
        .get(&request)
        .map_or(t2, |at| at.duration_since(start));
    let overlap_executions = workers
        .iter()
        .filter(|(_, stats)| genuine_search_overlap(stats, start, last_confirmed_unsettled))
        .count();
    let completed_searches = workers
        .iter()
        .filter(|(_, stats)| stats.completed_at.is_some())
        .count();
    let canceled_searches = workers
        .iter()
        .filter(|(_, stats)| stats.canceled_at.is_some())
        .count();
    let contention_eligible = contention_eligible(case, &workers, start, last_confirmed_unsettled);
    if case != Case::B0 && fixture.paths.len() >= 100_000 {
        assert!(contention_eligible, "substantive actual search execution overlapping indexing: {case:?}/{source:?};t2={t2:?};workers={workers:?}");
    }
    let worker_observations = workers.iter().map(|(id,stats)| serde_json::json!({"request_id":id,"candidates":stats.candidates,"evaluated_candidates":stats.evaluated_candidates,"started_ms":stats.started_at.map(|at|ms(at.duration_since(start))),"evaluation_completed_ms":stats.evaluation_completed_at.map(|at|ms(at.duration_since(start))),"completed_ms":stats.completed_at.map(|at|ms(at.duration_since(start))),"canceled_ms":stats.canceled_at.map(|at|ms(at.duration_since(start))),"skipped_canceled":stats.skipped_canceled})).collect();
    if case == Case::S2 {
        if fixture.paths.len() >= 100_000 {
            assert!(
                contention_eligible,
                "full S2 requires two distinct genuinely evaluated searches while indexing"
            );
        }
        assert!(
            cleared && query_edits == 3,
            "edit+clear requires actual earlier search activity"
        );
    }
    let actual: HashSet<_> = driver
        .app
        .shell
        .runtime
        .all_entries
        .iter()
        .map(|e| e.path.clone())
        .collect();
    assert_eq!(
        actual,
        fixture.paths.iter().cloned().collect(),
        "independent fixture membership"
    );
    assert_eq!(
        actual.len(),
        driver.app.shell.runtime.all_entries.len(),
        "no duplicates"
    );
    if source == Source::FileList {
        assert_eq!(
            driver
                .app
                .shell
                .runtime
                .all_entries
                .iter()
                .map(|e| &e.path)
                .collect::<Vec<_>>(),
            fixture.paths.iter().collect::<Vec<_>>(),
            "FileList line order"
        );
    }
    assert!(driver.app.shell.runtime.query_state.search_error.is_none());
    let input_trace = driver.input_events.iter().map(|(query, at, ingested)| {
        let dispatch = events.iter().find(|e|e.event == "dispatch" && e.candidates > 0 && e.query == *query && e.at >= *at);
        serde_json::json!({"query":query,"at_ms":ms(at.duration_since(start)),"ingested":ingested,"first_nonempty_dispatch_delay_ms":dispatch.map(|e|ms(e.at.duration_since(*at))),"latest_results_delay_ms":if query == case.final_query(){ Some(ms(t3.saturating_sub(at.duration_since(start)))) }else{None}})
    }).collect();
    assert!(
        match (source, &driver.app.shell.indexing.build.index.source) {
            (Source::FileList, IndexSource::FileList(path)) =>
                path == &fixture.root.join("FileList.txt"),
            (Source::Walker, IndexSource::Walker) => true,
            _ => false,
        },
        "actual final source differs from configured source"
    );
    Sample {
        request,
        own_request_settled,
        last_confirmed_unsettled,
        input_trace, phase_backtracks: driver.phase_backtracks,
        producer_overlap_dispatches,
        overlap_responses,
        cancel_requested,
        edit_checkpoints,
        search_events: events.iter().map(|e| serde_json::json!({"request_id":e.request_id,"at_ms":ms(e.at.duration_since(start)),"event":e.event,"candidates":e.candidates,"query":e.query})).collect(),
        contention_eligible, worker_observations, overlap_executions, completed_searches, canceled_searches,
        t1: observation.data_publish_end.map_or(Duration::ZERO, |end| end.duration_since(start)),
        terminal: observation.terminal_published.map_or(Duration::ZERO, |end| end.duration_since(start)),
        t2,
        t3,
        max_progress_gap: max_gap,
        max_ingest_gap,
        frame_max,
        frames,
        observation,
        overlap_dispatches,
        overlap_completed_or_superseded: completed.len(),
        query_edits,
        snapshot_signature: {
            let mut paths = actual.into_iter().collect::<Vec<_>>();
            paths.sort();
            signature(&fixture.root, paths.iter())
        },
        results_signature: signature(
            &fixture.root,
            driver.app.shell.runtime.results.iter().map(|(p, _)| p),
        ),
    }
}

pub(super) fn run_unobserved_b0(fixture: &Fixture) -> Duration {
    // Identical driver/oracle/cadence and settlement predicate. Because the
    // initial snapshot is empty, own-request settlement plus the exact oracle
    // independently establishes success without test-only terminal timestamps.
    let sample = run_sample_config(fixture, Case::B0, Source::FileList, false);
    assert_eq!(sample.observation.batches, 0);
    assert!(sample.observation.terminal_published.is_none());
    sample.t3
}

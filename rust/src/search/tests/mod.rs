use super::*;
use crate::ui_model::has_visible_match;
use memory_stats::memory_stats;
use std::fs;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

fn nearest_rank_percentile(samples: &[Duration], percentile: usize) -> Duration {
    assert!(!samples.is_empty(), "percentile samples must not be empty");
    assert!(
        (1..=100).contains(&percentile),
        "percentile must be in 1..=100"
    );
    let mut ordered = samples.to_vec();
    ordered.sort_unstable();
    let rank = ordered.len().saturating_mul(percentile).div_ceil(100);
    ordered[rank.saturating_sub(1)]
}

fn physical_rss_bytes() -> Option<u64> {
    memory_stats().map(|stats| stats.physical_mem as u64)
}

fn log_tc_185_rss(phase: &str, value: Option<u64>) {
    match value {
        Some(bytes) => eprintln!("tc_185 rss_phase={phase} physical_bytes={bytes}"),
        None => eprintln!("tc_185 rss_phase={phase} physical_bytes=unavailable"),
    }
}

fn record_peak_rss(peak: &AtomicU64, observed: &AtomicBool) {
    if let Some(current) = physical_rss_bytes() {
        observed.store(true, Ordering::Release);
        peak.fetch_max(current, Ordering::AcqRel);
    }
}

struct RssPeakSampler {
    stop: Arc<AtomicBool>,
    peak: Arc<AtomicU64>,
    observed: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl RssPeakSampler {
    fn start(initial: Option<u64>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let peak = Arc::new(AtomicU64::new(initial.unwrap_or(0)));
        let observed = Arc::new(AtomicBool::new(initial.is_some()));
        let worker_stop = Arc::clone(&stop);
        let worker_peak = Arc::clone(&peak);
        let worker_observed = Arc::clone(&observed);
        let worker = thread::spawn(move || loop {
            record_peak_rss(&worker_peak, &worker_observed);
            if worker_stop.load(Ordering::Acquire) {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        });
        Self {
            stop,
            peak,
            observed,
            worker: Some(worker),
        }
    }

    fn finish(mut self) -> Option<u64> {
        self.stop_and_join();
        self.observed
            .load(Ordering::Acquire)
            .then(|| self.peak.load(Ordering::Acquire))
    }

    fn stop_and_join(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.join().expect("tc_185 RSS sampler must join");
        }
    }
}

impl Drop for RssPeakSampler {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

#[test]
fn tc_185_nearest_rank_percentiles_use_sorted_ceiling_rank() {
    let samples = [
        Duration::from_millis(70),
        Duration::from_millis(10),
        Duration::from_millis(50),
        Duration::from_millis(30),
        Duration::from_millis(20),
        Duration::from_millis(60),
        Duration::from_millis(40),
    ];

    assert_eq!(
        nearest_rank_percentile(&samples, 50),
        Duration::from_millis(40)
    );
    assert_eq!(
        nearest_rank_percentile(&samples, 95),
        Duration::from_millis(70)
    );
    assert_eq!(
        nearest_rank_percentile(&samples, 99),
        Duration::from_millis(70)
    );
}

#[test]
fn orders_by_score_and_limit() {
    let entries = vec![
        PathBuf::from("/tmp/src/main.py"),
        PathBuf::from("/tmp/src/README.md"),
        PathBuf::from("/tmp/docs/design.md"),
    ];

    let out = search_entries("main", &entries, 2, false, true);
    assert!(!out.is_empty());
    assert_eq!(
        out[0].0.file_name().and_then(|s| s.to_str()),
        Some("main.py")
    );
    assert!(out.len() <= 2);
    if out.len() > 1 {
        assert!(out[0].1 >= out[1].1);
    }
}

#[test]
fn limited_search_matches_full_indexed_ranking() {
    let entries: Vec<PathBuf> = (0..200)
        .map(|i| PathBuf::from(format!("/tmp/src/module_{i:03}.rs")))
        .collect();

    let limited = try_search_entries_with_scope("module_1", &entries, 7, false, true, None, false)
        .expect("limited search");
    let full =
        try_search_entries_indexed_with_scope("module_1", &entries, false, true, None, false, None)
            .expect("full ranked search");
    let path_refs = entries.iter().map(PathBuf::as_path).collect::<Vec<_>>();
    let expected = materialize_scored_entries(&path_refs, full.into_iter().take(7).collect());

    assert_eq!(limited, expected);
}

#[test]
fn limited_search_reports_total_match_count() {
    let entries: Vec<PathBuf> = (0..20)
        .map(|i| PathBuf::from(format!("/tmp/src/module_{i:02}.rs")))
        .collect();

    let out =
        try_search_entries_with_scope_and_count("module", &entries, 5, false, true, None, false)
            .expect("search with count");

    assert_eq!(out.results.len(), 5);
    assert_eq!(out.total_match_count, 20);
}

#[test]
fn tc_155_regression_rank_search_compiles_query_once_per_request() {
    crate::query::reset_compile_counts();
    let entries = Arc::new(vec![Entry::new(
        PathBuf::from("/tmp/src/main.rs"),
        Some(crate::entry::EntryKind::file()),
    )]);
    let mut cache = SearchPrefixCache::default();

    let (result, error) = rank_search_results(
        &entries,
        "main",
        Path::new("/tmp"),
        10,
        false,
        true,
        true,
        &mut cache,
        SearchResultSortMode::Score,
        SearchResultSortScope::ShownResults,
    );

    assert!(error.is_none());
    assert_eq!(result.total_match_count, 1);
    assert_eq!(crate::query::query_compile_count(), 1);
}

#[test]
fn tc_155_regression_authoritative_search_still_applies_exclusion() {
    let entries = vec![
        PathBuf::from("/tmp/src/main.rs"),
        PathBuf::from("/tmp/docs/main.rs"),
    ];

    let result = search_entries("main !src", &entries, 10, false, true);

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].0, PathBuf::from("/tmp/docs/main.rs"));
}

#[test]
fn cancellable_rank_search_stops_before_publishing_partial_results() {
    let entries = Arc::new(
        (0..10_000)
            .map(|index| Entry::file(PathBuf::from(format!("/tmp/module-{index:05}.rs"))))
            .collect(),
    );
    let checks = AtomicUsize::new(0);
    let cancel_after_first_check = || checks.fetch_add(1, Ordering::Relaxed) >= 1;
    let mut cache = SearchPrefixCache::default();

    let outcome = rank_search_results_cancellable(
        &entries,
        "module",
        Path::new("/tmp"),
        100,
        false,
        true,
        true,
        &mut cache,
        SearchSortMode::Score,
        SearchSortScope::ShownResults,
        &cancel_after_first_check,
    );

    assert_eq!(outcome, SearchRunOutcome::Canceled);
    assert!(checks.load(Ordering::Relaxed) >= 2);
}

#[test]
fn cancellable_empty_query_checks_during_materialization_and_metadata_sort() {
    let entries = Arc::new(
        (0..10_000)
            .map(|index| Entry::unknown(PathBuf::from(format!("/tmp/item-{index:05}.rs"))))
            .collect(),
    );
    let checks = AtomicUsize::new(0);
    let cancel_during_metadata = || checks.fetch_add(1, Ordering::Relaxed) >= 45;
    let mut cache = SearchPrefixCache::default();

    let outcome = rank_search_results_cancellable(
        &entries,
        "",
        Path::new("/tmp"),
        100,
        false,
        true,
        true,
        &mut cache,
        SearchSortMode::SizeDesc,
        SearchSortScope::AllMatches,
        &cancel_during_metadata,
    );

    assert_eq!(outcome, SearchRunOutcome::Canceled);
    assert!(checks.load(Ordering::Relaxed) >= 46);
}

#[test]
fn all_matches_name_sort_can_surface_items_outside_score_limited_snapshot() {
    let entries = Arc::new(vec![
        Entry::new(
            PathBuf::from("/tmp/zeta/module.rs"),
            Some(crate::entry::EntryKind::file()),
        ),
        Entry::new(
            PathBuf::from("/tmp/alpha/module.rs"),
            Some(crate::entry::EntryKind::file()),
        ),
        Entry::new(
            PathBuf::from("/tmp/beta/module.rs"),
            Some(crate::entry::EntryKind::file()),
        ),
    ]);
    let mut cache = SearchPrefixCache::default();

    let (out, error) = rank_search_results(
        &entries,
        "module",
        Path::new("/tmp"),
        1,
        false,
        true,
        false,
        &mut cache,
        SearchResultSortMode::NameAsc,
        SearchResultSortScope::AllMatches,
    );

    assert!(error.is_none());
    assert_eq!(out.total_match_count, 3);
    assert_eq!(out.results.len(), 1);
    assert_eq!(out.results[0].0, PathBuf::from("/tmp/alpha/module.rs"));
}

#[test]
fn tc_057b_tc_163_shared_all_match_sort_orders_before_limit_for_empty_queries() {
    let entries = Arc::new(vec![
        Entry::new(PathBuf::from("/tmp/zeta"), None),
        Entry::new(PathBuf::from("/tmp/alpha"), None),
        Entry::new(PathBuf::from("/tmp/beta"), None),
    ]);
    let mut cache = SearchPrefixCache::default();

    let (out, error) = rank_search_results(
        &entries,
        "",
        Path::new("/tmp"),
        2,
        false,
        true,
        false,
        &mut cache,
        SearchSortMode::NameAsc,
        SearchSortScope::AllMatches,
    );

    assert!(error.is_none());
    assert_eq!(out.total_match_count, 3);
    assert_eq!(out.evaluated_candidate_count, 0);
    assert_eq!(
        out.results
            .iter()
            .map(|(path, _)| path.file_name().and_then(|name| name.to_str()))
            .collect::<Vec<_>>(),
        vec![Some("alpha"), Some("beta")]
    );
}

#[test]
fn tc_057b_tc_163_shared_all_match_sort_keeps_score_and_uses_stable_name_path_ties() {
    let entries = Arc::new(vec![
        Entry::new(PathBuf::from("/tmp/z/module.rs"), None),
        Entry::new(PathBuf::from("/tmp/a/module.rs"), None),
        Entry::new(PathBuf::from("/tmp/a/other.rs"), None),
    ]);
    let mut cache = SearchPrefixCache::default();

    let (name_sorted, name_error) = rank_search_results(
        &entries,
        "",
        Path::new("/tmp"),
        2,
        false,
        true,
        false,
        &mut cache,
        SearchSortMode::NameAsc,
        SearchSortScope::AllMatches,
    );
    assert!(name_error.is_none());
    assert_eq!(
        name_sorted.results,
        vec![
            (PathBuf::from("/tmp/a/module.rs"), 0.0),
            (PathBuf::from("/tmp/z/module.rs"), 0.0),
        ]
    );

    let score_entries = Arc::new(vec![
        Entry::new(PathBuf::from("/tmp/my_module.rs"), None),
        Entry::new(PathBuf::from("/tmp/module.rs"), None),
    ]);
    let (score_sorted, score_error) = rank_search_results(
        &score_entries,
        "module",
        Path::new("/tmp"),
        2,
        false,
        true,
        false,
        &mut cache,
        SearchSortMode::Score,
        SearchSortScope::AllMatches,
    );
    assert!(score_error.is_none());
    assert_eq!(score_sorted.results.len(), 2);
    assert_eq!(score_sorted.results[0].0, PathBuf::from("/tmp/module.rs"));
    assert_eq!(
        score_sorted.results[1].0,
        PathBuf::from("/tmp/my_module.rs")
    );
    assert!(score_sorted.results[0].1 > score_sorted.results[1].1);
}

#[test]
fn tc_057b_tc_163_shared_all_match_sort_returns_no_results_at_zero_limit() {
    let entries = Arc::new(vec![Entry::new(PathBuf::from("/tmp/module.rs"), None)]);
    let mut cache = SearchPrefixCache::default();

    let (out, error) = rank_search_results(
        &entries,
        "module",
        Path::new("/tmp"),
        0,
        false,
        true,
        false,
        &mut cache,
        SearchSortMode::NameAsc,
        SearchSortScope::AllMatches,
    );

    assert!(error.is_none());
    assert_eq!(out.total_match_count, 1);
    assert!(out.results.is_empty());
}

#[test]
fn tc_057b_tc_163_shared_all_match_metadata_sort_keeps_missing_and_folder_sizes_last() {
    let root = std::env::temp_dir().join(format!("flistwalker-shared-sort-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("folder")).expect("create folder");
    fs::write(root.join("small.txt"), b"a").expect("write small file");
    fs::write(root.join("large.txt"), b"abcdef").expect("write large file");
    let entries = Arc::new(vec![
        Entry::new(root.join("missing.txt"), None),
        Entry::new(root.join("folder"), None),
        Entry::new(root.join("large.txt"), None),
        Entry::new(root.join("small.txt"), None),
    ]);
    let mut cache = SearchPrefixCache::default();

    let (out, error) = rank_search_results(
        &entries,
        "",
        &root,
        4,
        false,
        true,
        false,
        &mut cache,
        SearchSortMode::SizeDesc,
        SearchSortScope::AllMatches,
    );

    assert!(error.is_none());
    assert_eq!(
        out.results,
        vec![
            (root.join("large.txt"), 0.0),
            (root.join("small.txt"), 0.0),
            (root.join("folder"), 0.0),
            (root.join("missing.txt"), 0.0),
        ]
    );
    fs::remove_dir_all(root).expect("remove temporary sort root");
}

#[test]
fn tc_057b_tc_163_shared_sort_returns_query_errors_without_partial_results() {
    let entries = Arc::new(vec![Entry::new(PathBuf::from("/tmp/module.rs"), None)]);
    let mut cache = SearchPrefixCache::default();

    let (out, error) = rank_search_results(
        &entries,
        "[",
        Path::new("/tmp"),
        10,
        true,
        true,
        false,
        &mut cache,
        SearchSortMode::NameAsc,
        SearchSortScope::AllMatches,
    );

    assert!(error.is_some());
    assert!(out.results.is_empty());
    assert_eq!(out.total_match_count, 0);
}

#[test]
fn parallel_collection_matches_sequential_ranking() {
    let entries: Vec<PathBuf> = (0..50_000)
        .map(|i| PathBuf::from(format!("/tmp/src/module_{i:05}.rs")))
        .collect();
    let path_refs = entries.iter().map(PathBuf::as_path).collect::<Vec<_>>();

    let sequential = try_collect_search_matches_with_mode(
        "module_123",
        &path_refs,
        SearchCollectOptions {
            use_regex: false,
            ignore_case: true,
            root: None,
            prefer_relative: false,
            candidate_indices: None,
            mode: SearchExecutionMode::Sequential,
        },
    )
    .expect("sequential matches")
    .scored;
    let parallel = try_collect_search_matches_with_mode(
        "module_123",
        &path_refs,
        SearchCollectOptions {
            use_regex: false,
            ignore_case: true,
            root: None,
            prefer_relative: false,
            candidate_indices: None,
            mode: SearchExecutionMode::Parallel,
        },
    )
    .expect("parallel matches")
    .scored;

    let mut sequential_sorted = sequential;
    let mut parallel_sorted = parallel;
    sort_scored_matches(&mut sequential_sorted);
    sort_scored_matches(&mut parallel_sorted);

    assert_eq!(parallel_sorted, sequential_sorted);
}

#[test]
fn indexed_search_with_candidates_matches_full_scan() {
    let entries = vec![
        PathBuf::from("/tmp/src/main.rs"),
        PathBuf::from("/tmp/src/mod.rs"),
        PathBuf::from("/tmp/src/domain.rs"),
        PathBuf::from("/tmp/src/memory.rs"),
    ];

    let base =
        try_search_entries_indexed_with_scope("ma", &entries, false, true, None, false, None)
            .expect("base query");
    let base_indices = base.iter().map(|x| x.index).collect::<Vec<_>>();
    let narrowed_full =
        try_search_entries_indexed_with_scope("mai", &entries, false, true, None, false, None)
            .expect("full scan query");
    let narrowed_from_candidates = try_search_entries_indexed_with_scope(
        "mai",
        &entries,
        false,
        true,
        None,
        false,
        Some(&base_indices),
    )
    .expect("candidate query");

    assert_eq!(narrowed_from_candidates, narrowed_full);
}

#[test]
fn empty_query_returns_empty() {
    let entries = vec![PathBuf::from("/tmp/a.txt")];
    let out = search_entries("", &entries, 10, false, true);
    assert!(out.is_empty());
}

#[test]
fn prioritizes_exact_filename_match() {
    let entries = vec![
        PathBuf::from("/tmp/src/main.py"),
        PathBuf::from("/tmp/src/main.py.bak"),
        PathBuf::from("/tmp/src/domain_main.py"),
    ];
    let out = search_entries("main.py", &entries, 10, false, true);
    assert!(!out.is_empty());
    assert_eq!(
        out[0].0.file_name().and_then(|s| s.to_str()),
        Some("main.py")
    );
}

#[test]
fn hides_non_matching_results() {
    let entries = vec![
        PathBuf::from("/tmp/src/main.py"),
        PathBuf::from("/tmp/docs/readme.md"),
    ];
    let out = search_entries("zzz", &entries, 10, false, true);
    assert!(out.is_empty());
}

#[test]
fn case_sensitive_search_respects_ignore_case_flag() {
    let entries = vec![
        PathBuf::from("/tmp/src/Main.rs"),
        PathBuf::from("/tmp/src/main.rs"),
    ];

    let sensitive = search_entries("Main", &entries, 10, false, false);
    let sensitive_names: Vec<&str> = sensitive
        .iter()
        .filter_map(|(p, _)| p.file_name().and_then(|s| s.to_str()))
        .collect();
    assert_eq!(sensitive_names, vec!["Main.rs"]);

    let insensitive = search_entries("Main", &entries, 10, false, true);
    let insensitive_names: Vec<&str> = insensitive
        .iter()
        .filter_map(|(p, _)| p.file_name().and_then(|s| s.to_str()))
        .collect();
    assert!(insensitive_names.contains(&"Main.rs"));
    assert!(insensitive_names.contains(&"main.rs"));
}

#[test]
fn exact_and_exclusion_tokens_work() {
    let entries = vec![
        PathBuf::from("/tmp/src/main.py"),
        PathBuf::from("/tmp/src/readme.md"),
    ];

    let exact = search_entries("'main", &entries, 10, false, true);
    assert_eq!(exact.len(), 1);

    let excluded = search_entries("!readme", &entries, 10, false, true);
    assert_eq!(excluded.len(), 1);
}

#[test]
fn exclusion_token_does_not_fuzzy_match() {
    let entries = vec![
        PathBuf::from("/tmp/src/main.py"),
        PathBuf::from("/tmp/src/m-a-i-n.py"),
        PathBuf::from("/tmp/src/readme.md"),
    ];

    let excluded = search_entries("!main", &entries, 10, false, true);
    let names: Vec<&str> = excluded
        .iter()
        .filter_map(|(p, _)| p.file_name().and_then(|s| s.to_str()))
        .collect();

    assert!(!names.contains(&"main.py"));
    assert!(names.contains(&"m-a-i-n.py"));
    assert!(names.contains(&"readme.md"));
}

#[test]
fn lone_operator_tokens_are_ignored() {
    let entries = vec![
        PathBuf::from("/tmp/src/main.py"),
        PathBuf::from("/tmp/src/readme.md"),
    ];

    let out_bang = search_entries("!", &entries, 10, false, true);
    assert_eq!(out_bang.len(), 2);

    let out_quote = search_entries("'", &entries, 10, false, true);
    assert_eq!(out_quote.len(), 2);

    let out_mixed = search_entries("main !", &entries, 10, false, true);
    assert_eq!(out_mixed.len(), 1);
    assert_eq!(
        out_mixed[0].0.file_name().and_then(|s| s.to_str()),
        Some("main.py")
    );
}

#[test]
fn exact_token_matches_literal_substring() {
    let entries = vec![
        PathBuf::from("/tmp/src/main.py"),
        PathBuf::from("/tmp/src/domain-main.rs"),
    ];
    let out = search_entries("'main", &entries, 10, false, true);
    assert_eq!(out.len(), 2);
}

#[test]
fn repeated_exact_tokens_require_repeated_literal_occurrences() {
    let entries = vec![
        PathBuf::from("/tmp/src/abc.txt"),
        PathBuf::from("/tmp/src/abc-abc.txt"),
        PathBuf::from("/tmp/src/abc/child-abc.txt"),
    ];
    let out = search_entries("'abc 'abc", &entries, 10, false, true);
    let names: Vec<String> = out
        .iter()
        .map(|(p, _)| p.to_string_lossy().into_owned())
        .collect();

    assert!(!names.iter().any(|path| path.ends_with("/abc.txt")));
    assert!(names.iter().any(|path| path.ends_with("/abc-abc.txt")));
    assert!(names
        .iter()
        .any(|path| path.ends_with("/abc/child-abc.txt")));
}

#[test]
fn exact_token_supports_or_operator() {
    let entries = vec![
        PathBuf::from("/tmp/src/foo.rs"),
        PathBuf::from("/tmp/src/bar.rs"),
        PathBuf::from("/tmp/src/x-y-z.rs"),
    ];
    let out = search_entries("'foo|bar", &entries, 10, false, true);
    let names: Vec<&str> = out
        .iter()
        .filter_map(|(p, _)| p.file_name().and_then(|s| s.to_str()))
        .collect();
    assert!(names.contains(&"foo.rs"));
    assert!(names.contains(&"bar.rs"));
    assert!(!names.contains(&"x-y-z.rs"));
}

#[test]
fn include_or_supports_mixed_exact_on_right_side() {
    let entries = vec![
        PathBuf::from("/tmp/src/abc.rs"),
        PathBuf::from("/tmp/src/a-b-c.rs"),
        PathBuf::from("/tmp/src/xyz.rs"),
        PathBuf::from("/tmp/src/x-y-z.rs"),
    ];
    let out = search_entries("abc|'xyz", &entries, 10, false, true);
    let names: Vec<&str> = out
        .iter()
        .filter_map(|(p, _)| p.file_name().and_then(|s| s.to_str()))
        .collect();
    assert!(names.contains(&"abc.rs"));
    assert!(names.contains(&"a-b-c.rs"));
    assert!(names.contains(&"xyz.rs"));
    assert!(!names.contains(&"x-y-z.rs"));
}

#[test]
fn include_or_supports_exact_on_both_sides() {
    let entries = vec![
        PathBuf::from("/tmp/src/abc.rs"),
        PathBuf::from("/tmp/src/a-b-c.rs"),
        PathBuf::from("/tmp/src/xyz.rs"),
        PathBuf::from("/tmp/src/x-y-z.rs"),
    ];
    let out = search_entries("'abc|'xyz", &entries, 10, false, true);
    let names: Vec<&str> = out
        .iter()
        .filter_map(|(p, _)| p.file_name().and_then(|s| s.to_str()))
        .collect();
    assert!(names.contains(&"abc.rs"));
    assert!(!names.contains(&"a-b-c.rs"));
    assert!(names.contains(&"xyz.rs"));
    assert!(!names.contains(&"x-y-z.rs"));
}

#[test]
fn exact_token_supports_anchor_with_quote_first_order() {
    let entries = vec![
        PathBuf::from("/tmp/src/main.py"),
        PathBuf::from("/tmp/src/amain.py"),
    ];
    let out = search_entries("'^main", &entries, 10, false, true);
    assert_eq!(out.len(), 1);
    assert_eq!(
        out[0].0.file_name().and_then(|s| s.to_str()),
        Some("main.py")
    );
}

#[test]
fn exact_token_supports_anchor_with_caret_first_order() {
    let entries = vec![
        PathBuf::from("/tmp/src/main.py"),
        PathBuf::from("/tmp/src/amain.py"),
    ];
    let out = search_entries("^'main", &entries, 10, false, true);
    assert_eq!(out.len(), 1);
    assert_eq!(
        out[0].0.file_name().and_then(|s| s.to_str()),
        Some("main.py")
    );
}

#[test]
fn multi_term_query_prioritizes_exact_term_hits() {
    let entries = vec![
        PathBuf::from("/tmp/src/barbaz.txt"),
        PathBuf::from("/tmp/src/bxxaxxr-bxaxz.txt"),
    ];
    let out = search_entries("bar baz", &entries, 10, false, true);
    assert!(!out.is_empty());
    assert_eq!(
        out[0].0.file_name().and_then(|s| s.to_str()),
        Some("barbaz.txt")
    );
}

#[test]
fn multi_term_query_prefers_literal_hits_per_token_over_subsequence_only_hits() {
    let entries = vec![
        PathBuf::from("/tmp/src/abc-def.txt"),
        PathBuf::from("/tmp/src/a-b-c-d-e-f.txt"),
    ];
    let out = search_entries("abc def", &entries, 10, false, true);
    assert!(!out.is_empty());
    assert_eq!(
        out[0].0.file_name().and_then(|s| s.to_str()),
        Some("abc-def.txt")
    );
}

#[test]
fn regex_query_works_when_enabled() {
    let entries = vec![
        PathBuf::from("/tmp/src/main.py"),
        PathBuf::from("/tmp/src/module.rs"),
    ];
    let out = search_entries("ma.*py", &entries, 10, true, true);
    assert_eq!(out.len(), 1);
    assert_eq!(
        out[0].0.file_name().and_then(|s| s.to_str()),
        Some("main.py")
    );
}

#[test]
fn regex_mode_keeps_plain_token_fuzzy_matching() {
    let entries = vec![
        PathBuf::from("/tmp/src/a-b-c.txt"),
        PathBuf::from("/tmp/src/xyz.txt"),
    ];
    let out = search_entries("abc", &entries, 10, true, true);
    assert_eq!(out.len(), 1);
    assert_eq!(
        out[0].0.file_name().and_then(|s| s.to_str()),
        Some("a-b-c.txt")
    );
}

#[test]
fn regex_mode_keeps_plain_or_token_fuzzy_matching() {
    let entries = vec![
        PathBuf::from("/tmp/src/a-b-c.txt"),
        PathBuf::from("/tmp/src/f-o-o.txt"),
        PathBuf::from("/tmp/src/xyz.txt"),
    ];
    let out = search_entries("abc|foo", &entries, 10, true, true);
    assert_eq!(out.len(), 2);
    assert_eq!(
        out[0].0.file_name().and_then(|s| s.to_str()),
        Some("a-b-c.txt")
    );
    assert_eq!(
        out[1].0.file_name().and_then(|s| s.to_str()),
        Some("f-o-o.txt")
    );
}

#[test]
fn regex_mode_preserves_regex_only_token_behavior() {
    let entries = vec![
        PathBuf::from("/tmp/src/main.py"),
        PathBuf::from("/tmp/src/m-a-i-n-p-y.txt"),
    ];
    let out = search_entries("ma.*py", &entries, 10, true, true);
    assert_eq!(out.len(), 1);
    assert_eq!(
        out[0].0.file_name().and_then(|s| s.to_str()),
        Some("main.py")
    );
}

#[test]
#[cfg(target_os = "windows")]
fn relative_search_normalizes_extended_drive_prefixes() {
    let root = PathBuf::from(r"C:\Users\tester");
    let entries = vec![PathBuf::from(r"\\?\C:\Users\tester\abc\def.txt")];
    let out = search_entries_with_scope("abc def", &entries, 10, false, true, Some(&root), true);
    assert_eq!(out.len(), 1);
}

#[test]
#[cfg(target_os = "windows")]
fn relative_search_normalizes_extended_unc_prefixes() {
    let root = PathBuf::from(r"\\server\share");
    let entries = vec![PathBuf::from(r"\\?\UNC\server\share\abc\def.txt")];
    let out = search_entries_with_scope("abc def", &entries, 10, false, true, Some(&root), true);
    assert_eq!(out.len(), 1);
}

#[test]
fn invalid_regex_returns_error_in_try_api() {
    let entries = vec![PathBuf::from("/tmp/src/main.py")];
    let err = try_search_entries_with_scope("[*", &entries, 10, true, true, None, false)
        .expect_err("invalid regex should return error");
    assert!(err.contains("invalid regex"));
}

#[test]
fn relative_search_results_are_visible_in_relative_display_on_posix_paths() {
    let root = PathBuf::from("/tmp/workspace");
    let entries = vec![
        root.join("abc/def.txt"),
        root.join("misc/xyz.txt"),
        PathBuf::from("/var/tmp/abc-def-outside.txt"),
    ];

    let out = search_entries_with_scope("abc def", &entries, 10, false, true, Some(&root), true);
    assert_eq!(out.len(), 2);
    assert!(out
        .iter()
        .all(|(path, _)| has_visible_match(path, &root, "abc def", true, true)));
}

#[test]
fn absolute_search_results_are_visible_in_absolute_display_on_posix_paths() {
    let root = PathBuf::from("/tmp/workspace");
    let entries = vec![
        PathBuf::from("/opt/cache/abc/def.txt"),
        PathBuf::from("/opt/cache/misc/xyz.txt"),
    ];

    let out = search_entries_with_scope("abc def", &entries, 10, false, true, Some(&root), false);
    assert_eq!(out.len(), 1);
    assert!(has_visible_match(&out[0].0, &root, "abc def", false, true));
}

#[test]
fn anchors_in_non_regex_are_fuzzy_with_adjacent_constraints() {
    let entries = vec![
        PathBuf::from("/tmp/src/main.py"),
        PathBuf::from("/tmp/src/amain.py"),
    ];
    let out = search_entries("^main", &entries, 10, false, true);
    assert_eq!(out.len(), 1);
    assert!(out[0].0.to_string_lossy().contains("main.py"));
}

#[test]
fn end_anchor_uses_adjacent_character_constraint() {
    let entries = vec![
        PathBuf::from("/tmp/src/domain"),
        PathBuf::from("/tmp/src/main.py"),
    ];
    let out = search_entries("main$", &entries, 10, false, true);
    assert_eq!(out.len(), 1);
    assert!(out[0].0.to_string_lossy().contains("domain"));
}

#[test]
#[ignore = "perf measurement; run explicitly"]
fn perf_search_100k_candidates_reports_latency() {
    let entries: Vec<PathBuf> = (0..100_000)
        .map(|i| PathBuf::from(format!("/tmp/src/module_{i:06}.rs")))
        .collect();
    let start = Instant::now();
    let out = search_entries("module_123", &entries, 100, false, true);
    let elapsed = start.elapsed();
    eprintln!("search_100k_elapsed_ms={}", elapsed.as_millis());
    assert!(!out.is_empty());
    assert!(elapsed < Duration::from_secs(2));
}

#[test]
#[ignore = "TC-156 release-mode perf regression; run explicitly"]
fn perf_search_100k_cold_warm_query_shapes() {
    const SAMPLE_COUNT: usize = 5;
    const HARD_SAMPLE_LIMIT: Duration = Duration::from_millis(250);
    let root = PathBuf::from("/tmp");
    let entries = Arc::new(
        (0..100_000)
            .map(|index| {
                Entry::file(PathBuf::from(format!(
                    "/tmp/src/group_{:02}/module_{index:06}.rs",
                    index % 32
                )))
            })
            .collect::<Vec<_>>(),
    );

    let mut warmup_cache = SearchPrefixCache::default();
    let _ = rank_search_results(
        &entries,
        "module_000",
        &root,
        100,
        false,
        true,
        true,
        &mut warmup_cache,
        SearchResultSortMode::Score,
        SearchResultSortScope::ShownResults,
    );

    let shapes = [
        ("selective-fuzzy", "module_099", false),
        ("dense-fuzzy", "module", false),
        ("multi-and", "src module_099", false),
        ("exact", "'module_099", false),
        ("inverse", "module_099 !vendor", false),
        ("anchor", "^module_099", false),
        ("or", "module_099|module_098", false),
        ("field-and", "dir:group_01 ext:rs", false),
        ("regex", r"module_09[0-9]{4}", true),
    ];

    for (label, query, use_regex) in shapes {
        let compile_started = Instant::now();
        let compiled = crate::query::CompiledQuery::compile(
            query,
            crate::query::QueryOptions {
                use_regex,
                ignore_case: true,
            },
        )
        .expect("compile perf query");
        let compile_micros = compile_started.elapsed().as_micros();
        std::hint::black_box(compiled);
        let mut samples = Vec::with_capacity(SAMPLE_COUNT);
        let mut last = None;
        for _ in 0..SAMPLE_COUNT {
            let mut cache = SearchPrefixCache::default();
            let started = Instant::now();
            let (result, error) = rank_search_results(
                &entries,
                query,
                &root,
                100,
                use_regex,
                true,
                true,
                &mut cache,
                SearchResultSortMode::Score,
                SearchResultSortScope::ShownResults,
            );
            samples.push(started.elapsed());
            assert!(error.is_none(), "{label}: {error:?}");
            assert!(result.total_match_count > 0, "{label} must match");
            last = Some(result);
        }
        samples.sort_unstable();
        let median = samples[SAMPLE_COUNT / 2];
        let maximum = *samples.last().expect("sample");
        let result = last.expect("result");
        eprintln!(
            "tc_156 shape={label} candidates={} evaluated={} matches={} compile_us={} median_ms={} max_ms={}",
            entries.len(),
            result.evaluated_candidate_count,
            result.total_match_count,
            compile_micros,
            median.as_millis(),
            maximum.as_millis()
        );
        assert!(
            median < HARD_SAMPLE_LIMIT,
            "{label} median {:?} exceeded {:?}",
            median,
            HARD_SAMPLE_LIMIT
        );
        assert!(
            maximum < HARD_SAMPLE_LIMIT,
            "{label} maximum {:?} exceeded {:?}",
            maximum,
            HARD_SAMPLE_LIMIT
        );
    }

    let unknown_kind_entries = Arc::new(
        (0..100_000)
            .map(|index| {
                let extension = if index % 100 == 0 { "rs" } else { "txt" };
                Entry::unknown(PathBuf::from(format!(
                    "/tmp/unknown/group_{:02}/module_{index:06}.{extension}",
                    index % 32
                )))
            })
            .collect::<Vec<_>>(),
    );
    let mut unknown_samples = Vec::with_capacity(SAMPLE_COUNT);
    let mut unknown_result = None;
    for _ in 0..SAMPLE_COUNT {
        let mut cache = SearchPrefixCache::default();
        let started = Instant::now();
        let (result, error) = rank_search_results(
            &unknown_kind_entries,
            "ext:rs",
            &root,
            100,
            false,
            true,
            true,
            &mut cache,
            SearchResultSortMode::Score,
            SearchResultSortScope::ShownResults,
        );
        unknown_samples.push(started.elapsed());
        assert!(error.is_none());
        assert_eq!(result.total_match_count, 1_000);
        unknown_result = Some(result);
    }
    unknown_samples.sort_unstable();
    let unknown_median = unknown_samples[SAMPLE_COUNT / 2];
    let unknown_maximum = *unknown_samples.last().expect("unknown-kind sample");
    let unknown_result = unknown_result.expect("unknown-kind result");
    eprintln!(
        "tc_156 shape=unknown-kind-ext candidates={} evaluated={} matches={} median_ms={} max_ms={}",
        unknown_kind_entries.len(),
        unknown_result.evaluated_candidate_count,
        unknown_result.total_match_count,
        unknown_median.as_millis(),
        unknown_maximum.as_millis()
    );
    assert!(unknown_median < HARD_SAMPLE_LIMIT);
    assert!(unknown_maximum < HARD_SAMPLE_LIMIT);

    let mut cold_cache = SearchPrefixCache::default();
    let (cold, cold_error) = rank_search_results(
        &entries,
        "module_0999",
        &root,
        100,
        false,
        true,
        true,
        &mut cold_cache,
        SearchResultSortMode::Score,
        SearchResultSortScope::ShownResults,
    );
    assert!(cold_error.is_none());

    let mut warm_samples = Vec::with_capacity(SAMPLE_COUNT);
    let mut warm_result = None;
    for _ in 0..SAMPLE_COUNT {
        let mut cache = SearchPrefixCache::default();
        let (_, seed_error) = rank_search_results(
            &entries,
            "module_099",
            &root,
            100,
            false,
            true,
            true,
            &mut cache,
            SearchResultSortMode::Score,
            SearchResultSortScope::ShownResults,
        );
        assert!(seed_error.is_none());
        let started = Instant::now();
        let (warm, error) = rank_search_results(
            &entries,
            "module_0999",
            &root,
            100,
            false,
            true,
            true,
            &mut cache,
            SearchResultSortMode::Score,
            SearchResultSortScope::ShownResults,
        );
        warm_samples.push(started.elapsed());
        assert!(error.is_none());
        warm_result = Some(warm);
    }
    warm_samples.sort_unstable();
    let warm_median = warm_samples[SAMPLE_COUNT / 2];
    let warm_maximum = *warm_samples.last().expect("warm sample");
    let warm = warm_result.expect("warm result");
    eprintln!(
        "tc_156 shape=prefix-warm candidates={} evaluated={} matches={} median_ms={} max_ms={}",
        entries.len(),
        warm.evaluated_candidate_count,
        warm.total_match_count,
        warm_median.as_millis(),
        warm_maximum.as_millis()
    );
    assert_eq!(warm.results, cold.results);
    assert_eq!(warm.total_match_count, cold.total_match_count);
    assert!(warm.evaluated_candidate_count < cold.evaluated_candidate_count);
    assert!(warm_median < HARD_SAMPLE_LIMIT);
    assert!(warm_maximum < HARD_SAMPLE_LIMIT);

    const SCALE_CANDIDATES: usize = 1_000_000;
    const SCALE_SAMPLE_COUNT: usize = 7;
    let before_fixture_rss = physical_rss_bytes();
    log_tc_185_rss("before_fixture", before_fixture_rss);

    let (after_fixture_rss, peak_search_rss) = {
        let scale_entries = Arc::new(
            (0..SCALE_CANDIDATES)
                .map(|index| {
                    Entry::file(PathBuf::from(format!(
                        "/tmp/scale/group_{:03}/module_{index:07}.rs",
                        index % 256
                    )))
                })
                .collect::<Vec<_>>(),
        );
        assert_eq!(scale_entries.len(), SCALE_CANDIDATES);
        let after_fixture_rss = physical_rss_bytes();
        log_tc_185_rss("after_fixture", after_fixture_rss);
        let peak_sampler = RssPeakSampler::start(after_fixture_rss);

        for (label, query) in [
            ("selective-fuzzy", "module_9999"),
            ("dense-fuzzy", "module"),
        ] {
            let mut samples = Vec::with_capacity(SCALE_SAMPLE_COUNT);
            let mut baseline = None;
            for _ in 0..SCALE_SAMPLE_COUNT {
                let mut cache = SearchPrefixCache::default();
                let started = Instant::now();
                let (result, error) = rank_search_results(
                    &scale_entries,
                    query,
                    &root,
                    100,
                    false,
                    true,
                    true,
                    &mut cache,
                    SearchResultSortMode::Score,
                    SearchResultSortScope::ShownResults,
                );
                samples.push(started.elapsed());
                assert!(error.is_none(), "tc_185 {label}: {error:?}");
                assert!(result.total_match_count > 0, "tc_185 {label} must match");
                let signature = (
                    result.total_match_count,
                    result.evaluated_candidate_count,
                    result.results.clone(),
                );
                if let Some(expected) = &baseline {
                    assert_eq!(
                        &signature, expected,
                        "tc_185 {label} count/order changed across repetitions"
                    );
                } else {
                    baseline = Some(signature);
                }
            }

            let baseline = baseline.expect("tc_185 baseline result");
            let p50 = nearest_rank_percentile(&samples, 50);
            let p95 = nearest_rank_percentile(&samples, 95);
            let p99 = nearest_rank_percentile(&samples, 99);
            eprintln!(
                "tc_185 shape={label} candidates={} samples={} evaluated={} matches={} results={} p50_ms={} p95_ms={} p99_ms={}",
                scale_entries.len(),
                samples.len(),
                baseline.1,
                baseline.0,
                baseline.2.len(),
                p50.as_millis(),
                p95.as_millis(),
                p99.as_millis()
            );
        }

        (after_fixture_rss, peak_sampler.finish())
    };
    log_tc_185_rss("peak_search", peak_search_rss);
    thread::sleep(Duration::from_secs(1));
    let after_drop_quiescence_rss = physical_rss_bytes();
    log_tc_185_rss("after_drop_quiescence", after_drop_quiescence_rss);
    std::hint::black_box((
        before_fixture_rss,
        after_fixture_rss,
        peak_search_rss,
        after_drop_quiescence_rss,
    ));
}

#[test]
fn exclusion_uses_visible_relative_path_when_scope_is_relative() {
    let root = PathBuf::from("/home/alice/work");
    let entries = vec![PathBuf::from("/home/alice/work/docs/readme.md")];

    let out = search_entries_with_scope("!ali", &entries, 10, false, true, Some(&root), true);

    assert_eq!(out.len(), 1);
}

#[test]
fn include_token_pipe_acts_as_or() {
    let entries = vec![
        PathBuf::from("/tmp/src/foo.txt"),
        PathBuf::from("/tmp/src/bar.txt"),
        PathBuf::from("/tmp/src/baz.txt"),
    ];

    let out = search_entries("abc|foo|bar", &entries, 10, false, true);
    let names: Vec<&str> = out
        .iter()
        .filter_map(|(p, _)| p.file_name().and_then(|s| s.to_str()))
        .collect();
    assert!(names.contains(&"foo.txt"));
    assert!(names.contains(&"bar.txt"));
    assert!(!names.contains(&"baz.txt"));
}

#[test]
fn include_token_pipe_still_combines_with_and_tokens() {
    let entries = vec![
        PathBuf::from("/tmp/src/foo.txt"),
        PathBuf::from("/tmp/docs/foo.txt"),
        PathBuf::from("/tmp/src/bar.txt"),
    ];

    let out = search_entries("src foo|bar", &entries, 10, false, true);
    let names: Vec<&str> = out
        .iter()
        .filter_map(|(p, _)| p.file_name().and_then(|s| s.to_str()))
        .collect();
    assert!(names.contains(&"foo.txt"));
    assert!(names.contains(&"bar.txt"));
    assert_eq!(out.len(), 2);
}

use super::*;
use crate::ui_model::has_visible_match;
use std::time::{Duration, Instant};

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

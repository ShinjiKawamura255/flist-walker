use crate::query::{
    include_alternatives, parse_include_alternative, parse_query, split_anchor, QuerySpec,
};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use regex::{Regex, RegexBuilder};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedScore {
    pub index: usize,
    pub score: f64,
}

fn is_subsequence(query: &str, text: &str) -> bool {
    let mut qi = 0usize;
    let q: Vec<char> = query.chars().collect();
    for ch in text.chars() {
        if qi < q.len() && ch == q[qi] {
            qi += 1;
        }
    }
    qi == q.len()
}

fn is_fuzzy_match(query: &str, text: &str) -> bool {
    let q = query.to_ascii_lowercase();
    let t = text.to_ascii_lowercase();
    t.contains(&q) || is_subsequence(&q, &t)
}

fn matches_anchored_literal(term: &str, text: &str) -> bool {
    let (anchored_start, anchored_end, core) = split_anchor(term);
    if core.is_empty() {
        return false;
    }

    if anchored_start && anchored_end {
        text == core
    } else if anchored_start {
        text.starts_with(core)
    } else if anchored_end {
        text.ends_with(core)
    } else {
        text.contains(core)
    }
}

fn matches_exact_term(term: &str, name: &str, full: &str) -> bool {
    let t = term.to_ascii_lowercase();
    include_alternatives(&t)
        .into_iter()
        .filter_map(|candidate| parse_include_alternative(candidate).map(|(_, c)| c))
        .any(|candidate| {
            matches_anchored_literal(&candidate, name) || matches_anchored_literal(&candidate, full)
        })
}

fn matches_exclusion_term(term: &str, name: &str, full: &str) -> bool {
    let t = term.to_ascii_lowercase();
    include_alternatives(&t)
        .into_iter()
        .filter_map(|candidate| parse_include_alternative(candidate).map(|(_, c)| c))
        .any(|candidate| {
            matches_anchored_literal(&candidate, name) || matches_anchored_literal(&candidate, full)
        })
}

fn matches_include_literal_term(term: &str, name: &str, full: &str) -> bool {
    let t = term.to_ascii_lowercase();
    let (anchored_start, anchored_end, core) = split_anchor(&t);
    if core.is_empty() {
        return false;
    }

    if anchored_start {
        let c = core.chars().next().unwrap_or_default();
        if !(name.starts_with(c) || full.starts_with(c)) {
            return false;
        }
    }
    if anchored_end {
        let c = core.chars().last().unwrap_or_default();
        if !(name.ends_with(c) || full.ends_with(c)) {
            return false;
        }
    }

    is_fuzzy_match(core, name) || is_fuzzy_match(core, full)
}

fn matches_include_term(term: &str, name: &str, full: &str, regex: Option<&Regex>) -> bool {
    if let Some(re) = regex {
        return re.is_match(name) || re.is_match(full);
    }

    include_alternatives(term)
        .into_iter()
        .filter_map(parse_include_alternative)
        .any(|(exact, candidate)| {
            if exact {
                matches_exact_term(&candidate, name, full)
            } else {
                matches_include_literal_term(&candidate, name, full)
            }
        })
}

fn searchable_full(path: &Path, root: Option<&Path>, prefer_relative: bool) -> String {
    if prefer_relative {
        if let Some(root) = root {
            if let Ok(rel) = path.strip_prefix(root) {
                return rel.to_string_lossy().to_ascii_lowercase();
            }
        }
    }
    path.to_string_lossy().to_ascii_lowercase()
}

fn matches_spec(
    spec: &QuerySpec,
    path: &Path,
    include_regexes: Option<&[Regex]>,
    root: Option<&Path>,
    prefer_relative: bool,
) -> bool {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let full = searchable_full(path, root, prefer_relative);

    for term in &spec.exclude_terms {
        if matches_exclusion_term(term, &name, &full) {
            return false;
        }
    }

    for term in &spec.exact_terms {
        if !matches_exact_term(term, &name, &full) {
            return false;
        }
    }

    for (idx, term) in spec.include_terms.iter().enumerate() {
        let regex = include_regexes.and_then(|items| items.get(idx));
        if !matches_include_term(term, &name, &full, regex) {
            return false;
        }
    }

    true
}

fn fallback_score(query: &str, text: &str) -> f64 {
    if query.is_empty() {
        return 0.0;
    }
    let q = query.to_ascii_lowercase();
    let t = text.to_ascii_lowercase();
    let mut score = 0.0;
    if t.contains(&q) {
        score += 25.0;
    }
    if t.starts_with(&q) {
        score += 30.0;
    }
    score + (q.len().min(t.len()) as f64)
}

pub fn search_entries(
    query: &str,
    entries: &[PathBuf],
    limit: usize,
    use_regex: bool,
) -> Vec<(PathBuf, f64)> {
    search_entries_with_scope(query, entries, limit, use_regex, None, false)
}

pub fn try_search_entries_with_scope(
    query: &str,
    entries: &[PathBuf],
    limit: usize,
    use_regex: bool,
    root: Option<&Path>,
    prefer_relative: bool,
) -> Result<Vec<(PathBuf, f64)>, String> {
    let scored = try_search_entries_indexed_with_scope(
        query,
        entries,
        use_regex,
        root,
        prefer_relative,
        None,
    )?;
    Ok(materialize_scored_entries(entries, scored, limit))
}

pub fn try_search_entries_indexed_with_scope(
    query: &str,
    entries: &[PathBuf],
    use_regex: bool,
    root: Option<&Path>,
    prefer_relative: bool,
    candidate_indices: Option<&[usize]>,
) -> Result<Vec<IndexedScore>, String> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let spec = parse_query(query);
    let include_regexes = if use_regex {
        let mut compiled = Vec::with_capacity(spec.include_terms.len());
        for term in &spec.include_terms {
            // Compile once per query term and reuse for all candidates.
            let re = RegexBuilder::new(term)
                .case_insensitive(true)
                .build()
                .map_err(|err| format!("invalid regex '{term}': {err}"))?;
            compiled.push(re);
        }
        Some(compiled)
    } else {
        None
    };

    let mut q = spec
        .include_terms
        .iter()
        .flat_map(|term| include_alternatives(term))
        .filter_map(|term| {
            let (_, candidate) = parse_include_alternative(term)?;
            let (_, _, core) = split_anchor(&candidate);
            (!core.is_empty()).then_some(core.to_string())
        })
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    if q.is_empty() {
        if let Some(first_exact) = spec.exact_terms.first() {
            q = first_exact.to_ascii_lowercase();
        }
    }

    let matcher = SkimMatcherV2::default();
    let mut scored = Vec::new();
    let indices: Vec<usize> = candidate_indices
        .map(|items| items.to_vec())
        .unwrap_or_else(|| (0..entries.len()).collect());
    scored.reserve(indices.len());

    for index in indices {
        let Some(path) = entries.get(index) else {
            continue;
        };
        if !matches_spec(
            &spec,
            path,
            include_regexes.as_deref(),
            root,
            prefer_relative,
        ) {
            continue;
        }
        let full = searchable_full(&path, root, prefer_relative);
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let full_lower = full;

        let mut score = if !q.is_empty() {
            matcher
                .fuzzy_match(&full_lower, &q)
                .map(|s| s as f64)
                .unwrap_or_else(|| fallback_score(&q, &full_lower))
        } else {
            0.0
        };

        if !q.is_empty() && name == q {
            score += 1000.0;
        } else if !q.is_empty() && full_lower == q {
            score += 900.0;
        }

        for term in &spec.exact_terms {
            if matches_exact_term(term, &name, &full_lower) {
                score += 800.0;
            }
        }
        for term in &spec.include_terms {
            for candidate in include_alternatives(term) {
                let Some((_, parsed)) = parse_include_alternative(candidate) else {
                    continue;
                };
                let (_, _, core) = split_anchor(&parsed);
                if core.is_empty() {
                    continue;
                }
                if matches_exact_term(core, &name, &full_lower) {
                    score += 300.0;
                    if name == core {
                        score += 300.0;
                    }
                }
            }
        }

        scored.push(IndexedScore { index, score });
    }

    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(scored)
}

fn materialize_scored_entries(
    entries: &[PathBuf],
    scored: Vec<IndexedScore>,
    limit: usize,
) -> Vec<(PathBuf, f64)> {
    if limit == 0 || scored.is_empty() {
        return Vec::new();
    }
    scored
        .into_iter()
        .take(limit)
        .filter_map(|item| {
            entries
                .get(item.index)
                .cloned()
                .map(|path| (path, item.score))
        })
        .collect()
}

pub fn search_entries_with_scope(
    query: &str,
    entries: &[PathBuf],
    limit: usize,
    use_regex: bool,
    root: Option<&Path>,
    prefer_relative: bool,
) -> Vec<(PathBuf, f64)> {
    try_search_entries_with_scope(query, entries, limit, use_regex, root, prefer_relative)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn orders_by_score_and_limit() {
        let entries = vec![
            PathBuf::from("/tmp/src/main.py"),
            PathBuf::from("/tmp/src/README.md"),
            PathBuf::from("/tmp/docs/design.md"),
        ];

        let out = search_entries("main", &entries, 2, false);
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
    fn indexed_search_with_candidates_matches_full_scan() {
        let entries = vec![
            PathBuf::from("/tmp/src/main.rs"),
            PathBuf::from("/tmp/src/mod.rs"),
            PathBuf::from("/tmp/src/domain.rs"),
            PathBuf::from("/tmp/src/memory.rs"),
        ];

        let base = try_search_entries_indexed_with_scope("ma", &entries, false, None, false, None)
            .expect("base query");
        let base_indices = base.iter().map(|x| x.index).collect::<Vec<_>>();
        let narrowed_full =
            try_search_entries_indexed_with_scope("mai", &entries, false, None, false, None)
                .expect("full scan query");
        let narrowed_from_candidates = try_search_entries_indexed_with_scope(
            "mai",
            &entries,
            false,
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
        let out = search_entries("", &entries, 10, false);
        assert!(out.is_empty());
    }

    #[test]
    fn prioritizes_exact_filename_match() {
        let entries = vec![
            PathBuf::from("/tmp/src/main.py"),
            PathBuf::from("/tmp/src/main.py.bak"),
            PathBuf::from("/tmp/src/domain_main.py"),
        ];
        let out = search_entries("main.py", &entries, 10, false);
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
        let out = search_entries("zzz", &entries, 10, false);
        assert!(out.is_empty());
    }

    #[test]
    fn exact_and_exclusion_tokens_work() {
        let entries = vec![
            PathBuf::from("/tmp/src/main.py"),
            PathBuf::from("/tmp/src/readme.md"),
        ];

        let exact = search_entries("'main", &entries, 10, false);
        assert_eq!(exact.len(), 1);

        let excluded = search_entries("!readme", &entries, 10, false);
        assert_eq!(excluded.len(), 1);
    }

    #[test]
    fn lone_operator_tokens_are_ignored() {
        let entries = vec![
            PathBuf::from("/tmp/src/main.py"),
            PathBuf::from("/tmp/src/readme.md"),
        ];

        let out_bang = search_entries("!", &entries, 10, false);
        assert_eq!(out_bang.len(), 2);

        let out_quote = search_entries("'", &entries, 10, false);
        assert_eq!(out_quote.len(), 2);

        let out_mixed = search_entries("main !", &entries, 10, false);
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
        let out = search_entries("'main", &entries, 10, false);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn exact_token_supports_or_operator() {
        let entries = vec![
            PathBuf::from("/tmp/src/foo.rs"),
            PathBuf::from("/tmp/src/bar.rs"),
            PathBuf::from("/tmp/src/x-y-z.rs"),
        ];
        let out = search_entries("'foo|bar", &entries, 10, false);
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
        let out = search_entries("abc|'xyz", &entries, 10, false);
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
        let out = search_entries("'abc|'xyz", &entries, 10, false);
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
        let out = search_entries("'^main", &entries, 10, false);
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
        let out = search_entries("^'main", &entries, 10, false);
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
        let out = search_entries("bar baz", &entries, 10, false);
        assert!(!out.is_empty());
        assert_eq!(
            out[0].0.file_name().and_then(|s| s.to_str()),
            Some("barbaz.txt")
        );
    }

    #[test]
    fn regex_query_works_when_enabled() {
        let entries = vec![
            PathBuf::from("/tmp/src/main.py"),
            PathBuf::from("/tmp/src/module.rs"),
        ];
        let out = search_entries("ma.*py", &entries, 10, true);
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].0.file_name().and_then(|s| s.to_str()),
            Some("main.py")
        );
    }

    #[test]
    fn invalid_regex_returns_error_in_try_api() {
        let entries = vec![PathBuf::from("/tmp/src/main.py")];
        let err = try_search_entries_with_scope("[*", &entries, 10, true, None, false)
            .expect_err("invalid regex should return error");
        assert!(err.contains("invalid regex"));
    }

    #[test]
    fn anchors_in_non_regex_are_fuzzy_with_adjacent_constraints() {
        let entries = vec![
            PathBuf::from("/tmp/src/main.py"),
            PathBuf::from("/tmp/src/amain.py"),
        ];
        let out = search_entries("^main", &entries, 10, false);
        assert_eq!(out.len(), 1);
        assert!(out[0].0.to_string_lossy().contains("main.py"));
    }

    #[test]
    fn end_anchor_uses_adjacent_character_constraint() {
        let entries = vec![
            PathBuf::from("/tmp/src/domain"),
            PathBuf::from("/tmp/src/main.py"),
        ];
        let out = search_entries("main$", &entries, 10, false);
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
        let out = search_entries("module_123", &entries, 100, false);
        let elapsed = start.elapsed();
        eprintln!("search_100k_elapsed_ms={}", elapsed.as_millis());
        assert!(!out.is_empty());
        // Keep a generous guard as a smoke check; target remains documented as 100ms SHOULD.
        assert!(elapsed < Duration::from_secs(2));
    }

    #[test]
    fn exclusion_uses_visible_relative_path_when_scope_is_relative() {
        let root = PathBuf::from("/home/alice/work");
        let entries = vec![PathBuf::from("/home/alice/work/docs/readme.md")];

        let out = search_entries_with_scope("!ali", &entries, 10, false, Some(&root), true);

        assert_eq!(out.len(), 1);
    }

    #[test]
    fn include_token_pipe_acts_as_or() {
        let entries = vec![
            PathBuf::from("/tmp/src/foo.txt"),
            PathBuf::from("/tmp/src/bar.txt"),
            PathBuf::from("/tmp/src/baz.txt"),
        ];

        let out = search_entries("abc|foo|bar", &entries, 10, false);
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

        let out = search_entries("src foo|bar", &entries, 10, false);
        let names: Vec<&str> = out
            .iter()
            .filter_map(|(p, _)| p.file_name().and_then(|s| s.to_str()))
            .collect();
        assert!(names.contains(&"foo.txt"));
        assert!(names.contains(&"bar.txt"));
        assert_eq!(out.len(), 2);
    }
}

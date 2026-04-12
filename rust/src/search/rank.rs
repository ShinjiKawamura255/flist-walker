use super::{IndexedScore, SearchCandidateScore};
use crate::entry::Entry;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};

fn compare_scored_candidates(a: &SearchCandidateScore, b: &SearchCandidateScore) -> Ordering {
    b.score
        .partial_cmp(&a.score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| a.ordinal.cmp(&b.ordinal))
}

pub(crate) fn sort_scored_matches(scored: &mut [SearchCandidateScore]) {
    scored.sort_unstable_by(compare_scored_candidates);
}

pub(crate) fn top_ranked_scores(
    mut scored: Vec<SearchCandidateScore>,
    limit: usize,
) -> Vec<IndexedScore> {
    if limit == 0 || scored.is_empty() {
        return Vec::new();
    }

    if scored.len() > limit {
        let keep = limit - 1;
        scored.select_nth_unstable_by(keep, compare_scored_candidates);
        scored.truncate(limit);
    }
    sort_scored_matches(&mut scored);
    scored
        .into_iter()
        .map(|item| IndexedScore {
            index: item.index,
            score: item.score,
        })
        .collect()
}

pub(crate) fn filter_search_results(
    results: Vec<(PathBuf, f64)>,
    root: &Path,
    query: &str,
    prefer_relative: bool,
    use_regex: bool,
    ignore_case: bool,
) -> Vec<(PathBuf, f64)> {
    if use_regex {
        return results;
    }

    results
        .into_iter()
        .filter(|(path, _)| {
            crate::query::has_visible_match(path, root, query, prefer_relative, ignore_case)
        })
        .collect()
}

pub(super) fn scored_indices_to_paths(
    entries: &[Entry],
    scored: &[IndexedScore],
    limit: usize,
) -> Vec<(PathBuf, f64)> {
    if limit == 0 || scored.is_empty() {
        return Vec::new();
    }
    scored
        .iter()
        .take(limit)
        .filter_map(|item| {
            entries
                .get(item.index)
                .map(|entry| (entry.path.clone(), item.score))
        })
        .collect()
}

pub(super) fn materialize_scored_entries(
    entries: &[&Path],
    scored: Vec<IndexedScore>,
) -> Vec<(PathBuf, f64)> {
    scored
        .into_iter()
        .filter_map(|item| {
            entries
                .get(item.index)
                .map(|path| (path.to_path_buf(), item.score))
        })
        .collect()
}

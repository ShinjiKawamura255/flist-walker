mod cache;
mod config;
mod execute;
mod match_eval;
mod rank;

use crate::entry::Entry;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, warn};

pub(crate) use cache::{SearchEntriesSnapshotKey, SearchPrefixCache};
use config::{resolve_execution_mode, SearchExecutionMode};
use execute::{
    collect_entries_parallel, collect_entries_sequential, collect_parallel, collect_sequential,
};
use match_eval::{compile_query, SearchContext};
pub(crate) use rank::filter_search_results;
use rank::{
    materialize_scored_entries, scored_indices_to_paths, sort_scored_matches, top_ranked_scores,
};

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedScore {
    pub index: usize,
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SearchCandidateScore {
    pub(crate) index: usize,
    pub(crate) score: f64,
    ordinal: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct SearchScoredMatches {
    pub(crate) scored: Vec<SearchCandidateScore>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn rank_search_results(
    entries: &Arc<Vec<Entry>>,
    query: &str,
    root: &Path,
    limit: usize,
    use_regex: bool,
    ignore_case: bool,
    prefer_relative: bool,
    prefix_cache: &mut SearchPrefixCache,
) -> (Vec<(PathBuf, f64)>, Option<String>) {
    let query_trimmed = query.trim().to_string();
    let snapshot = SearchEntriesSnapshotKey::from_entries(entries);
    let cached_candidates = if use_regex {
        None
    } else {
        prefix_cache.lookup_candidates(snapshot, &query_trimmed)
    };
    let scored_matches = match try_collect_entry_matches(
        query,
        entries,
        use_regex,
        ignore_case,
        Some(root),
        prefer_relative,
        cached_candidates.as_ref().map(|items| items.as_slice()),
    ) {
        Ok(scored_matches) => scored_matches,
        Err(err) => return (Vec::new(), Some(err)),
    };
    if SearchPrefixCache::is_cacheable_query(&query_trimmed)
        && scored_matches.scored.len() <= SearchPrefixCache::MAX_MATCHED_INDICES
    {
        let mut ranked = scored_matches.scored.clone();
        sort_scored_matches(&mut ranked);
        let matched_indices = ranked.iter().map(|item| item.index).collect();
        prefix_cache.maybe_store(snapshot, &query_trimmed, matched_indices);
    }
    let ranked = top_ranked_scores(scored_matches.scored, limit);
    let raw_results = scored_indices_to_paths(entries, &ranked, limit);
    (
        filter_search_results(
            raw_results,
            root,
            query,
            prefer_relative,
            use_regex,
            ignore_case,
        ),
        None,
    )
}

fn try_collect_entry_matches(
    query: &str,
    entries: &[Entry],
    use_regex: bool,
    ignore_case: bool,
    root: Option<&Path>,
    prefer_relative: bool,
    candidate_indices: Option<&[usize]>,
) -> Result<SearchScoredMatches, String> {
    try_collect_entry_matches_with_mode(
        query,
        entries,
        SearchCollectOptions {
            use_regex,
            ignore_case,
            root,
            prefer_relative,
            candidate_indices,
            mode: SearchExecutionMode::Auto,
        },
    )
}

pub(crate) fn try_collect_search_matches(
    query: &str,
    entries: &[&Path],
    use_regex: bool,
    ignore_case: bool,
    root: Option<&Path>,
    prefer_relative: bool,
    candidate_indices: Option<&[usize]>,
) -> Result<SearchScoredMatches, String> {
    try_collect_search_matches_with_mode(
        query,
        entries,
        SearchCollectOptions {
            use_regex,
            ignore_case,
            root,
            prefer_relative,
            candidate_indices,
            mode: SearchExecutionMode::Auto,
        },
    )
}

fn try_collect_entry_matches_with_mode(
    query: &str,
    entries: &[Entry],
    options: SearchCollectOptions<'_>,
) -> Result<SearchScoredMatches, String> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(SearchScoredMatches::default());
    }

    let compiled = compile_query(query, options.use_regex, options.ignore_case)?;
    let ctx = SearchContext {
        root: options.root,
        prefer_relative: options.prefer_relative,
        ignore_case: options.ignore_case,
    };
    let candidate_count = options
        .candidate_indices
        .map_or(entries.len(), |items| items.len());
    let execution = resolve_execution_mode(options.mode, candidate_count);
    Ok(match execution {
        SearchExecutionMode::Sequential => {
            collect_entries_sequential(entries, &compiled, ctx, options.candidate_indices)
        }
        SearchExecutionMode::Parallel => {
            collect_entries_parallel(entries, &compiled, ctx, options.candidate_indices)
        }
        SearchExecutionMode::Auto => unreachable!(),
    })
}

#[derive(Clone, Copy)]
struct SearchCollectOptions<'a> {
    use_regex: bool,
    ignore_case: bool,
    root: Option<&'a Path>,
    prefer_relative: bool,
    candidate_indices: Option<&'a [usize]>,
    mode: SearchExecutionMode,
}

fn try_collect_search_matches_with_mode(
    query: &str,
    entries: &[&Path],
    options: SearchCollectOptions<'_>,
) -> Result<SearchScoredMatches, String> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(SearchScoredMatches::default());
    }

    let compiled = compile_query(query, options.use_regex, options.ignore_case)?;
    let ctx = SearchContext {
        root: options.root,
        prefer_relative: options.prefer_relative,
        ignore_case: options.ignore_case,
    };
    let candidate_count = options
        .candidate_indices
        .map_or(entries.len(), |items| items.len());
    let execution = resolve_execution_mode(options.mode, candidate_count);
    Ok(match execution {
        SearchExecutionMode::Sequential => {
            collect_sequential(entries, &compiled, ctx, options.candidate_indices)
        }
        SearchExecutionMode::Parallel => {
            collect_parallel(entries, &compiled, ctx, options.candidate_indices)
        }
        SearchExecutionMode::Auto => unreachable!(),
    })
}

pub fn search_entries(
    query: &str,
    entries: &[PathBuf],
    limit: usize,
    use_regex: bool,
    ignore_case: bool,
) -> Vec<(PathBuf, f64)> {
    search_entries_with_scope(query, entries, limit, use_regex, ignore_case, None, false)
}

pub fn try_search_entries_with_scope(
    query: &str,
    entries: &[PathBuf],
    limit: usize,
    use_regex: bool,
    ignore_case: bool,
    root: Option<&Path>,
    prefer_relative: bool,
) -> Result<Vec<(PathBuf, f64)>, String> {
    let started_at = Instant::now();
    let path_refs = entries.iter().map(PathBuf::as_path).collect::<Vec<_>>();
    let scored = try_collect_search_matches(
        query,
        &path_refs,
        use_regex,
        ignore_case,
        root,
        prefer_relative,
        None,
    )?;
    let results = materialize_scored_entries(&path_refs, top_ranked_scores(scored.scored, limit));
    let elapsed_ms = started_at.elapsed().as_millis();
    debug!(
        query,
        entry_count = entries.len(),
        limit,
        use_regex,
        ignore_case,
        prefer_relative,
        elapsed_ms,
        "search completed"
    );
    if elapsed_ms >= 100 {
        warn!(
            query,
            entry_count = entries.len(),
            limit,
            elapsed_ms,
            "search latency exceeded 100ms target"
        );
    }
    Ok(results)
}

pub fn try_search_entries_indexed_with_scope(
    query: &str,
    entries: &[PathBuf],
    use_regex: bool,
    ignore_case: bool,
    root: Option<&Path>,
    prefer_relative: bool,
    candidate_indices: Option<&[usize]>,
) -> Result<Vec<IndexedScore>, String> {
    let path_refs = entries.iter().map(PathBuf::as_path).collect::<Vec<_>>();
    let mut scored = try_collect_search_matches(
        query,
        &path_refs,
        use_regex,
        ignore_case,
        root,
        prefer_relative,
        candidate_indices,
    )?
    .scored;
    sort_scored_matches(&mut scored);
    Ok(scored
        .into_iter()
        .map(|item| IndexedScore {
            index: item.index,
            score: item.score,
        })
        .collect())
}

pub fn search_entries_with_scope(
    query: &str,
    entries: &[PathBuf],
    limit: usize,
    use_regex: bool,
    ignore_case: bool,
    root: Option<&Path>,
    prefer_relative: bool,
) -> Vec<(PathBuf, f64)> {
    try_search_entries_with_scope(
        query,
        entries,
        limit,
        use_regex,
        ignore_case,
        root,
        prefer_relative,
    )
    .unwrap_or_default()
}

#[cfg(test)]
mod tests;

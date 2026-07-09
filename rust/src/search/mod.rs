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
#[cfg(test)]
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

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct SearchResultSet {
    pub(crate) results: Vec<(PathBuf, f64)>,
    pub(crate) total_match_count: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SearchResultSortMode {
    #[default]
    Score,
    NameAsc,
    NameDesc,
    ModifiedDesc,
    ModifiedAsc,
    CreatedDesc,
    CreatedAsc,
    SizeDesc,
    SizeAsc,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SearchResultSortScope {
    #[default]
    ShownResults,
    AllMatches,
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
    sort_mode: SearchResultSortMode,
    sort_scope: SearchResultSortScope,
) -> (SearchResultSet, Option<String>) {
    let query_trimmed = query.trim().to_string();
    let snapshot = SearchEntriesSnapshotKey::from_entries(entries);
    let cached_candidates = if use_regex {
        None
    } else {
        prefix_cache.lookup_candidates(snapshot, &query_trimmed)
    };
    let scored_matches = if query_trimmed.is_empty() {
        SearchScoredMatches {
            scored: entries
                .iter()
                .enumerate()
                .map(|(index, _)| SearchCandidateScore {
                    index,
                    score: 0.0,
                    ordinal: index,
                })
                .collect(),
        }
    } else {
        match try_collect_entry_matches(
            query,
            entries,
            use_regex,
            ignore_case,
            Some(root),
            prefer_relative,
            cached_candidates.as_ref().map(|items| items.as_slice()),
        ) {
            Ok(scored_matches) => scored_matches,
            Err(err) => return (SearchResultSet::default(), Some(err)),
        }
    };
    let mut scored_matches = scored_matches;
    if !use_regex {
        scored_matches.scored.retain(|item| {
            entries.get(item.index).is_some_and(|entry| {
                crate::query::has_visible_match(
                    entry.path(),
                    root,
                    query,
                    prefer_relative,
                    ignore_case,
                )
            })
        });
    }
    let total_match_count = scored_matches.scored.len();
    if SearchPrefixCache::is_cacheable_query(&query_trimmed)
        && scored_matches.scored.len() <= SearchPrefixCache::MAX_MATCHED_INDICES
    {
        let mut ranked = scored_matches.scored.clone();
        sort_scored_matches(&mut ranked);
        let matched_indices = ranked.iter().map(|item| item.index).collect();
        prefix_cache.maybe_store(snapshot, &query_trimmed, matched_indices);
    }
    let ranked = match (sort_scope, sort_mode) {
        (SearchResultSortScope::AllMatches, SearchResultSortMode::NameAsc)
        | (SearchResultSortScope::AllMatches, SearchResultSortMode::NameDesc) => {
            top_name_sorted_scores(entries, scored_matches.scored, limit, sort_mode)
        }
        (
            SearchResultSortScope::AllMatches,
            SearchResultSortMode::ModifiedDesc
            | SearchResultSortMode::ModifiedAsc
            | SearchResultSortMode::CreatedDesc
            | SearchResultSortMode::CreatedAsc
            | SearchResultSortMode::SizeDesc
            | SearchResultSortMode::SizeAsc,
        ) => top_metadata_sorted_scores(entries, scored_matches.scored, limit, sort_mode),
        _ => top_ranked_scores(scored_matches.scored, limit),
    };
    let results = scored_indices_to_paths(entries, &ranked, limit);
    (
        SearchResultSet {
            results,
            total_match_count,
        },
        None,
    )
}

fn entry_name_key(entry: &Entry) -> String {
    entry
        .path()
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn entry_path_key(entry: &Entry) -> String {
    crate::path_utils::path_key(entry.path()).replace('\\', "/")
}

fn top_name_sorted_scores(
    entries: &[Entry],
    scored: Vec<SearchCandidateScore>,
    limit: usize,
    mode: SearchResultSortMode,
) -> Vec<IndexedScore> {
    let desc = mode == SearchResultSortMode::NameDesc;
    let mut items = scored
        .into_iter()
        .filter_map(|item| {
            let entry = entries.get(item.index)?;
            Some((item, entry_name_key(entry), entry_path_key(entry)))
        })
        .collect::<Vec<_>>();
    items.sort_unstable_by(|a, b| {
        let cmp =
            a.1.cmp(&b.1)
                .then_with(|| a.2.cmp(&b.2))
                .then_with(|| a.0.ordinal.cmp(&b.0.ordinal));
        if desc {
            cmp.reverse()
        } else {
            cmp
        }
    });
    items
        .into_iter()
        .take(limit)
        .map(|(item, _, _)| IndexedScore {
            index: item.index,
            score: item.score,
        })
        .collect()
}

fn top_metadata_sorted_scores(
    entries: &[Entry],
    scored: Vec<SearchCandidateScore>,
    limit: usize,
    mode: SearchResultSortMode,
) -> Vec<IndexedScore> {
    let desc = matches!(
        mode,
        SearchResultSortMode::ModifiedDesc
            | SearchResultSortMode::CreatedDesc
            | SearchResultSortMode::SizeDesc
    );
    let mut items = scored
        .into_iter()
        .filter_map(|item| {
            let entry = entries.get(item.index)?;
            let metadata = std::fs::metadata(entry.path()).ok();
            let timestamp = match mode {
                SearchResultSortMode::ModifiedDesc | SearchResultSortMode::ModifiedAsc => {
                    metadata.as_ref().and_then(|meta| meta.modified().ok())
                }
                SearchResultSortMode::CreatedDesc | SearchResultSortMode::CreatedAsc => {
                    metadata.as_ref().and_then(|meta| meta.created().ok())
                }
                _ => None,
            };
            let size_bytes = match mode {
                SearchResultSortMode::SizeDesc | SearchResultSortMode::SizeAsc => metadata
                    .as_ref()
                    .filter(|meta| meta.is_file())
                    .map(|meta| meta.len()),
                _ => None,
            };
            Some((
                item,
                entry_name_key(entry),
                entry_path_key(entry),
                timestamp,
                size_bytes,
            ))
        })
        .collect::<Vec<_>>();
    items.sort_unstable_by(|a, b| {
        let value_cmp = if matches!(
            mode,
            SearchResultSortMode::SizeDesc | SearchResultSortMode::SizeAsc
        ) {
            compare_optional_sort_value(a.4, b.4, desc)
        } else {
            compare_optional_sort_value(a.3, b.3, desc)
        };
        value_cmp
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
            .then_with(|| a.0.ordinal.cmp(&b.0.ordinal))
    });
    items
        .into_iter()
        .take(limit)
        .map(|(item, _, _, _, _)| IndexedScore {
            index: item.index,
            score: item.score,
        })
        .collect()
}

fn compare_optional_sort_value<T: Ord>(
    a: Option<T>,
    b: Option<T>,
    desc: bool,
) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => {
            if desc {
                b.cmp(&a)
            } else {
                a.cmp(&b)
            }
        }
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
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
    Ok(try_search_entries_with_scope_and_count(
        query,
        entries,
        limit,
        use_regex,
        ignore_case,
        root,
        prefer_relative,
    )?
    .results)
}

pub(crate) fn try_search_entries_with_scope_and_count(
    query: &str,
    entries: &[PathBuf],
    limit: usize,
    use_regex: bool,
    ignore_case: bool,
    root: Option<&Path>,
    prefer_relative: bool,
) -> Result<SearchResultSet, String> {
    let started_at = Instant::now();
    let path_refs = entries.iter().map(PathBuf::as_path).collect::<Vec<_>>();
    let mut scored = try_collect_search_matches(
        query,
        &path_refs,
        use_regex,
        ignore_case,
        root,
        prefer_relative,
        None,
    )?;
    if !use_regex {
        if let Some(root) = root {
            scored.scored.retain(|item| {
                path_refs.get(item.index).is_some_and(|path| {
                    crate::query::has_visible_match(path, root, query, prefer_relative, ignore_case)
                })
            });
        }
    }
    let total_match_count = scored.scored.len();
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
    Ok(SearchResultSet {
        results,
        total_match_count,
    })
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

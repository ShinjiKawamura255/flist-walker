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

pub use cache::SearchPrefixCache;
use config::{resolve_execution_mode, SearchExecutionMode};
use execute::{
    collect_entries_parallel, collect_entries_sequential, collect_parallel, collect_sequential,
    CANCELLATION_CHECK_INTERVAL,
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
pub struct SearchResultSet {
    pub results: Vec<(PathBuf, f64)>,
    pub total_match_count: usize,
    pub evaluated_candidate_count: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchSortMode {
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

impl SearchSortMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Score => "Score",
            Self::NameAsc => "Name (A-Z)",
            Self::NameDesc => "Name (Z-A)",
            Self::ModifiedDesc => "Modified (New)",
            Self::ModifiedAsc => "Modified (Old)",
            Self::CreatedDesc => "Created (New)",
            Self::CreatedAsc => "Created (Old)",
            Self::SizeDesc => "Size (Large)",
            Self::SizeAsc => "Size (Small)",
        }
    }

    pub fn uses_metadata(self) -> bool {
        matches!(
            self,
            Self::ModifiedDesc
                | Self::ModifiedAsc
                | Self::CreatedDesc
                | Self::CreatedAsc
                | Self::SizeDesc
                | Self::SizeAsc
        )
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchSortScope {
    #[default]
    ShownResults,
    AllMatches,
}

impl SearchSortScope {
    pub fn label(self) -> &'static str {
        match self {
            Self::ShownResults => "Shown results",
            Self::AllMatches => "All matches",
        }
    }

    pub fn sorts_all_matches_before_limit(self, mode: SearchSortMode) -> bool {
        self == Self::AllMatches && mode != SearchSortMode::Score
    }
}

pub type SearchResultSortMode = SearchSortMode;
pub type SearchResultSortScope = SearchSortScope;

#[derive(Debug, PartialEq)]
pub(crate) enum SearchRunOutcome {
    Completed(SearchResultSet, Option<String>),
    Canceled,
}

fn never_cancel() -> bool {
    false
}

#[allow(clippy::too_many_arguments)]
pub fn rank_search_results(
    entries: &Arc<Vec<Entry>>,
    query: &str,
    root: &Path,
    limit: usize,
    use_regex: bool,
    ignore_case: bool,
    prefer_relative: bool,
    prefix_cache: &mut SearchPrefixCache,
    sort_mode: SearchSortMode,
    sort_scope: SearchSortScope,
) -> (SearchResultSet, Option<String>) {
    match rank_search_results_cancellable(
        entries,
        query,
        root,
        limit,
        use_regex,
        ignore_case,
        prefer_relative,
        prefix_cache,
        sort_mode,
        sort_scope,
        &never_cancel,
    ) {
        SearchRunOutcome::Completed(result_set, error) => (result_set, error),
        SearchRunOutcome::Canceled => unreachable!("non-cancellable search was canceled"),
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn rank_search_results_cancellable(
    entries: &Arc<Vec<Entry>>,
    query: &str,
    root: &Path,
    limit: usize,
    use_regex: bool,
    ignore_case: bool,
    prefer_relative: bool,
    prefix_cache: &mut SearchPrefixCache,
    sort_mode: SearchSortMode,
    sort_scope: SearchSortScope,
    cancellation: &(dyn Fn() -> bool + Sync),
) -> SearchRunOutcome {
    if cancellation() {
        return SearchRunOutcome::Canceled;
    }
    let query_trimmed = query.trim().to_string();
    let cached_candidates = if use_regex {
        None
    } else {
        prefix_cache.lookup_candidates(entries, root, ignore_case, prefer_relative, &query_trimmed)
    };
    let evaluated_candidate_count = if query_trimmed.is_empty() {
        0
    } else {
        cached_candidates
            .as_ref()
            .map_or(entries.len(), |candidates| candidates.len())
    };
    let scored_matches = if query_trimmed.is_empty() {
        let mut scored = Vec::with_capacity(entries.len());
        for (index, _) in entries.iter().enumerate() {
            if index.is_multiple_of(CANCELLATION_CHECK_INTERVAL) && cancellation() {
                return SearchRunOutcome::Canceled;
            }
            scored.push(SearchCandidateScore {
                index,
                score: 0.0,
                ordinal: index,
            });
        }
        SearchScoredMatches { scored }
    } else {
        match try_collect_entry_matches_cancellable(
            query,
            entries,
            use_regex,
            ignore_case,
            Some(root),
            prefer_relative,
            cached_candidates.as_ref().map(|items| items.as_slice()),
            cancellation,
        ) {
            Ok(Some(scored_matches)) => scored_matches,
            Ok(None) => return SearchRunOutcome::Canceled,
            Err(err) => {
                return SearchRunOutcome::Completed(SearchResultSet::default(), Some(err));
            }
        }
    };
    if cancellation() {
        return SearchRunOutcome::Canceled;
    }
    let total_match_count = scored_matches.scored.len();
    if SearchPrefixCache::is_cacheable_query(&query_trimmed)
        && scored_matches.scored.len() <= SearchPrefixCache::MAX_MATCHED_INDICES
    {
        let mut ranked = scored_matches.scored.clone();
        sort_scored_matches(&mut ranked);
        let matched_indices = ranked.iter().map(|item| item.index).collect();
        prefix_cache.maybe_store(
            entries,
            root,
            ignore_case,
            prefer_relative,
            &query_trimmed,
            matched_indices,
        );
    }
    let ranked = match sort_mode {
        SearchSortMode::NameAsc | SearchSortMode::NameDesc
            if sort_scope.sorts_all_matches_before_limit(sort_mode) =>
        {
            top_name_sorted_scores(entries, scored_matches.scored, limit, sort_mode)
        }
        _ if sort_scope.sorts_all_matches_before_limit(sort_mode) => {
            let Some(ranked) = top_metadata_sorted_scores(
                entries,
                scored_matches.scored,
                limit,
                sort_mode,
                cancellation,
            ) else {
                return SearchRunOutcome::Canceled;
            };
            ranked
        }
        _ => top_ranked_scores(scored_matches.scored, limit),
    };
    let results = scored_indices_to_paths(entries, &ranked, limit);
    if cancellation() {
        return SearchRunOutcome::Canceled;
    }
    SearchRunOutcome::Completed(
        SearchResultSet {
            results,
            total_match_count,
            evaluated_candidate_count,
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
    mode: SearchSortMode,
) -> Vec<IndexedScore> {
    let desc = mode == SearchSortMode::NameDesc;
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
    mode: SearchSortMode,
    cancellation: &(dyn Fn() -> bool + Sync),
) -> Option<Vec<IndexedScore>> {
    let desc = matches!(
        mode,
        SearchSortMode::ModifiedDesc | SearchSortMode::CreatedDesc | SearchSortMode::SizeDesc
    );
    let mut items = Vec::with_capacity(scored.len());
    for (ordinal, item) in scored.into_iter().enumerate() {
        if ordinal.is_multiple_of(CANCELLATION_CHECK_INTERVAL) && cancellation() {
            return None;
        }
        if let Some(entry) = entries.get(item.index) {
            let metadata = std::fs::metadata(entry.path()).ok();
            let timestamp = match mode {
                SearchSortMode::ModifiedDesc | SearchSortMode::ModifiedAsc => {
                    metadata.as_ref().and_then(|meta| meta.modified().ok())
                }
                SearchSortMode::CreatedDesc | SearchSortMode::CreatedAsc => {
                    metadata.as_ref().and_then(|meta| meta.created().ok())
                }
                _ => None,
            };
            let size_bytes = match mode {
                SearchSortMode::SizeDesc | SearchSortMode::SizeAsc => metadata
                    .as_ref()
                    .filter(|meta| meta.is_file())
                    .map(|meta| meta.len()),
                _ => None,
            };
            items.push((
                item,
                entry_name_key(entry),
                entry_path_key(entry),
                timestamp,
                size_bytes,
            ));
        }
    }
    items.sort_unstable_by(|a, b| {
        let value_cmp = if matches!(mode, SearchSortMode::SizeDesc | SearchSortMode::SizeAsc) {
            compare_optional_sort_value(a.4, b.4, desc)
        } else {
            compare_optional_sort_value(a.3, b.3, desc)
        };
        value_cmp
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
            .then_with(|| a.0.ordinal.cmp(&b.0.ordinal))
    });
    Some(
        items
            .into_iter()
            .take(limit)
            .map(|(item, _, _, _, _)| IndexedScore {
                index: item.index,
                score: item.score,
            })
            .collect(),
    )
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

#[allow(clippy::too_many_arguments)]
fn try_collect_entry_matches_cancellable(
    query: &str,
    entries: &[Entry],
    use_regex: bool,
    ignore_case: bool,
    root: Option<&Path>,
    prefer_relative: bool,
    candidate_indices: Option<&[usize]>,
    cancellation: &(dyn Fn() -> bool + Sync),
) -> Result<Option<SearchScoredMatches>, String> {
    try_collect_entry_matches_with_mode_cancellable(
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
        cancellation,
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

fn try_collect_entry_matches_with_mode_cancellable(
    query: &str,
    entries: &[Entry],
    options: SearchCollectOptions<'_>,
    cancellation: &(dyn Fn() -> bool + Sync),
) -> Result<Option<SearchScoredMatches>, String> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Some(SearchScoredMatches::default()));
    }

    let compiled = compile_query(query, options.use_regex, options.ignore_case)?;
    let ctx = SearchContext {
        root: options.root,
        prefer_relative: options.prefer_relative,
    };
    let candidate_count = options
        .candidate_indices
        .map_or(entries.len(), |items| items.len());
    let execution = resolve_execution_mode(options.mode, candidate_count);
    Ok(match execution {
        SearchExecutionMode::Sequential => collect_entries_sequential(
            entries,
            &compiled,
            ctx,
            options.candidate_indices,
            cancellation,
        ),
        SearchExecutionMode::Parallel => collect_entries_parallel(
            entries,
            &compiled,
            ctx,
            options.candidate_indices,
            cancellation,
        ),
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
    };
    let candidate_count = options
        .candidate_indices
        .map_or(entries.len(), |items| items.len());
    let execution = resolve_execution_mode(options.mode, candidate_count);
    Ok(match execution {
        SearchExecutionMode::Sequential => collect_sequential(
            entries,
            &compiled,
            ctx,
            options.candidate_indices,
            &never_cancel,
        )
        .expect("non-cancellable path collection was canceled"),
        SearchExecutionMode::Parallel => collect_parallel(
            entries,
            &compiled,
            ctx,
            options.candidate_indices,
            &never_cancel,
        )
        .expect("non-cancellable path collection was canceled"),
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
    let scored = try_collect_search_matches(
        query,
        &path_refs,
        use_regex,
        ignore_case,
        root,
        prefer_relative,
        None,
    )?;
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
        evaluated_candidate_count: entries.len(),
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

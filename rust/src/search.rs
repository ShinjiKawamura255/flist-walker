use crate::entry::Entry;
use crate::path_utils::normalize_windows_path;
use crate::query::{
    include_alternatives, parse_include_alternative, parse_query, split_anchor,
    token_uses_regex_syntax,
};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use rayon::prelude::*;
use rayon::{ThreadPool, ThreadPoolBuilder};
use regex::{Regex, RegexBuilder};
use std::collections::VecDeque;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Instant;
use std::{env, thread};
use tracing::{debug, warn};

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

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(crate) struct SearchEntriesSnapshotKey {
    pub(crate) ptr: usize,
    pub(crate) len: usize,
}

impl SearchEntriesSnapshotKey {
    pub(crate) fn from_entries(entries: &Arc<Vec<Entry>>) -> Self {
        Self {
            ptr: Arc::as_ptr(entries) as usize,
            len: entries.len(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SearchPrefixCacheEntry {
    snapshot: SearchEntriesSnapshotKey,
    query: String,
    matched_indices: Arc<Vec<usize>>,
    approx_bytes: usize,
}

#[derive(Default)]
pub(crate) struct SearchPrefixCache {
    pub(crate) entries: VecDeque<SearchPrefixCacheEntry>,
    pub(crate) total_bytes: usize,
}

impl SearchPrefixCache {
    pub(crate) const MAX_ENTRIES: usize = 64;
    pub(crate) const MAX_BYTES: usize = 8 * 1024 * 1024;
    pub(crate) const MAX_MATCHED_INDICES: usize = 20_000;
    const MIN_QUERY_LEN: usize = 3;

    pub(crate) fn is_cacheable_query(query: &str) -> bool {
        let q = query.trim();
        if q.len() < Self::MIN_QUERY_LEN {
            return false;
        }
        if q.contains(char::is_whitespace) {
            return false;
        }
        !q.contains(['|', '!', '\'', '^', '$'])
    }

    pub(crate) fn is_safe_prefix_extension(prefix: &str, query: &str) -> bool {
        if !Self::is_cacheable_query(prefix) || !Self::is_cacheable_query(query) {
            return false;
        }
        query.starts_with(prefix) && query.len() > prefix.len()
    }

    pub(crate) fn lookup_candidates(
        &mut self,
        snapshot: SearchEntriesSnapshotKey,
        query: &str,
    ) -> Option<Arc<Vec<usize>>> {
        if !Self::is_cacheable_query(query) {
            return None;
        }

        let mut best_idx = None;
        let mut best_len = 0usize;
        for (idx, entry) in self.entries.iter().enumerate() {
            if entry.snapshot != snapshot {
                continue;
            }
            if !Self::is_safe_prefix_extension(&entry.query, query) {
                continue;
            }
            if entry.query.len() > best_len {
                best_len = entry.query.len();
                best_idx = Some(idx);
            }
        }

        let idx = best_idx?;
        let entry = self.entries.remove(idx)?;
        let matched = Arc::clone(&entry.matched_indices);
        self.entries.push_back(entry);
        Some(matched)
    }

    pub(crate) fn maybe_store(
        &mut self,
        snapshot: SearchEntriesSnapshotKey,
        query: &str,
        matched_indices: Vec<usize>,
    ) {
        if !Self::is_cacheable_query(query) {
            return;
        }
        if matched_indices.is_empty() || matched_indices.len() > Self::MAX_MATCHED_INDICES {
            return;
        }

        let query = query.trim().to_string();
        let approx_bytes = query.len().saturating_add(
            matched_indices
                .len()
                .saturating_mul(std::mem::size_of::<usize>()),
        );
        if approx_bytes > Self::MAX_BYTES {
            return;
        }

        if let Some(existing_pos) = self
            .entries
            .iter()
            .position(|entry| entry.snapshot == snapshot && entry.query == query)
        {
            if let Some(old) = self.entries.remove(existing_pos) {
                self.total_bytes = self.total_bytes.saturating_sub(old.approx_bytes);
            }
        }

        self.total_bytes = self.total_bytes.saturating_add(approx_bytes);
        self.entries.push_back(SearchPrefixCacheEntry {
            snapshot,
            query,
            matched_indices: Arc::new(matched_indices),
            approx_bytes,
        });
        self.evict_over_limit();
    }

    fn evict_over_limit(&mut self) {
        while self.entries.len() > Self::MAX_ENTRIES || self.total_bytes > Self::MAX_BYTES {
            let Some(oldest) = self.entries.pop_front() else {
                break;
            };
            self.total_bytes = self.total_bytes.saturating_sub(oldest.approx_bytes);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SearchExecutionMode {
    Auto,
    Sequential,
    Parallel,
}

#[derive(Debug, Clone)]
struct LiteralPattern {
    anchored_start: bool,
    anchored_end: bool,
    core: String,
    core_chars: Vec<char>,
    first_char: Option<char>,
    last_char: Option<char>,
}

#[derive(Debug, Clone)]
struct AlternativeSet {
    alternatives: Vec<LiteralPattern>,
}

#[derive(Debug, Clone)]
struct IncludeAlternative {
    exact: bool,
    literal: LiteralPattern,
}

#[derive(Debug, Clone)]
enum IncludeMatcher {
    Regex(Regex),
    Alternatives(Vec<IncludeAlternative>),
}

#[derive(Debug, Clone)]
struct CompiledQuery {
    exact_terms: Vec<AlternativeSet>,
    exclude_terms: Vec<AlternativeSet>,
    include_terms: Vec<IncludeMatcher>,
    include_literal_bonus_terms: Vec<AlternativeSet>,
    include_exact_bonus_terms: Vec<LiteralPattern>,
    score_query: String,
}

#[derive(Clone, Copy)]
struct SearchContext<'a> {
    root: Option<&'a Path>,
    prefer_relative: bool,
    ignore_case: bool,
}

struct SearchableEntry {
    name: String,
    full: String,
}

#[derive(Default)]
struct SearchChunkResult {
    scored: Vec<SearchCandidateScore>,
}

const SEARCH_PARALLEL_THRESHOLD_DEFAULT: usize = 25_000;
const SEARCH_PARALLEL_CHUNK_MIN: usize = 1_024;
const SEARCH_PARALLEL_CHUNK_MAX: usize = 16_384;
const SEARCH_THREADS_MAX: usize = 32;

fn is_subsequence(query: &[char], text: &str) -> bool {
    let mut qi = 0usize;
    for ch in text.chars() {
        if qi < query.len() && ch == query[qi] {
            qi += 1;
        }
    }
    qi == query.len()
}

fn normalize_text(text: &str, ignore_case: bool) -> String {
    if ignore_case {
        text.to_ascii_lowercase()
    } else {
        text.to_string()
    }
}

fn compile_literal_pattern(term: &str, ignore_case: bool) -> Option<LiteralPattern> {
    let normalized = normalize_text(term, ignore_case);
    let (anchored_start, anchored_end, core) = split_anchor(&normalized);
    if core.is_empty() {
        return None;
    }

    let core = core.to_string();
    let core_chars = core.chars().collect::<Vec<_>>();
    let first_char = core_chars.first().copied();
    let last_char = core_chars.last().copied();
    Some(LiteralPattern {
        anchored_start,
        anchored_end,
        core,
        core_chars,
        first_char,
        last_char,
    })
}

fn compile_alternative_set(term: &str, ignore_case: bool) -> AlternativeSet {
    let normalized = normalize_text(term, ignore_case);
    let alternatives = include_alternatives(&normalized)
        .into_iter()
        .filter_map(|candidate| parse_include_alternative(candidate).map(|(_, value)| value))
        .filter_map(|candidate| compile_literal_pattern(&candidate, ignore_case))
        .collect();
    AlternativeSet { alternatives }
}

fn compile_non_exact_alternative_set(term: &str, ignore_case: bool) -> AlternativeSet {
    let normalized = normalize_text(term, ignore_case);
    let alternatives = include_alternatives(&normalized)
        .into_iter()
        .filter_map(parse_include_alternative)
        .filter_map(|(exact, candidate)| {
            if exact {
                return None;
            }
            compile_literal_pattern(&candidate, ignore_case)
        })
        .collect();
    AlternativeSet { alternatives }
}

fn compile_include_matcher(
    term: &str,
    use_regex: bool,
    ignore_case: bool,
) -> Result<IncludeMatcher, String> {
    if use_regex && token_uses_regex_syntax(term) {
        let re = RegexBuilder::new(term)
            .case_insensitive(ignore_case)
            .build()
            .map_err(|err| format!("invalid regex '{term}': {err}"))?;
        return Ok(IncludeMatcher::Regex(re));
    }

    let alternatives = include_alternatives(term)
        .into_iter()
        .filter_map(parse_include_alternative)
        .filter_map(|(exact, candidate)| {
            compile_literal_pattern(&candidate, ignore_case)
                .map(|literal| IncludeAlternative { exact, literal })
        })
        .collect();
    Ok(IncludeMatcher::Alternatives(alternatives))
}

fn build_score_query(
    include_terms: &[String],
    exact_terms: &[String],
    ignore_case: bool,
) -> String {
    let mut score_query = include_terms
        .iter()
        .flat_map(|term| include_alternatives(term))
        .filter_map(|term| {
            let (_, candidate) = parse_include_alternative(term)?;
            let (_, _, core) = split_anchor(&candidate);
            (!core.is_empty()).then_some(normalize_text(core, ignore_case))
        })
        .collect::<Vec<_>>()
        .join(" ");

    if score_query.is_empty() {
        if let Some(first_exact) = exact_terms.first() {
            score_query = normalize_text(first_exact, ignore_case);
        }
    }

    score_query
}

fn compile_query(query: &str, use_regex: bool, ignore_case: bool) -> Result<CompiledQuery, String> {
    let spec = parse_query(query);

    let exact_terms = spec
        .exact_terms
        .iter()
        .map(|term| compile_alternative_set(term, ignore_case))
        .collect::<Vec<_>>();
    let exclude_terms = spec
        .exclude_terms
        .iter()
        .map(|term| compile_alternative_set(term, ignore_case))
        .collect::<Vec<_>>();
    let mut include_terms = Vec::with_capacity(spec.include_terms.len());
    let mut include_literal_bonus_terms = Vec::new();
    let mut include_exact_bonus_terms = Vec::new();
    for term in &spec.include_terms {
        include_terms.push(compile_include_matcher(term, use_regex, ignore_case)?);
        if !use_regex {
            let literal_bonus_set = compile_non_exact_alternative_set(term, ignore_case);
            if !literal_bonus_set.alternatives.is_empty() {
                include_literal_bonus_terms.push(literal_bonus_set);
            }
        }
        if !use_regex {
            for candidate in include_alternatives(term) {
                let Some((exact, parsed)) = parse_include_alternative(candidate) else {
                    continue;
                };
                if !exact {
                    continue;
                }
                let (_, _, core) = split_anchor(&parsed);
                if let Some(pattern) = compile_literal_pattern(core, ignore_case) {
                    include_exact_bonus_terms.push(pattern);
                }
            }
        }
    }

    Ok(CompiledQuery {
        exact_terms,
        exclude_terms,
        include_terms,
        include_literal_bonus_terms,
        include_exact_bonus_terms,
        score_query: build_score_query(&spec.include_terms, &spec.exact_terms, ignore_case),
    })
}

fn bonus_for_alternative_set(set: &AlternativeSet, entry: &SearchableEntry) -> f64 {
    let mut bonus = 0.0;
    if set.alternatives.iter().any(|pattern| {
        matches_anchored_literal(pattern, &entry.name)
            || matches_anchored_literal(pattern, &entry.full)
    }) {
        bonus += 150.0;
    }
    if set
        .alternatives
        .iter()
        .any(|pattern| entry.name == pattern.core || entry.full == pattern.core)
    {
        bonus += 150.0;
    }
    bonus
}

fn matches_anchored_literal(pattern: &LiteralPattern, text: &str) -> bool {
    if pattern.anchored_start && pattern.anchored_end {
        text == pattern.core
    } else if pattern.anchored_start {
        text.starts_with(&pattern.core)
    } else if pattern.anchored_end {
        text.ends_with(&pattern.core)
    } else {
        text.contains(&pattern.core)
    }
}

fn matches_alternative_set(set: &AlternativeSet, name: &str, full: &str) -> bool {
    set.alternatives.iter().any(|pattern| {
        matches_anchored_literal(pattern, name) || matches_anchored_literal(pattern, full)
    })
}

fn matches_include_literal(pattern: &LiteralPattern, name: &str, full: &str) -> bool {
    if pattern.anchored_start
        && !matches!(
            pattern.first_char,
            Some(ch) if name.starts_with(ch) || full.starts_with(ch)
        )
    {
        return false;
    }
    if pattern.anchored_end
        && !matches!(
            pattern.last_char,
            Some(ch) if name.ends_with(ch) || full.ends_with(ch)
        )
    {
        return false;
    }

    name.contains(&pattern.core)
        || is_subsequence(&pattern.core_chars, name)
        || full.contains(&pattern.core)
        || is_subsequence(&pattern.core_chars, full)
}

fn matches_include_matcher(matcher: &IncludeMatcher, name: &str, full: &str) -> bool {
    match matcher {
        IncludeMatcher::Regex(re) => re.is_match(name) || re.is_match(full),
        IncludeMatcher::Alternatives(alternatives) => alternatives.iter().any(|alternative| {
            if alternative.exact {
                matches_anchored_literal(&alternative.literal, name)
                    || matches_anchored_literal(&alternative.literal, full)
            } else {
                matches_include_literal(&alternative.literal, name, full)
            }
        }),
    }
}

fn searchable_full(
    path: &Path,
    root: Option<&Path>,
    prefer_relative: bool,
    ignore_case: bool,
) -> String {
    let normalized_path = normalize_windows_path(path);
    if prefer_relative {
        if let Some(root) = root {
            let normalized_root = normalize_windows_path(root);
            if let Ok(rel) = normalized_path.strip_prefix(&normalized_root) {
                return normalize_text(&rel.to_string_lossy(), ignore_case);
            }
        }
    }
    normalize_text(&normalized_path.to_string_lossy(), ignore_case)
}

fn build_searchable_entry(path: &Path, ctx: SearchContext<'_>) -> SearchableEntry {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| normalize_text(s, ctx.ignore_case))
        .unwrap_or_default();
    let full = searchable_full(path, ctx.root, ctx.prefer_relative, ctx.ignore_case);
    SearchableEntry { name, full }
}

fn matches_compiled_query(compiled: &CompiledQuery, entry: &SearchableEntry) -> bool {
    for term in &compiled.exclude_terms {
        if matches_alternative_set(term, &entry.name, &entry.full) {
            return false;
        }
    }

    for term in &compiled.exact_terms {
        if !matches_alternative_set(term, &entry.name, &entry.full) {
            return false;
        }
    }

    for matcher in &compiled.include_terms {
        if !matches_include_matcher(matcher, &entry.name, &entry.full) {
            return false;
        }
    }

    true
}

fn fallback_score(query: &str, text: &str) -> f64 {
    if query.is_empty() {
        return 0.0;
    }

    let mut score = 0.0;
    if text.contains(query) {
        score += 25.0;
    }
    if text.starts_with(query) {
        score += 30.0;
    }
    score + (query.len().min(text.len()) as f64)
}

fn score_entry(matcher: &SkimMatcherV2, compiled: &CompiledQuery, entry: &SearchableEntry) -> f64 {
    let mut score = if compiled.score_query.is_empty() {
        0.0
    } else {
        matcher
            .fuzzy_match(&entry.full, &compiled.score_query)
            .map(|value| value as f64)
            .unwrap_or_else(|| fallback_score(&compiled.score_query, &entry.full))
    };

    if !compiled.score_query.is_empty() && entry.name == compiled.score_query {
        score += 1000.0;
    } else if !compiled.score_query.is_empty() && entry.full == compiled.score_query {
        score += 900.0;
    }

    for term in &compiled.exact_terms {
        if matches_alternative_set(term, &entry.name, &entry.full) {
            score += 800.0;
        }
    }

    for term in &compiled.include_literal_bonus_terms {
        score += bonus_for_alternative_set(term, entry);
    }

    for pattern in &compiled.include_exact_bonus_terms {
        if matches_anchored_literal(pattern, &entry.name)
            || matches_anchored_literal(pattern, &entry.full)
        {
            score += 300.0;
            if entry.name == pattern.core {
                score += 300.0;
            }
        }
    }

    score
}

fn evaluate_candidate(
    path: &Path,
    index: usize,
    ordinal: usize,
    compiled: &CompiledQuery,
    ctx: SearchContext<'_>,
    matcher: &SkimMatcherV2,
) -> Option<SearchCandidateScore> {
    let entry = build_searchable_entry(path, ctx);
    if !matches_compiled_query(compiled, &entry) {
        return None;
    }

    Some(SearchCandidateScore {
        index,
        score: score_entry(matcher, compiled, &entry),
        ordinal,
    })
}

fn merge_chunk_results(
    mut left: SearchChunkResult,
    mut right: SearchChunkResult,
) -> SearchChunkResult {
    left.scored.append(&mut right.scored);
    left
}

fn collect_sequential(
    entries: &[PathBuf],
    compiled: &CompiledQuery,
    ctx: SearchContext<'_>,
    candidate_indices: Option<&[usize]>,
) -> SearchScoredMatches {
    let matcher = SkimMatcherV2::default();
    let scored = match candidate_indices {
        Some(indices) => indices
            .iter()
            .copied()
            .enumerate()
            .filter_map(|(ordinal, index)| {
                entries.get(index).and_then(|path| {
                    evaluate_candidate(path, index, ordinal, compiled, ctx, &matcher)
                })
            })
            .collect(),
        None => entries
            .iter()
            .enumerate()
            .filter_map(|(index, path)| {
                evaluate_candidate(path, index, index, compiled, ctx, &matcher)
            })
            .collect(),
    };
    SearchScoredMatches { scored }
}

fn search_parallel_threshold() -> usize {
    env::var("FLISTWALKER_SEARCH_PARALLEL_THRESHOLD")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(SEARCH_PARALLEL_THRESHOLD_DEFAULT)
}

fn search_threads() -> usize {
    env::var("FLISTWALKER_SEARCH_THREADS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or_else(|| {
            thread::available_parallelism()
                .map(|value| value.get())
                .unwrap_or(1)
        })
        .min(SEARCH_THREADS_MAX)
}

fn search_parallel_chunk_size(candidate_count: usize) -> usize {
    let threads = search_threads().max(1);
    let target = candidate_count / threads.saturating_mul(8).max(1);
    target.clamp(SEARCH_PARALLEL_CHUNK_MIN, SEARCH_PARALLEL_CHUNK_MAX)
}

fn search_thread_pool() -> &'static Option<ThreadPool> {
    static POOL: OnceLock<Option<ThreadPool>> = OnceLock::new();
    POOL.get_or_init(|| {
        let threads = search_threads();
        if threads <= 1 {
            None
        } else {
            ThreadPoolBuilder::new().num_threads(threads).build().ok()
        }
    })
}

fn with_search_thread_pool<R: Send>(f: impl FnOnce() -> R + Send) -> R {
    match search_thread_pool() {
        Some(pool) => pool.install(f),
        None => f(),
    }
}

fn collect_parallel(
    entries: &[PathBuf],
    compiled: &CompiledQuery,
    ctx: SearchContext<'_>,
    candidate_indices: Option<&[usize]>,
) -> SearchScoredMatches {
    let candidate_count = candidate_indices.map_or(entries.len(), |items| items.len());
    let chunk_size = search_parallel_chunk_size(candidate_count);

    let scored = with_search_thread_pool(|| match candidate_indices {
        Some(indices) => {
            indices
                .par_chunks(chunk_size)
                .enumerate()
                .map(|(chunk_idx, chunk)| {
                    let matcher = SkimMatcherV2::default();
                    let base_ordinal = chunk_idx.saturating_mul(chunk_size);
                    let scored = chunk
                        .iter()
                        .copied()
                        .enumerate()
                        .filter_map(|(offset, index)| {
                            entries.get(index).and_then(|path| {
                                evaluate_candidate(
                                    path,
                                    index,
                                    base_ordinal + offset,
                                    compiled,
                                    ctx,
                                    &matcher,
                                )
                            })
                        })
                        .collect();
                    SearchChunkResult { scored }
                })
                .reduce(SearchChunkResult::default, merge_chunk_results)
                .scored
        }
        None => {
            (0..entries.len())
                .into_par_iter()
                .with_min_len(chunk_size)
                .fold(
                    || (SkimMatcherV2::default(), Vec::<SearchCandidateScore>::new()),
                    |(matcher, mut scored), index| {
                        if let Some(item) = evaluate_candidate(
                            &entries[index],
                            index,
                            index,
                            compiled,
                            ctx,
                            &matcher,
                        ) {
                            scored.push(item);
                        }
                        (matcher, scored)
                    },
                )
                .map(|(_, scored)| SearchChunkResult { scored })
                .reduce(SearchChunkResult::default, merge_chunk_results)
                .scored
        }
    });

    SearchScoredMatches { scored }
}

fn resolve_execution_mode(
    mode: SearchExecutionMode,
    candidate_count: usize,
) -> SearchExecutionMode {
    match mode {
        SearchExecutionMode::Auto => {
            if candidate_count >= search_parallel_threshold() && search_threads() > 1 {
                SearchExecutionMode::Parallel
            } else {
                SearchExecutionMode::Sequential
            }
        }
        other => other,
    }
}

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
            crate::ui_model::has_visible_match(path, root, query, prefer_relative, ignore_case)
        })
        .collect()
}

fn scored_indices_to_paths(
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
    let scored_matches = match try_collect_search_matches(
        query,
        &entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>(),
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

pub(crate) fn try_collect_search_matches(
    query: &str,
    entries: &[PathBuf],
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
    entries: &[PathBuf],
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
    let scored = try_collect_search_matches(
        query,
        entries,
        use_regex,
        ignore_case,
        root,
        prefer_relative,
        None,
    )?;
    let results = materialize_scored_entries(entries, top_ranked_scores(scored.scored, limit));
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
    let mut scored = try_collect_search_matches(
        query,
        entries,
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

fn materialize_scored_entries(
    entries: &[PathBuf],
    scored: Vec<IndexedScore>,
) -> Vec<(PathBuf, f64)> {
    scored
        .into_iter()
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
mod tests {
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

        let limited =
            try_search_entries_with_scope("module_1", &entries, 7, false, true, None, false)
                .expect("limited search");
        let full = try_search_entries_indexed_with_scope(
            "module_1", &entries, false, true, None, false, None,
        )
        .expect("full ranked search");
        let expected =
            materialize_scored_entries(entries.as_slice(), full.into_iter().take(7).collect());

        assert_eq!(limited, expected);
    }

    #[test]
    fn parallel_collection_matches_sequential_ranking() {
        let entries: Vec<PathBuf> = (0..50_000)
            .map(|i| PathBuf::from(format!("/tmp/src/module_{i:05}.rs")))
            .collect();

        let sequential = try_collect_search_matches_with_mode(
            "module_123",
            &entries,
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
            &entries,
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
        let out =
            search_entries_with_scope("abc def", &entries, 10, false, true, Some(&root), true);
        assert_eq!(out.len(), 1);
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn relative_search_normalizes_extended_unc_prefixes() {
        let root = PathBuf::from(r"\\server\share");
        let entries = vec![PathBuf::from(r"\\?\UNC\server\share\abc\def.txt")];
        let out =
            search_entries_with_scope("abc def", &entries, 10, false, true, Some(&root), true);
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

        let out =
            search_entries_with_scope("abc def", &entries, 10, false, true, Some(&root), true);
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

        let out =
            search_entries_with_scope("abc def", &entries, 10, false, true, Some(&root), false);
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
}

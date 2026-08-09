use super::{
    config::{search_parallel_chunk_size, with_search_thread_pool},
    match_eval::{evaluate_candidate, evaluate_entry_candidate, SearchContext},
    SearchCandidateScore, SearchScoredMatches,
};
use crate::entry::Entry;
use crate::query::CompiledQuery;
use fuzzy_matcher::skim::SkimMatcherV2;
use rayon::prelude::*;
use std::path::Path;

pub(super) const CANCELLATION_CHECK_INTERVAL: usize = 256;

fn should_cancel(check: &(dyn Fn() -> bool + Sync), ordinal: usize) -> bool {
    ordinal.is_multiple_of(CANCELLATION_CHECK_INTERVAL) && check()
}

#[derive(Default)]
struct SearchChunkResult {
    scored: Vec<SearchCandidateScore>,
}

fn merge_chunk_results(
    mut left: SearchChunkResult,
    mut right: SearchChunkResult,
) -> SearchChunkResult {
    left.scored.append(&mut right.scored);
    left
}

pub(super) fn collect_sequential(
    entries: &[&Path],
    compiled: &CompiledQuery,
    ctx: SearchContext<'_>,
    candidate_indices: Option<&[usize]>,
    cancellation: &(dyn Fn() -> bool + Sync),
) -> Option<SearchScoredMatches> {
    let matcher = SkimMatcherV2::default();
    let mut scored = Vec::new();
    match candidate_indices {
        Some(indices) => {
            for (ordinal, index) in indices.iter().copied().enumerate() {
                if should_cancel(cancellation, ordinal) {
                    return None;
                }
                if let Some(item) = entries.get(index).and_then(|path| {
                    evaluate_candidate(path, index, ordinal, compiled, ctx, &matcher)
                }) {
                    scored.push(item);
                }
            }
        }
        None => {
            for (index, path) in entries.iter().enumerate() {
                if should_cancel(cancellation, index) {
                    return None;
                }
                if let Some(item) = evaluate_candidate(path, index, index, compiled, ctx, &matcher)
                {
                    scored.push(item);
                }
            }
        }
    }
    Some(SearchScoredMatches { scored })
}

pub(super) fn collect_entries_sequential(
    entries: &[Entry],
    compiled: &CompiledQuery,
    ctx: SearchContext<'_>,
    candidate_indices: Option<&[usize]>,
    cancellation: &(dyn Fn() -> bool + Sync),
) -> Option<SearchScoredMatches> {
    let matcher = SkimMatcherV2::default();
    let mut scored = Vec::new();
    match candidate_indices {
        Some(indices) => {
            for (ordinal, index) in indices.iter().copied().enumerate() {
                if should_cancel(cancellation, ordinal) {
                    return None;
                }
                if let Some(item) = entries.get(index).and_then(|entry| {
                    evaluate_entry_candidate(entry, index, ordinal, compiled, ctx, &matcher)
                }) {
                    scored.push(item);
                }
            }
        }
        None => {
            for (index, entry) in entries.iter().enumerate() {
                if should_cancel(cancellation, index) {
                    return None;
                }
                if let Some(item) =
                    evaluate_entry_candidate(entry, index, index, compiled, ctx, &matcher)
                {
                    scored.push(item);
                }
            }
        }
    }
    Some(SearchScoredMatches { scored })
}

pub(super) fn collect_parallel(
    entries: &[&Path],
    compiled: &CompiledQuery,
    ctx: SearchContext<'_>,
    candidate_indices: Option<&[usize]>,
    cancellation: &(dyn Fn() -> bool + Sync),
) -> Option<SearchScoredMatches> {
    let candidate_count = candidate_indices.map_or(entries.len(), |items| items.len());
    let chunk_size = search_parallel_chunk_size(candidate_count);

    let scored = with_search_thread_pool(|| match candidate_indices {
        Some(indices) => indices
            .par_chunks(chunk_size)
            .enumerate()
            .map(|(chunk_idx, chunk)| -> Result<SearchChunkResult, ()> {
                let matcher = SkimMatcherV2::default();
                let base_ordinal = chunk_idx.saturating_mul(chunk_size);
                let mut scored = Vec::new();
                for (offset, index) in chunk.iter().copied().enumerate() {
                    let ordinal = base_ordinal + offset;
                    if should_cancel(cancellation, ordinal) {
                        return Err(());
                    }
                    if let Some(item) = entries.get(index).and_then(|path| {
                        evaluate_candidate(path, index, ordinal, compiled, ctx, &matcher)
                    }) {
                        scored.push(item);
                    }
                }
                Ok(SearchChunkResult { scored })
            })
            .try_reduce(SearchChunkResult::default, |left, right| {
                Ok(merge_chunk_results(left, right))
            })
            .ok()
            .map(|result| result.scored),
        None => (0..entries.len())
            .into_par_iter()
            .with_min_len(chunk_size)
            .try_fold(
                || (SkimMatcherV2::default(), Vec::<SearchCandidateScore>::new()),
                |(matcher, mut scored), index| {
                    if should_cancel(cancellation, index) {
                        return Err(());
                    }
                    if let Some(item) =
                        evaluate_candidate(entries[index], index, index, compiled, ctx, &matcher)
                    {
                        scored.push(item);
                    }
                    Ok((matcher, scored))
                },
            )
            .map(|result| result.map(|(_, scored)| SearchChunkResult { scored }))
            .try_reduce(SearchChunkResult::default, |left, right| {
                Ok(merge_chunk_results(left, right))
            })
            .ok()
            .map(|result| result.scored),
    });
    scored.map(|scored| SearchScoredMatches { scored })
}

pub(super) fn collect_entries_parallel(
    entries: &[Entry],
    compiled: &CompiledQuery,
    ctx: SearchContext<'_>,
    candidate_indices: Option<&[usize]>,
    cancellation: &(dyn Fn() -> bool + Sync),
) -> Option<SearchScoredMatches> {
    let candidate_count = candidate_indices.map_or(entries.len(), |items| items.len());
    let chunk_size = search_parallel_chunk_size(candidate_count);

    let scored = with_search_thread_pool(|| match candidate_indices {
        Some(indices) => indices
            .par_chunks(chunk_size)
            .enumerate()
            .map(|(chunk_idx, chunk)| -> Result<SearchChunkResult, ()> {
                let matcher = SkimMatcherV2::default();
                let base_ordinal = chunk_idx.saturating_mul(chunk_size);
                let mut scored = Vec::new();
                for (offset, index) in chunk.iter().copied().enumerate() {
                    let ordinal = base_ordinal + offset;
                    if should_cancel(cancellation, ordinal) {
                        return Err(());
                    }
                    if let Some(item) = entries.get(index).and_then(|entry| {
                        evaluate_entry_candidate(entry, index, ordinal, compiled, ctx, &matcher)
                    }) {
                        scored.push(item);
                    }
                }
                Ok(SearchChunkResult { scored })
            })
            .try_reduce(SearchChunkResult::default, |left, right| {
                Ok(merge_chunk_results(left, right))
            })
            .ok()
            .map(|result| result.scored),
        None => (0..entries.len())
            .into_par_iter()
            .with_min_len(chunk_size)
            .try_fold(
                || (SkimMatcherV2::default(), Vec::<SearchCandidateScore>::new()),
                |(matcher, mut scored), index| {
                    if should_cancel(cancellation, index) {
                        return Err(());
                    }
                    if let Some(item) = evaluate_entry_candidate(
                        &entries[index],
                        index,
                        index,
                        compiled,
                        ctx,
                        &matcher,
                    ) {
                        scored.push(item);
                    }
                    Ok((matcher, scored))
                },
            )
            .map(|result| result.map(|(_, scored)| SearchChunkResult { scored }))
            .try_reduce(SearchChunkResult::default, |left, right| {
                Ok(merge_chunk_results(left, right))
            })
            .ok()
            .map(|result| result.scored),
    });
    scored.map(|scored| SearchScoredMatches { scored })
}

use super::{
    config::{search_parallel_chunk_size, with_search_thread_pool},
    evaluate_candidate, CompiledQuery, SearchCandidateScore, SearchContext, SearchScoredMatches,
};
use fuzzy_matcher::skim::SkimMatcherV2;
use rayon::prelude::*;
use std::path::Path;

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

pub(super) fn collect_parallel(
    entries: &[&Path],
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
                            entries[index],
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

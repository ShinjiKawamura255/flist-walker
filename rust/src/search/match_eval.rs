use super::SearchCandidateScore;
use crate::entry::Entry;
use crate::query::{CompiledQuery, EvidenceLevel, QueryOptions};
use fuzzy_matcher::skim::SkimMatcherV2;
use std::path::Path;

#[derive(Clone, Copy)]
pub(super) struct SearchContext<'a> {
    pub(super) root: Option<&'a Path>,
    pub(super) prefer_relative: bool,
}

pub(super) fn compile_query(
    query: &str,
    use_regex: bool,
    ignore_case: bool,
) -> Result<CompiledQuery, String> {
    CompiledQuery::compile(
        query,
        QueryOptions {
            use_regex,
            ignore_case,
        },
    )
}

pub(super) fn evaluate_candidate(
    path: &Path,
    index: usize,
    ordinal: usize,
    compiled: &CompiledQuery,
    ctx: SearchContext<'_>,
    matcher: &SkimMatcherV2,
) -> Option<SearchCandidateScore> {
    let prepared = compiled.prepare_candidate(path, ctx.root, ctx.prefer_relative);
    compiled
        .evaluate_with_matcher(&prepared, EvidenceLevel::RankOnly, matcher)
        .map(|evaluation| SearchCandidateScore {
            index,
            score: evaluation.score,
            ordinal,
        })
}

pub(super) fn evaluate_entry_candidate(
    entry: &Entry,
    index: usize,
    ordinal: usize,
    compiled: &CompiledQuery,
    ctx: SearchContext<'_>,
    matcher: &SkimMatcherV2,
) -> Option<SearchCandidateScore> {
    let prepared = compiled.prepare_candidate_with_kind(
        entry.path(),
        ctx.root,
        ctx.prefer_relative,
        entry.kind.and_then(|kind| kind.is_dir),
    );
    compiled
        .evaluate_with_matcher(&prepared, EvidenceLevel::RankOnly, matcher)
        .map(|evaluation| SearchCandidateScore {
            index,
            score: evaluation.score,
            ordinal,
        })
}

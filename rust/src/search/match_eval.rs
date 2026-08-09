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
    let evaluation = if compiled.requires_file_kind() {
        compiled.evaluate_unknown_kind_with_matcher(
            path,
            ctx.root,
            ctx.prefer_relative,
            EvidenceLevel::RankOnly,
            matcher,
            || path.is_dir(),
        )
    } else {
        let prepared = compiled.prepare_candidate(path, ctx.root, ctx.prefer_relative);
        compiled.evaluate_with_matcher(&prepared, EvidenceLevel::RankOnly, matcher)
    };
    evaluation.map(|evaluation| SearchCandidateScore {
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
    evaluate_entry_candidate_with_kind_resolver(
        entry,
        index,
        ordinal,
        compiled,
        ctx,
        matcher,
        || entry.path().is_dir(),
    )
}

fn evaluate_entry_candidate_with_kind_resolver<F>(
    entry: &Entry,
    index: usize,
    ordinal: usize,
    compiled: &CompiledQuery,
    ctx: SearchContext<'_>,
    matcher: &SkimMatcherV2,
    resolve_is_dir: F,
) -> Option<SearchCandidateScore>
where
    F: FnOnce() -> bool,
{
    let evaluation = match entry.kind.and_then(|kind| kind.is_dir) {
        Some(is_dir) => {
            let prepared = compiled.prepare_candidate_with_kind(
                entry.path(),
                ctx.root,
                ctx.prefer_relative,
                Some(is_dir),
            );
            compiled.evaluate_with_matcher(&prepared, EvidenceLevel::RankOnly, matcher)
        }
        None if compiled.requires_file_kind() => compiled.evaluate_unknown_kind_with_matcher(
            entry.path(),
            ctx.root,
            ctx.prefer_relative,
            EvidenceLevel::RankOnly,
            matcher,
            resolve_is_dir,
        ),
        None => {
            let prepared = compiled.prepare_candidate(entry.path(), ctx.root, ctx.prefer_relative);
            compiled.evaluate_with_matcher(&prepared, EvidenceLevel::RankOnly, matcher)
        }
    };
    evaluation.map(|evaluation| SearchCandidateScore {
        index,
        score: evaluation.score,
        ordinal,
    })
}

#[cfg(test)]
mod tests {
    use super::{compile_query, evaluate_entry_candidate_with_kind_resolver, SearchContext};
    use crate::entry::Entry;
    use fuzzy_matcher::skim::SkimMatcherV2;
    use std::cell::Cell;
    use std::path::PathBuf;

    fn evaluate_unknown(
        entry: &Entry,
        query: &str,
        resolved_is_dir: bool,
        resolution_count: &Cell<usize>,
    ) -> Option<super::SearchCandidateScore> {
        let compiled = compile_query(query, false, true).expect("compile query");
        evaluate_entry_candidate_with_kind_resolver(
            entry,
            0,
            0,
            &compiled,
            SearchContext {
                root: None,
                prefer_relative: false,
            },
            &SkimMatcherV2::default(),
            || {
                resolution_count.set(resolution_count.get() + 1);
                resolved_is_dir
            },
        )
    }

    #[test]
    fn unknown_kind_ext_query_resolves_only_when_file_and_directory_results_differ() {
        let resolution_count = Cell::new(0);

        let non_matching = Entry::unknown(PathBuf::from("src/main.py"));
        assert!(evaluate_unknown(&non_matching, "ext:rs", false, &resolution_count).is_none());
        assert_eq!(resolution_count.get(), 0);

        let matching_file = Entry::unknown(PathBuf::from("src/main.rs"));
        assert!(evaluate_unknown(&matching_file, "ext:rs", false, &resolution_count).is_some());
        assert_eq!(resolution_count.get(), 1);

        let excluded_non_match = Entry::unknown(PathBuf::from("src/main.py"));
        assert!(evaluate_unknown(
            &excluded_non_match,
            "name:main !ext:rs",
            false,
            &resolution_count,
        )
        .is_some());
        assert_eq!(resolution_count.get(), 1);

        let excluded_match = Entry::unknown(PathBuf::from("src/main.rs"));
        assert!(evaluate_unknown(
            &excluded_match,
            "name:main !ext:rs",
            false,
            &resolution_count,
        )
        .is_none());
        assert_eq!(resolution_count.get(), 2);
    }
}

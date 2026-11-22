use super::SearchCandidateScore;
use crate::entry::Entry;
use crate::query::{CompiledQuery, QueryOptions};
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
    let score = compiled.rank_path_with_matcher(
        path,
        ctx.root,
        ctx.prefer_relative,
        None,
        matcher,
        || path.is_dir(),
    )?;
    Some(SearchCandidateScore {
        index,
        score,
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
    let score = compiled.rank_path_with_matcher(
        entry.path(),
        ctx.root,
        ctx.prefer_relative,
        entry.kind.and_then(|kind| kind.is_dir),
        matcher,
        resolve_is_dir,
    )?;
    Some(SearchCandidateScore {
        index,
        score,
        ordinal,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        compile_query, evaluate_candidate, evaluate_entry_candidate_with_kind_resolver,
        SearchContext,
    };
    use crate::entry::Entry;
    use crate::query::{EvidenceLevel, PreparedCandidate};
    use fuzzy_matcher::skim::SkimMatcherV2;
    use std::cell::Cell;
    use std::path::PathBuf;

    #[test]
    fn tc_155_rank_only_uses_minimal_candidate_and_preserves_highlight_score() {
        let root = PathBuf::from("親-Root");
        let mut paths = vec![
            root.join("日本語/Main-01.RS"),
            PathBuf::from("Outside/File.TXT"),
        ];
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            paths.push(root.join(std::ffi::OsStr::from_bytes(b"non-\xff.rs")));
        }
        paths.push(root.clone());
        let matcher = SkimMatcherV2::default();
        for (query, regex, minimal) in [
            ("Main", false, true),
            ("'Main !vendor", false, true),
            ("name:^Main|file$", false, true),
            ("name:Main '日本語", false, true),
            (r"name:main-[0-9]+\.rs", true, true),
            ("親-Root", false, true),
            ("dir:日本語 name:'Main", false, false),
            ("name:Main !path:'Outside", false, false),
            ("ext:'RS$", false, false),
        ] {
            for ignore_case in [false, true] {
                let compiled =
                    compile_query(query, regex, ignore_case).expect("compile parity query");
                for (root, prefer_relative) in [
                    (Some(root.as_path()), true),
                    (Some(root.as_path()), false),
                    (None, true),
                ] {
                    let ctx = SearchContext {
                        root,
                        prefer_relative,
                    };
                    for path in &paths {
                        let expected = compiled
                            .evaluate(
                                &compiled.prepare_candidate_with_kind(
                                    path,
                                    root,
                                    prefer_relative,
                                    Some(false),
                                ),
                                EvidenceLevel::WithSpans,
                            )
                            .map(|evaluation| evaluation.score);
                        let before = PreparedCandidate::full_preparation_count();
                        let direct = evaluate_candidate(path, 0, 0, &compiled, ctx, &matcher);
                        assert_eq!(direct.map(|item| item.score), expected, "{query} {path:?}");
                        for entry in [Entry::file(path.clone()), Entry::unknown(path.clone())] {
                            let result = evaluate_entry_candidate_with_kind_resolver(
                                &entry,
                                0,
                                0,
                                &compiled,
                                ctx,
                                &matcher,
                                || false,
                            );
                            assert_eq!(result.map(|item| item.score), expected, "{query} {path:?}");
                        }
                        if minimal {
                            assert_eq!(
                                PreparedCandidate::full_preparation_count(),
                                before,
                                "{query} prepared highlight or field projections while ranking"
                            );
                        }
                    }
                }
            }
        }
    }

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

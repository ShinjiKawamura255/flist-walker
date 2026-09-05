use std::path::Path;

mod compiled;

#[cfg(test)]
pub(crate) use compiled::{ignore_compile_count, query_compile_count, reset_compile_counts};
pub use compiled::{
    CompiledIgnoreTerms, CompiledQuery, EvidenceLevel, PreparedCandidate, QueryEvaluation,
    QueryOptions, QueryScope,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuerySpec {
    pub include_terms: Vec<QueryTerm>,
    pub exact_terms: Vec<QueryTerm>,
    pub exclude_terms: Vec<QueryTerm>,
    pub invalid_terms: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum QueryField {
    Any,
    Name,
    Path,
    Dir,
    Ext,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct QueryTerm {
    pub field: QueryField,
    pub value: String,
}

pub fn include_alternatives(term: &str) -> Vec<&str> {
    if !term.contains('|') {
        return vec![term];
    }
    let alts: Vec<&str> = term.split('|').filter(|s| !s.is_empty()).collect();
    if alts.is_empty() {
        vec![term]
    } else {
        alts
    }
}

pub fn split_anchor(term: &str) -> (bool, bool, &str) {
    let anchored_start = term.starts_with('^');
    let anchored_end = term.ends_with('$');

    let mut core = term;
    if anchored_start {
        core = core.strip_prefix('^').unwrap_or(core);
    }
    if anchored_end {
        core = core.strip_suffix('$').unwrap_or(core);
    }
    (anchored_start, anchored_end, core)
}

fn normalize_quoted_term(term: &str) -> String {
    if let Some(stripped) = term.strip_prefix("^'") {
        return format!("^{stripped}");
    }
    if let Some(stripped) = term.strip_prefix('\'') {
        return stripped.to_string();
    }
    term.to_string()
}

pub fn parse_include_alternative(candidate: &str) -> Option<(bool, String)> {
    if candidate.is_empty() {
        return None;
    }
    if let Some(stripped) = candidate.strip_prefix("^'") {
        if stripped.is_empty() {
            return None;
        }
        return Some((true, format!("^{stripped}")));
    }
    if let Some(stripped) = candidate.strip_prefix('\'') {
        if stripped.is_empty() {
            return None;
        }
        return Some((true, stripped.to_string()));
    }
    Some((false, candidate.to_string()))
}

pub fn parse_query(query: &str) -> QuerySpec {
    let mut include_terms = Vec::new();
    let mut exact_terms = Vec::new();
    let mut exclude_terms = Vec::new();
    let mut invalid_terms = Vec::new();

    for token in query.split_whitespace() {
        if token.is_empty() || token == "!" || token == "'" {
            continue;
        }
        if let Some(stripped) = token.strip_prefix('!') {
            if let Some(term) = parse_field_term(stripped, token, &mut invalid_terms) {
                exclude_terms.push(QueryTerm {
                    field: term.field,
                    value: normalize_quoted_term(&term.value),
                });
            }
            continue;
        }
        let Some(term) = parse_field_term(token, token, &mut invalid_terms) else {
            continue;
        };
        if term.value.contains('|') {
            include_terms.push(term);
            continue;
        }
        if term.value.starts_with('\'') || term.value.starts_with("^'") {
            let normalized = normalize_quoted_term(&term.value);
            if !normalized.is_empty() {
                exact_terms.push(QueryTerm {
                    field: term.field,
                    value: normalized,
                });
            }
        } else {
            include_terms.push(term);
        }
    }

    QuerySpec {
        include_terms,
        exact_terms,
        exclude_terms,
        invalid_terms,
    }
}

fn parse_field_term(
    token: &str,
    original: &str,
    invalid_terms: &mut Vec<String>,
) -> Option<QueryTerm> {
    let (field, value) = if let Some(value) = token.strip_prefix("name:") {
        (QueryField::Name, value)
    } else if let Some(value) = token.strip_prefix("path:") {
        (QueryField::Path, value)
    } else if let Some(value) = token.strip_prefix("dir:") {
        (QueryField::Dir, value)
    } else if let Some(value) = token.strip_prefix("ext:") {
        (QueryField::Ext, value)
    } else {
        (QueryField::Any, token)
    };
    if field != QueryField::Any && value.is_empty() {
        invalid_terms.push(original.to_string());
        return None;
    }
    (!value.is_empty()).then(|| QueryTerm {
        field,
        value: value.to_string(),
    })
}

pub fn token_uses_regex_syntax(token: &str) -> bool {
    token.chars().any(|ch| {
        matches!(
            ch,
            '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '\\'
        )
    })
}

pub fn has_visible_match(
    path: &Path,
    root: &Path,
    query: &str,
    prefer_relative: bool,
    ignore_case: bool,
) -> bool {
    if query.trim().is_empty() {
        return true;
    }
    let Ok(compiled) = CompiledQuery::compile(
        query,
        QueryOptions {
            use_regex: false,
            ignore_case,
        },
    ) else {
        return false;
    };
    let prepared = compiled.prepare_candidate(path, Some(root), prefer_relative);
    compiled.matches_positive_projection(&prepared)
}

pub fn path_matches_ignore_terms(
    path: &Path,
    root: &Path,
    ignore_terms: &[String],
    prefer_relative: bool,
    ignore_case: bool,
) -> bool {
    if ignore_terms.is_empty() {
        return false;
    }
    let compiled = CompiledIgnoreTerms::compile(ignore_terms, ignore_case);
    compiled.matches_path(
        path,
        QueryScope {
            root: Some(root),
            prefer_relative,
            ignore_case,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::{
        has_visible_match, parse_include_alternative, parse_query, path_matches_ignore_terms,
        query_compile_count, reset_compile_counts, split_anchor, token_uses_regex_syntax,
        CompiledQuery, EvidenceLevel, QueryField, QueryOptions, QuerySpec, QueryTerm,
    };
    use std::path::PathBuf;

    #[test]
    fn parse_query_preserves_existing_token_buckets() {
        let spec = parse_query("main 'file !readme abc|'xyz ^foo");

        assert_eq!(
            spec,
            QuerySpec {
                include_terms: vec![
                    QueryTerm {
                        field: QueryField::Any,
                        value: "main".to_string()
                    },
                    QueryTerm {
                        field: QueryField::Any,
                        value: "abc|'xyz".to_string()
                    },
                    QueryTerm {
                        field: QueryField::Any,
                        value: "^foo".to_string()
                    },
                ],
                exact_terms: vec![QueryTerm {
                    field: QueryField::Any,
                    value: "file".to_string()
                }],
                exclude_terms: vec![QueryTerm {
                    field: QueryField::Any,
                    value: "readme".to_string()
                }],
                invalid_terms: Vec::new(),
            }
        );
    }

    #[test]
    fn parse_include_alternative_keeps_exact_marker_information() {
        assert_eq!(
            parse_include_alternative("'main"),
            Some((true, "main".to_string()))
        );
        assert_eq!(
            parse_include_alternative("^'main"),
            Some((true, "^main".to_string()))
        );
        assert_eq!(
            parse_include_alternative("^main"),
            Some((false, "^main".to_string()))
        );
    }

    #[test]
    fn split_anchor_extracts_core_text() {
        assert_eq!(split_anchor("^main$"), (true, true, "main"));
        assert_eq!(split_anchor("^main"), (true, false, "main"));
        assert_eq!(split_anchor("main$"), (false, true, "main"));
    }

    #[test]
    fn token_uses_regex_syntax_is_conservative_for_regex_metacharacters() {
        assert!(!token_uses_regex_syntax("abc"));
        assert!(!token_uses_regex_syntax("^main$"));
        assert!(!token_uses_regex_syntax("foo|bar"));
        assert!(token_uses_regex_syntax("ma.*py"));
        assert!(token_uses_regex_syntax("foo[0-9]+"));
        assert!(token_uses_regex_syntax(r"foo\.bar"));
    }

    #[test]
    fn ignore_terms_use_literal_exclusion_matching_without_fuzzy_fallback() {
        let root = PathBuf::from("/tmp/root");
        let ignored = PathBuf::from("/tmp/root/build/old-cache.txt");
        let fuzzy_only = PathBuf::from("/tmp/root/build/o-l-d-cache.txt");
        let kept = PathBuf::from("/tmp/root/build/new-cache.txt");
        let terms = vec!["old".to_string(), "~".to_string()];

        assert!(path_matches_ignore_terms(
            &ignored, &root, &terms, true, true
        ));
        assert!(!path_matches_ignore_terms(
            &fuzzy_only,
            &root,
            &terms,
            true,
            true
        ));
        assert!(!path_matches_ignore_terms(&kept, &root, &terms, true, true));
    }

    #[test]
    fn ignore_terms_respect_ignore_case_flag() {
        let root = PathBuf::from("/tmp/root");
        let upper = PathBuf::from("/tmp/root/build/Old-cache.txt");
        let terms = vec!["old".to_string()];

        assert!(path_matches_ignore_terms(&upper, &root, &terms, true, true));
        assert!(!path_matches_ignore_terms(
            &upper, &root, &terms, true, false
        ));
    }

    #[test]
    fn tc_176_ignore_terms_match_across_path_separator_styles() {
        let root = PathBuf::from("/tmp/root");
        let ignored = root.join("src/generated/cache.txt");

        assert!(path_matches_ignore_terms(
            &ignored,
            &root,
            &[r"src\generated".to_string()],
            true,
            true,
        ));
    }

    #[test]
    fn tc_155_ignore_terms_preserve_literal_quote_behavior() {
        let root = PathBuf::from("/tmp/root");
        let terms = vec!["'old".to_string()];
        assert!(path_matches_ignore_terms(
            &root.join("build/'old-cache.txt"),
            &root,
            &terms,
            true,
            true,
        ));
        assert!(!path_matches_ignore_terms(
            &root.join("build/old-cache.txt"),
            &root,
            &terms,
            true,
            true,
        ));
    }

    #[test]
    fn visible_match_repeated_exact_tokens_require_repeated_literal_occurrences() {
        let root = PathBuf::from("/tmp/root");
        assert!(!has_visible_match(
            &root.join("abc.txt"),
            &root,
            "'abc 'abc",
            true,
            true
        ));
        assert!(has_visible_match(
            &root.join("abc-abc.txt"),
            &root,
            "'abc 'abc",
            true,
            true
        ));
        assert!(has_visible_match(
            &root.join("abc/child-abc.txt"),
            &root,
            "'abc 'abc",
            true,
            true
        ));
    }

    #[test]
    fn tc_155_regression_visible_match_remains_a_positive_term_projection() {
        let root = PathBuf::from("/tmp/root");
        let path = root.join("src/main.py");

        assert!(has_visible_match(&path, &root, "main !src", true, true));
        assert!(!has_visible_match(&path, &root, "main zzzz", true, true));
    }

    #[test]
    fn tc_155_compiled_query_supplies_visibility_score_and_multibyte_spans() {
        reset_compile_counts();
        let root = PathBuf::from("/tmp/root");
        let path = root.join("日本語/テスト-main.rs");
        let compiled = CompiledQuery::compile(
            "テスト 'main !vendor",
            QueryOptions {
                use_regex: false,
                ignore_case: true,
            },
        )
        .expect("compile query");
        let prepared = compiled.prepare_candidate(&path, Some(&root), true);

        let ranked = compiled
            .evaluate(&prepared, EvidenceLevel::RankOnly)
            .expect("rank match");
        assert!(ranked.score.is_finite());
        assert!(ranked.spans.is_empty());

        let highlighted = compiled
            .evaluate(&prepared, EvidenceLevel::WithSpans)
            .expect("highlight match");
        let visible: Vec<char> = prepared.visible_text().chars().collect();
        let highlighted_text: String = highlighted
            .spans
            .iter()
            .filter_map(|index| visible.get(*index))
            .collect();
        assert!(highlighted_text.contains("テスト"));
        assert!(highlighted_text.contains("main"));
        assert_eq!(query_compile_count(), 1);
    }

    #[test]
    fn tc_175_field_terms_filter_name_path_dir_and_extension_independently() {
        let root = PathBuf::from("/tmp/root");
        let path = root.join("src/config/archive.tar.gz");

        for query in [
            "name:archive",
            "path:^src/",
            "dir:config",
            "ext:gz",
            "ext:rs|gz",
            "name:'archive",
        ] {
            let compiled = CompiledQuery::compile(
                query,
                QueryOptions {
                    use_regex: false,
                    ignore_case: true,
                },
            )
            .unwrap_or_else(|error| panic!("compile {query}: {error}"));
            let candidate = compiled.prepare_candidate(&path, Some(&root), true);
            assert!(
                compiled
                    .evaluate(&candidate, EvidenceLevel::RankOnly)
                    .is_some(),
                "query should match: {query}"
            );
        }

        for query in ["name:src", "dir:archive", "ext:tar", "!dir:config"] {
            let compiled = CompiledQuery::compile(
                query,
                QueryOptions {
                    use_regex: false,
                    ignore_case: true,
                },
            )
            .unwrap_or_else(|error| panic!("compile {query}: {error}"));
            let candidate = compiled.prepare_candidate(&path, Some(&root), true);
            assert!(
                compiled
                    .evaluate(&candidate, EvidenceLevel::RankOnly)
                    .is_none(),
                "query should not match: {query}"
            );
        }
    }

    #[test]
    fn tc_175_unknown_prefix_stays_unscoped_and_empty_known_field_is_invalid() {
        let root = PathBuf::from("/tmp/root");
        let path = root.join("docs/tag:value.txt");
        let compiled = CompiledQuery::compile(
            "tag:value",
            QueryOptions {
                use_regex: false,
                ignore_case: true,
            },
        )
        .expect("unknown prefix remains a normal query term");
        let candidate = compiled.prepare_candidate(&path, Some(&root), true);
        assert!(compiled
            .evaluate(&candidate, EvidenceLevel::RankOnly)
            .is_some());

        for query in ["name:", "!path:", "dir:", "ext:"] {
            assert!(
                CompiledQuery::compile(
                    query,
                    QueryOptions {
                        use_regex: false,
                        ignore_case: true,
                    },
                )
                .is_err(),
                "empty known field should fail: {query}"
            );
        }
    }

    #[test]
    fn tc_175_extension_uses_only_the_final_file_suffix_and_excludes_directories() {
        let root = PathBuf::from("/tmp/root");
        let compiled = CompiledQuery::compile(
            "ext:gz",
            QueryOptions {
                use_regex: false,
                ignore_case: true,
            },
        )
        .expect("compile extension query");
        let file = compiled.prepare_candidate_with_kind(
            &root.join("archive.tar.gz"),
            Some(&root),
            true,
            Some(false),
        );
        let directory = compiled.prepare_candidate_with_kind(
            &root.join("archive.gz"),
            Some(&root),
            true,
            Some(true),
        );
        assert!(compiled.evaluate(&file, EvidenceLevel::RankOnly).is_some());
        assert!(compiled
            .evaluate(&directory, EvidenceLevel::RankOnly)
            .is_none());

        let tar = CompiledQuery::compile(
            "ext:tar",
            QueryOptions {
                use_regex: false,
                ignore_case: true,
            },
        )
        .expect("compile extension query");
        let file = tar.prepare_candidate_with_kind(
            &root.join("archive.tar.gz"),
            Some(&root),
            true,
            Some(false),
        );
        assert!(tar.evaluate(&file, EvidenceLevel::RankOnly).is_none());

        let dotfile =
            compiled.prepare_candidate_with_kind(&root.join(".gz"), Some(&root), true, Some(false));
        assert!(compiled
            .evaluate(&dotfile, EvidenceLevel::RankOnly)
            .is_none());
    }

    #[test]
    fn tc_175_regex_is_compiled_from_the_field_value() {
        let root = PathBuf::from("/tmp/root");
        let path = root.join("src/Archive-01.RS");
        let compiled = CompiledQuery::compile(
            r"name:^archive-[0-9]+\.rs$ ext:^rs$",
            QueryOptions {
                use_regex: true,
                ignore_case: true,
            },
        )
        .expect("compile field regex");
        let candidate = compiled.prepare_candidate_with_kind(&path, Some(&root), true, Some(false));
        assert!(compiled
            .evaluate(&candidate, EvidenceLevel::WithSpans)
            .is_some());
    }

    #[test]
    fn tc_175_field_highlights_map_back_to_the_visible_path() {
        let root = PathBuf::from("/tmp/root");
        let path = root.join("日本語/config-main.rs");
        let compiled = CompiledQuery::compile(
            "dir:日本語 ext:rs name:main",
            QueryOptions {
                use_regex: false,
                ignore_case: true,
            },
        )
        .expect("compile field query");
        let candidate = compiled.prepare_candidate(&path, Some(&root), true);
        let result = compiled
            .evaluate(&candidate, EvidenceLevel::WithSpans)
            .expect("field query match");
        let visible = candidate.visible_text().chars().collect::<Vec<_>>();
        let highlighted = result
            .spans
            .iter()
            .filter_map(|position| visible.get(*position))
            .collect::<String>();
        assert!(highlighted.contains("日本語"), "{highlighted}");
        assert!(highlighted.contains("main"), "{highlighted}");
        assert!(highlighted.contains("rs"), "{highlighted}");
    }

    fn alignment_evaluate(query: &str, regex: bool, relative: bool, child: &str) -> Option<String> {
        let root = PathBuf::from("/tmp/親-root");
        let compiled = CompiledQuery::compile(
            query,
            QueryOptions {
                use_regex: regex,
                ignore_case: true,
            },
        )
        .unwrap_or_else(|error| panic!("{query}: {error}"));
        let candidate = compiled.prepare_candidate_with_kind(
            &root.join(child),
            Some(&root),
            relative,
            Some(false),
        );
        compiled
            .evaluate(&candidate, EvidenceLevel::WithSpans)
            .map(|result| {
                let chars: Vec<_> = candidate.visible_text().chars().collect();
                result.spans.iter().map(|&index| chars[index]).collect()
            })
    }

    #[test]
    fn alignment_regex_or_retains_exact_alternatives_and_discards_empty_ones() {
        for query in ["'foo|b.r", "b.r|'foo", "|'foo||b.r|"] {
            assert_eq!(
                alignment_evaluate(query, true, true, "foo.txt"),
                Some("foo".into()),
                "{query}"
            );
            assert!(
                alignment_evaluate(query, true, true, "f-o-o.txt").is_none(),
                "{query}"
            );
        }
        for query in ["foo.*|", "|foo.*", "||foo.*||"] {
            assert!(
                alignment_evaluate(query, true, true, "unrelated.txt").is_none(),
                "{query}"
            );
            assert!(
                alignment_evaluate(query, true, true, "foobar.txt").is_some(),
                "{query}"
            );
        }
        assert_eq!(
            alignment_evaluate("'a.b|c.*", true, true, "a.b.txt"),
            Some("a.b".into())
        );
        assert!(alignment_evaluate("'a.b|c.*", true, true, "axb.txt").is_none());
        assert_eq!(
            alignment_evaluate("^'日本語$|z.*", true, true, "日本語"),
            Some("日本語".into())
        );
    }

    #[test]
    fn alignment_regex_or_preserves_nested_classes_escaped_pipes_and_flags() {
        for (query, child) in [
            (r"(foo|bar)[.]txt|'baz", "bar.txt"),
            (r"[a|b][.]txt|'baz", "a.txt"),
            (r"[a-z&&[^q|r]][.]txt|'baz", "b.txt"),
            (r"[]|][.]txt|'baz", "].txt"),
            (r"[^^][.]txt|'baz", "b.txt"),
            (r"(?-i)foo|'BAR", "bar"),
            (r"foo\|bar|'baz", "foo|bar"),
            (r"(?i)foo|BAR|'baz", "bar"),
            (r"'(left|right.*", "(left.txt"),
            (r"'[left|right.*", "[left.txt"),
        ] {
            assert!(
                alignment_evaluate(query, true, true, child).is_some(),
                "{query}: {child}"
            );
        }
        for query in ["(foo|bar", "foo.*|[", "foo.*|bar\\"] {
            assert!(
                CompiledQuery::compile(
                    query,
                    QueryOptions {
                        use_regex: true,
                        ignore_case: true
                    }
                )
                .is_err(),
                "{query}"
            );
        }
        assert!(alignment_evaluate("'ä|z.*", true, true, "Ä.txt").is_none());
        // Exclusions remain literal even in regex mode.
        assert!(alignment_evaluate("foo !b.r", true, true, "foo-bar.txt").is_some());
        assert!(alignment_evaluate("foo !b.r", true, true, "foo-b.r.txt").is_none());
    }

    #[test]
    fn alignment_field_paths_ignore_display_mode_and_map_multibyte_spans() {
        for relative in [true, false] {
            for query in [
                "path:^資料/",
                "dir:^資料$",
                "path:'資料/",
                "path:^資料/[a-z]+[.]rs$",
            ] {
                let regex = query.contains('[');
                let highlighted =
                    alignment_evaluate(query, regex, relative, "資料/main.rs").expect(query);
                assert!(highlighted.starts_with("資料"), "{query}: {highlighted}");
                assert!(!highlighted.contains("親-root"), "{query}: {highlighted}");
            }
            assert!(alignment_evaluate("!dir:親-root", false, relative, "資料/main.rs").is_some());
            assert!(alignment_evaluate("!dir:'資料", false, relative, "資料/main.rs").is_none());
            assert!(alignment_evaluate("dir:親-root", false, relative, "main.rs").is_none());
        }
    }

    #[test]
    fn alignment_literal_field_paths_normalize_both_separator_styles() {
        for relative in [true, false] {
            for query in [
                r"path:資料\main",
                r"path:'資料\main",
                r"path:nope|'資料\main",
                r"dir:資料\sub",
            ] {
                let child = if query.starts_with("dir:") {
                    "資料/sub/main.rs"
                } else {
                    "資料/main.rs"
                };
                assert!(
                    alignment_evaluate(query, false, relative, child).is_some(),
                    "{query}"
                );
            }
            for query in [r"!path:資料\main", r"!path:'資料\main", r"!dir:資料\sub"] {
                let child = if query.starts_with("!dir:") {
                    "資料/sub/main.rs"
                } else {
                    "資料/main.rs"
                };
                assert!(
                    alignment_evaluate(query, false, relative, child).is_none(),
                    "{query}"
                );
            }
            assert!(
                alignment_evaluate("!path:資料/main", false, relative, "資料/sub/main.rs")
                    .is_some()
            );
            assert!(
                alignment_evaluate(r"path:^資料/\w+\.rs$", true, relative, "資料/main.rs")
                    .is_some()
            );
            assert!(
                alignment_evaluate(r"path:'資料\main|no.*", true, relative, "資料/main.rs")
                    .is_some()
            );
        }
    }
}

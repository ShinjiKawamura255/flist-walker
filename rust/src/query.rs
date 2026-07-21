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
    pub include_terms: Vec<String>,
    pub exact_terms: Vec<String>,
    pub exclude_terms: Vec<String>,
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

    for token in query.split_whitespace() {
        if token.is_empty() || token == "!" || token == "'" {
            continue;
        }
        if let Some(stripped) = token.strip_prefix('!') {
            if !stripped.is_empty() {
                exclude_terms.push(normalize_quoted_term(stripped));
            }
            continue;
        }
        if token.contains('|') {
            include_terms.push(token.to_string());
            continue;
        }
        if token.starts_with('\'') || token.starts_with("^'") {
            let normalized = normalize_quoted_term(token);
            if !normalized.is_empty() {
                exact_terms.push(normalized);
            }
        } else {
            include_terms.push(token.to_string());
        }
    }

    QuerySpec {
        include_terms,
        exact_terms,
        exclude_terms,
    }
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
        CompiledQuery, EvidenceLevel, QueryOptions, QuerySpec,
    };
    use std::path::PathBuf;

    #[test]
    fn parse_query_preserves_existing_token_buckets() {
        let spec = parse_query("main 'file !readme abc|'xyz ^foo");

        assert_eq!(
            spec,
            QuerySpec {
                include_terms: vec![
                    "main".to_string(),
                    "abc|'xyz".to_string(),
                    "^foo".to_string(),
                ],
                exact_terms: vec!["file".to_string()],
                exclude_terms: vec!["readme".to_string()],
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
}

use std::collections::HashSet;
use std::path::Path;

use crate::query::{CompiledQuery, QueryOptions};

pub fn match_positions_for_path(
    path: &Path,
    root: &Path,
    query: &str,
    prefer_relative: bool,
    use_regex: bool,
    ignore_case: bool,
) -> HashSet<usize> {
    let Ok(compiled) = CompiledQuery::compile(
        query,
        QueryOptions {
            use_regex,
            ignore_case,
        },
    ) else {
        return HashSet::new();
    };
    match_positions_for_path_with_compiled(path, root, &compiled, prefer_relative)
}

pub fn match_positions_for_path_with_compiled(
    path: &Path,
    root: &Path,
    compiled: &CompiledQuery,
    prefer_relative: bool,
) -> HashSet<usize> {
    let prepared = compiled.prepare_candidate(path, Some(root), prefer_relative);
    compiled
        .positive_projection_spans(&prepared)
        .into_iter()
        .collect()
}

pub fn has_visible_match(
    path: &Path,
    root: &Path,
    query: &str,
    prefer_relative: bool,
    ignore_case: bool,
) -> bool {
    crate::query::has_visible_match(path, root, query, prefer_relative, ignore_case)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_model::display_path_with_mode;
    use std::path::PathBuf;

    #[test]
    fn match_positions_ascii_query_work_with_multibyte_path() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/日本語/docs/readme.txt");
        let positions = match_positions_for_path(&path, &root, "read", true, false, true);
        assert!(!positions.is_empty());
    }

    #[test]
    fn match_positions_multibyte_query_only_highlights_matched_chars() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/日本語/テスト資料.txt");
        let positions = match_positions_for_path(&path, &root, "テスト", true, false, true);
        let display = display_path_with_mode(&path, &root, true);
        let chars: Vec<char> = display.chars().collect();
        let highlighted: String = chars
            .iter()
            .enumerate()
            .filter_map(|(idx, ch)| positions.contains(&idx).then_some(*ch))
            .collect();
        assert_eq!(highlighted, "テスト");
    }

    #[test]
    fn match_positions_ignore_exclusion_token_for_highlight() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/main.py");
        let positions = match_positions_for_path(&path, &root, "main !readme", true, false, true);
        assert!(positions.len() >= 4);
    }

    #[test]
    fn tc_155_regression_highlight_remains_a_partial_positive_projection() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/main.py");

        assert!(match_positions_for_path(&path, &root, "main !src", true, false, true).len() >= 4);
        assert!(match_positions_for_path(&path, &root, "main zzzz", true, false, true).len() >= 4);
    }

    #[test]
    fn match_positions_support_exact_token_prefix() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/main.py");
        let positions = match_positions_for_path(&path, &root, "'main", true, false, true);
        assert!(positions.len() >= 4);
    }

    #[test]
    fn exact_token_does_not_fall_back_to_subsequence_matching() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/m-a-i-n.txt");
        let positions = match_positions_for_path(&path, &root, "'main", true, false, true);
        assert!(positions.is_empty());
        assert!(!has_visible_match(&path, &root, "'main", true, true));
    }

    #[test]
    fn has_visible_match_false_when_term_not_in_visible_text() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/main.py");
        assert!(!has_visible_match(&path, &root, "zzzz", true, true));
    }

    #[test]
    fn case_sensitive_highlight_and_visibility_respect_ignore_case_flag() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/Main.py");
        let sensitive = match_positions_for_path(&path, &root, "main", true, false, false);
        assert!(sensitive.is_empty());
        assert!(!has_visible_match(&path, &root, "main", true, false));
        let insensitive = match_positions_for_path(&path, &root, "main", true, false, true);
        assert!(!insensitive.is_empty());
        assert!(has_visible_match(&path, &root, "main", true, true));
    }

    #[test]
    fn has_visible_match_true_for_exclusion_only_query() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/main.py");
        assert!(has_visible_match(&path, &root, "!readme", true, true));
    }

    #[test]
    fn match_positions_regex_query_highlights_matched_span() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/main.py");
        assert!(!match_positions_for_path(&path, &root, "ma.*py", true, true, true).is_empty());
    }

    #[test]
    fn match_positions_regex_mode_plain_token_uses_fuzzy_highlight() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/a-b-c.txt");
        assert!(!match_positions_for_path(&path, &root, "abc", true, true, true).is_empty());
    }

    #[test]
    fn match_positions_regex_mode_plain_or_token_uses_fuzzy_highlight() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/f-o-o.txt");
        assert!(!match_positions_for_path(&path, &root, "abc|foo", true, true, true).is_empty());
    }

    #[test]
    fn match_positions_or_token_highlights_selected_alternative() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/foo.txt");
        assert!(
            !match_positions_for_path(&path, &root, "abc|foo|bar", true, false, true).is_empty()
        );
    }

    #[test]
    fn match_positions_or_token_with_left_exact_keeps_left_candidate() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/main.txt");
        assert!(!match_positions_for_path(&path, &root, "'main|", true, false, true).is_empty());
        assert!(has_visible_match(&path, &root, "'main|", true, true));
    }

    #[test]
    fn match_positions_or_token_supports_exact_on_right_side() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/xyz.txt");
        assert!(!match_positions_for_path(&path, &root, "abc|'xyz", true, false, true).is_empty());
    }

    #[test]
    fn exact_alternative_in_or_query_does_not_fall_back_to_subsequence_matching() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/m-a-i-n.txt");
        assert!(match_positions_for_path(&path, &root, "abc|'main", true, false, true).is_empty());
        assert!(!has_visible_match(&path, &root, "abc|'main", true, true));
    }

    #[test]
    fn has_visible_match_or_token_uses_alternative_hits() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/bar.txt");
        assert!(has_visible_match(&path, &root, "abc|foo|bar", true, true));
    }
}

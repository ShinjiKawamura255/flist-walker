use std::collections::HashSet;
use std::path::Path;

use regex::RegexBuilder;

use crate::query::{
    include_alternatives, parse_include_alternative, parse_query, split_anchor,
    token_uses_regex_syntax,
};

use super::display_path_with_mode;
fn chars_equal(a: char, b: char, ignore_case: bool) -> bool {
    if ignore_case && a.is_ascii() && b.is_ascii() {
        a.eq_ignore_ascii_case(&b)
    } else {
        a == b
    }
}

fn find_fuzzy_match_positions(text: &str, query: &str, ignore_case: bool) -> HashSet<usize> {
    let mut out = HashSet::new();
    if query.is_empty() {
        return out;
    }

    let text_chars: Vec<char> = text.chars().collect();
    let q_chars: Vec<char> = query.chars().collect();
    if q_chars.is_empty() {
        return out;
    }

    if q_chars.len() <= text_chars.len() {
        for start in 0..=text_chars.len() - q_chars.len() {
            if q_chars
                .iter()
                .enumerate()
                .all(|(offset, q)| chars_equal(text_chars[start + offset], *q, ignore_case))
            {
                for i in start..start + q_chars.len() {
                    out.insert(i);
                }
                return out;
            }
        }
    }

    let mut qi = 0usize;
    for (i, ch) in text_chars.iter().enumerate() {
        if qi < q_chars.len() && chars_equal(*ch, q_chars[qi], ignore_case) {
            out.insert(i);
            qi += 1;
        }
    }
    if qi == q_chars.len() {
        out
    } else {
        HashSet::new()
    }
}

fn exact_candidate_positions(text: &str, candidate: &str, ignore_case: bool) -> HashSet<usize> {
    let mut out = HashSet::new();
    let (anchored_start, anchored_end, core) = split_anchor(candidate);
    if core.is_empty() {
        return out;
    }

    let text_chars: Vec<char> = text.chars().collect();
    let core_chars: Vec<char> = core.chars().collect();
    if core_chars.len() > text_chars.len() {
        return out;
    }

    for start in 0..=text_chars.len() - core_chars.len() {
        if !core_chars
            .iter()
            .enumerate()
            .all(|(offset, query)| chars_equal(text_chars[start + offset], *query, ignore_case))
        {
            continue;
        }
        if anchored_start && start != 0 {
            continue;
        }
        if anchored_end && start + core_chars.len() != text_chars.len() {
            continue;
        }

        for idx in start..start + core_chars.len() {
            out.insert(idx);
        }
        return out;
    }

    out
}

fn exact_term_positions(text: &str, term: &str, ignore_case: bool) -> HashSet<usize> {
    for candidate in include_alternatives(term) {
        let positions = exact_candidate_positions(text, candidate, ignore_case);
        if !positions.is_empty() {
            return positions;
        }
    }
    HashSet::new()
}

fn include_candidate_positions(text: &str, candidate: &str, ignore_case: bool) -> HashSet<usize> {
    let Some((exact, candidate)) = parse_include_alternative(candidate) else {
        return HashSet::new();
    };
    if exact {
        return exact_candidate_positions(text, &candidate, ignore_case);
    }

    let (anchored_start, anchored_end, core) = split_anchor(&candidate);
    if core.is_empty() {
        return HashSet::new();
    }
    if anchored_start {
        let Some(first_char) = core.chars().next() else {
            return HashSet::new();
        };
        if !text
            .chars()
            .next()
            .is_some_and(|value| chars_equal(value, first_char, ignore_case))
        {
            return HashSet::new();
        }
    }
    if anchored_end {
        let Some(last_char) = core.chars().last() else {
            return HashSet::new();
        };
        if !text
            .chars()
            .last()
            .is_some_and(|value| chars_equal(value, last_char, ignore_case))
        {
            return HashSet::new();
        }
    }

    find_fuzzy_match_positions(text, core, ignore_case)
}

pub fn match_positions_for_path(
    path: &Path,
    root: &Path,
    query: &str,
    prefer_relative: bool,
    use_regex: bool,
    ignore_case: bool,
) -> HashSet<usize> {
    let mut positions = HashSet::new();
    let display = display_path_with_mode(path, root, prefer_relative);
    let filename = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let start = display
        .chars()
        .count()
        .saturating_sub(filename.chars().count());

    let spec = parse_query(query);

    for term in &spec.exact_terms {
        let hits = exact_term_positions(filename, term, ignore_case);
        if !hits.is_empty() {
            for pos in hits {
                positions.insert(start + pos);
            }
            continue;
        }
        let hits = exact_term_positions(&display, term, ignore_case);
        if !hits.is_empty() {
            positions.extend(hits);
        }
    }

    for term in &spec.include_terms {
        if use_regex && token_uses_regex_syntax(term) {
            let hits = find_regex_match_positions(filename, term, ignore_case);
            if !hits.is_empty() {
                for pos in hits {
                    positions.insert(start + pos);
                }
                continue;
            }
            positions.extend(find_regex_match_positions(&display, term, ignore_case));
            continue;
        }

        let mut matched_any = false;
        for candidate in include_alternatives(term) {
            let hits = include_candidate_positions(filename, candidate, ignore_case);
            if !hits.is_empty() {
                for pos in hits {
                    positions.insert(start + pos);
                }
                matched_any = true;
                break;
            }
            let hits = include_candidate_positions(&display, candidate, ignore_case);
            if !hits.is_empty() {
                positions.extend(hits);
                matched_any = true;
                break;
            }
        }
        if matched_any {
            continue;
        }
    }
    positions
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

fn find_regex_match_positions(text: &str, pattern: &str, ignore_case: bool) -> HashSet<usize> {
    let mut out = HashSet::new();
    let Ok(re) = RegexBuilder::new(pattern)
        .case_insensitive(ignore_case)
        .build()
    else {
        return out;
    };
    for mat in re.find_iter(text) {
        if mat.start() == mat.end() {
            continue;
        }
        let start = text[..mat.start()].chars().count();
        let len = text[mat.start()..mat.end()].chars().count();
        for idx in start..start + len {
            out.insert(idx);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("fff-rs-ui-{name}-{nonce}"))
    }

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
        let root = test_root("highlight-exclusion");
        let sample = root.join("src/main.py");
        fs::create_dir_all(sample.parent().expect("parent")).expect("create parent");
        fs::write(&sample, "print('x')\n").expect("write sample");

        let positions = match_positions_for_path(&sample, &root, "main !readme", true, false, true);
        assert!(positions.len() >= 4);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn match_positions_support_exact_token_prefix() {
        let root = test_root("highlight-exact");
        let sample = root.join("src/main.py");
        fs::create_dir_all(sample.parent().expect("parent")).expect("create parent");
        fs::write(&sample, "print('x')\n").expect("write sample");

        let positions = match_positions_for_path(&sample, &root, "'main", true, false, true);
        assert!(positions.len() >= 4);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn exact_token_does_not_fall_back_to_subsequence_matching() {
        let root = test_root("highlight-exact-no-fuzzy");
        let sample = root.join("src/m-a-i-n.txt");
        fs::create_dir_all(sample.parent().expect("parent")).expect("create parent");
        fs::write(&sample, "print('x')\n").expect("write sample");

        let positions = match_positions_for_path(&sample, &root, "'main", true, false, true);
        assert!(positions.is_empty());
        assert!(!has_visible_match(&sample, &root, "'main", true, true));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn has_visible_match_false_when_term_not_in_visible_text() {
        let root = test_root("visible-match");
        let sample = root.join("src/main.py");
        fs::create_dir_all(sample.parent().expect("parent")).expect("create parent");
        fs::write(&sample, "print('x')\n").expect("write sample");

        assert!(!has_visible_match(&sample, &root, "zzzz", true, true));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn case_sensitive_highlight_and_visibility_respect_ignore_case_flag() {
        let root = test_root("visible-case-sensitive");
        let sample = root.join("src/Main.py");
        fs::create_dir_all(sample.parent().expect("parent")).expect("create parent");
        fs::write(&sample, "print('x')\n").expect("write sample");

        let sensitive_positions =
            match_positions_for_path(&sample, &root, "main", true, false, false);
        assert!(sensitive_positions.is_empty());
        assert!(!has_visible_match(&sample, &root, "main", true, false));

        let insensitive_positions =
            match_positions_for_path(&sample, &root, "main", true, false, true);
        assert!(!insensitive_positions.is_empty());
        assert!(has_visible_match(&sample, &root, "main", true, true));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn has_visible_match_true_for_exclusion_only_query() {
        let root = test_root("visible-exclusion-only");
        let sample = root.join("src/main.py");
        fs::create_dir_all(sample.parent().expect("parent")).expect("create parent");
        fs::write(&sample, "print('x')\n").expect("write sample");

        assert!(has_visible_match(&sample, &root, "!readme", true, true));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn match_positions_regex_query_highlights_matched_span() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/main.py");
        let positions = match_positions_for_path(&path, &root, "ma.*py", true, true, true);
        assert!(!positions.is_empty());
    }

    #[test]
    fn match_positions_regex_mode_plain_token_uses_fuzzy_highlight() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/a-b-c.txt");
        let positions = match_positions_for_path(&path, &root, "abc", true, true, true);
        assert!(!positions.is_empty());
    }

    #[test]
    fn match_positions_regex_mode_plain_or_token_uses_fuzzy_highlight() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/f-o-o.txt");
        let positions = match_positions_for_path(&path, &root, "abc|foo", true, true, true);
        assert!(!positions.is_empty());
    }

    #[test]
    fn match_positions_or_token_highlights_selected_alternative() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/foo.txt");
        let positions = match_positions_for_path(&path, &root, "abc|foo|bar", true, false, true);
        assert!(!positions.is_empty());
    }

    #[test]
    fn match_positions_or_token_with_left_exact_keeps_left_candidate() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/main.txt");
        let positions = match_positions_for_path(&path, &root, "'main|", true, false, true);
        assert!(!positions.is_empty());
        assert!(has_visible_match(&path, &root, "'main|", true, true));
    }

    #[test]
    fn match_positions_or_token_supports_exact_on_right_side() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/xyz.txt");
        let positions = match_positions_for_path(&path, &root, "abc|'xyz", true, false, true);
        assert!(!positions.is_empty());
    }

    #[test]
    fn exact_alternative_in_or_query_does_not_fall_back_to_subsequence_matching() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/m-a-i-n.txt");
        let positions = match_positions_for_path(&path, &root, "abc|'main", true, false, true);
        assert!(positions.is_empty());
        assert!(!has_visible_match(&path, &root, "abc|'main", true, true));
    }

    #[test]
    fn has_visible_match_or_token_uses_alternative_hits() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/bar.txt");
        assert!(has_visible_match(&path, &root, "abc|foo|bar", true, true));
    }
}

use crate::path_utils::{display_path_with_mode, normalize_windows_path};
use std::path::Path;

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

fn chars_equal(a: char, b: char, ignore_case: bool) -> bool {
    if ignore_case && a.is_ascii() && b.is_ascii() {
        a.eq_ignore_ascii_case(&b)
    } else {
        a == b
    }
}

fn exact_candidate_matches_text(text: &str, candidate: &str, ignore_case: bool) -> bool {
    let (anchored_start, anchored_end, core) = split_anchor(candidate);
    if core.is_empty() {
        return false;
    }

    let text_chars: Vec<char> = text.chars().collect();
    let core_chars: Vec<char> = core.chars().collect();
    if core_chars.len() > text_chars.len() {
        return false;
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
        return true;
    }

    false
}

fn fuzzy_candidate_matches_text(text: &str, candidate: &str, ignore_case: bool) -> bool {
    let (anchored_start, anchored_end, core) = split_anchor(candidate);
    if core.is_empty() {
        return false;
    }

    if anchored_start {
        let Some(first_char) = core.chars().next() else {
            return false;
        };
        if !text
            .chars()
            .next()
            .is_some_and(|value| chars_equal(value, first_char, ignore_case))
        {
            return false;
        }
    }
    if anchored_end {
        let Some(last_char) = core.chars().last() else {
            return false;
        };
        if !text
            .chars()
            .last()
            .is_some_and(|value| chars_equal(value, last_char, ignore_case))
        {
            return false;
        }
    }

    let mut qi = 0usize;
    let query_chars: Vec<char> = core.chars().collect();
    for ch in text.chars() {
        if qi < query_chars.len() && chars_equal(ch, query_chars[qi], ignore_case) {
            qi += 1;
        }
    }
    qi == query_chars.len() || text.contains(core)
}

fn candidate_matches_text(text: &str, candidate: &str, ignore_case: bool) -> bool {
    let Some((exact, parsed)) = parse_include_alternative(candidate) else {
        return false;
    };
    if exact {
        exact_candidate_matches_text(text, &parsed, ignore_case)
    } else {
        fuzzy_candidate_matches_text(text, &parsed, ignore_case)
    }
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

    let spec = parse_query(query);
    if spec.include_terms.is_empty() && spec.exact_terms.is_empty() {
        return true;
    }

    let display = display_path_with_mode(path, root, prefer_relative);
    let normalized_path = normalize_windows_path(path);
    let filename = normalized_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();

    for term in &spec.exact_terms {
        if !exact_candidate_matches_text(filename, term, ignore_case)
            && !exact_candidate_matches_text(&display, term, ignore_case)
        {
            return false;
        }
    }

    for term in &spec.include_terms {
        let mut matched = false;
        for candidate in include_alternatives(term) {
            if candidate_matches_text(filename, candidate, ignore_case)
                || candidate_matches_text(&display, candidate, ignore_case)
            {
                matched = true;
                break;
            }
        }
        if !matched {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::{
        parse_include_alternative, parse_query, split_anchor, token_uses_regex_syntax, QuerySpec,
    };

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
}

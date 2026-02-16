use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use regex::RegexBuilder;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct QuerySpec {
    pub include_terms: Vec<String>,
    pub exact_terms: Vec<String>,
    pub exclude_terms: Vec<String>,
}

pub fn parse_query(query: &str) -> QuerySpec {
    let mut include_terms = Vec::new();
    let mut exact_terms = Vec::new();
    let mut exclude_terms = Vec::new();

    for token in query.split_whitespace() {
        if token.starts_with('\'') && token.len() > 1 {
            exact_terms.push(token[1..].to_string());
        } else if token.starts_with('!') && token.len() > 1 {
            exclude_terms.push(token[1..].to_string());
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

fn is_subsequence(query: &str, text: &str) -> bool {
    let mut qi = 0usize;
    let q: Vec<char> = query.chars().collect();
    for ch in text.chars() {
        if qi < q.len() && ch == q[qi] {
            qi += 1;
        }
    }
    qi == q.len()
}

fn is_fuzzy_match(query: &str, text: &str) -> bool {
    let q = query.to_ascii_lowercase();
    let t = text.to_ascii_lowercase();
    t.contains(&q) || is_subsequence(&q, &t)
}

fn matches_anchored_literal(term: &str, text: &str) -> bool {
    let anchored_start = term.starts_with('^');
    let anchored_end = term.ends_with('$');

    let mut core = term;
    if anchored_start {
        core = &core[1..];
    }
    if anchored_end && !core.is_empty() {
        core = &core[..core.len().saturating_sub(1)];
    }
    if core.is_empty() {
        return false;
    }

    if anchored_start && anchored_end {
        text == core
    } else if anchored_start {
        text.starts_with(core)
    } else if anchored_end {
        text.ends_with(core)
    } else {
        text.contains(core)
    }
}

fn matches_exact_term(term: &str, name: &str, full: &str) -> bool {
    let t = term.to_ascii_lowercase();
    matches_anchored_literal(&t, name) || matches_anchored_literal(&t, full)
}

fn matches_exclusion_term(term: &str, name: &str, full: &str) -> bool {
    let t = term.to_ascii_lowercase();
    matches_anchored_literal(&t, name) || matches_anchored_literal(&t, full)
}

fn matches_include_term(term: &str, name: &str, full: &str, use_regex: bool) -> bool {
    if use_regex {
        let regex = RegexBuilder::new(term).case_insensitive(true).build();
        if let Ok(re) = regex {
            return re.is_match(name) || re.is_match(full);
        }
        return false;
    }

    let t = term.to_ascii_lowercase();
    let anchored_start = t.starts_with('^');
    let anchored_end = t.ends_with('$');
    let mut core = t.clone();
    if anchored_start {
        core = core[1..].to_string();
    }
    if anchored_end && !core.is_empty() {
        core = core[..core.len().saturating_sub(1)].to_string();
    }
    if core.is_empty() {
        return false;
    }

    if anchored_start {
        let c = core.chars().next().unwrap_or_default();
        if !(name.starts_with(c) || full.starts_with(c)) {
            return false;
        }
    }
    if anchored_end {
        let c = core.chars().last().unwrap_or_default();
        if !(name.ends_with(c) || full.ends_with(c)) {
            return false;
        }
    }

    is_fuzzy_match(&core, name) || is_fuzzy_match(&core, full)
}

fn matches_spec(spec: &QuerySpec, path: &Path, use_regex: bool) -> bool {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let full = path.to_string_lossy().to_ascii_lowercase();

    for term in &spec.exclude_terms {
        if matches_exclusion_term(term, &name, &full) {
            return false;
        }
    }

    for term in &spec.exact_terms {
        if !matches_exact_term(term, &name, &full) {
            return false;
        }
    }

    for term in &spec.include_terms {
        if !matches_include_term(term, &name, &full, use_regex) {
            return false;
        }
    }

    true
}

fn fallback_score(query: &str, text: &str) -> f64 {
    if query.is_empty() {
        return 0.0;
    }
    let q = query.to_ascii_lowercase();
    let t = text.to_ascii_lowercase();
    let mut score = 0.0;
    if t.contains(&q) {
        score += 25.0;
    }
    if t.starts_with(&q) {
        score += 30.0;
    }
    score + (q.len().min(t.len()) as f64)
}

pub fn search_entries(
    query: &str,
    entries: &[PathBuf],
    limit: usize,
    use_regex: bool,
) -> Vec<(PathBuf, f64)> {
    let query = query.trim();
    if query.is_empty() || limit == 0 {
        return Vec::new();
    }

    let spec = parse_query(query);
    let filtered: Vec<PathBuf> = entries
        .iter()
        .filter(|p| matches_spec(&spec, p, use_regex))
        .cloned()
        .collect();

    if filtered.is_empty() {
        return Vec::new();
    }

    let mut q = spec.include_terms.join(" ").to_ascii_lowercase();
    if q.is_empty() {
        if let Some(first_exact) = spec.exact_terms.first() {
            q = first_exact.to_ascii_lowercase();
        }
    }

    let matcher = SkimMatcherV2::default();
    let mut scored = Vec::with_capacity(filtered.len());

    for path in filtered {
        let full = path.to_string_lossy().to_string();
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let full_lower = full.to_ascii_lowercase();

        let mut score = if !q.is_empty() {
            matcher
                .fuzzy_match(&full_lower, &q)
                .map(|s| s as f64)
                .unwrap_or_else(|| fallback_score(&q, &full_lower))
        } else {
            0.0
        };

        if !q.is_empty() && name == q {
            score += 1000.0;
        } else if !q.is_empty() && full_lower == q {
            score += 900.0;
        }

        for term in &spec.exact_terms {
            if matches_exact_term(term, &name, &full_lower) {
                score += 800.0;
            }
        }

        scored.push((path, score));
    }

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);
    scored
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_and_exclusion_tokens_work() {
        let entries = vec![
            PathBuf::from("/tmp/src/main.py"),
            PathBuf::from("/tmp/src/readme.md"),
        ];

        let exact = search_entries("'main", &entries, 10, false);
        assert_eq!(exact.len(), 1);

        let excluded = search_entries("!readme", &entries, 10, false);
        assert_eq!(excluded.len(), 1);
    }

    #[test]
    fn anchors_in_non_regex_are_fuzzy_with_adjacent_constraints() {
        let entries = vec![
            PathBuf::from("/tmp/src/main.py"),
            PathBuf::from("/tmp/src/amain.py"),
        ];
        let out = search_entries("^main", &entries, 10, false);
        assert_eq!(out.len(), 1);
        assert!(out[0].0.to_string_lossy().contains("main.py"));
    }
}

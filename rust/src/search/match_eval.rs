use super::SearchCandidateScore;
use crate::path_utils::normalize_windows_path;
use crate::query::{
    include_alternatives, parse_include_alternative, parse_query, split_anchor,
    token_uses_regex_syntax,
};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use regex::{Regex, RegexBuilder};
use std::path::Path;

#[derive(Debug, Clone)]
pub(super) struct LiteralPattern {
    anchored_start: bool,
    anchored_end: bool,
    core: String,
    core_chars: Vec<char>,
    first_char: Option<char>,
    last_char: Option<char>,
}

#[derive(Debug, Clone)]
pub(super) struct AlternativeSet {
    alternatives: Vec<LiteralPattern>,
}

#[derive(Debug, Clone)]
pub(super) struct IncludeAlternative {
    exact: bool,
    literal: LiteralPattern,
}

#[derive(Debug, Clone)]
pub(super) enum IncludeMatcher {
    Regex(Regex),
    Alternatives(Vec<IncludeAlternative>),
}

#[derive(Debug, Clone)]
pub(super) struct CompiledQuery {
    exact_terms: Vec<AlternativeSet>,
    exclude_terms: Vec<AlternativeSet>,
    include_terms: Vec<IncludeMatcher>,
    include_literal_bonus_terms: Vec<AlternativeSet>,
    include_exact_bonus_terms: Vec<LiteralPattern>,
    score_query: String,
}

#[derive(Clone, Copy)]
pub(super) struct SearchContext<'a> {
    pub(super) root: Option<&'a Path>,
    pub(super) prefer_relative: bool,
    pub(super) ignore_case: bool,
}

pub(super) struct SearchableEntry {
    name: String,
    full: String,
}

fn is_subsequence(query: &[char], text: &str) -> bool {
    let mut qi = 0usize;
    for ch in text.chars() {
        if qi < query.len() && ch == query[qi] {
            qi += 1;
        }
    }
    qi == query.len()
}

fn normalize_text(text: &str, ignore_case: bool) -> String {
    if ignore_case {
        text.to_ascii_lowercase()
    } else {
        text.to_string()
    }
}

fn compile_literal_pattern(term: &str, ignore_case: bool) -> Option<LiteralPattern> {
    let normalized = normalize_text(term, ignore_case);
    let (anchored_start, anchored_end, core) = split_anchor(&normalized);
    if core.is_empty() {
        return None;
    }

    let core = core.to_string();
    let core_chars = core.chars().collect::<Vec<_>>();
    let first_char = core_chars.first().copied();
    let last_char = core_chars.last().copied();
    Some(LiteralPattern {
        anchored_start,
        anchored_end,
        core,
        core_chars,
        first_char,
        last_char,
    })
}

fn compile_alternative_set(term: &str, ignore_case: bool) -> AlternativeSet {
    let normalized = normalize_text(term, ignore_case);
    let alternatives = include_alternatives(&normalized)
        .into_iter()
        .filter_map(|candidate| parse_include_alternative(candidate).map(|(_, value)| value))
        .filter_map(|candidate| compile_literal_pattern(&candidate, ignore_case))
        .collect();
    AlternativeSet { alternatives }
}

fn compile_non_exact_alternative_set(term: &str, ignore_case: bool) -> AlternativeSet {
    let normalized = normalize_text(term, ignore_case);
    let alternatives = include_alternatives(&normalized)
        .into_iter()
        .filter_map(parse_include_alternative)
        .filter_map(|(exact, candidate)| {
            if exact {
                return None;
            }
            compile_literal_pattern(&candidate, ignore_case)
        })
        .collect();
    AlternativeSet { alternatives }
}

fn compile_include_matcher(
    term: &str,
    use_regex: bool,
    ignore_case: bool,
) -> Result<IncludeMatcher, String> {
    if use_regex && token_uses_regex_syntax(term) {
        let re = RegexBuilder::new(term)
            .case_insensitive(ignore_case)
            .build()
            .map_err(|err| format!("invalid regex '{term}': {err}"))?;
        return Ok(IncludeMatcher::Regex(re));
    }

    let alternatives = include_alternatives(term)
        .into_iter()
        .filter_map(parse_include_alternative)
        .filter_map(|(exact, candidate)| {
            compile_literal_pattern(&candidate, ignore_case)
                .map(|literal| IncludeAlternative { exact, literal })
        })
        .collect();
    Ok(IncludeMatcher::Alternatives(alternatives))
}

fn build_score_query(
    include_terms: &[String],
    exact_terms: &[String],
    ignore_case: bool,
) -> String {
    let mut score_query = include_terms
        .iter()
        .flat_map(|term| include_alternatives(term))
        .filter_map(|term| {
            let (_, candidate) = parse_include_alternative(term)?;
            let (_, _, core) = split_anchor(&candidate);
            (!core.is_empty()).then_some(normalize_text(core, ignore_case))
        })
        .collect::<Vec<_>>()
        .join(" ");

    if score_query.is_empty() {
        if let Some(first_exact) = exact_terms.first() {
            score_query = normalize_text(first_exact, ignore_case);
        }
    }

    score_query
}

pub(super) fn compile_query(
    query: &str,
    use_regex: bool,
    ignore_case: bool,
) -> Result<CompiledQuery, String> {
    let spec = parse_query(query);

    let exact_terms = spec
        .exact_terms
        .iter()
        .map(|term| compile_alternative_set(term, ignore_case))
        .collect::<Vec<_>>();
    let exclude_terms = spec
        .exclude_terms
        .iter()
        .map(|term| compile_alternative_set(term, ignore_case))
        .collect::<Vec<_>>();
    let mut include_terms = Vec::with_capacity(spec.include_terms.len());
    let mut include_literal_bonus_terms = Vec::new();
    let mut include_exact_bonus_terms = Vec::new();
    for term in &spec.include_terms {
        include_terms.push(compile_include_matcher(term, use_regex, ignore_case)?);
        if !use_regex {
            let literal_bonus_set = compile_non_exact_alternative_set(term, ignore_case);
            if !literal_bonus_set.alternatives.is_empty() {
                include_literal_bonus_terms.push(literal_bonus_set);
            }
        }
        if !use_regex {
            for candidate in include_alternatives(term) {
                let Some((exact, parsed)) = parse_include_alternative(candidate) else {
                    continue;
                };
                if !exact {
                    continue;
                }
                let (_, _, core) = split_anchor(&parsed);
                if let Some(pattern) = compile_literal_pattern(core, ignore_case) {
                    include_exact_bonus_terms.push(pattern);
                }
            }
        }
    }

    Ok(CompiledQuery {
        exact_terms,
        exclude_terms,
        include_terms,
        include_literal_bonus_terms,
        include_exact_bonus_terms,
        score_query: build_score_query(&spec.include_terms, &spec.exact_terms, ignore_case),
    })
}

fn bonus_for_alternative_set(set: &AlternativeSet, entry: &SearchableEntry) -> f64 {
    let mut bonus = 0.0;
    if set.alternatives.iter().any(|pattern| {
        matches_anchored_literal(pattern, &entry.name)
            || matches_anchored_literal(pattern, &entry.full)
    }) {
        bonus += 150.0;
    }
    if set
        .alternatives
        .iter()
        .any(|pattern| entry.name == pattern.core || entry.full == pattern.core)
    {
        bonus += 150.0;
    }
    bonus
}

fn matches_anchored_literal(pattern: &LiteralPattern, text: &str) -> bool {
    if pattern.anchored_start && pattern.anchored_end {
        text == pattern.core
    } else if pattern.anchored_start {
        text.starts_with(&pattern.core)
    } else if pattern.anchored_end {
        text.ends_with(&pattern.core)
    } else {
        text.contains(&pattern.core)
    }
}

fn matches_alternative_set(set: &AlternativeSet, name: &str, full: &str) -> bool {
    set.alternatives.iter().any(|pattern| {
        matches_anchored_literal(pattern, name) || matches_anchored_literal(pattern, full)
    })
}

fn matches_include_literal(pattern: &LiteralPattern, name: &str, full: &str) -> bool {
    if pattern.anchored_start
        && !matches!(
            pattern.first_char,
            Some(ch) if name.starts_with(ch) || full.starts_with(ch)
        )
    {
        return false;
    }
    if pattern.anchored_end
        && !matches!(
            pattern.last_char,
            Some(ch) if name.ends_with(ch) || full.ends_with(ch)
        )
    {
        return false;
    }

    name.contains(&pattern.core)
        || is_subsequence(&pattern.core_chars, name)
        || full.contains(&pattern.core)
        || is_subsequence(&pattern.core_chars, full)
}

fn matches_include_matcher(matcher: &IncludeMatcher, name: &str, full: &str) -> bool {
    match matcher {
        IncludeMatcher::Regex(re) => re.is_match(name) || re.is_match(full),
        IncludeMatcher::Alternatives(alternatives) => alternatives.iter().any(|alternative| {
            if alternative.exact {
                matches_anchored_literal(&alternative.literal, name)
                    || matches_anchored_literal(&alternative.literal, full)
            } else {
                matches_include_literal(&alternative.literal, name, full)
            }
        }),
    }
}

fn searchable_full(
    path: &Path,
    root: Option<&Path>,
    prefer_relative: bool,
    ignore_case: bool,
) -> String {
    let normalized_path = normalize_windows_path(path);
    if prefer_relative {
        if let Some(root) = root {
            let normalized_root = normalize_windows_path(root);
            if let Ok(rel) = normalized_path.strip_prefix(&normalized_root) {
                return normalize_text(&rel.to_string_lossy(), ignore_case);
            }
        }
    }
    normalize_text(&normalized_path.to_string_lossy(), ignore_case)
}

fn build_searchable_entry(path: &Path, ctx: SearchContext<'_>) -> SearchableEntry {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| normalize_text(s, ctx.ignore_case))
        .unwrap_or_default();
    let full = searchable_full(path, ctx.root, ctx.prefer_relative, ctx.ignore_case);
    SearchableEntry { name, full }
}

fn matches_compiled_query(compiled: &CompiledQuery, entry: &SearchableEntry) -> bool {
    for term in &compiled.exclude_terms {
        if matches_alternative_set(term, &entry.name, &entry.full) {
            return false;
        }
    }

    for term in &compiled.exact_terms {
        if !matches_alternative_set(term, &entry.name, &entry.full) {
            return false;
        }
    }

    for matcher in &compiled.include_terms {
        if !matches_include_matcher(matcher, &entry.name, &entry.full) {
            return false;
        }
    }

    true
}

fn fallback_score(query: &str, text: &str) -> f64 {
    if query.is_empty() {
        return 0.0;
    }

    let mut score = 0.0;
    if text.contains(query) {
        score += 25.0;
    }
    if text.starts_with(query) {
        score += 30.0;
    }
    score + (query.len().min(text.len()) as f64)
}

fn score_entry(matcher: &SkimMatcherV2, compiled: &CompiledQuery, entry: &SearchableEntry) -> f64 {
    let mut score = if compiled.score_query.is_empty() {
        0.0
    } else {
        matcher
            .fuzzy_match(&entry.full, &compiled.score_query)
            .map(|value| value as f64)
            .unwrap_or_else(|| fallback_score(&compiled.score_query, &entry.full))
    };

    if !compiled.score_query.is_empty() && entry.name == compiled.score_query {
        score += 1000.0;
    } else if !compiled.score_query.is_empty() && entry.full == compiled.score_query {
        score += 900.0;
    }

    for term in &compiled.exact_terms {
        if matches_alternative_set(term, &entry.name, &entry.full) {
            score += 800.0;
        }
    }

    for term in &compiled.include_literal_bonus_terms {
        score += bonus_for_alternative_set(term, entry);
    }

    for pattern in &compiled.include_exact_bonus_terms {
        if matches_anchored_literal(pattern, &entry.name)
            || matches_anchored_literal(pattern, &entry.full)
        {
            score += 300.0;
            if entry.name == pattern.core {
                score += 300.0;
            }
        }
    }

    score
}

pub(super) fn evaluate_candidate(
    path: &Path,
    index: usize,
    ordinal: usize,
    compiled: &CompiledQuery,
    ctx: SearchContext<'_>,
    matcher: &SkimMatcherV2,
) -> Option<SearchCandidateScore> {
    let entry = build_searchable_entry(path, ctx);
    if !matches_compiled_query(compiled, &entry) {
        return None;
    }

    Some(SearchCandidateScore {
        index,
        score: score_entry(matcher, compiled, &entry),
        ordinal,
    })
}

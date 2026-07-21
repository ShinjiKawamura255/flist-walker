use super::{
    include_alternatives, parse_include_alternative, parse_query, split_anchor,
    token_uses_regex_syntax,
};
use crate::path_utils::{display_path_with_mode, normalize_windows_path};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use regex::{Regex, RegexBuilder};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[cfg(test)]
thread_local! {
    static QUERY_COMPILE_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static IGNORE_COMPILE_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_compile_counts() {
    QUERY_COMPILE_COUNT.set(0);
    IGNORE_COMPILE_COUNT.set(0);
}

#[cfg(test)]
pub(crate) fn query_compile_count() -> usize {
    QUERY_COMPILE_COUNT.get()
}

#[cfg(test)]
pub(crate) fn ignore_compile_count() -> usize {
    IGNORE_COMPILE_COUNT.get()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryOptions {
    pub use_regex: bool,
    pub ignore_case: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct QueryScope<'a> {
    pub root: Option<&'a Path>,
    pub prefer_relative: bool,
    pub ignore_case: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceLevel {
    RankOnly,
    WithSpans,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueryEvaluation {
    pub score: f64,
    pub spans: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct PreparedCandidate {
    name: String,
    full: String,
    visible: String,
    filename: String,
    filename_start: usize,
}

impl PreparedCandidate {
    fn from_path(path: &Path, scope: QueryScope<'_>) -> Self {
        let normalized_path = normalize_windows_path(path);
        let filename = normalized_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string();
        let visible_root = scope.root.unwrap_or_else(|| Path::new(""));
        let visible = display_path_with_mode(
            &normalized_path,
            visible_root,
            scope.prefer_relative && scope.root.is_some(),
        );
        let filename_start = visible
            .chars()
            .count()
            .saturating_sub(filename.chars().count());
        Self {
            name: normalize_text(&filename, scope.ignore_case),
            full: normalize_text(&visible, scope.ignore_case),
            visible,
            filename,
            filename_start,
        }
    }

    pub fn visible_text(&self) -> &str {
        &self.visible
    }
}

#[derive(Debug, Clone)]
struct LiteralPattern {
    anchored_start: bool,
    anchored_end: bool,
    core: String,
    core_chars: Vec<char>,
    first_char: Option<char>,
    last_char: Option<char>,
}

#[derive(Debug, Clone)]
struct AlternativeSet {
    alternatives: Vec<LiteralPattern>,
}

#[derive(Debug, Clone)]
struct ExactTermMatcher {
    set: AlternativeSet,
    required_unanchored_count: usize,
}

#[derive(Debug, Clone)]
struct IncludeAlternative {
    exact: bool,
    literal: LiteralPattern,
}

#[derive(Debug, Clone)]
enum IncludeMatcher {
    Regex(Regex),
    Alternatives(Vec<IncludeAlternative>),
}

#[derive(Debug, Clone)]
pub struct CompiledQuery {
    exact_terms: Vec<ExactTermMatcher>,
    exclude_terms: Vec<AlternativeSet>,
    include_terms: Vec<IncludeMatcher>,
    include_literal_bonus_terms: Vec<AlternativeSet>,
    include_exact_bonus_terms: Vec<LiteralPattern>,
    score_query: String,
    ignore_case: bool,
}

#[derive(Debug, Clone)]
pub struct CompiledIgnoreTerms {
    terms: Vec<AlternativeSet>,
}

impl CompiledIgnoreTerms {
    pub fn compile(terms: &[String], ignore_case: bool) -> Self {
        #[cfg(test)]
        IGNORE_COMPILE_COUNT.set(IGNORE_COMPILE_COUNT.get().saturating_add(1));
        Self {
            terms: terms
                .iter()
                .map(|term| compile_raw_alternative_set(term, ignore_case))
                .collect(),
        }
    }

    pub fn matches(&self, candidate: &PreparedCandidate) -> bool {
        self.terms
            .iter()
            .any(|term| matches_alternative_set(term, &candidate.name, &candidate.full))
    }

    pub fn matches_path(&self, path: &Path, scope: QueryScope<'_>) -> bool {
        self.matches(&PreparedCandidate::from_path(path, scope))
    }
}

impl CompiledQuery {
    pub fn compile(query: &str, options: QueryOptions) -> Result<Self, String> {
        #[cfg(test)]
        QUERY_COMPILE_COUNT.set(QUERY_COMPILE_COUNT.get().saturating_add(1));
        let spec = parse_query(query);
        let exact_terms = compile_exact_term_matchers(&spec.exact_terms, options.ignore_case);
        let exclude_terms = spec
            .exclude_terms
            .iter()
            .map(|term| compile_alternative_set(term, options.ignore_case))
            .collect::<Vec<_>>();
        let mut include_terms = Vec::with_capacity(spec.include_terms.len());
        let mut include_literal_bonus_terms = Vec::new();
        let mut include_exact_bonus_terms = Vec::new();
        for term in &spec.include_terms {
            include_terms.push(compile_include_matcher(
                term,
                options.use_regex,
                options.ignore_case,
            )?);
            if !options.use_regex {
                let literal_bonus_set =
                    compile_non_exact_alternative_set(term, options.ignore_case);
                if !literal_bonus_set.alternatives.is_empty() {
                    include_literal_bonus_terms.push(literal_bonus_set);
                }
                for candidate in include_alternatives(term) {
                    let Some((exact, parsed)) = parse_include_alternative(candidate) else {
                        continue;
                    };
                    if !exact {
                        continue;
                    }
                    let (_, _, core) = split_anchor(&parsed);
                    if let Some(pattern) = compile_literal_pattern(core, options.ignore_case) {
                        include_exact_bonus_terms.push(pattern);
                    }
                }
            }
        }

        Ok(Self {
            exact_terms,
            exclude_terms,
            include_terms,
            include_literal_bonus_terms,
            include_exact_bonus_terms,
            score_query: build_score_query(
                &spec.include_terms,
                &spec.exact_terms,
                options.ignore_case,
            ),
            ignore_case: options.ignore_case,
        })
    }

    pub fn evaluate(
        &self,
        candidate: &PreparedCandidate,
        evidence: EvidenceLevel,
    ) -> Option<QueryEvaluation> {
        self.evaluate_with_matcher(candidate, evidence, &SkimMatcherV2::default())
    }

    pub fn prepare_candidate(
        &self,
        path: &Path,
        root: Option<&Path>,
        prefer_relative: bool,
    ) -> PreparedCandidate {
        PreparedCandidate::from_path(
            path,
            QueryScope {
                root,
                prefer_relative,
                ignore_case: self.ignore_case,
            },
        )
    }

    // Regression guard: public visibility/highlight adapters intentionally project positive
    // clauses only. Do not replace these helpers with full evaluate() without updating the
    // paired tc_155_regression_* tests and the public compatibility contract.
    pub(crate) fn matches_positive_projection(&self, candidate: &PreparedCandidate) -> bool {
        matches_positive_terms(self, candidate)
    }

    pub(crate) fn positive_projection_spans(&self, candidate: &PreparedCandidate) -> Vec<usize> {
        collect_spans(self, candidate)
    }

    pub(crate) fn evaluate_with_matcher(
        &self,
        candidate: &PreparedCandidate,
        evidence: EvidenceLevel,
        matcher: &SkimMatcherV2,
    ) -> Option<QueryEvaluation> {
        if !matches_compiled_query(self, candidate) {
            return None;
        }
        let spans = match evidence {
            EvidenceLevel::RankOnly => Vec::new(),
            EvidenceLevel::WithSpans => collect_spans(self, candidate),
        };
        Some(QueryEvaluation {
            score: score_entry(matcher, self, candidate),
            spans,
        })
    }

    pub fn has_positive_terms(&self) -> bool {
        !self.exact_terms.is_empty() || !self.include_terms.is_empty()
    }
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
    Some(LiteralPattern {
        anchored_start,
        anchored_end,
        first_char: core_chars.first().copied(),
        last_char: core_chars.last().copied(),
        core,
        core_chars,
    })
}

fn compile_alternative_set(term: &str, ignore_case: bool) -> AlternativeSet {
    let normalized = normalize_text(term, ignore_case);
    AlternativeSet {
        alternatives: include_alternatives(&normalized)
            .into_iter()
            .filter_map(|candidate| parse_include_alternative(candidate).map(|(_, value)| value))
            .filter_map(|candidate| compile_literal_pattern(&candidate, ignore_case))
            .collect(),
    }
}

fn compile_raw_alternative_set(term: &str, ignore_case: bool) -> AlternativeSet {
    let normalized = normalize_text(term, ignore_case);
    AlternativeSet {
        alternatives: include_alternatives(&normalized)
            .into_iter()
            .filter_map(|candidate| compile_literal_pattern(candidate, ignore_case))
            .collect(),
    }
}

fn compile_exact_term_matchers(terms: &[String], ignore_case: bool) -> Vec<ExactTermMatcher> {
    let mut counts = BTreeMap::<String, usize>::new();
    for term in terms {
        *counts.entry(term.clone()).or_default() += 1;
    }
    counts
        .into_iter()
        .map(|(term, count)| {
            let set = compile_alternative_set(&term, ignore_case);
            let unanchored = set
                .alternatives
                .first()
                .is_some_and(|pattern| !pattern.anchored_start && !pattern.anchored_end);
            ExactTermMatcher {
                set,
                required_unanchored_count: if unanchored { count } else { 1 },
            }
        })
        .collect()
}

fn compile_non_exact_alternative_set(term: &str, ignore_case: bool) -> AlternativeSet {
    let normalized = normalize_text(term, ignore_case);
    AlternativeSet {
        alternatives: include_alternatives(&normalized)
            .into_iter()
            .filter_map(parse_include_alternative)
            .filter_map(|(exact, candidate)| {
                (!exact)
                    .then(|| compile_literal_pattern(&candidate, ignore_case))
                    .flatten()
            })
            .collect(),
    }
}

fn compile_include_matcher(
    term: &str,
    use_regex: bool,
    ignore_case: bool,
) -> Result<IncludeMatcher, String> {
    if use_regex && token_uses_regex_syntax(term) {
        return RegexBuilder::new(term)
            .case_insensitive(ignore_case)
            .build()
            .map(IncludeMatcher::Regex)
            .map_err(|error| format!("invalid regex '{term}': {error}"));
    }
    Ok(IncludeMatcher::Alternatives(
        include_alternatives(term)
            .into_iter()
            .filter_map(parse_include_alternative)
            .filter_map(|(exact, candidate)| {
                compile_literal_pattern(&candidate, ignore_case)
                    .map(|literal| IncludeAlternative { exact, literal })
            })
            .collect(),
    ))
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

fn is_subsequence(query: &[char], text: &str) -> bool {
    let mut qi = 0usize;
    for ch in text.chars() {
        if qi < query.len() && ch == query[qi] {
            qi += 1;
        }
    }
    qi == query.len()
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

fn literal_occurrence_count(text: &str, needle: &str) -> usize {
    if needle.is_empty() {
        return 0;
    }
    let mut count = 0;
    let mut search_from = 0;
    while let Some(offset) = text[search_from..].find(needle) {
        count += 1;
        search_from += offset + needle.len();
    }
    count
}

fn matches_exact_term(term: &ExactTermMatcher, name: &str, full: &str) -> bool {
    if term.required_unanchored_count <= 1 {
        return matches_alternative_set(&term.set, name, full);
    }
    term.set.alternatives.iter().any(|pattern| {
        literal_occurrence_count(name, &pattern.core)
            .max(literal_occurrence_count(full, &pattern.core))
            >= term.required_unanchored_count
    })
}

fn matches_include_literal(pattern: &LiteralPattern, name: &str, full: &str) -> bool {
    if pattern.anchored_start
        && !matches!(pattern.first_char, Some(ch) if name.starts_with(ch) || full.starts_with(ch))
    {
        return false;
    }
    if pattern.anchored_end
        && !matches!(pattern.last_char, Some(ch) if name.ends_with(ch) || full.ends_with(ch))
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
        IncludeMatcher::Regex(regex) => regex.is_match(name) || regex.is_match(full),
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

fn matches_compiled_query(compiled: &CompiledQuery, candidate: &PreparedCandidate) -> bool {
    !compiled
        .exclude_terms
        .iter()
        .any(|term| matches_alternative_set(term, &candidate.name, &candidate.full))
        && matches_positive_terms(compiled, candidate)
}

fn matches_positive_terms(compiled: &CompiledQuery, candidate: &PreparedCandidate) -> bool {
    compiled
        .exact_terms
        .iter()
        .all(|term| matches_exact_term(term, &candidate.name, &candidate.full))
        && compiled
            .include_terms
            .iter()
            .all(|matcher| matches_include_matcher(matcher, &candidate.name, &candidate.full))
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
    score + query.len().min(text.len()) as f64
}

fn bonus_for_alternative_set(set: &AlternativeSet, candidate: &PreparedCandidate) -> f64 {
    let mut bonus = 0.0;
    if set.alternatives.iter().any(|pattern| {
        matches_anchored_literal(pattern, &candidate.name)
            || matches_anchored_literal(pattern, &candidate.full)
    }) {
        bonus += 150.0;
    }
    if set
        .alternatives
        .iter()
        .any(|pattern| candidate.name == pattern.core || candidate.full == pattern.core)
    {
        bonus += 150.0;
    }
    bonus
}

fn score_entry(
    matcher: &SkimMatcherV2,
    compiled: &CompiledQuery,
    candidate: &PreparedCandidate,
) -> f64 {
    let mut score = if compiled.score_query.is_empty() {
        0.0
    } else {
        matcher
            .fuzzy_match(&candidate.full, &compiled.score_query)
            .map(|value| value as f64)
            .unwrap_or_else(|| fallback_score(&compiled.score_query, &candidate.full))
    };
    if !compiled.score_query.is_empty() && candidate.name == compiled.score_query {
        score += 1000.0;
    } else if !compiled.score_query.is_empty() && candidate.full == compiled.score_query {
        score += 900.0;
    }
    for term in &compiled.exact_terms {
        if matches_exact_term(term, &candidate.name, &candidate.full) {
            score += 800.0;
        }
    }
    for term in &compiled.include_literal_bonus_terms {
        score += bonus_for_alternative_set(term, candidate);
    }
    for pattern in &compiled.include_exact_bonus_terms {
        if matches_anchored_literal(pattern, &candidate.name)
            || matches_anchored_literal(pattern, &candidate.full)
        {
            score += 300.0;
            if candidate.name == pattern.core {
                score += 300.0;
            }
        }
    }
    score
}

fn chars_equal(left: char, right: char, ignore_case: bool) -> bool {
    if ignore_case && left.is_ascii() && right.is_ascii() {
        left.eq_ignore_ascii_case(&right)
    } else {
        left == right
    }
}

fn exact_positions(text: &str, pattern: &LiteralPattern, ignore_case: bool) -> Vec<usize> {
    let text_chars = text.chars().collect::<Vec<_>>();
    let core_chars = pattern.core.chars().collect::<Vec<_>>();
    if core_chars.is_empty() || core_chars.len() > text_chars.len() {
        return Vec::new();
    }
    for start in 0..=text_chars.len() - core_chars.len() {
        if !core_chars
            .iter()
            .enumerate()
            .all(|(offset, query)| chars_equal(text_chars[start + offset], *query, ignore_case))
        {
            continue;
        }
        if pattern.anchored_start && start != 0 {
            continue;
        }
        if pattern.anchored_end && start + core_chars.len() != text_chars.len() {
            continue;
        }
        return (start..start + core_chars.len()).collect();
    }
    Vec::new()
}

fn fuzzy_positions(text: &str, pattern: &LiteralPattern, ignore_case: bool) -> Vec<usize> {
    if pattern.anchored_start
        && !matches!(pattern.core.chars().next(), Some(first) if text.chars().next().is_some_and(|value| chars_equal(value, first, ignore_case)))
    {
        return Vec::new();
    }
    if pattern.anchored_end
        && !matches!(pattern.core.chars().last(), Some(last) if text.chars().last().is_some_and(|value| chars_equal(value, last, ignore_case)))
    {
        return Vec::new();
    }
    let text_chars = text.chars().collect::<Vec<_>>();
    let query_chars = pattern.core.chars().collect::<Vec<_>>();
    if query_chars.len() <= text_chars.len() {
        for start in 0..=text_chars.len() - query_chars.len() {
            if query_chars
                .iter()
                .enumerate()
                .all(|(offset, query)| chars_equal(text_chars[start + offset], *query, ignore_case))
            {
                return (start..start + query_chars.len()).collect();
            }
        }
    }
    let mut positions = Vec::with_capacity(query_chars.len());
    let mut query_index = 0usize;
    for (index, value) in text_chars.iter().enumerate() {
        if query_index < query_chars.len()
            && chars_equal(*value, query_chars[query_index], ignore_case)
        {
            positions.push(index);
            query_index += 1;
        }
    }
    if query_index == query_chars.len() {
        positions
    } else {
        Vec::new()
    }
}

fn regex_positions(text: &str, regex: &Regex) -> Vec<usize> {
    let mut positions = Vec::new();
    for matched in regex.find_iter(text) {
        if matched.start() == matched.end() {
            continue;
        }
        let start = text[..matched.start()].chars().count();
        let len = text[matched.start()..matched.end()].chars().count();
        positions.extend(start..start + len);
    }
    positions
}

fn add_pattern_positions(
    spans: &mut BTreeSet<usize>,
    candidate: &PreparedCandidate,
    pattern: &LiteralPattern,
    exact: bool,
    ignore_case: bool,
) {
    let filename_hits = if exact {
        exact_positions(&candidate.filename, pattern, ignore_case)
    } else {
        fuzzy_positions(&candidate.filename, pattern, ignore_case)
    };
    if !filename_hits.is_empty() {
        spans.extend(
            filename_hits
                .into_iter()
                .map(|position| candidate.filename_start + position),
        );
        return;
    }
    let visible_hits = if exact {
        exact_positions(&candidate.visible, pattern, ignore_case)
    } else {
        fuzzy_positions(&candidate.visible, pattern, ignore_case)
    };
    spans.extend(visible_hits);
}

fn collect_spans(compiled: &CompiledQuery, candidate: &PreparedCandidate) -> Vec<usize> {
    let mut spans = BTreeSet::new();
    for term in &compiled.exact_terms {
        if let Some(pattern) = term.set.alternatives.iter().find(|pattern| {
            matches_anchored_literal(pattern, &candidate.name)
                || matches_anchored_literal(pattern, &candidate.full)
        }) {
            add_pattern_positions(&mut spans, candidate, pattern, true, compiled.ignore_case);
        }
    }
    for matcher in &compiled.include_terms {
        match matcher {
            IncludeMatcher::Regex(regex) => {
                let filename_hits = regex_positions(&candidate.filename, regex);
                if filename_hits.is_empty() {
                    spans.extend(regex_positions(&candidate.visible, regex));
                } else {
                    spans.extend(
                        filename_hits
                            .into_iter()
                            .map(|position| candidate.filename_start + position),
                    );
                }
            }
            IncludeMatcher::Alternatives(alternatives) => {
                if let Some(alternative) = alternatives.iter().find(|alternative| {
                    if alternative.exact {
                        matches_anchored_literal(&alternative.literal, &candidate.name)
                            || matches_anchored_literal(&alternative.literal, &candidate.full)
                    } else {
                        matches_include_literal(
                            &alternative.literal,
                            &candidate.name,
                            &candidate.full,
                        )
                    }
                }) {
                    add_pattern_positions(
                        &mut spans,
                        candidate,
                        &alternative.literal,
                        alternative.exact,
                        compiled.ignore_case,
                    );
                }
            }
        }
    }
    spans.into_iter().collect()
}

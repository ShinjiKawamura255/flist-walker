use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

pub(crate) const MAX_QUERY_HISTORY_ENTRIES: usize = 100;

fn history_search_score(query: &str, candidate: &str, recency_rank: usize) -> Option<i64> {
    if query.trim().is_empty() {
        return Some(recency_rank as i64);
    }

    let matcher = SkimMatcherV2::default();
    matcher.fuzzy_match(candidate, query).or_else(|| {
        let query_lower = query.to_ascii_lowercase();
        let candidate_lower = candidate.to_ascii_lowercase();
        candidate_lower
            .contains(&query_lower)
            .then_some((query_lower.len() as i64) * 100 + recency_rank as i64)
    })
}

pub(crate) fn history_matches<'a, I>(query: &str, entries: I) -> Vec<String>
where
    I: DoubleEndedIterator<Item = &'a String> + ExactSizeIterator,
{
    let entry_count = entries.len();
    let mut scored = entries
        .rev()
        .enumerate()
        .filter_map(|(index, entry)| {
            history_search_score(query.trim(), entry, entry_count.saturating_sub(index))
                .map(|score| (entry.clone(), score, index))
        })
        .collect::<Vec<_>>();
    scored.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.2.cmp(&right.2)));
    scored.into_iter().map(|(entry, _, _)| entry).collect()
}

pub(crate) fn history_with_query<'a, I>(entries: I, query: &str) -> Option<Vec<String>>
where
    I: IntoIterator<Item = &'a String>,
{
    let query = query.trim();
    if query.is_empty() {
        return None;
    }

    let mut history = entries
        .into_iter()
        .filter(|entry| entry.as_str() != query)
        .cloned()
        .collect::<Vec<_>>();
    history.push(query.to_string());
    if history.len() > MAX_QUERY_HISTORY_ENTRIES {
        history.drain(..history.len() - MAX_QUERY_HISTORY_ENTRIES);
    }
    Some(history)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_matches_are_recent_first_and_fuzzy_filtered() {
        let entries = ["old".to_string(), "alpha".to_string(), "beta".to_string()];

        assert_eq!(
            history_matches("", entries.iter()),
            vec!["beta", "alpha", "old"]
        );
        assert_eq!(history_matches("p", entries.iter()), vec!["alpha"]);
    }

    #[test]
    fn history_with_query_trims_deduplicates_and_moves_to_the_back() {
        let entries = [
            "first".to_string(),
            "draft".to_string(),
            "second".to_string(),
        ];

        assert_eq!(
            history_with_query(entries.iter(), " draft "),
            Some(vec![
                "first".to_string(),
                "second".to_string(),
                "draft".to_string(),
            ])
        );
        assert_eq!(history_with_query(entries.iter(), " \t "), None);
    }

    #[test]
    fn history_with_query_keeps_only_the_latest_hundred_entries() {
        let entries = (0..MAX_QUERY_HISTORY_ENTRIES)
            .map(|index| format!("query-{index:03}"))
            .collect::<Vec<_>>();

        let updated = history_with_query(entries.iter(), "query-100").expect("non-empty query");

        assert_eq!(updated.len(), MAX_QUERY_HISTORY_ENTRIES);
        assert_eq!(updated.first().map(String::as_str), Some("query-001"));
        assert_eq!(updated.last().map(String::as_str), Some("query-100"));
    }
}

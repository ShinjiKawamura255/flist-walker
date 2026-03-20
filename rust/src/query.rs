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

#[cfg(test)]
mod tests {
    use super::{parse_include_alternative, parse_query, split_anchor, QuerySpec};

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
}

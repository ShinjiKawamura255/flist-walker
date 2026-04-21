use std::fs;
use std::path::{Path, PathBuf};

pub const IGNORE_LIST_FILE_NAME: &str = "flistwalker.ignore.txt";

pub fn current_exe_ignore_list_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.parent()
        .map(|parent| parent.join(IGNORE_LIST_FILE_NAME))
}

pub fn load_ignore_terms_from_current_exe() -> Vec<String> {
    current_exe_ignore_list_path()
        .as_deref()
        .map(load_ignore_terms_from_path)
        .unwrap_or_default()
}

pub fn load_ignore_terms_from_path(path: &Path) -> Vec<String> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    parse_ignore_terms(&text)
}

fn normalize_ignore_term(term: &str) -> Option<String> {
    let trimmed = term.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let normalized = trimmed.strip_prefix('!').unwrap_or(trimmed).trim();
    (!normalized.is_empty()).then(|| normalized.to_string())
}

pub fn parse_ignore_terms(text: &str) -> Vec<String> {
    let mut terms = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        for token in trimmed.split_whitespace() {
            if let Some(term) = normalize_ignore_term(token) {
                terms.push(term);
            }
        }
    }
    terms
}

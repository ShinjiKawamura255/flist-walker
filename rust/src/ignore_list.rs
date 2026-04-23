use crate::fs_atomic::write_text_atomic;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub const IGNORE_LIST_FILE_NAME: &str = "flistwalker.ignore.txt";
pub const IGNORE_LIST_SAMPLE_FILE_NAME: &str = "flistwalker.ignore.txt.example";
pub const IGNORE_LIST_SAMPLE_TEMPLATE: &str = include_str!("../../flistwalker.ignore.txt.example");

pub fn current_exe_ignore_list_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.parent()
        .map(|parent| parent.join(IGNORE_LIST_FILE_NAME))
}

pub fn current_exe_ignore_list_sample_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.parent()
        .map(|parent| parent.join(IGNORE_LIST_SAMPLE_FILE_NAME))
}

pub fn ensure_ignore_list_sample() -> Result<bool> {
    let Some(path) = current_exe_ignore_list_sample_path() else {
        return Ok(false);
    };
    ensure_ignore_list_sample_at(&path)
}

pub fn ensure_ignore_list_sample_at(path: &Path) -> Result<bool> {
    if path.exists() {
        return Ok(false);
    }
    write_text_atomic(path, IGNORE_LIST_SAMPLE_TEMPLATE)
        .with_context(|| format!("failed to create ignore list sample at {}", path.display()))?;
    Ok(true)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("flistwalker-ignore-list-{name}-{nonce}"))
    }

    #[test]
    fn parse_ignore_terms_skips_comments_and_blank_lines() {
        let terms = parse_ignore_terms(
            r#"
                # comment
                old  ~

                backup   tmp
            "#,
        );

        assert_eq!(terms, vec!["old", "~", "backup", "tmp"]);
    }

    #[test]
    fn load_ignore_terms_from_path_returns_empty_for_missing_file() {
        let path = Path::new("/definitely/missing/flistwalker.ignore.txt");
        assert!(load_ignore_terms_from_path(path).is_empty());
    }

    #[test]
    fn ensure_ignore_list_sample_at_creates_template_when_missing() {
        let root = test_root("create-sample");
        let path = root.join(IGNORE_LIST_SAMPLE_FILE_NAME);

        let created = ensure_ignore_list_sample_at(&path).expect("create sample");

        assert!(created);
        let text = fs::read_to_string(&path).expect("read sample");
        assert_eq!(text, IGNORE_LIST_SAMPLE_TEMPLATE);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ensure_ignore_list_sample_at_preserves_existing_file() {
        let root = test_root("preserve-sample");
        fs::create_dir_all(&root).expect("create root");
        let path = root.join(IGNORE_LIST_SAMPLE_FILE_NAME);
        fs::write(&path, "custom").expect("write existing");

        let created = ensure_ignore_list_sample_at(&path).expect("ensure sample");

        assert!(!created);
        assert_eq!(fs::read_to_string(&path).expect("read sample"), "custom");
        let _ = fs::remove_dir_all(&root);
    }
}

use crate::actions::choose_action;
use regex::RegexBuilder;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

fn normalize_windows_display(text: &str) -> String {
    #[cfg(windows)]
    {
        if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{}", rest);
        }
        if let Some(rest) = text.strip_prefix(r"\\?\") {
            return rest.to_string();
        }
    }
    text.to_string()
}

pub fn display_path(path: &Path, root: &Path) -> String {
    display_path_with_mode(path, root, true)
}

fn normalize_windows_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let raw = path.to_string_lossy();
        if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{}", rest));
        }
        if let Some(rest) = raw.strip_prefix(r"\\?\") {
            return PathBuf::from(rest);
        }
    }
    path.to_path_buf()
}

pub fn normalize_path_for_display(path: &Path) -> String {
    let normalized = normalize_windows_path(path);
    normalize_windows_display(&normalized.to_string_lossy())
}

pub fn display_path_with_mode(path: &Path, root: &Path, prefer_relative: bool) -> String {
    let normalized_path = normalize_windows_path(path);
    let normalized_root = normalize_windows_path(root);
    let raw = if prefer_relative {
        normalized_path
            .strip_prefix(&normalized_root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| normalized_path.to_string_lossy().to_string())
    } else {
        normalized_path.to_string_lossy().to_string()
    };
    normalize_windows_display(&raw)
}

fn find_match_positions(text: &str, query: &str) -> HashSet<usize> {
    let mut out = HashSet::new();
    if query.is_empty() {
        return out;
    }

    let text_chars: Vec<char> = text.chars().collect();
    let q_chars: Vec<char> = query.chars().collect();
    if q_chars.is_empty() {
        return out;
    }

    let chars_equal = |a: char, b: char| {
        if a.is_ascii() && b.is_ascii() {
            a.eq_ignore_ascii_case(&b)
        } else {
            a == b
        }
    };

    if q_chars.len() <= text_chars.len() {
        for start in 0..=text_chars.len() - q_chars.len() {
            if q_chars
                .iter()
                .enumerate()
                .all(|(offset, q)| chars_equal(text_chars[start + offset], *q))
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
        if qi < q_chars.len() && chars_equal(*ch, q_chars[qi]) {
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

fn highlight_terms(query: &str, use_regex: bool) -> Vec<String> {
    let mut terms = Vec::new();
    for mut token in query.split_whitespace().map(ToString::to_string) {
        if token.starts_with('!') {
            continue;
        }
        let is_exact = token.starts_with('\'');
        if is_exact {
            token = token[1..].to_string();
        }
        if use_regex && !is_exact {
            if !token.is_empty() {
                terms.push(token);
            }
            continue;
        }
        if token.starts_with('^') {
            token = token[1..].to_string();
        }
        if token.ends_with('$') {
            token = token[..token.len().saturating_sub(1)].to_string();
        }
        if !token.is_empty() {
            terms.push(token);
        }
    }
    terms
}

pub fn match_positions_for_path(
    path: &Path,
    root: &Path,
    query: &str,
    prefer_relative: bool,
    use_regex: bool,
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

    for term in highlight_terms(query, use_regex) {
        let hits = if use_regex {
            find_regex_match_positions(filename, &term)
        } else {
            find_match_positions(filename, &term)
        };
        if !hits.is_empty() {
            for pos in hits {
                positions.insert(start + pos);
            }
            continue;
        }
        if use_regex {
            positions.extend(find_regex_match_positions(&display, &term));
        } else {
            positions.extend(find_match_positions(&display, &term));
        }
    }
    positions
}

pub fn has_visible_match(path: &Path, root: &Path, query: &str, prefer_relative: bool) -> bool {
    if query.trim().is_empty() {
        return true;
    }
    if highlight_terms(query, false).is_empty() {
        // Exclusion-only queries are already filtered by search logic.
        return true;
    }
    !match_positions_for_path(path, root, query, prefer_relative, false).is_empty()
}

fn find_regex_match_positions(text: &str, pattern: &str) -> HashSet<usize> {
    let mut out = HashSet::new();
    let Ok(re) = RegexBuilder::new(pattern).case_insensitive(true).build() else {
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

pub fn build_preview_text(path: &Path) -> String {
    build_preview_text_with_kind(path, path.is_dir())
}

pub fn build_preview_text_with_kind(path: &Path, is_dir: bool) -> String {
    const PREVIEW_MAX_LINES: usize = 20;
    const PREVIEW_MAX_BYTES: usize = 64 * 1024;

    let normalized_path = normalize_path_for_display(path);
    if is_dir {
        return build_directory_preview_text(path, &normalized_path);
    }

    if should_skip_preview(path, is_dir) {
        return format!(
            "File: {}\nAction: {:?}\n\n<on-demand file: preview skipped>",
            normalized_path,
            choose_action(path)
        );
    }

    let action = format!("{:?}", choose_action(path));
    let head = format!("File: {}\nAction: {}\n", normalized_path, action);

    match read_preview_lines(path, PREVIEW_MAX_LINES, PREVIEW_MAX_BYTES) {
        Ok(preview) => {
            if preview.is_empty() {
                format!("{}\n<empty file>", head)
            } else {
                format!("{}\n{}", head, preview.join("\n"))
            }
        }
        Err(_) => format!("{}\n<binary or unreadable file>", head),
    }
}

fn read_preview_lines(
    path: &Path,
    max_lines: usize,
    max_bytes: usize,
) -> std::io::Result<Vec<String>> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut out = Vec::new();
    let mut bytes_read = 0usize;

    while out.len() < max_lines && bytes_read < max_bytes {
        let mut line = String::new();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break;
        }
        bytes_read = bytes_read.saturating_add(n);
        let trimmed = line.trim_end_matches(&['\r', '\n'][..]).to_string();
        out.push(trimmed);
    }

    Ok(out)
}

pub fn should_skip_preview(path: &Path, is_dir: bool) -> bool {
    !is_dir && is_on_demand_file(path)
}

fn is_on_demand_file(path: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;
        const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x0004_0000;
        const FILE_ATTRIBUTE_UNPINNED: u32 = 0x0010_0000;
        const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
        return std::fs::metadata(path)
            .map(|m| {
                let attrs = m.file_attributes();
                (attrs
                    & (FILE_ATTRIBUTE_OFFLINE
                        | FILE_ATTRIBUTE_RECALL_ON_OPEN
                        | FILE_ATTRIBUTE_UNPINNED
                        | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS))
                    != 0
            })
            .unwrap_or(false);
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}

fn build_directory_preview_text(path: &Path, normalized_path: &str) -> String {
    const MAX_LINES: usize = 24;
    const MAX_NAME_CHARS: usize = 80;

    let read = std::fs::read_dir(path);
    let Ok(iter) = read else {
        return format!("Directory: {}\nChildren: <unavailable>", normalized_path);
    };

    let mut entries: Vec<_> = iter.flatten().collect();
    entries.sort_by_key(|e| {
        e.file_name()
            .to_string_lossy()
            .to_string()
            .to_ascii_lowercase()
    });

    let total = entries.len();
    if total == 0 {
        return format!("Directory: {}\nChildren: 0\n<empty>", normalized_path);
    }

    let mut lines = Vec::new();
    for entry in entries.iter().take(MAX_LINES) {
        let name = entry.file_name().to_string_lossy().to_string();
        let short = truncate_chars(&name, MAX_NAME_CHARS);
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let marker = if is_dir { "[D]" } else { "[F]" };
        lines.push(format!("{} {}", marker, short));
    }
    if total > MAX_LINES {
        lines.push(format!("... ({} more)", total - MAX_LINES));
    }

    format!(
        "Directory: {}\nChildren: {}\nScope: direct children only\n\n{}",
        normalized_path,
        total,
        lines.join("\n")
    )
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let keep = max_chars.saturating_sub(3);
    let mut out: String = text.chars().take(keep).collect();
    out.push_str("...");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("fff-rs-ui-{name}-{nonce}"))
    }

    #[test]
    fn display_path_uses_relative_path() {
        let root = test_root("display-relative");
        let sample = root.join("src/main.py");
        fs::create_dir_all(sample.parent().expect("parent")).expect("create parent");
        fs::write(&sample, "print('x')\n").expect("write sample");

        let label = display_path(&sample, &root);
        assert!(label.contains("src/main.py"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn match_positions_ascii_query_work_with_multibyte_path() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/日本語/docs/readme.txt");
        let positions = match_positions_for_path(&path, &root, "read", true, false);
        assert!(!positions.is_empty());
    }

    #[test]
    fn match_positions_multibyte_query_only_highlights_matched_chars() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/日本語/テスト資料.txt");
        let positions = match_positions_for_path(&path, &root, "テスト", true, false);
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

        let positions = match_positions_for_path(&sample, &root, "main !readme", true, false);
        assert!(positions.len() >= 4);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn match_positions_support_exact_token_prefix() {
        let root = test_root("highlight-exact");
        let sample = root.join("src/main.py");
        fs::create_dir_all(sample.parent().expect("parent")).expect("create parent");
        fs::write(&sample, "print('x')\n").expect("write sample");

        let positions = match_positions_for_path(&sample, &root, "'main", true, false);
        assert!(positions.len() >= 4);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn has_visible_match_false_when_term_not_in_visible_text() {
        let root = test_root("visible-match");
        let sample = root.join("src/main.py");
        fs::create_dir_all(sample.parent().expect("parent")).expect("create parent");
        fs::write(&sample, "print('x')\n").expect("write sample");

        assert!(!has_visible_match(&sample, &root, "zzzz", true));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn has_visible_match_true_for_exclusion_only_query() {
        let root = test_root("visible-exclusion-only");
        let sample = root.join("src/main.py");
        fs::create_dir_all(sample.parent().expect("parent")).expect("create parent");
        fs::write(&sample, "print('x')\n").expect("write sample");

        assert!(has_visible_match(&sample, &root, "!readme", true));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn match_positions_regex_query_highlights_matched_span() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/main.py");
        let positions = match_positions_for_path(&path, &root, "ma.*py", true, true);
        assert!(!positions.is_empty());
    }

    #[test]
    fn build_preview_text_for_directory() {
        let root = test_root("preview-dir");
        fs::create_dir_all(&root).expect("create dir");
        let child_dir = root.join("child");
        fs::create_dir_all(&child_dir).expect("create child dir");
        fs::write(root.join("a.txt"), "x").expect("write file");
        fs::write(child_dir.join("b.txt"), "y").expect("write nested file");

        let preview = build_preview_text(&root);
        assert!(preview.contains("Directory:"));
        assert!(preview.contains("Children:"));
        assert!(preview.contains("Scope: direct children only"));
        assert!(preview.contains("[D] child"));
        assert!(preview.contains("[F] a.txt"));
        assert!(!preview.contains("b.txt"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn build_preview_text_for_file_contains_action_and_content() {
        let root = test_root("preview-file");
        fs::create_dir_all(&root).expect("create dir");
        let file = root.join("notes.txt");
        fs::write(&file, "line1\nline2\n").expect("write file");

        let preview = build_preview_text(&file);
        assert!(preview.contains("File:"));
        assert!(preview.contains("Action:"));
        assert!(preview.contains("line1"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn build_preview_text_limits_lines() {
        let root = test_root("preview-limit-lines");
        fs::create_dir_all(&root).expect("create dir");
        let file = root.join("many-lines.txt");
        let body = (1..=40)
            .map(|i| format!("line{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&file, format!("{body}\n")).expect("write file");

        let preview = build_preview_text(&file);
        assert!(preview.contains("line1"));
        assert!(preview.contains("line20"));
        assert!(!preview.contains("line21"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn normalize_path_for_display_strips_extended_prefix_for_drive_path() {
        let raw = PathBuf::from(r"\\?\C:\Users\tester\file.txt");
        assert_eq!(
            normalize_path_for_display(&raw),
            r"C:\Users\tester\file.txt"
        );
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn normalize_path_for_display_converts_unc_extended_prefix() {
        let raw = PathBuf::from(r"\\?\UNC\server\share\folder\file.txt");
        assert_eq!(
            normalize_path_for_display(&raw),
            r"\\server\share\folder\file.txt"
        );
    }
}

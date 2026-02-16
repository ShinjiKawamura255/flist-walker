use crate::actions::choose_action;
use std::collections::HashSet;
use std::path::Path;

pub fn display_path(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.to_string_lossy().to_string())
}

fn find_match_positions(text: &str, query: &str) -> HashSet<usize> {
    let mut out = HashSet::new();
    if query.is_empty() {
        return out;
    }

    let text_lower = text.to_ascii_lowercase();
    let q = query.to_ascii_lowercase();

    if let Some(start) = text_lower.find(&q) {
        for i in start..start + q.len() {
            out.insert(i);
        }
        return out;
    }

    let text_chars: Vec<char> = text_lower.chars().collect();
    let q_chars: Vec<char> = q.chars().collect();
    let mut qi = 0usize;
    for (i, ch) in text_chars.iter().enumerate() {
        if qi < q_chars.len() && *ch == q_chars[qi] {
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

fn highlight_terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    for mut token in query.split_whitespace().map(ToString::to_string) {
        if token.starts_with('!') {
            continue;
        }
        if token.starts_with('\'') {
            token = token[1..].to_string();
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

pub fn match_positions_for_path(path: &Path, root: &Path, query: &str) -> HashSet<usize> {
    let mut positions = HashSet::new();
    let display = display_path(path, root);
    let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or_default();
    let start = display.len().saturating_sub(filename.len());

    for term in highlight_terms(query) {
        let hits = find_match_positions(filename, &term);
        if !hits.is_empty() {
            for pos in hits {
                positions.insert(start + pos);
            }
            continue;
        }
        positions.extend(find_match_positions(&display, &term));
    }
    positions
}

pub fn has_visible_match(path: &Path, root: &Path, query: &str) -> bool {
    if query.trim().is_empty() {
        return true;
    }
    !match_positions_for_path(path, root, query).is_empty()
}

pub fn build_preview_text(path: &Path) -> String {
    if path.is_dir() {
        let children = std::fs::read_dir(path).map(|it| it.count()).ok();
        return match children {
            Some(n) => format!("Directory: {}\nChildren: {}", path.display(), n),
            None => format!("Directory: {}\nChildren: <unavailable>", path.display()),
        };
    }

    let action = format!("{:?}", choose_action(path));
    let head = format!("File: {}\nAction: {}\n", path.display(), action);

    match std::fs::read_to_string(path) {
        Ok(text) => {
            let preview: Vec<&str> = text.lines().take(20).collect();
            if preview.is_empty() {
                format!("{}\n<empty file>", head)
            } else {
                format!("{}\n{}", head, preview.join("\n"))
            }
        }
        Err(_) => format!("{}\n<binary or unreadable file>", head),
    }
}

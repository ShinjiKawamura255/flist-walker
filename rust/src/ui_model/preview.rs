use std::fs::File;
use std::io::Read;
use std::path::Path;

use encoding_rs::{EUC_JP, SHIFT_JIS, UTF_16BE, UTF_16LE, WINDOWS_1252};

use super::{normalize_path_for_display, should_skip_preview};
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
            "File: {}\n\n<on-demand file: preview skipped>",
            normalized_path
        );
    }

    let size_line = std::fs::metadata(path)
        .ok()
        .filter(|metadata| metadata.is_file())
        .map(|metadata| format!("Size: {}\n", format_file_size(metadata.len())))
        .unwrap_or_default();
    let head = format!("File: {}\n{}", normalized_path, size_line);

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

fn format_file_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn read_preview_lines(
    path: &Path,
    max_lines: usize,
    max_bytes: usize,
) -> std::io::Result<Vec<String>> {
    let mut file = File::open(path)?;
    let mut bytes = Vec::with_capacity(max_bytes.min(8192));
    let mut handle = (&mut file).take(max_bytes as u64);
    handle.read_to_end(&mut bytes)?;
    decode_preview_lines(&bytes, max_lines).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "preview decode failed")
    })
}

fn decode_preview_lines(bytes: &[u8], max_lines: usize) -> Option<Vec<String>> {
    if bytes.is_empty() {
        return Some(Vec::new());
    }
    if looks_like_binary(bytes) {
        return None;
    }

    let mut candidates = preview_decoding_candidates(bytes);
    candidates.push(decode_utf8_preview(bytes));
    candidates.extend(preview_fallback_decoders(bytes));

    candidates
        .into_iter()
        .flatten()
        .map(|decoded| split_preview_lines(&decoded, max_lines))
        .next()
}

fn preview_decoding_candidates(bytes: &[u8]) -> Vec<Option<String>> {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return vec![decode_utf8_preview(&bytes[3..])];
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return vec![decode_with_encoding(&bytes[2..], UTF_16LE)];
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return vec![decode_with_encoding(&bytes[2..], UTF_16BE)];
    }
    Vec::new()
}

fn preview_fallback_decoders(bytes: &[u8]) -> Vec<Option<String>> {
    #[cfg(windows)]
    {
        vec![
            decode_with_encoding(bytes, SHIFT_JIS),
            decode_with_encoding(bytes, EUC_JP),
            decode_with_encoding(bytes, WINDOWS_1252),
        ]
    }
    #[cfg(not(windows))]
    {
        vec![
            decode_with_encoding(bytes, SHIFT_JIS),
            decode_with_encoding(bytes, EUC_JP),
            decode_with_encoding(bytes, WINDOWS_1252),
        ]
    }
}

fn decode_utf8_preview(bytes: &[u8]) -> Option<String> {
    std::str::from_utf8(bytes).ok().map(|text| text.to_string())
}

fn decode_with_encoding(bytes: &[u8], encoding: &'static encoding_rs::Encoding) -> Option<String> {
    let (decoded, _used_encoding, had_errors) = encoding.decode(bytes);
    if had_errors {
        return None;
    }
    let text = decoded.into_owned();
    if contains_too_many_control_chars(&text) {
        return None;
    }
    Some(text)
}

fn split_preview_lines(decoded: &str, max_lines: usize) -> Vec<String> {
    decoded
        .lines()
        .take(max_lines)
        .map(|line| line.trim_end_matches('\r').to_string())
        .collect()
}

fn looks_like_binary(bytes: &[u8]) -> bool {
    bytes.contains(&0) && !has_utf16_bom(bytes)
}

fn has_utf16_bom(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF])
}

fn contains_too_many_control_chars(text: &str) -> bool {
    let mut suspicious = 0usize;
    let mut total = 0usize;
    for ch in text.chars() {
        if ch == '\n' || ch == '\r' || ch == '\t' {
            continue;
        }
        total = total.saturating_add(1);
        if ch.is_control() {
            suspicious = suspicious.saturating_add(1);
        }
    }
    total > 0 && suspicious.saturating_mul(20) > total
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
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("fff-rs-ui-{name}-{nonce}"))
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
    fn build_preview_text_for_file_contains_content_without_action_policy() {
        let root = test_root("preview-file");
        fs::create_dir_all(&root).expect("create dir");

        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::fs::PermissionsExt;

            let file = root.join("run.sh");
            fs::write(&file, "#!/bin/sh\necho hi\n").expect("write file");
            let mut perms = fs::metadata(&file).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&file, perms).expect("set permissions");

            let preview = build_preview_text(&file);
            assert!(preview.contains("File:"));
            assert!(preview.contains("Size: 18 B"));
            assert!(preview.contains("hi"));
            assert!(!preview.contains("Action:"));
            assert!(!preview.contains("Execute"));
        }

        #[cfg(target_os = "windows")]
        {
            let file = root.join("tool.exe");
            fs::write(&file, "bin").expect("write file");

            let preview = build_preview_text(&file);
            assert!(preview.contains("File:"));
            assert!(preview.contains("Size: 3 B"));
            assert!(!preview.contains("Action:"));
            assert!(!preview.contains("Execute"));
        }

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn build_preview_text_formats_large_file_size() {
        let root = test_root("preview-size");
        fs::create_dir_all(&root).expect("create dir");
        let file = root.join("large.bin");
        fs::write(&file, vec![0u8; 1536]).expect("write file");

        let preview = build_preview_text(&file);
        assert!(preview.contains("Size: 1.5 KiB"), "{preview}");
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
    fn build_preview_text_decodes_shift_jis_script() {
        let root = test_root("preview-shift-jis");
        fs::create_dir_all(&root).expect("create dir");
        let file = root.join("script.vbs");
        let (encoded, _used_encoding, had_errors) = SHIFT_JIS.encode("msgbox \"こんにちは\"\r\n");
        assert!(!had_errors);
        fs::write(&file, encoded.as_ref()).expect("write shift_jis file");

        let preview = build_preview_text(&file);
        assert!(preview.contains("script.vbs"));
        assert!(preview.contains("こんにちは"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn build_preview_text_decodes_utf16le_with_bom() {
        let root = test_root("preview-utf16le");
        fs::create_dir_all(&root).expect("create dir");
        let file = root.join("script.ps1");
        let mut bytes = vec![0xFF, 0xFE];
        for unit in "Write-Host \"hello\"\r\n".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        fs::write(&file, bytes).expect("write utf16 file");

        let preview = build_preview_text(&file);
        assert!(preview.contains("Write-Host \"hello\""), "{preview}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn build_preview_text_keeps_binary_files_unreadable() {
        let root = test_root("preview-binary");
        fs::create_dir_all(&root).expect("create dir");
        let file = root.join("blob.bin");
        fs::write(&file, [0x00, 0x01, 0x02, 0x03, 0x04]).expect("write binary file");

        let preview = build_preview_text(&file);
        assert!(preview.contains("<binary or unreadable file>"));
        let _ = fs::remove_dir_all(&root);
    }
}

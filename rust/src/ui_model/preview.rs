use std::fs::{File, Metadata};
use std::io::Read;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use encoding_rs::{EUC_JP, SHIFT_JIS, UTF_16BE, UTF_16LE, WINDOWS_1252};

use super::{normalize_path_for_display, should_skip_preview};
pub fn build_preview_text(path: &Path) -> String {
    build_preview_text_with_kind(path, path.is_dir())
}

pub fn build_preview_text_with_kind(path: &Path, is_dir: bool) -> String {
    const PREVIEW_MAX_LINES: usize = 20;
    const PREVIEW_MAX_BYTES: usize = 64 * 1024;

    let normalized_path = normalize_path_for_display(path);
    if !is_dir && should_skip_preview(path, is_dir) {
        return format!("File: {normalized_path}\n\n<on-demand file: preview skipped>");
    }

    let metadata = std::fs::metadata(path).ok();
    let symlink_metadata = std::fs::symlink_metadata(path).ok();
    if is_dir {
        return build_directory_preview_text(
            path,
            &normalized_path,
            metadata.as_ref(),
            symlink_metadata.as_ref(),
        );
    }

    let head = build_entry_header(
        path,
        "File",
        &normalized_path,
        metadata.as_ref(),
        symlink_metadata.as_ref(),
    );
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

fn build_directory_preview_text(
    path: &Path,
    normalized_path: &str,
    metadata: Option<&Metadata>,
    symlink_metadata: Option<&Metadata>,
) -> String {
    const MAX_LINES: usize = 24;
    const MAX_NAME_CHARS: usize = 80;

    let read = std::fs::read_dir(path);
    let Ok(iter) = read else {
        return format!(
            "{}\nChildren: <unavailable>",
            build_entry_header(
                path,
                "Directory",
                normalized_path,
                metadata,
                symlink_metadata
            )
        );
    };

    let mut entries: Vec<_> = iter.flatten().collect();
    entries.sort_by_key(|e| {
        e.file_name()
            .to_string_lossy()
            .to_string()
            .to_ascii_lowercase()
    });

    let total = entries.len();
    let header = build_entry_header(
        path,
        "Directory",
        normalized_path,
        metadata,
        symlink_metadata,
    );
    if total == 0 {
        return format!("{header}\nChildren: 0\n<empty>");
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
        "{header}\nChildren: {total}\nScope: direct children only\n\n{}",
        lines.join("\n")
    )
}

fn build_entry_header(
    path: &Path,
    kind: &str,
    normalized_path: &str,
    metadata: Option<&Metadata>,
    symlink_metadata: Option<&Metadata>,
) -> String {
    let mut lines = vec![format!("{kind}: {normalized_path}")];
    let is_symlink = symlink_metadata
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false);
    let metadata_prefix = if is_symlink { "Target " } else { "" };

    if let Some(metadata) = metadata {
        if kind == "File" && metadata.is_file() {
            lines.push(format!(
                "{metadata_prefix}Size: {}",
                format_file_size(metadata.len())
            ));
        }
        if let Some(created) = metadata.created().ok().and_then(format_system_time) {
            lines.push(format!("{metadata_prefix}Created: {created}"));
        }
        if let Some(updated) = metadata.modified().ok().and_then(format_system_time) {
            lines.push(format!("{metadata_prefix}Updated: {updated}"));
        }
    }

    if let Some(attribute_metadata) = symlink_metadata.or(metadata) {
        let attributes = metadata_attributes(attribute_metadata);
        if !attributes.is_empty() {
            lines.push(format!("Attributes: {}", attributes.join(", ")));
        }
    }

    if is_symlink {
        let target = std::fs::read_link(path)
            .map(|target| normalize_path_for_display(&target))
            .unwrap_or_else(|_| "<unavailable>".to_string());
        lines.push(format!("Target: {target}"));
    }

    lines.join("\n")
}

fn metadata_attributes(metadata: &Metadata) -> Vec<&'static str> {
    let mut attributes = Vec::new();
    if metadata.permissions().readonly() {
        attributes.push("Read-only");
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;

        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x0002;
        if metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0 {
            attributes.push("Hidden");
        }
    }
    attributes
}

fn format_system_time(time: SystemTime) -> Option<String> {
    let seconds = match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_secs()).ok()?,
        Err(error) => {
            let duration = error.duration();
            let seconds = i64::try_from(duration.as_secs()).ok()?;
            seconds
                .checked_neg()?
                .checked_sub(i64::from(duration.subsec_nanos() > 0))?
        }
    };
    let days = seconds.div_euclid(86_400);
    let day_seconds = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_date_from_days(days);
    let hour = day_seconds / 3_600;
    let minute = day_seconds % 3_600 / 60;
    Some(format!(
        "{year:04}-{month:02}-{day:02} {hour:02}:{minute:02} UTC"
    ))
}

// Howard Hinnant's proleptic Gregorian conversion, kept dependency-free so
// preview formatting remains deterministic across supported platforms.
fn civil_date_from_days(days_since_unix_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_unix_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };
    (year, month as u32, day as u32)
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
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
        assert!(preview.contains("Updated:"));
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
    fn build_preview_text_for_file_includes_updated_metadata() {
        let root = test_root("preview-updated");
        fs::create_dir_all(&root).expect("create dir");
        let file = root.join("sample.txt");
        fs::write(&file, "hello\n").expect("write file");

        let preview = build_preview_text(&file);
        assert!(preview.contains("Updated:"), "{preview}");
        if fs::metadata(&file).expect("metadata").created().is_ok() {
            assert!(preview.contains("Created:"), "{preview}");
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn build_preview_text_for_readonly_file_includes_attribute() {
        let root = test_root("preview-readonly");
        fs::create_dir_all(&root).expect("create dir");
        let file = root.join("readonly.txt");
        fs::write(&file, "hello\n").expect("write file");
        let mut permissions = fs::metadata(&file).expect("metadata").permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&file, permissions).expect("set readonly");

        let preview = build_preview_text(&file);
        assert!(preview.contains("Attributes: Read-only"), "{preview}");
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn build_preview_text_for_symlink_includes_target() {
        use std::os::unix::fs::symlink;

        let root = test_root("preview-symlink");
        fs::create_dir_all(&root).expect("create dir");
        let target = root.join("target.txt");
        let link = root.join("link.txt");
        fs::write(&target, "hello\n").expect("write target");
        symlink("target.txt", &link).expect("create symlink");

        let preview = build_preview_text(&link);
        assert!(preview.contains("Target Size: 6 B"), "{preview}");
        assert!(preview.contains("Target Updated:"), "{preview}");
        if fs::metadata(&target)
            .expect("target metadata")
            .created()
            .is_ok()
        {
            assert!(preview.contains("Target Created:"), "{preview}");
        }
        assert!(preview.contains("Target: target.txt"), "{preview}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn format_system_time_uses_compact_utc_format() {
        let timestamp = UNIX_EPOCH + Duration::from_secs(1_783_020_900);

        assert_eq!(
            format_system_time(timestamp).as_deref(),
            Some("2026-07-02 19:35 UTC")
        );
    }

    #[test]
    fn format_system_time_supports_dates_before_unix_epoch() {
        let timestamp = UNIX_EPOCH - Duration::from_secs(60);

        assert_eq!(
            format_system_time(timestamp).as_deref(),
            Some("1969-12-31 23:59 UTC")
        );
    }

    #[cfg(windows)]
    #[test]
    fn build_preview_text_for_hidden_file_includes_attribute() {
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x0002;

        let root = test_root("preview-hidden");
        fs::create_dir_all(&root).expect("create dir");
        let file = root.join("hidden.txt");
        fs::write(&file, "hello\n").expect("write file");
        let original_attributes = windows_file_attributes(&file);
        set_windows_file_attributes(&file, original_attributes | FILE_ATTRIBUTE_HIDDEN);

        let preview = build_preview_text(&file);
        assert!(preview.contains("Attributes: Hidden"), "{preview}");

        set_windows_file_attributes(&file, original_attributes);
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn build_preview_text_for_windows_symlink_includes_target_metadata() {
        use std::os::windows::fs::symlink_file;

        let root = test_root("preview-windows-symlink");
        fs::create_dir_all(&root).expect("create dir");
        let target = root.join("target.txt");
        let link = root.join("link.txt");
        fs::write(&target, "hello\n").expect("write target");
        if let Err(error) = symlink_file("target.txt", &link) {
            eprintln!("skipping Windows symlink preview test: {error}");
            let _ = fs::remove_dir_all(&root);
            return;
        }

        let preview = build_preview_text(&link);
        assert!(preview.contains("Target Size: 6 B"), "{preview}");
        assert!(preview.contains("Target Updated:"), "{preview}");
        assert!(preview.contains("Target:"), "{preview}");
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    fn windows_file_attributes(path: &Path) -> u32 {
        use std::os::windows::fs::MetadataExt;

        fs::symlink_metadata(path)
            .expect("metadata")
            .file_attributes()
    }

    #[cfg(windows)]
    fn set_windows_file_attributes(path: &Path, attributes: u32) {
        use std::os::windows::ffi::OsStrExt;

        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn SetFileAttributesW(path: *const u16, attributes: u32) -> i32;
        }

        let wide = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let result = unsafe { SetFileAttributesW(wide.as_ptr(), attributes) };
        assert_ne!(
            result,
            0,
            "SetFileAttributesW failed for {}",
            path.display()
        );
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

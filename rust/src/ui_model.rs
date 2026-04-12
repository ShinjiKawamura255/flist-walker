use crate::actions::choose_action;
use crate::path_utils::{
    display_path_with_mode as display_display_path_with_mode,
    normalize_path_for_display as normalize_display_path,
};
use crate::query::{
    include_alternatives, parse_include_alternative, parse_query, split_anchor,
    token_uses_regex_syntax,
};
use encoding_rs::{EUC_JP, SHIFT_JIS, UTF_16BE, UTF_16LE, WINDOWS_1252};
use regex::RegexBuilder;
use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::path::Path;
#[cfg(windows)]
use std::{ffi::c_void, os::windows::ffi::OsStrExt, ptr, sync::OnceLock};

pub fn display_path(path: &Path, root: &Path) -> String {
    display_path_with_mode(path, root, true)
}

pub fn normalize_path_for_display(path: &Path) -> String {
    normalize_display_path(path)
}

pub fn display_path_with_mode(path: &Path, root: &Path, prefer_relative: bool) -> String {
    display_display_path_with_mode(path, root, prefer_relative)
}

fn chars_equal(a: char, b: char, ignore_case: bool) -> bool {
    if ignore_case && a.is_ascii() && b.is_ascii() {
        a.eq_ignore_ascii_case(&b)
    } else {
        a == b
    }
}

fn find_fuzzy_match_positions(text: &str, query: &str, ignore_case: bool) -> HashSet<usize> {
    let mut out = HashSet::new();
    if query.is_empty() {
        return out;
    }

    let text_chars: Vec<char> = text.chars().collect();
    let q_chars: Vec<char> = query.chars().collect();
    if q_chars.is_empty() {
        return out;
    }

    if q_chars.len() <= text_chars.len() {
        for start in 0..=text_chars.len() - q_chars.len() {
            if q_chars
                .iter()
                .enumerate()
                .all(|(offset, q)| chars_equal(text_chars[start + offset], *q, ignore_case))
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
        if qi < q_chars.len() && chars_equal(*ch, q_chars[qi], ignore_case) {
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

fn exact_candidate_positions(text: &str, candidate: &str, ignore_case: bool) -> HashSet<usize> {
    let mut out = HashSet::new();
    let (anchored_start, anchored_end, core) = split_anchor(candidate);
    if core.is_empty() {
        return out;
    }

    let text_chars: Vec<char> = text.chars().collect();
    let core_chars: Vec<char> = core.chars().collect();
    if core_chars.len() > text_chars.len() {
        return out;
    }

    for start in 0..=text_chars.len() - core_chars.len() {
        if !core_chars
            .iter()
            .enumerate()
            .all(|(offset, query)| chars_equal(text_chars[start + offset], *query, ignore_case))
        {
            continue;
        }
        if anchored_start && start != 0 {
            continue;
        }
        if anchored_end && start + core_chars.len() != text_chars.len() {
            continue;
        }

        for idx in start..start + core_chars.len() {
            out.insert(idx);
        }
        return out;
    }

    out
}

fn exact_term_positions(text: &str, term: &str, ignore_case: bool) -> HashSet<usize> {
    for candidate in include_alternatives(term) {
        let positions = exact_candidate_positions(text, candidate, ignore_case);
        if !positions.is_empty() {
            return positions;
        }
    }
    HashSet::new()
}

fn include_candidate_positions(text: &str, candidate: &str, ignore_case: bool) -> HashSet<usize> {
    let Some((exact, candidate)) = parse_include_alternative(candidate) else {
        return HashSet::new();
    };
    if exact {
        return exact_candidate_positions(text, &candidate, ignore_case);
    }

    let (anchored_start, anchored_end, core) = split_anchor(&candidate);
    if core.is_empty() {
        return HashSet::new();
    }
    if anchored_start {
        let Some(first_char) = core.chars().next() else {
            return HashSet::new();
        };
        if !text
            .chars()
            .next()
            .is_some_and(|value| chars_equal(value, first_char, ignore_case))
        {
            return HashSet::new();
        }
    }
    if anchored_end {
        let Some(last_char) = core.chars().last() else {
            return HashSet::new();
        };
        if !text
            .chars()
            .last()
            .is_some_and(|value| chars_equal(value, last_char, ignore_case))
        {
            return HashSet::new();
        }
    }

    find_fuzzy_match_positions(text, core, ignore_case)
}

pub fn match_positions_for_path(
    path: &Path,
    root: &Path,
    query: &str,
    prefer_relative: bool,
    use_regex: bool,
    ignore_case: bool,
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

    let spec = parse_query(query);

    for term in &spec.exact_terms {
        let hits = exact_term_positions(filename, term, ignore_case);
        if !hits.is_empty() {
            for pos in hits {
                positions.insert(start + pos);
            }
            continue;
        }
        let hits = exact_term_positions(&display, term, ignore_case);
        if !hits.is_empty() {
            positions.extend(hits);
        }
    }

    for term in &spec.include_terms {
        if use_regex && token_uses_regex_syntax(term) {
            let hits = find_regex_match_positions(filename, term, ignore_case);
            if !hits.is_empty() {
                for pos in hits {
                    positions.insert(start + pos);
                }
                continue;
            }
            positions.extend(find_regex_match_positions(&display, term, ignore_case));
            continue;
        }

        let mut matched_any = false;
        for candidate in include_alternatives(term) {
            let hits = include_candidate_positions(filename, candidate, ignore_case);
            if !hits.is_empty() {
                for pos in hits {
                    positions.insert(start + pos);
                }
                matched_any = true;
                break;
            }
            let hits = include_candidate_positions(&display, candidate, ignore_case);
            if !hits.is_empty() {
                positions.extend(hits);
                matched_any = true;
                break;
            }
        }
        if matched_any {
            continue;
        }
    }
    positions
}

pub fn has_visible_match(
    path: &Path,
    root: &Path,
    query: &str,
    prefer_relative: bool,
    ignore_case: bool,
) -> bool {
    crate::query::has_visible_match(path, root, query, prefer_relative, ignore_case)
}

fn find_regex_match_positions(text: &str, pattern: &str, ignore_case: bool) -> HashSet<usize> {
    let mut out = HashSet::new();
    let Ok(re) = RegexBuilder::new(pattern)
        .case_insensitive(ignore_case)
        .build()
    else {
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

pub fn should_skip_preview(path: &Path, is_dir: bool) -> bool {
    !is_dir && is_on_demand_file(path)
}

fn is_on_demand_file(path: &Path) -> bool {
    #[cfg(windows)]
    {
        if let Some(info) = read_file_attribute_tag_info(path) {
            return should_skip_preview_from_attr_tag(info.file_attributes, Some(info.reparse_tag));
        }

        return std::fs::metadata(path)
            .map(|m| should_skip_preview_from_attr_tag(metadata_file_attributes(&m), None))
            .unwrap_or(false);
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug)]
struct FileAttributeTagInfoRecord {
    file_attributes: u32,
    reparse_tag: u32,
}

#[cfg(windows)]
fn metadata_file_attributes(metadata: &std::fs::Metadata) -> u32 {
    use std::os::windows::fs::MetadataExt;

    metadata.file_attributes()
}

#[cfg_attr(not(any(test, windows)), allow(dead_code))]
fn should_skip_preview_from_attr_tag(file_attributes: u32, reparse_tag: Option<u32>) -> bool {
    has_on_demand_attributes(file_attributes)
        || reparse_tag
            .map(|tag| is_cloud_placeholder(file_attributes, tag))
            .unwrap_or(false)
}

#[cfg_attr(not(any(test, windows)), allow(dead_code))]
fn has_on_demand_attributes(file_attributes: u32) -> bool {
    const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;
    const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x0004_0000;
    const FILE_ATTRIBUTE_UNPINNED: u32 = 0x0010_0000;
    const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;

    (file_attributes
        & (FILE_ATTRIBUTE_OFFLINE
            | FILE_ATTRIBUTE_RECALL_ON_OPEN
            | FILE_ATTRIBUTE_UNPINNED
            | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS))
        != 0
}

#[cfg(windows)]
fn is_cloud_placeholder(file_attributes: u32, reparse_tag: u32) -> bool {
    cf_get_placeholder_state_from_attribute_tag(file_attributes, reparse_tag) != 0
}

#[cfg(not(windows))]
#[cfg_attr(not(test), allow(dead_code))]
fn is_cloud_placeholder(_file_attributes: u32, _reparse_tag: u32) -> bool {
    false
}

#[cfg(windows)]
fn read_file_attribute_tag_info(path: &Path) -> Option<FileAttributeTagInfoRecord> {
    const FILE_READ_ATTRIBUTES: u32 = 0x0080;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    const FILE_SHARE_DELETE: u32 = 0x0000_0004;
    const OPEN_EXISTING: u32 = 3;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_ATTRIBUTE_TAG_INFO_CLASS: i32 = 9;

    let mut wide_path: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide_path.push(0);

    let handle = unsafe {
        create_file_w(
            wide_path.as_ptr(),
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ptr::null_mut(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            ptr::null_mut(),
        )
    };

    if handle == invalid_handle_value() {
        return None;
    }

    let mut info = RawFileAttributeTagInfo {
        file_attributes: 0,
        reparse_tag: 0,
    };
    let ok = unsafe {
        get_file_information_by_handle_ex(
            handle,
            FILE_ATTRIBUTE_TAG_INFO_CLASS,
            (&mut info as *mut RawFileAttributeTagInfo).cast::<c_void>(),
            std::mem::size_of::<RawFileAttributeTagInfo>() as u32,
        ) != 0
    };
    unsafe {
        close_handle(handle);
    }

    ok.then_some(FileAttributeTagInfoRecord {
        file_attributes: info.file_attributes,
        reparse_tag: info.reparse_tag,
    })
}

#[cfg(windows)]
#[repr(C)]
struct RawFileAttributeTagInfo {
    file_attributes: u32,
    reparse_tag: u32,
}

#[cfg(windows)]
fn invalid_handle_value() -> *mut c_void {
    (-1isize) as *mut c_void
}

#[cfg(all(windows, target_env = "gnu"))]
#[link(name = "kernel32")]
extern "system" {
    fn CreateFileW(
        lpFileName: *const u16,
        dwDesiredAccess: u32,
        dwShareMode: u32,
        lpSecurityAttributes: *mut c_void,
        dwCreationDisposition: u32,
        dwFlagsAndAttributes: u32,
        hTemplateFile: *mut c_void,
    ) -> *mut c_void;
    fn GetFileInformationByHandleEx(
        hFile: *mut c_void,
        FileInformationClass: i32,
        lpFileInformation: *mut c_void,
        dwBufferSize: u32,
    ) -> i32;
    fn CloseHandle(hObject: *mut c_void) -> i32;
    fn LoadLibraryW(lpLibFileName: *const u16) -> *mut c_void;
    fn GetProcAddress(hModule: *mut c_void, lpProcName: *const u8) -> *mut c_void;
}

#[cfg(all(windows, not(target_env = "gnu")))]
#[link(name = "Kernel32")]
extern "system" {
    fn CreateFileW(
        lpFileName: *const u16,
        dwDesiredAccess: u32,
        dwShareMode: u32,
        lpSecurityAttributes: *mut c_void,
        dwCreationDisposition: u32,
        dwFlagsAndAttributes: u32,
        hTemplateFile: *mut c_void,
    ) -> *mut c_void;
    fn GetFileInformationByHandleEx(
        hFile: *mut c_void,
        FileInformationClass: i32,
        lpFileInformation: *mut c_void,
        dwBufferSize: u32,
    ) -> i32;
    fn CloseHandle(hObject: *mut c_void) -> i32;
    fn LoadLibraryW(lpLibFileName: *const u16) -> *mut c_void;
    fn GetProcAddress(hModule: *mut c_void, lpProcName: *const u8) -> *mut c_void;
}

#[cfg(windows)]
unsafe fn create_file_w(
    path: *const u16,
    desired_access: u32,
    share_mode: u32,
    security_attributes: *mut c_void,
    creation_disposition: u32,
    flags_and_attributes: u32,
    template_file: *mut c_void,
) -> *mut c_void {
    CreateFileW(
        path,
        desired_access,
        share_mode,
        security_attributes,
        creation_disposition,
        flags_and_attributes,
        template_file,
    )
}

#[cfg(windows)]
unsafe fn get_file_information_by_handle_ex(
    handle: *mut c_void,
    info_class: i32,
    file_information: *mut c_void,
    buffer_size: u32,
) -> i32 {
    GetFileInformationByHandleEx(handle, info_class, file_information, buffer_size)
}

#[cfg(windows)]
unsafe fn close_handle(handle: *mut c_void) -> i32 {
    CloseHandle(handle)
}

#[cfg(windows)]
fn cf_get_placeholder_state_from_attribute_tag(file_attributes: u32, reparse_tag: u32) -> u32 {
    type CfGetPlaceholderStateFromAttributeTagFn = unsafe extern "system" fn(u32, u32) -> u32;

    fn resolve() -> Option<CfGetPlaceholderStateFromAttributeTagFn> {
        static FN: OnceLock<Option<CfGetPlaceholderStateFromAttributeTagFn>> = OnceLock::new();

        *FN.get_or_init(|| {
            let mut dll_name: Vec<u16> = "cldapi.dll".encode_utf16().collect();
            dll_name.push(0);
            let module = unsafe { LoadLibraryW(dll_name.as_ptr()) };
            if module.is_null() {
                return None;
            }

            let proc = unsafe {
                GetProcAddress(module, b"CfGetPlaceholderStateFromAttributeTag\0".as_ptr())
            };
            if proc.is_null() {
                None
            } else {
                Some(unsafe {
                    std::mem::transmute::<*mut c_void, CfGetPlaceholderStateFromAttributeTagFn>(
                        proc,
                    )
                })
            }
        })
    }

    resolve()
        .map(|func| unsafe { func(file_attributes, reparse_tag) })
        .unwrap_or(0)
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
        let positions = match_positions_for_path(&path, &root, "read", true, false, true);
        assert!(!positions.is_empty());
    }

    #[test]
    fn match_positions_multibyte_query_only_highlights_matched_chars() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/日本語/テスト資料.txt");
        let positions = match_positions_for_path(&path, &root, "テスト", true, false, true);
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

        let positions = match_positions_for_path(&sample, &root, "main !readme", true, false, true);
        assert!(positions.len() >= 4);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn match_positions_support_exact_token_prefix() {
        let root = test_root("highlight-exact");
        let sample = root.join("src/main.py");
        fs::create_dir_all(sample.parent().expect("parent")).expect("create parent");
        fs::write(&sample, "print('x')\n").expect("write sample");

        let positions = match_positions_for_path(&sample, &root, "'main", true, false, true);
        assert!(positions.len() >= 4);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn exact_token_does_not_fall_back_to_subsequence_matching() {
        let root = test_root("highlight-exact-no-fuzzy");
        let sample = root.join("src/m-a-i-n.txt");
        fs::create_dir_all(sample.parent().expect("parent")).expect("create parent");
        fs::write(&sample, "print('x')\n").expect("write sample");

        let positions = match_positions_for_path(&sample, &root, "'main", true, false, true);
        assert!(positions.is_empty());
        assert!(!has_visible_match(&sample, &root, "'main", true, true));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn has_visible_match_false_when_term_not_in_visible_text() {
        let root = test_root("visible-match");
        let sample = root.join("src/main.py");
        fs::create_dir_all(sample.parent().expect("parent")).expect("create parent");
        fs::write(&sample, "print('x')\n").expect("write sample");

        assert!(!has_visible_match(&sample, &root, "zzzz", true, true));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn case_sensitive_highlight_and_visibility_respect_ignore_case_flag() {
        let root = test_root("visible-case-sensitive");
        let sample = root.join("src/Main.py");
        fs::create_dir_all(sample.parent().expect("parent")).expect("create parent");
        fs::write(&sample, "print('x')\n").expect("write sample");

        let sensitive_positions =
            match_positions_for_path(&sample, &root, "main", true, false, false);
        assert!(sensitive_positions.is_empty());
        assert!(!has_visible_match(&sample, &root, "main", true, false));

        let insensitive_positions =
            match_positions_for_path(&sample, &root, "main", true, false, true);
        assert!(!insensitive_positions.is_empty());
        assert!(has_visible_match(&sample, &root, "main", true, true));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn has_visible_match_true_for_exclusion_only_query() {
        let root = test_root("visible-exclusion-only");
        let sample = root.join("src/main.py");
        fs::create_dir_all(sample.parent().expect("parent")).expect("create parent");
        fs::write(&sample, "print('x')\n").expect("write sample");

        assert!(has_visible_match(&sample, &root, "!readme", true, true));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn match_positions_regex_query_highlights_matched_span() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/main.py");
        let positions = match_positions_for_path(&path, &root, "ma.*py", true, true, true);
        assert!(!positions.is_empty());
    }

    #[test]
    fn match_positions_regex_mode_plain_token_uses_fuzzy_highlight() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/a-b-c.txt");
        let positions = match_positions_for_path(&path, &root, "abc", true, true, true);
        assert!(!positions.is_empty());
    }

    #[test]
    fn match_positions_regex_mode_plain_or_token_uses_fuzzy_highlight() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/f-o-o.txt");
        let positions = match_positions_for_path(&path, &root, "abc|foo", true, true, true);
        assert!(!positions.is_empty());
    }

    #[test]
    fn match_positions_or_token_highlights_selected_alternative() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/foo.txt");
        let positions = match_positions_for_path(&path, &root, "abc|foo|bar", true, false, true);
        assert!(!positions.is_empty());
    }

    #[test]
    fn match_positions_or_token_with_left_exact_keeps_left_candidate() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/main.txt");
        let positions = match_positions_for_path(&path, &root, "'main|", true, false, true);
        assert!(!positions.is_empty());
        assert!(has_visible_match(&path, &root, "'main|", true, true));
    }

    #[test]
    fn match_positions_or_token_supports_exact_on_right_side() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/xyz.txt");
        let positions = match_positions_for_path(&path, &root, "abc|'xyz", true, false, true);
        assert!(!positions.is_empty());
    }

    #[test]
    fn exact_alternative_in_or_query_does_not_fall_back_to_subsequence_matching() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/m-a-i-n.txt");
        let positions = match_positions_for_path(&path, &root, "abc|'main", true, false, true);
        assert!(positions.is_empty());
        assert!(!has_visible_match(&path, &root, "abc|'main", true, true));
    }

    #[test]
    fn has_visible_match_or_token_uses_alternative_hits() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/src/bar.txt");
        assert!(has_visible_match(&path, &root, "abc|foo|bar", true, true));
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

    #[test]
    fn on_demand_attribute_bits_skip_preview_without_reparse_tag() {
        assert!(should_skip_preview_from_attr_tag(0x0000_1000, None));
        assert!(should_skip_preview_from_attr_tag(0x0004_0000, None));
        assert!(should_skip_preview_from_attr_tag(0x0010_0000, None));
        assert!(should_skip_preview_from_attr_tag(0x0040_0000, None));
    }

    #[test]
    fn plain_file_attributes_do_not_skip_preview() {
        assert!(!should_skip_preview_from_attr_tag(0, None));
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

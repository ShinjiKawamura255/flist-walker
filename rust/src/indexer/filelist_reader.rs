use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use super::filelist_hierarchy::apply_nested_filelist_overrides;
use super::filelist_writer::filelist_modified_time;

const FILELIST_READ_BUFFER_BYTES: usize = 1024 * 1024;
const FILELIST_VALIDATION_CHUNK_BYTES: usize = 64 * 1024;
const FILELIST_MAX_LINE_PAYLOAD_BYTES: usize = 1024 * 1024;
const FILELIST_MAX_RAW_LINE_BYTES: usize = FILELIST_MAX_LINE_PAYLOAD_BYTES + 5;
const UTF8_BOM: &[u8; 3] = b"\xEF\xBB\xBF";

pub fn find_filelist(root: &Path) -> Option<PathBuf> {
    let upper = root.join("FileList.txt");
    if upper.is_file() {
        return Some(upper);
    }
    let lower = root.join("filelist.txt");
    if lower.is_file() {
        return Some(lower);
    }

    fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.is_file()
                && p.file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.eq_ignore_ascii_case("filelist.txt"))
                    == Some(true)
        })
}

pub fn find_filelist_in_first_level(root: &Path) -> Option<PathBuf> {
    find_filelist(root)
}

pub fn parse_filelist(
    filelist_path: &Path,
    root: &Path,
    include_files: bool,
    include_dirs: bool,
) -> Result<Vec<PathBuf>> {
    parse_filelist_collect(filelist_path, root, include_files, include_dirs, &|| false)
}

pub fn build_entries_from_filelist_hierarchy<C>(
    filelist_path: &Path,
    root: &Path,
    include_files: bool,
    include_dirs: bool,
    should_cancel: C,
) -> Result<Vec<PathBuf>>
where
    C: Fn() -> bool,
{
    let root_modified = filelist_modified_time(filelist_path);
    let mut entries = Vec::new();
    parse_filelist_stream(
        filelist_path,
        root,
        include_files,
        include_dirs,
        &should_cancel,
        |path, _is_dir| entries.push(path),
    )?;
    apply_nested_filelist_overrides(
        filelist_path,
        root,
        root_modified,
        &mut entries,
        include_files,
        include_dirs,
        &should_cancel,
    )?;
    Ok(entries)
}

pub fn apply_filelist_hierarchy_overrides<C>(
    filelist_path: &Path,
    root: &Path,
    entries: &mut Vec<PathBuf>,
    include_files: bool,
    include_dirs: bool,
    should_cancel: C,
) -> Result<bool>
where
    C: Fn() -> bool,
{
    let root_modified = filelist_modified_time(filelist_path);
    apply_nested_filelist_overrides(
        filelist_path,
        root,
        root_modified,
        entries,
        include_files,
        include_dirs,
        &should_cancel,
    )
}

pub(super) fn parse_filelist_collect<C>(
    filelist_path: &Path,
    root: &Path,
    include_files: bool,
    include_dirs: bool,
    should_cancel: &C,
) -> Result<Vec<PathBuf>>
where
    C: Fn() -> bool,
{
    let mut out = Vec::new();
    parse_filelist_stream(
        filelist_path,
        root,
        include_files,
        include_dirs,
        should_cancel,
        |path, _is_dir| out.push(path),
    )?;
    Ok(out)
}

struct ValidatedFileListLine<'a> {
    logical: &'a str,
    serialized_without_bom: &'a str,
}

fn invalid_filelist_encoding(
    filelist_path: &Path,
    byte_offset: usize,
    detail: &str,
) -> anyhow::Error {
    anyhow::anyhow!(
        "invalid FileList encoding at byte {byte_offset} in {}: {detail}; expected UTF-8 (optional BOM)",
        filelist_path.display()
    )
}

struct CancellableLineReader<'a, R, C> {
    inner: &'a mut R,
    should_cancel: &'a C,
    filelist_path: &'a Path,
    line_number: usize,
    consumed: usize,
}

#[derive(Debug)]
struct FileListReadCanceled;

impl std::fmt::Display for FileListReadCanceled {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("superseded")
    }
}

impl std::error::Error for FileListReadCanceled {}

fn is_filelist_read_canceled(error: &std::io::Error) -> bool {
    error
        .get_ref()
        .and_then(|source| source.downcast_ref::<FileListReadCanceled>())
        .is_some()
}

impl<R, C> Read for CancellableLineReader<'_, R, C>
where
    R: BufRead,
    C: Fn() -> bool,
{
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        let copied = {
            let available = self.fill_buf()?;
            let copied = available.len().min(output.len());
            output[..copied].copy_from_slice(&available[..copied]);
            copied
        };
        self.consume(copied);
        Ok(copied)
    }
}

impl<R, C> BufRead for CancellableLineReader<'_, R, C>
where
    R: BufRead,
    C: Fn() -> bool,
{
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        if (self.should_cancel)() {
            return Err(std::io::Error::other(FileListReadCanceled));
        }
        let available = self.inner.fill_buf()?;
        if available.is_empty() {
            return Ok(available);
        }
        if self.consumed >= FILELIST_MAX_RAW_LINE_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "FileList line {} in {} exceeds the 1 MiB logical line limit",
                    self.line_number,
                    self.filelist_path.display()
                ),
            ));
        }
        let remaining = FILELIST_MAX_RAW_LINE_BYTES - self.consumed;
        let exposed = available
            .len()
            .min(FILELIST_VALIDATION_CHUNK_BYTES)
            .min(remaining);
        Ok(&available[..exposed])
    }

    fn consume(&mut self, amount: usize) {
        self.consumed = self.consumed.saturating_add(amount);
        self.inner.consume(amount);
    }
}

fn read_bounded_raw_line<R, C>(
    reader: &mut R,
    raw: &mut Vec<u8>,
    filelist_path: &Path,
    line_number: usize,
    should_cancel: &C,
) -> Result<usize>
where
    R: BufRead,
    C: Fn() -> bool,
{
    raw.clear();
    let mut cancellable = CancellableLineReader {
        inner: reader,
        should_cancel,
        filelist_path,
        line_number,
        consumed: 0,
    };
    match cancellable.read_until(b'\n', raw) {
        Ok(bytes_read) => Ok(bytes_read),
        Err(err) if is_filelist_read_canceled(&err) => Err(anyhow::anyhow!("superseded")),
        Err(err) => Err(anyhow::anyhow!(
            "failed to read {}: {err}",
            filelist_path.display()
        )),
    }
}

fn validate_filelist_line<'a>(
    raw: &'a [u8],
    first_line: bool,
    line_start_offset: usize,
    line_number: usize,
    filelist_path: &Path,
) -> Result<ValidatedFileListLine<'a>> {
    if first_line && (raw.starts_with(&[0xFF, 0xFE]) || raw.starts_with(&[0xFE, 0xFF])) {
        return Err(invalid_filelist_encoding(
            filelist_path,
            line_start_offset,
            "UTF-16 BOM is not supported",
        ));
    }

    let bom_len = usize::from(first_line && raw.starts_with(UTF8_BOM)) * UTF8_BOM.len();
    let serialized = &raw[bom_len..];
    let mut payload_end = serialized.len();
    if serialized.get(payload_end.wrapping_sub(1)) == Some(&b'\n') {
        payload_end -= 1;
        if serialized.get(payload_end.wrapping_sub(1)) == Some(&b'\r') {
            payload_end -= 1;
        }
    }
    if payload_end > FILELIST_MAX_LINE_PAYLOAD_BYTES {
        anyhow::bail!(
            "FileList line {line_number} in {} exceeds the 1 MiB logical line limit",
            filelist_path.display()
        );
    }
    if serialized.contains(&0) {
        let nul_offset = serialized
            .iter()
            .position(|byte| *byte == 0)
            .expect("contains confirmed a NUL byte");
        return Err(invalid_filelist_encoding(
            filelist_path,
            line_start_offset + bom_len + nul_offset,
            "NUL bytes are not allowed",
        ));
    }
    let serialized_without_bom = std::str::from_utf8(serialized).map_err(|err| {
        invalid_filelist_encoding(
            filelist_path,
            line_start_offset + bom_len + err.valid_up_to(),
            "malformed or unsupported byte sequence",
        )
    })?;
    Ok(ValidatedFileListLine {
        logical: &serialized_without_bom[..payload_end],
        serialized_without_bom,
    })
}

fn validate_filelist_reader<R, C>(
    reader: &mut R,
    filelist_path: &Path,
    should_cancel: &C,
) -> Result<()>
where
    R: BufRead,
    C: Fn() -> bool,
{
    let mut raw = Vec::new();
    let mut line_number = 1usize;
    let mut line_start_offset = 0usize;
    loop {
        let bytes_read =
            read_bounded_raw_line(reader, &mut raw, filelist_path, line_number, should_cancel)?;
        if bytes_read == 0 {
            return Ok(());
        }
        validate_filelist_line(
            &raw,
            line_number == 1,
            line_start_offset,
            line_number,
            filelist_path,
        )?;
        line_start_offset = line_start_offset.saturating_add(bytes_read);
        line_number = line_number.saturating_add(1);
    }
}

pub(crate) fn open_validated_filelist<C>(
    filelist_path: &Path,
    should_cancel: &C,
) -> Result<BufReader<File>>
where
    C: Fn() -> bool,
{
    let file = File::open(filelist_path)
        .with_context(|| format!("failed to read {}", filelist_path.display()))?;
    let mut reader = BufReader::with_capacity(FILELIST_READ_BUFFER_BYTES, file);
    validate_filelist_reader(&mut reader, filelist_path, should_cancel)?;
    reader
        .seek(SeekFrom::Start(0))
        .with_context(|| format!("failed to reread {}", filelist_path.display()))?;
    Ok(reader)
}

#[cfg(test)]
pub(crate) fn validate_filelist_encoding<C>(filelist_path: &Path, should_cancel: &C) -> Result<()>
where
    C: Fn() -> bool,
{
    open_validated_filelist(filelist_path, should_cancel).map(|_| ())
}

pub(super) fn read_filelist_text_strict(filelist_path: &Path) -> std::io::Result<String> {
    let file = File::open(filelist_path)?;
    let mut reader = BufReader::with_capacity(FILELIST_READ_BUFFER_BYTES, file);
    let mut raw = Vec::new();
    let mut text = String::new();
    let mut line_number = 1usize;
    let mut line_start_offset = 0usize;
    loop {
        let bytes_read =
            read_bounded_raw_line(&mut reader, &mut raw, filelist_path, line_number, &|| false)
                .map_err(|err| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, err.to_string())
                })?;
        if bytes_read == 0 {
            return Ok(text);
        }
        let line = validate_filelist_line(
            &raw,
            line_number == 1,
            line_start_offset,
            line_number,
            filelist_path,
        )
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err.to_string()))?;
        text.push_str(line.serialized_without_bom);
        line_start_offset = line_start_offset.saturating_add(bytes_read);
        line_number = line_number.saturating_add(1);
    }
}

pub fn parse_filelist_stream<F, C>(
    filelist_path: &Path,
    root: &Path,
    include_files: bool,
    include_dirs: bool,
    should_cancel: C,
    mut on_entry: F,
) -> Result<()>
where
    F: FnMut(PathBuf, Option<bool>),
    C: Fn() -> bool,
{
    let mut reader = open_validated_filelist(filelist_path, &should_cancel)?;

    let mut raw = Vec::new();
    let mut seen = HashSet::new();
    let filelist_base = filelist_path.parent().unwrap_or(root);
    let mut line_number = 1usize;
    let mut line_start_offset = 0usize;
    loop {
        let bytes_read = read_bounded_raw_line(
            &mut reader,
            &mut raw,
            filelist_path,
            line_number,
            &should_cancel,
        )?;
        if bytes_read == 0 {
            break;
        }
        let validated = validate_filelist_line(
            &raw,
            line_number == 1,
            line_start_offset,
            line_number,
            filelist_path,
        )?;
        line_start_offset = line_start_offset.saturating_add(bytes_read);
        line_number = line_number.saturating_add(1);
        let line = validated.logical.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let candidates = resolve_filelist_entry_candidates(line, filelist_base, root);
        if include_files && include_dirs {
            // Keep FileList indexing on the current control fast path: choose the
            // platform-preferred lexical candidate and avoid per-line existence probes
            // in the initial stream.
            if let Some(path) = candidates.into_iter().next() {
                if seen.insert(path.clone()) {
                    on_entry(path, None);
                }
            }
            continue;
        }

        for candidate in candidates {
            let Ok(meta) = candidate.metadata() else {
                continue;
            };
            let is_dir = meta.is_dir();
            let is_file = meta.is_file();
            if is_file && !include_files {
                continue;
            }
            if is_dir && !include_dirs {
                continue;
            }
            if !is_file && !is_dir {
                continue;
            }
            if seen.insert(candidate.clone()) {
                on_entry(candidate, Some(is_dir));
            }
            break;
        }
    }
    Ok(())
}

pub(crate) fn resolve_filelist_entry_candidates(
    line: &str,
    filelist_base: &Path,
    root: &Path,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let raw = strip_wrapping_quotes(line);
    if !raw.contains('\\') {
        if raw.is_empty() {
            return candidates;
        }
        let p = Path::new(raw);
        if p.is_absolute() {
            push_unique_candidate(&mut candidates, p.to_path_buf());
            return candidates;
        }

        push_unique_candidate(&mut candidates, filelist_base.join(p));
        if filelist_base != root {
            push_unique_candidate(&mut candidates, root.join(p));
        }
        return candidates;
    }

    let raws = preferred_filelist_raw_candidates(raw);
    for raw in raws {
        if raw.is_empty() {
            continue;
        }
        let p = PathBuf::from(&raw);
        if p.is_absolute() {
            push_unique_candidate(&mut candidates, p.clone());
        } else if looks_like_windows_absolute_path(&raw) {
            #[cfg(windows)]
            {
                push_unique_candidate(&mut candidates, PathBuf::from(&raw));
            }
            #[cfg(not(windows))]
            {
                if let Some(wsl) = windows_path_to_wsl(&raw) {
                    push_unique_candidate(&mut candidates, wsl);
                }
            }
        } else {
            push_unique_candidate(&mut candidates, filelist_base.join(&p));
            if filelist_base != root {
                push_unique_candidate(&mut candidates, root.join(&p));
            }
        }
    }
    candidates
}

fn preferred_filelist_raw_candidates(raw: &str) -> Vec<String> {
    if !raw.contains('\\') {
        return vec![raw.to_string()];
    }

    let normalized = raw.replace('\\', "/");
    #[cfg(windows)]
    {
        if normalized == raw {
            vec![raw.to_string()]
        } else {
            vec![raw.to_string(), normalized]
        }
    }
    #[cfg(not(windows))]
    {
        if normalized == raw {
            vec![raw.to_string()]
        } else {
            vec![normalized, raw.to_string()]
        }
    }
}

fn push_unique_candidate(candidates: &mut Vec<PathBuf>, candidate: PathBuf) {
    if !candidates.iter().any(|existing| existing == &candidate) {
        candidates.push(candidate);
    }
}

pub(super) fn strip_wrapping_quotes(line: &str) -> &str {
    let bytes = line.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        &line[1..line.len() - 1]
    } else {
        line
    }
}

pub(super) fn looks_like_windows_absolute_path(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    if raw.starts_with(r"\\") {
        return true;
    }
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
}

#[cfg(not(windows))]
pub(crate) fn windows_path_to_wsl(raw: &str) -> Option<PathBuf> {
    let bytes = raw.as_bytes();
    if bytes.len() < 3
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || (bytes[2] != b'\\' && bytes[2] != b'/')
    {
        return None;
    }
    let drive = (bytes[0] as char).to_ascii_lowercase();
    let rest = raw[3..].replace('\\', "/");
    Some(PathBuf::from(format!("/mnt/{drive}/{rest}")))
}

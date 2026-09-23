use std::fs::{File, Metadata};
use std::io::{Read, Seek, SeekFrom};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use encoding_rs::{EUC_JP, SHIFT_JIS, WINDOWS_1252};

use super::{metadata_attributes, normalize_path_for_display, should_skip_preview};
use super::{SyntaxHighlight, SyntaxLanguage, SyntaxSpan};

const INITIAL_LINE_LIMIT: usize = 100;
const INITIAL_BYTE_LIMIT: usize = 64 * 1024;
const MORE_LINE_LIMIT: usize = 500;
const MORE_BYTE_LIMIT: usize = 256 * 1024;
const TOTAL_LINE_LIMIT: usize = 5_000;
const TOTAL_BYTE_LIMIT: usize = 1024 * 1024;
const DECODED_BYTE_LIMIT: usize = 4 * 1024 * 1024;
const READ_BLOCK: usize = 4 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewPageError {
    Empty,
    Binary,
    DecodeFailed,
    PermissionDenied,
    NotFound,
    OnDemandSkipped,
    ReadFailed,
    Changed,
    LimitReached,
    Canceled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewPageState {
    More,
    Eof,
    LimitReached,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewEncoding {
    Utf8,
    Utf16Le,
    Utf16Be,
    ShiftJis,
    EucJp,
    Windows1252,
}

#[derive(Clone, Debug)]
pub struct PreviewHeader {
    pub path: PathBuf,
    pub size: u64,
    pub created: Option<SystemTime>,
    pub modified: Option<SystemTime>,
    pub is_symlink: bool,
    pub target: Option<String>,
    pub attributes: Vec<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileIdentity {
    size: u64,
    modified: Option<SystemTime>,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(windows)]
    volume: Option<u32>,
    #[cfg(windows)]
    file_index: Option<u64>,
}

impl FileIdentity {
    fn from_file(file: &File, metadata: &Metadata) -> Self {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        #[cfg(windows)]
        let windows_identity = windows_file_identity(file);
        #[cfg(not(windows))]
        let _ = file;
        Self {
            size: metadata.len(),
            modified: metadata.modified().ok(),
            #[cfg(unix)]
            device: metadata.dev(),
            #[cfg(unix)]
            inode: metadata.ino(),
            #[cfg(windows)]
            volume: windows_identity.map(|(volume, _)| volume),
            #[cfg(windows)]
            file_index: windows_identity.map(|(_, file_index)| file_index),
        }
    }
}

#[cfg(windows)]
fn windows_file_identity(file: &File) -> Option<(u32, u64)> {
    use std::ffi::c_void;
    use std::os::windows::io::AsRawHandle;

    #[repr(C)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[repr(C)]
    struct ByHandleFileInformation {
        attributes: u32,
        creation: FileTime,
        access: FileTime,
        write: FileTime,
        volume: u32,
        size_high: u32,
        size_low: u32,
        links: u32,
        index_high: u32,
        index_low: u32,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetFileInformationByHandle(
            handle: *mut c_void,
            info: *mut ByHandleFileInformation,
        ) -> i32;
    }
    let mut info = std::mem::MaybeUninit::<ByHandleFileInformation>::uninit();
    let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) };
    if ok == 0 {
        return None;
    }
    let info = unsafe { info.assume_init() };
    Some((
        info.volume,
        (u64::from(info.index_high) << 32) | u64::from(info.index_low),
    ))
}

#[derive(Clone, Debug)]
struct PageCursor {
    identity: FileIdentity,
    encoding: PreviewEncoding,
    read_offset: u64,
    pending_raw: Vec<u8>,
    pending_index: usize,
    line_open: bool,
}

#[derive(Clone, Debug)]
pub struct PagedTextPreview {
    pub header: PreviewHeader,
    body: String,
    lines: Vec<Range<u32>>,
    cursor: PageCursor,
    state: PreviewPageState,
    syntax: Option<SyntaxHighlight>,
}

impl PagedTextPreview {
    pub fn initial(path: &Path, canceled: &dyn Fn() -> bool) -> Result<Self, PreviewPageError> {
        Self::head_with_policy(path, INITIAL_LINE_LIMIT, true, canceled)
    }

    pub fn tui_head(path: &Path, canceled: &dyn Fn() -> bool) -> Result<Self, PreviewPageError> {
        Self::head_with_policy(path, 20, false, canceled)
    }

    fn head_with_policy(
        path: &Path,
        max_lines: usize,
        with_syntax: bool,
        canceled: &dyn Fn() -> bool,
    ) -> Result<Self, PreviewPageError> {
        if canceled() {
            return Err(PreviewPageError::Canceled);
        }
        if should_skip_preview(path, false) {
            return Err(PreviewPageError::OnDemandSkipped);
        }
        let mut file = File::open(path).map_err(classify_io_error)?;
        let metadata = file.metadata().map_err(classify_io_error)?;
        if metadata.len() == 0 {
            return Err(PreviewPageError::Empty);
        }
        let identity = FileIdentity::from_file(&file, &metadata);
        let symlink_metadata = std::fs::symlink_metadata(path).ok();
        let is_symlink = symlink_metadata
            .as_ref()
            .is_some_and(|meta| meta.file_type().is_symlink());
        let attributes = metadata_attributes(symlink_metadata.as_ref().unwrap_or(&metadata));
        let target = is_symlink.then(|| {
            std::fs::read_link(path)
                .map(|target| normalize_path_for_display(&target))
                .unwrap_or_else(|_| "<unavailable>".to_string())
        });
        // Keep the page budget at 64 KiB while probing at most the remainder of
        // one UTF-8 scalar beyond the encoding sample.
        let sample_size = metadata.len().min((INITIAL_BYTE_LIMIT + 3) as u64) as usize;
        let mut sample = vec![0u8; sample_size];
        file.read_exact(&mut sample).map_err(classify_io_error)?;
        if sample.is_empty() {
            return Err(PreviewPageError::Empty);
        }
        let sample_cap = sample.len().min(INITIAL_BYTE_LIMIT);
        let sample_window = &sample[..sample_cap];
        let sample_boundary = sample_window
            .iter()
            .enumerate()
            .filter(|(_, byte)| **byte == b'\n')
            .nth(max_lines.saturating_sub(1))
            .map_or(sample_cap, |(index, _)| index + 1);
        let (mut encoding, bom_len) = detect_encoding(
            &sample_window[..sample_boundary],
            &sample[sample_boundary..],
        )?;
        if encoding == PreviewEncoding::Utf8
            && sample_window[..sample_boundary].iter().all(u8::is_ascii)
            && sample_boundary < sample_cap
        {
            // ASCII is shared by UTF-8 and the legacy codecs. A bounded probe may
            // identify Japanese text on the next page without accepting late NULs
            // or treating an isolated invalid byte as Windows-1252.
            if let Ok((candidate @ (PreviewEncoding::ShiftJis | PreviewEncoding::EucJp), _)) =
                detect_encoding(sample_window, &sample[sample_cap..])
            {
                encoding = candidate;
            }
        }
        file.seek(SeekFrom::Start(bom_len as u64))
            .map_err(classify_io_error)?;
        let mut document = Self {
            header: PreviewHeader {
                path: path.to_path_buf(),
                size: metadata.len(),
                created: metadata.created().ok(),
                modified: metadata.modified().ok(),
                is_symlink,
                target,
                attributes,
            },
            body: String::new(),
            lines: Vec::new(),
            cursor: PageCursor {
                identity,
                encoding,
                read_offset: bom_len as u64,
                pending_raw: Vec::new(),
                pending_index: 0,
                line_open: false,
            },
            state: PreviewPageState::More,
            syntax: with_syntax
                .then(|| SyntaxLanguage::for_path(path))
                .flatten()
                .map(SyntaxHighlight::new),
        };
        document.read_page(&mut file, max_lines, INITIAL_BYTE_LIMIT - bom_len, canceled)?;
        document.verify_identity(path, &file)?;
        document.update_syntax(canceled);
        Ok(document)
    }

    pub fn read_more(
        &self,
        path: &Path,
        canceled: &dyn Fn() -> bool,
    ) -> Result<Self, PreviewPageError> {
        if self.state != PreviewPageState::More {
            return Err(PreviewPageError::LimitReached);
        }
        if path != self.header.path || should_skip_preview(path, false) {
            return Err(PreviewPageError::OnDemandSkipped);
        }
        if canceled() {
            return Err(PreviewPageError::Canceled);
        }
        let mut file = File::open(path).map_err(classify_io_error)?;
        let metadata = file.metadata().map_err(classify_io_error)?;
        let identity = FileIdentity::from_file(&file, &metadata);
        if identity != self.cursor.identity {
            return Err(PreviewPageError::Changed);
        }
        file.seek(SeekFrom::Start(self.cursor.read_offset))
            .map_err(classify_io_error)?;
        let mut next = self.clone();
        next.read_page(&mut file, MORE_LINE_LIMIT, MORE_BYTE_LIMIT, canceled)?;
        next.verify_identity(path, &file)?;
        next.update_syntax(canceled);
        Ok(next)
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn line(&self, index: usize) -> Option<&str> {
        let range = self.lines.get(index)?;
        self.body.get(range.start as usize..range.end as usize)
    }

    pub fn state(&self) -> PreviewPageState {
        self.state
    }

    pub fn encoding(&self) -> PreviewEncoding {
        self.cursor.encoding
    }

    pub fn raw_bytes_read(&self) -> u64 {
        self.cursor.read_offset
    }

    #[cfg(test)]
    pub(crate) fn reserve_body_capacity_for_test(&mut self, minimum: usize) {
        if self.body.capacity() < minimum {
            self.body
                .reserve_exact(minimum.saturating_sub(self.body.len()));
        }
    }

    pub fn capacity_bytes(&self) -> usize {
        self.body
            .capacity()
            .saturating_add(self.lines.capacity() * std::mem::size_of::<Range<u32>>())
            .saturating_add(self.cursor.pending_raw.capacity())
            .saturating_add(
                self.syntax
                    .as_ref()
                    .map_or(0, SyntaxHighlight::capacity_bytes),
            )
            .saturating_add(self.header.path.as_os_str().len())
            .saturating_add(self.header.target.as_ref().map_or(0, String::capacity))
            .saturating_add(self.header.attributes.capacity() * std::mem::size_of::<&'static str>())
            .saturating_add(std::mem::size_of::<Self>())
    }

    pub fn syntax(&self) -> Option<&SyntaxHighlight> {
        self.syntax.as_ref()
    }

    pub fn syntax_spans(&self) -> &[SyntaxSpan] {
        self.syntax.as_ref().map_or(&[], SyntaxHighlight::spans)
    }

    pub fn line_range(&self, index: usize) -> Option<Range<u32>> {
        self.lines.get(index).cloned()
    }

    fn update_syntax(&mut self, canceled: &dyn Fn() -> bool) {
        if let Some(syntax) = &mut self.syntax {
            syntax.append(&self.body, canceled);
            if self.state != PreviewPageState::More {
                syntax.finish(&self.body);
            }
        }
    }

    fn verify_identity(&self, path: &Path, file: &File) -> Result<(), PreviewPageError> {
        let handle = FileIdentity::from_file(file, &file.metadata().map_err(classify_io_error)?);
        let current_file = File::open(path).map_err(classify_io_error)?;
        let current = FileIdentity::from_file(
            &current_file,
            &current_file.metadata().map_err(classify_io_error)?,
        );
        if handle != self.cursor.identity || current != self.cursor.identity {
            return Err(PreviewPageError::Changed);
        }
        Ok(())
    }

    fn read_page(
        &mut self,
        file: &mut File,
        max_new_lines: usize,
        max_new_bytes: usize,
        canceled: &dyn Fn() -> bool,
    ) -> Result<(), PreviewPageError> {
        let mut page_read = 0usize;
        let mut added_lines = 0usize;
        let mut decoded_count = 0usize;
        let mut suspicious_count = 0usize;
        loop {
            if canceled() {
                return Err(PreviewPageError::Canceled);
            }
            if self.lines.len() >= TOTAL_LINE_LIMIT && !self.cursor.line_open {
                self.state = if self.has_unread_data() {
                    PreviewPageState::LimitReached
                } else {
                    PreviewPageState::Eof
                };
                break;
            }
            let scalar = self.next_scalar(file, &mut page_read, max_new_bytes)?;
            let ch = match scalar {
                NextScalar::Char(ch) => ch,
                NextScalar::Eof => {
                    self.state = PreviewPageState::Eof;
                    break;
                }
                NextScalar::PageBudget => {
                    self.state = PreviewPageState::More;
                    break;
                }
                NextScalar::TotalBudget => {
                    self.state = PreviewPageState::LimitReached;
                    break;
                }
            };
            if ch == '\0' {
                return Err(PreviewPageError::Binary);
            }
            decoded_count += 1;
            if ch.is_control() && !matches!(ch, '\n' | '\r' | '\t') {
                suspicious_count += 1;
            }
            if self.body.len().saturating_add(ch.len_utf8()) > DECODED_BYTE_LIMIT {
                self.state = PreviewPageState::LimitReached;
                break;
            }
            let was_open = self.cursor.line_open;
            self.append_char(ch)?;
            if !was_open {
                added_lines += 1;
            }
            if ch == '\n' && added_lines >= max_new_lines {
                self.state = if self.has_unread_data() {
                    PreviewPageState::More
                } else {
                    PreviewPageState::Eof
                };
                break;
            }
        }
        if decoded_count > 0 && suspicious_count.saturating_mul(20) > decoded_count {
            return Err(PreviewPageError::Binary);
        }
        if self.state == PreviewPageState::More && !self.has_unread_data() {
            self.state = PreviewPageState::Eof;
        }
        Ok(())
    }

    fn append_char(&mut self, ch: char) -> Result<(), PreviewPageError> {
        if !self.cursor.line_open {
            let position =
                u32::try_from(self.body.len()).map_err(|_| PreviewPageError::LimitReached)?;
            self.lines.push(position..position);
            self.cursor.line_open = true;
        }
        if ch == '\n' {
            if self.body.ends_with('\r') {
                self.body.pop();
                if let Some(line) = self.lines.last_mut() {
                    line.end -= 1;
                }
            }
            self.body.push('\n');
            self.cursor.line_open = false;
        } else {
            self.body.push(ch);
            if let Some(line) = self.lines.last_mut() {
                line.end =
                    u32::try_from(self.body.len()).map_err(|_| PreviewPageError::LimitReached)?;
            }
        }
        Ok(())
    }

    fn has_unread_data(&self) -> bool {
        self.cursor.pending_index < self.cursor.pending_raw.len()
            || self.cursor.read_offset < self.cursor.identity.size
    }

    fn next_scalar(
        &mut self,
        file: &mut File,
        page_read: &mut usize,
        page_limit: usize,
    ) -> Result<NextScalar, PreviewPageError> {
        let first = match self.ensure_raw(file, 1, page_read, page_limit)? {
            RawAvailability::Available => self.cursor.pending_raw[self.cursor.pending_index],
            RawAvailability::Eof => return Ok(NextScalar::Eof),
            RawAvailability::PageBudget => return Ok(NextScalar::PageBudget),
            RawAvailability::TotalBudget => return Ok(NextScalar::TotalBudget),
        };
        let mut width = scalar_width(self.cursor.encoding, first)?;
        match self.ensure_raw(file, width, page_read, page_limit)? {
            RawAvailability::Available => {}
            RawAvailability::Eof => return Err(PreviewPageError::DecodeFailed),
            RawAvailability::PageBudget => return Ok(NextScalar::PageBudget),
            RawAvailability::TotalBudget => return Ok(NextScalar::TotalBudget),
        }
        if matches!(
            self.cursor.encoding,
            PreviewEncoding::Utf16Le | PreviewEncoding::Utf16Be
        ) {
            let start = self.cursor.pending_index;
            let pair = [
                self.cursor.pending_raw[start],
                self.cursor.pending_raw[start + 1],
            ];
            let unit = if self.cursor.encoding == PreviewEncoding::Utf16Le {
                u16::from_le_bytes(pair)
            } else {
                u16::from_be_bytes(pair)
            };
            if (0xD800..=0xDBFF).contains(&unit) {
                width = 4;
                match self.ensure_raw(file, width, page_read, page_limit)? {
                    RawAvailability::Available => {}
                    RawAvailability::Eof => return Err(PreviewPageError::DecodeFailed),
                    RawAvailability::PageBudget => return Ok(NextScalar::PageBudget),
                    RawAvailability::TotalBudget => return Ok(NextScalar::TotalBudget),
                }
            }
        }
        let start = self.cursor.pending_index;
        let bytes = &self.cursor.pending_raw[start..start + width];
        let ch = decode_scalar(self.cursor.encoding, bytes)?;
        self.cursor.pending_index += width;
        Ok(NextScalar::Char(ch))
    }

    fn ensure_raw(
        &mut self,
        file: &mut File,
        needed: usize,
        page_read: &mut usize,
        page_limit: usize,
    ) -> Result<RawAvailability, PreviewPageError> {
        while self.cursor.pending_raw.len() - self.cursor.pending_index < needed {
            if self.cursor.read_offset >= self.cursor.identity.size {
                return Ok(RawAvailability::Eof);
            }
            if self.cursor.read_offset >= TOTAL_BYTE_LIMIT as u64 {
                return Ok(RawAvailability::TotalBudget);
            }
            if *page_read >= page_limit {
                return Ok(RawAvailability::PageBudget);
            }
            if self.cursor.pending_index > 0 {
                self.cursor.pending_raw.drain(..self.cursor.pending_index);
                self.cursor.pending_index = 0;
            }
            let available_total = TOTAL_BYTE_LIMIT - self.cursor.read_offset as usize;
            let amount = READ_BLOCK
                .min(page_limit - *page_read)
                .min(available_total)
                .min((self.cursor.identity.size - self.cursor.read_offset) as usize);
            let old_len = self.cursor.pending_raw.len();
            self.cursor.pending_raw.resize(old_len + amount, 0);
            let read = file
                .read(&mut self.cursor.pending_raw[old_len..])
                .map_err(classify_io_error)?;
            self.cursor.pending_raw.truncate(old_len + read);
            if read == 0 {
                return Ok(RawAvailability::Eof);
            }
            self.cursor.read_offset += read as u64;
            *page_read += read;
        }
        Ok(RawAvailability::Available)
    }
}

enum NextScalar {
    Char(char),
    Eof,
    PageBudget,
    TotalBudget,
}

enum RawAvailability {
    Available,
    Eof,
    PageBudget,
    TotalBudget,
}

fn classify_io_error(error: std::io::Error) -> PreviewPageError {
    match error.kind() {
        std::io::ErrorKind::NotFound => PreviewPageError::NotFound,
        std::io::ErrorKind::PermissionDenied => PreviewPageError::PermissionDenied,
        _ => PreviewPageError::ReadFailed,
    }
}

fn detect_encoding(
    bytes: &[u8],
    boundary_lookahead: &[u8],
) -> Result<(PreviewEncoding, usize), PreviewPageError> {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return Ok((PreviewEncoding::Utf8, 3));
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return Ok((PreviewEncoding::Utf16Le, 2));
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return Ok((PreviewEncoding::Utf16Be, 2));
    }
    if bytes.contains(&0) {
        return Err(PreviewPageError::Binary);
    }
    match std::str::from_utf8(bytes) {
        Ok(_) => return Ok((PreviewEncoding::Utf8, 0)),
        Err(error) if valid_utf8_boundary(bytes, boundary_lookahead, &error) => {
            return Ok((PreviewEncoding::Utf8, 0));
        }
        Err(_) => {}
    }
    for (encoding, codec) in [
        (PreviewEncoding::ShiftJis, SHIFT_JIS),
        (PreviewEncoding::EucJp, EUC_JP),
        (PreviewEncoding::Windows1252, WINDOWS_1252),
    ] {
        for trim in 0..=2usize.min(bytes.len().saturating_sub(1)) {
            if codec
                .decode_without_bom_handling_and_without_replacement(&bytes[..bytes.len() - trim])
                .is_some()
            {
                return Ok((encoding, 0));
            }
        }
    }
    Err(PreviewPageError::DecodeFailed)
}

fn valid_utf8_boundary(bytes: &[u8], lookahead: &[u8], error: &std::str::Utf8Error) -> bool {
    if error.error_len().is_some() {
        return false;
    }
    let Some(partial) = bytes.get(error.valid_up_to()..) else {
        return false;
    };
    let width: usize = match partial.first() {
        Some(0xC2..=0xDF) => 2,
        Some(0xE0..=0xEF) => 3,
        Some(0xF0..=0xF4) => 4,
        _ => return false,
    };
    let Some(missing) = width.checked_sub(partial.len()) else {
        return false;
    };
    if missing == 0 {
        return false;
    }
    let Some(completion) = lookahead.get(..missing) else {
        return false;
    };
    let mut scalar = [0u8; 4];
    scalar[..partial.len()].copy_from_slice(partial);
    scalar[partial.len()..width].copy_from_slice(completion);
    std::str::from_utf8(&scalar[..width]).is_ok()
}

fn scalar_width(encoding: PreviewEncoding, first: u8) -> Result<usize, PreviewPageError> {
    let width = match encoding {
        PreviewEncoding::Utf8 => match first {
            0x00..=0x7F => 1,
            0xC2..=0xDF => 2,
            0xE0..=0xEF => 3,
            0xF0..=0xF4 => 4,
            _ => return Err(PreviewPageError::DecodeFailed),
        },
        PreviewEncoding::Utf16Le | PreviewEncoding::Utf16Be => 2,
        PreviewEncoding::ShiftJis => match first {
            0x81..=0x9F | 0xE0..=0xFC => 2,
            _ => 1,
        },
        PreviewEncoding::EucJp => match first {
            0x8F => 3,
            0x8E | 0xA1..=0xFE => 2,
            _ => 1,
        },
        PreviewEncoding::Windows1252 => 1,
    };
    Ok(width)
}

fn decode_scalar(encoding: PreviewEncoding, bytes: &[u8]) -> Result<char, PreviewPageError> {
    match encoding {
        PreviewEncoding::Utf8 => std::str::from_utf8(bytes)
            .ok()
            .and_then(|text| text.chars().next())
            .ok_or(PreviewPageError::DecodeFailed),
        PreviewEncoding::Utf16Le | PreviewEncoding::Utf16Be => {
            let code = if encoding == PreviewEncoding::Utf16Le {
                u16::from_le_bytes([bytes[0], bytes[1]])
            } else {
                u16::from_be_bytes([bytes[0], bytes[1]])
            };
            if bytes.len() == 4 {
                let low = if encoding == PreviewEncoding::Utf16Le {
                    u16::from_le_bytes([bytes[2], bytes[3]])
                } else {
                    u16::from_be_bytes([bytes[2], bytes[3]])
                };
                if !(0xD800..=0xDBFF).contains(&code) || !(0xDC00..=0xDFFF).contains(&low) {
                    return Err(PreviewPageError::DecodeFailed);
                }
                let scalar = 0x10000 + (((code as u32 - 0xD800) << 10) | (low as u32 - 0xDC00));
                char::from_u32(scalar).ok_or(PreviewPageError::DecodeFailed)
            } else {
                char::from_u32(code as u32).ok_or(PreviewPageError::DecodeFailed)
            }
        }
        PreviewEncoding::ShiftJis | PreviewEncoding::EucJp | PreviewEncoding::Windows1252 => {
            let codec = match encoding {
                PreviewEncoding::ShiftJis => SHIFT_JIS,
                PreviewEncoding::EucJp => EUC_JP,
                _ => WINDOWS_1252,
            };
            codec
                .decode_without_bom_handling_and_without_replacement(bytes)
                .and_then(|text| text.chars().next())
                .ok_or(PreviewPageError::DecodeFailed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture(name: &str, bytes: &[u8]) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("flistwalker-paged-{name}-{nonce}.txt"));
        fs::write(&path, bytes).expect("write preview fixture");
        path
    }

    #[test]
    fn initial_and_more_pages_preserve_each_line_exactly_once() {
        let input = (1..=601)
            .map(|index| format!("line {index}\n"))
            .collect::<String>();
        let path = fixture("lines", input.as_bytes());
        let first = PagedTextPreview::initial(&path, &|| false).expect("initial page");
        assert_eq!(first.line_count(), 100);
        assert_eq!(first.line(0), Some("line 1"));
        assert_eq!(first.line(99), Some("line 100"));
        assert_eq!(first.state(), PreviewPageState::More);
        let second = first.read_more(&path, &|| false).expect("second page");
        assert_eq!(second.line_count(), 600);
        assert_eq!(second.line(100), Some("line 101"));
        assert_eq!(second.line(599), Some("line 600"));
        let third = second.read_more(&path, &|| false).expect("third page");
        assert_eq!(third.line_count(), 601);
        assert_eq!(third.line(600), Some("line 601"));
        assert_eq!(third.state(), PreviewPageState::Eof);
        assert_eq!(third.body(), input);
        fs::remove_file(path).expect("cleanup fixture");
    }

    #[test]
    fn byte_boundary_keeps_utf8_scalar_and_partial_line_for_next_page() {
        let input = format!("{}é-suffix\nnext\n", "a".repeat(65_535));
        let path = fixture("utf8-boundary", input.as_bytes());
        let first = PagedTextPreview::initial(&path, &|| false).expect("initial page");
        assert_eq!(first.line_count(), 1);
        assert_eq!(first.state(), PreviewPageState::More);
        let next = first.read_more(&path, &|| false).expect("next page");
        assert_eq!(next.body(), input);
        assert_eq!(next.line_count(), 2);
        fs::remove_file(path).expect("cleanup fixture");
    }

    #[test]
    fn invalid_utf8_continuation_after_sample_uses_legacy_encoding_for_gui_and_tui() {
        let mut input = vec![b'a'; INITIAL_BYTE_LIMIT - 1];
        input.extend_from_slice(&[0xC2, b'A', b'\n']);
        let path = fixture("invalid-utf8-sample-boundary", &input);

        let gui = PagedTextPreview::initial(&path, &|| false).expect("GUI legacy preview");
        let tui = PagedTextPreview::tui_head(&path, &|| false).expect("TUI legacy preview");
        assert_eq!(gui.encoding(), PreviewEncoding::ShiftJis);
        assert_eq!(tui.encoding(), PreviewEncoding::ShiftJis);
        assert!(
            gui.body().ends_with('ﾂ'),
            "boundary byte must not be discarded"
        );
        assert!(
            tui.body().ends_with('ﾂ'),
            "boundary byte must not be discarded"
        );
        let complete = gui
            .read_more(&path, &|| false)
            .expect("legacy continuation");
        assert!(complete.body().ends_with("ﾂA\n"));

        fs::remove_file(path).expect("cleanup fixture");
    }

    #[test]
    fn valid_utf8_scalar_across_sample_boundary_keeps_utf8_for_gui_and_tui() {
        for scalar in ["¢", "日", "😀"] {
            let input = format!("{}{}-suffix\n", "a".repeat(INITIAL_BYTE_LIMIT - 1), scalar);
            let path = fixture("valid-utf8-sample-boundary", input.as_bytes());
            let gui = PagedTextPreview::initial(&path, &|| false).expect("GUI UTF-8 preview");
            let tui = PagedTextPreview::tui_head(&path, &|| false).expect("TUI UTF-8 preview");
            assert_eq!(gui.encoding(), PreviewEncoding::Utf8);
            assert_eq!(tui.encoding(), PreviewEncoding::Utf8);
            assert_eq!(
                gui.read_more(&path, &|| false).expect("GUI more").body(),
                input
            );
            assert_eq!(
                tui.read_more(&path, &|| false).expect("TUI more").body(),
                input
            );
            fs::remove_file(path).expect("cleanup fixture");
        }
    }

    #[test]
    fn late_binary_page_never_changes_committed_text() {
        let input = format!("{}\0bad\n", "safe\n".repeat(100));
        let path = fixture("late-binary", input.as_bytes());
        let first = PagedTextPreview::initial(&path, &|| false).expect("initial page");
        let before = first.body().to_owned();
        let error = first.read_more(&path, &|| false).expect_err("late NUL");
        assert_eq!(error, PreviewPageError::Binary);
        assert_eq!(first.body(), before);
        fs::remove_file(path).expect("cleanup fixture");
    }

    #[test]
    fn utf16_bom_accepts_ascii_nul_bytes_and_surrogate_pair() {
        let text = format!("{}😀\nnext\n", "x\n".repeat(99));
        let mut bytes = vec![0xFF, 0xFE];
        for unit in text.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        let path = fixture("utf16", &bytes);
        let first = PagedTextPreview::initial(&path, &|| false).expect("UTF-16 initial");
        assert_eq!(first.encoding(), PreviewEncoding::Utf16Le);
        assert_eq!(first.line_count(), 100);
        assert_eq!(first.line(99), Some("😀"));
        let next = first.read_more(&path, &|| false).expect("UTF-16 more");
        assert_eq!(next.body(), text);
        fs::remove_file(path).expect("cleanup fixture");
    }

    #[test]
    fn legacy_encodings_and_crlf_decode_without_replacement() {
        for (name, codec, expected_encoding) in [
            (
                "shift-jis",
                encoding_rs::SHIFT_JIS,
                PreviewEncoding::ShiftJis,
            ),
            ("euc-jp", encoding_rs::EUC_JP, PreviewEncoding::EucJp),
        ] {
            let (encoded, _, had_errors) = codec.encode("日本語\r\n二行目\r\n");
            assert!(!had_errors);
            let path = fixture(name, &encoded);
            let preview = PagedTextPreview::initial(&path, &|| false).expect("legacy preview");
            assert_eq!(preview.encoding(), expected_encoding);
            assert_eq!(preview.body(), "日本語\n二行目\n");
            fs::remove_file(path).expect("cleanup fixture");
        }
    }

    #[test]
    fn legacy_encoding_after_ascii_prefix_is_detected_for_gui_and_tui() {
        for (name, codec, expected_encoding) in [
            (
                "shift-jis-late",
                encoding_rs::SHIFT_JIS,
                PreviewEncoding::ShiftJis,
            ),
            ("euc-jp-late", encoding_rs::EUC_JP, PreviewEncoding::EucJp),
        ] {
            let text = format!("{}日本語\n", "ascii-only-longline\n".repeat(15));
            let (encoded, _, had_errors) = codec.encode(&text);
            assert!(!had_errors);
            let path = fixture(name, &encoded);
            let gui = PagedTextPreview::initial(&path, &|| false).expect("GUI legacy preview");
            let tui = PagedTextPreview::tui_head(&path, &|| false).expect("TUI legacy preview");
            assert_eq!(gui.encoding(), expected_encoding);
            assert_eq!(tui.encoding(), expected_encoding);
            assert!(gui.body().contains("日本語"));
            assert!(tui.body().contains("日本語"));
            fs::remove_file(path).expect("cleanup fixture");
        }
    }

    #[test]
    fn legacy_encoding_after_first_page_ascii_is_selected_from_bounded_sample() {
        for (name, codec, expected_encoding) in [
            (
                "shift-jis-next-page",
                encoding_rs::SHIFT_JIS,
                PreviewEncoding::ShiftJis,
            ),
            (
                "euc-jp-next-page",
                encoding_rs::EUC_JP,
                PreviewEncoding::EucJp,
            ),
        ] {
            let text = format!("{}日本語\n", "ascii\n".repeat(100));
            let (encoded, _, had_errors) = codec.encode(&text);
            assert!(!had_errors);
            let path = fixture(name, &encoded);
            let gui = PagedTextPreview::initial(&path, &|| false).expect("GUI first page");
            assert_eq!(gui.encoding(), expected_encoding);
            assert_eq!(gui.line_count(), 100);
            assert_eq!(
                gui.read_more(&path, &|| false)
                    .expect("legacy continuation")
                    .body(),
                text
            );
            let tui = PagedTextPreview::tui_head(&path, &|| false).expect("TUI head");
            assert_eq!(tui.encoding(), expected_encoding);
            fs::remove_file(path).expect("cleanup fixture");
        }
    }

    #[test]
    fn late_invalid_utf8_is_a_failed_page_and_source_change_is_detected() {
        let mut input = "ok\n".repeat(100).into_bytes();
        input.extend_from_slice(&[0xFF, b'\n']);
        let path = fixture("late-invalid", &input);
        let first = PagedTextPreview::initial(&path, &|| false).expect("initial preview");
        assert!(matches!(
            first.read_more(&path, &|| false),
            Err(PreviewPageError::DecodeFailed)
        ));
        assert_eq!(first.body(), "ok\n".repeat(100));
        fs::write(&path, "changed length").expect("replace source");
        assert!(matches!(
            first.read_more(&path, &|| false),
            Err(PreviewPageError::Changed)
        ));
        fs::remove_file(path).expect("cleanup fixture");
    }

    #[test]
    fn no_newline_file_is_bounded_by_bytes_and_reports_limit() {
        let path = fixture("no-newline", &vec![b'x'; 2 * 1024 * 1024]);
        let mut preview = PagedTextPreview::initial(&path, &|| false).expect("initial page");
        assert_eq!(preview.raw_bytes_read(), 64 * 1024);
        while preview.state() == PreviewPageState::More {
            preview = preview
                .read_more(&path, &|| false)
                .expect("bounded more page");
        }
        assert_eq!(preview.state(), PreviewPageState::LimitReached);
        assert_eq!(preview.raw_bytes_read(), 1024 * 1024);
        assert_eq!(preview.line_count(), 1);
        assert!(preview.capacity_bytes() <= 8 * 1024 * 1024);
        fs::remove_file(path).expect("cleanup fixture");
    }

    #[test]
    fn exact_line_boundaries_and_total_line_limit_are_stable() {
        for count in [99, 100, 101, 599, 600, 601, 5_000, 5_001] {
            let input = (0..count).map(|n| format!("row {n}\n")).collect::<String>();
            let path = fixture("line-boundaries", input.as_bytes());
            let mut preview = PagedTextPreview::initial(&path, &|| false).expect("first page");
            assert_eq!(preview.line_count(), count.min(100));
            while preview.state() == PreviewPageState::More {
                preview = preview.read_more(&path, &|| false).expect("more page");
            }
            assert_eq!(preview.line_count(), count.min(TOTAL_LINE_LIMIT));
            if count <= TOTAL_LINE_LIMIT {
                assert_eq!(preview.body(), input);
                assert_eq!(preview.state(), PreviewPageState::Eof);
            } else {
                assert_eq!(preview.state(), PreviewPageState::LimitReached);
            }
            fs::remove_file(path).expect("cleanup fixture");
        }
    }

    #[test]
    fn change_and_cancellation_leave_committed_page_untouched() {
        let path = fixture("cancel-more", "first\n".repeat(101).as_bytes());
        let first = PagedTextPreview::initial(&path, &|| false).expect("first page");
        let old_body = first.body().to_string();
        assert_eq!(
            first.read_more(&path, &|| true).expect_err("canceled"),
            PreviewPageError::Canceled
        );
        assert_eq!(first.body(), old_body);
        fs::write(&path, "replaced\n").expect("replace file");
        assert_eq!(
            first.read_more(&path, &|| false).expect_err("changed"),
            PreviewPageError::Changed
        );
        assert_eq!(first.body(), old_body);
        fs::remove_file(path).expect("cleanup fixture");
    }
}

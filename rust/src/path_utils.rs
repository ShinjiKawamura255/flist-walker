use std::path::{Path, PathBuf};

pub fn path_key(path: &Path) -> String {
    #[cfg(windows)]
    {
        path.to_string_lossy().to_string().to_ascii_lowercase()
    }
    #[cfg(not(windows))]
    {
        path.to_string_lossy().to_string()
    }
}

pub fn strip_windows_extended_prefix(text: &str) -> String {
    normalize_text_for_display(text)
}

/// Normalizes arbitrary user-visible text, including diagnostics that can contain
/// more than one embedded Windows path.
pub fn normalize_text_for_display(text: &str) -> String {
    #[cfg(windows)]
    {
        let mut normalized = String::with_capacity(text.len());
        let mut remaining = text;
        while let Some(index) = remaining.find(r"\\?\") {
            normalized.push_str(&remaining[..index]);
            let after_prefix = &remaining[index + r"\\?\".len()..];
            if after_prefix
                .get(..r"UNC\".len())
                .is_some_and(|namespace| namespace.eq_ignore_ascii_case(r"UNC\"))
            {
                normalized.push_str(r"\\");
                remaining = &after_prefix[r"UNC\".len()..];
            } else {
                remaining = after_prefix;
            }
        }
        normalized.push_str(remaining);
        normalized
    }
    #[cfg(not(windows))]
    text.to_string()
}

pub fn normalize_windows_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let raw = path.to_string_lossy();
        PathBuf::from(strip_windows_extended_prefix(&raw))
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

pub fn normalize_windows_path_buf(path: PathBuf) -> PathBuf {
    normalize_windows_path(&path)
}

#[cfg(windows)]
pub(crate) fn windows_non_verbatim_path(path: &Path) -> Option<PathBuf> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    const VERBATIM_PREFIX: &[u16] = &[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16];
    const VERBATIM_UNC_PREFIX: &[u16] = &[
        b'\\' as u16,
        b'\\' as u16,
        b'?' as u16,
        b'\\' as u16,
        b'U' as u16,
        b'N' as u16,
        b'C' as u16,
        b'\\' as u16,
    ];

    let wide = path.as_os_str().encode_wide().collect::<Vec<_>>();
    let normalized = if let Some(rest) = wide.strip_prefix(VERBATIM_UNC_PREFIX) {
        [b'\\' as u16, b'\\' as u16]
            .into_iter()
            .chain(rest.iter().copied())
            .collect::<Vec<_>>()
    } else {
        wide.strip_prefix(VERBATIM_PREFIX)?.to_vec()
    };
    Some(PathBuf::from(std::ffi::OsString::from_wide(&normalized)))
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
    strip_windows_extended_prefix(&raw)
}

pub fn output_path_bytes(
    path: &Path,
    root: &Path,
    prefer_relative: bool,
    _nul_delimited: bool,
) -> Vec<u8> {
    #[cfg(unix)]
    if _nul_delimited {
        use std::os::unix::ffi::OsStrExt;

        let output_path = if prefer_relative {
            path.strip_prefix(root).unwrap_or(path)
        } else {
            path
        };
        return output_path.as_os_str().as_bytes().to_vec();
    }

    display_path_with_mode(path, root, prefer_relative).into_bytes()
}

pub fn normalize_path_for_display(path: &Path) -> String {
    let normalized = normalize_windows_path(path);
    strip_windows_extended_prefix(&normalized.to_string_lossy())
}

pub fn normalize_windows_shell_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        normalize_windows_path(path)
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_text_for_display;

    #[test]
    #[cfg(target_os = "windows")]
    fn tc202_regression_display_text_strips_all_embedded_verbatim_path_prefixes() {
        assert_eq!(
            normalize_text_for_display(
                r"source=\\?\C:\tools\flistwalker.exe backup=\\?\UnC\server\share\old.exe"
            ),
            r"source=C:\tools\flistwalker.exe backup=\\server\share\old.exe"
        );
    }

    #[test]
    fn tc202_regression_display_text_preserves_non_verbatim_content() {
        let message = r"Update failed: access denied (error 5)";
        assert_eq!(normalize_text_for_display(message), message);
    }
}

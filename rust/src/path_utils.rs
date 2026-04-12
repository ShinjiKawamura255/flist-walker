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

pub fn normalize_windows_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let raw = path.to_string_lossy();
        return PathBuf::from(strip_windows_extended_prefix(&raw));
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

pub fn normalize_windows_path_buf(path: PathBuf) -> PathBuf {
    normalize_windows_path(&path)
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

pub fn normalize_path_for_display(path: &Path) -> String {
    let normalized = normalize_windows_path(path);
    strip_windows_extended_prefix(&normalized.to_string_lossy())
}

pub fn normalize_windows_shell_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        return normalize_windows_path(path);
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

use std::path::{Path, PathBuf};

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

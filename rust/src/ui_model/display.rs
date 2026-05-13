use std::path::Path;

use crate::path_utils::{
    display_path_with_mode as display_display_path_with_mode,
    normalize_path_for_display as normalize_display_path,
};

pub fn display_path(path: &Path, root: &Path) -> String {
    display_path_with_mode(path, root, true)
}

pub fn normalize_path_for_display(path: &Path) -> String {
    normalize_display_path(path)
}

pub fn display_path_with_mode(path: &Path, root: &Path, prefer_relative: bool) -> String {
    display_display_path_with_mode(path, root, prefer_relative)
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

use std::path::{Path, PathBuf};

pub const WINDOWS_GUI_BIN_NAME: &str = "flistwalker";

fn gnu_windows_resource_object(out_dir: &Path) -> PathBuf {
    out_dir.join("resource.o")
}

pub fn cargo_directives_for_windows_resource_bin(
    target_env: &str,
    bin_name: &str,
    out_dir: &Path,
) -> Vec<String> {
    if target_env != "gnu" {
        return Vec::new();
    }

    vec![format!(
        "cargo:rustc-link-arg-bin={bin_name}={}",
        gnu_windows_resource_object(out_dir).display()
    )]
}

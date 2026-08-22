use std::path::{Path, PathBuf};

pub const WINDOWS_GUI_BIN_NAME: &str = "flistwalker";
pub const WINDOWS_CLI_BIN_NAME: &str = "fw";

pub fn should_use_gnu_resource_tools(target_env: &str) -> bool {
    target_env == "gnu"
}

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

pub fn cargo_directives_for_all_windows_bins(target_env: &str, out_dir: &Path) -> Vec<String> {
    [WINDOWS_GUI_BIN_NAME, WINDOWS_CLI_BIN_NAME]
        .into_iter()
        .flat_map(|bin_name| {
            cargo_directives_for_windows_resource_bin(target_env, bin_name, out_dir)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tc_148_windows_host_gnu_uses_explicit_resource_linking() {
        assert!(should_use_gnu_resource_tools("gnu"));
        assert!(!should_use_gnu_resource_tools("msvc"));
    }
}

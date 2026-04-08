use crate::ui_model::normalize_path_for_display;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub(crate) fn action_notice_for_targets(targets: &[PathBuf]) -> String {
    if targets.len() == 1 {
        format!("Action: {}", normalize_path_for_display(&targets[0]))
    } else {
        format!("Action: launched {} items", targets.len())
    }
}

pub(crate) fn action_targets_for_request(paths: &[PathBuf], open_parent_for_files: bool) -> Vec<PathBuf> {
    if !open_parent_for_files {
        return paths.to_vec();
    }

    let mut unique = HashSet::with_capacity(paths.len());
    let mut targets = Vec::with_capacity(paths.len());
    for path in paths {
        let target = action_target_path_for_open_in_folder(path);
        if unique.insert(target.clone()) {
            targets.push(target);
        }
    }
    targets
}

pub(crate) fn action_target_path_for_open_in_folder(path: &Path) -> PathBuf {
    if path.is_dir() {
        return path.to_path_buf();
    }
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| path.to_path_buf())
}

use crate::ui_model::normalize_path_for_display;
use std::path::PathBuf;

pub(crate) fn action_notice_for_targets(targets: &[PathBuf]) -> String {
    if targets.len() == 1 {
        format!("Action: {}", normalize_path_for_display(&targets[0]))
    } else {
        format!("Action: launched {} items", targets.len())
    }
}

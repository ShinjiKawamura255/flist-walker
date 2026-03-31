use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum TabAccentColor {
    Teal,
    Indigo,
    Azure,
    Amber,
    Olive,
    Emerald,
    Crimson,
    Magenta,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(super) struct SavedWindowGeometry {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) monitor_width: Option<f32>,
    pub(super) monitor_height: Option<f32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(super) struct UiState {
    pub(super) last_root: Option<String>,
    pub(super) default_root: Option<String>,
    pub(super) show_preview: Option<bool>,
    pub(super) preview_panel_width: Option<f32>,
    #[serde(default)]
    pub(super) query_history: Vec<String>,
    #[serde(default)]
    pub(super) results_panel_width: Option<f32>,
    #[serde(default)]
    pub(super) tabs: Vec<SavedTabState>,
    pub(super) active_tab: Option<usize>,
    pub(super) window: Option<SavedWindowGeometry>,
    #[serde(default)]
    pub(super) skipped_update_target_version: Option<String>,
    #[serde(default)]
    pub(super) suppress_update_check_failure_dialog: bool,
}

#[derive(Clone, Debug, Default)]
pub(super) struct LaunchSettings {
    pub(super) last_root: Option<PathBuf>,
    pub(super) default_root: Option<PathBuf>,
    pub(super) show_preview: bool,
    pub(super) preview_panel_width: f32,
    pub(super) query_history: Vec<String>,
    pub(super) restore_tabs: Vec<SavedTabState>,
    pub(super) restore_active_tab: Option<usize>,
    pub(super) skipped_update_target_version: Option<String>,
    pub(super) suppress_update_check_failure_dialog: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct SavedTabState {
    pub(super) root: String,
    pub(super) use_filelist: bool,
    pub(super) use_regex: bool,
    #[serde(default = "default_ignore_case")]
    pub(super) ignore_case: bool,
    pub(super) include_files: bool,
    pub(super) include_dirs: bool,
    pub(super) query: String,
    #[serde(default)]
    pub(super) query_history: Vec<String>,
    #[serde(default)]
    pub(super) tab_accent: Option<TabAccentColor>,
}

fn default_ignore_case() -> bool {
    true
}

impl FlistWalkerApp {
    pub(super) fn ui_state_file_path() -> Option<PathBuf> {
        #[cfg(windows)]
        {
            if let Some(base) = std::env::var_os("USERPROFILE") {
                return Some(PathBuf::from(base).join(".flistwalker_ui_state.json"));
            }
        }
        #[cfg(not(windows))]
        {
            if let Some(base) = std::env::var_os("HOME") {
                return Some(PathBuf::from(base).join(".flistwalker_ui_state.json"));
            }
        }
        None
    }

    pub(super) fn load_ui_state() -> UiState {
        let Some(path) = Self::ui_state_file_path() else {
            return UiState::default();
        };
        Self::read_ui_state_from_path(&path)
    }

    fn read_ui_state_from_path(path: &Path) -> UiState {
        let Ok(text) = fs::read_to_string(path) else {
            return UiState::default();
        };
        serde_json::from_str::<UiState>(&text).unwrap_or_default()
    }

    #[cfg(test)]
    pub(super) fn load_ui_state_from_path(path: &Path) -> UiState {
        Self::read_ui_state_from_path(path)
    }

    pub(super) fn load_launch_settings() -> LaunchSettings {
        Self::launch_settings_from_ui_state(Self::load_ui_state())
    }

    #[cfg(test)]
    pub(super) fn load_launch_settings_from_path(path: &Path) -> LaunchSettings {
        Self::launch_settings_from_ui_state(Self::read_ui_state_from_path(path))
    }

    #[cfg(test)]
    pub(super) fn load_launch_settings_from_path_with_history_persist_disabled(
        path: &Path,
        disabled: bool,
    ) -> LaunchSettings {
        Self::launch_settings_from_ui_state_inner(Self::read_ui_state_from_path(path), disabled)
    }

    fn launch_settings_from_ui_state(ui_state: UiState) -> LaunchSettings {
        Self::launch_settings_from_ui_state_inner(ui_state, Self::history_persist_disabled())
    }

    fn launch_settings_from_ui_state_inner(
        ui_state: UiState,
        history_persist_disabled: bool,
    ) -> LaunchSettings {
        let last_root = ui_state
            .last_root
            .as_deref()
            .map(PathBuf::from)
            .map(Self::normalize_windows_path);
        let default_root = ui_state
            .default_root
            .as_deref()
            .map(PathBuf::from)
            .map(Self::normalize_windows_path);
        let show_preview = ui_state.show_preview.unwrap_or(true);
        let preview_panel_width = ui_state
            .preview_panel_width
            .or(ui_state.results_panel_width)
            .unwrap_or(Self::DEFAULT_PREVIEW_PANEL_WIDTH)
            .max(Self::MIN_PREVIEW_PANEL_WIDTH);
        LaunchSettings {
            last_root,
            default_root,
            show_preview,
            preview_panel_width,
            query_history: if history_persist_disabled {
                Vec::new()
            } else {
                ui_state
                    .query_history
                    .into_iter()
                    .rev()
                    .take(Self::QUERY_HISTORY_MAX)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect()
            },
            restore_tabs: ui_state.tabs,
            restore_active_tab: ui_state.active_tab,
            skipped_update_target_version: ui_state.skipped_update_target_version,
            suppress_update_check_failure_dialog: ui_state.suppress_update_check_failure_dialog,
        }
    }

    pub(super) fn restore_tabs_enabled() -> bool {
        std::env::var("FLISTWALKER_RESTORE_TABS")
            .ok()
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    }

    pub(super) fn sanitize_saved_tabs(
        tabs: &[SavedTabState],
        active_tab: Option<usize>,
    ) -> Option<(Vec<SavedTabState>, usize)> {
        let history_persist_disabled = Self::history_persist_disabled();
        let sanitized: Vec<SavedTabState> = tabs
            .iter()
            .filter_map(|tab| {
                let root = Self::normalize_windows_path(PathBuf::from(&tab.root));
                if !root.is_dir() {
                    return None;
                }
                Some(SavedTabState {
                    root: root.to_string_lossy().to_string(),
                    use_filelist: tab.use_filelist,
                    use_regex: tab.use_regex,
                    ignore_case: tab.ignore_case,
                    include_files: tab.include_files,
                    include_dirs: tab.include_dirs,
                    query: tab.query.clone(),
                    query_history: if history_persist_disabled {
                        Vec::new()
                    } else {
                        tab.query_history
                            .iter()
                            .rev()
                            .take(Self::QUERY_HISTORY_MAX)
                            .cloned()
                            .collect::<Vec<_>>()
                            .into_iter()
                            .rev()
                            .collect()
                    },
                    tab_accent: tab.tab_accent,
                })
            })
            .collect();
        if sanitized.is_empty() {
            return None;
        }
        let active = active_tab
            .unwrap_or(0)
            .min(sanitized.len().saturating_sub(1));
        Some((sanitized, active))
    }
}

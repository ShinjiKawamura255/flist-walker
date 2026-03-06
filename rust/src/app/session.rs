use super::*;
use serde::{Deserialize, Serialize};

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
    pub(super) default_root: Option<String>,
    pub(super) show_preview: Option<bool>,
    pub(super) preview_panel_width: Option<f32>,
    #[serde(default)]
    pub(super) results_panel_width: Option<f32>,
    #[serde(default)]
    pub(super) tabs: Vec<SavedTabState>,
    pub(super) active_tab: Option<usize>,
    pub(super) window: Option<SavedWindowGeometry>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct LaunchSettings {
    pub(super) default_root: Option<PathBuf>,
    pub(super) show_preview: bool,
    pub(super) preview_panel_width: f32,
    pub(super) restore_tabs: Vec<SavedTabState>,
    pub(super) restore_active_tab: Option<usize>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct SavedTabState {
    pub(super) root: String,
    pub(super) use_filelist: bool,
    pub(super) use_regex: bool,
    pub(super) include_files: bool,
    pub(super) include_dirs: bool,
    pub(super) query: String,
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
        let Ok(text) = fs::read_to_string(path) else {
            return UiState::default();
        };
        serde_json::from_str::<UiState>(&text).unwrap_or_default()
    }

    pub(super) fn load_launch_settings() -> LaunchSettings {
        let ui_state = Self::load_ui_state();
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
            default_root,
            show_preview,
            preview_panel_width,
            restore_tabs: ui_state.tabs,
            restore_active_tab: ui_state.active_tab,
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
                    include_files: tab.include_files,
                    include_dirs: tab.include_dirs,
                    query: tab.query.clone(),
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

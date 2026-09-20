//! Complete persisted document schema; defaults and validation preserve the file format.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum TabAccentColor {
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
pub(crate) struct SavedWindowGeometry {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) monitor_width: Option<f32>,
    pub(crate) monitor_height: Option<f32>,
    #[serde(default)]
    pub(crate) pixels_per_point: Option<f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct UiState {
    pub(crate) last_root: Option<String>,
    pub(crate) default_root: Option<String>,
    pub(crate) show_preview: Option<bool>,
    #[serde(default = "default_ignore_list_enabled")]
    pub(crate) ignore_list_enabled: bool,
    pub(crate) preview_panel_width: Option<f32>,
    #[serde(default)]
    pub(crate) query_history: Vec<String>,
    #[serde(default)]
    pub(crate) results_panel_width: Option<f32>,
    #[serde(default)]
    pub(crate) tabs: Vec<SavedTabState>,
    pub(crate) active_tab: Option<usize>,
    pub(crate) window: Option<SavedWindowGeometry>,
    #[serde(default)]
    pub(crate) skipped_update_target_version: Option<String>,
    #[serde(default)]
    pub(crate) suppress_update_check_failure_dialog: bool,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            last_root: None,
            default_root: None,
            show_preview: None,
            ignore_list_enabled: true,
            preview_panel_width: None,
            query_history: Vec::new(),
            results_panel_width: None,
            tabs: Vec::new(),
            active_tab: None,
            window: None,
            skipped_update_target_version: None,
            suppress_update_check_failure_dialog: false,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SavedTabState {
    pub(crate) root: String,
    pub(crate) use_filelist: bool,
    pub(crate) use_regex: bool,
    #[serde(default = "default_ignore_case")]
    pub(crate) ignore_case: bool,
    pub(crate) include_files: bool,
    pub(crate) include_dirs: bool,
    #[serde(default)]
    pub(crate) max_depth: crate::indexer::MaxDepth,
    #[serde(default)]
    pub(crate) follow_links: bool,
    pub(crate) query: String,
    #[serde(default)]
    pub(crate) query_history: Vec<String>,
    #[serde(default)]
    pub(crate) tab_accent: Option<TabAccentColor>,
}

fn default_ignore_case() -> bool {
    true
}

fn default_ignore_list_enabled() -> bool {
    true
}

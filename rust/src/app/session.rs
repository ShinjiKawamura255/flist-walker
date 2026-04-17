use super::FlistWalkerApp;
use crate::path_utils::{normalize_windows_path_buf, path_key};
use crate::fs_atomic::write_text_atomic;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

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
    pub(super) fn persist_state_and_shutdown(&mut self, phase: &str) {
        self.apply_stable_window_geometry(true);
        self.shell.ui.ui_state_dirty = true;
        self.maybe_save_ui_state(true);
        let _ = self.shutdown_workers_with_timeout(Self::WORKER_JOIN_TIMEOUT, phase);
    }

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
            .map(normalize_windows_path_buf);
        let default_root = ui_state
            .default_root
            .as_deref()
            .map(PathBuf::from)
            .map(normalize_windows_path_buf);
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
                let root = normalize_windows_path_buf(PathBuf::from(&tab.root));
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

    pub fn startup_window_geometry() -> Option<(egui::Pos2, egui::Vec2)> {
        let state = Self::load_ui_state();
        let saved = state.window?;
        let normalized = Self::normalize_restore_geometry(saved);
        Self::append_window_trace(
            "startup_window_geometry",
            &format!("normalized={:?}", normalized),
        );
        Some((
            egui::pos2(normalized.x, normalized.y),
            egui::vec2(normalized.width, normalized.height),
        ))
    }

    pub fn startup_window_size() -> Option<egui::Vec2> {
        let (_, size) = Self::startup_window_geometry()?;
        Some(size)
    }

    fn saved_roots_file_path() -> Option<PathBuf> {
        #[cfg(windows)]
        {
            if let Some(base) = std::env::var_os("USERPROFILE") {
                return Some(PathBuf::from(base).join(".flistwalker_roots.txt"));
            }
        }
        #[cfg(not(windows))]
        {
            if let Some(base) = std::env::var_os("HOME") {
                return Some(PathBuf::from(base).join(".flistwalker_roots.txt"));
            }
        }
        None
    }

    pub(super) fn load_saved_roots() -> Vec<PathBuf> {
        let Some(file) = Self::saved_roots_file_path() else {
            return Vec::new();
        };
        let Ok(text) = fs::read_to_string(file) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }
            let path = normalize_windows_path_buf(PathBuf::from(line));
            let key = path_key(&path);
            if seen.insert(key) {
                out.push(path);
            }
        }
        out
    }

    fn save_saved_roots(&self) {
        let Some(file) = Self::saved_roots_file_path() else {
            return;
        };
        if let Some(parent) = file.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let text = self
            .shell
            .features
            .root_browser
            .saved_roots
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let text_to_write = if text.is_empty() {
            String::new()
        } else {
            format!("{text}\n")
        };
        let _ = write_text_atomic(&file, &text_to_write);
    }

    pub(super) fn add_current_root_to_saved(&mut self) {
        let root = self
            .shell
            .runtime
            .root
            .canonicalize()
            .unwrap_or_else(|_| self.shell.runtime.root.clone());
        let root = normalize_windows_path_buf(root);
        let key = path_key(&root);
        if self
            .shell
            .features
            .root_browser
            .saved_roots
            .iter()
            .any(|p| path_key(p) == key)
        {
            self.set_notice("Current root is already registered");
            return;
        }
        self.shell
            .features
            .root_browser
            .saved_roots
            .push(root.clone());
        self.shell
            .features
            .root_browser
            .saved_roots
            .sort_by_key(|p| p.to_string_lossy().to_string().to_ascii_lowercase());
        self.save_saved_roots();
        self.set_notice(format!("Registered root: {}", root.display()));
    }

    pub(super) fn set_current_root_as_default(&mut self) {
        self.set_current_root_as_default_with(Self::restore_tabs_enabled());
    }

    pub(super) fn set_current_root_as_default_with(&mut self, restore_tabs_enabled: bool) {
        if !Self::can_set_current_root_as_default_with(restore_tabs_enabled) {
            self.set_notice("Set as default is disabled while FLISTWALKER_RESTORE_TABS is enabled");
            return;
        }
        let root = self
            .shell
            .runtime
            .root
            .canonicalize()
            .unwrap_or_else(|_| self.shell.runtime.root.clone());
        let root = normalize_windows_path_buf(root);
        self.shell.features.root_browser.default_root = Some(root.clone());
        self.mark_ui_state_dirty();
        self.persist_ui_state_now();
        self.set_notice(format!("Set default root: {}", root.display()));
    }

    pub(super) fn can_set_current_root_as_default(&self) -> bool {
        Self::can_set_current_root_as_default_with(Self::restore_tabs_enabled())
    }

    pub(super) fn can_set_current_root_as_default_with(restore_tabs_enabled: bool) -> bool {
        !restore_tabs_enabled
    }

    pub(super) fn remove_current_root_from_saved(&mut self) {
        let key = path_key(&self.shell.runtime.root);
        let before = self.shell.features.root_browser.saved_roots.len();
        self.shell
            .features
            .root_browser
            .saved_roots
            .retain(|p| path_key(p) != key);
        if self.shell.features.root_browser.saved_roots.len() == before {
            self.set_notice("Current root is not in saved list");
            return;
        }
        if self
            .shell
            .features
            .root_browser
            .default_root
            .as_ref()
            .is_some_and(|p| path_key(p) == key)
        {
            self.shell.features.root_browser.default_root = None;
            self.mark_ui_state_dirty();
        }
        self.save_saved_roots();
        self.set_notice("Removed current root from saved list");
    }

    pub(super) fn save_ui_state(&self) {
        let Some(path) = Self::ui_state_file_path() else {
            return;
        };
        self.save_ui_state_to_path(&path);
    }

    pub(super) fn save_ui_state_to_path(&self, path: &Path) {
        self.save_ui_state_to_path_inner(path, Self::history_persist_disabled());
    }

    #[cfg(test)]
    pub(super) fn save_ui_state_to_path_with_history_persist_disabled(
        &self,
        path: &Path,
        disabled: bool,
    ) {
        self.save_ui_state_to_path_inner(path, disabled);
    }

    fn save_ui_state_to_path_inner(&self, path: &Path, history_persist_disabled: bool) {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let last_root_for_startup = if !Self::restore_tabs_enabled() {
            self.shell
                .features
                .root_browser
                .default_root
                .clone()
                .or_else(|| Some(self.shell.runtime.root.clone()))
                .unwrap_or_else(|| self.shell.runtime.root.clone())
        } else {
            self.shell.runtime.root.clone()
        };
        let state = UiState {
            last_root: Some(
                last_root_for_startup
                    .canonicalize()
                    .unwrap_or(last_root_for_startup)
                    .to_string_lossy()
                    .to_string(),
            ),
            default_root: self
                .shell
                .features
                .root_browser
                .default_root
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
            show_preview: Some(self.shell.ui.show_preview),
            preview_panel_width: Some(self.shell.ui.preview_panel_width),
            query_history: if history_persist_disabled {
                Vec::new()
            } else {
                self.shell
                    .runtime
                    .query_state
                    .query_history
                    .iter()
                    .cloned()
                    .collect()
            },
            results_panel_width: None,
            tabs: self.saved_tabs_for_ui_state(),
            active_tab: Some(self.shell.tabs.active_tab_index()),
            window: self.shell.ui.window_geometry.clone(),
            skipped_update_target_version: self
                .shell
                .features
                .update
                .state
                .skipped_target_version
                .clone(),
            suppress_update_check_failure_dialog: self
                .shell
                .features
                .update
                .state
                .suppress_check_failure_dialog,
        };
        if let Ok(text) = serde_json::to_string_pretty(&state) {
            let _ = write_text_atomic(path, &text);
            Self::append_window_trace(
                "save_ui_state",
                &format!(
                    "window={:?} preview_panel_width={:.1}",
                    state.window, self.shell.ui.preview_panel_width
                ),
            );
        }
    }

    pub(super) fn mark_ui_state_dirty(&mut self) {
        self.shell.ui.ui_state_dirty = true;
    }

    pub(super) fn maybe_save_ui_state(&mut self, force: bool) {
        if !self.shell.ui.ui_state_dirty {
            return;
        }
        if force || self.shell.ui.last_ui_state_save.elapsed() >= Self::UI_STATE_SAVE_INTERVAL {
            self.save_ui_state();
            self.shell.ui.ui_state_dirty = false;
            self.shell.ui.last_ui_state_save = Instant::now();
        }
    }

    pub(super) fn persist_ui_state_now(&mut self) {
        self.save_ui_state();
        self.shell.ui.ui_state_dirty = false;
        self.shell.ui.last_ui_state_save = Instant::now();
    }

    #[cfg(test)]
    pub(super) fn persist_ui_state_to_path_now(&mut self, path: &Path) {
        self.save_ui_state_to_path(path);
        self.shell.ui.ui_state_dirty = false;
        self.shell.ui.last_ui_state_save = Instant::now();
    }

    fn to_stable_window_geometry(geom: SavedWindowGeometry) -> SavedWindowGeometry {
        let round = |v: f32| (v * 10.0).round() / 10.0;
        let mut width = round(geom.width.max(640.0));
        let mut height = round(geom.height.max(400.0));
        if let Some(mw) = geom.monitor_width {
            let cap = round(mw.max(640.0));
            width = width.min(cap);
        }
        if let Some(mh) = geom.monitor_height {
            let cap = round(mh.max(400.0));
            height = height.min(cap);
        }
        SavedWindowGeometry {
            x: round(geom.x),
            y: round(geom.y),
            width,
            height,
            monitor_width: geom.monitor_width.map(round),
            monitor_height: geom.monitor_height.map(round),
        }
    }

    pub(super) fn window_geometry_from_rects(
        outer_rect: egui::Rect,
        inner_rect: Option<egui::Rect>,
        monitor_size: Option<egui::Vec2>,
    ) -> SavedWindowGeometry {
        let size_rect = inner_rect.unwrap_or(outer_rect);
        SavedWindowGeometry {
            x: outer_rect.min.x,
            y: outer_rect.min.y,
            width: size_rect.width(),
            height: size_rect.height(),
            monitor_width: monitor_size.map(|s| s.x),
            monitor_height: monitor_size.map(|s| s.y),
        }
    }

    pub(super) fn normalize_restore_geometry(saved: SavedWindowGeometry) -> SavedWindowGeometry {
        let mut width = saved.width.max(640.0);
        let mut height = saved.height.max(400.0);
        if let Some(mw) = saved.monitor_width {
            width = width.min(mw.max(640.0));
        }
        if let Some(mh) = saved.monitor_height {
            height = height.min(mh.max(400.0));
        }
        SavedWindowGeometry {
            x: saved.x,
            y: saved.y,
            width,
            height,
            monitor_width: saved.monitor_width,
            monitor_height: saved.monitor_height,
        }
    }

    pub(super) fn apply_stable_window_geometry(&mut self, force: bool) {
        let Some(pending) = self.shell.ui.pending_window_geometry.clone() else {
            return;
        };
        if !force
            && self.shell.ui.last_window_geometry_change.elapsed()
                < Self::WINDOW_GEOMETRY_SETTLE_INTERVAL
        {
            return;
        }
        if self.shell.ui.window_geometry.as_ref() != Some(&pending) {
            self.shell.ui.window_geometry = Some(pending.clone());
            self.mark_ui_state_dirty();
            Self::append_window_trace(
                "window_geometry_committed",
                &format!(
                    "committed={:?} force={}",
                    self.shell.ui.window_geometry, force
                ),
            );
        }
        self.shell.ui.pending_window_geometry = None;
    }

    pub(super) fn capture_window_geometry(&mut self, ctx: &egui::Context) {
        let next = ctx.input(|i| {
            let outer = i.viewport().outer_rect?;
            let inner = i.viewport().inner_rect;
            let monitor_size = i.viewport().monitor_size;
            Some(Self::window_geometry_from_rects(outer, inner, monitor_size))
        });
        let Some(next) = next.map(Self::to_stable_window_geometry) else {
            return;
        };
        if let (Some(mw), Some(mh)) = (next.monitor_width, next.monitor_height) {
            let width_limit = (mw * 1.05).max(640.0);
            let height_limit = (mh * 1.05).max(400.0);
            if next.width > width_limit || next.height > height_limit {
                Self::append_window_trace(
                    "capture_window_geometry_rejected_oversize",
                    &format!(
                        "x={:.1} y={:.1} w={:.1} h={:.1} mw={:.1} mh={:.1}",
                        next.x, next.y, next.width, next.height, mw, mh
                    ),
                );
                return;
            }
        }
        if self.shell.ui.pending_window_geometry.as_ref() != Some(&next)
            && self.shell.ui.window_geometry.as_ref() != Some(&next)
        {
            let prev_committed = self.shell.ui.window_geometry.clone();
            let prev_pending = self.shell.ui.pending_window_geometry.clone();
            self.shell.ui.pending_window_geometry = Some(next);
            self.shell.ui.last_window_geometry_change = Instant::now();
            if Self::window_trace_verbose_enabled() {
                Self::append_window_trace(
                    "capture_window_geometry_changed",
                    &format!(
                        "prev_committed={:?} prev_pending={:?} next_pending={:?}",
                        prev_committed, prev_pending, self.shell.ui.pending_window_geometry
                    ),
                );
            }
        }
    }
}

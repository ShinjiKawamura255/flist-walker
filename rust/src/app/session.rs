use super::FlistWalkerApp;
#[cfg(test)]
use crate::fs_atomic::write_text_atomic;
use crate::path_utils::normalize_windows_path_buf;
#[cfg(test)]
pub(super) use crate::persistence::SettingsCommitReceipt;
#[cfg(test)]
use crate::persistence::{
    canonicalize_last_root_for_persistence, shutdown_ui_state_persistence_for_test,
};
pub(super) use crate::persistence::{
    enqueue_settings_commit, SavedTabState, SavedWindowGeometry, SettingsCommitRequest,
    SettingsCommitResponse, TabAccentColor, UiState, UiStatePatch,
};
use crate::persistence::{enqueue_ui_state_patch, flush_ui_state_persistence};
use eframe::egui;
#[cfg(test)]
use serde_json::Value;
#[cfg(test)]
use std::fs;
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
use std::time::Instant;

/// Startup placement resolved against physical monitor rectangles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StartupWindowPlacement {
    pub physical_position: Option<egui::Pos2>,
    pub logical_size: egui::Vec2,
    pub scale_factor: f32,
}

#[derive(Clone, Debug, Default)]
pub(super) struct LaunchSettings {
    pub(super) last_root: Option<PathBuf>,
    pub(super) default_root: Option<PathBuf>,
    pub(super) show_preview: bool,
    pub(super) ignore_list_enabled: bool,
    pub(super) preview_panel_width: f32,
    pub(super) query_history: Vec<String>,
    pub(super) restore_tabs: Vec<SavedTabState>,
    pub(super) restore_active_tab: Option<usize>,
    pub(super) skipped_update_target_version: Option<String>,
    pub(super) suppress_update_check_failure_dialog: bool,
    #[cfg(test)]
    pub(super) test_settings_paths: Option<super::TestSettingsPaths>,
}

impl FlistWalkerApp {
    pub(super) const SET_DEFAULT_DISABLED_BY_RESTORE_TABS_NOTICE: &'static str =
        "Set as default is unavailable while Restore Tabs is enabled because the last session takes priority at startup.";
    pub(super) const SET_DEFAULT_DISABLED_BY_RESTORE_TABS_TOOLTIP: &'static str =
        "Unavailable while Restore Tabs is enabled. The last session takes priority at startup, so the default root is not used.";

    pub(super) fn persist_state_and_shutdown(&mut self, phase: &str) {
        self.apply_stable_window_geometry(true);
        self.shell.ui.ui_state_dirty = true;
        if self.settings_commit_in_progress() {
            if let Some(path) = self.persistence_ui_state_file_path() {
                flush_ui_state_persistence(&path, Self::WORKER_JOIN_TIMEOUT);
            }
            self.poll_settings_commit_response();
        }
        self.maybe_save_ui_state(true);
        if let Some(path) = self.persistence_ui_state_file_path() {
            flush_ui_state_persistence(&path, Self::WORKER_JOIN_TIMEOUT);
        }
        let _ = self.shutdown_workers_with_timeout(Self::WORKER_JOIN_TIMEOUT, phase);
        Self::shutdown_window_trace(Self::WORKER_JOIN_TIMEOUT);
        #[cfg(test)]
        if let Some(path) = self.persistence_ui_state_file_path() {
            shutdown_ui_state_persistence_for_test(&path, Self::WORKER_JOIN_TIMEOUT);
        }
    }

    pub(super) fn ui_state_file_path() -> Option<PathBuf> {
        crate::persistence::ui_state_file_path()
    }

    pub(super) fn persistence_ui_state_file_path(&self) -> Option<PathBuf> {
        #[cfg(test)]
        {
            self.test_settings_paths
                .as_ref()
                .map(|paths| paths.ui_state.clone())
        }
        #[cfg(not(test))]
        {
            Self::ui_state_file_path()
        }
    }

    #[cfg(test)]
    pub(super) fn ui_state_file_path_in(base: &Path) -> PathBuf {
        crate::persistence::ui_state_file_path_in(base)
    }

    pub(super) fn load_ui_state() -> UiState {
        crate::persistence::load_ui_state()
    }

    #[cfg(test)]
    fn read_ui_state_from_path(path: &Path) -> UiState {
        crate::persistence::read_ui_state_from_path(path)
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
            ignore_list_enabled: ui_state.ignore_list_enabled,
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
            #[cfg(test)]
            test_settings_paths: None,
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
                if root.as_os_str().is_empty() {
                    return None;
                }
                Some(SavedTabState {
                    root: root.to_string_lossy().to_string(),
                    use_filelist: tab.use_filelist,
                    use_regex: tab.use_regex,
                    ignore_case: tab.ignore_case,
                    include_files: tab.include_files,
                    include_dirs: tab.include_dirs,
                    max_depth: tab.max_depth,
                    follow_links: tab.follow_links,
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

    pub(super) fn saved_roots_file_path() -> Option<PathBuf> {
        crate::persistence::saved_roots_file_path()
    }

    #[cfg(test)]
    pub(super) fn saved_roots_file_path_in(base: &Path) -> PathBuf {
        crate::persistence::saved_roots_file_path_in(base)
    }

    pub(super) fn persistence_saved_roots_file_path(&self) -> Option<PathBuf> {
        #[cfg(test)]
        {
            self.test_settings_paths
                .as_ref()
                .map(|paths| paths.saved_roots.clone())
        }
        #[cfg(not(test))]
        {
            Self::saved_roots_file_path()
        }
    }

    #[cfg(not(test))]
    pub(super) fn load_saved_roots() -> Vec<PathBuf> {
        crate::persistence::load_saved_roots()
    }

    #[cfg(test)]
    pub(super) fn load_saved_roots_from_path(file: &Path) -> Vec<PathBuf> {
        crate::persistence::read_saved_roots_from_path(file)
    }
}

impl FlistWalkerApp {
    pub(super) fn set_current_root_as_default(&mut self) {
        self.set_current_root_as_default_with(Self::restore_tabs_enabled());
    }

    pub(super) fn set_current_root_as_default_with(&mut self, restore_tabs_enabled: bool) {
        if !Self::can_set_current_root_as_default_with(restore_tabs_enabled) {
            self.set_notice(Self::SET_DEFAULT_DISABLED_BY_RESTORE_TABS_NOTICE);
            return;
        }
        self.start_default_root_settings_commit(self.shell.runtime.root.clone());
    }

    pub(super) fn can_set_current_root_as_default(&self) -> bool {
        Self::can_set_current_root_as_default_with(Self::restore_tabs_enabled())
            && !self.settings_commit_in_progress()
    }

    pub(super) fn can_set_current_root_as_default_with(restore_tabs_enabled: bool) -> bool {
        !restore_tabs_enabled
    }

    pub(super) fn save_ui_state(&self) {
        let Some(path) = self.persistence_ui_state_file_path() else {
            return;
        };
        let history_persist_disabled = Self::history_persist_disabled();
        let state = self.ui_state_snapshot(history_persist_disabled);
        enqueue_ui_state_patch(
            path,
            UiStatePatch::from_ui_state(&state),
            state.query_history,
            history_persist_disabled,
        );
    }

    #[cfg(test)]
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

    fn ui_state_snapshot(&self, history_persist_disabled: bool) -> UiState {
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
        UiState {
            last_root: Some(last_root_for_startup.to_string_lossy().to_string()),
            default_root: self
                .shell
                .features
                .root_browser
                .default_root
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
            show_preview: Some(self.shell.ui.show_preview),
            ignore_list_enabled: self.shell.ui.ignore_list_enabled,
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
        }
    }

    #[cfg(test)]
    fn save_ui_state_to_path_inner(&self, path: &Path, history_persist_disabled: bool) {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let state = self.ui_state_snapshot(history_persist_disabled);
        let mut state =
            serde_json::to_value(state).unwrap_or_else(|_| Value::Object(Default::default()));
        canonicalize_last_root_for_persistence(&mut state);
        if let Ok(text) = serde_json::to_string_pretty(&state) {
            let _ = write_text_atomic(path, &text);
            Self::append_window_trace(
                "save_ui_state",
                &format!(
                    "window={:?} preview_panel_width={:.1}",
                    self.shell.ui.window_geometry, self.shell.ui.preview_panel_width
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
        if self.settings_commit_in_progress() {
            return;
        }
        if force || self.shell.ui.last_ui_state_save.elapsed() >= Self::UI_STATE_SAVE_INTERVAL {
            self.save_ui_state();
            self.shell.ui.ui_state_dirty = false;
            self.shell.ui.last_ui_state_save = Instant::now();
        }
    }

    pub(super) fn persist_ui_state_now(&mut self) {
        if self.settings_commit_in_progress() {
            self.shell.ui.ui_state_dirty = true;
            return;
        }
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
            pixels_per_point: geom.pixels_per_point,
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
            pixels_per_point: None,
        }
    }

    pub fn startup_window_placement(
        monitors: &[(egui::Rect, f32)],
        current_monitor: Option<usize>,
        can_position: bool,
    ) -> Option<StartupWindowPlacement> {
        Some(Self::startup_window_placement_from_ui_state(
            Self::load_ui_state(),
            monitors,
            current_monitor,
            can_position,
        ))
    }

    pub(super) fn startup_window_placement_from_ui_state(
        ui_state: UiState,
        monitors: &[(egui::Rect, f32)],
        current_monitor: Option<usize>,
        can_position: bool,
    ) -> StartupWindowPlacement {
        let saved = ui_state.window.unwrap_or_else(|| SavedWindowGeometry {
            width: 1400.0,
            height: 900.0,
            ..Default::default()
        });
        Self::normalize_startup_placement(saved, monitors, current_monitor, can_position)
    }

    pub(super) fn normalize_startup_placement(
        saved: SavedWindowGeometry,
        monitors: &[(egui::Rect, f32)],
        current_monitor: Option<usize>,
        can_position: bool,
    ) -> StartupWindowPlacement {
        let positive = |value: f32| value.is_finite() && value > 0.0;
        let scale = saved.pixels_per_point.filter(|value| positive(*value));
        let physical_position = scale
            .filter(|_| can_position && saved.x.is_finite() && saved.y.is_finite())
            .map(|scale| egui::pos2(saved.x * scale, saved.y * scale))
            .filter(|position| position.is_finite());
        let valid = monitors
            .iter()
            .enumerate()
            .filter(|(_, (rect, scale))| {
                rect.is_finite()
                    && positive(rect.width())
                    && positive(rect.height())
                    && positive(*scale)
            })
            .collect::<Vec<_>>();
        let target = physical_position
            .and_then(|position| {
                valid
                    .iter()
                    .min_by(|(_, (left, _)), (_, (right, _))| {
                        left.clamp(position)
                            .distance_sq(position)
                            .total_cmp(&right.clamp(position).distance_sq(position))
                    })
                    .copied()
            })
            .or_else(|| {
                valid
                    .iter()
                    .find(|(index, _)| Some(*index) == current_monitor)
                    .copied()
            })
            .or_else(|| valid.first().copied());
        let bounded = |value: f32, fallback: f32, minimum: f32, previous_monitor: Option<f32>| {
            let value = if positive(value) { value } else { fallback };
            value.clamp(minimum, 16000.0).min(
                previous_monitor
                    .filter(|value| positive(*value))
                    .unwrap_or(16000.0)
                    .max(minimum),
            )
        };
        let mut logical_size = egui::vec2(
            bounded(saved.width, 1400.0, 640.0, saved.monitor_width),
            bounded(saved.height, 900.0, 400.0, saved.monitor_height),
        );
        let Some((_, (monitor, scale_factor))) = target else {
            return StartupWindowPlacement {
                physical_position: None,
                logical_size,
                scale_factor: 1.0,
            };
        };
        logical_size = logical_size.min(monitor.size() / *scale_factor);
        // Clamp against one real screen, never the bounding box across screen gaps.
        let clamp_to_monitor = |position: egui::Pos2| {
            let max = (monitor.max - logical_size * *scale_factor).max(monitor.min);
            egui::pos2(
                position.x.clamp(monitor.min.x, max.x),
                position.y.clamp(monitor.min.y, max.y),
            )
        };
        let physical_position = physical_position.map(clamp_to_monitor).or_else(|| {
            can_position.then(|| {
                // Regression guard: never delegate a positionless startup to the window
                // manager when a real monitor is known. Platform/default placement can otherwise
                // reuse an off-screen or display-gap position after the monitor layout changes.
                clamp_to_monitor(monitor.center() - logical_size * *scale_factor * 0.5)
            })
        });
        StartupWindowPlacement {
            physical_position,
            logical_size,
            scale_factor: *scale_factor,
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
        let pixels_per_point = ctx.pixels_per_point();
        let next = ctx.input(|i| {
            let outer = i.viewport().outer_rect?;
            let inner = i.viewport().inner_rect;
            let monitor_size = i.viewport().monitor_size;
            let mut saved = Self::window_geometry_from_rects(outer, inner, monitor_size);
            saved.pixels_per_point = Some(pixels_per_point);
            Some(saved)
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

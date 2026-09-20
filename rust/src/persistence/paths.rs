//! Settings locations, legacy migration, and synchronous startup reads.
use super::schema::UiState;
use super::worker::seed_persisted_history_snapshot;
use crate::path_utils::{normalize_windows_path_buf, path_key};
#[cfg(not(test))]
use crate::runtime_config::settings_base_dir;
use crate::runtime_config::{legacy_settings_base_dirs, migrate_file_if_needed};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn ui_state_file_path() -> Option<PathBuf> {
    #[cfg(test)]
    {
        None
    }
    #[cfg(not(test))]
    {
        settings_base_dir().map(|base| ui_state_file_path_in(&base))
    }
}

pub(crate) fn ui_state_file_path_in(base: &Path) -> PathBuf {
    base.join(".flistwalker_ui_state.json")
}

pub(crate) fn load_ui_state() -> UiState {
    let Some(path) = ui_state_file_path() else {
        return UiState::default();
    };
    let source_path = migrate_or_legacy_ui_state_path(&path);
    let state = read_ui_state_from_path(&source_path);
    seed_persisted_history_snapshot(path, &state.query_history);
    state
}

pub(crate) fn read_ui_state_from_path(path: &Path) -> UiState {
    let Ok(text) = fs::read_to_string(path) else {
        return UiState::default();
    };
    serde_json::from_str::<UiState>(&text).unwrap_or_default()
}

pub(crate) fn saved_roots_file_path() -> Option<PathBuf> {
    #[cfg(test)]
    {
        None
    }
    #[cfg(not(test))]
    {
        settings_base_dir().map(|base| saved_roots_file_path_in(&base))
    }
}

pub(crate) fn saved_roots_file_path_in(base: &Path) -> PathBuf {
    base.join(".flistwalker_roots.txt")
}

pub(crate) fn load_saved_roots() -> Vec<PathBuf> {
    let Some(file) = saved_roots_file_path() else {
        return Vec::new();
    };
    let file = migrate_or_legacy_saved_roots_path(&file);
    read_saved_roots_from_path(&file)
}

pub(crate) fn migrate_or_legacy_ui_state_path(current_path: &Path) -> PathBuf {
    let legacy_paths = legacy_settings_base_dirs()
        .into_iter()
        .map(|base| ui_state_file_path_in(&base))
        .collect::<Vec<_>>();
    migrate_or_legacy_path(current_path, &legacy_paths)
}

pub(crate) fn migrate_or_legacy_saved_roots_path(current_path: &Path) -> PathBuf {
    let legacy_paths = legacy_settings_base_dirs()
        .into_iter()
        .map(|base| saved_roots_file_path_in(&base))
        .collect::<Vec<_>>();
    migrate_or_legacy_path(current_path, &legacy_paths)
}

pub(crate) fn migrate_or_legacy_path(current_path: &Path, legacy_paths: &[PathBuf]) -> PathBuf {
    if current_path.exists() {
        return current_path.to_path_buf();
    }
    for legacy_path in legacy_paths {
        if migrate_file_if_needed(current_path, legacy_path) {
            return current_path.to_path_buf();
        }
    }
    for legacy_path in legacy_paths {
        if legacy_path.exists() {
            return legacy_path.to_path_buf();
        }
    }
    current_path.to_path_buf()
}

pub(crate) fn read_saved_roots_from_path(file: &Path) -> Vec<PathBuf> {
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

//! Shared settings and query-history persistence for all frontends.
mod paths;
mod schema;
mod worker;

use crate::path_utils::normalize_windows_path_buf;
use crate::query_history::MAX_QUERY_HISTORY_ENTRIES;
pub(crate) use paths::{
    load_saved_roots, load_ui_state, read_ui_state_from_path, saved_roots_file_path,
    ui_state_file_path,
};
#[cfg(test)]
pub(crate) use paths::{
    read_saved_roots_from_path, saved_roots_file_path_in, ui_state_file_path_in,
};
pub(crate) use schema::{SavedTabState, SavedWindowGeometry, TabAccentColor, UiState};
use std::path::{Path, PathBuf};
pub use worker::AsyncHistoryPersistence;
#[cfg(test)]
pub(crate) use worker::{
    canonicalize_last_root_for_persistence, shutdown_ui_state_persistence_for_test,
    SettingsCommitReceipt,
};
pub(crate) use worker::{
    enqueue_settings_commit, enqueue_ui_state_patch, flush_ui_state_persistence,
    ui_state_persistence_status, SettingsCommitRequest, SettingsCommitResponse, UiStatePatch,
    UiStatePersistenceStatus,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PersistedRootsAndHistory {
    pub last_root: Option<PathBuf>,
    pub default_root: Option<PathBuf>,
    pub saved_roots: Vec<PathBuf>,
    pub query_history: Vec<String>,
}

/// Reports whether query-history persistence is enabled for the current process.
/// Consumers use this before starting an interactive session so the terminal loop
/// never needs to inspect configuration or acquire persistence locks.
pub fn history_persistence_enabled() -> bool {
    !history_persist_disabled()
}

pub fn load_persisted_roots_and_history() -> PersistedRootsAndHistory {
    let ui_state = load_ui_state();
    let saved_roots = load_saved_roots();
    persisted_roots_and_history_from_ui_state(ui_state, saved_roots, history_persist_disabled())
}

pub fn load_persisted_roots_and_history_from_paths(
    ui_state_path: &Path,
    saved_roots_path: &Path,
    history_persist_disabled: bool,
) -> PersistedRootsAndHistory {
    persisted_roots_and_history_from_ui_state(
        read_ui_state_from_path(ui_state_path),
        paths::read_saved_roots_from_path(saved_roots_path),
        history_persist_disabled,
    )
}

fn persisted_roots_and_history_from_ui_state(
    ui_state: UiState,
    saved_roots: Vec<PathBuf>,
    history_persist_disabled: bool,
) -> PersistedRootsAndHistory {
    PersistedRootsAndHistory {
        last_root: ui_state
            .last_root
            .map(PathBuf::from)
            .map(normalize_windows_path_buf),
        default_root: ui_state
            .default_root
            .map(PathBuf::from)
            .map(normalize_windows_path_buf),
        saved_roots,
        query_history: if history_persist_disabled {
            Vec::new()
        } else {
            ui_state
                .query_history
                .into_iter()
                .rev()
                .take(MAX_QUERY_HISTORY_ENTRIES)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect()
        },
    }
}

pub(crate) fn history_persist_disabled() -> bool {
    std::env::var("FLISTWALKER_DISABLE_HISTORY_PERSIST")
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

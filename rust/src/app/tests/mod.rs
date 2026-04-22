#![allow(unused_imports)]

pub(super) use crate::app::coordinator::path_is_within_root;
pub(super) use crate::app::index_coordinator::IndexResponseRoute;
pub(super) use crate::app::session::UiState;
pub(super) use crate::app::state::{
    BackgroundIndexState, PendingFileListAfterIndex, PendingFileListAncestorConfirmation,
    PendingFileListConfirmation, PendingFileListUseWalkerConfirmation, SortMetadata,
    UpdateCheckFailureState, UpdateManager, UpdatePromptState, UpdateState,
};
pub(super) use crate::app::worker_protocol::{
    KindResolveRequest, KindResolveResponse, UpdateRequestKind,
};
pub(super) use crate::app::{clear_process_shutdown_request, process_shutdown_requested};
pub(super) use crate::app::{
    egui, ActionRequest, ActionResponse, AppRuntimeState, AppShellState, CacheStateBundle,
    EntryKind, FileListDialogKind, FileListManager, FileListRequest, FileListResponse,
    FlistWalkerApp, HighlightCacheKey, HighlightCacheState, IndexBuildResult, IndexEntry,
    IndexRequest, IndexResponse, IndexSource, LaunchSettings, PreviewRequest, PreviewResponse,
    QueryState, ResultSortMode, RootBrowserState, RuntimeUiState, SavedTabState,
    SavedWindowGeometry, SearchCoordinator, SearchRequest, SearchResponse, SortMetadataCacheState,
    SortMetadataRequest, SortMetadataResponse, TabAccentColor, TabAccentPalette, TabDragState,
    TabSessionState, UpdateRequest, UpdateResponse, WorkerBus, WorkerRuntime,
};
pub(super) use crate::app::{render_tabs, request_process_shutdown, spawn_kind_resolver_worker};
pub(super) use crate::entry::Entry;
pub(super) use crate::path_utils::{normalize_windows_path_buf, path_key};
pub(super) use crate::search::{SearchEntriesSnapshotKey, SearchPrefixCache};
pub(super) use crate::ui_model::normalize_path_for_display;
pub(super) use crate::updater::{UpdateCandidate, UpdateSupport};
pub(super) use std::collections::{HashMap, HashSet, VecDeque};
pub(super) use std::fs;
pub(super) use std::path::{Path, PathBuf};
pub(super) use std::sync::atomic::{AtomicBool, Ordering};
pub(super) use std::sync::mpsc;
pub(super) use std::sync::Arc;
pub(super) use std::thread;
pub(super) use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

mod support;
pub(super) use support::{
    commit_query_history_for_test, emacs_shortcut_modifiers, entries_count_from_status,
    gui_shortcut_modifiers, is_action_notice, reset_index_request_state_for_test,
    run_shortcuts_frame, tab_switch_shortcut_modifiers, test_root,
};

pub(super) fn unknown_entry(path: PathBuf) -> Entry {
    Entry::unknown(path)
}

pub(super) fn file_entry(path: PathBuf) -> Entry {
    Entry::file(path)
}

pub(super) fn dir_entry(path: PathBuf) -> Entry {
    Entry::dir(path)
}

mod app_core;
mod index_pipeline;
mod pipeline_tests;
mod query_history;
mod render_tests;
mod session_restore;
mod session_tabs;
mod shortcuts;
mod update_commands;
mod window_ime;

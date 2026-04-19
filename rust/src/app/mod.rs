use crate::entry::{Entry, EntryDisplayKind, EntryKind};
use crate::indexer::{IndexBuildResult, IndexSource};
use crate::path_utils::normalize_windows_path_buf;
use crate::ui_model::{display_path_with_mode, match_positions_for_path, normalize_path_for_display};
use crate::updater::{
    forced_update_check_failure_message, self_update_disabled, should_skip_update_prompt,
    UpdateSupport,
};
use eframe::egui;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};

mod bootstrap;
mod config;
mod cache;
mod coordinator;
mod filelist;
mod index_coordinator;
mod index_worker;
mod input;
mod input_dialogs;
mod input_history;
mod pipeline;
mod pipeline_owner;
mod preview_flow;
mod query_state;
mod render;
mod render_dialogs;
mod render_panels;
mod render_snapshot;
mod render_tabs;
mod render_theme;
mod response_flow;
mod result_flow;
mod result_reducer;
mod root_browser;
mod search_coordinator;
mod session;
mod state;
mod tab_state;
mod tabs;
mod ui_state;
mod update;
mod worker_tasks;
mod worker_bus;
mod worker_bus_lifecycle;
mod worker_protocol;
mod worker_runtime;
mod worker_support;
mod workers;

use cache::{EntryKindCacheState, HighlightCacheState, PreviewCacheState, SortMetadataCacheState};
use coordinator::{normalized_compare_key, path_is_within_root};
use index_coordinator::IndexCoordinator;
use index_worker::spawn_index_worker;
use pipeline_owner::PipelineOwner;
use query_state::QueryState;
use search_coordinator::SearchCoordinator;
use session::{LaunchSettings, SavedTabState, SavedWindowGeometry, TabAccentColor};
use state::{
    AppRuntimeState, AppShellState, BackgroundIndexState, CacheStateBundle, FeatureStateBundle,
    FileListDialogKind, FileListManager, HighlightCacheKey, PendingFileListAfterIndex,
    PendingFileListAncestorConfirmation, PendingFileListConfirmation,
    PendingFileListUseWalkerConfirmation, ResultSortMode, RootBrowserState, SortMetadata,
    TabAccentPalette, TabDragState, TabSessionState,
};
use tab_state::AppTabState;
use ui_state::RuntimeUiState;
use worker_bus::{
    ActionWorkerBus, FileListWorkerBus, KindWorkerBus, PreviewWorkerBus, SortWorkerBus,
    UpdateWorkerBus, WorkerBus,
};
use worker_protocol::{
    ActionRequest, ActionResponse, FileListRequest, FileListResponse, IndexEntry, IndexRequest,
    IndexResponse, KindResolveRequest, PreviewRequest, PreviewResponse, SearchRequest,
    SearchResponse, SortMetadataRequest, SortMetadataResponse, UpdateRequest, UpdateRequestKind,
    UpdateResponse,
};
use worker_runtime::WorkerRuntime;
use workers::{
    spawn_action_worker, spawn_filelist_worker, spawn_kind_resolver_worker, spawn_preview_worker,
    spawn_search_worker, spawn_sort_metadata_worker, spawn_update_worker,
};
mod shell_support;
#[cfg(test)]
pub(crate) use shell_support::clear_process_shutdown_request;
pub(crate) use shell_support::process_shutdown_requested;
pub use shell_support::{configure_egui_fonts, request_process_shutdown};

impl TabAccentColor {
    pub(super) const ALL: [Self; 8] = [
        Self::Teal,
        Self::Indigo,
        Self::Azure,
        Self::Amber,
        Self::Olive,
        Self::Emerald,
        Self::Crimson,
        Self::Magenta,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Teal => "Teal",
            Self::Indigo => "Indigo",
            Self::Azure => "Azure",
            Self::Amber => "Amber",
            Self::Olive => "Olive",
            Self::Emerald => "Emerald",
            Self::Crimson => "Crimson",
            Self::Magenta => "Magenta",
        }
    }

    pub(super) const fn palette(self, dark_mode: bool) -> TabAccentPalette {
        match (dark_mode, self) {
            (true, Self::Teal) => {
                TabAccentPalette::new((0x10, 0x2A, 0x30), (0x1F, 0x76, 0x7D), (0xE4, 0xFD, 0xFF))
            }
            (true, Self::Indigo) => {
                TabAccentPalette::new((0x16, 0x15, 0x2E), (0x4E, 0x52, 0xA6), (0xF4, 0xF2, 0xFF))
            }
            (true, Self::Azure) => {
                TabAccentPalette::new((0x0F, 0x1B, 0x33), (0x2B, 0x78, 0xC4), (0xE2, 0xF1, 0xFF))
            }
            (true, Self::Amber) => {
                TabAccentPalette::new((0x2D, 0x1F, 0x0F), (0xB5, 0x6B, 0x17), (0xFF, 0xE8, 0xC2))
            }
            (true, Self::Olive) => {
                TabAccentPalette::new((0x20, 0x27, 0x12), (0x6E, 0x8C, 0x23), (0xF0, 0xFF, 0xD8))
            }
            (true, Self::Emerald) => {
                TabAccentPalette::new((0x0F, 0x28, 0x1D), (0x1E, 0x8B, 0x5B), (0xE3, 0xFF, 0xF2))
            }
            (true, Self::Crimson) => {
                TabAccentPalette::new((0x2B, 0x11, 0x16), (0xB5, 0x45, 0x4F), (0xFF, 0xE3, 0xE7))
            }
            (true, Self::Magenta) => {
                TabAccentPalette::new((0x2A, 0x0F, 0x2B), (0x9B, 0x3E, 0xA8), (0xFF, 0xE6, 0xFF))
            }
            (false, Self::Teal) => {
                TabAccentPalette::new((0xE3, 0xF4, 0xF6), (0x74, 0xB9, 0xC0), (0x1F, 0x5A, 0x62))
            }
            (false, Self::Indigo) => {
                TabAccentPalette::new((0xEC, 0xEB, 0xFA), (0x8E, 0x87, 0xD6), (0x2F, 0x2F, 0x6A))
            }
            (false, Self::Azure) => {
                TabAccentPalette::new((0xE6, 0xF0, 0xFB), (0x7A, 0xAD, 0xE3), (0x1F, 0x3E, 0x69))
            }
            (false, Self::Amber) => {
                TabAccentPalette::new((0xFF, 0xF3, 0xDD), (0xE1, 0xA4, 0x4B), (0x6B, 0x4A, 0x16))
            }
            (false, Self::Olive) => {
                TabAccentPalette::new((0xEE, 0xF5, 0xDA), (0xA2, 0xB8, 0x5F), (0x45, 0x55, 0x1F))
            }
            (false, Self::Emerald) => {
                TabAccentPalette::new((0xE5, 0xF6, 0xEE), (0x6F, 0xB9, 0x8A), (0x1F, 0x5A, 0x3D))
            }
            (false, Self::Crimson) => {
                TabAccentPalette::new((0xFB, 0xE7, 0xEA), (0xE1, 0x89, 0x95), (0x6A, 0x1E, 0x2A))
            }
            (false, Self::Magenta) => {
                TabAccentPalette::new((0xF7, 0xE8, 0xF8), (0xD0, 0x8F, 0xD8), (0x5A, 0x1F, 0x60))
            }
        }
    }
}

/// eframe/egui の UI フレームと各種ワーカーを結線する coordinator。
pub struct FlistWalkerApp {
    shell: AppShellState,
}

impl FlistWalkerApp {
    /// 既定の launch 設定でアプリを初期化する。
    pub fn new(root: PathBuf, limit: usize, query: String) -> Self {
        Self::build_new(root, limit, query)
    }

    /// 永続化済み launch 設定と保存 tab を考慮して起動する。
    pub fn from_launch(root: PathBuf, limit: usize, query: String, root_explicit: bool) -> Self {
        Self::build_from_launch(root, limit, query, root_explicit)
    }
}

#[cfg(test)]
mod tests;

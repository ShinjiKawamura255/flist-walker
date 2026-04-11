use crate::entry::{Entry, EntryDisplayKind, EntryKind};
use crate::fs_atomic::write_text_atomic;
use crate::indexer::{
    find_filelist_in_first_level, has_ancestor_filelists, IndexBuildResult, IndexSource,
};
use crate::path_utils::normalize_windows_path_buf;
use crate::ui_model::{
    build_preview_text_with_kind, display_path_with_mode, match_positions_for_path,
    normalize_path_for_display, should_skip_preview,
};
use crate::updater::{
    forced_update_check_failure_message, self_update_disabled, should_skip_update_prompt,
    UpdateCandidate, UpdateSupport,
};
use eframe::egui;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::collections::{HashMap, HashSet, VecDeque};
#[allow(unused_imports)]
use std::fs;
#[allow(unused_imports)]
use std::fs::OpenOptions;
#[allow(unused_imports)]
use std::io::Write;
use std::path::{Path, PathBuf};
#[allow(unused_imports)]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
#[allow(unused_imports)]
use std::sync::{Arc, Mutex, OnceLock};
#[allow(unused_imports)]
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

mod bootstrap;
mod cache;
mod coordinator;
mod filelist;
mod index_coordinator;
mod index_worker;
mod input;
mod pipeline;
mod pipeline_owner;
mod preview_flow;
mod query_state;
mod render;
mod result_flow;
mod result_reducer;
mod search_coordinator;
mod session;
mod state;
mod tab_state;
mod tabs;
mod ui_state;
mod update;
mod worker_bus;
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
    TabAccentPalette, TabDragState, TabSessionState, UpdateCheckFailureState, UpdateManager,
    UpdatePromptState, UpdateState,
};
use tab_state::{AppTabState, TabIndexState, TabQueryState, TabResultState};
use ui_state::RuntimeUiState;
use worker_bus::{
    ActionWorkerBus, FileListWorkerBus, KindWorkerBus, PreviewWorkerBus, SortWorkerBus,
    UpdateWorkerBus, WorkerBus,
};
#[cfg(test)]
use worker_protocol::KindResolveResponse;
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
pub use shell_support::{configure_egui_fonts, request_process_shutdown};
pub(crate) use shell_support::process_shutdown_requested;
#[cfg(test)]
pub(crate) use shell_support::clear_process_shutdown_request;

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

impl std::ops::Deref for FlistWalkerApp {
    type Target = AppShellState;

    fn deref(&self) -> &Self::Target {
        &self.shell
    }
}

impl std::ops::DerefMut for FlistWalkerApp {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.shell
    }
}

impl FlistWalkerApp {
    const PREVIEW_CACHE_MAX_BYTES: usize = 32 * 1024 * 1024;
    const HIGHLIGHT_CACHE_MAX: usize = 256;
    const SORT_METADATA_CACHE_MAX: usize = 4096;
    const TAB_DRAG_START_DISTANCE: f32 = 6.0;
    const QUERY_HISTORY_MAX: usize = 100;
    const QUERY_HISTORY_IDLE_DELAY: Duration = Duration::from_millis(400);
    const INCREMENTAL_SEARCH_REFRESH_INTERVAL: Duration = Duration::from_millis(300);
    const INCREMENTAL_SEARCH_REFRESH_INTERVAL_DURING_INDEX: Duration = Duration::from_millis(1500);
    const INCREMENTAL_SEARCH_MIN_DELTA_DURING_INDEX: usize = 2048;
    const PAGE_MOVE_ROWS: isize = 10;
    const DEFAULT_PREVIEW_PANEL_WIDTH: f32 = 440.0;
    const MIN_RESULTS_PANEL_WIDTH: f32 = 220.0;
    const MIN_PREVIEW_PANEL_WIDTH: f32 = 220.0;
    const ROOT_SELECTOR_POPUP_ID: &'static str = "root-selector-popup";
    const INDEX_MAX_CONCURRENT: usize = 2;
    const INDEX_MAX_QUEUE: usize = 4;
    const UI_STATE_SAVE_INTERVAL: Duration = Duration::from_millis(500);
    const WINDOW_GEOMETRY_SETTLE_INTERVAL: Duration = Duration::from_millis(350);
    const MEMORY_SAMPLE_INTERVAL: Duration = Duration::from_millis(1000);
    // Regression guard: app close should not stall on background workers once
    // shutdown has been requested and all request senders have been dropped.
    const WORKER_JOIN_TIMEOUT: Duration = Duration::from_millis(250);
    const SHRINK_MIN_CAPACITY: usize = 4096;
    const SEARCH_HINTS_TOOLTIP: &'static str = "\
Search hints:
- トークンは AND 条件（例: main py）
- abc|foo|bar : OR 条件（スペースなしの | で連結）
- 'term : 完全一致トークン（例: 'main.py）
- !term : 除外トークン（例: main !test）
- ^term : 先頭一致を優先（例: ^src）
- term$ : 末尾一致を優先（例: .rs$）";

    /// 既定の launch 設定でアプリを初期化する。
    pub fn new(root: PathBuf, limit: usize, query: String) -> Self {
        let launch = LaunchSettings {
            show_preview: true,
            preview_panel_width: Self::DEFAULT_PREVIEW_PANEL_WIDTH,
            ..LaunchSettings::default()
        };
        Self::new_with_launch(root, limit, query, launch, None)
    }

    /// 永続化済み launch 設定と保存 tab を考慮して起動する。
    pub fn from_launch(root: PathBuf, limit: usize, query: String, root_explicit: bool) -> Self {
        let launch = Self::load_launch_settings();
        let restore_tabs_enabled = Self::restore_tabs_enabled();
        let saved_last_root = launch
            .last_root
            .as_ref()
            .and_then(|p| p.canonicalize().ok())
            .map(normalize_windows_path_buf)
            .filter(|p| p.is_dir());
        let saved_default = launch
            .default_root
            .as_ref()
            .and_then(|p| p.canonicalize().ok())
            .map(normalize_windows_path_buf)
            .filter(|p| p.is_dir());
        let restore_session = if restore_tabs_enabled && !root_explicit && query.trim().is_empty() {
            Self::sanitize_saved_tabs(&launch.restore_tabs, launch.restore_active_tab)
        } else {
            None
        };
        let chosen_root = Self::choose_startup_root(
            root,
            root_explicit,
            restore_tabs_enabled,
            restore_session.as_ref(),
            saved_last_root,
            saved_default,
        );
        let mut app = Self::new_with_launch(chosen_root, limit, query, launch, restore_session);
        app.request_startup_update_check();
        app
    }

    /// worker 群と launch seed を束ねて coordinator 本体を組み立てる。
    fn new_with_launch(
        root: PathBuf,
        limit: usize,
        query: String,
        launch: LaunchSettings,
        restore_session: Option<(Vec<SavedTabState>, usize)>,
    ) -> Self {
        let (
            search_tx,
            search_rx,
            worker_bus,
            index_tx,
            index_rx,
            latest_index_request_ids,
            worker_runtime,
        ) = Self::bootstrap_workers().into_parts();
        let (
            root,
            limit,
            query,
            query_history,
            saved_roots,
            default_root,
            show_preview,
            preview_panel_width,
            update_state,
        ) = Self::launch_seed(root, limit, query, &launch).into_parts();
        let mut app = Self {
            shell: AppShellState {
                runtime: AppRuntimeState {
                    root,
                    limit,
                    query_state: QueryState::new(query, query_history),
                    use_filelist: true,
                    use_regex: false,
                    ignore_case: true,
                    include_files: true,
                    include_dirs: true,
                    index: IndexBuildResult {
                        entries: Vec::new(),
                        source: IndexSource::None,
                    },
                    all_entries: Arc::new(Vec::new()),
                    entries: Arc::new(Vec::new()),
                    base_results: Vec::new(),
                    results: Vec::new(),
                    result_sort_mode: ResultSortMode::Score,
                    pinned_paths: HashSet::new(),
                    current_row: Some(0),
                    preview: String::new(),
                    notice: String::new(),
                    status_line: "Initializing...".to_string(),
                },
                search: SearchCoordinator::new(search_tx, search_rx),
                worker_bus,
                indexing: IndexCoordinator::new(index_tx, index_rx, latest_index_request_ids),
                ui: RuntimeUiState::new(show_preview, preview_panel_width),
                cache: CacheStateBundle {
                    preview: PreviewCacheState::default(),
                    highlight: HighlightCacheState::with_scope_ignore_case(true),
                    entry_kind: EntryKindCacheState::default(),
                    sort_metadata: SortMetadataCacheState::default(),
                },
                tabs: TabSessionState::default(),
                features: FeatureStateBundle {
                    root_browser: RootBrowserState {
                        #[cfg(test)]
                        browse_dialog_result: None,
                        saved_roots,
                        default_root,
                    },
                    filelist: FileListManager::default(),
                    update: UpdateManager::from_state(update_state),
                },
                worker_runtime: Some(worker_runtime),
            },
        };
        if let Some(path) = Self::window_trace_path() {
            Self::append_window_trace("app_initialized", &format!("path={}", path.display()));
        }
        if let Some((tabs, active_tab)) = restore_session {
            app.initialize_tabs_from_saved(tabs, active_tab);
        } else {
            app.initialize_tabs();
            app.request_index_refresh();
        }
        app
    }
}

#[cfg(test)]
mod tests;

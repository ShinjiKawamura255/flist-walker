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
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
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
mod query_state;
mod render;
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
    BackgroundIndexState, CacheStateBundle, FileListDialogKind, FileListManager, HighlightCacheKey,
    PendingFileListAfterIndex, PendingFileListAncestorConfirmation, PendingFileListConfirmation,
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

static PROCESS_SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn request_process_shutdown() {
    PROCESS_SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
}

fn process_shutdown_requested() -> bool {
    PROCESS_SHUTDOWN_REQUESTED.load(Ordering::SeqCst)
}

#[cfg(test)]
fn clear_process_shutdown_request() {
    PROCESS_SHUTDOWN_REQUESTED.store(false, Ordering::SeqCst);
}

pub fn configure_egui_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    if let Some(font_bytes) = load_cjk_font_bytes() {
        let font_name = "cjk_ui".to_string();
        fonts
            .font_data
            .insert(font_name.clone(), egui::FontData::from_owned(font_bytes));
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            family.insert(0, font_name.clone());
        }
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            family.push(font_name);
        }
    }

    ctx.set_fonts(fonts);
}

fn load_cjk_font_bytes() -> Option<Vec<u8>> {
    let mut candidates: Vec<&str> = Vec::new();

    #[cfg(windows)]
    {
        candidates.extend([
            r"C:\Windows\Fonts\YuGothR.ttc",
            r"C:\Windows\Fonts\YuGothM.ttc",
            r"C:\Windows\Fonts\meiryo.ttc",
            r"C:\Windows\Fonts\msgothic.ttc",
            r"C:\Windows\Fonts\MSYH.TTC",
        ]);
    }

    #[cfg(target_os = "macos")]
    {
        candidates.extend([
            "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
            "/System/Library/Fonts/ヒラギノ丸ゴ ProN W4.ttc",
            "/System/Library/Fonts/Hiragino Sans GB.ttc",
        ]);
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        candidates.extend([
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansJP-Regular.otf",
            "/usr/share/fonts/truetype/noto/NotoSansJP-Regular.otf",
        ]);
    }

    candidates.into_iter().find_map(|path| fs::read(path).ok())
}

/// eframe/egui の UI フレームと各種ワーカーを結線する coordinator。
pub struct FlistWalkerApp {
    root: PathBuf,
    limit: usize,
    query_state: QueryState,
    use_filelist: bool,
    use_regex: bool,
    ignore_case: bool,
    include_files: bool,
    include_dirs: bool,
    index: IndexBuildResult,
    all_entries: Arc<Vec<Entry>>,
    entries: Arc<Vec<Entry>>,
    base_results: Vec<(PathBuf, f64)>,
    results: Vec<(PathBuf, f64)>,
    result_sort_mode: ResultSortMode,
    pinned_paths: HashSet<PathBuf>,
    current_row: Option<usize>,
    preview: String,
    notice: String,
    status_line: String,
    search: SearchCoordinator,
    worker_bus: WorkerBus,
    indexing: IndexCoordinator,
    ui: RuntimeUiState,
    root_browser: RootBrowserState,
    cache: CacheStateBundle,
    tabs: TabSessionState,
    filelist_state: FileListManager,
    update_state: UpdateManager,
    worker_runtime: Option<WorkerRuntime>,
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

    fn window_trace_enabled() -> bool {
        static ENABLED: OnceLock<bool> = OnceLock::new();
        *ENABLED.get_or_init(|| {
            std::env::var("FLISTWALKER_WINDOW_TRACE")
                .map(|v| {
                    !(v == "0" || v.eq_ignore_ascii_case("false") || v.eq_ignore_ascii_case("off"))
                })
                .unwrap_or(false)
        })
    }

    fn window_trace_verbose_enabled() -> bool {
        static VERBOSE: OnceLock<bool> = OnceLock::new();
        *VERBOSE.get_or_init(|| {
            std::env::var("FLISTWALKER_WINDOW_TRACE_VERBOSE")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("on"))
                .unwrap_or(false)
        })
    }

    fn window_trace_path() -> Option<PathBuf> {
        if let Some(path) = std::env::var_os("FLISTWALKER_WINDOW_TRACE_PATH") {
            let path = PathBuf::from(path);
            if !path.as_os_str().is_empty() {
                return Some(path);
            }
        }
        #[cfg(windows)]
        {
            if let Some(base) = std::env::var_os("USERPROFILE") {
                return Some(PathBuf::from(base).join(".flistwalker_window_trace.log"));
            }
        }
        #[cfg(not(windows))]
        {
            if let Some(base) = std::env::var_os("HOME") {
                return Some(PathBuf::from(base).join(".flistwalker_window_trace.log"));
            }
        }
        None
    }

    fn append_window_trace(event: &str, details: &str) {
        if !Self::window_trace_enabled() {
            return;
        }
        let Some(path) = Self::window_trace_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or_default();
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "ts={} event={} {}", ts, event, details);
        }
    }

    /// ウィンドウ診断イベントを opt-in ログへ追記する。
    pub fn trace_window_event(event: &str, details: &str) {
        Self::append_window_trace(event, details);
    }

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
            search: SearchCoordinator::new(search_tx, search_rx),
            worker_bus,
            indexing: IndexCoordinator::new(index_tx, index_rx, latest_index_request_ids),
            ui: RuntimeUiState::new(show_preview, preview_panel_width),
            root_browser: RootBrowserState {
                #[cfg(test)]
                browse_dialog_result: None,
                saved_roots,
                default_root,
            },
            cache: CacheStateBundle {
                preview: PreviewCacheState::default(),
                highlight: HighlightCacheState::with_scope_ignore_case(true),
                entry_kind: EntryKindCacheState::default(),
                sort_metadata: SortMetadataCacheState::default(),
            },
            tabs: TabSessionState::default(),
            filelist_state: FileListManager::default(),
            update_state: UpdateManager::from_state(update_state),
            worker_runtime: Some(worker_runtime),
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

    /// クエリ履歴の永続化を一時的に無効化しているかを返す。
    fn history_persist_disabled() -> bool {
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

    /// root 外の path を含む操作要求の先頭違反要素を返す。
    fn first_action_path_outside_root(&self, paths: &[PathBuf]) -> Option<PathBuf> {
        paths
            .iter()
            .find(|path| !path_is_within_root(&self.root, path))
            .cloned()
    }

    /// 現在 root の表示文字列を UI 向けに整形する。
    fn root_display_text(&self) -> String {
        normalize_windows_path_buf(self.root.clone())
            .to_string_lossy()
            .to_string()
    }

    /// root 変更で失効する entry/result 系 state を掃除する。
    fn clear_root_scoped_entry_state(&mut self) {
        self.index.entries.clear();
        self.index.entries.shrink_to_fit();
        self.index.source = IndexSource::None;
        self.all_entries = Arc::new(Vec::new());
        self.entries = Arc::new(Vec::new());
        self.cache.entry_kind.clear();
        self.base_results.clear();
        self.base_results.shrink_to_fit();
        self.results.clear();
        self.results.shrink_to_fit();
        self.indexing.incremental_filtered_entries.clear();
        self.indexing.incremental_filtered_entries.shrink_to_fit();
        self.worker_bus.sort.pending_request_id = None;
        self.worker_bus.sort.in_progress = false;
        self.result_sort_mode = ResultSortMode::Score;
        self.clear_sort_metadata_cache();
        self.indexing.last_search_snapshot_len = 0;
    }

    /// 現在の source に応じて相対パス表示を優先するかを返す。
    fn prefer_relative_display(&self) -> bool {
        matches!(
            self.index.source,
            IndexSource::Walker | IndexSource::FileList(_)
        )
    }

    /// 指定 source に対する表示パス方針を返す。
    fn prefer_relative_display_for(source: &IndexSource) -> bool {
        matches!(source, IndexSource::Walker | IndexSource::FileList(_))
    }

    /// FileList source で type filter を固定する必要があるかを返す。
    fn use_filelist_requires_locked_filters(&self) -> bool {
        self.use_filelist && !matches!(self.index.source, IndexSource::Walker)
    }

    /// include flags に対して entry が可視対象かを判定する。
    fn is_entry_visible_for_flags(entry: &Entry, include_files: bool, include_dirs: bool) -> bool {
        entry.is_visible_for_flags(include_files, include_dirs)
    }

    /// 現在の filter 設定で entry が見えるかを返す。
    fn is_entry_visible_for_current_filter(&self, entry: &Entry) -> bool {
        let kind = self.find_entry_kind(entry.path()).or(entry.kind);
        match kind {
            Some(kind) => {
                (kind.is_dir && self.include_dirs) || (!kind.is_dir && self.include_files)
            }
            None => self.include_files && self.include_dirs,
        }
    }

    fn rebuild_entry_kind_cache(&mut self) {
        self.cache.entry_kind.rebuild_from_sources(&[
            self.all_entries.as_ref(),
            &self.index.entries,
            self.entries.as_ref(),
        ]);
    }

    // Regression Guard (v0.16.0):
    // DO NOT invoke `set_entry_kind_in_arc_batch` or `Arc::make_mut` here.
    // Iterating and cloning all elements in the 500k+ `entries` arrays for every 512-item batch
    // from the background worker locks up the main frame loop entirely. All kinds are now fetched
    // lazily/reactively via `self.cache.entry_kind` specifically to avoid UI freezes.
    fn apply_entry_kind_updates(&mut self, updates: &[(PathBuf, EntryKind)]) {
        if updates.is_empty() {
            return;
        }
        for (path, kind) in updates {
            self.cache.entry_kind.set(path.clone(), *kind);
        }
    }

    /// entry snapshot から path に対応する kind を探す。
    fn find_entry_kind(&self, path: &Path) -> Option<EntryKind> {
        self.cache.entry_kind.get(path)
    }

    /// 同一 path を持つ entry へ解決済み kind を反映する。
    #[cfg(test)]
    #[allow(dead_code)]
    fn set_entry_kind(&mut self, path: &Path, kind: EntryKind) {
        self.apply_entry_kind_updates(&[(path.to_path_buf(), kind)]);
    }

    #[cfg(test)]
    fn worker_join_timeout() -> Duration {
        Self::WORKER_JOIN_TIMEOUT
    }
}

#[cfg(test)]
mod tests;

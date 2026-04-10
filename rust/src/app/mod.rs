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
use memory_stats::memory_stats;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
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
use coordinator::{
    build_status_line, normalized_compare_key, path_is_within_root, StatusLineContext,
};
use index_coordinator::IndexCoordinator;
use index_worker::spawn_index_worker;
use pipeline_owner::PipelineOwner;
use query_state::QueryState;
use search_coordinator::SearchCoordinator;
use session::{LaunchSettings, SavedTabState, SavedWindowGeometry, TabAccentColor};
use state::{
    BackgroundIndexState, CacheStateBundle, FileListDialogKind, FileListManager, HighlightCacheKey,
    PendingFileListAfterIndex, PendingFileListAncestorConfirmation, PendingFileListConfirmation,
    PendingFileListUseWalkerConfirmation, RequestTabRoutingState, ResultSortMode, RootBrowserState,
    SortMetadata, TabAccentPalette, TabDragState, UpdateCheckFailureState, UpdateManager,
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
use worker_runtime::{WorkerJoinSummary, WorkerRuntime};
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
    pending_restore_refresh: bool,
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
    tabs: Vec<AppTabState>,
    active_tab: usize,
    next_tab_id: u64,
    request_tab_routing: RequestTabRoutingState,
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
            pending_restore_refresh: false,
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
            tabs: Vec::new(),
            active_tab: 0,
            next_tab_id: 1,
            request_tab_routing: RequestTabRoutingState::default(),
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

    /// root 単位で破棄すべき sort metadata cache をまとめて消す。
    fn clear_sort_metadata_cache(&mut self) {
        self.cache.sort_metadata.clear();
    }

    /// 結果ソートに使う時刻属性を上限付き cache へ保存する。
    fn cache_sort_metadata(&mut self, path: PathBuf, metadata: SortMetadata) {
        self.cache
            .sort_metadata
            .insert_bounded(path, metadata, Self::SORT_METADATA_CACHE_MAX);
    }

    /// sort mode ごとに比較対象の timestamp を取り出す。
    fn sort_metadata_value(metadata: SortMetadata, mode: ResultSortMode) -> Option<SystemTime> {
        match mode {
            ResultSortMode::ModifiedDesc | ResultSortMode::ModifiedAsc => metadata.modified,
            ResultSortMode::CreatedDesc | ResultSortMode::CreatedAsc => metadata.created,
            _ => None,
        }
    }

    /// 指定 path の timestamp sort key を cache から取得する。
    fn sort_timestamp_for_path(
        cache: &HashMap<PathBuf, SortMetadata>,
        path: &Path,
        mode: ResultSortMode,
    ) -> Option<SystemTime> {
        cache
            .get(path)
            .copied()
            .and_then(|metadata| Self::sort_metadata_value(metadata, mode))
    }

    /// Name sort 用の比較キーをファイル名優先で正規化する。
    fn path_name_key(path: &Path) -> String {
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
    }

    /// base result snapshot から指定 sort mode の表示順を再構築する。
    fn build_sorted_results_from(
        base_results: &[(PathBuf, f64)],
        mode: ResultSortMode,
        cache: &HashMap<PathBuf, SortMetadata>,
    ) -> Vec<(PathBuf, f64)> {
        let mut items = base_results.iter().cloned().enumerate().collect::<Vec<_>>();
        match mode {
            ResultSortMode::Score => return base_results.to_vec(),
            ResultSortMode::NameAsc | ResultSortMode::NameDesc => {
                let desc = matches!(mode, ResultSortMode::NameDesc);
                items.sort_by(|(idx_a, (path_a, _)), (idx_b, (path_b, _))| {
                    let cmp = Self::path_name_key(path_a)
                        .cmp(&Self::path_name_key(path_b))
                        .then_with(|| {
                            normalized_compare_key(path_a).cmp(&normalized_compare_key(path_b))
                        })
                        .then_with(|| idx_a.cmp(idx_b));
                    if desc {
                        cmp.reverse()
                    } else {
                        cmp
                    }
                });
            }
            ResultSortMode::ModifiedDesc
            | ResultSortMode::ModifiedAsc
            | ResultSortMode::CreatedDesc
            | ResultSortMode::CreatedAsc => {
                let desc = matches!(
                    mode,
                    ResultSortMode::ModifiedDesc | ResultSortMode::CreatedDesc
                );
                items.sort_by(|(idx_a, (path_a, _)), (idx_b, (path_b, _))| {
                    let time_a = Self::sort_timestamp_for_path(cache, path_a, mode);
                    let time_b = Self::sort_timestamp_for_path(cache, path_b, mode);
                    match (time_a, time_b) {
                        (Some(a), Some(b)) => {
                            if desc {
                                b.cmp(&a)
                            } else {
                                a.cmp(&b)
                            }
                        }
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (None, None) => std::cmp::Ordering::Equal,
                    }
                    .then_with(|| Self::path_name_key(path_a).cmp(&Self::path_name_key(path_b)))
                    .then_with(|| {
                        normalized_compare_key(path_a).cmp(&normalized_compare_key(path_b))
                    })
                    .then_with(|| idx_a.cmp(idx_b))
                });
            }
        }
        items.into_iter().map(|(_, entry)| entry).collect()
    }

    /// 現在の base result snapshot から表示用の整列結果を生成する。
    fn build_sorted_results(&self, mode: ResultSortMode) -> Vec<(PathBuf, f64)> {
        Self::build_sorted_results_from(
            &self.base_results,
            mode,
            self.cache.sort_metadata.get_map(),
        )
    }

    /// 結果一覧を差し替えつつ current row と scroll 方針を維持する。
    fn replace_results_snapshot(
        &mut self,
        results: Vec<(PathBuf, f64)>,
        keep_scroll_position: bool,
    ) {
        self.worker_bus.sort.pending_request_id = None;
        self.worker_bus.sort.in_progress = false;
        self.result_sort_mode = ResultSortMode::Score;
        self.base_results = results.clone();
        // Regression guard: search refreshes must keep the cursor on the same row number.
        // Following the previous path here makes the highlight jump when the query changes.
        self.apply_results_with_selection_policy(results, keep_scroll_position, false);
    }

    /// 非 score sort を解除し、必要なら base snapshot を前面へ戻す。
    fn invalidate_result_sort(&mut self, keep_scroll_position: bool) {
        let had_non_score_sort = self.result_sort_mode != ResultSortMode::Score;
        self.worker_bus.sort.pending_request_id = None;
        self.worker_bus.sort.in_progress = false;
        self.result_sort_mode = ResultSortMode::Score;
        if had_non_score_sort && !self.base_results.is_empty() && self.results != self.base_results
        {
            self.apply_results_with_selection_policy(
                self.base_results.clone(),
                keep_scroll_position,
                true,
            );
        } else {
            self.refresh_status_line();
        }
    }

    /// 欠けている metadata だけを sort worker に依頼する。
    fn request_sort_metadata(&mut self, mode: ResultSortMode, missing_paths: Vec<PathBuf>) {
        let request_id = self.worker_bus.sort.next_request_id;
        self.worker_bus.sort.next_request_id =
            self.worker_bus.sort.next_request_id.saturating_add(1);
        self.worker_bus.sort.pending_request_id = Some(request_id);
        self.worker_bus.sort.in_progress = true;
        self.bind_sort_request_to_current_tab(request_id);
        self.refresh_status_line();
        if self
            .worker_bus
            .sort
            .tx
            .send(SortMetadataRequest {
                request_id,
                paths: missing_paths,
                mode,
            })
            .is_err()
        {
            self.worker_bus.sort.pending_request_id = None;
            self.worker_bus.sort.in_progress = false;
            self.set_notice("Sort worker is unavailable");
        }
    }

    /// 現在の sort mode を結果スナップショットへ反映する。
    fn apply_result_sort(&mut self, keep_scroll_position: bool) {
        if self.base_results.is_empty() {
            self.worker_bus.sort.pending_request_id = None;
            self.worker_bus.sort.in_progress = false;
            self.refresh_status_line();
            return;
        }
        if !self.result_sort_mode.uses_metadata() {
            let sorted = self.build_sorted_results(self.result_sort_mode);
            self.worker_bus.sort.pending_request_id = None;
            self.worker_bus.sort.in_progress = false;
            self.apply_results_with_selection_policy(sorted, keep_scroll_position, false);
            return;
        }

        let missing_paths = self
            .base_results
            .iter()
            .map(|(path, _)| path.clone())
            .filter(|path| !self.cache.sort_metadata.contains(path))
            .collect::<Vec<_>>();
        if missing_paths.is_empty() {
            let sorted = self.build_sorted_results(self.result_sort_mode);
            self.worker_bus.sort.pending_request_id = None;
            self.worker_bus.sort.in_progress = false;
            self.apply_results_with_selection_policy(sorted, keep_scroll_position, false);
            return;
        }

        self.request_sort_metadata(self.result_sort_mode, missing_paths);
    }

    /// sort mode を切り替え、即時適用または metadata 解決を始める。
    fn set_result_sort_mode(&mut self, mode: ResultSortMode) {
        self.result_sort_mode = mode;
        self.apply_result_sort(false);
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

    /// 現在の進行状況と notice から status line を再構築する。
    fn refresh_status_line(&mut self) {
        let indexed_count = if self.indexing.in_progress {
            if self.index.entries.is_empty() {
                self.all_entries.len()
            } else {
                self.index.entries.len()
            }
        } else {
            self.all_entries.len()
        };
        let memory = self.memory_usage_text();
        self.status_line = build_status_line(StatusLineContext {
            active_tab: self.active_tab,
            tab_count: self.tabs.len(),
            indexed_count,
            results_len: self.results.len(),
            limit: self.limit,
            pinned_paths_len: self.pinned_paths.len(),
            search_in_progress: self.search.in_progress(),
            indexing_in_progress: self.indexing.in_progress,
            action_in_progress: self.worker_bus.action.in_progress,
            filelist_in_progress: self.filelist_state.in_progress,
            filelist_cancel_requested: self.filelist_state.cancel_requested,
            update_in_progress: self.update_state.in_progress,
            sort_in_progress: self.worker_bus.sort.in_progress,
            history_search_active: self.query_state.history_search_active,
            history_search_results_len: self.query_state.history_search_results.len(),
            query_history_len: self.query_state.query_history.len(),
            notice: &self.notice,
            memory_text: memory,
        });
    }

    /// 定期的にメモリ使用量をサンプリングし表示文字列へ変換する。
    fn memory_usage_text(&mut self) -> Option<String> {
        if self.ui.memory_usage_bytes.is_none()
            || self.ui.last_memory_sample.elapsed() >= Self::MEMORY_SAMPLE_INTERVAL
        {
            self.ui.last_memory_sample = Instant::now();
            self.ui.memory_usage_bytes = memory_stats().map(|stats| stats.physical_mem as u64);
        }
        self.ui
            .memory_usage_bytes
            .map(|bytes| format!("{:.1} MiB", bytes as f64 / 1024.0 / 1024.0))
    }

    /// notice を更新し status line と同期する。
    fn set_notice(&mut self, notice: impl Into<String>) {
        self.notice = notice.into();
        self.refresh_status_line();
    }

    /// notice を消去し status line を再計算する。
    fn clear_notice(&mut self) {
        self.notice.clear();
        self.refresh_status_line();
    }

    /// action worker 実行中の進捗ラベルを返す。
    fn action_progress_label(&self) -> Option<&'static str> {
        if self.worker_bus.action.in_progress {
            Some("Opening...")
        } else {
            None
        }
    }

    /// action worker の応答を現在 tab または背景 tab に反映する。
    fn poll_action_response(&mut self) {
        while let Ok(response) = self.worker_bus.action.rx.try_recv() {
            if self.apply_active_action_response(&response) {
                continue;
            }
            self.apply_background_action_response(response);
        }
    }

    /// sort worker の応答を cache と tab state へ適用する。
    fn poll_sort_response(&mut self) {
        while let Ok(response) = self.worker_bus.sort.rx.try_recv() {
            for (path, metadata) in &response.entries {
                self.cache_sort_metadata(path.clone(), *metadata);
            }

            if self.apply_active_sort_response(&response) {
                continue;
            }
            self.apply_background_sort_response(response);
        }
    }

    /// ページ単位のカーソル移動を行う。
    fn move_page(&mut self, direction: isize) {
        self.move_row(direction.saturating_mul(Self::PAGE_MOVE_ROWS));
    }

    /// 先頭行へ移動し preview を更新する。
    fn move_to_first_row(&mut self) {
        self.commit_query_history_if_needed(true);
        if self.results.is_empty() {
            return;
        }
        self.current_row = Some(0);
        self.ui.scroll_to_current = true;
        self.request_preview_for_current();
        self.refresh_status_line();
    }

    /// 末尾行へ移動し preview を更新する。
    fn move_to_last_row(&mut self) {
        self.commit_query_history_if_needed(true);
        if self.results.is_empty() {
            return;
        }
        self.current_row = Some(self.results.len().saturating_sub(1));
        self.ui.scroll_to_current = true;
        self.request_preview_for_current();
        self.refresh_status_line();
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

    /// kind 未確定 entry の遅延解決が必要な filter 状態かを返す。
    fn kind_resolution_needed_for_filters(&self) -> bool {
        !self.include_files || !self.include_dirs
    }

    /// kind 解決キューと epoch を初期化し直す。
    fn reset_kind_resolution_state(&mut self) {
        self.indexing.pending_kind_paths.clear();
        self.indexing.pending_kind_paths_set.clear();
        self.indexing.in_flight_kind_paths.clear();
        self.indexing.kind_resolution_in_progress = false;
        self.indexing.kind_resolution_epoch = self.indexing.kind_resolution_epoch.saturating_add(1);
    }

    /// 表示中または incremental index 中の entry から kind 未解決 path を拾う。
    fn queue_unknown_kind_paths_for_active_entries(&mut self) {
        if !self.kind_resolution_needed_for_filters() {
            return;
        }
        let source: Vec<PathBuf> = if self.indexing.in_progress && !self.index.entries.is_empty() {
            self.index
                .entries
                .iter()
                .map(|entry| entry.path.clone())
                .collect()
        } else {
            self.all_entries
                .iter()
                .map(|entry| entry.path.clone())
                .collect()
        };
        self.queue_unknown_kind_paths(&source);
    }

    /// walker 完了後の全 entry から kind 未解決 path を拾う。
    fn queue_unknown_kind_paths_for_completed_walker_entries(&mut self) {
        for i in 0..self.all_entries.len() {
            let path = &self.all_entries[i].path;
            if self.find_entry_kind(path).is_none() {
                if !self.indexing.pending_kind_paths_set.contains(path)
                    && !self.indexing.in_flight_kind_paths.contains(path)
                {
                    let p = path.clone();
                    self.indexing.pending_kind_paths_set.insert(p.clone());
                    self.indexing.pending_kind_paths.push_back(p);
                }
            }
        }
    }

    /// 指定 path 群から kind 未解決のものだけを queue へ積む。
    fn queue_unknown_kind_paths(&mut self, source: &[PathBuf]) {
        for path in source {
            if self.find_entry_kind(path).is_none() {
                self.queue_kind_resolution(path.clone());
            }
        }
    }

    /// kind 解決キューへ重複なしで path を追加する。
    fn queue_kind_resolution(&mut self, path: PathBuf) {
        if self.indexing.pending_kind_paths_set.contains(&path)
            || self.indexing.in_flight_kind_paths.contains(&path)
        {
            return;
        }
        self.indexing.pending_kind_paths_set.insert(path.clone());
        self.indexing.pending_kind_paths.push_back(path);
    }

    /// kind resolver worker へ frame 予算内で request を流す。
    fn pump_kind_resolution_requests(&mut self) {
        const MAX_DISPATCH_PER_FRAME: usize = 128;
        let mut dispatched = 0usize;
        while dispatched < MAX_DISPATCH_PER_FRAME {
            let Some(path) = self.indexing.pending_kind_paths.pop_front() else {
                break;
            };
            self.indexing.pending_kind_paths_set.remove(&path);
            let req = KindResolveRequest {
                epoch: self.indexing.kind_resolution_epoch,
                path: path.clone(),
            };
            if self.worker_bus.kind.tx.send(req).is_err() {
                break;
            }
            self.indexing.in_flight_kind_paths.insert(path);
            dispatched = dispatched.saturating_add(1);
        }
        self.indexing.kind_resolution_in_progress = !self.indexing.pending_kind_paths.is_empty()
            || !self.indexing.in_flight_kind_paths.is_empty();
    }

    /// kind resolver 応答を吸収し filter/preview を必要最小限で更新する。
    fn poll_kind_response(&mut self) {
        const MAX_MESSAGES_PER_FRAME: usize = 512;
        let mut processed = 0usize;
        let mut resolved_any = false;
        let mut resolved_current_row = false;
        let mut resolved_updates: Vec<(PathBuf, EntryKind)> = Vec::new();

        while let Ok(response) = self.worker_bus.kind.rx.try_recv() {
            if response.epoch != self.indexing.kind_resolution_epoch {
                continue;
            }
            self.indexing.in_flight_kind_paths.remove(&response.path);
            if let Some(kind) = response.kind {
                if self.current_row.is_some_and(|row| {
                    self.results
                        .get(row)
                        .is_some_and(|(path, _)| *path == response.path)
                }) {
                    resolved_current_row = true;
                }
                resolved_updates.push((response.path.clone(), kind));
                resolved_any = true;
            }
            processed = processed.saturating_add(1);
            if processed >= MAX_MESSAGES_PER_FRAME {
                break;
            }
        }

        if !resolved_updates.is_empty() {
            self.apply_entry_kind_updates(&resolved_updates);
        }

        self.indexing.kind_resolution_in_progress = !self.indexing.pending_kind_paths.is_empty()
            || !self.indexing.in_flight_kind_paths.is_empty();

        if resolved_any && (!self.include_files || !self.include_dirs) {
            self.apply_entry_filters(true);
        }
        if resolved_current_row && self.ui.show_preview {
            self.request_preview_for_current();
        }
    }

    /// 結果一覧内の current row を相対移動する。
    fn move_row(&mut self, delta: isize) {
        self.commit_query_history_if_needed(true);
        if self.results.is_empty() {
            return;
        }
        let row = self.current_row.unwrap_or(0) as isize;
        let next = (row + delta).clamp(0, self.results.len() as isize - 1) as usize;
        self.current_row = Some(next);
        self.ui.scroll_to_current = true;
        self.request_preview_for_current();
        self.refresh_status_line();
    }

    /// current row を pinned selection に追加または解除する。
    fn toggle_pin_current(&mut self) {
        if let Some(row) = self.current_row {
            if let Some((path, _)) = self.results.get(row) {
                if self.pinned_paths.contains(path) {
                    self.pinned_paths.remove(path);
                } else {
                    self.pinned_paths.insert(path.clone());
                }
                self.refresh_status_line();
            }
        }
    }

    /// pinned selection 優先で action 対象 path を列挙する。
    fn selected_paths(&self) -> Vec<PathBuf> {
        if !self.pinned_paths.is_empty() {
            let mut out: Vec<PathBuf> = self.pinned_paths.iter().cloned().collect();
            out.sort();
            return out;
        }
        self.current_row
            .and_then(|row| self.results.get(row).map(|(p, _)| vec![p.clone()]))
            .unwrap_or_default()
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

    /// 既定動作で選択 path を実行またはオープンする。
    fn execute_selected(&mut self) {
        self.execute_selected_with_options(false);
    }

    /// Enter 系アクション用に file は親フォルダオープンへ切り替えられる実行入口。
    fn execute_selected_for_activation(&mut self, open_parent_for_files: bool) {
        self.execute_selected_with_options(open_parent_for_files);
    }

    /// 選択項目の格納フォルダを開く。
    fn execute_selected_open_folder(&mut self) {
        self.execute_selected_for_activation(true);
    }

    /// worker dispatch と root 外 path ガードを含めて action を起動する。
    fn execute_selected_with_options(&mut self, open_parent_for_files: bool) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }
        if let Some(blocked) = self.first_action_path_outside_root(&paths) {
            self.worker_bus.action.pending_request_id = None;
            self.worker_bus.action.in_progress = false;
            self.set_notice(format!(
                "Action blocked: path is outside current root: {}",
                normalize_path_for_display(&blocked)
            ));
            return;
        }

        let request_id = self.worker_bus.action.next_request_id;
        self.worker_bus.action.next_request_id =
            self.worker_bus.action.next_request_id.saturating_add(1);
        self.worker_bus.action.pending_request_id = Some(request_id);
        self.worker_bus.action.in_progress = true;
        self.bind_action_request_to_current_tab(request_id);

        if paths.len() == 1 {
            if open_parent_for_files {
                self.set_notice(format!(
                    "Action: open containing folder for {}",
                    normalize_path_for_display(&paths[0])
                ));
            } else {
                self.set_notice(format!("Action: {}", normalize_path_for_display(&paths[0])));
            }
        } else if open_parent_for_files {
            self.set_notice(format!(
                "Action: launched {} containing folder items",
                paths.len()
            ));
        } else {
            self.set_notice(format!("Action: launched {} items", paths.len()));
        }

        let req = ActionRequest {
            request_id,
            paths,
            open_parent_for_files,
        };
        if self.worker_bus.action.tx.send(req).is_err() {
            self.worker_bus.action.pending_request_id = None;
            self.worker_bus.action.in_progress = false;
            self.set_notice("Action worker is unavailable");
        }
    }

    /// 選択 path を clipboard 用文字列へ変換して UI 出力へ流す。
    fn copy_selected_paths(&mut self, ctx: &egui::Context) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }
        let text = Self::clipboard_paths_text(&paths);
        ctx.output_mut(|o| o.copied_text = text);
        if paths.len() == 1 {
            self.set_notice(format!(
                "Copied path: {}",
                normalize_path_for_display(&paths[0])
            ));
        } else {
            self.set_notice(format!("Copied {} paths to clipboard", paths.len()));
        }
    }

    /// clipboard 向けの複数 path 文字列を構築する。
    fn clipboard_paths_text(paths: &[PathBuf]) -> String {
        paths
            .iter()
            .map(|p| normalize_path_for_display(p))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// pinned selection を全解除する。
    fn clear_pinned(&mut self) {
        self.pinned_paths.clear();
        self.set_notice("Cleared pinned selections");
    }

    /// query と選択状態を初期化し一覧表示へ戻す。
    fn clear_query_and_selection(&mut self) {
        self.query_state.query.clear();
        self.reset_query_history_navigation();
        self.reset_history_search_state();
        self.query_state.query_history_dirty_since = None;
        self.pinned_paths.clear();
        // Keep the list visible after Esc/Ctrl+G by restoring the default row selection.
        self.current_row = Some(0);
        self.preview.clear();
        self.update_results();
        self.ui.focus_query_requested = true;
        self.set_notice("Cleared selection and query");
    }

    /// 現在の index source を status 向け文言へ整形する。
    fn source_text(&self) -> String {
        match &self.index.source {
            IndexSource::FileList(path) => format!(
                "Source: FileList ({})",
                path.file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("FileList.txt")
            ),
            IndexSource::Walker => "Source: Walker".to_string(),
            IndexSource::None => "Source: None".to_string(),
        }
    }

    #[cfg(test)]
    fn worker_join_timeout() -> Duration {
        Self::WORKER_JOIN_TIMEOUT
    }

    /// worker request sender を dummy channel へ差し替えて shutdown を開始する。
    fn disconnect_worker_channels(&mut self) {
        let (dummy_search_tx, _) = mpsc::channel::<SearchRequest>();
        let (dummy_preview_tx, _) = mpsc::channel::<PreviewRequest>();
        let (dummy_action_tx, _) = mpsc::channel::<ActionRequest>();
        let (dummy_sort_tx, _) = mpsc::channel::<SortMetadataRequest>();
        let (dummy_kind_tx, _) = mpsc::channel::<KindResolveRequest>();
        let (dummy_filelist_tx, _) = mpsc::channel::<FileListRequest>();
        let (dummy_update_tx, _) = mpsc::channel::<UpdateRequest>();
        let (dummy_index_tx, _) = mpsc::channel::<IndexRequest>();
        let old_search_tx = std::mem::replace(&mut self.search.tx, dummy_search_tx);
        let old_preview_tx = std::mem::replace(&mut self.worker_bus.preview.tx, dummy_preview_tx);
        let old_action_tx = std::mem::replace(&mut self.worker_bus.action.tx, dummy_action_tx);
        let old_sort_tx = std::mem::replace(&mut self.worker_bus.sort.tx, dummy_sort_tx);
        let old_kind_tx = std::mem::replace(&mut self.worker_bus.kind.tx, dummy_kind_tx);
        let old_filelist_tx =
            std::mem::replace(&mut self.worker_bus.filelist.tx, dummy_filelist_tx);
        let old_update_tx = std::mem::replace(&mut self.worker_bus.update.tx, dummy_update_tx);
        let old_index_tx = std::mem::replace(&mut self.indexing.tx, dummy_index_tx);
        drop(old_search_tx);
        drop(old_preview_tx);
        drop(old_action_tx);
        drop(old_sort_tx);
        drop(old_kind_tx);
        drop(old_filelist_tx);
        drop(old_update_tx);
        drop(old_index_tx);
    }

    /// worker 群へ shutdown を通知し、短い timeout で join を待つ。
    fn shutdown_workers_with_timeout(
        &mut self,
        timeout: Duration,
        phase: &str,
    ) -> Option<WorkerJoinSummary> {
        let runtime = self.worker_runtime.as_ref()?;
        runtime.request_shutdown();
        self.disconnect_worker_channels();
        let runtime = self.worker_runtime.take()?;
        let summary = runtime.join_all_with_timeout(timeout);
        if summary.joined < summary.total {
            let pending = if summary.pending.is_empty() {
                "unknown".to_string()
            } else {
                summary.pending.join(", ")
            };
            eprintln!(
                "Worker shutdown timeout during {phase}: joined {}/{} threads within {:?}; pending: {pending}",
                summary.joined, summary.total, timeout
            );
        }
        Some(summary)
    }

    fn request_viewport_close_if_needed(&mut self, ctx: &egui::Context) -> bool {
        if process_shutdown_requested() {
            self.set_notice("Shutdown requested by signal");
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return true;
        }
        if self.update_state.close_requested_for_install {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return true;
        }
        false
    }

    fn poll_runtime_events(&mut self) {
        self.poll_index_response();
        self.poll_search_response();
        self.poll_action_response();
        self.poll_sort_response();
        self.poll_preview_response();
        self.poll_kind_response();
        self.pump_kind_resolution_requests();
        self.poll_filelist_response();
        self.poll_update_response();
    }

    fn schedule_frame_repaint(&mut self, ctx: &egui::Context) {
        let memory_elapsed = self.ui.last_memory_sample.elapsed();
        if memory_elapsed >= Self::MEMORY_SAMPLE_INTERVAL {
            self.refresh_status_line();
        } else {
            ctx.request_repaint_after(Self::MEMORY_SAMPLE_INTERVAL - memory_elapsed);
        }
        if self.search.in_progress()
            || self.indexing.in_progress
            || self.worker_bus.preview.in_progress
            || self.worker_bus.action.in_progress
            || self.worker_bus.sort.in_progress
            || self.indexing.kind_resolution_in_progress
            || self.filelist_state.in_progress
            || self.update_state.in_progress
            || self.any_tab_async_in_progress()
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
    }

    fn run_ui_frame(&mut self, ctx: &egui::Context) {
        self.capture_window_geometry(ctx);
        self.apply_stable_window_geometry(false);
        // Handle app shortcuts before widget rendering so Tab is not consumed by egui focus traversal.
        self.handle_shortcuts(ctx);

        self.render_top_panel(ctx);
        self.render_status_panel(ctx);
        self.render_filelist_dialogs(ctx);
        self.render_update_dialog(ctx);
        self.render_central_panel(ctx);
        self.dispatch_render_commands(ctx);
        self.maybe_save_ui_state(false);
    }

    fn run_update_cycle(&mut self, ctx: &egui::Context) -> bool {
        self.poll_runtime_events();
        if self.request_viewport_close_if_needed(ctx) {
            return false;
        }
        self.commit_query_history_if_needed(false);
        self.schedule_frame_repaint(ctx);
        self.run_ui_frame(ctx);
        true
    }

    fn persist_state_and_shutdown(&mut self, phase: &str) {
        self.apply_stable_window_geometry(true);
        self.ui.ui_state_dirty = true;
        self.maybe_save_ui_state(true);
        let _ = self.shutdown_workers_with_timeout(Self::WORKER_JOIN_TIMEOUT, phase);
    }
}

impl eframe::App for FlistWalkerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.run_update_cycle(ctx) {
            return;
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.persist_state_and_shutdown("app exit");
    }
}

impl Drop for FlistWalkerApp {
    fn drop(&mut self) {
        self.persist_state_and_shutdown("drop fallback");
    }
}

#[cfg(test)]
mod tests;

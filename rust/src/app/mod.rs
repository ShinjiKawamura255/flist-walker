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
mod filelist;
mod input;
mod pipeline;
mod render;
mod session;
mod state;
mod tab_state;
mod tabs;
mod update;
mod workers;

use cache::*;
#[allow(unused_imports)]
use input::*;
#[allow(unused_imports)]
use render::*;
use session::*;
use state::*;
use tab_state::*;
use workers::*;

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

pub struct FlistWalkerApp {
    root: PathBuf,
    limit: usize,
    query: String,
    query_history: VecDeque<String>,
    query_history_cursor: Option<usize>,
    query_history_draft: Option<String>,
    query_history_dirty_since: Option<Instant>,
    history_search_active: bool,
    history_search_query: String,
    history_search_original_query: String,
    history_search_results: Vec<String>,
    history_search_current: Option<usize>,
    pending_restore_refresh: bool,
    use_filelist: bool,
    use_regex: bool,
    ignore_case: bool,
    include_files: bool,
    include_dirs: bool,
    index: IndexBuildResult,
    all_entries: Arc<Vec<PathBuf>>,
    entries: Arc<Vec<PathBuf>>,
    entry_kinds: HashMap<PathBuf, EntryKind>,
    base_results: Vec<(PathBuf, f64)>,
    results: Vec<(PathBuf, f64)>,
    result_sort_mode: ResultSortMode,
    pinned_paths: HashSet<PathBuf>,
    current_row: Option<usize>,
    preview: String,
    notice: String,
    status_line: String,
    kill_buffer: String,
    search_tx: Sender<SearchRequest>,
    search_rx: Receiver<SearchResponse>,
    preview_tx: Sender<PreviewRequest>,
    preview_rx: Receiver<PreviewResponse>,
    action_tx: Sender<ActionRequest>,
    action_rx: Receiver<ActionResponse>,
    sort_tx: Sender<SortMetadataRequest>,
    sort_rx: Receiver<SortMetadataResponse>,
    kind_tx: Sender<KindResolveRequest>,
    kind_rx: Receiver<KindResolveResponse>,
    filelist_tx: Sender<FileListRequest>,
    filelist_rx: Receiver<FileListResponse>,
    update_tx: Sender<UpdateRequest>,
    update_rx: Receiver<UpdateResponse>,
    index_tx: Sender<IndexRequest>,
    index_rx: Receiver<IndexResponse>,
    next_request_id: u64,
    pending_request_id: Option<u64>,
    next_index_request_id: u64,
    pending_index_request_id: Option<u64>,
    next_preview_request_id: u64,
    pending_preview_request_id: Option<u64>,
    next_action_request_id: u64,
    pending_action_request_id: Option<u64>,
    next_sort_request_id: u64,
    pending_sort_request_id: Option<u64>,
    latest_index_request_ids: Arc<Mutex<HashMap<u64, u64>>>,
    pending_index_queue: VecDeque<IndexRequest>,
    index_inflight_requests: HashSet<u64>,
    search_in_progress: bool,
    index_in_progress: bool,
    preview_in_progress: bool,
    action_in_progress: bool,
    sort_in_progress: bool,
    kind_resolution_in_progress: bool,
    pending_copy_shortcut: bool,
    #[cfg(test)]
    browse_dialog_result: Option<Result<Option<PathBuf>, String>>,
    root_dropdown_highlight: Option<usize>,
    scroll_to_current: bool,
    preview_resize_in_progress: bool,
    focus_query_requested: bool,
    unfocus_query_requested: bool,
    saved_roots: Vec<PathBuf>,
    default_root: Option<PathBuf>,
    show_preview: bool,
    preview_panel_width: f32,
    window_geometry: Option<SavedWindowGeometry>,
    pending_window_geometry: Option<SavedWindowGeometry>,
    last_window_geometry_change: Instant,
    ui_state_dirty: bool,
    last_ui_state_save: Instant,
    last_memory_sample: Instant,
    memory_usage_bytes: Option<u64>,
    ime_composition_active: bool,
    prev_space_down: bool,
    query_input_id: egui::Id,
    tab_drag_state: Option<TabDragState>,
    preview_cache: PreviewCacheState,
    highlight_cache: HighlightCacheState,
    sort_metadata_cache: SortMetadataCacheState,
    incremental_filtered_entries: Vec<PathBuf>,
    pending_index_entries: VecDeque<IndexEntry>,
    pending_index_entries_request_id: Option<u64>,
    pending_kind_paths: VecDeque<PathBuf>,
    pending_kind_paths_set: HashSet<PathBuf>,
    in_flight_kind_paths: HashSet<PathBuf>,
    kind_resolution_epoch: u64,
    last_incremental_results_refresh: Instant,
    last_search_snapshot_len: usize,
    search_resume_pending: bool,
    search_rerun_pending: bool,
    tabs: Vec<AppTabState>,
    active_tab: usize,
    next_tab_id: u64,
    index_request_tabs: HashMap<u64, u64>,
    background_index_states: HashMap<u64, BackgroundIndexState>,
    search_request_tabs: HashMap<u64, u64>,
    preview_request_tabs: HashMap<u64, u64>,
    action_request_tabs: HashMap<u64, u64>,
    sort_request_tabs: HashMap<u64, u64>,
    filelist_state: FileListWorkflowState,
    update_state: UpdateState,
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

    pub fn trace_window_event(event: &str, details: &str) {
        Self::append_window_trace(event, details);
    }

    pub fn new(root: PathBuf, limit: usize, query: String) -> Self {
        let launch = LaunchSettings {
            show_preview: true,
            preview_panel_width: Self::DEFAULT_PREVIEW_PANEL_WIDTH,
            ..LaunchSettings::default()
        };
        Self::new_with_launch(root, limit, query, launch, None)
    }

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

    fn new_with_launch(
        root: PathBuf,
        limit: usize,
        query: String,
        launch: LaunchSettings,
        restore_session: Option<(Vec<SavedTabState>, usize)>,
    ) -> Self {
        let bootstrap = Self::bootstrap_workers();
        let seed = Self::launch_seed(root, limit, query, &launch);
        let mut app = Self {
            root: seed.root,
            limit: seed.limit,
            query: seed.query,
            query_history: seed.query_history,
            query_history_cursor: None,
            query_history_draft: None,
            query_history_dirty_since: None,
            history_search_active: false,
            history_search_query: String::new(),
            history_search_original_query: String::new(),
            history_search_results: Vec::new(),
            history_search_current: None,
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
            entry_kinds: HashMap::new(),
            base_results: Vec::new(),
            results: Vec::new(),
            result_sort_mode: ResultSortMode::Score,
            pinned_paths: HashSet::new(),
            current_row: Some(0),
            preview: String::new(),
            notice: String::new(),
            status_line: "Initializing...".to_string(),
            kill_buffer: String::new(),
            search_tx: bootstrap.search_tx,
            search_rx: bootstrap.search_rx,
            preview_tx: bootstrap.preview_tx,
            preview_rx: bootstrap.preview_rx,
            action_tx: bootstrap.action_tx,
            action_rx: bootstrap.action_rx,
            sort_tx: bootstrap.sort_tx,
            sort_rx: bootstrap.sort_rx,
            kind_tx: bootstrap.kind_tx,
            kind_rx: bootstrap.kind_rx,
            filelist_tx: bootstrap.filelist_tx,
            filelist_rx: bootstrap.filelist_rx,
            update_tx: bootstrap.update_tx,
            update_rx: bootstrap.update_rx,
            index_tx: bootstrap.index_tx,
            index_rx: bootstrap.index_rx,
            next_request_id: 1,
            pending_request_id: None,
            next_index_request_id: 1,
            pending_index_request_id: None,
            next_preview_request_id: 1,
            pending_preview_request_id: None,
            next_action_request_id: 1,
            pending_action_request_id: None,
            next_sort_request_id: 1,
            pending_sort_request_id: None,
            latest_index_request_ids: bootstrap.latest_index_request_ids,
            pending_index_queue: VecDeque::new(),
            index_inflight_requests: HashSet::new(),
            search_in_progress: false,
            index_in_progress: false,
            preview_in_progress: false,
            action_in_progress: false,
            sort_in_progress: false,
            kind_resolution_in_progress: false,
            pending_copy_shortcut: false,
            #[cfg(test)]
            browse_dialog_result: None,
            root_dropdown_highlight: None,
            scroll_to_current: true,
            preview_resize_in_progress: false,
            focus_query_requested: true,
            unfocus_query_requested: false,
            saved_roots: seed.saved_roots,
            default_root: seed.default_root,
            show_preview: seed.show_preview,
            preview_panel_width: seed.preview_panel_width,
            window_geometry: None,
            pending_window_geometry: None,
            last_window_geometry_change: Instant::now(),
            ui_state_dirty: false,
            last_ui_state_save: Instant::now(),
            last_memory_sample: Instant::now(),
            memory_usage_bytes: None,
            ime_composition_active: false,
            prev_space_down: false,
            query_input_id: egui::Id::new("query-input"),
            tab_drag_state: None,
            preview_cache: PreviewCacheState::default(),
            highlight_cache: HighlightCacheState {
                scope_ignore_case: true,
                ..HighlightCacheState::default()
            },
            sort_metadata_cache: SortMetadataCacheState::default(),
            incremental_filtered_entries: Vec::new(),
            pending_index_entries: VecDeque::new(),
            pending_index_entries_request_id: None,
            pending_kind_paths: VecDeque::new(),
            pending_kind_paths_set: HashSet::new(),
            in_flight_kind_paths: HashSet::new(),
            kind_resolution_epoch: 1,
            last_incremental_results_refresh: Instant::now(),
            last_search_snapshot_len: 0,
            search_resume_pending: false,
            search_rerun_pending: false,
            tabs: Vec::new(),
            active_tab: 0,
            next_tab_id: 1,
            index_request_tabs: HashMap::new(),
            background_index_states: HashMap::new(),
            search_request_tabs: HashMap::new(),
            preview_request_tabs: HashMap::new(),
            action_request_tabs: HashMap::new(),
            sort_request_tabs: HashMap::new(),
            filelist_state: FileListWorkflowState::default(),
            update_state: seed.update_state,
            worker_runtime: Some(bootstrap.worker_runtime),
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

    fn clear_sort_metadata_cache(&mut self) {
        self.sort_metadata_cache.entries.clear();
        self.sort_metadata_cache.order.clear();
    }

    fn cache_sort_metadata(&mut self, path: PathBuf, metadata: SortMetadata) {
        if !self.sort_metadata_cache.entries.contains_key(&path) {
            self.sort_metadata_cache.order.push_back(path.clone());
        }
        self.sort_metadata_cache
            .entries
            .insert(path.clone(), metadata);
        while self.sort_metadata_cache.order.len() > Self::SORT_METADATA_CACHE_MAX {
            if let Some(oldest) = self.sort_metadata_cache.order.pop_front() {
                self.sort_metadata_cache.entries.remove(&oldest);
            }
        }
        if !self.sort_metadata_cache.entries.contains_key(&path) {
            self.sort_metadata_cache
                .order
                .retain(|entry| entry != &path);
        }
    }

    fn sort_metadata_value(metadata: SortMetadata, mode: ResultSortMode) -> Option<SystemTime> {
        match mode {
            ResultSortMode::ModifiedDesc | ResultSortMode::ModifiedAsc => metadata.modified,
            ResultSortMode::CreatedDesc | ResultSortMode::CreatedAsc => metadata.created,
            _ => None,
        }
    }

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

    fn path_name_key(path: &Path) -> String {
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
    }

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
                            Self::normalized_compare_key(path_a)
                                .cmp(&Self::normalized_compare_key(path_b))
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
                        Self::normalized_compare_key(path_a)
                            .cmp(&Self::normalized_compare_key(path_b))
                    })
                    .then_with(|| idx_a.cmp(idx_b))
                });
            }
        }
        items.into_iter().map(|(_, entry)| entry).collect()
    }

    fn build_sorted_results(&self, mode: ResultSortMode) -> Vec<(PathBuf, f64)> {
        Self::build_sorted_results_from(&self.base_results, mode, &self.sort_metadata_cache.entries)
    }

    fn replace_results_snapshot(
        &mut self,
        results: Vec<(PathBuf, f64)>,
        keep_scroll_position: bool,
    ) {
        self.pending_sort_request_id = None;
        self.sort_in_progress = false;
        self.result_sort_mode = ResultSortMode::Score;
        self.base_results = results.clone();
        // Regression guard: search refreshes must keep the cursor on the same row number.
        // Following the previous path here makes the highlight jump when the query changes.
        self.apply_results_with_selection_policy(results, keep_scroll_position, false);
    }

    fn invalidate_result_sort(&mut self, keep_scroll_position: bool) {
        let had_non_score_sort = self.result_sort_mode != ResultSortMode::Score;
        self.pending_sort_request_id = None;
        self.sort_in_progress = false;
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

    fn request_sort_metadata(&mut self, mode: ResultSortMode, missing_paths: Vec<PathBuf>) {
        let request_id = self.next_sort_request_id;
        self.next_sort_request_id = self.next_sort_request_id.saturating_add(1);
        self.pending_sort_request_id = Some(request_id);
        self.sort_in_progress = true;
        if let Some(tab_id) = self.current_tab_id() {
            self.sort_request_tabs.insert(request_id, tab_id);
        }
        self.refresh_status_line();
        if self
            .sort_tx
            .send(SortMetadataRequest {
                request_id,
                paths: missing_paths,
                mode,
            })
            .is_err()
        {
            self.pending_sort_request_id = None;
            self.sort_in_progress = false;
            self.set_notice("Sort worker is unavailable");
        }
    }

    fn apply_result_sort(&mut self, keep_scroll_position: bool) {
        if self.base_results.is_empty() {
            self.pending_sort_request_id = None;
            self.sort_in_progress = false;
            self.refresh_status_line();
            return;
        }
        if !self.result_sort_mode.uses_metadata() {
            let sorted = self.build_sorted_results(self.result_sort_mode);
            self.pending_sort_request_id = None;
            self.sort_in_progress = false;
            self.apply_results_with_selection_policy(sorted, keep_scroll_position, false);
            return;
        }

        let missing_paths = self
            .base_results
            .iter()
            .map(|(path, _)| path.clone())
            .filter(|path| !self.sort_metadata_cache.entries.contains_key(path))
            .collect::<Vec<_>>();
        if missing_paths.is_empty() {
            let sorted = self.build_sorted_results(self.result_sort_mode);
            self.pending_sort_request_id = None;
            self.sort_in_progress = false;
            self.apply_results_with_selection_policy(sorted, keep_scroll_position, false);
            return;
        }

        self.request_sort_metadata(self.result_sort_mode, missing_paths);
    }

    fn set_result_sort_mode(&mut self, mode: ResultSortMode) {
        self.result_sort_mode = mode;
        self.apply_result_sort(false);
    }

    fn normalized_compare_key(path: &Path) -> String {
        let mut key = normalize_windows_path_buf(path.to_path_buf())
            .to_string_lossy()
            .replace('\\', "/");
        while key.len() > 1 && key.ends_with('/') {
            key.pop();
        }
        #[cfg(windows)]
        {
            key.make_ascii_lowercase();
        }
        key
    }

    fn path_is_within_root(root: &Path, path: &Path) -> bool {
        let root_key = Self::normalized_compare_key(root);
        let path_key = Self::normalized_compare_key(path);
        if path_key == root_key
            || path_key
                .strip_prefix(&root_key)
                .is_some_and(|suffix| suffix.starts_with('/'))
        {
            return true;
        }

        let canonical_root = root.canonicalize().ok();
        let canonical_path = path.canonicalize().ok();
        match (canonical_root, canonical_path) {
            (Some(canonical_root), Some(canonical_path)) => {
                let root_key = Self::normalized_compare_key(&canonical_root);
                let path_key = Self::normalized_compare_key(&canonical_path);
                path_key == root_key
                    || path_key
                        .strip_prefix(&root_key)
                        .is_some_and(|suffix| suffix.starts_with('/'))
            }
            _ => false,
        }
    }

    fn first_action_path_outside_root(&self, paths: &[PathBuf]) -> Option<PathBuf> {
        paths
            .iter()
            .find(|path| !Self::path_is_within_root(&self.root, path))
            .cloned()
    }

    fn root_display_text(&self) -> String {
        normalize_windows_path_buf(self.root.clone())
            .to_string_lossy()
            .to_string()
    }

    fn clear_root_scoped_entry_state(&mut self) {
        self.index.entries.clear();
        self.index.entries.shrink_to_fit();
        self.index.source = IndexSource::None;
        self.all_entries = Arc::new(Vec::new());
        self.entries = Arc::new(Vec::new());
        self.entry_kinds.clear();
        self.entry_kinds.shrink_to_fit();
        self.base_results.clear();
        self.base_results.shrink_to_fit();
        self.results.clear();
        self.results.shrink_to_fit();
        self.incremental_filtered_entries.clear();
        self.incremental_filtered_entries.shrink_to_fit();
        self.pending_sort_request_id = None;
        self.sort_in_progress = false;
        self.result_sort_mode = ResultSortMode::Score;
        self.clear_sort_metadata_cache();
        self.last_search_snapshot_len = 0;
    }

    fn apply_root_change(&mut self, new_root: PathBuf) {
        let normalized = normalize_windows_path_buf(new_root);
        if Self::path_key(&normalized) == Self::path_key(&self.root) {
            return;
        }

        self.root = normalized;
        self.reset_query_history_navigation();
        self.query_history_dirty_since = None;
        self.reset_history_search_state();
        // Avoid launching/copying stale selections from the previous root.
        self.pinned_paths.clear();
        self.current_row = None;
        self.preview.clear();
        self.preview_in_progress = false;
        self.pending_preview_request_id = None;
        self.clear_root_scoped_entry_state();
        self.sync_active_tab_state();
        self.mark_ui_state_dirty();
        self.cancel_stale_pending_filelist_confirmation();
        self.cancel_stale_pending_filelist_ancestor_confirmation();
        self.cancel_stale_pending_filelist_use_walker_confirmation();
        self.request_index_refresh();
        self.set_notice(format!("Root changed: {}", self.root_display_text()));
    }

    fn browse_for_root(&mut self) {
        let dialog_root = normalize_windows_path_buf(self.root.clone());
        match self.select_root_via_dialog(&dialog_root) {
            Ok(Some(dir)) => self.apply_root_change(dir),
            Ok(None) => {}
            Err(err) => self.set_notice(format!("Browse failed: {}", err)),
        }
    }

    fn browse_for_root_in_new_tab(&mut self) {
        let dialog_root = normalize_windows_path_buf(self.root.clone());
        match self.select_root_via_dialog(&dialog_root) {
            Ok(Some(dir)) => {
                self.create_new_tab();
                self.apply_root_change(dir);
            }
            Ok(None) => {}
            Err(err) => self.set_notice(format!("Browse failed: {}", err)),
        }
    }

    #[cfg(test)]
    fn select_root_via_dialog(&mut self, _dialog_root: &Path) -> Result<Option<PathBuf>, String> {
        self.browse_dialog_result
            .take()
            .unwrap_or(Ok(None))
    }

    #[cfg(not(test))]
    fn select_root_via_dialog(&mut self, dialog_root: &Path) -> Result<Option<PathBuf>, String> {
        native_dialog::FileDialog::new()
            .set_location(dialog_root)
            .show_open_single_dir()
            .map_err(|err| err.to_string())
    }

    fn root_selector_popup_id() -> egui::Id {
        egui::Id::new(Self::ROOT_SELECTOR_POPUP_ID)
    }

    fn is_root_dropdown_open(&self, ctx: &egui::Context) -> bool {
        ctx.memory(|mem| mem.is_popup_open(Self::root_selector_popup_id()))
    }

    fn current_root_dropdown_index(&self) -> Option<usize> {
        let current_key = Self::path_key(&self.root);
        self.saved_roots
            .iter()
            .position(|path| Self::path_key(path) == current_key)
    }

    fn sync_root_dropdown_highlight(&mut self) {
        let max_index = self.saved_roots.len().checked_sub(1);
        self.root_dropdown_highlight = match (self.root_dropdown_highlight, max_index) {
            (_, None) => None,
            (Some(index), Some(max)) => Some(index.min(max)),
            (None, Some(_)) => self.current_root_dropdown_index().or(Some(0)),
        };
    }

    fn open_root_dropdown(&mut self, ctx: &egui::Context) {
        self.sync_root_dropdown_highlight();
        ctx.memory_mut(|mem| mem.open_popup(Self::root_selector_popup_id()));
        self.focus_query_requested = false;
        self.unfocus_query_requested = true;
    }

    fn close_root_dropdown(&mut self, ctx: &egui::Context) {
        ctx.memory_mut(|mem| mem.close_popup());
    }

    fn move_root_dropdown_selection(&mut self, delta: isize) {
        let Some(max_index) = self.saved_roots.len().checked_sub(1) else {
            self.root_dropdown_highlight = None;
            return;
        };
        let current = self
            .root_dropdown_highlight
            .or_else(|| self.current_root_dropdown_index())
            .unwrap_or(0) as isize;
        let next = (current + delta).clamp(0, max_index as isize) as usize;
        self.root_dropdown_highlight = Some(next);
    }

    fn apply_root_dropdown_selection(&mut self, ctx: &egui::Context) {
        let selected = self
            .root_dropdown_highlight
            .and_then(|index| self.saved_roots.get(index).cloned());
        self.close_root_dropdown(ctx);
        if let Some(root) = selected {
            self.apply_root_change(root);
        }
    }

    fn prefer_relative_display(&self) -> bool {
        matches!(
            self.index.source,
            IndexSource::Walker | IndexSource::FileList(_)
        )
    }

    fn prefer_relative_display_for(source: &IndexSource) -> bool {
        matches!(source, IndexSource::Walker | IndexSource::FileList(_))
    }

    fn use_filelist_requires_locked_filters(&self) -> bool {
        self.use_filelist && !matches!(self.index.source, IndexSource::Walker)
    }

    fn is_entry_visible_for_flags(
        entry_kinds: &HashMap<PathBuf, EntryKind>,
        path: &Path,
        include_files: bool,
        include_dirs: bool,
    ) -> bool {
        match entry_kinds.get(path).copied() {
            Some(kind) => (kind.is_dir && include_dirs) || (!kind.is_dir && include_files),
            None => include_files && include_dirs,
        }
    }

    fn refresh_status_line(&mut self) {
        let tab_label = if self.tabs.is_empty() {
            "Tab: 1/1".to_string()
        } else {
            format!("Tab: {}/{}", self.active_tab + 1, self.tabs.len())
        };
        let indexed_count = if self.index_in_progress {
            if self.index.entries.is_empty() {
                self.all_entries.len()
            } else {
                self.index.entries.len()
            }
        } else {
            self.all_entries.len()
        };
        let clipped = self.results.len() >= self.limit;
        let clip_text = if clipped {
            format!(" (limit {} reached)", self.limit)
        } else {
            String::new()
        };
        let pinned = if self.pinned_paths.is_empty() {
            String::new()
        } else {
            format!(" | Pinned: {}", self.pinned_paths.len())
        };
        let searching = if self.search_in_progress {
            " | Searching..."
        } else {
            ""
        };
        let indexing = if self.index_in_progress {
            " | Indexing..."
        } else {
            ""
        };
        let executing = if self.action_in_progress {
            " | Executing..."
        } else {
            ""
        };
        let creating_filelist = if self.filelist_state.in_progress {
            if self.filelist_state.cancel_requested {
                " | Canceling FileList..."
            } else {
                " | Creating FileList..."
            }
        } else {
            ""
        };
        let updating = if self.update_state.in_progress {
            " | Updating..."
        } else {
            ""
        };
        let sorting = if self.sort_in_progress {
            " | Sorting..."
        } else {
            ""
        };
        let history_search = if self.history_search_active {
            format!(
                " | History search: {}/{}",
                self.history_search_results.len(),
                self.query_history.len()
            )
        } else {
            String::new()
        };
        let notice = if self.notice.is_empty() {
            String::new()
        } else {
            format!(" | {}", self.notice)
        };
        let memory = match self.memory_usage_text() {
            Some(mem) => format!(" | Mem: {mem}"),
            None => String::new(),
        };

        self.status_line = format!(
            "{} | Entries: {} | Results: {}{}{}{}{}{}{}{}{}{}{}{}",
            tab_label,
            indexed_count,
            self.results.len(),
            clip_text,
            pinned,
            searching,
            indexing,
            executing,
            creating_filelist,
            updating,
            sorting,
            history_search,
            memory,
            notice
        );
    }

    fn memory_usage_text(&mut self) -> Option<String> {
        if self.memory_usage_bytes.is_none()
            || self.last_memory_sample.elapsed() >= Self::MEMORY_SAMPLE_INTERVAL
        {
            self.last_memory_sample = Instant::now();
            self.memory_usage_bytes = memory_stats().map(|stats| stats.physical_mem as u64);
        }
        self.memory_usage_bytes
            .map(|bytes| format!("{:.1} MiB", bytes as f64 / 1024.0 / 1024.0))
    }

    fn set_notice(&mut self, notice: impl Into<String>) {
        self.notice = notice.into();
        self.refresh_status_line();
    }

    fn clear_notice(&mut self) {
        self.notice.clear();
        self.refresh_status_line();
    }

    fn action_progress_label(&self) -> Option<&'static str> {
        if self.action_in_progress {
            Some("Opening...")
        } else {
            None
        }
    }

    fn poll_action_response(&mut self) {
        while let Ok(response) = self.action_rx.try_recv() {
            let target_tab_id = self.action_request_tabs.remove(&response.request_id);
            if Some(response.request_id) == self.pending_action_request_id {
                self.pending_action_request_id = None;
                self.action_in_progress = false;
                self.set_notice(response.notice);
                continue;
            }

            let Some(tab_id) = target_tab_id else {
                continue;
            };
            let Some(tab_index) = self.find_tab_index_by_id(tab_id) else {
                continue;
            };
            let Some(tab) = self.tabs.get_mut(tab_index) else {
                continue;
            };
            if Some(response.request_id) != tab.pending_action_request_id {
                continue;
            }
            tab.pending_action_request_id = None;
            tab.action_in_progress = false;
            tab.notice = response.notice;
        }
    }

    fn poll_sort_response(&mut self) {
        while let Ok(response) = self.sort_rx.try_recv() {
            let target_tab_id = self.sort_request_tabs.remove(&response.request_id);
            for (path, metadata) in response.entries {
                self.cache_sort_metadata(path, metadata);
            }

            if Some(response.request_id) == self.pending_sort_request_id {
                self.pending_sort_request_id = None;
                self.sort_in_progress = false;
                if response.mode == self.result_sort_mode {
                    self.apply_result_sort(false);
                } else {
                    self.refresh_status_line();
                }
                continue;
            }

            let Some(tab_id) = target_tab_id else {
                continue;
            };
            let Some(tab_index) = self.find_tab_index_by_id(tab_id) else {
                continue;
            };
            let Some(tab) = self.tabs.get_mut(tab_index) else {
                continue;
            };
            if Some(response.request_id) != tab.result_state.pending_sort_request_id {
                continue;
            }
            tab.result_state.pending_sort_request_id = None;
            tab.result_state.sort_in_progress = false;
            if response.mode == tab.result_state.result_sort_mode {
                tab.result_state.results = Self::build_sorted_results_from(
                    &tab.result_state.base_results,
                    tab.result_state.result_sort_mode,
                    &self.sort_metadata_cache.entries,
                );
                tab.result_state.results_compacted = false;
                if tab.result_state.results.is_empty() {
                    tab.result_state.current_row = None;
                    tab.result_state.preview.clear();
                    tab.pending_preview_request_id = None;
                    tab.preview_in_progress = false;
                } else {
                    let max_index = tab.result_state.results.len().saturating_sub(1);
                    tab.result_state.current_row =
                        tab.result_state.current_row.map(|row| row.min(max_index));
                }
                Self::compact_inactive_tab_state(tab);
            }
        }
    }

    fn move_page(&mut self, direction: isize) {
        self.move_row(direction.saturating_mul(Self::PAGE_MOVE_ROWS));
    }

    fn move_to_first_row(&mut self) {
        self.commit_query_history_if_needed(true);
        if self.results.is_empty() {
            return;
        }
        self.current_row = Some(0);
        self.scroll_to_current = true;
        self.request_preview_for_current();
        self.refresh_status_line();
    }

    fn move_to_last_row(&mut self) {
        self.commit_query_history_if_needed(true);
        if self.results.is_empty() {
            return;
        }
        self.current_row = Some(self.results.len().saturating_sub(1));
        self.scroll_to_current = true;
        self.request_preview_for_current();
        self.refresh_status_line();
    }

    fn is_entry_visible_for_current_filter(&self, path: &Path) -> bool {
        match self.entry_kinds.get(path).copied() {
            Some(kind) => {
                (kind.is_dir && self.include_dirs) || (!kind.is_dir && self.include_files)
            }
            None => self.include_files && self.include_dirs,
        }
    }

    fn kind_resolution_needed_for_filters(&self) -> bool {
        !self.include_files || !self.include_dirs
    }

    fn reset_kind_resolution_state(&mut self) {
        self.pending_kind_paths.clear();
        self.pending_kind_paths_set.clear();
        self.in_flight_kind_paths.clear();
        self.kind_resolution_in_progress = false;
        self.kind_resolution_epoch = self.kind_resolution_epoch.saturating_add(1);
    }

    fn queue_unknown_kind_paths_for_active_entries(&mut self) {
        if !self.kind_resolution_needed_for_filters() {
            return;
        }
        let source: Vec<PathBuf> = if self.index_in_progress && !self.index.entries.is_empty() {
            self.index.entries.clone()
        } else {
            self.all_entries.as_ref().clone()
        };
        self.queue_unknown_kind_paths(&source);
    }

    fn queue_unknown_kind_paths_for_completed_walker_entries(&mut self) {
        let source = self.all_entries.as_ref().clone();
        self.queue_unknown_kind_paths(&source);
    }

    fn queue_unknown_kind_paths(&mut self, source: &[PathBuf]) {
        for path in source {
            if !self.entry_kinds.contains_key(path) {
                self.queue_kind_resolution(path.clone());
            }
        }
    }

    fn queue_kind_resolution(&mut self, path: PathBuf) {
        if self.pending_kind_paths_set.contains(&path) || self.in_flight_kind_paths.contains(&path)
        {
            return;
        }
        self.pending_kind_paths_set.insert(path.clone());
        self.pending_kind_paths.push_back(path);
    }

    fn pump_kind_resolution_requests(&mut self) {
        const MAX_DISPATCH_PER_FRAME: usize = 128;
        let mut dispatched = 0usize;
        while dispatched < MAX_DISPATCH_PER_FRAME {
            let Some(path) = self.pending_kind_paths.pop_front() else {
                break;
            };
            self.pending_kind_paths_set.remove(&path);
            let req = KindResolveRequest {
                epoch: self.kind_resolution_epoch,
                path: path.clone(),
            };
            if self.kind_tx.send(req).is_err() {
                break;
            }
            self.in_flight_kind_paths.insert(path);
            dispatched = dispatched.saturating_add(1);
        }
        self.kind_resolution_in_progress =
            !self.pending_kind_paths.is_empty() || !self.in_flight_kind_paths.is_empty();
    }

    fn poll_kind_response(&mut self) {
        const MAX_MESSAGES_PER_FRAME: usize = 512;
        let mut processed = 0usize;
        let mut resolved_any = false;
        let mut resolved_current_row = false;

        while let Ok(response) = self.kind_rx.try_recv() {
            if response.epoch != self.kind_resolution_epoch {
                continue;
            }
            self.in_flight_kind_paths.remove(&response.path);
            if let Some(kind) = response.kind {
                if self.current_row.is_some_and(|row| {
                    self.results
                        .get(row)
                        .is_some_and(|(path, _)| *path == response.path)
                }) {
                    resolved_current_row = true;
                }
                self.entry_kinds.insert(response.path, kind);
                resolved_any = true;
            }
            processed = processed.saturating_add(1);
            if processed >= MAX_MESSAGES_PER_FRAME {
                break;
            }
        }

        self.kind_resolution_in_progress =
            !self.pending_kind_paths.is_empty() || !self.in_flight_kind_paths.is_empty();

        if resolved_any && (!self.include_files || !self.include_dirs) {
            self.apply_entry_filters(true);
        }
        if resolved_current_row && self.show_preview {
            self.request_preview_for_current();
        }
    }

    fn move_row(&mut self, delta: isize) {
        self.commit_query_history_if_needed(true);
        if self.results.is_empty() {
            return;
        }
        let row = self.current_row.unwrap_or(0) as isize;
        let next = (row + delta).clamp(0, self.results.len() as isize - 1) as usize;
        self.current_row = Some(next);
        self.scroll_to_current = true;
        self.request_preview_for_current();
        self.refresh_status_line();
    }

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

    fn execute_selected(&mut self) {
        self.execute_selected_with_options(false);
    }

    fn execute_selected_for_activation(&mut self, open_parent_for_files: bool) {
        self.execute_selected_with_options(open_parent_for_files);
    }

    fn execute_selected_open_folder(&mut self) {
        self.execute_selected_for_activation(true);
    }

    fn execute_selected_with_options(&mut self, open_parent_for_files: bool) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }
        if let Some(blocked) = self.first_action_path_outside_root(&paths) {
            self.pending_action_request_id = None;
            self.action_in_progress = false;
            self.set_notice(format!(
                "Action blocked: path is outside current root: {}",
                normalize_path_for_display(&blocked)
            ));
            return;
        }

        let request_id = self.next_action_request_id;
        self.next_action_request_id = self.next_action_request_id.saturating_add(1);
        self.pending_action_request_id = Some(request_id);
        self.action_in_progress = true;
        if let Some(tab_id) = self.current_tab_id() {
            self.action_request_tabs.insert(request_id, tab_id);
        }

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
        if self.action_tx.send(req).is_err() {
            self.pending_action_request_id = None;
            self.action_in_progress = false;
            self.set_notice("Action worker is unavailable");
        }
    }

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

    fn clipboard_paths_text(paths: &[PathBuf]) -> String {
        paths
            .iter()
            .map(|p| normalize_path_for_display(p))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn clear_pinned(&mut self) {
        self.pinned_paths.clear();
        self.set_notice("Cleared pinned selections");
    }

    fn clear_query_and_selection(&mut self) {
        self.query.clear();
        self.reset_query_history_navigation();
        self.reset_history_search_state();
        self.query_history_dirty_since = None;
        self.pinned_paths.clear();
        // Keep the list visible after Esc/Ctrl+G by restoring the default row selection.
        self.current_row = Some(0);
        self.preview.clear();
        self.update_results();
        self.focus_query_requested = true;
        self.set_notice("Cleared selection and query");
    }

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

    fn disconnect_worker_channels(&mut self) {
        let (dummy_search_tx, _) = mpsc::channel::<SearchRequest>();
        let (dummy_preview_tx, _) = mpsc::channel::<PreviewRequest>();
        let (dummy_action_tx, _) = mpsc::channel::<ActionRequest>();
        let (dummy_sort_tx, _) = mpsc::channel::<SortMetadataRequest>();
        let (dummy_kind_tx, _) = mpsc::channel::<KindResolveRequest>();
        let (dummy_filelist_tx, _) = mpsc::channel::<FileListRequest>();
        let (dummy_update_tx, _) = mpsc::channel::<UpdateRequest>();
        let (dummy_index_tx, _) = mpsc::channel::<IndexRequest>();
        let old_search_tx = std::mem::replace(&mut self.search_tx, dummy_search_tx);
        let old_preview_tx = std::mem::replace(&mut self.preview_tx, dummy_preview_tx);
        let old_action_tx = std::mem::replace(&mut self.action_tx, dummy_action_tx);
        let old_sort_tx = std::mem::replace(&mut self.sort_tx, dummy_sort_tx);
        let old_kind_tx = std::mem::replace(&mut self.kind_tx, dummy_kind_tx);
        let old_filelist_tx = std::mem::replace(&mut self.filelist_tx, dummy_filelist_tx);
        let old_update_tx = std::mem::replace(&mut self.update_tx, dummy_update_tx);
        let old_index_tx = std::mem::replace(&mut self.index_tx, dummy_index_tx);
        drop(old_search_tx);
        drop(old_preview_tx);
        drop(old_action_tx);
        drop(old_sort_tx);
        drop(old_kind_tx);
        drop(old_filelist_tx);
        drop(old_update_tx);
        drop(old_index_tx);
    }

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
}

impl eframe::App for FlistWalkerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if process_shutdown_requested() {
            self.set_notice("Shutdown requested by signal");
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        self.poll_index_response();
        self.poll_search_response();
        self.poll_action_response();
        self.poll_sort_response();
        self.poll_preview_response();
        self.poll_kind_response();
        self.pump_kind_resolution_requests();
        self.poll_filelist_response();
        self.poll_update_response();
        if self.update_state.close_requested_for_install {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        self.commit_query_history_if_needed(false);
        let memory_elapsed = self.last_memory_sample.elapsed();
        if memory_elapsed >= Self::MEMORY_SAMPLE_INTERVAL {
            self.refresh_status_line();
        } else {
            ctx.request_repaint_after(Self::MEMORY_SAMPLE_INTERVAL - memory_elapsed);
        }
        if self.search_in_progress
            || self.index_in_progress
            || self.preview_in_progress
            || self.action_in_progress
            || self.sort_in_progress
            || self.kind_resolution_in_progress
            || self.filelist_state.in_progress
            || self.update_state.in_progress
            || self.any_tab_async_in_progress()
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
        self.capture_window_geometry(ctx);
        self.apply_stable_window_geometry(false);
        // Handle app shortcuts before widget rendering so Tab is not consumed by egui focus traversal.
        self.handle_shortcuts(ctx);

        self.render_top_panel(ctx);
        self.render_status_panel(ctx);
        self.render_filelist_dialogs(ctx);
        self.render_update_dialog(ctx);
        self.render_central_panel(ctx);
        self.maybe_save_ui_state(false);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.apply_stable_window_geometry(true);
        self.ui_state_dirty = true;
        self.maybe_save_ui_state(true);
        let _ = self.shutdown_workers_with_timeout(Self::WORKER_JOIN_TIMEOUT, "app exit");
    }
}

impl Drop for FlistWalkerApp {
    fn drop(&mut self) {
        self.apply_stable_window_geometry(true);
        self.ui_state_dirty = true;
        self.maybe_save_ui_state(true);
        let _ = self.shutdown_workers_with_timeout(Self::WORKER_JOIN_TIMEOUT, "drop fallback");
    }
}

#[cfg(test)]
mod tests;

use crate::fs_atomic::write_text_atomic;
use crate::indexer::{
    find_filelist_in_first_level, has_ancestor_filelists, IndexBuildResult, IndexSource,
};
use crate::ui_model::{
    build_preview_text_with_kind, display_path_with_mode, match_positions_for_path,
    normalize_path_for_display, should_skip_preview,
};
use crate::updater::{
    self_update_disabled, should_skip_update_prompt, UpdateCandidate, UpdateSupport,
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

#[path = "app/input.rs"]
mod input;
#[path = "app/render.rs"]
mod render;
#[path = "app/session.rs"]
mod session;
#[path = "app/workers.rs"]
mod workers;

#[allow(unused_imports)]
use input::*;
#[allow(unused_imports)]
use render::*;
use session::*;
use workers::*;

#[derive(Clone, Debug)]
struct AppTabState {
    id: u64,
    root: PathBuf,
    use_filelist: bool,
    use_regex: bool,
    ignore_case: bool,
    include_files: bool,
    include_dirs: bool,
    index: IndexBuildResult,
    all_entries: Arc<Vec<PathBuf>>,
    entries: Arc<Vec<PathBuf>>,
    entry_kinds: HashMap<PathBuf, EntryKind>,
    pending_index_request_id: Option<u64>,
    index_in_progress: bool,
    pending_index_entries: VecDeque<IndexEntry>,
    pending_index_entries_request_id: Option<u64>,
    pending_kind_paths: VecDeque<PathBuf>,
    pending_kind_paths_set: HashSet<PathBuf>,
    in_flight_kind_paths: HashSet<PathBuf>,
    kind_resolution_epoch: u64,
    kind_resolution_in_progress: bool,
    incremental_filtered_entries: Vec<PathBuf>,
    last_incremental_results_refresh: Instant,
    last_search_snapshot_len: usize,
    search_resume_pending: bool,
    search_rerun_pending: bool,
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
    base_results: Vec<(PathBuf, f64)>,
    results: Vec<(PathBuf, f64)>,
    result_sort_mode: ResultSortMode,
    pending_sort_request_id: Option<u64>,
    sort_in_progress: bool,
    pinned_paths: HashSet<PathBuf>,
    current_row: Option<usize>,
    preview: String,
    results_compacted: bool,
    notice: String,
    pending_request_id: Option<u64>,
    pending_preview_request_id: Option<u64>,
    pending_action_request_id: Option<u64>,
    search_in_progress: bool,
    preview_in_progress: bool,
    action_in_progress: bool,
    scroll_to_current: bool,
    focus_query_requested: bool,
    unfocus_query_requested: bool,
}

#[derive(Default)]
struct BackgroundIndexState {
    source: Option<IndexSource>,
    entries: Vec<PathBuf>,
    entry_kinds: HashMap<PathBuf, EntryKind>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct SortMetadata {
    pub(super) modified: Option<SystemTime>,
    pub(super) created: Option<SystemTime>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ResultSortMode {
    #[default]
    Score,
    NameAsc,
    NameDesc,
    ModifiedDesc,
    ModifiedAsc,
    CreatedDesc,
    CreatedAsc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EntryDisplayKind {
    File,
    Dir,
    Link,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct EntryKind {
    pub(super) display: EntryDisplayKind,
    pub(super) is_dir: bool,
}

impl EntryKind {
    pub(super) const fn file() -> Self {
        Self {
            display: EntryDisplayKind::File,
            is_dir: false,
        }
    }

    pub(super) const fn dir() -> Self {
        Self {
            display: EntryDisplayKind::Dir,
            is_dir: true,
        }
    }

    pub(super) const fn link(is_dir: bool) -> Self {
        Self {
            display: EntryDisplayKind::Link,
            is_dir,
        }
    }
}

impl ResultSortMode {
    fn label(self) -> &'static str {
        match self {
            Self::Score => "Score",
            Self::NameAsc => "Name (A-Z)",
            Self::NameDesc => "Name (Z-A)",
            Self::ModifiedDesc => "Modified (New)",
            Self::ModifiedAsc => "Modified (Old)",
            Self::CreatedDesc => "Created (New)",
            Self::CreatedAsc => "Created (Old)",
        }
    }

    fn uses_metadata(self) -> bool {
        matches!(
            self,
            Self::ModifiedDesc | Self::ModifiedAsc | Self::CreatedDesc | Self::CreatedAsc
        )
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

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct HighlightCacheKey {
    path: PathBuf,
    prefer_relative: bool,
    use_regex: bool,
    ignore_case: bool,
}

struct PendingFileListConfirmation {
    tab_id: u64,
    root: PathBuf,
    entries: Vec<PathBuf>,
    existing_path: PathBuf,
}

struct PendingFileListAncestorConfirmation {
    tab_id: u64,
    root: PathBuf,
    entries: Vec<PathBuf>,
}

struct PendingFileListAfterIndex {
    tab_id: u64,
    root: PathBuf,
}

struct PendingFileListUseWalkerConfirmation {
    source_tab_id: u64,
    root: PathBuf,
}

#[derive(Clone, Debug)]
struct UpdatePromptState {
    candidate: UpdateCandidate,
    skip_until_next_version: bool,
    install_started: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FileListDialogKind {
    Overwrite,
    Ancestor,
    UseWalker,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TabDragState {
    source_index: usize,
    hover_index: usize,
    press_pos: egui::Pos2,
    dragging: bool,
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
    next_filelist_request_id: u64,
    pending_filelist_request_id: Option<u64>,
    next_update_request_id: u64,
    pending_update_request_id: Option<u64>,
    pending_filelist_request_tab_id: Option<u64>,
    pending_filelist_root: Option<PathBuf>,
    pending_filelist_cancel: Option<Arc<AtomicBool>>,
    pending_filelist_after_index: Option<PendingFileListAfterIndex>,
    pending_filelist_confirmation: Option<PendingFileListConfirmation>,
    pending_filelist_ancestor_confirmation: Option<PendingFileListAncestorConfirmation>,
    pending_filelist_use_walker_confirmation: Option<PendingFileListUseWalkerConfirmation>,
    latest_index_request_ids: Arc<Mutex<HashMap<u64, u64>>>,
    pending_index_queue: VecDeque<IndexRequest>,
    index_inflight_requests: HashSet<u64>,
    search_in_progress: bool,
    index_in_progress: bool,
    preview_in_progress: bool,
    action_in_progress: bool,
    sort_in_progress: bool,
    kind_resolution_in_progress: bool,
    filelist_in_progress: bool,
    filelist_cancel_requested: bool,
    update_in_progress: bool,
    pending_copy_shortcut: bool,
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
    active_filelist_dialog: Option<FileListDialogKind>,
    active_filelist_dialog_button: usize,
    update_prompt: Option<UpdatePromptState>,
    skipped_update_target_version: Option<String>,
    close_requested_for_update: bool,
    tab_drag_state: Option<TabDragState>,
    preview_cache: HashMap<PathBuf, String>,
    preview_cache_order: VecDeque<PathBuf>,
    preview_cache_total_bytes: usize,
    highlight_cache_scope_query: String,
    highlight_cache_scope_root: PathBuf,
    highlight_cache_scope_use_regex: bool,
    highlight_cache_scope_ignore_case: bool,
    highlight_cache_scope_prefer_relative: bool,
    highlight_cache: HashMap<HighlightCacheKey, Arc<Vec<u16>>>,
    highlight_cache_order: VecDeque<HighlightCacheKey>,
    sort_metadata_cache: HashMap<PathBuf, SortMetadata>,
    sort_metadata_cache_order: VecDeque<PathBuf>,
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
            .map(Self::normalize_windows_path)
            .filter(|p| p.is_dir());
        let saved_default = launch
            .default_root
            .as_ref()
            .and_then(|p| p.canonicalize().ok())
            .map(Self::normalize_windows_path)
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
        let worker_shutdown = Arc::new(AtomicBool::new(false));
        let mut worker_runtime = WorkerRuntime::new(Arc::clone(&worker_shutdown));
        let (search_tx, search_rx, search_handle) =
            spawn_search_worker(Arc::clone(&worker_shutdown));
        worker_runtime.push("search", search_handle);
        let (preview_tx, preview_rx, preview_handle) =
            spawn_preview_worker(Arc::clone(&worker_shutdown));
        worker_runtime.push("preview", preview_handle);
        let (action_tx, action_rx, action_handle) =
            spawn_action_worker(Arc::clone(&worker_shutdown));
        worker_runtime.push("action", action_handle);
        let (sort_tx, sort_rx, sort_handle) =
            spawn_sort_metadata_worker(Arc::clone(&worker_shutdown));
        worker_runtime.push("sort-metadata", sort_handle);
        let (kind_tx, kind_rx, kind_handle) =
            spawn_kind_resolver_worker(Arc::clone(&worker_shutdown));
        worker_runtime.push("kind-resolver", kind_handle);
        let (filelist_tx, filelist_rx, filelist_handle) =
            spawn_filelist_worker(Arc::clone(&worker_shutdown));
        worker_runtime.push("filelist", filelist_handle);
        let (update_tx, update_rx, update_handle) =
            spawn_update_worker(Arc::clone(&worker_shutdown));
        worker_runtime.push("update", update_handle);
        let latest_index_request_ids = Arc::new(Mutex::new(HashMap::new()));
        let (index_tx, index_rx, index_handles) = spawn_index_worker(
            Arc::clone(&worker_shutdown),
            Arc::clone(&latest_index_request_ids),
        );
        for (idx, handle) in index_handles.into_iter().enumerate() {
            worker_runtime.push(format!("index-{idx}"), handle);
        }
        let mut app = Self {
            root: Self::normalize_windows_path(root),
            limit: limit.clamp(1, 1000),
            query,
            query_history: launch.query_history.iter().cloned().collect(),
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
            search_tx,
            search_rx,
            preview_tx,
            preview_rx,
            action_tx,
            action_rx,
            sort_tx,
            sort_rx,
            kind_tx,
            kind_rx,
            filelist_tx,
            filelist_rx,
            update_tx,
            update_rx,
            index_tx,
            index_rx,
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
            next_filelist_request_id: 1,
            pending_filelist_request_id: None,
            next_update_request_id: 1,
            pending_update_request_id: None,
            pending_filelist_request_tab_id: None,
            pending_filelist_root: None,
            pending_filelist_cancel: None,
            pending_filelist_after_index: None,
            pending_filelist_confirmation: None,
            pending_filelist_ancestor_confirmation: None,
            pending_filelist_use_walker_confirmation: None,
            latest_index_request_ids,
            pending_index_queue: VecDeque::new(),
            index_inflight_requests: HashSet::new(),
            search_in_progress: false,
            index_in_progress: false,
            preview_in_progress: false,
            action_in_progress: false,
            sort_in_progress: false,
            kind_resolution_in_progress: false,
            filelist_in_progress: false,
            filelist_cancel_requested: false,
            update_in_progress: false,
            pending_copy_shortcut: false,
            scroll_to_current: true,
            preview_resize_in_progress: false,
            focus_query_requested: true,
            unfocus_query_requested: false,
            saved_roots: Self::load_saved_roots(),
            default_root: launch.default_root.clone(),
            show_preview: launch.show_preview,
            preview_panel_width: launch
                .preview_panel_width
                .max(Self::MIN_PREVIEW_PANEL_WIDTH),
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
            active_filelist_dialog: None,
            active_filelist_dialog_button: 0,
            update_prompt: None,
            skipped_update_target_version: launch.skipped_update_target_version,
            close_requested_for_update: false,
            tab_drag_state: None,
            preview_cache: HashMap::new(),
            preview_cache_order: VecDeque::new(),
            preview_cache_total_bytes: 0,
            highlight_cache_scope_query: String::new(),
            highlight_cache_scope_root: PathBuf::new(),
            highlight_cache_scope_use_regex: false,
            highlight_cache_scope_ignore_case: true,
            highlight_cache_scope_prefer_relative: false,
            highlight_cache: HashMap::new(),
            highlight_cache_order: VecDeque::new(),
            sort_metadata_cache: HashMap::new(),
            sort_metadata_cache_order: VecDeque::new(),
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

    fn normalize_windows_path(path: PathBuf) -> PathBuf {
        #[cfg(windows)]
        {
            let raw = path.to_string_lossy();
            if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
                return PathBuf::from(format!(r"\\{}", rest));
            }
            if let Some(rest) = raw.strip_prefix(r"\\?\") {
                return PathBuf::from(rest);
            }
        }
        path
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

    fn request_startup_update_check(&mut self) {
        if self_update_disabled() {
            self.pending_update_request_id = None;
            self.update_in_progress = false;
            return;
        }
        let request_id = self.next_update_request_id;
        self.next_update_request_id = self.next_update_request_id.saturating_add(1);
        self.pending_update_request_id = Some(request_id);
        self.update_in_progress = true;
        if self
            .update_tx
            .send(UpdateRequest {
                request_id,
                kind: UpdateRequestKind::Check,
            })
            .is_err()
        {
            self.pending_update_request_id = None;
            self.update_in_progress = false;
        }
    }

    fn start_update_install(&mut self) {
        let Some(prompt) = self.update_prompt.as_ref() else {
            return;
        };
        if prompt.install_started {
            return;
        }
        let candidate = prompt.candidate.clone();
        let current_exe = match std::env::current_exe() {
            Ok(path) => path,
            Err(err) => {
                self.set_notice(format!(
                    "Update failed: failed to resolve current executable: {err}"
                ));
                return;
            }
        };
        if let Some(prompt) = self.update_prompt.as_mut() {
            prompt.install_started = true;
        }
        let request_id = self.next_update_request_id;
        self.next_update_request_id = self.next_update_request_id.saturating_add(1);
        self.pending_update_request_id = Some(request_id);
        self.update_in_progress = true;
        if self
            .update_tx
            .send(UpdateRequest {
                request_id,
                kind: UpdateRequestKind::DownloadAndApply {
                    candidate: candidate.clone(),
                    current_exe,
                },
            })
            .is_err()
        {
            self.pending_update_request_id = None;
            self.update_in_progress = false;
            if let Some(prompt) = self.update_prompt.as_mut() {
                prompt.install_started = false;
            }
            self.set_notice("Update worker is unavailable");
            return;
        }
        self.set_notice(format!(
            "Downloading update {}...",
            candidate.target_version
        ));
    }

    fn dismiss_update_prompt(&mut self) {
        self.update_prompt = None;
    }

    fn skip_update_prompt_until_next_version(&mut self) {
        let Some(target_version) = self
            .update_prompt
            .as_ref()
            .map(|prompt| prompt.candidate.target_version.clone())
        else {
            return;
        };
        self.skipped_update_target_version = Some(target_version.clone());
        self.mark_ui_state_dirty();
        self.persist_ui_state_now();
        self.update_prompt = None;
        self.set_notice(format!(
            "Update {} hidden until a newer version is available",
            target_version
        ));
    }

    fn update_prompt_is_suppressed(&self, candidate: &UpdateCandidate) -> bool {
        should_skip_update_prompt(
            &candidate.target_version,
            self.skipped_update_target_version.as_deref(),
        )
    }

    fn clear_sort_metadata_cache(&mut self) {
        self.sort_metadata_cache.clear();
        self.sort_metadata_cache_order.clear();
    }

    fn cache_sort_metadata(&mut self, path: PathBuf, metadata: SortMetadata) {
        if !self.sort_metadata_cache.contains_key(&path) {
            self.sort_metadata_cache_order.push_back(path.clone());
        }
        self.sort_metadata_cache.insert(path.clone(), metadata);
        while self.sort_metadata_cache_order.len() > Self::SORT_METADATA_CACHE_MAX {
            if let Some(oldest) = self.sort_metadata_cache_order.pop_front() {
                self.sort_metadata_cache.remove(&oldest);
            }
        }
        if !self.sort_metadata_cache.contains_key(&path) {
            self.sort_metadata_cache_order
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
        Self::build_sorted_results_from(&self.base_results, mode, &self.sort_metadata_cache)
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
            .filter(|path| !self.sort_metadata_cache.contains_key(path))
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
        let mut key = Self::normalize_windows_path(path.to_path_buf())
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
        Self::normalize_windows_path(self.root.clone())
            .to_string_lossy()
            .to_string()
    }

    fn choose_startup_root(
        root: PathBuf,
        root_explicit: bool,
        restore_tabs_enabled: bool,
        restore_session: Option<&(Vec<SavedTabState>, usize)>,
        last_root: Option<PathBuf>,
        default_root: Option<PathBuf>,
    ) -> PathBuf {
        if root_explicit {
            return root;
        }
        if let Some((tabs, active_tab)) = restore_session {
            if let Some(tab_root) = tabs.get(*active_tab).map(|tab| PathBuf::from(&tab.root)) {
                return tab_root;
            }
        }
        if restore_tabs_enabled {
            last_root.or(default_root).unwrap_or(root)
        } else {
            default_root.or(last_root).unwrap_or(root)
        }
    }

    fn initialize_tabs(&mut self) {
        let id = self.next_tab_id;
        self.next_tab_id = self.next_tab_id.saturating_add(1);
        self.tabs = vec![self.capture_active_tab_state(id)];
        self.active_tab = 0;
    }

    fn restored_tab_state(&self, id: u64, saved: &SavedTabState) -> AppTabState {
        AppTabState {
            id,
            root: Self::normalize_windows_path(PathBuf::from(&saved.root)),
            use_filelist: saved.use_filelist,
            use_regex: saved.use_regex,
            ignore_case: saved.ignore_case,
            include_files: saved.include_files,
            include_dirs: saved.include_dirs,
            index: IndexBuildResult {
                entries: Vec::new(),
                source: IndexSource::None,
            },
            all_entries: Arc::new(Vec::new()),
            entries: Arc::new(Vec::new()),
            entry_kinds: HashMap::new(),
            pending_index_request_id: None,
            index_in_progress: false,
            pending_index_entries: VecDeque::new(),
            pending_index_entries_request_id: None,
            pending_kind_paths: VecDeque::new(),
            pending_kind_paths_set: HashSet::new(),
            in_flight_kind_paths: HashSet::new(),
            kind_resolution_epoch: 1,
            kind_resolution_in_progress: false,
            incremental_filtered_entries: Vec::new(),
            last_incremental_results_refresh: Instant::now(),
            last_search_snapshot_len: 0,
            search_resume_pending: false,
            search_rerun_pending: false,
            query: saved.query.clone(),
            query_history: self.query_history.clone(),
            query_history_cursor: None,
            query_history_draft: None,
            query_history_dirty_since: None,
            history_search_active: false,
            history_search_query: String::new(),
            history_search_original_query: String::new(),
            history_search_results: Vec::new(),
            history_search_current: None,
            pending_restore_refresh: true,
            base_results: Vec::new(),
            results: Vec::new(),
            result_sort_mode: ResultSortMode::Score,
            pending_sort_request_id: None,
            sort_in_progress: false,
            pinned_paths: HashSet::new(),
            // Tabs do not persist selection, so restored tabs start with the first row selected.
            current_row: Some(0),
            preview: String::new(),
            results_compacted: false,
            notice: "Restored tab".to_string(),
            pending_request_id: None,
            pending_preview_request_id: None,
            pending_action_request_id: None,
            search_in_progress: false,
            preview_in_progress: false,
            action_in_progress: false,
            scroll_to_current: true,
            focus_query_requested: false,
            unfocus_query_requested: false,
        }
    }

    fn initialize_tabs_from_saved(&mut self, saved_tabs: Vec<SavedTabState>, active_tab: usize) {
        self.tabs = saved_tabs
            .iter()
            .map(|saved| {
                let id = self.next_tab_id;
                self.next_tab_id = self.next_tab_id.saturating_add(1);
                self.restored_tab_state(id, saved)
            })
            .collect();
        self.active_tab = active_tab.min(self.tabs.len().saturating_sub(1));
        if let Some(tab) = self.tabs.get(self.active_tab).cloned() {
            self.apply_tab_state(&tab);
            self.focus_query_requested = true;
            self.unfocus_query_requested = false;
            self.trigger_restore_refresh_for_active_tab();
            self.notice = "Restored tab session".to_string();
            self.refresh_status_line();
        }
    }

    fn current_tab_id(&self) -> Option<u64> {
        self.tabs.get(self.active_tab).map(|tab| tab.id)
    }

    fn shrink_vec_if_sparse<T>(vec: &mut Vec<T>) {
        let cap = vec.capacity();
        let len = vec.len();
        if cap >= Self::SHRINK_MIN_CAPACITY && cap > len.saturating_mul(2) {
            vec.shrink_to_fit();
        }
    }

    fn shrink_deque_if_sparse<T>(deque: &mut VecDeque<T>) {
        let cap = deque.capacity();
        let len = deque.len();
        if cap >= Self::SHRINK_MIN_CAPACITY && cap > len.saturating_mul(2) {
            deque.shrink_to_fit();
        }
    }

    fn shrink_checkpoint_buffers(&mut self) {
        Self::shrink_vec_if_sparse(&mut self.index.entries);
        Self::shrink_vec_if_sparse(&mut self.incremental_filtered_entries);
        Self::shrink_deque_if_sparse(&mut self.pending_index_entries);
        Self::shrink_deque_if_sparse(&mut self.pending_kind_paths);
    }

    fn shrink_tab_checkpoint_buffers(tab: &mut AppTabState) {
        Self::shrink_vec_if_sparse(&mut tab.index.entries);
        Self::shrink_vec_if_sparse(&mut tab.incremental_filtered_entries);
        Self::shrink_deque_if_sparse(&mut tab.pending_index_entries);
        Self::shrink_deque_if_sparse(&mut tab.pending_kind_paths);
    }

    fn compact_inactive_tab_state(tab: &mut AppTabState) {
        let can_compact_results = !tab.index_in_progress
            && !tab.search_in_progress
            && !tab.sort_in_progress
            && tab.pending_request_id.is_none()
            && tab.pending_sort_request_id.is_none();
        if can_compact_results && !tab.results.is_empty() {
            tab.results.clear();
            tab.results.shrink_to_fit();
            tab.results_compacted = true;
        }
        if !tab.preview_in_progress {
            tab.preview.clear();
        }
        Self::shrink_tab_checkpoint_buffers(tab);
    }

    fn restore_results_from_compacted_tab(&mut self) {
        let was_compacted = self
            .tabs
            .get(self.active_tab)
            .map(|tab| tab.results_compacted)
            .unwrap_or(false);
        if !was_compacted {
            return;
        }
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.results_compacted = false;
        }

        if self.base_results.is_empty() {
            if self.query.trim().is_empty() {
                let results = self
                    .entries
                    .iter()
                    .take(self.limit)
                    .cloned()
                    .map(|p| (p, 0.0))
                    .collect();
                self.replace_results_snapshot(results, true);
            } else {
                self.refresh_status_line();
            }
            return;
        }

        if self.result_sort_mode == ResultSortMode::Score {
            self.apply_results_with_selection_policy(self.base_results.clone(), true, false);
        } else {
            self.apply_result_sort(true);
        }
    }

    fn capture_active_tab_state(&self, id: u64) -> AppTabState {
        AppTabState {
            id,
            root: self.root.clone(),
            use_filelist: self.use_filelist,
            use_regex: self.use_regex,
            ignore_case: self.ignore_case,
            include_files: self.include_files,
            include_dirs: self.include_dirs,
            index: self.index.clone(),
            all_entries: Arc::clone(&self.all_entries),
            entries: Arc::clone(&self.entries),
            entry_kinds: self.entry_kinds.clone(),
            pending_index_request_id: self.pending_index_request_id,
            index_in_progress: self.index_in_progress,
            pending_index_entries: self.pending_index_entries.clone(),
            pending_index_entries_request_id: self.pending_index_entries_request_id,
            pending_kind_paths: self.pending_kind_paths.clone(),
            pending_kind_paths_set: self.pending_kind_paths_set.clone(),
            in_flight_kind_paths: self.in_flight_kind_paths.clone(),
            kind_resolution_epoch: self.kind_resolution_epoch,
            kind_resolution_in_progress: self.kind_resolution_in_progress,
            incremental_filtered_entries: self.incremental_filtered_entries.clone(),
            last_incremental_results_refresh: self.last_incremental_results_refresh,
            last_search_snapshot_len: self.last_search_snapshot_len,
            search_resume_pending: self.search_resume_pending,
            search_rerun_pending: self.search_rerun_pending,
            query: self.query.clone(),
            query_history: self.query_history.clone(),
            query_history_cursor: self.query_history_cursor,
            query_history_draft: self.query_history_draft.clone(),
            query_history_dirty_since: self.query_history_dirty_since,
            history_search_active: self.history_search_active,
            history_search_query: self.history_search_query.clone(),
            history_search_original_query: self.history_search_original_query.clone(),
            history_search_results: self.history_search_results.clone(),
            history_search_current: self.history_search_current,
            pending_restore_refresh: self.pending_restore_refresh,
            base_results: self.base_results.clone(),
            results: self.results.clone(),
            result_sort_mode: self.result_sort_mode,
            pending_sort_request_id: self.pending_sort_request_id,
            sort_in_progress: self.sort_in_progress,
            pinned_paths: self.pinned_paths.clone(),
            current_row: self.current_row,
            preview: self.preview.clone(),
            results_compacted: false,
            notice: self.notice.clone(),
            pending_request_id: self.pending_request_id,
            pending_preview_request_id: self.pending_preview_request_id,
            pending_action_request_id: self.pending_action_request_id,
            search_in_progress: self.search_in_progress,
            preview_in_progress: self.preview_in_progress,
            action_in_progress: self.action_in_progress,
            scroll_to_current: self.scroll_to_current,
            focus_query_requested: self.focus_query_requested,
            unfocus_query_requested: self.unfocus_query_requested,
        }
    }

    fn apply_tab_state(&mut self, tab: &AppTabState) {
        self.root = tab.root.clone();
        self.use_filelist = tab.use_filelist;
        self.use_regex = tab.use_regex;
        self.ignore_case = tab.ignore_case;
        self.include_files = tab.include_files;
        self.include_dirs = tab.include_dirs;
        self.index = tab.index.clone();
        self.all_entries = Arc::clone(&tab.all_entries);
        self.entries = Arc::clone(&tab.entries);
        self.entry_kinds = tab.entry_kinds.clone();
        self.pending_index_request_id = tab.pending_index_request_id;
        self.index_in_progress = tab.index_in_progress;
        self.pending_index_entries = tab.pending_index_entries.clone();
        self.pending_index_entries_request_id = tab.pending_index_entries_request_id;
        self.pending_kind_paths = tab.pending_kind_paths.clone();
        self.pending_kind_paths_set = tab.pending_kind_paths_set.clone();
        self.in_flight_kind_paths = tab.in_flight_kind_paths.clone();
        self.kind_resolution_epoch = tab.kind_resolution_epoch;
        self.kind_resolution_in_progress = tab.kind_resolution_in_progress;
        self.incremental_filtered_entries = tab.incremental_filtered_entries.clone();
        self.last_incremental_results_refresh = tab.last_incremental_results_refresh;
        self.last_search_snapshot_len = tab.last_search_snapshot_len;
        self.search_resume_pending = tab.search_resume_pending;
        self.search_rerun_pending = tab.search_rerun_pending;
        self.query = tab.query.clone();
        self.reset_query_history_navigation();
        self.query_history_dirty_since = None;
        self.reset_history_search_state();
        self.pending_restore_refresh = tab.pending_restore_refresh;
        self.base_results = tab.base_results.clone();
        self.results = tab.results.clone();
        self.result_sort_mode = tab.result_sort_mode;
        self.pending_sort_request_id = tab.pending_sort_request_id;
        self.sort_in_progress = tab.sort_in_progress;
        self.pinned_paths = tab.pinned_paths.clone();
        self.current_row = tab.current_row;
        self.preview = tab.preview.clone();
        self.notice = tab.notice.clone();
        self.pending_request_id = tab.pending_request_id;
        self.pending_preview_request_id = tab.pending_preview_request_id;
        self.pending_action_request_id = tab.pending_action_request_id;
        self.search_in_progress = tab.search_in_progress;
        self.preview_in_progress = tab.preview_in_progress;
        self.action_in_progress = tab.action_in_progress;
        self.scroll_to_current = tab.scroll_to_current;
        self.focus_query_requested = tab.focus_query_requested;
        self.unfocus_query_requested = tab.unfocus_query_requested;
        self.refresh_status_line();
    }

    fn sync_active_tab_state(&mut self) {
        let Some(id) = self.tabs.get(self.active_tab).map(|tab| tab.id) else {
            return;
        };
        self.commit_query_history_if_needed(true);
        let snapshot = self.capture_active_tab_state(id);
        if let Some(slot) = self.tabs.get_mut(self.active_tab) {
            *slot = snapshot;
        }
    }

    fn find_tab_index_by_id(&self, tab_id: u64) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id == tab_id)
    }

    fn trigger_restore_refresh_for_active_tab(&mut self) {
        if !self.pending_restore_refresh {
            return;
        }
        self.pending_restore_refresh = false;
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.pending_restore_refresh = false;
        }
        self.request_index_refresh();
    }

    fn switch_to_tab_index(&mut self, next_index: usize) {
        if next_index >= self.tabs.len() || next_index == self.active_tab {
            return;
        }
        self.tab_drag_state = None;
        self.shrink_checkpoint_buffers();
        let previous_active = self.active_tab;
        self.sync_active_tab_state();
        if let Some(previous_tab) = self.tabs.get_mut(previous_active) {
            Self::compact_inactive_tab_state(previous_tab);
        }
        if let Some(next_tab) = self.tabs.get_mut(next_index) {
            Self::shrink_tab_checkpoint_buffers(next_tab);
        }
        self.active_tab = next_index;
        if let Some(tab) = self.tabs.get(next_index).cloned() {
            self.apply_tab_state(&tab);
        }
        self.restore_results_from_compacted_tab();
        self.focus_query_requested = true;
        self.unfocus_query_requested = false;
        self.trigger_restore_refresh_for_active_tab();
    }

    fn create_new_tab(&mut self) {
        self.tab_drag_state = None;
        let previous_active = self.active_tab;
        self.sync_active_tab_state();
        if let Some(previous_tab) = self.tabs.get_mut(previous_active) {
            Self::compact_inactive_tab_state(previous_tab);
        }
        let id = self.next_tab_id;
        self.next_tab_id = self.next_tab_id.saturating_add(1);
        let mut tab = self.capture_active_tab_state(id);
        tab.use_filelist = true;
        tab.query.clear();
        tab.query_history = self.query_history.clone();
        tab.query_history_cursor = None;
        tab.query_history_draft = None;
        tab.query_history_dirty_since = None;
        tab.history_search_active = false;
        tab.history_search_query.clear();
        tab.history_search_original_query.clear();
        tab.history_search_results.clear();
        tab.history_search_current = None;
        tab.pending_restore_refresh = false;
        tab.base_results = self
            .entries
            .iter()
            .take(self.limit)
            .cloned()
            .map(|p| (p, 0.0))
            .collect();
        tab.result_sort_mode = ResultSortMode::Score;
        tab.pending_sort_request_id = None;
        tab.sort_in_progress = false;
        tab.pinned_paths.clear();
        tab.current_row = None;
        tab.preview.clear();
        tab.results_compacted = false;
        tab.notice = "Opened new tab".to_string();
        tab.pending_request_id = None;
        tab.pending_preview_request_id = None;
        tab.pending_action_request_id = None;
        tab.pending_index_request_id = None;
        tab.search_in_progress = false;
        tab.index_in_progress = false;
        tab.preview_in_progress = false;
        tab.action_in_progress = false;
        tab.pending_index_entries.clear();
        tab.pending_index_entries_request_id = None;
        tab.pending_kind_paths.clear();
        tab.pending_kind_paths_set.clear();
        tab.in_flight_kind_paths.clear();
        tab.kind_resolution_in_progress = false;
        tab.kind_resolution_epoch = 1;
        tab.incremental_filtered_entries.clear();
        tab.last_search_snapshot_len = tab.entries.len();
        tab.last_incremental_results_refresh = Instant::now();
        tab.search_resume_pending = false;
        tab.search_rerun_pending = false;
        tab.scroll_to_current = true;
        tab.focus_query_requested = true;
        tab.unfocus_query_requested = false;
        tab.results = tab.base_results.clone();
        self.tabs.push(tab.clone());
        self.active_tab = self.tabs.len().saturating_sub(1);
        self.apply_tab_state(&tab);
    }

    fn create_new_tab_for_root(&mut self, root: PathBuf, use_filelist: bool) -> Option<u64> {
        self.create_new_tab();
        self.root = root;
        self.use_filelist = use_filelist;
        self.include_files = true;
        self.include_dirs = true;
        self.sync_active_tab_state();
        self.current_tab_id()
    }

    fn close_active_tab(&mut self) {
        self.close_tab_index(self.active_tab);
    }

    fn close_tab_index(&mut self, index: usize) {
        if self.tabs.len() <= 1 || index >= self.tabs.len() {
            if self.tabs.len() <= 1 {
                self.set_notice("Cannot close the last tab");
            }
            return;
        }
        self.tab_drag_state = None;
        self.sync_active_tab_state();
        let removed = self.tabs.remove(index);
        if self
            .pending_filelist_after_index
            .as_ref()
            .is_some_and(|pending| pending.tab_id == removed.id)
        {
            self.pending_filelist_after_index = None;
        }
        if self
            .pending_filelist_confirmation
            .as_ref()
            .is_some_and(|pending| pending.tab_id == removed.id)
        {
            self.pending_filelist_confirmation = None;
        }
        if self
            .pending_filelist_ancestor_confirmation
            .as_ref()
            .is_some_and(|pending| pending.tab_id == removed.id)
        {
            self.pending_filelist_ancestor_confirmation = None;
        }
        if self
            .pending_filelist_use_walker_confirmation
            .as_ref()
            .is_some_and(|pending| pending.source_tab_id == removed.id)
        {
            self.pending_filelist_use_walker_confirmation = None;
        }
        self.index_request_tabs
            .retain(|_, tab_id| *tab_id != removed.id);
        self.pending_index_queue
            .retain(|req| req.tab_id != removed.id);
        if let Ok(mut latest) = self.latest_index_request_ids.lock() {
            latest.remove(&removed.id);
        }
        self.background_index_states
            .retain(|request_id, _| self.index_request_tabs.contains_key(request_id));
        self.search_request_tabs
            .retain(|_, tab_id| *tab_id != removed.id);
        self.preview_request_tabs
            .retain(|_, tab_id| *tab_id != removed.id);
        self.action_request_tabs
            .retain(|_, tab_id| *tab_id != removed.id);
        self.sort_request_tabs
            .retain(|_, tab_id| *tab_id != removed.id);
        if index < self.active_tab {
            self.active_tab = self.active_tab.saturating_sub(1);
        }
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len().saturating_sub(1);
        }
        self.memory_usage_bytes = None;
        if let Some(tab) = self.tabs.get(self.active_tab).cloned() {
            self.apply_tab_state(&tab);
        }
    }

    fn move_tab(&mut self, from_index: usize, to_index: usize) {
        if from_index >= self.tabs.len() || to_index >= self.tabs.len() || from_index == to_index {
            return;
        }
        self.tab_drag_state = None;
        self.sync_active_tab_state();
        let Some(active_tab_id) = self.tabs.get(self.active_tab).map(|tab| tab.id) else {
            return;
        };
        let moved = self.tabs.remove(from_index);
        self.tabs.insert(to_index, moved);
        if let Some(new_active) = self.find_tab_index_by_id(active_tab_id) {
            self.active_tab = new_active;
        }
        if let Some(tab) = self.tabs.get(self.active_tab).cloned() {
            self.apply_tab_state(&tab);
        }
    }

    fn activate_next_tab(&mut self) {
        if self.tabs.len() <= 1 {
            return;
        }
        let next = (self.active_tab + 1) % self.tabs.len();
        self.switch_to_tab_index(next);
    }

    fn activate_previous_tab(&mut self) {
        if self.tabs.len() <= 1 {
            return;
        }
        let next = if self.active_tab == 0 {
            self.tabs.len() - 1
        } else {
            self.active_tab - 1
        };
        self.switch_to_tab_index(next);
    }

    fn activate_tab_shortcut(&mut self, shortcut_number: usize) {
        let Some(target_index) = shortcut_number.checked_sub(1) else {
            return;
        };
        if target_index >= self.tabs.len() || target_index >= 9 {
            return;
        }
        self.switch_to_tab_index(target_index);
    }

    fn tab_root_label(root: &Path) -> String {
        let normalized = Self::normalize_windows_path(root.to_path_buf());
        let raw = normalized.to_string_lossy().to_string();
        let trimmed = raw.trim_end_matches(['/', '\\']);
        if trimmed.is_empty() {
            return "/".to_string();
        }
        if trimmed.len() == 2 && trimmed.as_bytes().get(1) == Some(&b':') {
            return trimmed.to_string();
        }

        if let Some(name) = normalized.file_name().and_then(|s| s.to_str()) {
            if !name.is_empty() {
                return name.to_string();
            }
        }
        raw
    }

    fn tab_title(&self, tab: &AppTabState, _index: usize) -> String {
        Self::tab_root_label(&tab.root)
    }

    fn any_tab_async_in_progress(&self) -> bool {
        self.tabs.iter().any(|tab| {
            tab.search_in_progress
                || tab.preview_in_progress
                || tab.action_in_progress
                || tab.index_in_progress
                || tab.sort_in_progress
        })
    }

    fn saved_tab_state_from_app(&self) -> SavedTabState {
        SavedTabState {
            root: self.root.to_string_lossy().to_string(),
            use_filelist: self.use_filelist,
            use_regex: self.use_regex,
            ignore_case: self.ignore_case,
            include_files: self.include_files,
            include_dirs: self.include_dirs,
            query: self.query.clone(),
            query_history: if Self::history_persist_disabled() {
                Vec::new()
            } else {
                self.query_history.iter().cloned().collect()
            },
        }
    }

    fn saved_tab_state_from_tab(tab: &AppTabState) -> SavedTabState {
        SavedTabState {
            root: tab.root.to_string_lossy().to_string(),
            use_filelist: tab.use_filelist,
            use_regex: tab.use_regex,
            ignore_case: tab.ignore_case,
            include_files: tab.include_files,
            include_dirs: tab.include_dirs,
            query: tab.query.clone(),
            query_history: if Self::history_persist_disabled() {
                Vec::new()
            } else {
                tab.query_history.iter().cloned().collect()
            },
        }
    }

    fn saved_tabs_for_ui_state(&self) -> Vec<SavedTabState> {
        self.tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                if index == self.active_tab {
                    self.saved_tab_state_from_app()
                } else {
                    Self::saved_tab_state_from_tab(tab)
                }
            })
            .collect()
    }

    fn save_ui_state(&self) {
        let Some(path) = Self::ui_state_file_path() else {
            return;
        };
        self.save_ui_state_to_path(&path);
    }

    fn save_ui_state_to_path(&self, path: &Path) {
        self.save_ui_state_to_path_inner(path, Self::history_persist_disabled());
    }

    #[cfg(test)]
    fn save_ui_state_to_path_with_history_persist_disabled(&self, path: &Path, disabled: bool) {
        self.save_ui_state_to_path_inner(path, disabled);
    }

    fn save_ui_state_to_path_inner(&self, path: &Path, history_persist_disabled: bool) {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let last_root_for_startup = if !Self::restore_tabs_enabled() {
            self.default_root
                .clone()
                .or_else(|| Some(self.root.clone()))
                .unwrap_or_else(|| self.root.clone())
        } else {
            self.root.clone()
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
                .default_root
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
            show_preview: Some(self.show_preview),
            preview_panel_width: Some(self.preview_panel_width),
            query_history: if history_persist_disabled {
                Vec::new()
            } else {
                self.query_history.iter().cloned().collect()
            },
            results_panel_width: None,
            tabs: self.saved_tabs_for_ui_state(),
            active_tab: Some(self.active_tab),
            window: self.window_geometry.clone(),
            skipped_update_target_version: self.skipped_update_target_version.clone(),
        };
        if let Ok(text) = serde_json::to_string_pretty(&state) {
            let _ = write_text_atomic(path, &text);
            Self::append_window_trace(
                "save_ui_state",
                &format!(
                    "window={:?} preview_panel_width={:.1}",
                    state.window, self.preview_panel_width
                ),
            );
        }
    }

    fn mark_ui_state_dirty(&mut self) {
        self.ui_state_dirty = true;
    }

    fn maybe_save_ui_state(&mut self, force: bool) {
        if !self.ui_state_dirty {
            return;
        }
        if force || self.last_ui_state_save.elapsed() >= Self::UI_STATE_SAVE_INTERVAL {
            self.save_ui_state();
            self.ui_state_dirty = false;
            self.last_ui_state_save = Instant::now();
        }
    }

    fn persist_ui_state_now(&mut self) {
        self.save_ui_state();
        self.ui_state_dirty = false;
        self.last_ui_state_save = Instant::now();
    }

    #[cfg(test)]
    fn persist_ui_state_to_path_now(&mut self, path: &Path) {
        self.save_ui_state_to_path(path);
        self.ui_state_dirty = false;
        self.last_ui_state_save = Instant::now();
    }

    fn to_stable_window_geometry(geom: SavedWindowGeometry) -> SavedWindowGeometry {
        let round = |v: f32| (v * 10.0).round() / 10.0;
        let mut width = round(geom.width.max(640.0));
        let mut height = round(geom.height.max(400.0));
        if let Some(mw) = geom.monitor_width {
            // Clamp to current monitor bounds so transient cross-monitor values
            // are not persisted and replayed at the next startup.
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

    fn window_geometry_from_rects(
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

    fn normalize_restore_geometry(saved: SavedWindowGeometry) -> SavedWindowGeometry {
        let mut width = saved.width.max(640.0);
        let mut height = saved.height.max(400.0);
        if let Some(mw) = saved.monitor_width {
            // Use the last known monitor dimensions as a hard upper bound.
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

    fn apply_stable_window_geometry(&mut self, force: bool) {
        let Some(pending) = self.pending_window_geometry.clone() else {
            return;
        };
        if !force
            && self.last_window_geometry_change.elapsed() < Self::WINDOW_GEOMETRY_SETTLE_INTERVAL
        {
            return;
        }
        if self.window_geometry.as_ref() != Some(&pending) {
            self.window_geometry = Some(pending.clone());
            self.mark_ui_state_dirty();
            Self::append_window_trace(
                "window_geometry_committed",
                &format!("committed={:?} force={}", self.window_geometry, force),
            );
        }
        self.pending_window_geometry = None;
    }

    fn capture_window_geometry(&mut self, ctx: &egui::Context) {
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
        if self.pending_window_geometry.as_ref() != Some(&next)
            && self.window_geometry.as_ref() != Some(&next)
        {
            let prev_committed = self.window_geometry.clone();
            let prev_pending = self.pending_window_geometry.clone();
            self.pending_window_geometry = Some(next);
            self.last_window_geometry_change = Instant::now();
            if Self::window_trace_verbose_enabled() {
                Self::append_window_trace(
                    "capture_window_geometry_changed",
                    &format!(
                        "prev_committed={:?} prev_pending={:?} next_pending={:?}",
                        prev_committed, prev_pending, self.pending_window_geometry
                    ),
                );
            }
        }
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

    fn path_key(path: &Path) -> String {
        #[cfg(windows)]
        {
            return path.to_string_lossy().to_string().to_ascii_lowercase();
        }
        #[cfg(not(windows))]
        {
            path.to_string_lossy().to_string()
        }
    }

    fn load_saved_roots() -> Vec<PathBuf> {
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
            let path = Self::normalize_windows_path(PathBuf::from(line));
            let key = Self::path_key(&path);
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

    fn add_current_root_to_saved(&mut self) {
        let root = self
            .root
            .canonicalize()
            .unwrap_or_else(|_| self.root.clone());
        let root = Self::normalize_windows_path(root);
        let key = Self::path_key(&root);
        if self.saved_roots.iter().any(|p| Self::path_key(p) == key) {
            self.set_notice("Current root is already registered");
            return;
        }
        self.saved_roots.push(root.clone());
        self.saved_roots
            .sort_by_key(|p| p.to_string_lossy().to_string().to_ascii_lowercase());
        self.save_saved_roots();
        self.set_notice(format!("Registered root: {}", root.display()));
    }

    fn set_current_root_as_default(&mut self) {
        self.set_current_root_as_default_with(Self::restore_tabs_enabled());
    }

    fn set_current_root_as_default_with(&mut self, restore_tabs_enabled: bool) {
        if !Self::can_set_current_root_as_default_with(restore_tabs_enabled) {
            self.set_notice("Set as default is disabled while FLISTWALKER_RESTORE_TABS is enabled");
            return;
        }
        let root = self
            .root
            .canonicalize()
            .unwrap_or_else(|_| self.root.clone());
        let root = Self::normalize_windows_path(root);
        self.default_root = Some(root.clone());
        self.mark_ui_state_dirty();
        self.persist_ui_state_now();
        self.set_notice(format!("Set default root: {}", root.display()));
    }

    fn can_set_current_root_as_default(&self) -> bool {
        Self::can_set_current_root_as_default_with(Self::restore_tabs_enabled())
    }

    fn can_set_current_root_as_default_with(restore_tabs_enabled: bool) -> bool {
        !restore_tabs_enabled
    }

    fn remove_current_root_from_saved(&mut self) {
        let key = Self::path_key(&self.root);
        let before = self.saved_roots.len();
        self.saved_roots.retain(|p| Self::path_key(p) != key);
        if self.saved_roots.len() == before {
            self.set_notice("Current root is not in saved list");
            return;
        }
        if self
            .default_root
            .as_ref()
            .is_some_and(|p| Self::path_key(p) == key)
        {
            self.default_root = None;
            self.mark_ui_state_dirty();
        }
        self.save_saved_roots();
        self.set_notice("Removed current root from saved list");
    }

    fn cancel_stale_pending_filelist_confirmation(&mut self) {
        let current_tab_id = self.current_tab_id().unwrap_or_default();
        let should_cancel = self
            .pending_filelist_confirmation
            .as_ref()
            .is_some_and(|pending| {
                pending.tab_id == current_tab_id
                    && Self::path_key(&pending.root) != Self::path_key(&self.root)
            });
        if should_cancel {
            self.pending_filelist_confirmation = None;
            self.set_notice("Pending FileList overwrite canceled because root changed");
        }
    }

    fn cancel_stale_pending_filelist_ancestor_confirmation(&mut self) {
        let current_tab_id = self.current_tab_id().unwrap_or_default();
        let should_cancel = self
            .pending_filelist_ancestor_confirmation
            .as_ref()
            .is_some_and(|pending| {
                pending.tab_id == current_tab_id
                    && Self::path_key(&pending.root) != Self::path_key(&self.root)
            });
        if should_cancel {
            self.pending_filelist_ancestor_confirmation = None;
            self.set_notice(
                "Pending Create File List ancestor update canceled because root changed",
            );
        }
    }

    fn cancel_stale_pending_filelist_use_walker_confirmation(&mut self) {
        let current_tab_id = self.current_tab_id().unwrap_or_default();
        let should_cancel = self
            .pending_filelist_use_walker_confirmation
            .as_ref()
            .is_some_and(|pending| {
                pending.source_tab_id == current_tab_id
                    && Self::path_key(&pending.root) != Self::path_key(&self.root)
            });
        if should_cancel {
            self.pending_filelist_use_walker_confirmation = None;
            self.set_notice("Pending Create File List confirmation canceled because root changed");
        }
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
        let normalized = Self::normalize_windows_path(new_root);
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
        let creating_filelist = if self.filelist_in_progress {
            if self.filelist_cancel_requested {
                " | Canceling FileList..."
            } else {
                " | Creating FileList..."
            }
        } else {
            ""
        };
        let updating = if self.update_in_progress {
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

    fn request_index_refresh(&mut self) {
        self.ensure_entry_filters();
        self.invalidate_result_sort(true);
        self.clear_sort_metadata_cache();
        self.pending_restore_refresh = false;
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.pending_restore_refresh = false;
        }
        self.cancel_stale_pending_filelist_confirmation();
        self.cancel_stale_pending_filelist_ancestor_confirmation();
        self.cancel_stale_pending_filelist_use_walker_confirmation();
        let current_tab_id = self.current_tab_id().unwrap_or_default();
        if self
            .pending_filelist_after_index
            .as_ref()
            .is_some_and(|pending| {
                pending.tab_id == current_tab_id
                    && Self::path_key(&pending.root) != Self::path_key(&self.root)
            })
        {
            self.pending_filelist_after_index = None;
            self.set_notice("Deferred Create File List canceled because root changed");
        }
        let request_id = self.next_index_request_id;
        self.next_index_request_id = self.next_index_request_id.saturating_add(1);
        self.pending_index_request_id = Some(request_id);
        if let Some(tab_id) = self.current_tab_id() {
            self.index_request_tabs.insert(request_id, tab_id);
            if let Ok(mut latest) = self.latest_index_request_ids.lock() {
                latest.insert(tab_id, request_id);
            }
        }
        self.index_in_progress = true;
        // Cancel in-flight search requests so responses computed from stale snapshots
        // cannot override results while a new index request is running.
        self.pending_request_id = None;
        self.search_in_progress = false;
        // Non-empty query should resume quickly once fresh index batches arrive.
        self.search_resume_pending = !self.query.trim().is_empty();
        self.search_rerun_pending = false;

        self.index.entries.clear();
        self.index.source = IndexSource::None;
        self.preview_cache.clear();
        self.preview_cache_order.clear();
        self.preview_cache_total_bytes = 0;
        self.highlight_cache.clear();
        self.highlight_cache_order.clear();
        self.incremental_filtered_entries.clear();
        self.pending_index_entries.clear();
        self.pending_index_entries_request_id = None;
        self.pending_kind_paths.clear();
        self.pending_kind_paths_set.clear();
        self.in_flight_kind_paths.clear();
        self.kind_resolution_in_progress = false;
        self.kind_resolution_epoch = self.kind_resolution_epoch.saturating_add(1);
        self.pending_preview_request_id = None;
        self.preview_in_progress = false;
        self.last_incremental_results_refresh = Instant::now();
        self.last_search_snapshot_len = 0;
        self.refresh_status_line();

        let req = IndexRequest {
            request_id,
            tab_id: self.current_tab_id().unwrap_or_default(),
            root: self.root.clone(),
            use_filelist: self.use_filelist,
            include_files: self.include_files,
            include_dirs: self.include_dirs,
        };
        self.enqueue_index_request(req);
        self.dispatch_index_queue();
    }

    fn maybe_reindex_from_filter_toggles(
        &mut self,
        use_filelist_changed: bool,
        files_changed: bool,
        dirs_changed: bool,
    ) {
        let mut reindex = use_filelist_changed;
        reindex |= files_changed || dirs_changed;
        if self.use_filelist_requires_locked_filters()
            && (!self.include_files || !self.include_dirs)
        {
            self.include_files = true;
            self.include_dirs = true;
            reindex = true;
        }
        reindex |= self.ensure_entry_filters();
        if reindex {
            self.request_index_refresh();
        }
    }

    fn enqueue_index_request(&mut self, req: IndexRequest) {
        let active_tab_id = self.current_tab_id().unwrap_or_default();
        let stale_inflight: Vec<u64> = self
            .index_inflight_requests
            .iter()
            .copied()
            .filter(|request_id| {
                self.index_request_tabs
                    .get(request_id)
                    .is_some_and(|tab_id| *tab_id == req.tab_id)
            })
            .collect();
        for request_id in stale_inflight {
            self.index_inflight_requests.remove(&request_id);
            self.index_request_tabs.remove(&request_id);
            self.background_index_states.remove(&request_id);
        }
        self.pending_index_queue
            .retain(|queued| queued.tab_id != req.tab_id);
        self.pending_index_queue.push_back(req);

        while self.pending_index_queue.len() > Self::INDEX_MAX_QUEUE {
            let drop_idx = self
                .pending_index_queue
                .iter()
                .position(|queued| queued.tab_id != active_tab_id)
                .unwrap_or(0);
            let dropped = self.pending_index_queue.remove(drop_idx);
            if let Some(dropped) = dropped {
                if let Some(tab_index) = self.find_tab_index_by_id(dropped.tab_id) {
                    if let Some(tab) = self.tabs.get_mut(tab_index) {
                        if tab.pending_index_request_id == Some(dropped.request_id) {
                            tab.pending_index_request_id = None;
                            tab.index_in_progress = false;
                            tab.notice = "Index request dropped due to queue limit".to_string();
                        }
                    }
                }
                self.index_request_tabs.remove(&dropped.request_id);
                self.background_index_states.remove(&dropped.request_id);
            }
        }
    }

    fn queued_request_for_tab_exists(&self, tab_id: u64) -> bool {
        self.pending_index_queue
            .iter()
            .any(|req| req.tab_id == tab_id)
    }

    fn has_inflight_for_tab(&self, tab_id: u64) -> bool {
        self.index_inflight_requests.iter().any(|request_id| {
            self.index_request_tabs
                .get(request_id)
                .is_some_and(|rid_tab_id| *rid_tab_id == tab_id)
        })
    }

    fn pop_next_index_request(&mut self) -> Option<IndexRequest> {
        let active_tab_id = self.current_tab_id()?;
        if let Some(pos) = self
            .pending_index_queue
            .iter()
            .position(|req| req.tab_id == active_tab_id && !self.has_inflight_for_tab(req.tab_id))
        {
            return self.pending_index_queue.remove(pos);
        }
        if let Some(pos) = self
            .pending_index_queue
            .iter()
            .position(|req| !self.has_inflight_for_tab(req.tab_id))
        {
            return self.pending_index_queue.remove(pos);
        }
        None
    }

    fn preempt_background_for_active_request(&mut self) -> bool {
        let Some(active_tab_id) = self.current_tab_id() else {
            return false;
        };
        if !self.queued_request_for_tab_exists(active_tab_id) {
            return false;
        }
        if self.index_inflight_requests.len() < Self::INDEX_MAX_CONCURRENT {
            return false;
        }

        let victim_request_id = self
            .index_inflight_requests
            .iter()
            .copied()
            .find(|request_id| {
                self.index_request_tabs
                    .get(request_id)
                    .is_some_and(|tab_id| *tab_id != active_tab_id)
            });
        let Some(victim_request_id) = victim_request_id else {
            return false;
        };
        let Some(victim_tab_id) = self.index_request_tabs.get(&victim_request_id).copied() else {
            return false;
        };
        let replacement_request_id = self
            .pending_index_queue
            .iter()
            .rev()
            .find(|req| req.tab_id == victim_tab_id)
            .map(|req| req.request_id)
            .unwrap_or(0);

        if let Ok(mut latest) = self.latest_index_request_ids.lock() {
            if latest.get(&victim_tab_id).copied() == Some(replacement_request_id) {
                return false;
            }
            latest.insert(victim_tab_id, replacement_request_id);
            return true;
        }
        false
    }

    fn dispatch_index_queue(&mut self) {
        loop {
            if self.index_inflight_requests.len() >= Self::INDEX_MAX_CONCURRENT {
                let _ = self.preempt_background_for_active_request();
                break;
            }
            let Some(req) = self.pop_next_index_request() else {
                break;
            };
            let req_id = req.request_id;
            if self.index_tx.send(req).is_err() {
                self.index_request_tabs.clear();
                self.pending_index_queue.clear();
                self.background_index_states.clear();
                self.index_inflight_requests.clear();
                self.index_in_progress = false;
                self.pending_index_request_id = None;
                self.set_notice("Index worker is unavailable");
                break;
            } else {
                self.index_inflight_requests.insert(req_id);
            }
        }
    }

    fn enqueue_search_request_for_tab_index(&mut self, tab_index: usize) {
        let Some(tab) = self.tabs.get_mut(tab_index) else {
            return;
        };
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1);
        tab.pending_request_id = Some(request_id);
        tab.search_in_progress = true;
        self.search_request_tabs.insert(request_id, tab.id);

        let req = SearchRequest {
            request_id,
            query: tab.query.clone(),
            entries: Arc::clone(&tab.entries),
            limit: self.limit,
            use_regex: tab.use_regex,
            ignore_case: tab.ignore_case,
            root: tab.root.clone(),
            prefer_relative: Self::prefer_relative_display_for(&tab.index.source),
        };
        if self.search_tx.send(req).is_err() {
            tab.pending_request_id = None;
            tab.search_in_progress = false;
            tab.notice = "Search worker is unavailable".to_string();
        }
    }

    fn handle_background_index_response(&mut self, tab_index: usize, msg: IndexResponse) {
        let mut trigger_search = false;
        let mut cleanup_request_id: Option<u64> = None;
        let mut deferred_filelist: Option<(u64, PathBuf, Vec<PathBuf>)> = None;

        {
            let Some(tab) = self.tabs.get_mut(tab_index) else {
                return;
            };
            match msg {
                IndexResponse::Started { request_id, source } => {
                    if tab.pending_index_request_id != Some(request_id) {
                        return;
                    }
                    tab.index.source = source.clone();
                    self.background_index_states
                        .entry(request_id)
                        .or_default()
                        .source = Some(source);
                }
                IndexResponse::Batch {
                    request_id,
                    entries,
                } => {
                    if tab.pending_index_request_id != Some(request_id) {
                        return;
                    }
                    let state = self.background_index_states.entry(request_id).or_default();
                    for entry in entries {
                        if entry.kind_known {
                            state.entry_kinds.insert(entry.path.clone(), entry.kind);
                        }
                        state.entries.push(entry.path);
                    }
                }
                IndexResponse::ReplaceAll {
                    request_id,
                    entries,
                } => {
                    if tab.pending_index_request_id != Some(request_id) {
                        return;
                    }
                    let state = self.background_index_states.entry(request_id).or_default();
                    state.entries.clear();
                    state.entry_kinds.clear();
                    for entry in entries {
                        if entry.kind_known {
                            state.entry_kinds.insert(entry.path.clone(), entry.kind);
                        }
                        state.entries.push(entry.path);
                    }
                }
                IndexResponse::Finished { request_id, source } => {
                    if tab.pending_index_request_id != Some(request_id) {
                        cleanup_request_id = Some(request_id);
                    } else {
                        let state = self
                            .background_index_states
                            .remove(&request_id)
                            .unwrap_or_default();
                        tab.index.source = state.source.unwrap_or(source);
                        tab.index.entries.clear();
                        tab.all_entries = Arc::new(state.entries);
                        tab.entry_kinds = state.entry_kinds;
                        if tab.include_files && tab.include_dirs {
                            tab.entries = Arc::clone(&tab.all_entries);
                        } else {
                            let filtered: Vec<PathBuf> = tab
                                .all_entries
                                .iter()
                                .filter(|path| {
                                    Self::is_entry_visible_for_flags(
                                        &tab.entry_kinds,
                                        path,
                                        tab.include_files,
                                        tab.include_dirs,
                                    )
                                })
                                .cloned()
                                .collect();
                            tab.entries = Arc::new(filtered);
                        }
                        tab.pending_index_request_id = None;
                        tab.index_in_progress = false;
                        tab.pending_index_entries.clear();
                        tab.pending_index_entries_request_id = None;
                        tab.pending_kind_paths.clear();
                        tab.pending_kind_paths_set.clear();
                        tab.in_flight_kind_paths.clear();
                        tab.kind_resolution_in_progress = false;
                        tab.search_resume_pending = false;
                        tab.search_rerun_pending = false;
                        tab.last_search_snapshot_len = tab.entries.len();
                        tab.last_incremental_results_refresh = Instant::now();
                        if self
                            .pending_filelist_after_index
                            .as_ref()
                            .is_some_and(|pending| {
                                pending.tab_id == tab.id
                                    && Self::path_key(&pending.root) == Self::path_key(&tab.root)
                            })
                        {
                            deferred_filelist =
                                Some((tab.id, tab.root.clone(), tab.all_entries.as_ref().clone()));
                            self.pending_filelist_after_index = None;
                        }

                        if tab.query.trim().is_empty() {
                            tab.results = tab
                                .entries
                                .iter()
                                .take(self.limit)
                                .cloned()
                                .map(|p| (p, 0.0))
                                .collect();
                            if tab.results.is_empty() {
                                tab.current_row = None;
                                tab.preview.clear();
                                tab.pending_preview_request_id = None;
                                tab.preview_in_progress = false;
                            } else {
                                let max_index = tab.results.len().saturating_sub(1);
                                tab.current_row = Some(tab.current_row.unwrap_or(0).min(max_index));
                            }
                        } else {
                            trigger_search = true;
                        }
                        Self::shrink_tab_checkpoint_buffers(tab);
                        cleanup_request_id = Some(request_id);
                    }
                }
                IndexResponse::Failed { request_id, error } => {
                    if tab.pending_index_request_id != Some(request_id) {
                        cleanup_request_id = Some(request_id);
                    } else {
                        tab.index_in_progress = false;
                        tab.pending_index_request_id = None;
                        tab.search_resume_pending = false;
                        tab.search_rerun_pending = false;
                        tab.pending_index_entries.clear();
                        tab.pending_index_entries_request_id = None;
                        Self::shrink_tab_checkpoint_buffers(tab);
                        tab.notice = format!("Indexing failed: {}", error);
                        cleanup_request_id = Some(request_id);
                    }
                }
                IndexResponse::Canceled { request_id } => {
                    if tab.pending_index_request_id == Some(request_id) {
                        tab.index_in_progress = false;
                        tab.pending_index_request_id = None;
                        tab.search_resume_pending = false;
                        tab.search_rerun_pending = false;
                        tab.pending_index_entries.clear();
                        tab.pending_index_entries_request_id = None;
                        Self::shrink_tab_checkpoint_buffers(tab);
                    }
                    cleanup_request_id = Some(request_id);
                }
                IndexResponse::Truncated { request_id, limit } => {
                    if tab.pending_index_request_id == Some(request_id) {
                        tab.notice = format!(
                            "Walker capped at {} entries (set FLISTWALKER_WALKER_MAX_ENTRIES to adjust)",
                            limit
                        );
                    }
                }
            }
        }

        if let Some((tab_id, root, entries)) = deferred_filelist {
            self.request_filelist_creation(tab_id, root, entries);
        }
        if trigger_search {
            self.enqueue_search_request_for_tab_index(tab_index);
        }
        if let Some(request_id) = cleanup_request_id {
            self.index_request_tabs.remove(&request_id);
            self.background_index_states.remove(&request_id);
            self.index_inflight_requests.remove(&request_id);
        }
        self.dispatch_index_queue();
    }

    fn poll_index_response(&mut self) {
        const MAX_MESSAGES_PER_FRAME: usize = 12;
        const FRAME_BUDGET: Duration = Duration::from_millis(4);
        const MAX_INDEX_ENTRIES_PER_FRAME: usize = 256;

        let frame_start = Instant::now();
        let mut processed = 0usize;
        let mut has_index_progress = false;
        let mut finished_current_request = false;
        while let Ok(msg) = self.index_rx.try_recv() {
            let request_id = match &msg {
                IndexResponse::Started { request_id, .. }
                | IndexResponse::Batch { request_id, .. }
                | IndexResponse::ReplaceAll { request_id, .. }
                | IndexResponse::Finished { request_id, .. }
                | IndexResponse::Failed { request_id, .. }
                | IndexResponse::Canceled { request_id }
                | IndexResponse::Truncated { request_id, .. } => *request_id,
            };
            let target_tab_id = self.index_request_tabs.get(&request_id).copied();
            let current_tab_id = self.current_tab_id();
            if let Some(tab_id) = target_tab_id {
                if Some(tab_id) != current_tab_id {
                    if let Some(tab_index) = self.find_tab_index_by_id(tab_id) {
                        self.handle_background_index_response(tab_index, msg);
                    } else {
                        self.index_request_tabs.remove(&request_id);
                        self.background_index_states.remove(&request_id);
                        self.index_inflight_requests.remove(&request_id);
                    }
                    processed = processed.saturating_add(1);
                    if processed >= MAX_MESSAGES_PER_FRAME || frame_start.elapsed() >= FRAME_BUDGET
                    {
                        break;
                    }
                    continue;
                }
            }

            let terminal_request_id = match &msg {
                IndexResponse::Finished { request_id, .. }
                | IndexResponse::Failed { request_id, .. }
                | IndexResponse::Canceled { request_id } => Some(*request_id),
                _ => None,
            };

            match msg {
                IndexResponse::Started { request_id, source } => {
                    if Some(request_id) != self.pending_index_request_id {
                        continue;
                    }
                    self.index.source = source;
                    self.refresh_status_line();
                }
                IndexResponse::Batch {
                    request_id,
                    entries,
                } => {
                    if Some(request_id) != self.pending_index_request_id {
                        continue;
                    }
                    self.queue_index_batch(request_id, entries);
                    has_index_progress = true;
                }
                IndexResponse::ReplaceAll {
                    request_id,
                    entries,
                } => {
                    if Some(request_id) != self.pending_index_request_id {
                        continue;
                    }
                    self.pending_index_entries.clear();
                    self.pending_index_entries_request_id = None;
                    self.index.entries.clear();
                    self.incremental_filtered_entries.clear();
                    self.entry_kinds.clear();
                    self.queue_index_batch(request_id, entries);
                    has_index_progress = true;
                }
                IndexResponse::Finished { request_id, source } => {
                    if Some(request_id) != self.pending_index_request_id {
                        self.index_request_tabs.remove(&request_id);
                        self.background_index_states.remove(&request_id);
                        self.index_inflight_requests.remove(&request_id);
                        continue;
                    }
                    self.drain_queued_index_entries(request_id, usize::MAX);
                    self.index.source = source;
                    self.all_entries = Arc::new(std::mem::take(&mut self.index.entries));
                    self.last_search_snapshot_len = self.all_entries.len();
                    self.incremental_filtered_entries.clear();
                    self.pending_index_entries.clear();
                    self.pending_index_entries_request_id = None;
                    self.pending_index_request_id = None;
                    self.index_request_tabs.remove(&request_id);
                    self.background_index_states.remove(&request_id);
                    self.index_in_progress = false;
                    self.apply_entry_filters(true);
                    self.search_resume_pending = false;
                    self.search_rerun_pending = false;
                    self.clear_notice();
                    let current_tab_id = self.current_tab_id().unwrap_or_default();
                    if self
                        .pending_filelist_after_index
                        .as_ref()
                        .is_some_and(|pending| {
                            pending.tab_id == current_tab_id
                                && Self::path_key(&pending.root) == Self::path_key(&self.root)
                        })
                    {
                        let root = self.root.clone();
                        let entries = self.filelist_entries_snapshot();
                        self.pending_filelist_after_index = None;
                        self.request_filelist_creation(current_tab_id, root, entries);
                    }
                    self.shrink_checkpoint_buffers();
                    self.index_inflight_requests.remove(&request_id);
                    finished_current_request = true;
                    break;
                }
                IndexResponse::Failed { request_id, error } => {
                    if Some(request_id) != self.pending_index_request_id {
                        self.index_request_tabs.remove(&request_id);
                        self.background_index_states.remove(&request_id);
                        self.index_inflight_requests.remove(&request_id);
                        continue;
                    }
                    self.index_in_progress = false;
                    self.pending_index_request_id = None;
                    self.search_resume_pending = false;
                    self.search_rerun_pending = false;
                    self.pending_filelist_after_index = None;
                    self.pending_index_entries.clear();
                    self.pending_index_entries_request_id = None;
                    self.index_request_tabs.remove(&request_id);
                    self.background_index_states.remove(&request_id);
                    self.shrink_checkpoint_buffers();
                    self.set_notice(format!("Indexing failed: {}", error));
                }
                IndexResponse::Canceled { request_id } => {
                    if Some(request_id) == self.pending_index_request_id {
                        self.index_in_progress = false;
                        self.pending_index_request_id = None;
                        self.search_resume_pending = false;
                        self.search_rerun_pending = false;
                        self.pending_index_entries.clear();
                        self.pending_index_entries_request_id = None;
                        self.shrink_checkpoint_buffers();
                    }
                    self.index_request_tabs.remove(&request_id);
                    self.background_index_states.remove(&request_id);
                }
                IndexResponse::Truncated { request_id, limit } => {
                    if Some(request_id) == self.pending_index_request_id {
                        self.set_notice(format!(
                            "Walker capped at {} entries (set FLISTWALKER_WALKER_MAX_ENTRIES to adjust)",
                            limit
                        ));
                    }
                }
            }

            if let Some(request_id) = terminal_request_id {
                self.index_inflight_requests.remove(&request_id);
            }

            processed = processed.saturating_add(1);
            if processed >= MAX_MESSAGES_PER_FRAME || frame_start.elapsed() >= FRAME_BUDGET {
                break;
            }
        }

        if finished_current_request {
            self.dispatch_index_queue();
            return;
        }

        if let Some(request_id) = self.pending_index_request_id {
            let remaining_budget = FRAME_BUDGET.saturating_sub(frame_start.elapsed());
            let consumed = if remaining_budget.is_zero() {
                // Avoid starvation when message handling consumed this frame budget.
                self.drain_queued_index_entries(request_id, 32)
            } else {
                self.drain_queued_index_entries_with_budget(
                    request_id,
                    Instant::now(),
                    remaining_budget,
                    MAX_INDEX_ENTRIES_PER_FRAME,
                )
            };
            has_index_progress |= consumed;
        }

        if !has_index_progress {
            self.dispatch_index_queue();
            return;
        }

        if self.query.trim().is_empty() {
            self.apply_incremental_empty_query_results();
        } else {
            self.maybe_refresh_incremental_search();
        }
        self.dispatch_index_queue();
    }

    fn ensure_entry_filters(&mut self) -> bool {
        if !self.include_files && !self.include_dirs {
            self.include_files = true;
            return true;
        }
        false
    }

    fn apply_results_with_selection_policy(
        &mut self,
        results: Vec<(PathBuf, f64)>,
        keep_scroll_position: bool,
        preserve_selected_path: bool,
    ) {
        fn clamp_row(current_row: Option<usize>, results_len: usize) -> Option<usize> {
            current_row.map(|row| row.min(results_len.saturating_sub(1)))
        }

        let selected_path = preserve_selected_path
            .then(|| {
                self.current_row
                    .and_then(|row| self.results.get(row).map(|(path, _)| path.clone()))
            })
            .flatten();
        let previous_row = self.current_row;
        self.results = results;
        if self.results.is_empty() {
            self.current_row = None;
            self.preview.clear();
            self.preview_in_progress = false;
            self.pending_preview_request_id = None;
        } else {
            let previous_row = clamp_row(previous_row, self.results.len());
            self.current_row = selected_path
                .and_then(|selected| self.results.iter().position(|(path, _)| *path == selected))
                .or(previous_row);
            self.request_preview_for_current();
            if !keep_scroll_position {
                self.scroll_to_current = true;
            }
        }
        self.refresh_status_line();
    }

    fn enqueue_search_request(&mut self) {
        self.commit_query_history_if_needed(false);
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1);
        self.pending_request_id = Some(request_id);
        if let Some(tab_id) = self.current_tab_id() {
            self.search_request_tabs.insert(request_id, tab_id);
        }
        self.search_in_progress = true;
        self.refresh_status_line();

        let req = SearchRequest {
            request_id,
            query: self.query.clone(),
            entries: Arc::clone(&self.entries),
            limit: self.limit,
            use_regex: self.use_regex,
            ignore_case: self.ignore_case,
            root: self.root.clone(),
            prefer_relative: self.prefer_relative_display(),
        };

        if self.search_tx.send(req).is_err() {
            self.pending_request_id = None;
            self.search_in_progress = false;
            self.set_notice("Search worker is unavailable");
        }
    }

    fn poll_search_response(&mut self) {
        while let Ok(response) = self.search_rx.try_recv() {
            let target_tab_id = self.search_request_tabs.remove(&response.request_id);
            if Some(response.request_id) == self.pending_request_id {
                self.pending_request_id = None;
                self.search_in_progress = false;
                if let Some(error) = response.error {
                    self.set_notice(format!("Search failed: {error}"));
                } else {
                    self.clear_notice();
                }
                self.replace_results_snapshot(response.results, false);
                if self.search_rerun_pending
                    && !self.query.trim().is_empty()
                    && self.index_in_progress
                    && self.should_refresh_incremental_search()
                {
                    self.search_rerun_pending = false;
                    self.search_resume_pending = false;
                    self.sync_entries_from_incremental();
                    self.last_search_snapshot_len = self.entries.len();
                    self.last_incremental_results_refresh = Instant::now();
                    self.update_results();
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
            tab.pending_request_id = None;
            tab.search_in_progress = false;
            tab.notice = response
                .error
                .map(|error| format!("Search failed: {error}"))
                .unwrap_or_default();
            tab.base_results = response.results.clone();
            tab.results = response.results;
            tab.results_compacted = false;
            tab.result_sort_mode = ResultSortMode::Score;
            tab.pending_sort_request_id = None;
            tab.sort_in_progress = false;
            if tab.results.is_empty() {
                tab.current_row = None;
                tab.preview.clear();
                tab.pending_preview_request_id = None;
                tab.preview_in_progress = false;
            } else {
                let max_index = tab.results.len().saturating_sub(1);
                tab.current_row = tab.current_row.map(|row| row.min(max_index));
            }
            Self::compact_inactive_tab_state(tab);
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
            if Some(response.request_id) != tab.pending_sort_request_id {
                continue;
            }
            tab.pending_sort_request_id = None;
            tab.sort_in_progress = false;
            if response.mode == tab.result_sort_mode {
                tab.results = Self::build_sorted_results_from(
                    &tab.base_results,
                    tab.result_sort_mode,
                    &self.sort_metadata_cache,
                );
                tab.results_compacted = false;
                if tab.results.is_empty() {
                    tab.current_row = None;
                    tab.preview.clear();
                    tab.pending_preview_request_id = None;
                    tab.preview_in_progress = false;
                } else {
                    let max_index = tab.results.len().saturating_sub(1);
                    tab.current_row = tab.current_row.map(|row| row.min(max_index));
                }
                Self::compact_inactive_tab_state(tab);
            }
        }
    }

    fn poll_preview_response(&mut self) {
        while let Ok(response) = self.preview_rx.try_recv() {
            let target_tab_id = self.preview_request_tabs.remove(&response.request_id);
            if Some(response.request_id) == self.pending_preview_request_id {
                self.pending_preview_request_id = None;
                self.preview_in_progress = false;
                self.cache_preview(response.path.clone(), response.preview.clone());
                if let Some(row) = self.current_row {
                    if let Some((current_path, _)) = self.results.get(row) {
                        if *current_path == response.path {
                            self.preview = response.preview;
                        }
                    }
                }
                continue;
            }
            let Some(tab_id) = target_tab_id else {
                continue;
            };
            let Some(tab_index) = self.find_tab_index_by_id(tab_id) else {
                continue;
            };
            self.cache_preview(response.path.clone(), response.preview.clone());
            if let Some(tab) = self.tabs.get_mut(tab_index) {
                tab.pending_preview_request_id = None;
                tab.preview_in_progress = false;
                let current_path = if tab.results_compacted {
                    tab.current_row
                        .and_then(|row| tab.base_results.get(row).map(|(path, _)| path))
                } else {
                    tab.current_row
                        .and_then(|row| tab.results.get(row).map(|(path, _)| path))
                };
                if current_path.is_some_and(|current_path| *current_path == response.path) {
                    tab.preview = response.preview;
                }
            }
        }
    }

    fn cache_preview(&mut self, path: PathBuf, preview: String) {
        let new_bytes = preview.len();
        if let Some(old) = self.preview_cache.get(&path) {
            self.preview_cache_total_bytes =
                self.preview_cache_total_bytes.saturating_sub(old.len());
        }
        if !self.preview_cache.contains_key(&path) {
            self.preview_cache_order.push_back(path.clone());
        }
        self.preview_cache.insert(path, preview);
        self.preview_cache_total_bytes = self.preview_cache_total_bytes.saturating_add(new_bytes);
        // Keep cache bounded so long browse sessions do not grow memory unbounded.
        while self.preview_cache_total_bytes > Self::PREVIEW_CACHE_MAX_BYTES {
            if let Some(oldest) = self.preview_cache_order.pop_front() {
                if let Some(evicted) = self.preview_cache.remove(&oldest) {
                    self.preview_cache_total_bytes =
                        self.preview_cache_total_bytes.saturating_sub(evicted.len());
                }
            } else {
                break;
            }
        }
    }

    fn clear_highlight_cache(&mut self) {
        self.highlight_cache.clear();
        self.highlight_cache_order.clear();
    }

    fn ensure_highlight_cache_scope(&mut self, prefer_relative: bool) {
        if self.highlight_cache_scope_query == self.query
            && Self::path_key(&self.highlight_cache_scope_root) == Self::path_key(&self.root)
            && self.highlight_cache_scope_use_regex == self.use_regex
            && self.highlight_cache_scope_ignore_case == self.ignore_case
            && self.highlight_cache_scope_prefer_relative == prefer_relative
        {
            return;
        }
        self.highlight_cache_scope_query = self.query.clone();
        self.highlight_cache_scope_root = self.root.clone();
        self.highlight_cache_scope_use_regex = self.use_regex;
        self.highlight_cache_scope_ignore_case = self.ignore_case;
        self.highlight_cache_scope_prefer_relative = prefer_relative;
        self.clear_highlight_cache();
    }

    fn cache_highlight_positions_for_key(&mut self, key: HighlightCacheKey, positions: Vec<u16>) {
        if !self.highlight_cache.contains_key(&key) {
            self.highlight_cache_order.push_back(key.clone());
        }
        self.highlight_cache.insert(key, Arc::new(positions));
        while self.highlight_cache_order.len() > Self::HIGHLIGHT_CACHE_MAX {
            if let Some(oldest) = self.highlight_cache_order.pop_front() {
                self.highlight_cache.remove(&oldest);
            }
        }
    }

    fn compact_highlight_positions(positions: HashSet<usize>) -> Vec<u16> {
        let mut compact = positions
            .into_iter()
            .filter_map(|idx| u16::try_from(idx).ok())
            .collect::<Vec<_>>();
        compact.sort_unstable();
        compact.dedup();
        compact
    }

    fn highlight_positions_for_path_cached(
        &mut self,
        path: &Path,
        prefer_relative: bool,
    ) -> Arc<Vec<u16>> {
        static EMPTY: OnceLock<Arc<Vec<u16>>> = OnceLock::new();

        self.ensure_highlight_cache_scope(prefer_relative);
        if self.query.trim().is_empty() {
            return Arc::clone(EMPTY.get_or_init(|| Arc::new(Vec::new())));
        }

        let key = HighlightCacheKey {
            path: path.to_path_buf(),
            prefer_relative,
            use_regex: self.use_regex,
            ignore_case: self.ignore_case,
        };

        if let Some(positions) = self.highlight_cache.get(&key) {
            return Arc::clone(positions);
        }

        let positions = Self::compact_highlight_positions(match_positions_for_path(
            path,
            &self.root,
            &self.query,
            prefer_relative,
            self.use_regex,
            self.ignore_case,
        ));
        self.cache_highlight_positions_for_key(key.clone(), positions);
        self.highlight_cache
            .get(&key)
            .cloned()
            .unwrap_or_else(|| Arc::clone(EMPTY.get_or_init(|| Arc::new(Vec::new()))))
    }

    fn is_highlighted_position(positions: &[u16], idx: usize) -> bool {
        let Ok(idx16) = u16::try_from(idx) else {
            return false;
        };
        positions.binary_search(&idx16).is_ok()
    }

    fn update_results(&mut self) {
        if self.query.trim().is_empty() {
            self.pending_request_id = None;
            self.search_in_progress = false;
            let results = self
                .entries
                .iter()
                .take(self.limit)
                .cloned()
                .map(|p| (p, 0.0))
                .collect();
            self.replace_results_snapshot(results, false);
            return;
        }
        self.enqueue_search_request();
    }

    fn queue_index_batch(&mut self, request_id: u64, entries: Vec<IndexEntry>) {
        if self.pending_index_entries_request_id != Some(request_id) {
            self.pending_index_entries.clear();
            self.pending_index_entries_request_id = Some(request_id);
        }
        self.pending_index_entries.extend(entries);
    }

    fn ingest_index_entry(&mut self, entry: IndexEntry) {
        if entry.kind_known {
            self.entry_kinds.insert(entry.path.clone(), entry.kind);
        } else {
            self.entry_kinds.remove(&entry.path);
            if self.kind_resolution_needed_for_filters() {
                self.queue_kind_resolution(entry.path.clone());
            }
        }
        self.index.entries.push(entry.path.clone());
        if self.is_entry_visible_for_current_filter(&entry.path) {
            self.incremental_filtered_entries.push(entry.path);
        }
    }

    fn drain_queued_index_entries(&mut self, request_id: u64, max_entries: usize) -> bool {
        if self.pending_index_entries_request_id != Some(request_id) {
            return false;
        }
        let mut processed = 0usize;
        while processed < max_entries {
            let Some(entry) = self.pending_index_entries.pop_front() else {
                break;
            };
            self.ingest_index_entry(entry);
            processed = processed.saturating_add(1);
        }
        if self.pending_index_entries.is_empty() {
            self.pending_index_entries_request_id = None;
        }
        processed > 0
    }

    fn drain_queued_index_entries_with_budget(
        &mut self,
        request_id: u64,
        frame_start: Instant,
        budget: Duration,
        max_entries: usize,
    ) -> bool {
        if self.pending_index_entries_request_id != Some(request_id) {
            return false;
        }
        let mut processed = 0usize;
        while processed < max_entries && frame_start.elapsed() < budget {
            let Some(entry) = self.pending_index_entries.pop_front() else {
                break;
            };
            self.ingest_index_entry(entry);
            processed = processed.saturating_add(1);
        }
        if self.pending_index_entries.is_empty() {
            self.pending_index_entries_request_id = None;
        }
        processed > 0
    }

    fn sync_entries_from_incremental(&mut self) {
        self.entries = Arc::new(self.incremental_filtered_entries.clone());
    }

    fn apply_incremental_empty_query_results(&mut self) {
        self.sync_entries_from_incremental();
        self.pending_request_id = None;
        self.search_in_progress = false;
        let results = self
            .entries
            .iter()
            .take(self.limit)
            .cloned()
            .map(|p| (p, 0.0))
            .collect();
        self.replace_results_snapshot(results, true);
    }

    fn maybe_refresh_incremental_search(&mut self) {
        if self.query.trim().is_empty() {
            return;
        }

        if self.search_resume_pending {
            if self.search_in_progress {
                self.search_rerun_pending = true;
                return;
            }
            self.sync_entries_from_incremental();
            self.last_search_snapshot_len = self.entries.len();
            self.last_incremental_results_refresh = Instant::now();
            self.update_results();
            self.search_resume_pending = false;
            return;
        }

        let current_len = self.incremental_filtered_entries.len();
        if self.should_refresh_incremental_search() {
            if self.search_in_progress {
                self.search_rerun_pending = true;
                return;
            }
            self.sync_entries_from_incremental();
            self.last_search_snapshot_len = current_len;
            self.last_incremental_results_refresh = Instant::now();
            self.update_results();
        }
    }

    fn should_refresh_incremental_search(&self) -> bool {
        let current_len = self.incremental_filtered_entries.len();
        let delta = current_len.saturating_sub(self.last_search_snapshot_len);
        if delta == 0 {
            return false;
        }
        if self.index_in_progress {
            if delta < Self::INCREMENTAL_SEARCH_MIN_DELTA_DURING_INDEX {
                return false;
            }
            return self.last_incremental_results_refresh.elapsed()
                >= Self::INCREMENTAL_SEARCH_REFRESH_INTERVAL_DURING_INDEX;
        }
        self.last_incremental_results_refresh.elapsed() >= Self::INCREMENTAL_SEARCH_REFRESH_INTERVAL
    }

    fn filtered_entries(&self, source: &[PathBuf]) -> Vec<PathBuf> {
        source
            .iter()
            .filter(|path| self.is_entry_visible_for_current_filter(path))
            .cloned()
            .collect()
    }

    fn apply_entry_filters(&mut self, keep_scroll_position: bool) {
        if self.kind_resolution_needed_for_filters() {
            self.queue_unknown_kind_paths_for_active_entries();
        } else if !self.pending_kind_paths.is_empty() || !self.in_flight_kind_paths.is_empty() {
            self.reset_kind_resolution_state();
        }

        let source_is_all_entries = !self.index_in_progress || self.index.entries.is_empty();
        let base = if !source_is_all_entries {
            &self.index.entries
        } else {
            self.all_entries.as_ref()
        };
        if source_is_all_entries && self.include_files && self.include_dirs {
            self.entries = Arc::clone(&self.all_entries);
        } else {
            self.entries = Arc::new(self.filtered_entries(base));
        }
        if self.index_in_progress {
            self.incremental_filtered_entries = self.entries.as_ref().clone();
        } else {
            self.incremental_filtered_entries.clear();
        }
        self.last_search_snapshot_len = self.entries.len();
        self.search_rerun_pending = false;

        if self.query.trim().is_empty() {
            self.pending_request_id = None;
            self.search_in_progress = false;
            let results = self
                .entries
                .iter()
                .take(self.limit)
                .cloned()
                .map(|p| (p, 0.0))
                .collect();
            self.replace_results_snapshot(results, keep_scroll_position);
        } else {
            self.update_results();
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

    fn current_result_kind(&self) -> Option<EntryKind> {
        let row = self.current_row?;
        let (path, _) = self.results.get(row)?;
        self.entry_kinds.get(path).copied()
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
        for path in source {
            if !self.entry_kinds.contains_key(&path) {
                self.queue_kind_resolution(path);
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

    fn request_preview_for_current(&mut self) {
        if !self.show_preview {
            self.preview.clear();
            self.preview_in_progress = false;
            self.pending_preview_request_id = None;
            if !self.kind_resolution_needed_for_filters()
                && (!self.pending_kind_paths.is_empty() || !self.in_flight_kind_paths.is_empty())
            {
                self.reset_kind_resolution_state();
            }
            return;
        }

        if let Some(row) = self.current_row {
            if let Some((path, _)) = self.results.get(row) {
                if let Some(cached) = self.preview_cache.get(path) {
                    self.preview = cached.clone();
                    self.preview_in_progress = false;
                    self.pending_preview_request_id = None;
                    return;
                }

                let Some(kind) = self.current_result_kind() else {
                    self.preview = "Resolving entry type...".to_string();
                    self.queue_kind_resolution(path.clone());
                    self.pump_kind_resolution_requests();
                    self.preview_in_progress = false;
                    self.pending_preview_request_id = None;
                    return;
                };
                let is_dir = kind.is_dir;
                if should_skip_preview(path, is_dir) {
                    let preview = build_preview_text_with_kind(path, is_dir);
                    self.cache_preview(path.clone(), preview.clone());
                    self.preview = preview;
                    self.preview_in_progress = false;
                    self.pending_preview_request_id = None;
                    return;
                }
                self.preview = "Loading preview...".to_string();
                let request_id = self.next_preview_request_id;
                self.next_preview_request_id = self.next_preview_request_id.saturating_add(1);
                self.pending_preview_request_id = Some(request_id);
                if let Some(tab_id) = self.current_tab_id() {
                    self.preview_request_tabs.insert(request_id, tab_id);
                }
                self.preview_in_progress = true;
                let req = PreviewRequest {
                    request_id,
                    path: path.clone(),
                    is_dir,
                };
                if self.preview_tx.send(req).is_err() {
                    self.preview_in_progress = false;
                    self.pending_preview_request_id = None;
                    self.preview = "<preview unavailable>".to_string();
                }
                return;
            }
        }
        self.preview.clear();
        self.preview_in_progress = false;
        self.pending_preview_request_id = None;
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

    fn filelist_entries_snapshot(&self) -> Vec<PathBuf> {
        self.all_entries
            .iter()
            .filter(|path| self.is_entry_visible_for_current_filter(path))
            .cloned()
            .collect()
    }

    fn start_filelist_creation(
        &mut self,
        tab_id: u64,
        root: PathBuf,
        entries: Vec<PathBuf>,
        propagate_to_ancestors: bool,
    ) {
        self.pending_filelist_after_index = None;
        let cancel = Arc::new(AtomicBool::new(false));
        let request_id = self.next_filelist_request_id;
        self.next_filelist_request_id = self.next_filelist_request_id.saturating_add(1);
        self.pending_filelist_request_id = Some(request_id);
        self.pending_filelist_request_tab_id = Some(tab_id);
        self.pending_filelist_root = Some(root.clone());
        self.pending_filelist_cancel = Some(Arc::clone(&cancel));
        self.filelist_in_progress = true;
        self.filelist_cancel_requested = false;
        self.refresh_status_line();

        let req = FileListRequest {
            request_id,
            tab_id,
            root,
            entries,
            propagate_to_ancestors,
            cancel,
        };
        if self.filelist_tx.send(req).is_err() {
            self.pending_filelist_request_id = None;
            self.pending_filelist_request_tab_id = None;
            self.pending_filelist_root = None;
            self.pending_filelist_cancel = None;
            self.filelist_in_progress = false;
            self.filelist_cancel_requested = false;
            self.refresh_status_line();
            self.set_notice("Create File List worker is unavailable");
        }
    }

    fn request_filelist_creation(&mut self, tab_id: u64, root: PathBuf, entries: Vec<PathBuf>) {
        if let Some(existing_path) = find_filelist_in_first_level(&root) {
            self.pending_filelist_confirmation = Some(PendingFileListConfirmation {
                tab_id,
                root,
                entries,
                existing_path: existing_path.clone(),
            });
            self.set_notice(format!(
                "{} already exists. Choose overwrite or cancel.",
                existing_path.display()
            ));
            return;
        }
        self.request_filelist_creation_after_overwrite_check(tab_id, root, entries);
    }

    fn request_filelist_creation_after_overwrite_check(
        &mut self,
        tab_id: u64,
        root: PathBuf,
        entries: Vec<PathBuf>,
    ) {
        if has_ancestor_filelists(&root) {
            self.pending_filelist_ancestor_confirmation =
                Some(PendingFileListAncestorConfirmation {
                    tab_id,
                    root,
                    entries,
                });
            self.set_notice(
                "Create File List will also update parent FileList entries. Continue or choose current root only.",
            );
            return;
        }
        self.start_filelist_creation(tab_id, root, entries, false);
    }

    fn confirm_pending_filelist_overwrite(&mut self) {
        let Some(pending) = self.pending_filelist_confirmation.take() else {
            return;
        };
        self.request_filelist_creation_after_overwrite_check(
            pending.tab_id,
            pending.root,
            pending.entries,
        );
    }

    fn confirm_pending_filelist_ancestor_propagation(&mut self) {
        let Some(pending) = self.pending_filelist_ancestor_confirmation.take() else {
            return;
        };
        self.start_filelist_creation(pending.tab_id, pending.root, pending.entries, true);
    }

    fn skip_pending_filelist_ancestor_propagation(&mut self) {
        let Some(pending) = self.pending_filelist_ancestor_confirmation.take() else {
            return;
        };
        self.start_filelist_creation(pending.tab_id, pending.root, pending.entries, false);
    }

    fn confirm_pending_filelist_use_walker(&mut self) {
        let Some(pending) = self.pending_filelist_use_walker_confirmation.take() else {
            return;
        };
        let Some(new_tab_id) = self.create_new_tab_for_root(pending.root.clone(), false) else {
            self.set_notice("Failed to prepare a new tab for FileList creation");
            return;
        };
        self.pending_filelist_after_index = Some(PendingFileListAfterIndex {
            tab_id: new_tab_id,
            root: pending.root,
        });
        self.request_index_refresh();
        self.set_notice("Preparing Walker index in new tab for Create File List");
    }

    fn cancel_pending_filelist_overwrite(&mut self) {
        if self.pending_filelist_confirmation.take().is_some() {
            self.set_notice("Create File List canceled");
        }
    }

    fn cancel_pending_filelist_ancestor_confirmation(&mut self) {
        if self.pending_filelist_ancestor_confirmation.take().is_some() {
            self.set_notice("Create File List canceled");
        }
    }

    fn cancel_pending_filelist_use_walker(&mut self) {
        if self
            .pending_filelist_use_walker_confirmation
            .take()
            .is_some()
        {
            self.set_notice("Create File List canceled");
        }
    }

    fn can_cancel_create_filelist(&self) -> bool {
        self.pending_filelist_after_index.is_some()
            || self.pending_filelist_confirmation.is_some()
            || self.pending_filelist_ancestor_confirmation.is_some()
            || self.pending_filelist_use_walker_confirmation.is_some()
            || self.filelist_in_progress
    }

    fn cancel_create_filelist(&mut self) {
        if self.pending_filelist_confirmation.is_some() {
            self.cancel_pending_filelist_overwrite();
            return;
        }
        if self.pending_filelist_ancestor_confirmation.is_some() {
            self.cancel_pending_filelist_ancestor_confirmation();
            return;
        }
        if self.pending_filelist_use_walker_confirmation.is_some() {
            self.cancel_pending_filelist_use_walker();
            return;
        }
        if self.pending_filelist_after_index.take().is_some() {
            self.set_notice("Create File List canceled");
            return;
        }
        if self.filelist_in_progress && !self.filelist_cancel_requested {
            if let Some(cancel) = self.pending_filelist_cancel.as_ref() {
                cancel.store(true, Ordering::Relaxed);
            }
            self.filelist_cancel_requested = true;
            self.refresh_status_line();
            self.set_notice("Canceling Create File List...");
        }
    }

    fn create_filelist(&mut self) {
        if self.filelist_in_progress {
            self.set_notice("Create File List is already running");
            return;
        }
        if self.pending_filelist_confirmation.is_some() {
            self.set_notice("Confirm overwrite or cancel first");
            return;
        }
        if self.pending_filelist_ancestor_confirmation.is_some() {
            self.set_notice("Confirm ancestor FileList update choice or cancel first");
            return;
        }
        if self.pending_filelist_use_walker_confirmation.is_some() {
            self.set_notice("Confirm Create File List action or cancel first");
            return;
        }
        let Some(tab_id) = self.current_tab_id() else {
            self.set_notice("Create File List is unavailable without an active tab");
            return;
        };
        if self.use_filelist_requires_locked_filters() {
            self.pending_filelist_use_walker_confirmation =
                Some(PendingFileListUseWalkerConfirmation {
                    source_tab_id: tab_id,
                    root: self.root.clone(),
                });
            self.set_notice("Confirmation required: Create File List needs Walker indexing");
            return;
        }

        let mut needs_reindex = false;
        if !self.include_files || !self.include_dirs {
            self.include_files = true;
            self.include_dirs = true;
            needs_reindex = true;
        }
        if !matches!(self.index.source, IndexSource::Walker) {
            needs_reindex = true;
        }
        if self.index_in_progress {
            self.pending_filelist_after_index = Some(PendingFileListAfterIndex {
                tab_id,
                root: self.root.clone(),
            });
            if needs_reindex {
                self.request_index_refresh();
                self.set_notice(
                    "Preparing Walker index with files/folders enabled before Create File List",
                );
            } else {
                self.set_notice("Waiting for current indexing to finish before Create File List");
            }
            return;
        }

        if needs_reindex {
            self.pending_filelist_after_index = Some(PendingFileListAfterIndex {
                tab_id,
                root: self.root.clone(),
            });
            self.request_index_refresh();
            self.set_notice(
                "Preparing Walker index with files/folders enabled before Create File List",
            );
            return;
        }

        let entries = self.filelist_entries_snapshot();
        self.request_filelist_creation(tab_id, self.root.clone(), entries);
    }

    fn poll_filelist_response(&mut self) {
        while let Ok(response) = self.filelist_rx.try_recv() {
            let Some(pending) = self.pending_filelist_request_id else {
                continue;
            };
            let requested_root = self.pending_filelist_root.clone();
            let requested_tab_id = self.pending_filelist_request_tab_id;
            match response {
                FileListResponse::Finished {
                    request_id,
                    root,
                    path,
                    count,
                } => {
                    if request_id != pending {
                        continue;
                    }
                    self.pending_filelist_request_id = None;
                    self.pending_filelist_request_tab_id = None;
                    self.pending_filelist_root = None;
                    self.pending_filelist_cancel = None;
                    self.filelist_in_progress = false;
                    self.filelist_cancel_requested = false;
                    self.refresh_status_line();

                    let same_requested_root = requested_root
                        .as_ref()
                        .map(|r| Self::path_key(r) == Self::path_key(&root))
                        .unwrap_or(true);
                    let same_current_root = Self::path_key(&self.root) == Self::path_key(&root);

                    if !same_requested_root {
                        continue;
                    }
                    if let Some(tab_id) = requested_tab_id {
                        if let Some(tab_index) = self.find_tab_index_by_id(tab_id) {
                            if let Some(tab) = self.tabs.get_mut(tab_index) {
                                tab.use_filelist = true;
                            }
                            if tab_index == self.active_tab {
                                self.use_filelist = true;
                            }
                        }
                    }
                    if !same_current_root {
                        self.set_notice(format!(
                            "Created {}: {} entries (previous root)",
                            path.display(),
                            count
                        ));
                        continue;
                    }

                    self.set_notice(format!("Created {}: {} entries", path.display(), count));
                    if requested_tab_id == self.current_tab_id() && self.use_filelist {
                        self.request_index_refresh();
                    }
                }
                FileListResponse::Failed {
                    request_id,
                    root,
                    error,
                } => {
                    if request_id != pending {
                        continue;
                    }
                    self.pending_filelist_request_id = None;
                    self.pending_filelist_request_tab_id = None;
                    self.pending_filelist_root = None;
                    self.pending_filelist_cancel = None;
                    self.filelist_in_progress = false;
                    self.filelist_cancel_requested = false;
                    self.refresh_status_line();

                    let same_requested_root = requested_root
                        .as_ref()
                        .map(|r| Self::path_key(r) == Self::path_key(&root))
                        .unwrap_or(true);
                    let same_current_root = Self::path_key(&self.root) == Self::path_key(&root);
                    if !same_requested_root || !same_current_root {
                        self.set_notice(format!(
                            "Create File List failed for previous root: {}",
                            error
                        ));
                        continue;
                    }

                    self.set_notice(format!("Create File List failed: {}", error));
                }
                FileListResponse::Canceled { request_id, root } => {
                    if request_id != pending {
                        continue;
                    }
                    self.pending_filelist_request_id = None;
                    self.pending_filelist_request_tab_id = None;
                    self.pending_filelist_root = None;
                    self.pending_filelist_cancel = None;
                    self.filelist_in_progress = false;
                    self.filelist_cancel_requested = false;
                    self.refresh_status_line();

                    let same_requested_root = requested_root
                        .as_ref()
                        .map(|r| Self::path_key(r) == Self::path_key(&root))
                        .unwrap_or(true);
                    if !same_requested_root {
                        continue;
                    }

                    self.set_notice("Create File List canceled");
                }
            }
        }
    }

    fn poll_update_response(&mut self) {
        while let Ok(response) = self.update_rx.try_recv() {
            let Some(pending) = self.pending_update_request_id else {
                continue;
            };
            match response {
                UpdateResponse::UpToDate { request_id } => {
                    if request_id != pending {
                        continue;
                    }
                    self.pending_update_request_id = None;
                    self.update_in_progress = false;
                }
                UpdateResponse::CheckFailedSilent { request_id } => {
                    if request_id != pending {
                        continue;
                    }
                    self.pending_update_request_id = None;
                    self.update_in_progress = false;
                }
                UpdateResponse::Available {
                    request_id,
                    candidate,
                } => {
                    if request_id != pending {
                        continue;
                    }
                    self.pending_update_request_id = None;
                    self.update_in_progress = false;
                    if !self.update_prompt_is_suppressed(&candidate) {
                        self.update_prompt = Some(UpdatePromptState {
                            candidate,
                            skip_until_next_version: false,
                            install_started: false,
                        });
                    }
                }
                UpdateResponse::ApplyStarted {
                    request_id,
                    target_version,
                } => {
                    if request_id != pending {
                        continue;
                    }
                    self.pending_update_request_id = None;
                    self.update_in_progress = false;
                    self.update_prompt = None;
                    self.set_notice(format!("Restarting to apply update {}...", target_version));
                    self.close_requested_for_update = true;
                }
                UpdateResponse::Failed { request_id, error } => {
                    if request_id != pending {
                        continue;
                    }
                    self.pending_update_request_id = None;
                    self.update_in_progress = false;
                    if let Some(prompt) = self.update_prompt.as_mut() {
                        prompt.install_started = false;
                    }
                    self.set_notice(error);
                }
            }
        }
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
        if self.close_requested_for_update {
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
            || self.filelist_in_progress
            || self.update_in_progress
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
#[path = "app/tests/mod.rs"]
mod tests;

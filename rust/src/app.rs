use crate::actions::execute_or_open;
use crate::indexer::{
    find_filelist_in_first_level, parse_filelist, write_filelist, IndexBuildResult, IndexSource,
};
use crate::search::try_search_entries_with_scope;
use crate::ui_model::{
    build_preview_text_with_kind, display_path_with_mode, has_visible_match,
    match_positions_for_path, normalize_path_for_display, should_skip_preview,
};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use walkdir::WalkDir;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct SavedWindowGeometry {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct UiState {
    default_root: Option<String>,
    show_preview: Option<bool>,
    results_panel_width: Option<f32>,
    window: Option<SavedWindowGeometry>,
}

#[derive(Clone, Debug, Default)]
struct LaunchSettings {
    default_root: Option<PathBuf>,
    show_preview: bool,
    results_panel_width: f32,
    window: Option<SavedWindowGeometry>,
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

struct SearchRequest {
    request_id: u64,
    query: String,
    entries: Arc<Vec<PathBuf>>,
    limit: usize,
    use_regex: bool,
    root: PathBuf,
    prefer_relative: bool,
}

struct SearchResponse {
    request_id: u64,
    results: Vec<(PathBuf, f64)>,
    error: Option<String>,
}

fn filter_search_results(
    results: Vec<(PathBuf, f64)>,
    root: &Path,
    query: &str,
    prefer_relative: bool,
    use_regex: bool,
) -> Vec<(PathBuf, f64)> {
    if use_regex {
        return results;
    }

    results
        .into_iter()
        .filter(|(path, _)| has_visible_match(path, root, query, prefer_relative))
        .collect()
}

#[derive(Clone)]
struct IndexEntry {
    path: PathBuf,
    is_dir: bool,
}

struct IndexRequest {
    request_id: u64,
    root: PathBuf,
    use_filelist: bool,
    include_files: bool,
    include_dirs: bool,
}

enum IndexResponse {
    Started {
        request_id: u64,
        source: IndexSource,
    },
    Batch {
        request_id: u64,
        entries: Vec<IndexEntry>,
    },
    Finished {
        request_id: u64,
        source: IndexSource,
    },
    Failed {
        request_id: u64,
        error: String,
    },
}

struct PreviewRequest {
    request_id: u64,
    path: PathBuf,
    is_dir: bool,
}

struct PreviewResponse {
    request_id: u64,
    path: PathBuf,
    preview: String,
}

struct FileListRequest {
    request_id: u64,
    root: PathBuf,
    entries: Vec<PathBuf>,
}

struct PendingFileListConfirmation {
    root: PathBuf,
    entries: Vec<PathBuf>,
    existing_path: PathBuf,
}

enum FileListResponse {
    Finished {
        request_id: u64,
        root: PathBuf,
        path: PathBuf,
        count: usize,
    },
    Failed {
        request_id: u64,
        root: PathBuf,
        error: String,
    },
}

fn spawn_search_worker() -> (Sender<SearchRequest>, Receiver<SearchResponse>) {
    let (tx_req, rx_req) = mpsc::channel::<SearchRequest>();
    let (tx_res, rx_res) = mpsc::channel::<SearchResponse>();

    thread::spawn(move || {
        while let Ok(mut req) = rx_req.recv() {
            while let Ok(newer) = rx_req.try_recv() {
                req = newer;
            }
            let (results, error) = match try_search_entries_with_scope(
                &req.query,
                &req.entries,
                req.limit,
                req.use_regex,
                Some(&req.root),
                req.prefer_relative,
            ) {
                Ok(raw_results) => (
                    filter_search_results(
                        raw_results,
                        &req.root,
                        &req.query,
                        req.prefer_relative,
                        req.use_regex,
                    ),
                    None,
                ),
                Err(err) => (Vec::new(), Some(err)),
            };

            if tx_res
                .send(SearchResponse {
                    request_id: req.request_id,
                    results,
                    error,
                })
                .is_err()
            {
                break;
            }
        }
    });

    (tx_req, rx_res)
}

fn spawn_preview_worker() -> (Sender<PreviewRequest>, Receiver<PreviewResponse>) {
    let (tx_req, rx_req) = mpsc::channel::<PreviewRequest>();
    let (tx_res, rx_res) = mpsc::channel::<PreviewResponse>();

    thread::spawn(move || {
        while let Ok(mut req) = rx_req.recv() {
            while let Ok(newer) = rx_req.try_recv() {
                req = newer;
            }
            let preview = build_preview_text_with_kind(&req.path, req.is_dir);
            if tx_res
                .send(PreviewResponse {
                    request_id: req.request_id,
                    path: req.path,
                    preview,
                })
                .is_err()
            {
                break;
            }
        }
    });

    (tx_req, rx_res)
}

fn spawn_filelist_worker() -> (Sender<FileListRequest>, Receiver<FileListResponse>) {
    let (tx_req, rx_req) = mpsc::channel::<FileListRequest>();
    let (tx_res, rx_res) = mpsc::channel::<FileListResponse>();

    thread::spawn(move || {
        while let Ok(req) = rx_req.recv() {
            let count = req.entries.len();
            let result =
                write_filelist(&req.root, &req.entries, "FileList.txt").map(|path| (path, count));
            let msg = match result {
                Ok((path, count)) => FileListResponse::Finished {
                    request_id: req.request_id,
                    root: req.root.clone(),
                    path,
                    count,
                },
                Err(err) => FileListResponse::Failed {
                    request_id: req.request_id,
                    root: req.root.clone(),
                    error: err.to_string(),
                },
            };
            if tx_res.send(msg).is_err() {
                break;
            }
        }
    });

    (tx_req, rx_res)
}

fn flush_batch(
    tx_res: &Sender<IndexResponse>,
    request_id: u64,
    buffer: &mut Vec<IndexEntry>,
) -> bool {
    if buffer.is_empty() {
        return true;
    }
    let entries = std::mem::take(buffer);
    tx_res
        .send(IndexResponse::Batch {
            request_id,
            entries,
        })
        .is_ok()
}

fn stream_filelist_index(
    tx_res: &Sender<IndexResponse>,
    req: &IndexRequest,
    root: &std::path::Path,
    filelist: PathBuf,
    latest_request_id: &AtomicU64,
) -> std::result::Result<IndexSource, String> {
    let parsed = parse_filelist(&filelist, root, req.include_files, req.include_dirs)
        .map_err(|e| e.to_string())?;

    let source = IndexSource::FileList(filelist);
    if tx_res
        .send(IndexResponse::Started {
            request_id: req.request_id,
            source: source.clone(),
        })
        .is_err()
    {
        return Err("index receiver closed".to_string());
    }

    let mut buffer: Vec<IndexEntry> = Vec::new();
    let mut last_flush = Instant::now();
    for path in parsed {
        if latest_request_id.load(Ordering::Relaxed) != req.request_id {
            return Err("superseded".to_string());
        }
        let is_dir = path.is_dir();
        buffer.push(IndexEntry { path, is_dir });
        if buffer.len() >= 256 || last_flush.elapsed() >= Duration::from_millis(100) {
            if !flush_batch(tx_res, req.request_id, &mut buffer) {
                return Err("index receiver closed".to_string());
            }
            last_flush = Instant::now();
        }
    }

    if !flush_batch(tx_res, req.request_id, &mut buffer) {
        return Err("index receiver closed".to_string());
    }
    Ok(source)
}

fn stream_walker_index(
    tx_res: &Sender<IndexResponse>,
    req: &IndexRequest,
    root: &std::path::Path,
    latest_request_id: &AtomicU64,
) -> std::result::Result<IndexSource, String> {
    let source = IndexSource::Walker;
    if tx_res
        .send(IndexResponse::Started {
            request_id: req.request_id,
            source: source.clone(),
        })
        .is_err()
    {
        return Err("index receiver closed".to_string());
    }

    let mut buffer: Vec<IndexEntry> = Vec::new();
    let mut last_flush = Instant::now();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .min_depth(1)
        .into_iter()
        .flatten()
    {
        if latest_request_id.load(Ordering::Relaxed) != req.request_id {
            return Err("superseded".to_string());
        }
        let is_dir = entry.file_type().is_dir();
        if (is_dir && !req.include_dirs) || (!is_dir && !req.include_files) {
            continue;
        }
        buffer.push(IndexEntry {
            path: entry.path().to_path_buf(),
            is_dir,
        });
        if buffer.len() >= 256 || last_flush.elapsed() >= Duration::from_millis(100) {
            if !flush_batch(tx_res, req.request_id, &mut buffer) {
                return Err("index receiver closed".to_string());
            }
            last_flush = Instant::now();
        }
    }

    if !flush_batch(tx_res, req.request_id, &mut buffer) {
        return Err("index receiver closed".to_string());
    }
    Ok(source)
}

fn spawn_index_worker(
    latest_request_id: Arc<AtomicU64>,
) -> (Sender<IndexRequest>, Receiver<IndexResponse>) {
    let (tx_req, rx_req) = mpsc::channel::<IndexRequest>();
    let (tx_res, rx_res) = mpsc::channel::<IndexResponse>();
    let latest_request_id_worker = Arc::clone(&latest_request_id);

    thread::spawn(move || {
        while let Ok(mut req) = rx_req.recv() {
            while let Ok(newer) = rx_req.try_recv() {
                req = newer;
            }
            latest_request_id_worker.store(req.request_id, Ordering::Relaxed);

            if !req.include_files && !req.include_dirs {
                if tx_res
                    .send(IndexResponse::Started {
                        request_id: req.request_id,
                        source: IndexSource::None,
                    })
                    .is_err()
                {
                    break;
                }
                if tx_res
                    .send(IndexResponse::Finished {
                        request_id: req.request_id,
                        source: IndexSource::None,
                    })
                    .is_err()
                {
                    break;
                }
                continue;
            }

            let root = req.root.canonicalize().unwrap_or_else(|_| req.root.clone());
            let result = if req.use_filelist {
                if let Some(filelist) = find_filelist_in_first_level(&root) {
                    stream_filelist_index(
                        &tx_res,
                        &req,
                        &root,
                        filelist,
                        latest_request_id_worker.as_ref(),
                    )
                } else {
                    stream_walker_index(&tx_res, &req, &root, latest_request_id_worker.as_ref())
                }
            } else {
                stream_walker_index(&tx_res, &req, &root, latest_request_id_worker.as_ref())
            };

            match result {
                Ok(source) => {
                    if tx_res
                        .send(IndexResponse::Finished {
                            request_id: req.request_id,
                            source,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
                Err(error) => {
                    if error == "superseded" {
                        continue;
                    }
                    if tx_res
                        .send(IndexResponse::Failed {
                            request_id: req.request_id,
                            error,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
    });

    (tx_req, rx_res)
}

pub struct FlistWalkerApp {
    root: PathBuf,
    limit: usize,
    query: String,
    use_filelist: bool,
    use_regex: bool,
    include_files: bool,
    include_dirs: bool,
    index: IndexBuildResult,
    all_entries: Arc<Vec<PathBuf>>,
    entries: Arc<Vec<PathBuf>>,
    entry_kinds: HashMap<PathBuf, bool>,
    results: Vec<(PathBuf, f64)>,
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
    filelist_tx: Sender<FileListRequest>,
    filelist_rx: Receiver<FileListResponse>,
    index_tx: Sender<IndexRequest>,
    index_rx: Receiver<IndexResponse>,
    next_request_id: u64,
    pending_request_id: Option<u64>,
    next_index_request_id: u64,
    pending_index_request_id: Option<u64>,
    next_preview_request_id: u64,
    pending_preview_request_id: Option<u64>,
    next_filelist_request_id: u64,
    pending_filelist_request_id: Option<u64>,
    pending_filelist_root: Option<PathBuf>,
    pending_filelist_after_index_root: Option<PathBuf>,
    pending_filelist_confirmation: Option<PendingFileListConfirmation>,
    latest_index_request_id: Arc<AtomicU64>,
    search_in_progress: bool,
    index_in_progress: bool,
    preview_in_progress: bool,
    filelist_in_progress: bool,
    scroll_to_current: bool,
    focus_query_requested: bool,
    unfocus_query_requested: bool,
    saved_roots: Vec<PathBuf>,
    default_root: Option<PathBuf>,
    show_preview: bool,
    results_panel_width: f32,
    pending_window_restore: Option<SavedWindowGeometry>,
    window_geometry: Option<SavedWindowGeometry>,
    ui_state_dirty: bool,
    last_ui_state_save: Instant,
    query_input_id: egui::Id,
    preview_cache: HashMap<PathBuf, String>,
    preview_cache_order: VecDeque<PathBuf>,
    last_incremental_results_refresh: Instant,
    last_search_snapshot_len: usize,
    search_resume_pending: bool,
}

impl FlistWalkerApp {
    const PREVIEW_CACHE_MAX: usize = 512;
    const INCREMENTAL_SEARCH_REFRESH_INTERVAL: Duration = Duration::from_millis(300);
    const PAGE_MOVE_ROWS: isize = 10;
    const DEFAULT_RESULTS_PANEL_WIDTH: f32 = 760.0;
    const MIN_RESULTS_PANEL_WIDTH: f32 = 220.0;
    const MIN_PREVIEW_PANEL_WIDTH: f32 = 220.0;
    const UI_STATE_SAVE_INTERVAL: Duration = Duration::from_millis(500);

    pub fn new(root: PathBuf, limit: usize, query: String) -> Self {
        let launch = LaunchSettings {
            show_preview: true,
            results_panel_width: Self::DEFAULT_RESULTS_PANEL_WIDTH,
            ..LaunchSettings::default()
        };
        Self::new_with_launch(root, limit, query, launch)
    }

    pub fn from_launch(root: PathBuf, limit: usize, query: String, root_explicit: bool) -> Self {
        let launch = Self::load_launch_settings();
        let saved_default = launch
            .default_root
            .as_ref()
            .and_then(|p| p.canonicalize().ok())
            .map(Self::normalize_windows_path)
            .filter(|p| p.is_dir());
        let chosen_root = if root_explicit {
            root
        } else {
            saved_default.unwrap_or(root)
        };
        Self::new_with_launch(chosen_root, limit, query, launch)
    }

    fn new_with_launch(root: PathBuf, limit: usize, query: String, launch: LaunchSettings) -> Self {
        let (search_tx, search_rx) = spawn_search_worker();
        let (preview_tx, preview_rx) = spawn_preview_worker();
        let (filelist_tx, filelist_rx) = spawn_filelist_worker();
        let latest_index_request_id = Arc::new(AtomicU64::new(0));
        let (index_tx, index_rx) = spawn_index_worker(Arc::clone(&latest_index_request_id));
        let mut app = Self {
            root: Self::normalize_windows_path(root),
            limit: limit.clamp(1, 1000),
            query,
            use_filelist: false,
            use_regex: false,
            include_files: true,
            include_dirs: true,
            index: IndexBuildResult {
                entries: Vec::new(),
                source: IndexSource::None,
            },
            all_entries: Arc::new(Vec::new()),
            entries: Arc::new(Vec::new()),
            entry_kinds: HashMap::new(),
            results: Vec::new(),
            pinned_paths: HashSet::new(),
            current_row: None,
            preview: String::new(),
            notice: String::new(),
            status_line: "Initializing...".to_string(),
            kill_buffer: String::new(),
            search_tx,
            search_rx,
            preview_tx,
            preview_rx,
            filelist_tx,
            filelist_rx,
            index_tx,
            index_rx,
            next_request_id: 1,
            pending_request_id: None,
            next_index_request_id: 1,
            pending_index_request_id: None,
            next_preview_request_id: 1,
            pending_preview_request_id: None,
            next_filelist_request_id: 1,
            pending_filelist_request_id: None,
            pending_filelist_root: None,
            pending_filelist_after_index_root: None,
            pending_filelist_confirmation: None,
            latest_index_request_id,
            search_in_progress: false,
            index_in_progress: false,
            preview_in_progress: false,
            filelist_in_progress: false,
            scroll_to_current: true,
            focus_query_requested: false,
            unfocus_query_requested: false,
            saved_roots: Self::load_saved_roots(),
            default_root: launch.default_root.clone(),
            show_preview: launch.show_preview,
            results_panel_width: launch
                .results_panel_width
                .max(Self::MIN_RESULTS_PANEL_WIDTH),
            pending_window_restore: launch.window.clone(),
            window_geometry: None,
            ui_state_dirty: false,
            last_ui_state_save: Instant::now(),
            query_input_id: egui::Id::new("query-input"),
            preview_cache: HashMap::new(),
            preview_cache_order: VecDeque::new(),
            last_incremental_results_refresh: Instant::now(),
            last_search_snapshot_len: 0,
            search_resume_pending: false,
        };
        app.request_index_refresh();
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

    fn root_display_text(&self) -> String {
        Self::normalize_windows_path(self.root.clone())
            .to_string_lossy()
            .to_string()
    }

    fn ui_state_file_path() -> Option<PathBuf> {
        #[cfg(windows)]
        {
            if let Some(base) = std::env::var_os("USERPROFILE") {
                return Some(PathBuf::from(base).join(".flistwalker_ui_state.json"));
            }
        }
        #[cfg(not(windows))]
        {
            if let Some(base) = std::env::var_os("HOME") {
                return Some(PathBuf::from(base).join(".flistwalker_ui_state.json"));
            }
        }
        None
    }

    fn load_ui_state() -> UiState {
        let Some(path) = Self::ui_state_file_path() else {
            return UiState::default();
        };
        let Ok(text) = fs::read_to_string(path) else {
            return UiState::default();
        };
        serde_json::from_str::<UiState>(&text).unwrap_or_default()
    }

    fn load_launch_settings() -> LaunchSettings {
        let ui_state = Self::load_ui_state();
        let default_root = ui_state
            .default_root
            .as_deref()
            .map(PathBuf::from)
            .map(Self::normalize_windows_path);
        let show_preview = ui_state.show_preview.unwrap_or(true);
        let results_panel_width = ui_state
            .results_panel_width
            .unwrap_or(Self::DEFAULT_RESULTS_PANEL_WIDTH)
            .max(Self::MIN_RESULTS_PANEL_WIDTH);
        LaunchSettings {
            default_root,
            show_preview,
            results_panel_width,
            window: ui_state.window,
        }
    }

    fn save_ui_state(&self) {
        let Some(path) = Self::ui_state_file_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let state = UiState {
            default_root: self
                .default_root
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
            show_preview: Some(self.show_preview),
            results_panel_width: Some(self.results_panel_width),
            window: self.window_geometry.clone(),
        };
        if let Ok(text) = serde_json::to_string_pretty(&state) {
            let _ = fs::write(path, text);
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

    fn to_stable_window_geometry(geom: SavedWindowGeometry) -> SavedWindowGeometry {
        let round = |v: f32| (v * 10.0).round() / 10.0;
        SavedWindowGeometry {
            x: round(geom.x),
            y: round(geom.y),
            width: round(geom.width.max(640.0)),
            height: round(geom.height.max(400.0)),
        }
    }

    fn capture_window_geometry(&mut self, ctx: &egui::Context) {
        let next = ctx.input(|i| {
            i.viewport().outer_rect.map(|rect| SavedWindowGeometry {
                x: rect.min.x,
                y: rect.min.y,
                width: rect.width(),
                height: rect.height(),
            })
        });
        let Some(next) = next.map(Self::to_stable_window_geometry) else {
            return;
        };
        if self.window_geometry.as_ref() != Some(&next) {
            self.window_geometry = Some(next);
            self.mark_ui_state_dirty();
        }
    }

    fn apply_pending_window_restore(&mut self, ctx: &egui::Context) {
        let Some(saved) = self.pending_window_restore.clone() else {
            return;
        };

        let monitor_size = ctx.input(|i| i.viewport().monitor_size);
        let mut width = saved.width.max(640.0);
        let mut height = saved.height.max(400.0);
        let mut x = saved.x;
        let mut y = saved.y;

        if let Some(monitor_size) = monitor_size {
            width = width.min(monitor_size.x.max(640.0));
            height = height.min(monitor_size.y.max(400.0));
            let max_x = (monitor_size.x - width).max(0.0);
            let max_y = (monitor_size.y - height).max(0.0);
            x = x.clamp(0.0, max_x);
            y = y.clamp(0.0, max_y);
        } else if ctx.input(|i| i.viewport().outer_rect).is_none() {
            return;
        }

        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(width, height)));
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
        self.pending_window_restore = None;
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
        let _ = fs::write(
            file,
            if text.is_empty() {
                text
            } else {
                format!("{text}\n")
            },
        );
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
        let root = self
            .root
            .canonicalize()
            .unwrap_or_else(|_| self.root.clone());
        let root = Self::normalize_windows_path(root);
        self.default_root = Some(root.clone());
        self.mark_ui_state_dirty();
        self.save_ui_state();
        self.ui_state_dirty = false;
        self.last_ui_state_save = Instant::now();
        self.set_notice(format!("Set default root: {}", root.display()));
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
        let should_cancel = self
            .pending_filelist_confirmation
            .as_ref()
            .is_some_and(|pending| Self::path_key(&pending.root) != Self::path_key(&self.root));
        if should_cancel {
            self.pending_filelist_confirmation = None;
            self.set_notice("Pending FileList overwrite canceled because root changed");
        }
    }

    fn apply_root_change(&mut self, new_root: PathBuf) {
        let normalized = Self::normalize_windows_path(new_root);
        if Self::path_key(&normalized) == Self::path_key(&self.root) {
            return;
        }

        self.root = normalized;
        // Avoid launching/copying stale selections from the previous root.
        self.pinned_paths.clear();
        self.current_row = None;
        self.preview.clear();
        self.preview_in_progress = false;
        self.pending_preview_request_id = None;
        self.cancel_stale_pending_filelist_confirmation();
        self.request_index_refresh();
        self.set_notice(format!("Root changed: {}", self.root_display_text()));
    }

    fn prefer_relative_display(&self) -> bool {
        matches!(
            self.index.source,
            IndexSource::Walker | IndexSource::FileList(_)
        )
    }

    fn refresh_status_line(&mut self) {
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
        let creating_filelist = if self.filelist_in_progress {
            " | Creating FileList..."
        } else {
            ""
        };
        let notice = if self.notice.is_empty() {
            String::new()
        } else {
            format!(" | {}", self.notice)
        };

        self.status_line = format!(
            "Entries: {} | Results: {}{}{}{}{}{}{}",
            indexed_count,
            self.results.len(),
            clip_text,
            pinned,
            searching,
            indexing,
            creating_filelist,
            notice
        );
    }

    fn set_notice(&mut self, notice: impl Into<String>) {
        self.notice = notice.into();
        self.refresh_status_line();
    }

    fn clear_notice(&mut self) {
        self.notice.clear();
        self.refresh_status_line();
    }

    fn request_index_refresh(&mut self) {
        self.ensure_entry_filters();
        self.cancel_stale_pending_filelist_confirmation();
        if self
            .pending_filelist_after_index_root
            .as_ref()
            .is_some_and(|pending_root| Self::path_key(pending_root) != Self::path_key(&self.root))
        {
            self.pending_filelist_after_index_root = None;
            self.set_notice("Deferred Create File List canceled because root changed");
        }
        let request_id = self.next_index_request_id;
        self.next_index_request_id = self.next_index_request_id.saturating_add(1);
        self.latest_index_request_id
            .store(request_id, Ordering::Relaxed);
        self.pending_index_request_id = Some(request_id);
        self.index_in_progress = true;
        // Cancel in-flight search requests so responses computed from stale snapshots
        // cannot override results while a new index request is running.
        self.pending_request_id = None;
        self.search_in_progress = false;
        // Non-empty query should resume quickly once fresh index batches arrive.
        self.search_resume_pending = !self.query.trim().is_empty();

        self.index.entries.clear();
        self.index.source = IndexSource::None;
        self.preview_cache.clear();
        self.preview_cache_order.clear();
        self.pending_preview_request_id = None;
        self.preview_in_progress = false;
        self.last_incremental_results_refresh = Instant::now();
        self.last_search_snapshot_len = 0;
        self.refresh_status_line();

        let req = IndexRequest {
            request_id,
            root: self.root.clone(),
            use_filelist: self.use_filelist,
            include_files: true,
            include_dirs: true,
        };
        if self.index_tx.send(req).is_err() {
            self.index_in_progress = false;
            self.pending_index_request_id = None;
            self.set_notice("Index worker is unavailable");
        }
    }

    fn poll_index_response(&mut self) {
        const MAX_MESSAGES_PER_FRAME: usize = 12;
        const FRAME_BUDGET: Duration = Duration::from_millis(4);

        let frame_start = Instant::now();
        let mut processed = 0usize;
        let mut needs_incremental_refresh = false;
        let mut finished_current_request = false;
        while let Ok(msg) = self.index_rx.try_recv() {
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
                    for entry in entries {
                        self.entry_kinds.insert(entry.path.clone(), entry.is_dir);
                        self.index.entries.push(entry.path);
                    }
                    needs_incremental_refresh = true;
                }
                IndexResponse::Finished { request_id, source } => {
                    if Some(request_id) != self.pending_index_request_id {
                        continue;
                    }
                    self.index.source = source;
                    self.all_entries = Arc::new(std::mem::take(&mut self.index.entries));
                    self.last_search_snapshot_len = self.all_entries.len();
                    self.pending_index_request_id = None;
                    self.index_in_progress = false;
                    self.apply_entry_filters(true);
                    self.search_resume_pending = false;
                    self.clear_notice();
                    if self
                        .pending_filelist_after_index_root
                        .as_ref()
                        .is_some_and(|pending_root| {
                            Self::path_key(pending_root) == Self::path_key(&self.root)
                        })
                    {
                        let root = self.root.clone();
                        let entries = self.filelist_entries_snapshot();
                        self.pending_filelist_after_index_root = None;
                        self.request_filelist_creation(root, entries);
                    }
                    finished_current_request = true;
                    needs_incremental_refresh = false;
                    break;
                }
                IndexResponse::Failed { request_id, error } => {
                    if Some(request_id) != self.pending_index_request_id {
                        continue;
                    }
                    self.index_in_progress = false;
                    self.pending_index_request_id = None;
                    self.search_resume_pending = false;
                    self.pending_filelist_after_index_root = None;
                    self.set_notice(format!("Indexing failed: {}", error));
                }
            }

            processed = processed.saturating_add(1);
            if processed >= MAX_MESSAGES_PER_FRAME || frame_start.elapsed() >= FRAME_BUDGET {
                break;
            }
        }

        if !needs_incremental_refresh {
            return;
        }
        if finished_current_request {
            return;
        }

        if self.query.trim().is_empty() {
            // Empty query is the progressive browsing mode: show newest indexed entries
            // immediately without cloning the whole snapshot for search workers.
            self.update_results_from_index_progress();
            return;
        }

        if self.search_resume_pending {
            self.entries = Arc::new(self.filtered_entries(&self.index.entries));
            self.last_search_snapshot_len = self.entries.len();
            self.last_incremental_results_refresh = Instant::now();
            self.update_results();
            self.search_resume_pending = false;
            return;
        }

        let current_len = self.index.entries.len();
        let delta = current_len.saturating_sub(self.last_search_snapshot_len);
        if delta > 0
            && self.last_incremental_results_refresh.elapsed()
                >= Self::INCREMENTAL_SEARCH_REFRESH_INTERVAL
        {
            // Non-empty query still needs progressive refresh, but cloning huge snapshots
            // every batch stalls UI. Throttle by time window to keep results visible
            // even on very slow indexers with small incremental deltas.
            self.entries = Arc::new(self.filtered_entries(&self.index.entries));
            self.last_search_snapshot_len = self.entries.len();
            self.last_incremental_results_refresh = Instant::now();
            self.update_results();
        }
    }

    fn ensure_entry_filters(&mut self) -> bool {
        if !self.include_files && !self.include_dirs {
            self.include_files = true;
            return true;
        }
        false
    }

    fn apply_results(&mut self, results: Vec<(PathBuf, f64)>) {
        self.apply_results_with_scroll_policy(results, false);
    }

    fn apply_results_with_scroll_policy(
        &mut self,
        results: Vec<(PathBuf, f64)>,
        keep_scroll_position: bool,
    ) {
        self.results = results;
        if self.results.is_empty() {
            self.current_row = None;
            self.preview.clear();
            self.preview_in_progress = false;
            self.pending_preview_request_id = None;
        } else {
            let max_index = self.results.len().saturating_sub(1);
            self.current_row = Some(self.current_row.unwrap_or(0).min(max_index));
            self.request_preview_for_current();
            if !keep_scroll_position {
                self.scroll_to_current = true;
            }
        }
        self.refresh_status_line();
    }

    fn enqueue_search_request(&mut self) {
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1);
        self.pending_request_id = Some(request_id);
        self.search_in_progress = true;
        self.refresh_status_line();

        let req = SearchRequest {
            request_id,
            query: self.query.clone(),
            entries: Arc::clone(&self.entries),
            limit: self.limit,
            use_regex: self.use_regex,
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
            if Some(response.request_id) == self.pending_request_id {
                self.pending_request_id = None;
                self.search_in_progress = false;
                if let Some(error) = response.error {
                    self.set_notice(format!("Search failed: {error}"));
                } else {
                    self.clear_notice();
                }
                self.apply_results(response.results);
            }
        }
    }

    fn poll_preview_response(&mut self) {
        while let Ok(response) = self.preview_rx.try_recv() {
            if Some(response.request_id) != self.pending_preview_request_id {
                continue;
            }
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
        }
    }

    fn cache_preview(&mut self, path: PathBuf, preview: String) {
        if !self.preview_cache.contains_key(&path) {
            self.preview_cache_order.push_back(path.clone());
        }
        self.preview_cache.insert(path, preview);
        // Keep cache bounded so long browse sessions do not grow memory unbounded.
        while self.preview_cache_order.len() > Self::PREVIEW_CACHE_MAX {
            if let Some(oldest) = self.preview_cache_order.pop_front() {
                self.preview_cache.remove(&oldest);
            }
        }
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
            self.apply_results(results);
            return;
        }
        self.enqueue_search_request();
    }

    fn update_results_from_index_progress(&mut self) {
        self.pending_request_id = None;
        self.search_in_progress = false;
        let filtered = self.filtered_entries(&self.index.entries);
        self.entries = Arc::new(filtered);
        let results = self
            .entries
            .iter()
            .take(self.limit)
            .cloned()
            .map(|p| (p, 0.0))
            .collect();
        self.apply_results_with_scroll_policy(results, true);
    }

    fn filtered_entries(&self, source: &[PathBuf]) -> Vec<PathBuf> {
        source
            .iter()
            .filter(|path| {
                let is_dir = self.entry_kinds.get(*path).copied().unwrap_or(false);
                (is_dir && self.include_dirs) || (!is_dir && self.include_files)
            })
            .cloned()
            .collect()
    }

    fn apply_entry_filters(&mut self, keep_scroll_position: bool) {
        let base = if self.index_in_progress && !self.index.entries.is_empty() {
            &self.index.entries
        } else {
            self.all_entries.as_ref()
        };
        self.entries = Arc::new(self.filtered_entries(base));

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
            self.apply_results_with_scroll_policy(results, keep_scroll_position);
        } else {
            self.update_results();
        }
    }

    fn move_page(&mut self, direction: isize) {
        self.move_row(direction.saturating_mul(Self::PAGE_MOVE_ROWS));
    }

    fn current_result_kind(&self) -> Option<bool> {
        let row = self.current_row?;
        let (path, _) = self.results.get(row)?;
        self.entry_kinds.get(path).copied()
    }

    fn request_preview_for_current(&mut self) {
        if let Some(row) = self.current_row {
            if let Some((path, _)) = self.results.get(row) {
                if let Some(cached) = self.preview_cache.get(path) {
                    self.preview = cached.clone();
                    self.preview_in_progress = false;
                    self.pending_preview_request_id = None;
                    return;
                }

                let Some(is_dir) = self.current_result_kind() else {
                    self.preview.clear();
                    self.preview_in_progress = false;
                    self.pending_preview_request_id = None;
                    return;
                };
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

    fn toggle_pin_and_move(&mut self, delta: isize) {
        if let Some(row) = self.current_row {
            if let Some((path, _)) = self.results.get(row) {
                if self.pinned_paths.contains(path) {
                    self.pinned_paths.remove(path);
                } else {
                    self.pinned_paths.insert(path.clone());
                }
            }
        }
        self.move_row(delta);
        self.refresh_status_line();
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
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }

        for path in &paths {
            if let Err(err) = execute_or_open(path) {
                self.set_notice(format!("Action failed: {}", err));
                return;
            }
        }

        if paths.len() == 1 {
            self.set_notice(format!("Action: {}", paths[0].display()));
        } else {
            self.set_notice(format!("Action: launched {} items", paths.len()));
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
            self.set_notice(format!("Copied path: {}", paths[0].display()));
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
        self.pinned_paths.clear();
        self.current_row = None;
        self.preview.clear();
        self.update_results();
        self.focus_query_requested = true;
        self.set_notice("Cleared selection and query");
    }

    fn filelist_entries_snapshot(&self) -> Vec<PathBuf> {
        self.all_entries
            .iter()
            .filter(|path| {
                let is_dir = self.entry_kinds.get(*path).copied().unwrap_or(false);
                (is_dir && self.include_dirs) || (!is_dir && self.include_files)
            })
            .cloned()
            .collect()
    }

    fn start_filelist_creation(&mut self, root: PathBuf, entries: Vec<PathBuf>) {
        self.pending_filelist_after_index_root = None;
        let request_id = self.next_filelist_request_id;
        self.next_filelist_request_id = self.next_filelist_request_id.saturating_add(1);
        self.pending_filelist_request_id = Some(request_id);
        self.pending_filelist_root = Some(root.clone());
        self.filelist_in_progress = true;
        self.refresh_status_line();

        let req = FileListRequest {
            request_id,
            root,
            entries,
        };
        if self.filelist_tx.send(req).is_err() {
            self.pending_filelist_request_id = None;
            self.pending_filelist_root = None;
            self.filelist_in_progress = false;
            self.set_notice("Create File List worker is unavailable");
        }
    }

    fn request_filelist_creation(&mut self, root: PathBuf, entries: Vec<PathBuf>) {
        if let Some(existing_path) = find_filelist_in_first_level(&root) {
            self.pending_filelist_confirmation = Some(PendingFileListConfirmation {
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
        self.start_filelist_creation(root, entries);
    }

    fn confirm_pending_filelist_overwrite(&mut self) {
        let Some(pending) = self.pending_filelist_confirmation.take() else {
            return;
        };
        self.start_filelist_creation(pending.root, pending.entries);
    }

    fn cancel_pending_filelist_overwrite(&mut self) {
        if self.pending_filelist_confirmation.take().is_some() {
            self.set_notice("Create File List canceled");
        }
    }

    fn create_filelist(&mut self) {
        if self.filelist_in_progress {
            self.set_notice("Create File List is already running");
            return;
        }
        if self.index_in_progress {
            self.pending_filelist_after_index_root = Some(self.root.clone());
            self.set_notice(
                "Indexing in progress. Create File List will run after indexing finishes",
            );
            return;
        }

        let entries = self.filelist_entries_snapshot();
        self.request_filelist_creation(self.root.clone(), entries);
    }

    fn poll_filelist_response(&mut self) {
        while let Ok(response) = self.filelist_rx.try_recv() {
            let Some(pending) = self.pending_filelist_request_id else {
                continue;
            };
            let requested_root = self.pending_filelist_root.clone();
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
                    self.pending_filelist_root = None;
                    self.filelist_in_progress = false;

                    let same_requested_root = requested_root
                        .as_ref()
                        .map(|r| Self::path_key(r) == Self::path_key(&root))
                        .unwrap_or(true);
                    let same_current_root = Self::path_key(&self.root) == Self::path_key(&root);

                    if !same_requested_root || !same_current_root {
                        self.set_notice(format!(
                            "Created {}: {} entries (previous root)",
                            path.display(),
                            count
                        ));
                        continue;
                    }

                    self.set_notice(format!("Created {}: {} entries", path.display(), count));
                    if self.use_filelist {
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
                    self.pending_filelist_root = None;
                    self.filelist_in_progress = false;

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

    fn char_count(text: &str) -> usize {
        text.chars().count()
    }

    fn byte_index_at_char(text: &str, char_index: usize) -> usize {
        if char_index == 0 {
            return 0;
        }
        text.char_indices()
            .nth(char_index)
            .map(|(idx, _)| idx)
            .unwrap_or(text.len())
    }

    fn remove_char_range(text: &mut String, start: usize, end: usize) -> String {
        if start >= end {
            return String::new();
        }
        let start_byte = Self::byte_index_at_char(text, start);
        let end_byte = Self::byte_index_at_char(text, end);
        let removed = text[start_byte..end_byte].to_string();
        text.replace_range(start_byte..end_byte, "");
        removed
    }

    fn insert_at_char(text: &mut String, pos: usize, value: &str) {
        let byte_pos = Self::byte_index_at_char(text, pos);
        text.insert_str(byte_pos, value);
    }

    fn is_word_char(ch: char) -> bool {
        ch.is_alphanumeric() || ch == '_' || ch == '-'
    }

    fn selection_range(cursor: usize, anchor: usize) -> Option<(usize, usize)> {
        if cursor == anchor {
            None
        } else {
            Some((cursor.min(anchor), cursor.max(anchor)))
        }
    }

    fn apply_ctrl_h_delete(
        &mut self,
        cursor: &mut usize,
        anchor: &mut usize,
        text_already_changed: bool,
    ) -> (bool, bool) {
        // Some backends map Ctrl+H to Backspace at the widget level.
        // Avoid applying our delete logic twice in the same frame.
        if text_already_changed {
            return (false, false);
        }

        if let Some((start, end)) = Self::selection_range(*cursor, *anchor) {
            Self::remove_char_range(&mut self.query, start, end);
            *cursor = start;
            *anchor = start;
            return (true, true);
        }

        if *cursor > 0 {
            let start = *cursor - 1;
            Self::remove_char_range(&mut self.query, start, *cursor);
            *cursor = start;
            *anchor = start;
            return (true, true);
        }

        (false, false)
    }

    fn apply_emacs_query_shortcuts(
        &mut self,
        ctx: &egui::Context,
        output: &mut egui::text_edit::TextEditOutput,
    ) -> bool {
        if !output.response.has_focus() {
            return false;
        }

        let emacs_mods = egui::Modifiers {
            command: true,
            ..Default::default()
        };
        let pressed = |key: egui::Key| ctx.input_mut(|i| i.consume_key(emacs_mods, key));
        let space_pressed = ctx
            .input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space))
            || ctx.input_mut(|i| {
                i.consume_key(
                    egui::Modifiers {
                        shift: true,
                        ..Default::default()
                    },
                    egui::Key::Space,
                )
            });

        let mut text_changed = false;
        let mut cursor_changed = false;
        let char_len = Self::char_count(&self.query);
        let ccursor = output.state.ccursor_range().unwrap_or_else(|| {
            egui::text_edit::CCursorRange::one(egui::text::CCursor::new(char_len))
        });
        let mut cursor = ccursor.primary.index.min(char_len);
        let mut anchor = ccursor.secondary.index.min(char_len);

        if space_pressed && !output.response.changed() {
            if let Some((start, end)) = Self::selection_range(cursor, anchor) {
                Self::remove_char_range(&mut self.query, start, end);
                cursor = start;
            }
            Self::insert_at_char(&mut self.query, cursor, " ");
            cursor += 1;
            anchor = cursor;
            text_changed = true;
            cursor_changed = true;
        } else if pressed(egui::Key::A) {
            cursor = 0;
            anchor = 0;
            cursor_changed = true;
        } else if pressed(egui::Key::E) {
            let end = Self::char_count(&self.query);
            cursor = end;
            anchor = end;
            cursor_changed = true;
        } else if pressed(egui::Key::B) {
            let next = cursor.saturating_sub(1);
            if next != cursor {
                cursor = next;
                anchor = next;
                cursor_changed = true;
            }
        } else if pressed(egui::Key::F) {
            let end = Self::char_count(&self.query);
            let next = (cursor + 1).min(end);
            if next != cursor {
                cursor = next;
                anchor = next;
                cursor_changed = true;
            }
        } else if pressed(egui::Key::H) {
            let (changed, moved) =
                self.apply_ctrl_h_delete(&mut cursor, &mut anchor, output.response.changed());
            text_changed |= changed;
            cursor_changed |= moved;
        } else if pressed(egui::Key::D) {
            if let Some((start, end)) = Self::selection_range(cursor, anchor) {
                Self::remove_char_range(&mut self.query, start, end);
                cursor = start;
                anchor = start;
                text_changed = true;
                cursor_changed = true;
            } else {
                let end = Self::char_count(&self.query);
                if cursor < end {
                    Self::remove_char_range(&mut self.query, cursor, cursor + 1);
                    text_changed = true;
                    cursor_changed = true;
                }
            }
        } else if pressed(egui::Key::W) {
            if let Some((start, end)) = Self::selection_range(cursor, anchor) {
                self.kill_buffer = Self::remove_char_range(&mut self.query, start, end);
                cursor = start;
                anchor = start;
                text_changed = true;
                cursor_changed = true;
            } else if cursor > 0 {
                let chars: Vec<char> = self.query.chars().collect();
                let mut start = cursor;
                while start > 0 && chars[start - 1].is_whitespace() {
                    start -= 1;
                }
                while start > 0 && Self::is_word_char(chars[start - 1]) {
                    start -= 1;
                }
                if start < cursor {
                    self.kill_buffer = Self::remove_char_range(&mut self.query, start, cursor);
                    cursor = start;
                    anchor = start;
                    text_changed = true;
                    cursor_changed = true;
                }
            }
        } else if pressed(egui::Key::K) {
            let end = Self::char_count(&self.query);
            if cursor < end {
                self.kill_buffer = Self::remove_char_range(&mut self.query, cursor, end);
                anchor = cursor;
                text_changed = true;
                cursor_changed = true;
            }
        } else if pressed(egui::Key::Y) {
            if !self.kill_buffer.is_empty() {
                if let Some((start, end)) = Self::selection_range(cursor, anchor) {
                    Self::remove_char_range(&mut self.query, start, end);
                    cursor = start;
                }
                Self::insert_at_char(&mut self.query, cursor, &self.kill_buffer);
                cursor += Self::char_count(&self.kill_buffer);
                anchor = cursor;
                text_changed = true;
                cursor_changed = true;
            }
        } else if pressed(egui::Key::U) && cursor > 0 {
            Self::remove_char_range(&mut self.query, 0, cursor);
            cursor = 0;
            anchor = 0;
            text_changed = true;
            cursor_changed = true;
        }

        if cursor_changed {
            output
                .state
                .set_ccursor_range(Some(egui::text_edit::CCursorRange::two(
                    egui::text::CCursor::new(anchor),
                    egui::text::CCursor::new(cursor),
                )));
            output.state.clone().store(ctx, output.response.id);
            ctx.request_repaint();
        }

        text_changed
    }

    fn render_results_and_preview(&mut self, ui: &mut egui::Ui) {
        if self.show_preview {
            let max_results_width = (ui.available_width() - Self::MIN_PREVIEW_PANEL_WIDTH)
                .max(Self::MIN_RESULTS_PANEL_WIDTH);
            let panel = egui::SidePanel::left("results-panel")
                .resizable(true)
                .default_width(self.results_panel_width.min(max_results_width))
                .min_width(Self::MIN_RESULTS_PANEL_WIDTH)
                .max_width(max_results_width);
            let response = panel.show_inside(ui, |ui| {
                self.render_results_list(ui);
            });
            let new_width = response
                .response
                .rect
                .width()
                .max(Self::MIN_RESULTS_PANEL_WIDTH);
            if (new_width - self.results_panel_width).abs() > 1.0 {
                self.results_panel_width = new_width;
                self.mark_ui_state_dirty();
            }
            ui.heading("Preview");
            let preview_size = ui.available_size();
            ui.add_sized(
                preview_size,
                egui::TextEdit::multiline(&mut self.preview).interactive(false),
            );
        } else {
            self.render_results_list(ui);
        }
        self.scroll_to_current = false;
    }

    fn render_results_list(&mut self, ui: &mut egui::Ui) {
        ui.heading("Results");
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut clicked_row: Option<usize> = None;
                let mut execute_row: Option<usize> = None;
                let prefer_relative = self.prefer_relative_display();

                for i in 0..self.results.len() {
                    let Some((path, _score)) = self.results.get(i) else {
                        continue;
                    };
                    let is_current = self.current_row == Some(i);
                    let is_pinned = self.pinned_paths.contains(path);
                    let marker_current = if is_current { "▶" } else { "·" };
                    let marker_pin = if is_pinned { "◆" } else { "·" };
                    let is_dir = self.entry_kinds.get(path).copied().unwrap_or(false);
                    let display = display_path_with_mode(path, &self.root, prefer_relative);
                    let positions = match_positions_for_path(
                        path,
                        &self.root,
                        &self.query,
                        prefer_relative,
                        self.use_regex,
                    );

                    let mut job = egui::text::LayoutJob::default();
                    job.append(
                        &format!("{} {} ", marker_current, marker_pin),
                        0.0,
                        egui::TextFormat {
                            color: if is_current {
                                egui::Color32::LIGHT_BLUE
                            } else {
                                egui::Color32::GRAY
                            },
                            ..Default::default()
                        },
                    );
                    let kind = if is_dir { "DIR " } else { "FILE" };
                    job.append(
                        kind,
                        0.0,
                        egui::TextFormat {
                            color: if is_dir {
                                egui::Color32::from_rgb(52, 211, 153)
                            } else {
                                egui::Color32::from_rgb(96, 165, 250)
                            },
                            ..Default::default()
                        },
                    );
                    job.append(" ", 0.0, egui::TextFormat::default());

                    for (idx, ch) in display.chars().enumerate() {
                        let color = if positions.contains(&idx) {
                            egui::Color32::from_rgb(245, 158, 11)
                        } else {
                            egui::Color32::from_rgb(229, 231, 235)
                        };
                        job.append(
                            &ch.to_string(),
                            0.0,
                            egui::TextFormat {
                                color,
                                ..Default::default()
                            },
                        );
                    }

                    let selected_bg = if ui.visuals().dark_mode {
                        egui::Color32::from_rgb(48, 53, 62)
                    } else {
                        egui::Color32::from_rgb(228, 232, 238)
                    };
                    let fill = if is_current {
                        selected_bg
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    let row = egui::Frame::none()
                        .fill(fill)
                        .inner_margin(egui::Margin::symmetric(3.0, 2.0))
                        .rounding(egui::Rounding::same(3.0))
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(job)
                                    .wrap(false)
                                    .sense(egui::Sense::click()),
                            )
                        });
                    let response = row.inner;
                    if is_current && self.scroll_to_current {
                        response.scroll_to_me(None);
                    }
                    if response.clicked() {
                        clicked_row = Some(i);
                    }
                    if response.double_clicked() {
                        execute_row = Some(i);
                    }
                }
                if let Some(i) = clicked_row {
                    self.current_row = Some(i);
                    self.request_preview_for_current();
                    self.refresh_status_line();
                }
                if let Some(i) = execute_row {
                    self.current_row = Some(i);
                    self.execute_selected();
                }
            });
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown))
            || ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::N))
        {
            self.move_row(1);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp))
            || ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::P))
        {
            self.move_row(-1);
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::V)) {
            self.move_page(1);
        }
        if ctx.input(|i| i.modifiers.alt && i.key_pressed(egui::Key::V)) {
            self.move_page(-1);
        }

        let tab_forward = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab));
        if tab_forward {
            self.toggle_pin_and_move(1);
            // Keep keyboard focus on query input to avoid default widget focus traversal.
            self.focus_query_requested = true;
        }

        let tab_backward = ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab));
        if tab_backward {
            self.toggle_pin_and_move(-1);
            self.focus_query_requested = true;
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Enter))
            || ctx.input(|i| {
                i.modifiers.ctrl && (i.key_pressed(egui::Key::J) || i.key_pressed(egui::Key::M))
            })
        {
            self.execute_selected();
        }
        let copy_mod = egui::Modifiers {
            ctrl: true,
            shift: true,
            ..Default::default()
        };
        if ctx.input_mut(|i| i.consume_key(copy_mod, egui::Key::C)) {
            self.copy_selected_paths(ctx);
        }

        let ctrl_mod = egui::Modifiers {
            ctrl: true,
            ..Default::default()
        };
        if ctx.input_mut(|i| i.consume_key(ctrl_mod, egui::Key::L)) {
            let has_focus = ctx.memory(|m| m.has_focus(self.query_input_id));
            if has_focus {
                self.focus_query_requested = false;
                self.unfocus_query_requested = true;
            } else {
                self.focus_query_requested = true;
                self.unfocus_query_requested = false;
            }
        }
        if ctx.input_mut(|i| i.consume_key(ctrl_mod, egui::Key::G)) {
            self.clear_query_and_selection();
        }
    }
}

impl eframe::App for FlistWalkerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_pending_window_restore(ctx);
        self.poll_index_response();
        self.poll_search_response();
        self.poll_preview_response();
        self.poll_filelist_response();
        self.handle_shortcuts(ctx);
        if self.search_in_progress
            || self.index_in_progress
            || self.preview_in_progress
            || self.filelist_in_progress
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
        self.capture_window_geometry(ctx);

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let row_height = ui.spacing().interact_size.y;
                ui.add_sized([44.0, row_height], egui::Label::new("Root:"));
                let button_width = 96.0;
                let set_default_width = 130.0;
                let add_width = 100.0;
                let remove_width = 130.0;
                let field_width = (ui.available_width()
                    - button_width
                    - add_width
                    - set_default_width
                    - remove_width
                    - (ui.spacing().item_spacing.x * 4.0))
                    .max(120.0);
                let selected_text = self.root_display_text();
                let mut next_root: Option<PathBuf> = None;
                ui.allocate_ui_with_layout(
                    egui::vec2(field_width, row_height),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        egui::ComboBox::from_id_source("root-selector")
                            .width(field_width)
                            .selected_text(selected_text)
                            .show_ui(ui, |ui| {
                                for p in &self.saved_roots {
                                    let text = Self::normalize_windows_path(p.clone())
                                        .to_string_lossy()
                                        .to_string();
                                    let is_selected =
                                        Self::path_key(p) == Self::path_key(&self.root);
                                    if ui.selectable_label(is_selected, text).clicked() {
                                        next_root = Some(p.clone());
                                    }
                                }
                            });
                    },
                );
                if ui
                    .add_sized([button_width, row_height], egui::Button::new("Browse..."))
                    .clicked()
                {
                    let dialog_root = Self::normalize_windows_path(self.root.clone());
                    match native_dialog::FileDialog::new()
                        .set_location(&dialog_root)
                        .show_open_single_dir()
                    {
                        Ok(Some(dir)) => {
                            self.apply_root_change(dir);
                        }
                        Ok(None) => {}
                        Err(err) => {
                            self.set_notice(format!("Browse failed: {}", err));
                        }
                    }
                }
                if ui
                    .add_sized(
                        [set_default_width, row_height],
                        egui::Button::new("Set as default"),
                    )
                    .clicked()
                {
                    self.set_current_root_as_default();
                }
                if ui
                    .add_sized([add_width, row_height], egui::Button::new("Add to list"))
                    .clicked()
                {
                    self.add_current_root_to_saved();
                }
                if ui
                    .add_sized(
                        [remove_width, row_height],
                        egui::Button::new("Remove from list"),
                    )
                    .clicked()
                {
                    self.remove_current_root_from_saved();
                }
                if let Some(root) = next_root {
                    self.apply_root_change(root);
                }
            });

            ui.horizontal(|ui| {
                let mut reindex = false;
                let mut filter_changed = false;
                reindex |= ui
                    .checkbox(&mut self.use_filelist, "Use FileList")
                    .changed();
                if ui.checkbox(&mut self.use_regex, "Regex").changed() {
                    self.update_results();
                }
                filter_changed |= ui.checkbox(&mut self.include_files, "Files").changed();
                filter_changed |= ui.checkbox(&mut self.include_dirs, "Folders").changed();
                if ui.checkbox(&mut self.show_preview, "Preview").changed() {
                    self.mark_ui_state_dirty();
                }
                filter_changed |= self.ensure_entry_filters();
                ui.separator();
                ui.label(self.source_text());
                if filter_changed {
                    self.apply_entry_filters(false);
                }
                if reindex {
                    self.request_index_refresh();
                }
            });

            let mut output = egui::TextEdit::singleline(&mut self.query)
                .id(self.query_input_id)
                .lock_focus(true)
                .desired_width(f32::INFINITY)
                .hint_text("Type to fuzzy-search files/folders...")
                .show(ui);
            if self.focus_query_requested {
                output.response.request_focus();
                self.focus_query_requested = false;
            }
            if self.unfocus_query_requested {
                output.response.surrender_focus();
                self.unfocus_query_requested = false;
            }
            if self.apply_emacs_query_shortcuts(ctx, &mut output) {
                self.update_results();
            }
            if output.response.changed() {
                self.update_results();
            }

            ui.horizontal(|ui| {
                if ui.button("Open / Execute").clicked() {
                    self.execute_selected();
                }
                if ui.button("Copy Path(s)").clicked() {
                    self.copy_selected_paths(ctx);
                }
                if ui.button("Clear Selected").clicked() {
                    self.clear_pinned();
                }
                let create_label = if self.filelist_in_progress {
                    "Create File List (Running...)"
                } else {
                    "Create File List"
                };
                if ui.button(create_label).clicked() {
                    self.create_filelist();
                }
                if ui.button("Refresh Index").clicked() {
                    self.request_index_refresh();
                }
            });
        });

        egui::TopBottomPanel::bottom("status")
            .resizable(false)
            .exact_height(24.0)
            .show(ctx, |ui| {
                ui.add(egui::Label::new(&self.status_line).truncate(true));
            });

        let mut overwrite = false;
        let mut cancel_overwrite = false;
        if let Some(pending) = &self.pending_filelist_confirmation {
            egui::Window::new("Overwrite FileList?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.label(format!(
                        "{} already exists. Overwrite it?",
                        pending.existing_path.display()
                    ));
                    ui.horizontal(|ui| {
                        if ui.button("Overwrite").clicked() {
                            overwrite = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel_overwrite = true;
                        }
                    });
                });
        }
        if overwrite {
            self.confirm_pending_filelist_overwrite();
        } else if cancel_overwrite {
            self.cancel_pending_filelist_overwrite();
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_results_and_preview(ui);
        });
        self.maybe_save_ui_state(false);
    }
}

impl Drop for FlistWalkerApp {
    fn drop(&mut self) {
        self.maybe_save_ui_state(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::mpsc;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("fff-rs-app-{name}-{nonce}"))
    }

    fn entries_count_from_status(status_line: &str) -> usize {
        status_line
            .strip_prefix("Entries: ")
            .and_then(|rest| rest.split(" | ").next())
            .and_then(|n| n.parse::<usize>().ok())
            .unwrap_or(0)
    }

    #[test]
    fn clear_query_and_selection_clears_state() {
        let root = test_root("clear");
        fs::create_dir_all(&root).expect("create dir");
        let file = root.join("a.txt");
        fs::write(&file, "x").expect("write file");

        let mut app = FlistWalkerApp::new(root.clone(), 50, "abc".to_string());
        app.pinned_paths.insert(file.clone());
        app.current_row = Some(0);
        app.preview = "preview".to_string();

        app.clear_query_and_selection();

        assert!(app.query.is_empty());
        assert!(app.pinned_paths.is_empty());
        assert!(app.focus_query_requested);
        assert!(app.notice.contains("Cleared selection and query"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn move_row_sets_scroll_tracking() {
        let root = test_root("scroll");
        fs::create_dir_all(&root).expect("create dir");
        let file1 = root.join("a.txt");
        let file2 = root.join("b.txt");
        fs::write(&file1, "x").expect("write file1");
        fs::write(&file2, "x").expect("write file2");

        let mut app = FlistWalkerApp::new(root.clone(), 50, "".to_string());
        app.results = vec![(file1, 0.0), (file2, 0.0)];
        app.current_row = Some(0);
        app.scroll_to_current = false;

        app.move_row(1);

        assert_eq!(app.current_row, Some(1));
        assert!(app.scroll_to_current);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ctrl_h_deletes_only_one_char_when_widget_did_not_change_text() {
        let root = test_root("ctrl-h-single");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.query = "abcd".to_string();
        let mut cursor = 4usize;
        let mut anchor = 4usize;

        let (text_changed, cursor_changed) =
            app.apply_ctrl_h_delete(&mut cursor, &mut anchor, false);

        assert!(text_changed);
        assert!(cursor_changed);
        assert_eq!(app.query, "abc");
        assert_eq!(cursor, 3);
        assert_eq!(anchor, 3);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ctrl_h_does_not_delete_twice_when_widget_already_changed_text() {
        let root = test_root("ctrl-h-guard");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        // Simulate that TextEdit already handled one Backspace and this frame query is already updated.
        app.query = "abc".to_string();
        let mut cursor = 3usize;
        let mut anchor = 3usize;

        let (text_changed, cursor_changed) =
            app.apply_ctrl_h_delete(&mut cursor, &mut anchor, true);

        assert!(!text_changed);
        assert!(!cursor_changed);
        assert_eq!(app.query, "abc");
        assert_eq!(cursor, 3);
        assert_eq!(anchor, 3);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn prefer_relative_display_is_enabled_for_filelist_source() {
        let root = test_root("prefer-relative-filelist");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.index.source = IndexSource::FileList(root.join("FileList.txt"));

        assert!(app.prefer_relative_display());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn regex_query_is_not_filtered_out_by_visible_match_guard() {
        let root = PathBuf::from("/tmp");
        let results = vec![(PathBuf::from("/tmp/src/main.py"), 42.0)];

        let out = filter_search_results(results, &root, "ma.*py", true, true);

        assert_eq!(out.len(), 1);
    }

    #[test]
    fn preview_cache_is_bounded() {
        let root = test_root("preview-cache-bounded");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

        for i in 0..=FlistWalkerApp::PREVIEW_CACHE_MAX {
            let path = root.join(format!("file-{i}.txt"));
            app.cache_preview(path.clone(), format!("preview-{i}"));
        }

        assert_eq!(app.preview_cache.len(), FlistWalkerApp::PREVIEW_CACHE_MAX);
        assert_eq!(
            app.preview_cache_order.len(),
            FlistWalkerApp::PREVIEW_CACHE_MAX
        );
        let evicted = root.join("file-0.txt");
        assert!(!app.preview_cache.contains_key(&evicted));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn search_error_updates_notice() {
        let root = test_root("search-error-notice");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        let (tx, rx) = mpsc::channel::<SearchResponse>();
        app.search_rx = rx;
        app.pending_request_id = Some(7);
        app.search_in_progress = true;

        tx.send(SearchResponse {
            request_id: 7,
            results: Vec::new(),
            error: Some("invalid regex '[*': syntax error".to_string()),
        })
        .expect("send search response");

        app.poll_search_response();

        assert!(!app.search_in_progress);
        assert!(app.notice.contains("Search failed:"));
        assert!(app.notice.contains("invalid regex"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn stale_search_response_is_ignored_after_index_refresh() {
        let root = test_root("stale-search-after-refresh");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, "abc".to_string());
        let (search_tx, search_rx) = mpsc::channel::<SearchResponse>();
        let (index_tx, _index_rx) = mpsc::channel::<IndexRequest>();
        app.search_rx = search_rx;
        app.index_tx = index_tx;
        app.pending_request_id = Some(5);
        app.search_in_progress = true;
        app.results = vec![(root.join("before.txt"), 0.0)];

        app.request_index_refresh();

        search_tx
            .send(SearchResponse {
                request_id: 5,
                results: vec![(root.join("stale.txt"), 1.0)],
                error: None,
            })
            .expect("send stale search response");

        app.poll_search_response();

        assert!(!app.search_in_progress);
        assert_eq!(app.pending_request_id, None);
        assert_eq!(app.results[0].0, root.join("before.txt"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn index_refresh_marks_search_resume_pending_for_non_empty_query() {
        let root = test_root("resume-pending-on-refresh");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, "abc".to_string());
        let (index_tx, _index_rx) = mpsc::channel::<IndexRequest>();
        app.index_tx = index_tx;

        app.request_index_refresh();

        assert!(app.search_resume_pending);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn non_empty_query_resumes_search_immediately_on_first_index_batch() {
        let root = test_root("resume-first-batch");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, "main".to_string());
        let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
        app.index_tx = index_tx;
        // Use a manual search channel so the test can inspect enqueued requests.
        let (search_tx_real, search_rx_real) = mpsc::channel::<SearchRequest>();
        app.search_tx = search_tx_real;

        app.request_index_refresh();
        let req = index_rx.try_recv().expect("index request should be sent");

        let (tx_idx, rx_idx) = mpsc::channel::<IndexResponse>();
        app.index_rx = rx_idx;
        tx_idx
            .send(IndexResponse::Batch {
                request_id: req.request_id,
                entries: vec![IndexEntry {
                    path: root.join("main.rs"),
                    is_dir: false,
                }],
            })
            .expect("send batch");

        // Simulate that normal throttle window has not elapsed yet.
        app.last_incremental_results_refresh = Instant::now();
        app.poll_index_response();

        let search_req = search_rx_real
            .try_recv()
            .expect("search should resume immediately");
        assert_eq!(search_req.query, "main");
        assert!(!app.search_resume_pending);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn create_filelist_waits_while_indexing() {
        let root = test_root("filelist-waits-indexing");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.index_in_progress = true;

        app.create_filelist();

        assert_eq!(app.pending_filelist_after_index_root, Some(root.clone()));
        assert!(app.pending_filelist_request_id.is_none());
        assert!(!app.filelist_in_progress);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn deferred_filelist_starts_after_index_finished() {
        let root = test_root("filelist-after-index-finished");
        fs::create_dir_all(&root).expect("create dir");
        let path = root.join("main.rs");
        fs::write(&path, "fn main() {}").expect("write file");

        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        let (filelist_tx, filelist_rx) = mpsc::channel::<FileListRequest>();
        app.filelist_tx = filelist_tx;
        let (index_tx, index_rx) = mpsc::channel::<IndexResponse>();
        app.index_rx = index_rx;

        app.index_in_progress = true;
        app.pending_index_request_id = Some(77);
        app.entry_kinds.insert(path.clone(), false);
        app.index.entries = vec![path.clone()];
        app.create_filelist();

        index_tx
            .send(IndexResponse::Finished {
                request_id: 77,
                source: IndexSource::Walker,
            })
            .expect("send finished");
        app.poll_index_response();

        let req = filelist_rx
            .try_recv()
            .expect("filelist request should be sent");
        assert_eq!(req.root, root);
        assert_eq!(req.entries, vec![path]);
        assert!(app.pending_filelist_after_index_root.is_none());
        assert!(app.filelist_in_progress);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn deferred_filelist_is_canceled_when_root_changes() {
        let root_old = test_root("filelist-deferred-cancel-old");
        let root_new = test_root("filelist-deferred-cancel-new");
        fs::create_dir_all(&root_old).expect("create old dir");
        fs::create_dir_all(&root_new).expect("create new dir");
        let mut app = FlistWalkerApp::new(root_old.clone(), 50, String::new());
        let (index_tx, _index_rx) = mpsc::channel::<IndexRequest>();
        app.index_tx = index_tx;
        app.pending_filelist_after_index_root = Some(root_old.clone());
        app.root = root_new.clone();

        app.request_index_refresh();

        assert!(app.pending_filelist_after_index_root.is_none());
        assert!(app.notice.contains("Deferred Create File List canceled"));
        let _ = fs::remove_dir_all(&root_old);
        let _ = fs::remove_dir_all(&root_new);
    }

    #[test]
    fn root_change_clears_stale_selection_state() {
        let root_old = test_root("root-change-clear-selection-old");
        let root_new = test_root("root-change-clear-selection-new");
        fs::create_dir_all(&root_old).expect("create old dir");
        fs::create_dir_all(&root_new).expect("create new dir");
        let old_path = root_old.join("old.txt");
        fs::write(&old_path, "x").expect("write old file");

        let mut app = FlistWalkerApp::new(root_old.clone(), 50, String::new());
        let (tx, rx) = mpsc::channel::<IndexRequest>();
        app.index_tx = tx;
        app.pinned_paths.insert(old_path);
        app.current_row = Some(0);
        app.preview = "stale preview".to_string();
        app.results = vec![(root_old.join("result.txt"), 0.0)];

        app.apply_root_change(root_new.clone());

        assert!(app.pinned_paths.is_empty());
        assert_eq!(app.current_row, None);
        assert!(app.preview.is_empty());
        let req = rx.try_recv().expect("index request should be sent");
        assert_eq!(req.root, root_new);
        let _ = fs::remove_dir_all(&root_old);
        let _ = fs::remove_dir_all(&root_new);
    }

    #[test]
    fn root_change_cancels_pending_filelist_overwrite_confirmation() {
        let root_old = test_root("root-change-cancel-overwrite-old");
        let root_new = test_root("root-change-cancel-overwrite-new");
        fs::create_dir_all(&root_old).expect("create old dir");
        fs::create_dir_all(&root_new).expect("create new dir");

        let mut app = FlistWalkerApp::new(root_old.clone(), 50, String::new());
        let (tx, _rx) = mpsc::channel::<IndexRequest>();
        app.index_tx = tx;
        app.pending_filelist_confirmation = Some(PendingFileListConfirmation {
            root: root_old.clone(),
            entries: vec![root_old.join("a.txt")],
            existing_path: root_old.join("FileList.txt"),
        });

        app.apply_root_change(root_new.clone());

        assert!(app.pending_filelist_confirmation.is_none());
        let _ = fs::remove_dir_all(&root_old);
        let _ = fs::remove_dir_all(&root_new);
    }

    #[test]
    fn filelist_finished_updates_state_and_notice() {
        let root = test_root("filelist-finished");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        let (tx, rx) = mpsc::channel::<FileListResponse>();
        app.filelist_rx = rx;
        app.pending_filelist_request_id = Some(11);
        app.pending_filelist_root = Some(root.clone());
        app.filelist_in_progress = true;
        app.use_filelist = false;

        let filelist = root.join("FileList.txt");
        tx.send(FileListResponse::Finished {
            request_id: 11,
            root: root.clone(),
            path: filelist.clone(),
            count: 3,
        })
        .expect("send filelist response");

        app.poll_filelist_response();

        assert_eq!(app.pending_filelist_request_id, None);
        assert!(!app.filelist_in_progress);
        assert!(app.notice.contains("Created"));
        assert!(app.notice.contains("3 entries"));
        assert!(app.notice.contains(filelist.to_string_lossy().as_ref()));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn create_filelist_requests_overwrite_confirmation_when_file_exists() {
        let root = test_root("filelist-overwrite-confirm");
        fs::create_dir_all(&root).expect("create dir");
        fs::write(root.join("FileList.txt"), "old\n").expect("write filelist");
        let path = root.join("main.rs");
        fs::write(&path, "fn main() {}").expect("write file");

        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.index_in_progress = false;
        app.all_entries = Arc::new(vec![path.clone()]);
        app.entry_kinds.insert(path, false);

        app.create_filelist();

        assert!(app.pending_filelist_confirmation.is_some());
        assert!(!app.filelist_in_progress);
        assert!(app.pending_filelist_request_id.is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn confirm_pending_overwrite_starts_filelist_creation() {
        let root = test_root("filelist-overwrite-confirm-start");
        fs::create_dir_all(&root).expect("create dir");
        let file_path = root.join("FileList.txt");
        let entries = vec![root.join("src/main.rs")];
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        let (filelist_tx, filelist_rx) = mpsc::channel::<FileListRequest>();
        app.filelist_tx = filelist_tx;
        app.pending_filelist_confirmation = Some(PendingFileListConfirmation {
            root: root.clone(),
            entries: entries.clone(),
            existing_path: file_path,
        });

        app.confirm_pending_filelist_overwrite();

        let req = filelist_rx
            .try_recv()
            .expect("filelist request should be sent");
        assert_eq!(req.root, root);
        assert_eq!(req.entries, entries);
        assert!(app.filelist_in_progress);
        assert!(app.pending_filelist_confirmation.is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn filelist_finished_triggers_reindex_when_enabled() {
        let root = test_root("filelist-reindex");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        let (filelist_tx, filelist_rx) = mpsc::channel::<FileListResponse>();
        app.filelist_rx = filelist_rx;
        let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
        app.index_tx = index_tx;
        app.pending_filelist_request_id = Some(12);
        app.pending_filelist_root = Some(root.clone());
        app.filelist_in_progress = true;
        app.use_filelist = true;

        filelist_tx
            .send(FileListResponse::Finished {
                request_id: 12,
                root: root.clone(),
                path: root.join("FileList.txt"),
                count: 5,
            })
            .expect("send filelist response");

        app.poll_filelist_response();

        let req = index_rx.try_recv().expect("reindex request should be sent");
        assert_eq!(req.root, root);
        assert!(req.use_filelist);
        assert!(app.index_in_progress);
        assert!(app.pending_index_request_id.is_some());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn filelist_failed_updates_state_and_notice() {
        let root = test_root("filelist-failed");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        let (tx, rx) = mpsc::channel::<FileListResponse>();
        app.filelist_rx = rx;
        app.pending_filelist_request_id = Some(13);
        app.pending_filelist_root = Some(root.clone());
        app.filelist_in_progress = true;

        tx.send(FileListResponse::Failed {
            request_id: 13,
            root: root.clone(),
            error: "disk full".to_string(),
        })
        .expect("send filelist response");

        app.poll_filelist_response();

        assert_eq!(app.pending_filelist_request_id, None);
        assert!(!app.filelist_in_progress);
        assert!(app.notice.contains("Create File List failed: disk full"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn filelist_finished_for_previous_root_does_not_trigger_reindex() {
        let root_old = test_root("filelist-prev-root-old");
        let root_new = test_root("filelist-prev-root-new");
        fs::create_dir_all(&root_old).expect("create old dir");
        fs::create_dir_all(&root_new).expect("create new dir");
        let mut app = FlistWalkerApp::new(root_old.clone(), 50, String::new());
        let (filelist_tx, filelist_rx) = mpsc::channel::<FileListResponse>();
        app.filelist_rx = filelist_rx;
        let (_index_tx, index_rx) = mpsc::channel::<IndexRequest>();
        app.index_tx = _index_tx;
        app.pending_filelist_request_id = Some(51);
        app.pending_filelist_root = Some(root_old.clone());
        app.filelist_in_progress = true;
        app.use_filelist = true;
        app.root = root_new.clone();

        filelist_tx
            .send(FileListResponse::Finished {
                request_id: 51,
                root: root_old.clone(),
                path: root_old.join("FileList.txt"),
                count: 9,
            })
            .expect("send filelist response");

        app.poll_filelist_response();

        assert!(index_rx.try_recv().is_err());
        assert!(!app.filelist_in_progress);
        assert!(app.notice.contains("previous root"));
        let _ = fs::remove_dir_all(&root_old);
        let _ = fs::remove_dir_all(&root_new);
    }

    #[test]
    fn filelist_failed_for_previous_root_reports_without_rewinding_state() {
        let root_old = test_root("filelist-prev-root-fail-old");
        let root_new = test_root("filelist-prev-root-fail-new");
        fs::create_dir_all(&root_old).expect("create old dir");
        fs::create_dir_all(&root_new).expect("create new dir");
        let mut app = FlistWalkerApp::new(root_old.clone(), 50, String::new());
        let (tx, rx) = mpsc::channel::<FileListResponse>();
        app.filelist_rx = rx;
        app.pending_filelist_request_id = Some(52);
        app.pending_filelist_root = Some(root_old.clone());
        app.filelist_in_progress = true;
        app.root = root_new;

        tx.send(FileListResponse::Failed {
            request_id: 52,
            root: root_old.clone(),
            error: "permission denied".to_string(),
        })
        .expect("send filelist response");

        app.poll_filelist_response();

        assert_eq!(app.pending_filelist_request_id, None);
        assert!(!app.filelist_in_progress);
        assert!(app.notice.contains("previous root"));
        let _ = fs::remove_dir_all(&root_old);
    }

    #[test]
    fn non_empty_query_incremental_refresh_updates_entries_with_small_delta() {
        let root = test_root("incremental-small-delta");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, "main".to_string());
        let (tx, rx) = mpsc::channel::<IndexResponse>();
        app.index_rx = rx;
        app.pending_index_request_id = Some(21);
        app.index_in_progress = true;
        app.last_incremental_results_refresh = Instant::now() - Duration::from_secs(1);

        let path = root.join("main.rs");
        tx.send(IndexResponse::Batch {
            request_id: 21,
            entries: vec![IndexEntry {
                path: path.clone(),
                is_dir: false,
            }],
        })
        .expect("send index batch");

        app.poll_index_response();

        assert_eq!(app.entries.len(), 1);
        assert_eq!(app.entries[0], path);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn empty_query_keeps_results_after_batch_and_finished_in_same_poll() {
        let root = test_root("empty-query-finished-priority");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        let (tx, rx) = mpsc::channel::<IndexResponse>();
        app.index_rx = rx;
        app.pending_index_request_id = Some(31);
        app.index_in_progress = true;

        let path = root.join("main.rs");
        tx.send(IndexResponse::Batch {
            request_id: 31,
            entries: vec![IndexEntry {
                path: path.clone(),
                is_dir: false,
            }],
        })
        .expect("send index batch");
        tx.send(IndexResponse::Finished {
            request_id: 31,
            source: IndexSource::Walker,
        })
        .expect("send index finished");

        app.poll_index_response();

        assert_eq!(app.entries.len(), 1);
        assert_eq!(app.results.len(), 1);
        assert_eq!(app.entries[0], path);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn app_defaults_use_filelist_off() {
        let root = test_root("default-use-filelist-off");
        fs::create_dir_all(&root).expect("create dir");
        let app = FlistWalkerApp::new(root.clone(), 50, String::new());
        assert!(!app.use_filelist);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn status_line_prefers_current_index_count_while_indexing() {
        let root = test_root("status-line-current-index-count");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.index_in_progress = true;
        app.all_entries = Arc::new(
            (0..10)
                .map(|i| root.join(format!("old-{i}.txt")))
                .collect::<Vec<_>>(),
        );
        app.index.entries = (0..3)
            .map(|i| root.join(format!("new-{i}.txt")))
            .collect::<Vec<_>>();

        app.refresh_status_line();

        assert_eq!(entries_count_from_status(&app.status_line), 3);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn request_index_refresh_keeps_existing_entries_visible_until_new_results_arrive() {
        let root = test_root("refresh-keeps-visible");
        fs::create_dir_all(&root).expect("create dir");
        let path = root.join("keep.txt");
        fs::write(&path, "x").expect("write file");

        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        let (tx, _rx) = mpsc::channel::<IndexRequest>();
        app.index_tx = tx;
        app.entries = Arc::new(vec![path.clone()]);
        app.results = vec![(path.clone(), 0.0)];
        app.current_row = Some(0);
        app.preview = "keep".to_string();

        app.request_index_refresh();

        assert_eq!(app.entries.len(), 1);
        assert_eq!(app.results.len(), 1);
        assert_eq!(app.current_row, Some(0));
        assert_eq!(app.preview, "keep");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn incremental_empty_query_update_preserves_scroll_position_flag() {
        let root = test_root("incremental-preserve-scroll");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        let (tx, rx) = mpsc::channel::<IndexResponse>();
        app.index_rx = rx;
        app.pending_index_request_id = Some(41);
        app.index_in_progress = true;
        app.scroll_to_current = false;
        app.current_row = Some(0);

        let path = root.join("main.rs");
        tx.send(IndexResponse::Batch {
            request_id: 41,
            entries: vec![IndexEntry {
                path,
                is_dir: false,
            }],
        })
        .expect("send index batch");

        app.poll_index_response();

        assert!(!app.scroll_to_current);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn move_page_moves_by_fixed_rows_and_clamps() {
        let root = test_root("move-page");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.results = (0..30)
            .map(|i| (root.join(format!("f{i}.txt")), 0.0))
            .collect();
        app.current_row = Some(0);

        app.move_page(1);
        assert_eq!(app.current_row, Some(10));

        app.move_page(-1);
        assert_eq!(app.current_row, Some(0));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn request_index_refresh_reenables_files_when_both_filters_are_off() {
        let root = test_root("request-refresh-filter-guard");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        let (tx, rx) = mpsc::channel::<IndexRequest>();
        app.index_tx = tx;
        app.include_files = false;
        app.include_dirs = false;

        app.request_index_refresh();

        let req = rx.try_recv().expect("index request should be sent");
        assert!(req.include_files);
        assert!(req.include_dirs);
        assert!(app.include_files);
        assert!(!app.include_dirs);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn request_index_refresh_uses_latest_toggle_state() {
        let root = test_root("request-refresh-toggle-state");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        let (tx, rx) = mpsc::channel::<IndexRequest>();
        app.index_tx = tx;
        app.use_filelist = false;
        app.include_files = false;
        app.include_dirs = true;

        app.request_index_refresh();

        let req = rx.try_recv().expect("index request should be sent");
        assert!(!req.use_filelist);
        assert!(req.include_files);
        assert!(req.include_dirs);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn clipboard_text_normalizes_extended_and_unc_paths() {
        let paths = vec![
            PathBuf::from(r"\\?\C:\Users\tester\file.txt"),
            PathBuf::from(r"\\?\UNC\server\share\folder\file.txt"),
        ];
        let text = FlistWalkerApp::clipboard_paths_text(&paths);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], r"C:\Users\tester\file.txt");
        assert_eq!(lines[1], r"\\server\share\folder\file.txt");
    }
}

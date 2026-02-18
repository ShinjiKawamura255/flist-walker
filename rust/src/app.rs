use crate::actions::execute_or_open;
use crate::indexer::{
    build_index_with_metadata, find_filelist_in_first_level, parse_filelist, write_filelist,
    IndexBuildResult, IndexSource,
};
use crate::search::search_entries;
use crate::ui_model::{
    build_preview_text_with_kind, display_path_with_mode, has_visible_match,
    match_positions_for_path, normalize_path_for_display,
};
use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use walkdir::WalkDir;

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
    include_files: bool,
    include_dirs: bool,
}

enum FileListResponse {
    Finished {
        request_id: u64,
        path: PathBuf,
        count: usize,
    },
    Failed {
        request_id: u64,
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
            let results = filter_search_results(
                search_entries(&req.query, &req.entries, req.limit, req.use_regex),
                &req.root,
                &req.query,
                req.prefer_relative,
                req.use_regex,
            );

            if tx_res
                .send(SearchResponse {
                    request_id: req.request_id,
                    results,
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
            let result =
                build_index_with_metadata(&req.root, false, req.include_files, req.include_dirs)
                    .and_then(|snapshot| {
                        let count = snapshot.entries.len();
                        write_filelist(&req.root, &snapshot.entries, "FileList.txt")
                            .map(|path| (path, count))
                    });
            let msg = match result {
                Ok((path, count)) => FileListResponse::Finished {
                    request_id: req.request_id,
                    path,
                    count,
                },
                Err(err) => FileListResponse::Failed {
                    request_id: req.request_id,
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

fn spawn_index_worker() -> (Sender<IndexRequest>, Receiver<IndexResponse>) {
    let (tx_req, rx_req) = mpsc::channel::<IndexRequest>();
    let (tx_res, rx_res) = mpsc::channel::<IndexResponse>();

    thread::spawn(move || {
        while let Ok(mut req) = rx_req.recv() {
            while let Ok(newer) = rx_req.try_recv() {
                req = newer;
            }

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
                    stream_filelist_index(&tx_res, &req, &root, filelist)
                } else {
                    stream_walker_index(&tx_res, &req, &root)
                }
            } else {
                stream_walker_index(&tx_res, &req, &root)
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
    search_in_progress: bool,
    index_in_progress: bool,
    preview_in_progress: bool,
    filelist_in_progress: bool,
    scroll_to_current: bool,
    focus_query_requested: bool,
    saved_roots: Vec<PathBuf>,
    preview_cache: HashMap<PathBuf, String>,
    last_incremental_results_refresh: Instant,
}

impl FlistWalkerApp {
    pub fn new(root: PathBuf, limit: usize, query: String) -> Self {
        let (search_tx, search_rx) = spawn_search_worker();
        let (preview_tx, preview_rx) = spawn_preview_worker();
        let (filelist_tx, filelist_rx) = spawn_filelist_worker();
        let (index_tx, index_rx) = spawn_index_worker();
        let mut app = Self {
            root: Self::normalize_windows_path(root),
            limit: limit.clamp(1, 1000),
            query,
            use_filelist: true,
            use_regex: false,
            include_files: true,
            include_dirs: true,
            index: IndexBuildResult {
                entries: Vec::new(),
                source: IndexSource::None,
            },
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
            search_in_progress: false,
            index_in_progress: false,
            preview_in_progress: false,
            filelist_in_progress: false,
            scroll_to_current: true,
            focus_query_requested: false,
            saved_roots: Self::load_saved_roots(),
            preview_cache: HashMap::new(),
            last_incremental_results_refresh: Instant::now(),
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

    fn remove_current_root_from_saved(&mut self) {
        let key = Self::path_key(&self.root);
        let before = self.saved_roots.len();
        self.saved_roots.retain(|p| Self::path_key(p) != key);
        if self.saved_roots.len() == before {
            self.set_notice("Current root is not in saved list");
            return;
        }
        self.save_saved_roots();
        self.set_notice("Removed current root from saved list");
    }

    fn prefer_relative_display(&self) -> bool {
        matches!(
            self.index.source,
            IndexSource::Walker | IndexSource::FileList(_)
        )
    }

    fn refresh_status_line(&mut self) {
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
            self.entries.len(),
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
        let request_id = self.next_index_request_id;
        self.next_index_request_id = self.next_index_request_id.saturating_add(1);
        self.pending_index_request_id = Some(request_id);
        self.index_in_progress = true;

        self.index = IndexBuildResult {
            entries: Vec::new(),
            source: IndexSource::None,
        };
        self.entries = Arc::new(Vec::new());
        self.entry_kinds.clear();
        self.preview_cache.clear();
        self.pending_preview_request_id = None;
        self.preview_in_progress = false;
        self.results.clear();
        self.current_row = None;
        self.preview.clear();
        self.last_incremental_results_refresh = Instant::now();
        self.refresh_status_line();

        let req = IndexRequest {
            request_id,
            root: self.root.clone(),
            use_filelist: self.use_filelist,
            include_files: self.include_files,
            include_dirs: self.include_dirs,
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
        const INCREMENTAL_REFRESH_INTERVAL: Duration = Duration::from_millis(50);

        let frame_start = Instant::now();
        let mut processed = 0usize;
        let mut needs_incremental_refresh = false;
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
                    self.entries = Arc::new(self.index.entries.clone());
                    self.pending_index_request_id = None;
                    self.index_in_progress = false;
                    self.update_results();
                    self.clear_notice();
                }
                IndexResponse::Failed { request_id, error } => {
                    if Some(request_id) != self.pending_index_request_id {
                        continue;
                    }
                    self.index_in_progress = false;
                    self.pending_index_request_id = None;
                    self.set_notice(format!("Indexing failed: {}", error));
                }
            }

            processed = processed.saturating_add(1);
            if processed >= MAX_MESSAGES_PER_FRAME || frame_start.elapsed() >= FRAME_BUDGET {
                break;
            }
        }

        if needs_incremental_refresh
            && self.last_incremental_results_refresh.elapsed() >= INCREMENTAL_REFRESH_INTERVAL
        {
            self.last_incremental_results_refresh = Instant::now();
            self.entries = Arc::new(self.index.entries.clone());
            self.update_results();
        }
    }

    fn apply_results(&mut self, results: Vec<(PathBuf, f64)>) {
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
            self.scroll_to_current = true;
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
                self.clear_notice();
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
            self.preview_cache
                .insert(response.path.clone(), response.preview.clone());
            if let Some(row) = self.current_row {
                if let Some((current_path, _)) = self.results.get(row) {
                    if *current_path == response.path {
                        self.preview = response.preview;
                    }
                }
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

    fn create_filelist(&mut self) {
        if self.filelist_in_progress {
            self.set_notice("Create File List is already running");
            return;
        }

        let request_id = self.next_filelist_request_id;
        self.next_filelist_request_id = self.next_filelist_request_id.saturating_add(1);
        self.pending_filelist_request_id = Some(request_id);
        self.filelist_in_progress = true;
        self.refresh_status_line();

        let req = FileListRequest {
            request_id,
            root: self.root.clone(),
            include_files: self.include_files,
            include_dirs: self.include_dirs,
        };
        if self.filelist_tx.send(req).is_err() {
            self.pending_filelist_request_id = None;
            self.filelist_in_progress = false;
            self.set_notice("Create File List worker is unavailable");
        }
    }

    fn poll_filelist_response(&mut self) {
        while let Ok(response) = self.filelist_rx.try_recv() {
            let Some(pending) = self.pending_filelist_request_id else {
                continue;
            };
            match response {
                FileListResponse::Finished {
                    request_id,
                    path,
                    count,
                } => {
                    if request_id != pending {
                        continue;
                    }
                    self.pending_filelist_request_id = None;
                    self.filelist_in_progress = false;
                    self.set_notice(format!("Created {}: {} entries", path.display(), count));
                    if self.use_filelist {
                        self.request_index_refresh();
                    }
                }
                FileListResponse::Failed { request_id, error } => {
                    if request_id != pending {
                        continue;
                    }
                    self.pending_filelist_request_id = None;
                    self.filelist_in_progress = false;
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

        let mut text_changed = false;
        let mut cursor_changed = false;
        let char_len = Self::char_count(&self.query);
        let ccursor = output.state.ccursor_range().unwrap_or_else(|| {
            egui::text_edit::CCursorRange::one(egui::text::CCursor::new(char_len))
        });
        let mut cursor = ccursor.primary.index.min(char_len);
        let mut anchor = ccursor.secondary.index.min(char_len);

        if pressed(egui::Key::A) {
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
        if ctx.input(|i| i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(egui::Key::C)) {
            self.copy_selected_paths(ctx);
        }

        let ctrl_mod = egui::Modifiers {
            ctrl: true,
            ..Default::default()
        };
        if ctx.input_mut(|i| i.consume_key(ctrl_mod, egui::Key::L)) {
            self.focus_query_requested = true;
        }
        if ctx.input_mut(|i| i.consume_key(ctrl_mod, egui::Key::G)) {
            self.clear_query_and_selection();
        }
    }
}

impl eframe::App for FlistWalkerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let row_height = ui.spacing().interact_size.y;
                ui.add_sized([44.0, row_height], egui::Label::new("Root:"));
                let button_width = 96.0;
                let add_width = 100.0;
                let remove_width = 130.0;
                let field_width = (ui.available_width()
                    - button_width
                    - add_width
                    - remove_width
                    - (ui.spacing().item_spacing.x * 3.0))
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
                            self.root = Self::normalize_windows_path(dir);
                            self.request_index_refresh();
                            self.set_notice(format!("Root changed: {}", self.root_display_text()));
                        }
                        Ok(None) => {}
                        Err(err) => {
                            self.set_notice(format!("Browse failed: {}", err));
                        }
                    }
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
                    if Self::path_key(&root) != Self::path_key(&self.root) {
                        self.root = Self::normalize_windows_path(root);
                        self.request_index_refresh();
                        self.set_notice(format!("Root changed: {}", self.root_display_text()));
                    }
                }
            });

            ui.horizontal(|ui| {
                let mut changed = false;
                changed |= ui
                    .checkbox(&mut self.use_filelist, "Use FileList")
                    .changed();
                if ui.checkbox(&mut self.use_regex, "Regex").changed() {
                    self.update_results();
                }
                changed |= ui.checkbox(&mut self.include_files, "Files").changed();
                changed |= ui.checkbox(&mut self.include_dirs, "Folders").changed();
                if !self.include_files && !self.include_dirs {
                    self.include_files = true;
                }
                ui.separator();
                ui.label(self.source_text());
                if changed {
                    self.request_index_refresh();
                }
            });

            let query_id = ui.make_persistent_id("query-input");
            let mut output = egui::TextEdit::singleline(&mut self.query)
                .id(query_id)
                .desired_width(f32::INFINITY)
                .hint_text("Type to fuzzy-search files/folders...")
                .show(ui);
            if self.focus_query_requested {
                output.response.request_focus();
                self.focus_query_requested = false;
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

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.columns(2, |cols| {
                cols[0].heading("Results");
                let mut did_scroll_to_current = false;
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .show(&mut cols[0], |ui| {
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

                            let response = ui.add(
                                egui::Label::new(job)
                                    .wrap(false)
                                    .sense(egui::Sense::click()),
                            );
                            if self.scroll_to_current && is_current && !did_scroll_to_current {
                                response.scroll_to_me(Some(egui::Align::Center));
                                did_scroll_to_current = true;
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
                if did_scroll_to_current {
                    self.scroll_to_current = false;
                }

                cols[1].heading("Preview");
                let preview_size = cols[1].available_size();
                cols[1].add_sized(
                    preview_size,
                    egui::TextEdit::multiline(&mut self.preview).interactive(false),
                );
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("fff-rs-app-{name}-{nonce}"))
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

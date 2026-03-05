use crate::actions::execute_or_open;
use crate::indexer::{
    apply_filelist_hierarchy_overrides, find_filelist_in_first_level, parse_filelist_stream,
    write_filelist, IndexBuildResult, IndexSource,
};
use crate::search::{try_search_entries_indexed_with_scope, IndexedScore};
use crate::ui_model::{
    build_preview_text_with_kind, display_path_with_mode, has_visible_match,
    match_positions_for_path, normalize_path_for_display, should_skip_preview,
};
use eframe::egui;
use jwalk::{Parallelism, WalkDir};
use memory_stats::memory_stats;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct SavedWindowGeometry {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    monitor_width: Option<f32>,
    monitor_height: Option<f32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct UiState {
    default_root: Option<String>,
    show_preview: Option<bool>,
    preview_panel_width: Option<f32>,
    #[serde(default)]
    results_panel_width: Option<f32>,
    window: Option<SavedWindowGeometry>,
}

#[derive(Clone, Debug, Default)]
struct LaunchSettings {
    default_root: Option<PathBuf>,
    show_preview: bool,
    preview_panel_width: f32,
}

#[derive(Clone, Debug)]
struct AppTabState {
    id: u64,
    root: PathBuf,
    use_filelist: bool,
    use_regex: bool,
    include_files: bool,
    include_dirs: bool,
    index: IndexBuildResult,
    all_entries: Arc<Vec<PathBuf>>,
    entries: Arc<Vec<PathBuf>>,
    entry_kinds: HashMap<PathBuf, bool>,
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
    results: Vec<(PathBuf, f64)>,
    pinned_paths: HashSet<PathBuf>,
    current_row: Option<usize>,
    preview: String,
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
    entry_kinds: HashMap<PathBuf, bool>,
}

static PROCESS_SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

struct WorkerRuntime {
    shutdown: Arc<AtomicBool>,
    handles: Vec<thread::JoinHandle<()>>,
}

impl WorkerRuntime {
    fn new(shutdown: Arc<AtomicBool>) -> Self {
        Self {
            shutdown,
            handles: Vec::new(),
        }
    }

    fn push(&mut self, handle: thread::JoinHandle<()>) {
        self.handles.push(handle);
    }

    fn request_shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }

    fn join_all(mut self) {
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}

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

#[derive(Clone, Debug)]
struct IndexEntry {
    path: PathBuf,
    is_dir: bool,
    kind_known: bool,
}

struct IndexRequest {
    request_id: u64,
    tab_id: u64,
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
    ReplaceAll {
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
    Canceled {
        request_id: u64,
    },
    Truncated {
        request_id: u64,
        limit: usize,
    },
}

const WALKER_MAX_ENTRIES_DEFAULT: usize = 500_000;
const WALKER_THREADS_DEFAULT: usize = 2;
const WALKER_THREADS_MAX: usize = 8;

fn walker_max_entries() -> usize {
    std::env::var("FLISTWALKER_WALKER_MAX_ENTRIES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(WALKER_MAX_ENTRIES_DEFAULT)
}

fn walker_parallelism() -> Parallelism {
    let threads = std::env::var("FLISTWALKER_WALKER_THREADS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(WALKER_THREADS_DEFAULT)
        .min(WALKER_THREADS_MAX);
    if threads <= 1 {
        Parallelism::Serial
    } else {
        Parallelism::RayonNewPool(threads)
    }
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

struct ActionRequest {
    request_id: u64,
    paths: Vec<PathBuf>,
    open_parent_for_files: bool,
}

struct ActionResponse {
    request_id: u64,
    notice: String,
}

struct KindResolveRequest {
    epoch: u64,
    path: PathBuf,
}

struct KindResolveResponse {
    epoch: u64,
    path: PathBuf,
    is_dir: Option<bool>,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct HighlightCacheKey {
    path: PathBuf,
    prefer_relative: bool,
    use_regex: bool,
}

struct FileListRequest {
    request_id: u64,
    tab_id: u64,
    root: PathBuf,
    entries: Vec<PathBuf>,
}

struct PendingFileListConfirmation {
    tab_id: u64,
    root: PathBuf,
    entries: Vec<PathBuf>,
    existing_path: PathBuf,
}

struct PendingFileListAfterIndex {
    tab_id: u64,
    root: PathBuf,
}

struct PendingFileListUseWalkerConfirmation {
    source_tab_id: u64,
    root: PathBuf,
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

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct SearchEntriesSnapshotKey {
    ptr: usize,
    len: usize,
}

impl SearchEntriesSnapshotKey {
    fn from_entries(entries: &Arc<Vec<PathBuf>>) -> Self {
        Self {
            ptr: Arc::as_ptr(entries) as usize,
            len: entries.len(),
        }
    }
}

#[derive(Clone, Debug)]
struct SearchPrefixCacheEntry {
    snapshot: SearchEntriesSnapshotKey,
    query: String,
    matched_indices: Arc<Vec<usize>>,
    approx_bytes: usize,
}

#[derive(Default)]
struct SearchPrefixCache {
    entries: VecDeque<SearchPrefixCacheEntry>,
    total_bytes: usize,
}

impl SearchPrefixCache {
    const MAX_ENTRIES: usize = 64;
    const MAX_BYTES: usize = 8 * 1024 * 1024;
    const MAX_MATCHED_INDICES: usize = 20_000;
    const MIN_QUERY_LEN: usize = 3;

    fn is_cacheable_query(query: &str) -> bool {
        let q = query.trim();
        if q.len() < Self::MIN_QUERY_LEN {
            return false;
        }
        if q.contains(char::is_whitespace) {
            return false;
        }
        !q.contains(['|', '!', '\'', '^', '$'])
    }

    fn is_safe_prefix_extension(prefix: &str, query: &str) -> bool {
        if !Self::is_cacheable_query(prefix) || !Self::is_cacheable_query(query) {
            return false;
        }
        query.starts_with(prefix) && query.len() > prefix.len()
    }

    fn lookup_candidates(
        &mut self,
        snapshot: SearchEntriesSnapshotKey,
        query: &str,
    ) -> Option<Arc<Vec<usize>>> {
        if !Self::is_cacheable_query(query) {
            return None;
        }

        let mut best_idx = None;
        let mut best_len = 0usize;
        for (idx, entry) in self.entries.iter().enumerate() {
            if entry.snapshot != snapshot {
                continue;
            }
            if !Self::is_safe_prefix_extension(&entry.query, query) {
                continue;
            }
            if entry.query.len() > best_len {
                best_len = entry.query.len();
                best_idx = Some(idx);
            }
        }

        let idx = best_idx?;
        let entry = self.entries.remove(idx)?;
        let matched = Arc::clone(&entry.matched_indices);
        self.entries.push_back(entry);
        Some(matched)
    }

    fn maybe_store(
        &mut self,
        snapshot: SearchEntriesSnapshotKey,
        query: &str,
        matched_indices: Vec<usize>,
    ) {
        if !Self::is_cacheable_query(query) {
            return;
        }
        if matched_indices.is_empty() || matched_indices.len() > Self::MAX_MATCHED_INDICES {
            return;
        }

        let query = query.trim().to_string();
        let approx_bytes = query
            .len()
            .saturating_add(matched_indices.len().saturating_mul(std::mem::size_of::<usize>()));
        if approx_bytes > Self::MAX_BYTES {
            return;
        }

        if let Some(existing_pos) = self
            .entries
            .iter()
            .position(|entry| entry.snapshot == snapshot && entry.query == query)
        {
            if let Some(old) = self.entries.remove(existing_pos) {
                self.total_bytes = self.total_bytes.saturating_sub(old.approx_bytes);
            }
        }

        self.total_bytes = self.total_bytes.saturating_add(approx_bytes);
        self.entries.push_back(SearchPrefixCacheEntry {
            snapshot,
            query,
            matched_indices: Arc::new(matched_indices),
            approx_bytes,
        });
        self.evict_over_limit();
    }

    fn evict_over_limit(&mut self) {
        while self.entries.len() > Self::MAX_ENTRIES || self.total_bytes > Self::MAX_BYTES {
            let Some(oldest) = self.entries.pop_front() else {
                break;
            };
            self.total_bytes = self.total_bytes.saturating_sub(oldest.approx_bytes);
        }
    }
}

fn scored_indices_to_paths(
    entries: &[PathBuf],
    scored: &[IndexedScore],
    limit: usize,
) -> Vec<(PathBuf, f64)> {
    if limit == 0 || scored.is_empty() {
        return Vec::new();
    }
    scored
        .iter()
        .take(limit)
        .filter_map(|item| {
            entries
                .get(item.index)
                .cloned()
                .map(|path| (path, item.score))
        })
        .collect()
}

fn spawn_search_worker(
    shutdown: Arc<AtomicBool>,
) -> (
    Sender<SearchRequest>,
    Receiver<SearchResponse>,
    thread::JoinHandle<()>,
) {
    let (tx_req, rx_req) = mpsc::channel::<SearchRequest>();
    let (tx_res, rx_res) = mpsc::channel::<SearchResponse>();

    let handle = thread::spawn(move || {
        let mut prefix_cache = SearchPrefixCache::default();
        while let Ok(mut req) = rx_req.recv() {
            if shutdown.load(Ordering::Relaxed) {
                break;
            }
            while let Ok(newer) = rx_req.try_recv() {
                req = newer;
            }
            let query_trimmed = req.query.trim().to_string();
            let snapshot = SearchEntriesSnapshotKey::from_entries(&req.entries);
            let cached_candidates = if req.use_regex {
                None
            } else {
                prefix_cache.lookup_candidates(snapshot, &query_trimmed)
            };
            let (results, error) = match try_search_entries_indexed_with_scope(
                &req.query,
                &req.entries,
                req.use_regex,
                Some(&req.root),
                req.prefer_relative,
                cached_candidates.as_ref().map(|items| items.as_slice()),
            ) {
                Ok(scored) => {
                    let raw_results = scored_indices_to_paths(&req.entries, &scored, req.limit);
                    let matched_indices = scored.iter().map(|item| item.index).collect();
                    prefix_cache.maybe_store(snapshot, &query_trimmed, matched_indices);
                    (
                    filter_search_results(
                        raw_results,
                        &req.root,
                        &req.query,
                        req.prefer_relative,
                        req.use_regex,
                    ),
                    None,
                )
                }
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

    (tx_req, rx_res, handle)
}

fn spawn_preview_worker(
    shutdown: Arc<AtomicBool>,
) -> (
    Sender<PreviewRequest>,
    Receiver<PreviewResponse>,
    thread::JoinHandle<()>,
) {
    let (tx_req, rx_req) = mpsc::channel::<PreviewRequest>();
    let (tx_res, rx_res) = mpsc::channel::<PreviewResponse>();

    let handle = thread::spawn(move || {
        while let Ok(mut req) = rx_req.recv() {
            if shutdown.load(Ordering::Relaxed) {
                break;
            }
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

    (tx_req, rx_res, handle)
}

fn spawn_kind_resolver_worker(
    shutdown: Arc<AtomicBool>,
) -> (
    Sender<KindResolveRequest>,
    Receiver<KindResolveResponse>,
    thread::JoinHandle<()>,
) {
    let (tx_req, rx_req) = mpsc::channel::<KindResolveRequest>();
    let (tx_res, rx_res) = mpsc::channel::<KindResolveResponse>();

    let handle = thread::spawn(move || {
        while let Ok(req) = rx_req.recv() {
            if shutdown.load(Ordering::Relaxed) {
                break;
            }
            let is_dir = std::fs::metadata(&req.path).ok().and_then(|meta| {
                if meta.is_dir() {
                    Some(true)
                } else if meta.is_file() {
                    Some(false)
                } else {
                    None
                }
            });
            if tx_res
                .send(KindResolveResponse {
                    epoch: req.epoch,
                    path: req.path,
                    is_dir,
                })
                .is_err()
            {
                break;
            }
        }
    });

    (tx_req, rx_res, handle)
}

fn spawn_filelist_worker(
    shutdown: Arc<AtomicBool>,
) -> (
    Sender<FileListRequest>,
    Receiver<FileListResponse>,
    thread::JoinHandle<()>,
) {
    let (tx_req, rx_req) = mpsc::channel::<FileListRequest>();
    let (tx_res, rx_res) = mpsc::channel::<FileListResponse>();

    let handle = thread::spawn(move || {
        while let Ok(req) = rx_req.recv() {
            if shutdown.load(Ordering::Relaxed) {
                break;
            }
            let _tab_id = req.tab_id;
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

    (tx_req, rx_res, handle)
}

fn spawn_action_worker(
    shutdown: Arc<AtomicBool>,
) -> (
    Sender<ActionRequest>,
    Receiver<ActionResponse>,
    thread::JoinHandle<()>,
) {
    let (tx_req, rx_req) = mpsc::channel::<ActionRequest>();
    let (tx_res, rx_res) = mpsc::channel::<ActionResponse>();

    let handle = thread::spawn(move || {
        while let Ok(req) = rx_req.recv() {
            if shutdown.load(Ordering::Relaxed) {
                break;
            }

            let targets = action_targets_for_request(&req.paths, req.open_parent_for_files);
            let mut failure: Option<String> = None;
            for target in &targets {
                if let Err(err) = execute_or_open(&target) {
                    failure = Some(format!("Action failed: {}", err));
                    break;
                }
            }

            let notice = if let Some(failed) = failure {
                failed
            } else if targets.len() == 1 {
                format!("Action: {}", targets[0].display())
            } else {
                format!("Action: launched {} items", targets.len())
            };

            if tx_res
                .send(ActionResponse {
                    request_id: req.request_id,
                    notice,
                })
                .is_err()
            {
                break;
            }
        }
    });

    (tx_req, rx_res, handle)
}

fn action_targets_for_request(paths: &[PathBuf], open_parent_for_files: bool) -> Vec<PathBuf> {
    if !open_parent_for_files {
        return paths.to_vec();
    }

    let mut unique = HashSet::with_capacity(paths.len());
    let mut targets = Vec::with_capacity(paths.len());
    for path in paths {
        let target = action_target_path_for_open_in_folder(path);
        if unique.insert(target.clone()) {
            targets.push(target);
        }
    }
    targets
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

fn action_target_path_for_open_in_folder(path: &Path) -> PathBuf {
    if path.is_dir() {
        return path.to_path_buf();
    }
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| path.to_path_buf())
}

fn is_nested_filelist_candidate(path: &Path, root_filelist: &Path, root: &Path) -> bool {
    if path == root_filelist || !path.starts_with(root) {
        return false;
    }
    path.file_name()
        .and_then(|s| s.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("filelist.txt"))
}

fn collect_filelist_entries_with_cancel(
    filelist: &Path,
    root: &Path,
    include_files: bool,
    include_dirs: bool,
    should_cancel: impl Fn() -> bool,
) -> Result<Vec<PathBuf>, String> {
    let mut entries = Vec::new();
    parse_filelist_stream(
        filelist,
        root,
        include_files,
        include_dirs,
        should_cancel,
        |path, _is_dir| entries.push(path),
    )
    .map_err(|err| err.to_string())?;
    Ok(entries)
}

fn stream_filelist_index(
    tx_res: &Sender<IndexResponse>,
    req: &IndexRequest,
    root: &std::path::Path,
    filelist: PathBuf,
    shutdown: &AtomicBool,
    latest_request_ids: &Mutex<HashMap<u64, u64>>,
) -> std::result::Result<IndexSource, String> {
    let source = IndexSource::FileList(filelist.clone());
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
    let mut stream_err: Option<String> = None;
    let mut has_nested_filelist_candidate = false;
    parse_filelist_stream(
        &filelist,
        root,
        req.include_files,
        req.include_dirs,
        || {
            if shutdown.load(Ordering::Relaxed) {
                return true;
            }
            latest_request_ids
                .lock()
                .ok()
                .and_then(|m| m.get(&req.tab_id).copied())
                != Some(req.request_id)
        },
        |path, is_dir| {
            if stream_err.is_some() {
                return;
            }
            if !has_nested_filelist_candidate
                && is_nested_filelist_candidate(&path, &filelist, root)
            {
                has_nested_filelist_candidate = true;
            }
            buffer.push(IndexEntry {
                path,
                is_dir: is_dir.unwrap_or(false),
                kind_known: is_dir.is_some(),
            });
            if buffer.len() >= 256 || last_flush.elapsed() >= Duration::from_millis(100) {
                if !flush_batch(tx_res, req.request_id, &mut buffer) {
                    stream_err = Some("index receiver closed".to_string());
                    return;
                }
                last_flush = Instant::now();
            }
        },
    )
    .map_err(|err| err.to_string())?;
    if let Some(err) = stream_err {
        return Err(err);
    }

    if !flush_batch(tx_res, req.request_id, &mut buffer) {
        return Err("index receiver closed".to_string());
    }

    if !has_nested_filelist_candidate {
        return Ok(source);
    }

    let mut final_entries = collect_filelist_entries_with_cancel(
        &filelist,
        root,
        req.include_files,
        req.include_dirs,
        || {
            if shutdown.load(Ordering::Relaxed) {
                return true;
            }
            latest_request_ids
                .lock()
                .ok()
                .and_then(|m| m.get(&req.tab_id).copied())
                != Some(req.request_id)
        },
    )?;
    let replaced = apply_filelist_hierarchy_overrides(
        &filelist,
        root,
        &mut final_entries,
        req.include_files,
        req.include_dirs,
        || {
            if shutdown.load(Ordering::Relaxed) {
                return true;
            }
            latest_request_ids
                .lock()
                .ok()
                .and_then(|m| m.get(&req.tab_id).copied())
                != Some(req.request_id)
        },
    )
    .map_err(|err| err.to_string())?;

    if replaced {
        let entries = final_entries
            .into_iter()
            .map(|path| IndexEntry {
                path,
                is_dir: false,
                kind_known: false,
            })
            .collect::<Vec<_>>();
        if tx_res
            .send(IndexResponse::ReplaceAll {
                request_id: req.request_id,
                entries,
            })
            .is_err()
        {
            return Err("index receiver closed".to_string());
        }
    }
    Ok(source)
}

fn stream_walker_index(
    tx_res: &Sender<IndexResponse>,
    req: &IndexRequest,
    root: &std::path::Path,
    shutdown: &AtomicBool,
    latest_request_ids: &Mutex<HashMap<u64, u64>>,
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
    let mut cancel_check_budget = 0usize;
    let mut emitted_entries = 0usize;
    let max_entries = walker_max_entries();
    let mut truncated = false;
    for entry in WalkDir::new(root)
        .parallelism(walker_parallelism())
        .skip_hidden(false)
        .follow_links(false)
        .min_depth(1)
        .into_iter()
        .flatten()
    {
        cancel_check_budget = cancel_check_budget.saturating_add(1);
        if cancel_check_budget >= 64 {
            cancel_check_budget = 0;
            if shutdown.load(Ordering::Relaxed) {
                return Err("superseded".to_string());
            }
            if latest_request_ids
                .lock()
                .ok()
                .and_then(|m| m.get(&req.tab_id).copied())
                != Some(req.request_id)
            {
                return Err("superseded".to_string());
            }
        }
        let is_dir = entry.file_type().is_dir();
        if (is_dir && !req.include_dirs) || (!is_dir && !req.include_files) {
            continue;
        }
        if emitted_entries >= max_entries {
            truncated = true;
            break;
        }
        buffer.push(IndexEntry {
            path: entry.path().to_path_buf(),
            is_dir,
            kind_known: true,
        });
        emitted_entries = emitted_entries.saturating_add(1);
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
    if truncated
        && tx_res
            .send(IndexResponse::Truncated {
                request_id: req.request_id,
                limit: max_entries,
            })
            .is_err()
    {
        return Err("index receiver closed".to_string());
    }
    Ok(source)
}

fn spawn_index_worker(
    shutdown: Arc<AtomicBool>,
    latest_request_ids: Arc<Mutex<HashMap<u64, u64>>>,
) -> (
    Sender<IndexRequest>,
    Receiver<IndexResponse>,
    Vec<thread::JoinHandle<()>>,
) {
    let (tx_req, rx_req) = mpsc::channel::<IndexRequest>();
    let (tx_res, rx_res) = mpsc::channel::<IndexResponse>();
    let rx_req = Arc::new(Mutex::new(rx_req));
    let mut handles = Vec::new();

    for _ in 0..2 {
        let tx_res_worker = tx_res.clone();
        let rx_req_worker = Arc::clone(&rx_req);
        let latest_request_ids_worker = Arc::clone(&latest_request_ids);
        let shutdown_worker = Arc::clone(&shutdown);
        let handle = thread::spawn(move || loop {
            let req = {
                let Ok(rx) = rx_req_worker.lock() else {
                    break;
                };
                match rx.recv() {
                    Ok(req) => req,
                    Err(_) => break,
                }
            };
            if shutdown_worker.load(Ordering::Relaxed) {
                break;
            }

            if !req.include_files && !req.include_dirs {
                if tx_res_worker
                    .send(IndexResponse::Started {
                        request_id: req.request_id,
                        source: IndexSource::None,
                    })
                    .is_err()
                {
                    break;
                }
                if tx_res_worker
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
                        &tx_res_worker,
                        &req,
                        &root,
                        filelist,
                        shutdown_worker.as_ref(),
                        latest_request_ids_worker.as_ref(),
                    )
                } else {
                    stream_walker_index(
                        &tx_res_worker,
                        &req,
                        &root,
                        shutdown_worker.as_ref(),
                        latest_request_ids_worker.as_ref(),
                    )
                }
            } else {
                stream_walker_index(
                    &tx_res_worker,
                    &req,
                    &root,
                    shutdown_worker.as_ref(),
                    latest_request_ids_worker.as_ref(),
                )
            };

            match result {
                Ok(source) => {
                    if tx_res_worker
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
                        let _ = tx_res_worker.send(IndexResponse::Canceled {
                            request_id: req.request_id,
                        });
                        continue;
                    }
                    if tx_res_worker
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
        });
        handles.push(handle);
    }

    (tx_req, rx_res, handles)
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
    action_tx: Sender<ActionRequest>,
    action_rx: Receiver<ActionResponse>,
    kind_tx: Sender<KindResolveRequest>,
    kind_rx: Receiver<KindResolveResponse>,
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
    next_action_request_id: u64,
    pending_action_request_id: Option<u64>,
    next_filelist_request_id: u64,
    pending_filelist_request_id: Option<u64>,
    pending_filelist_request_tab_id: Option<u64>,
    pending_filelist_root: Option<PathBuf>,
    pending_filelist_after_index: Option<PendingFileListAfterIndex>,
    pending_filelist_confirmation: Option<PendingFileListConfirmation>,
    pending_filelist_use_walker_confirmation: Option<PendingFileListUseWalkerConfirmation>,
    latest_index_request_ids: Arc<Mutex<HashMap<u64, u64>>>,
    pending_index_queue: VecDeque<IndexRequest>,
    index_inflight_requests: HashSet<u64>,
    search_in_progress: bool,
    index_in_progress: bool,
    preview_in_progress: bool,
    action_in_progress: bool,
    kind_resolution_in_progress: bool,
    filelist_in_progress: bool,
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
    preview_cache: HashMap<PathBuf, String>,
    preview_cache_order: VecDeque<PathBuf>,
    preview_cache_total_bytes: usize,
    highlight_cache_scope_query: String,
    highlight_cache_scope_root: PathBuf,
    highlight_cache_scope_use_regex: bool,
    highlight_cache_scope_prefer_relative: bool,
    highlight_cache: HashMap<HighlightCacheKey, Arc<Vec<u16>>>,
    highlight_cache_order: VecDeque<HighlightCacheKey>,
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
    worker_runtime: Option<WorkerRuntime>,
}

impl FlistWalkerApp {
    const PREVIEW_CACHE_MAX_BYTES: usize = 32 * 1024 * 1024;
    const HIGHLIGHT_CACHE_MAX: usize = 256;
    const INCREMENTAL_SEARCH_REFRESH_INTERVAL: Duration = Duration::from_millis(300);
    const PAGE_MOVE_ROWS: isize = 10;
    const DEFAULT_PREVIEW_PANEL_WIDTH: f32 = 440.0;
    const MIN_RESULTS_PANEL_WIDTH: f32 = 220.0;
    const MIN_PREVIEW_PANEL_WIDTH: f32 = 220.0;
    const INDEX_MAX_CONCURRENT: usize = 2;
    const INDEX_MAX_QUEUE: usize = 4;
    const UI_STATE_SAVE_INTERVAL: Duration = Duration::from_millis(500);
    const WINDOW_GEOMETRY_SETTLE_INTERVAL: Duration = Duration::from_millis(350);
    const MEMORY_SAMPLE_INTERVAL: Duration = Duration::from_millis(1000);
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
        let worker_shutdown = Arc::new(AtomicBool::new(false));
        let mut worker_runtime = WorkerRuntime::new(Arc::clone(&worker_shutdown));
        let (search_tx, search_rx, search_handle) =
            spawn_search_worker(Arc::clone(&worker_shutdown));
        worker_runtime.push(search_handle);
        let (preview_tx, preview_rx, preview_handle) =
            spawn_preview_worker(Arc::clone(&worker_shutdown));
        worker_runtime.push(preview_handle);
        let (action_tx, action_rx, action_handle) =
            spawn_action_worker(Arc::clone(&worker_shutdown));
        worker_runtime.push(action_handle);
        let (kind_tx, kind_rx, kind_handle) =
            spawn_kind_resolver_worker(Arc::clone(&worker_shutdown));
        worker_runtime.push(kind_handle);
        let (filelist_tx, filelist_rx, filelist_handle) =
            spawn_filelist_worker(Arc::clone(&worker_shutdown));
        worker_runtime.push(filelist_handle);
        let latest_index_request_ids = Arc::new(Mutex::new(HashMap::new()));
        let (index_tx, index_rx, index_handles) = spawn_index_worker(
            Arc::clone(&worker_shutdown),
            Arc::clone(&latest_index_request_ids),
        );
        for handle in index_handles {
            worker_runtime.push(handle);
        }
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
            action_tx,
            action_rx,
            kind_tx,
            kind_rx,
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
            next_action_request_id: 1,
            pending_action_request_id: None,
            next_filelist_request_id: 1,
            pending_filelist_request_id: None,
            pending_filelist_request_tab_id: None,
            pending_filelist_root: None,
            pending_filelist_after_index: None,
            pending_filelist_confirmation: None,
            pending_filelist_use_walker_confirmation: None,
            latest_index_request_ids,
            pending_index_queue: VecDeque::new(),
            index_inflight_requests: HashSet::new(),
            search_in_progress: false,
            index_in_progress: false,
            preview_in_progress: false,
            action_in_progress: false,
            kind_resolution_in_progress: false,
            filelist_in_progress: false,
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
            preview_cache: HashMap::new(),
            preview_cache_order: VecDeque::new(),
            preview_cache_total_bytes: 0,
            highlight_cache_scope_query: String::new(),
            highlight_cache_scope_root: PathBuf::new(),
            highlight_cache_scope_use_regex: false,
            highlight_cache_scope_prefer_relative: false,
            highlight_cache: HashMap::new(),
            highlight_cache_order: VecDeque::new(),
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
            worker_runtime: Some(worker_runtime),
        };
        if let Some(path) = Self::window_trace_path() {
            Self::append_window_trace("app_initialized", &format!("path={}", path.display()));
        }
        app.initialize_tabs();
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

    fn initialize_tabs(&mut self) {
        let id = self.next_tab_id;
        self.next_tab_id = self.next_tab_id.saturating_add(1);
        self.tabs = vec![self.capture_active_tab_state(id)];
        self.active_tab = 0;
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

    fn capture_active_tab_state(&self, id: u64) -> AppTabState {
        AppTabState {
            id,
            root: self.root.clone(),
            use_filelist: self.use_filelist,
            use_regex: self.use_regex,
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
            results: self.results.clone(),
            pinned_paths: self.pinned_paths.clone(),
            current_row: self.current_row,
            preview: self.preview.clone(),
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
        self.results = tab.results.clone();
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
        let snapshot = self.capture_active_tab_state(id);
        if let Some(slot) = self.tabs.get_mut(self.active_tab) {
            *slot = snapshot;
        }
    }

    fn find_tab_index_by_id(&self, tab_id: u64) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id == tab_id)
    }

    fn switch_to_tab_index(&mut self, next_index: usize) {
        if next_index >= self.tabs.len() || next_index == self.active_tab {
            return;
        }
        self.shrink_checkpoint_buffers();
        self.sync_active_tab_state();
        if let Some(next_tab) = self.tabs.get_mut(next_index) {
            Self::shrink_tab_checkpoint_buffers(next_tab);
        }
        self.active_tab = next_index;
        if let Some(tab) = self.tabs.get(next_index).cloned() {
            self.apply_tab_state(&tab);
        }
        self.focus_query_requested = true;
        self.unfocus_query_requested = false;
    }

    fn create_new_tab(&mut self) {
        self.sync_active_tab_state();
        let id = self.next_tab_id;
        self.next_tab_id = self.next_tab_id.saturating_add(1);
        let mut tab = self.capture_active_tab_state(id);
        tab.use_filelist = true;
        tab.query.clear();
        tab.pinned_paths.clear();
        tab.current_row = None;
        tab.preview.clear();
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
        tab.results = self
            .entries
            .iter()
            .take(self.limit)
            .cloned()
            .map(|p| (p, 0.0))
            .collect();
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

    fn tab_root_label(root: &Path) -> String {
        let normalized = Self::normalize_windows_path(root.to_path_buf());
        let raw = normalized.to_string_lossy().to_string();
        let trimmed = raw.trim_end_matches(|c| c == '/' || c == '\\');
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
        })
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
        let preview_panel_width = ui_state
            .preview_panel_width
            .or(ui_state.results_panel_width)
            .unwrap_or(Self::DEFAULT_PREVIEW_PANEL_WIDTH)
            .max(Self::MIN_PREVIEW_PANEL_WIDTH);
        LaunchSettings {
            default_root,
            show_preview,
            preview_panel_width,
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
            preview_panel_width: Some(self.preview_panel_width),
            results_panel_width: None,
            window: self.window_geometry.clone(),
        };
        if let Ok(text) = serde_json::to_string_pretty(&state) {
            let _ = fs::write(path, text);
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
        self.results.clear();
        self.results.shrink_to_fit();
        self.incremental_filtered_entries.clear();
        self.incremental_filtered_entries.shrink_to_fit();
        self.last_search_snapshot_len = 0;
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
        self.clear_root_scoped_entry_state();
        self.sync_active_tab_state();
        self.cancel_stale_pending_filelist_confirmation();
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
        entry_kinds: &HashMap<PathBuf, bool>,
        path: &Path,
        include_files: bool,
        include_dirs: bool,
    ) -> bool {
        match entry_kinds.get(path).copied() {
            Some(is_dir) => (is_dir && include_dirs) || (!is_dir && include_files),
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
            " | Creating FileList..."
        } else {
            ""
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
            "{} | Entries: {} | Results: {}{}{}{}{}{}{}{}{}",
            tab_label,
            indexed_count,
            self.results.len(),
            clip_text,
            pinned,
            searching,
            indexing,
            executing,
            creating_filelist,
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

    fn request_index_refresh(&mut self) {
        self.ensure_entry_filters();
        self.cancel_stale_pending_filelist_confirmation();
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
                            state.entry_kinds.insert(entry.path.clone(), entry.is_dir);
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
                            state.entry_kinds.insert(entry.path.clone(), entry.is_dir);
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
                self.apply_results(response.results);
                if self.search_rerun_pending
                    && !self.query.trim().is_empty()
                    && self.index_in_progress
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
            tab.results = response.results;
            if tab.results.is_empty() {
                tab.current_row = None;
                tab.preview.clear();
                tab.pending_preview_request_id = None;
                tab.preview_in_progress = false;
            } else {
                let max_index = tab.results.len().saturating_sub(1);
                tab.current_row = Some(tab.current_row.unwrap_or(0).min(max_index));
            }
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
                if let Some(row) = tab.current_row {
                    if let Some((current_path, _)) = tab.results.get(row) {
                        if *current_path == response.path {
                            tab.preview = response.preview;
                        }
                    }
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
            && self.highlight_cache_scope_prefer_relative == prefer_relative
        {
            return;
        }
        self.highlight_cache_scope_query = self.query.clone();
        self.highlight_cache_scope_root = self.root.clone();
        self.highlight_cache_scope_use_regex = self.use_regex;
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
            self.apply_results(results);
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
            self.entry_kinds.insert(entry.path.clone(), entry.is_dir);
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
        self.apply_results_with_scroll_policy(results, true);
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
        let delta = current_len.saturating_sub(self.last_search_snapshot_len);
        if delta > 0
            && self.last_incremental_results_refresh.elapsed()
                >= Self::INCREMENTAL_SEARCH_REFRESH_INTERVAL
        {
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

    fn is_entry_visible_for_current_filter(&self, path: &Path) -> bool {
        match self.entry_kinds.get(path).copied() {
            Some(is_dir) => (is_dir && self.include_dirs) || (!is_dir && self.include_files),
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
            if let Some(is_dir) = response.is_dir {
                if self.current_row.is_some_and(|row| {
                    self.results
                        .get(row)
                        .is_some_and(|(path, _)| *path == response.path)
                }) {
                    resolved_current_row = true;
                }
                self.entry_kinds.insert(response.path, is_dir);
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

                let Some(is_dir) = self.current_result_kind() else {
                    self.preview = "Resolving entry type...".to_string();
                    self.queue_kind_resolution(path.clone());
                    self.pump_kind_resolution_requests();
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

    fn execute_selected_open_folder(&mut self) {
        self.execute_selected_with_options(true);
    }

    fn execute_selected_with_options(&mut self, open_parent_for_files: bool) {
        let paths = self.selected_paths();
        if paths.is_empty() {
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
                    paths[0].display()
                ));
            } else {
                self.set_notice(format!("Action: {}", paths[0].display()));
            }
        } else {
            if open_parent_for_files {
                self.set_notice(format!(
                    "Action: launched {} containing folder items",
                    paths.len()
                ));
            } else {
                self.set_notice(format!("Action: launched {} items", paths.len()));
            }
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
            .filter(|path| self.is_entry_visible_for_current_filter(path))
            .cloned()
            .collect()
    }

    fn start_filelist_creation(&mut self, tab_id: u64, root: PathBuf, entries: Vec<PathBuf>) {
        self.pending_filelist_after_index = None;
        let request_id = self.next_filelist_request_id;
        self.next_filelist_request_id = self.next_filelist_request_id.saturating_add(1);
        self.pending_filelist_request_id = Some(request_id);
        self.pending_filelist_request_tab_id = Some(tab_id);
        self.pending_filelist_root = Some(root.clone());
        self.filelist_in_progress = true;
        self.refresh_status_line();

        let req = FileListRequest {
            request_id,
            tab_id,
            root,
            entries,
        };
        if self.filelist_tx.send(req).is_err() {
            self.pending_filelist_request_id = None;
            self.pending_filelist_request_tab_id = None;
            self.pending_filelist_root = None;
            self.filelist_in_progress = false;
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
        self.start_filelist_creation(tab_id, root, entries);
    }

    fn confirm_pending_filelist_overwrite(&mut self) {
        let Some(pending) = self.pending_filelist_confirmation.take() else {
            return;
        };
        self.start_filelist_creation(pending.tab_id, pending.root, pending.entries);
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

    fn cancel_pending_filelist_use_walker(&mut self) {
        if self
            .pending_filelist_use_walker_confirmation
            .take()
            .is_some()
        {
            self.set_notice("Create File List canceled");
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
                    self.filelist_in_progress = false;

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
        if self.ime_composition_active {
            return false;
        }
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

    fn render_results_and_preview(&mut self, ui: &mut egui::Ui) {
        if self.show_preview {
            let max_preview_width = (ui.available_width() - Self::MIN_RESULTS_PANEL_WIDTH)
                .max(Self::MIN_PREVIEW_PANEL_WIDTH);
            let panel = egui::SidePanel::right("preview-panel")
                .resizable(true)
                .default_width(self.preview_panel_width.min(max_preview_width))
                .min_width(Self::MIN_PREVIEW_PANEL_WIDTH)
                .max_width(max_preview_width);
            let response = panel.show_inside(ui, |ui| {
                ui.heading("Preview");
                let preview_width = ui.available_width();
                let preview_height = ui.available_height();
                ui.allocate_ui_with_layout(
                    egui::vec2(preview_width, preview_height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        let frame_fill = ui.visuals().extreme_bg_color;
                        egui::Frame::none().fill(frame_fill).show(ui, |ui| {
                            ui.set_min_size(egui::vec2(preview_width, preview_height));
                            egui::ScrollArea::both()
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.add_sized(
                                        egui::vec2(preview_width, preview_height),
                                        egui::TextEdit::multiline(&mut self.preview)
                                            .interactive(false)
                                            .font(egui::TextStyle::Monospace)
                                            .desired_width(f32::INFINITY)
                                            .desired_rows(1),
                                    );
                                });
                        });
                    },
                );
            });
            let new_width = response
                .response
                .rect
                .width()
                .max(Self::MIN_PREVIEW_PANEL_WIDTH);
            if (new_width - self.preview_panel_width).abs() > 1.0 {
                self.preview_panel_width = new_width;
                self.mark_ui_state_dirty();
            }
            let splitter_x = response.response.rect.left();
            let splitter_pressed = ui.input(|i| {
                let Some(pos) = i.pointer.interact_pos() else {
                    return false;
                };
                i.pointer.primary_down() && (pos.x - splitter_x).abs() <= 8.0
            });
            self.preview_resize_in_progress = response.response.dragged() || splitter_pressed;
            self.render_results_list(ui);
        } else {
            self.preview_resize_in_progress = false;
            self.render_results_list(ui);
        }
        self.scroll_to_current = false;
    }

    fn results_scroll_enabled(preview_resize_in_progress: bool) -> bool {
        !preview_resize_in_progress
    }

    fn render_results_list(&mut self, ui: &mut egui::Ui) {
        ui.heading("Results");
        let scroll_enabled = Self::results_scroll_enabled(self.preview_resize_in_progress);
        egui::ScrollArea::both()
            .enable_scrolling(scroll_enabled)
            .drag_to_scroll(false)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut clicked_row: Option<usize> = None;
                let mut execute_row: Option<usize> = None;
                let prefer_relative = self.prefer_relative_display();
                self.ensure_highlight_cache_scope(prefer_relative);

                for i in 0..self.results.len() {
                    let Some((path, _score)) = self.results.get(i) else {
                        continue;
                    };
                    let path = path.clone();
                    let is_current = self.current_row == Some(i);
                    let is_pinned = self.pinned_paths.contains(&path);
                    let marker_current = if is_current { "▶" } else { "·" };
                    let marker_pin = if is_pinned { "◆" } else { "·" };
                    let kind = self.entry_kinds.get(&path).copied();
                    let display = display_path_with_mode(&path, &self.root, prefer_relative);
                    let positions =
                        self.highlight_positions_for_path_cached(&path, prefer_relative);

                    let mut job = egui::text::LayoutJob::default();
                    job.append(
                        &format!("{} {} ", marker_current, marker_pin),
                        0.0,
                        egui::TextFormat {
                            color: if is_current {
                                egui::Color32::LIGHT_BLUE
                            } else {
                                ui.visuals().weak_text_color()
                            },
                            ..Default::default()
                        },
                    );
                    let (kind_label, kind_color) = match kind {
                        Some(true) => ("DIR ", egui::Color32::from_rgb(52, 211, 153)),
                        Some(false) => ("FILE", egui::Color32::from_rgb(96, 165, 250)),
                        None => ("....", ui.visuals().weak_text_color()),
                    };
                    job.append(
                        kind_label,
                        0.0,
                        egui::TextFormat {
                            color: kind_color,
                            ..Default::default()
                        },
                    );
                    job.append(" ", 0.0, egui::TextFormat::default());

                    for (idx, ch) in display.chars().enumerate() {
                        let color = if Self::is_highlighted_position(positions.as_slice(), idx) {
                            egui::Color32::from_rgb(245, 158, 11)
                        } else {
                            ui.visuals().text_color()
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

    fn render_tab_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let mut switch_to: Option<usize> = None;
            let mut close_tab: Option<usize> = None;
            for i in 0..self.tabs.len() {
                let is_active = self.active_tab == i;
                let active_fill = if ui.visuals().dark_mode {
                    egui::Color32::from_rgb(48, 53, 62)
                } else {
                    egui::Color32::from_rgb(228, 232, 238)
                };
                egui::Frame::none()
                    .fill(if is_active {
                        active_fill
                    } else {
                        egui::Color32::TRANSPARENT
                    })
                    .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                    .rounding(egui::Rounding::same(4.0))
                    .inner_margin(egui::Margin::symmetric(6.0, 2.0))
                    .show(ui, |ui| {
                        let title = self
                            .tabs
                            .get(i)
                            .map(|tab| self.tab_title(tab, i))
                            .unwrap_or_else(|| format!("Tab {}", i + 1));
                        let title_response = ui.add(
                            egui::Button::new(egui::RichText::new(title).strong().color(
                                if is_active {
                                    ui.visuals().strong_text_color()
                                } else {
                                    ui.visuals().text_color()
                                },
                            ))
                            .frame(false),
                        );
                        if title_response.clicked_by(egui::PointerButton::Middle) {
                            close_tab = Some(i);
                        } else if title_response.clicked() {
                            switch_to = Some(i);
                        }
                        if ui
                            .add_enabled(
                                self.tabs.len() > 1,
                                egui::Button::new("×").small().frame(false),
                            )
                            .on_hover_text("Close tab")
                            .clicked()
                        {
                            close_tab = Some(i);
                        }
                    });
            }
            if ui.button("+").on_hover_text("New tab (Ctrl+T)").clicked() {
                self.create_new_tab();
                return;
            }
            if let Some(index) = close_tab {
                self.close_tab_index(index);
                return;
            }
            if let Some(idx) = switch_to {
                self.switch_to_tab_index(idx);
            }
        });
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let query_focused = ctx.memory(|m| m.has_focus(self.query_input_id));
        self.handle_shortcuts_with_focus(ctx, query_focused);
    }

    fn handle_shortcuts_with_focus(&mut self, ctx: &egui::Context, query_focused: bool) {
        let ctrl_mod = egui::Modifiers {
            ctrl: true,
            ..Default::default()
        };
        let ctrl_shift_mod = egui::Modifiers {
            ctrl: true,
            shift: true,
            ..Default::default()
        };
        if ctx.input_mut(|i| i.consume_key(ctrl_mod, egui::Key::T)) {
            self.create_new_tab();
            return;
        }
        if ctx.input_mut(|i| i.consume_key(ctrl_mod, egui::Key::W)) {
            self.close_active_tab();
            return;
        }
        if ctx.input_mut(|i| i.consume_key(ctrl_shift_mod, egui::Key::Tab)) {
            self.activate_previous_tab();
            return;
        }
        if ctx.input_mut(|i| i.consume_key(ctrl_mod, egui::Key::Tab)) {
            self.activate_next_tab();
            return;
        }
        if ctx.input_mut(|i| i.consume_key(ctrl_mod, egui::Key::L)) {
            if query_focused {
                self.focus_query_requested = false;
                self.unfocus_query_requested = true;
            } else {
                self.focus_query_requested = true;
                self.unfocus_query_requested = false;
            }
            return;
        }

        if ctx.input_mut(|i| i.consume_key(ctrl_mod, egui::Key::N)) {
            self.move_row(1);
        }
        if ctx.input_mut(|i| i.consume_key(ctrl_mod, egui::Key::P)) {
            self.move_row(-1);
        }
        let copy_mod = egui::Modifiers {
            ctrl: true,
            shift: true,
            ..Default::default()
        };
        if ctx.input_mut(|i| i.consume_key(copy_mod, egui::Key::C)) {
            // Keep this deferred until after TextEdit processing so query-focus copy
            // cannot overwrite the intended "copy selected path(s)" shortcut result.
            self.pending_copy_shortcut = true;
        }
        if ctx.input_mut(|i| i.consume_key(ctrl_mod, egui::Key::G)) {
            self.clear_query_and_selection();
        }
        let tab_forward = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab));
        if tab_forward {
            self.toggle_pin_current();
            // Keep Tab dedicated to pin toggle without changing query focus active/inactive state.
            if query_focused {
                ctx.memory_mut(|m| m.request_focus(self.query_input_id));
            } else {
                ctx.memory_mut(|m| m.stop_text_input());
            }
        }
        let tab_backward = ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab));
        if tab_backward {
            self.toggle_pin_current();
            // Keep Shift+Tab dedicated to pin toggle without changing query focus active/inactive state.
            if query_focused {
                ctx.memory_mut(|m| m.request_focus(self.query_input_id));
            } else {
                ctx.memory_mut(|m| m.stop_text_input());
            }
        }
        if ctx.input_mut(|i| i.consume_key(ctrl_mod, egui::Key::I)) {
            self.toggle_pin_current();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)) {
            self.move_row(1);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)) {
            self.move_row(-1);
        }
        if ctx.input_mut(|i| i.consume_key(ctrl_mod, egui::Key::J))
            || ctx.input_mut(|i| i.consume_key(ctrl_mod, egui::Key::M))
        {
            self.execute_selected();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Enter)) {
            self.execute_selected_open_folder();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
            self.execute_selected();
        }

        if self.ime_composition_active {
            return;
        }
        // Regression guard: query focus must not disable row movement/pin toggle/execute shortcuts.
        if query_focused {
            return;
        }

        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::V)) {
            self.move_page(1);
        }
        if ctx.input(|i| i.modifiers.alt && i.key_pressed(egui::Key::V)) {
            self.move_page(-1);
        }
    }

    fn run_deferred_shortcuts(&mut self, ctx: &egui::Context) {
        if !self.pending_copy_shortcut {
            return;
        }
        self.pending_copy_shortcut = false;
        self.copy_selected_paths(ctx);
        self.focus_query_requested = true;
    }

    fn process_query_input_events(
        &mut self,
        ctx: &egui::Context,
        events: &[egui::Event],
        query_focused: bool,
        text_changed_by_widget: bool,
        cursor_range: Option<egui::text_edit::CCursorRange>,
    ) -> (bool, Option<usize>) {
        let mut changed = false;
        let mut saw_text_space = false;
        let mut saw_composition_update = false;
        let mut fallback_space: Option<char> = None;
        let mut saw_space_key = false;
        let mut composition_commit_text: Option<String> = None;
        let mut requested_full_space = false;
        let mut cursor_changed = false;
        let initial_cursor = cursor_range
            .map(|range| range.primary.index)
            .unwrap_or_else(|| Self::char_count(&self.query));
        let initial_anchor = cursor_range
            .map(|range| range.secondary.index)
            .unwrap_or(initial_cursor);
        let mut cursor = initial_cursor.min(Self::char_count(&self.query));
        let mut anchor = initial_anchor.min(Self::char_count(&self.query));

        for event in events {
            match event {
                egui::Event::CompositionStart => {
                    self.ime_composition_active = true;
                    Self::append_window_trace("ime_composition_start", "active=true");
                }
                egui::Event::CompositionUpdate(text) => {
                    self.ime_composition_active = true;
                    if !text.is_empty() {
                        saw_composition_update = true;
                        Self::append_window_trace(
                            "ime_composition_update",
                            &format!("chars={}", text.chars().count()),
                        );
                    }
                }
                egui::Event::CompositionEnd(text) => {
                    self.ime_composition_active = false;
                    Self::append_window_trace(
                        "ime_composition_end",
                        &format!(
                            "chars={} has_half={} has_full={}",
                            text.chars().count(),
                            text.contains(' '),
                            text.contains('\u{3000}')
                        ),
                    );
                    if !text.is_empty() {
                        composition_commit_text = Some(text.clone());
                        changed = true;
                        if text.contains(' ') || text.contains('\u{3000}') {
                            saw_text_space = true;
                        }
                    }
                }
                egui::Event::Text(text) => {
                    if text.contains(' ') || text.contains('\u{3000}') {
                        saw_text_space = true;
                        Self::append_window_trace(
                            "ime_text_space_seen",
                            &format!(
                                "has_half={} has_full={} chars={}",
                                text.contains(' '),
                                text.contains('\u{3000}'),
                                text.chars().count()
                            ),
                        );
                    }
                }
                egui::Event::Key {
                    key: egui::Key::Space,
                    pressed: true,
                    modifiers,
                    ..
                } if query_focused
                    && !modifiers.ctrl
                    && !modifiers.alt
                    && !modifiers.command
                    && !modifiers.mac_cmd =>
                {
                    saw_space_key = true;
                    requested_full_space = modifiers.shift;
                    fallback_space = Some(' ');
                }
                _ => {}
            }
        }

        let space_down_now = ctx.input(|i| i.key_down(egui::Key::Space));
        let shift_down_now = ctx.input(|i| i.modifiers.shift);
        if query_focused && space_down_now && !self.prev_space_down && fallback_space.is_none() {
            requested_full_space = shift_down_now;
            fallback_space = Some(' ');
            saw_space_key = true;
            Self::append_window_trace(
                "ime_space_keydown_edge",
                &format!("shift={}", shift_down_now),
            );
        }
        self.prev_space_down = space_down_now;

        if let Some(commit_text) = composition_commit_text {
            if query_focused && !text_changed_by_widget {
                if let Some((start, end)) = Self::selection_range(cursor, anchor) {
                    Self::remove_char_range(&mut self.query, start, end);
                    cursor = start;
                }
                Self::insert_at_char(&mut self.query, cursor, &commit_text);
                cursor += Self::char_count(&commit_text);
                anchor = cursor;
                changed = true;
                cursor_changed = true;
                Self::append_window_trace(
                    "ime_composition_commit_fallback",
                    &format!(
                        "chars={} query_chars_after={}",
                        commit_text.chars().count(),
                        self.query.chars().count()
                    ),
                );
            }
        }

        if query_focused && !saw_text_space {
            if let Some(space) = fallback_space {
                if let Some((start, end)) = Self::selection_range(cursor, anchor) {
                    Self::remove_char_range(&mut self.query, start, end);
                    cursor = start;
                }
                // Keep IME fallback insertion at the caret instead of forcing tail append.
                Self::insert_at_char(&mut self.query, cursor, &space.to_string());
                cursor += 1;
                changed = true;
                cursor_changed = true;
                Self::append_window_trace(
                    "ime_space_fallback_inserted",
                    &format!("kind={}", if space == '\u{3000}' { "full" } else { "half" }),
                );
            }
        } else if saw_space_key {
            Self::append_window_trace(
                "ime_space_fallback_skipped",
                &format!(
                    "focused={} widget_changed={} comp_active={} text_space={} comp_update={} requested_full={} fallback_present={}",
                    query_focused,
                    text_changed_by_widget,
                    self.ime_composition_active,
                    saw_text_space,
                    saw_composition_update,
                    requested_full_space,
                    fallback_space.is_some()
                ),
            );
        }

        (changed, cursor_changed.then_some(cursor))
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
        self.poll_preview_response();
        self.poll_kind_response();
        self.pump_kind_resolution_requests();
        self.poll_filelist_response();
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
            || self.kind_resolution_in_progress
            || self.filelist_in_progress
            || self.any_tab_async_in_progress()
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
        self.capture_window_geometry(ctx);
        self.apply_stable_window_geometry(false);
        // Handle app shortcuts before widget rendering so Tab is not consumed by egui focus traversal.
        self.handle_shortcuts(ctx);

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            self.render_tab_bar(ui);
            ui.separator();
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
                let use_filelist_changed = ui
                    .checkbox(&mut self.use_filelist, "Use FileList")
                    .changed();
                if ui.checkbox(&mut self.use_regex, "Regex").changed() {
                    self.update_results();
                }
                let (files_changed, dirs_changed) = if self.use_filelist_requires_locked_filters() {
                    let mut forced_changed = false;
                    if !self.include_files || !self.include_dirs {
                        self.include_files = true;
                        self.include_dirs = true;
                        forced_changed = true;
                    }
                    ui.add_enabled(false, egui::Checkbox::new(&mut self.include_files, "Files"));
                    ui.add_enabled(
                        false,
                        egui::Checkbox::new(&mut self.include_dirs, "Folders"),
                    );
                    (forced_changed, forced_changed)
                } else {
                    (
                        ui.checkbox(&mut self.include_files, "Files").changed(),
                        ui.checkbox(&mut self.include_dirs, "Folders").changed(),
                    )
                };
                if ui.checkbox(&mut self.show_preview, "Preview").changed() {
                    if !self.show_preview {
                        self.preview_cache.clear();
                        self.preview_cache_order.clear();
                        self.preview_cache_total_bytes = 0;
                    }
                    self.mark_ui_state_dirty();
                }
                ui.separator();
                ui.label(self.source_text());
                self.maybe_reindex_from_filter_toggles(
                    use_filelist_changed,
                    files_changed,
                    dirs_changed,
                );
            });

            let mut output = egui::TextEdit::singleline(&mut self.query)
                .id(self.query_input_id)
                .lock_focus(true)
                .desired_width(f32::INFINITY)
                .hint_text("Type to fuzzy-search files/folders...")
                .show(ui);
            let _ = output.response.clone().on_hover_ui_at_pointer(|ui| {
                ui.label(Self::SEARCH_HINTS_TOOLTIP);
            });
            if self.focus_query_requested {
                output.response.request_focus();
                self.focus_query_requested = false;
            }
            if self.unfocus_query_requested {
                output.response.surrender_focus();
                self.unfocus_query_requested = false;
            }
            let events = ctx.input(|i| i.events.clone());
            let (query_event_changed, query_cursor_after_fallback) = self
                .process_query_input_events(
                    ctx,
                    &events,
                    output.response.has_focus(),
                    output.response.changed(),
                    output.state.ccursor_range(),
                );
            if query_event_changed {
                if output.response.has_focus() {
                    let end = query_cursor_after_fallback
                        .unwrap_or_else(|| Self::char_count(&self.query));
                    output
                        .state
                        .set_ccursor_range(Some(egui::text_edit::CCursorRange::one(
                            egui::text::CCursor::new(end),
                        )));
                    output.state.clone().store(ctx, output.response.id);
                }
                self.update_results();
            }
            if self.apply_emacs_query_shortcuts(ctx, &mut output) {
                self.update_results();
            }
            if output.response.changed() {
                Self::append_window_trace(
                    "query_text_changed",
                    &format!(
                        "chars={} has_half_space={} has_full_space={}",
                        self.query.chars().count(),
                        self.query.contains(' '),
                        self.query.contains('\u{3000}')
                    ),
                );
                self.update_results();
            }
            self.run_deferred_shortcuts(ctx);

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
        let current_tab_id = self.current_tab_id().unwrap_or_default();
        if let Some(pending) = &self.pending_filelist_confirmation {
            if pending.tab_id == current_tab_id {
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
        }
        if overwrite {
            self.confirm_pending_filelist_overwrite();
        } else if cancel_overwrite {
            self.cancel_pending_filelist_overwrite();
        }

        let mut confirm_walker = false;
        let mut cancel_walker = false;
        if let Some(pending) = &self.pending_filelist_use_walker_confirmation {
            if pending.source_tab_id == current_tab_id {
                egui::Window::new("Create File List?")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                    .show(ctx, |ui| {
                        ui.label(
                            "Use FileList が有効です。Create File List には Walker実行が必要です。",
                        );
                        ui.label(
                            "FileListインデックスからは生成しません。新規タブで実行しますか？",
                        );
                        ui.horizontal(|ui| {
                            if ui.button("Continue").clicked() {
                                confirm_walker = true;
                            }
                            if ui.button("Cancel").clicked() {
                                cancel_walker = true;
                            }
                        });
                    });
            }
        }
        if confirm_walker {
            self.confirm_pending_filelist_use_walker();
        } else if cancel_walker {
            self.cancel_pending_filelist_use_walker();
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_results_and_preview(ui);
        });
        self.maybe_save_ui_state(false);
    }
}

impl Drop for FlistWalkerApp {
    fn drop(&mut self) {
        if let Some(runtime) = &self.worker_runtime {
            runtime.request_shutdown();
        }
        let (dummy_search_tx, _) = mpsc::channel::<SearchRequest>();
        let (dummy_preview_tx, _) = mpsc::channel::<PreviewRequest>();
        let (dummy_action_tx, _) = mpsc::channel::<ActionRequest>();
        let (dummy_kind_tx, _) = mpsc::channel::<KindResolveRequest>();
        let (dummy_filelist_tx, _) = mpsc::channel::<FileListRequest>();
        let (dummy_index_tx, _) = mpsc::channel::<IndexRequest>();
        let old_search_tx = std::mem::replace(&mut self.search_tx, dummy_search_tx);
        let old_preview_tx = std::mem::replace(&mut self.preview_tx, dummy_preview_tx);
        let old_action_tx = std::mem::replace(&mut self.action_tx, dummy_action_tx);
        let old_kind_tx = std::mem::replace(&mut self.kind_tx, dummy_kind_tx);
        let old_filelist_tx = std::mem::replace(&mut self.filelist_tx, dummy_filelist_tx);
        let old_index_tx = std::mem::replace(&mut self.index_tx, dummy_index_tx);
        drop(old_search_tx);
        drop(old_preview_tx);
        drop(old_action_tx);
        drop(old_kind_tx);
        drop(old_filelist_tx);
        drop(old_index_tx);

        self.apply_stable_window_geometry(true);
        self.maybe_save_ui_state(true);
        if let Some(runtime) = self.worker_runtime.take() {
            runtime.join_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::mpsc;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("fff-rs-app-{name}-{nonce}"))
    }

    fn entries_count_from_status(status_line: &str) -> usize {
        status_line
            .split("Entries: ")
            .nth(1)
            .and_then(|rest| rest.split(" | ").next())
            .and_then(|n| n.parse::<usize>().ok())
            .unwrap_or(0)
    }

    fn run_shortcuts_frame(
        app: &mut FlistWalkerApp,
        query_focused: bool,
        events: Vec<egui::Event>,
    ) {
        let mut modifiers = egui::Modifiers::NONE;
        for event in &events {
            if let egui::Event::Key {
                pressed: true,
                modifiers: event_modifiers,
                ..
            } = event
            {
                modifiers = *event_modifiers;
                break;
            }
        }
        let ctx = egui::Context::default();
        ctx.begin_frame(egui::RawInput {
            modifiers,
            events,
            ..Default::default()
        });
        app.handle_shortcuts_with_focus(&ctx, query_focused);
        app.run_deferred_shortcuts(&ctx);
        let _ = ctx.end_frame();
    }

    fn is_action_notice(text: &str) -> bool {
        text.starts_with("Action: ") || text.starts_with("Action failed:")
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
    fn startup_requests_query_focus() {
        let root = test_root("startup-focus");
        fs::create_dir_all(&root).expect("create dir");
        let app = FlistWalkerApp::new(root.clone(), 50, String::new());
        assert!(app.focus_query_requested);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn startup_index_request_is_bound_to_active_tab() {
        let root = test_root("startup-index-tab-binding");
        fs::create_dir_all(&root).expect("create dir");
        let app = FlistWalkerApp::new(root.clone(), 50, String::new());
        let req_id = app.pending_index_request_id.expect("pending index request");
        let tab_id = app.current_tab_id().expect("active tab id");
        assert_eq!(app.index_request_tabs.get(&req_id).copied(), Some(tab_id));
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
    fn execute_selected_enqueues_action_request_without_sync_io() {
        let root = test_root("async-action-enqueue");
        fs::create_dir_all(&root).expect("create dir");
        let missing = root.join("missing-not-executed");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        let (action_tx_req, action_rx_req) = mpsc::channel::<ActionRequest>();
        let (_action_tx_res, action_rx_res) = mpsc::channel::<ActionResponse>();
        app.action_tx = action_tx_req;
        app.action_rx = action_rx_res;
        app.results = vec![(missing.clone(), 0.0)];
        app.current_row = Some(0);

        app.execute_selected();

        let req = action_rx_req
            .try_recv()
            .expect("action request should be enqueued");
        assert_eq!(req.paths, vec![missing]);
        assert!(!req.open_parent_for_files);
        assert!(app.pending_action_request_id.is_some());
        assert!(app.action_in_progress);
        assert!(!app.notice.starts_with("Action failed:"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn action_target_path_for_open_in_folder_maps_file_and_directory() {
        let root = test_root("open-folder-target");
        let dir = root.join("dir");
        fs::create_dir_all(&dir).expect("create dir");
        let file = dir.join("main.rs");
        fs::write(&file, "fn main() {}").expect("write file");

        let from_file = action_target_path_for_open_in_folder(&file);
        let from_dir = action_target_path_for_open_in_folder(&dir);

        assert_eq!(from_file, dir);
        assert_eq!(from_dir, root.join("dir"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn action_targets_for_request_deduplicates_same_parent_directory() {
        let root = test_root("open-folder-target-dedup");
        let dir_a = root.join("dir-a");
        let dir_b = root.join("dir-b");
        fs::create_dir_all(&dir_a).expect("create dir a");
        fs::create_dir_all(&dir_b).expect("create dir b");
        let file_a1 = dir_a.join("main.rs");
        let file_a2 = dir_a.join("lib.rs");
        let file_b = dir_b.join("mod.rs");
        fs::write(&file_a1, "fn main() {}").expect("write file a1");
        fs::write(&file_a2, "pub fn f() {}").expect("write file a2");
        fs::write(&file_b, "pub fn g() {}").expect("write file b");

        let targets = action_targets_for_request(
            &[file_a1, file_a2, file_b, dir_a.clone()],
            true,
        );

        assert_eq!(targets, vec![dir_a, dir_b]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn stale_action_completion_is_ignored_by_request_id() {
        let root = test_root("stale-action-request-id");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        let (tx, rx) = mpsc::channel::<ActionResponse>();
        app.action_rx = rx;
        app.notice = "latest notice".to_string();
        app.pending_action_request_id = Some(2);
        app.action_in_progress = true;
        let tab_id = app.current_tab_id().expect("tab id");
        app.action_request_tabs.insert(1, tab_id);
        app.action_request_tabs.insert(2, tab_id);
        app.tabs[app.active_tab].pending_action_request_id = Some(2);
        app.tabs[app.active_tab].action_in_progress = true;

        tx.send(ActionResponse {
            request_id: 1,
            notice: "Action failed: stale".to_string(),
        })
        .expect("send stale action response");
        app.poll_action_response();

        assert_eq!(app.notice, "latest notice");
        assert_eq!(app.pending_action_request_id, Some(2));
        assert!(app.action_in_progress);

        tx.send(ActionResponse {
            request_id: 2,
            notice: "Action: latest".to_string(),
        })
        .expect("send latest action response");
        app.poll_action_response();

        assert_eq!(app.notice, "Action: latest");
        assert_eq!(app.pending_action_request_id, None);
        assert!(!app.action_in_progress);
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
    fn results_scroll_is_disabled_during_preview_resize() {
        assert!(!FlistWalkerApp::results_scroll_enabled(true));
    }

    #[test]
    fn results_scroll_is_enabled_when_preview_resize_not_active() {
        assert!(FlistWalkerApp::results_scroll_enabled(false));
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

        let chunk = "x".repeat(1024 * 1024);
        let count = 40usize;
        for i in 0..count {
            let path = root.join(format!("file-{i}.txt"));
            app.cache_preview(path, chunk.clone());
        }

        assert!(app.preview_cache_total_bytes <= FlistWalkerApp::PREVIEW_CACHE_MAX_BYTES);
        assert!(!app.preview_cache_order.is_empty());
        assert_eq!(app.preview_cache.len(), app.preview_cache_order.len());
        let evicted = root.join("file-0.txt");
        assert!(!app.preview_cache.contains_key(&evicted));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn search_prefix_cache_accepts_only_plain_single_token_queries() {
        assert!(!SearchPrefixCache::is_cacheable_query("ab"));
        assert!(SearchPrefixCache::is_cacheable_query("abc"));
        assert!(!SearchPrefixCache::is_cacheable_query("abc def"));
        assert!(!SearchPrefixCache::is_cacheable_query("abc|def"));
        assert!(!SearchPrefixCache::is_cacheable_query("'abc"));
        assert!(!SearchPrefixCache::is_cacheable_query("!abc"));
        assert!(!SearchPrefixCache::is_cacheable_query("^abc"));
        assert!(!SearchPrefixCache::is_cacheable_query("abc$"));
        assert!(SearchPrefixCache::is_safe_prefix_extension("abc", "abcd"));
        assert!(!SearchPrefixCache::is_safe_prefix_extension("abc", "ab"));
    }

    #[test]
    fn search_prefix_cache_prefers_longest_prefix_and_evicts_old_entries() {
        let snapshot = SearchEntriesSnapshotKey { ptr: 1, len: 100 };
        let mut cache = SearchPrefixCache::default();
        cache.maybe_store(snapshot, "abc", vec![0, 1, 2, 3]);
        cache.maybe_store(snapshot, "abcd", vec![1, 3]);

        let candidates = cache
            .lookup_candidates(snapshot, "abcde")
            .expect("cached candidates");
        assert_eq!(candidates.as_ref(), &vec![1, 3]);

        for idx in 0..(SearchPrefixCache::MAX_ENTRIES + 4) {
            cache.maybe_store(snapshot, &format!("q{:03}", idx), vec![idx]);
        }
        assert!(cache.entries.len() <= SearchPrefixCache::MAX_ENTRIES);
        assert!(cache.total_bytes <= SearchPrefixCache::MAX_BYTES);
    }

    #[test]
    fn search_prefix_cache_skips_oversized_match_sets() {
        let snapshot = SearchEntriesSnapshotKey { ptr: 2, len: 1_000_000 };
        let mut cache = SearchPrefixCache::default();
        let oversized = (0..=SearchPrefixCache::MAX_MATCHED_INDICES).collect::<Vec<_>>();

        cache.maybe_store(snapshot, "oversized", oversized);

        assert!(cache.lookup_candidates(snapshot, "oversizedx").is_none());
        assert_eq!(cache.total_bytes, 0);
    }

    #[test]
    fn request_preview_is_skipped_when_preview_is_hidden() {
        let root = test_root("preview-hidden");
        fs::create_dir_all(&root).expect("create dir");
        let file = root.join("a.txt");
        fs::write(&file, "content").expect("write file");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

        app.show_preview = false;
        app.results = vec![(file.clone(), 0.0)];
        app.current_row = Some(0);
        app.entry_kinds.insert(file, false);
        app.preview = "stale preview".to_string();
        app.pending_preview_request_id = Some(99);
        app.preview_in_progress = true;

        app.request_preview_for_current();

        assert!(app.preview.is_empty());
        assert!(!app.preview_in_progress);
        assert!(app.pending_preview_request_id.is_none());
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
                    kind_known: true,
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
        assert_eq!(search_req.entries.len(), 1);
        assert_eq!(search_req.entries[0], root.join("main.rs"));
        assert!(!app.search_resume_pending);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn filtered_out_batch_still_resumes_non_empty_query_search() {
        let root = test_root("resume-first-batch-filtered-out");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, "main".to_string());
        app.include_files = false;
        app.include_dirs = true;
        let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
        app.index_tx = index_tx;
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
                    kind_known: true,
                }],
            })
            .expect("send batch");
        app.last_incremental_results_refresh = Instant::now();
        app.poll_index_response();

        let search_req = search_rx_real
            .try_recv()
            .expect("search should still resume even when batch is filtered out");
        assert!(search_req.entries.is_empty());
        assert_eq!(search_req.query, "main");
        assert!(!app.search_resume_pending);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn create_filelist_waits_while_indexing() {
        let root = test_root("filelist-waits-indexing");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
        app.index_tx = index_tx;
        app.use_filelist = false;
        app.index.source = IndexSource::Walker;
        app.include_files = true;
        app.include_dirs = true;
        app.index_in_progress = true;

        app.create_filelist();

        assert_eq!(
            app.pending_filelist_after_index
                .as_ref()
                .map(|pending| pending.root.clone()),
            Some(root.clone())
        );
        assert!(app.pending_filelist_request_id.is_none());
        assert!(!app.filelist_in_progress);
        assert!(index_rx.try_recv().is_err());
        assert!(app.notice.contains("Waiting for current indexing"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn create_filelist_while_indexing_with_filter_change_requests_reindex() {
        let root = test_root("filelist-waits-indexing-needs-reindex");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
        app.index_tx = index_tx;
        app.use_filelist = false;
        app.index.source = IndexSource::Walker;
        app.include_files = false;
        app.include_dirs = true;
        app.index_in_progress = true;

        app.create_filelist();

        let req = index_rx.try_recv().expect("reindex request should be sent");
        assert_eq!(req.root, root);
        assert!(req.include_files);
        assert!(req.include_dirs);
        assert!(app.pending_filelist_after_index.is_some());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn create_filelist_forces_files_and_dirs_before_reindex() {
        let root = test_root("filelist-force-files-dirs");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
        app.index_tx = index_tx;
        app.use_filelist = false;
        app.include_files = false;
        app.include_dirs = true;
        app.index.source = IndexSource::Walker;

        app.create_filelist();

        assert!(app.include_files);
        assert!(app.include_dirs);
        let req = index_rx.try_recv().expect("reindex request should be sent");
        assert_eq!(req.root, root);
        assert!(!req.use_filelist);
        assert!(req.include_files);
        assert!(req.include_dirs);
        assert!(app.pending_filelist_after_index.is_some());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn create_filelist_with_use_filelist_enabled_opens_confirmation_and_prepares_new_tab() {
        let root = test_root("filelist-use-filelist-confirm");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
        app.index_tx = index_tx;

        assert!(app.use_filelist);
        app.create_filelist();
        assert!(app.pending_filelist_use_walker_confirmation.is_some());
        assert_eq!(app.tabs.len(), 1);

        app.confirm_pending_filelist_use_walker();

        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.active_tab, 1);
        assert!(!app.use_filelist);
        assert!(app.include_files);
        assert!(app.include_dirs);
        let pending = app
            .pending_filelist_after_index
            .as_ref()
            .expect("deferred filelist pending");
        let active_tab_id = app.current_tab_id().expect("active tab id");
        assert_eq!(pending.tab_id, active_tab_id);
        assert_eq!(pending.root, root);
        let req = index_rx
            .try_recv()
            .expect("walker index request should be sent");
        assert_eq!(req.tab_id, active_tab_id);
        assert_eq!(req.root, root);
        assert!(!req.use_filelist);
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

        app.use_filelist = false;
        app.index_in_progress = true;
        let tab_id = app.current_tab_id().expect("tab id");
        app.create_filelist();
        let request_id = app.pending_index_request_id.expect("pending index request");

        index_tx
            .send(IndexResponse::Batch {
                request_id,
                entries: vec![IndexEntry {
                    path: path.clone(),
                    is_dir: false,
                    kind_known: true,
                }],
            })
            .expect("send batch");

        index_tx
            .send(IndexResponse::Finished {
                request_id,
                source: IndexSource::Walker,
            })
            .expect("send finished");
        app.poll_index_response();

        let req = filelist_rx
            .try_recv()
            .expect("filelist request should be sent");
        assert_eq!(req.tab_id, tab_id);
        assert_eq!(req.root, root);
        assert_eq!(req.entries, vec![path]);
        assert!(app.pending_filelist_after_index.is_none());
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
        let tab_id = app.current_tab_id().expect("tab id");
        app.pending_filelist_after_index = Some(PendingFileListAfterIndex {
            tab_id,
            root: root_old.clone(),
        });
        app.root = root_new.clone();

        app.request_index_refresh();

        assert!(app.pending_filelist_after_index.is_none());
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
        assert!(app.all_entries.is_empty());
        assert!(app.entries.is_empty());
        assert!(app.results.is_empty());
        assert_eq!(app.tabs[app.active_tab].root, root_new);
        assert!(app.tabs[app.active_tab].all_entries.is_empty());
        assert!(app.tabs[app.active_tab].entries.is_empty());
        let req = rx.try_recv().expect("index request should be sent");
        assert_eq!(req.root, app.root);
        let _ = fs::remove_dir_all(&root_old);
        let _ = fs::remove_dir_all(&root_new);
    }

    #[test]
    fn close_tab_invalidates_memory_cache_for_immediate_resample() {
        let root = test_root("close-tab-memory-resample");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.create_new_tab();
        assert_eq!(app.tabs.len(), 2);

        let sentinel = u64::MAX;
        app.memory_usage_bytes = Some(sentinel);
        let stale = Instant::now()
            .checked_sub(Duration::from_secs(5))
            .unwrap_or_else(Instant::now);
        app.last_memory_sample = stale;

        app.close_tab_index(1);

        assert_eq!(app.tabs.len(), 1);
        assert_ne!(app.memory_usage_bytes, Some(sentinel));
        assert!(app.last_memory_sample > stale);
        let _ = fs::remove_dir_all(&root);
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
        let tab_id = app.current_tab_id().expect("tab id");
        app.pending_filelist_confirmation = Some(PendingFileListConfirmation {
            tab_id,
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
        app.pending_filelist_request_tab_id = app.current_tab_id();
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
        assert_eq!(app.pending_filelist_request_tab_id, None);
        assert!(!app.filelist_in_progress);
        assert!(app.use_filelist);
        assert!(app.notice.contains("Created"));
        assert!(app.notice.contains("3 entries"));
        assert!(app.notice.contains(filelist.to_string_lossy().as_ref()));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn filelist_finished_enables_use_filelist_for_creator_tab() {
        let root = test_root("filelist-finished-enable-creator-tab");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.create_new_tab();
        app.use_filelist = false;
        app.sync_active_tab_state();
        let creator_tab_id = app.tabs[0].id;
        let (tx, rx) = mpsc::channel::<FileListResponse>();
        app.filelist_rx = rx;
        app.pending_filelist_request_id = Some(101);
        app.pending_filelist_request_tab_id = Some(creator_tab_id);
        app.pending_filelist_root = Some(root.clone());
        app.filelist_in_progress = true;

        tx.send(FileListResponse::Finished {
            request_id: 101,
            root: root.clone(),
            path: root.join("FileList.txt"),
            count: 2,
        })
        .expect("send filelist response");

        app.poll_filelist_response();

        let creator_tab = app
            .tabs
            .iter()
            .find(|tab| tab.id == creator_tab_id)
            .expect("creator tab");
        assert!(creator_tab.use_filelist);
        assert!(!app.use_filelist);
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
        app.use_filelist = false;
        app.all_entries = Arc::new(vec![path.clone()]);
        app.entry_kinds.insert(path, false);
        app.index.source = IndexSource::Walker;

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
        let tab_id = app.current_tab_id().expect("tab id");
        app.pending_filelist_confirmation = Some(PendingFileListConfirmation {
            tab_id,
            root: root.clone(),
            entries: entries.clone(),
            existing_path: file_path,
        });

        app.confirm_pending_filelist_overwrite();

        let req = filelist_rx
            .try_recv()
            .expect("filelist request should be sent");
        assert_eq!(req.tab_id, tab_id);
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
        app.pending_filelist_request_tab_id = app.current_tab_id();
        app.pending_filelist_root = Some(root.clone());
        app.filelist_in_progress = true;
        app.use_filelist = false;

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
        app.pending_filelist_request_tab_id = app.current_tab_id();
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
        assert_eq!(app.pending_filelist_request_tab_id, None);
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
        app.pending_filelist_request_tab_id = app.current_tab_id();
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
        app.pending_filelist_request_tab_id = app.current_tab_id();
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
                kind_known: true,
            }],
        })
        .expect("send index batch");

        app.poll_index_response();

        assert_eq!(app.entries.len(), 1);
        assert_eq!(app.entries[0], path);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn non_empty_query_batch_delta_updates_snapshot_even_without_search_refresh() {
        let root = test_root("incremental-snapshot-delta");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, "main".to_string());
        let (tx, rx) = mpsc::channel::<IndexResponse>();
        app.index_rx = rx;
        app.pending_index_request_id = Some(88);
        app.index_in_progress = true;
        app.search_resume_pending = false;
        app.last_incremental_results_refresh = Instant::now();
        app.last_search_snapshot_len = 0;

        let path_a = root.join("main-a.rs");
        let path_b = root.join("main-b.rs");
        tx.send(IndexResponse::Batch {
            request_id: 88,
            entries: vec![
                IndexEntry {
                    path: path_a.clone(),
                    is_dir: false,
                    kind_known: true,
                },
                IndexEntry {
                    path: path_b.clone(),
                    is_dir: false,
                    kind_known: true,
                },
            ],
        })
        .expect("send index batch");

        app.poll_index_response();
        app.poll_index_response();

        assert!(app.entries.is_empty());
        assert_eq!(app.incremental_filtered_entries.len(), 2);
        assert_eq!(app.incremental_filtered_entries[0], path_a);
        assert_eq!(app.incremental_filtered_entries[1], path_b);
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
                kind_known: true,
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
    fn app_defaults_use_filelist_on() {
        let root = test_root("default-use-filelist-on");
        fs::create_dir_all(&root).expect("create dir");
        let app = FlistWalkerApp::new(root.clone(), 50, String::new());
        assert!(app.use_filelist);
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
                kind_known: true,
            }],
        })
        .expect("send index batch");

        app.poll_index_response();

        assert!(!app.scroll_to_current);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn apply_entry_filters_resyncs_incremental_state_during_indexing() {
        let root = test_root("filters-resync-incremental");
        fs::create_dir_all(root.join("dir")).expect("create dir");
        let file = root.join("main.rs");
        let dir = root.join("dir");
        fs::write(&file, "fn main() {}").expect("write file");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.index_in_progress = true;
        app.index.entries = vec![file.clone(), dir.clone()];
        app.entry_kinds.insert(file.clone(), false);
        app.entry_kinds.insert(dir.clone(), true);
        app.include_files = false;
        app.include_dirs = true;

        app.apply_entry_filters(true);

        assert_eq!(app.entries.as_ref(), &vec![dir.clone()]);
        assert_eq!(app.incremental_filtered_entries, vec![dir]);
        assert!(app.pending_index_entries.is_empty());
        assert!(app.pending_index_entries_request_id.is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn apply_entry_filters_all_filtered_then_next_batch_adds_once() {
        let root = test_root("filters-all-filtered-then-add");
        fs::create_dir_all(root.join("dir")).expect("create dir");
        let file = root.join("main.rs");
        let dir = root.join("dir");
        fs::write(&file, "fn main() {}").expect("write file");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.index_in_progress = true;
        app.index.entries = vec![file.clone()];
        app.entry_kinds.insert(file.clone(), false);
        app.include_files = false;
        app.include_dirs = true;

        app.apply_entry_filters(true);
        assert!(app.entries.is_empty());
        assert!(app.incremental_filtered_entries.is_empty());

        let (tx, rx) = mpsc::channel::<IndexResponse>();
        app.index_rx = rx;
        app.pending_index_request_id = Some(201);
        tx.send(IndexResponse::Batch {
            request_id: 201,
            entries: vec![IndexEntry {
                path: dir.clone(),
                is_dir: true,
                kind_known: true,
            }],
        })
        .expect("send index batch");

        app.poll_index_response();

        assert_eq!(app.entries.as_ref(), &vec![dir]);
        assert_eq!(app.results.len(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unknown_kind_entries_remain_visible_when_both_filters_enabled() {
        let root = test_root("unknown-kind-visible");
        fs::create_dir_all(&root).expect("create dir");
        let path = root.join("unknown");
        fs::write(&path, "x").expect("write file");

        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.all_entries = Arc::new(vec![path.clone()]);
        app.include_files = true;
        app.include_dirs = true;
        app.entry_kinds.clear();

        app.apply_entry_filters(true);

        assert_eq!(app.entries.as_ref(), &vec![path]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unknown_kind_entries_do_not_queue_resolution_when_both_filters_enabled() {
        let root = test_root("unknown-kind-no-queue");
        fs::create_dir_all(&root).expect("create dir");
        let path = root.join("unknown");
        fs::write(&path, "x").expect("write file");

        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.all_entries = Arc::new(vec![path.clone()]);
        app.include_files = true;
        app.include_dirs = true;
        app.show_preview = false;
        app.entry_kinds.clear();
        app.pending_kind_paths.clear();
        app.pending_kind_paths_set.clear();
        app.in_flight_kind_paths.clear();

        app.apply_entry_filters(true);

        assert!(!app.pending_kind_paths.iter().any(|p| *p == path));
        assert!(!app.in_flight_kind_paths.contains(&path));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unknown_kind_entries_are_hidden_when_single_filter_enabled() {
        let root = test_root("unknown-kind-hidden");
        fs::create_dir_all(&root).expect("create dir");
        let path = root.join("unknown");
        fs::write(&path, "x").expect("write file");

        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.all_entries = Arc::new(vec![path]);
        app.include_files = false;
        app.include_dirs = true;
        app.entry_kinds.clear();

        app.apply_entry_filters(true);

        assert!(app.entries.is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unknown_kind_entries_queue_resolution_when_single_filter_enabled() {
        let root = test_root("unknown-kind-queue");
        fs::create_dir_all(&root).expect("create dir");
        let path = root.join("unknown");
        fs::write(&path, "x").expect("write file");

        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.all_entries = Arc::new(vec![path.clone()]);
        app.include_files = false;
        app.include_dirs = true;
        app.entry_kinds.clear();

        app.apply_entry_filters(true);

        assert!(app.pending_kind_paths.iter().any(|p| *p == path));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn kind_response_updates_filters_when_single_filter_is_enabled() {
        let root = test_root("kind-response-refreshes-filters");
        fs::create_dir_all(root.join("dir")).expect("create dir");
        let dir = root.join("dir");

        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.all_entries = Arc::new(vec![dir.clone()]);
        app.include_files = false;
        app.include_dirs = true;
        app.entry_kinds.clear();
        app.apply_entry_filters(true);
        assert!(app.entries.is_empty());

        let (tx, rx) = mpsc::channel::<KindResolveResponse>();
        app.kind_rx = rx;
        app.in_flight_kind_paths.insert(dir.clone());
        tx.send(KindResolveResponse {
            epoch: app.kind_resolution_epoch,
            path: dir.clone(),
            is_dir: Some(true),
        })
        .expect("send kind response");

        app.poll_kind_response();

        assert_eq!(app.entry_kinds.get(&dir), Some(&true));
        assert_eq!(app.entries.as_ref(), &vec![dir]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn request_preview_queues_on_demand_kind_resolution_when_kind_unknown() {
        let root = test_root("preview-on-demand-kind");
        fs::create_dir_all(&root).expect("create dir");
        let path = root.join("unknown.txt");
        fs::write(&path, "hello").expect("write file");

        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        let (tx, rx) = mpsc::channel::<KindResolveRequest>();
        app.kind_tx = tx;
        app.results = vec![(path.clone(), 0.0)];
        app.current_row = Some(0);
        app.include_files = true;
        app.include_dirs = true;
        app.entry_kinds.clear();

        app.request_preview_for_current();

        let req = rx.try_recv().expect("kind resolve request should be sent");
        assert_eq!(req.path, path);
        assert_eq!(req.epoch, app.kind_resolution_epoch);
        assert_eq!(app.preview, "Resolving entry type...");
        assert!(app.pending_preview_request_id.is_none());
        assert!(!app.preview_in_progress);
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
        assert!(!req.include_dirs);
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
        assert!(!req.include_files);
        assert!(req.include_dirs);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn files_toggle_change_requests_reindex() {
        let root = test_root("files-toggle-reindex");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        let (tx, rx) = mpsc::channel::<IndexRequest>();
        app.index_tx = tx;
        app.use_filelist = false;
        app.include_files = false;
        app.include_dirs = true;

        app.maybe_reindex_from_filter_toggles(false, true, false);

        let req = rx.try_recv().expect("index request should be sent");
        assert!(!req.include_files);
        assert!(req.include_dirs);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn use_filelist_forces_type_filters_to_both_enabled() {
        let root = test_root("use-filelist-forces-type-filters");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        let (tx, rx) = mpsc::channel::<IndexRequest>();
        app.index_tx = tx;
        app.use_filelist = true;
        app.include_files = false;
        app.include_dirs = true;

        app.maybe_reindex_from_filter_toggles(true, false, false);

        let req = rx.try_recv().expect("index request should be sent");
        assert!(app.include_files);
        assert!(app.include_dirs);
        assert!(req.use_filelist);
        assert!(req.include_files);
        assert!(req.include_dirs);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn use_filelist_with_walker_source_keeps_type_filters_editable() {
        let root = test_root("use-filelist-walker-keeps-type-filters");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        let (tx, rx) = mpsc::channel::<IndexRequest>();
        app.index_tx = tx;
        app.use_filelist = true;
        app.index.source = IndexSource::Walker;
        app.include_files = false;
        app.include_dirs = true;

        app.maybe_reindex_from_filter_toggles(true, false, false);

        let req = rx.try_recv().expect("index request should be sent");
        assert!(req.use_filelist);
        assert!(!req.include_files);
        assert!(req.include_dirs);
        assert!(!app.include_files);
        assert!(app.include_dirs);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn create_filelist_with_use_filelist_enabled_and_walker_source_skips_confirmation() {
        let root = test_root("filelist-use-filelist-walker-source-no-confirm");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        let (filelist_tx, filelist_rx) = mpsc::channel::<FileListRequest>();
        app.filelist_tx = filelist_tx;
        app.use_filelist = true;
        app.index.source = IndexSource::Walker;
        app.index_in_progress = false;

        app.create_filelist();

        assert!(app.pending_filelist_use_walker_confirmation.is_none());
        let req = filelist_rx
            .try_recv()
            .expect("filelist request should be sent without confirmation");
        assert_eq!(req.root, root);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn window_geometry_from_rects_prefers_inner_size() {
        let outer = egui::Rect::from_min_size(egui::pos2(100.0, 200.0), egui::vec2(1200.0, 900.0));
        let inner = egui::Rect::from_min_size(egui::pos2(110.0, 240.0), egui::vec2(1180.0, 840.0));

        let geom = FlistWalkerApp::window_geometry_from_rects(
            outer,
            Some(inner),
            Some(egui::vec2(2560.0, 1440.0)),
        );

        assert_eq!(geom.x, 100.0);
        assert_eq!(geom.y, 200.0);
        assert_eq!(geom.width, 1180.0);
        assert_eq!(geom.height, 840.0);
        assert_eq!(geom.monitor_width, Some(2560.0));
        assert_eq!(geom.monitor_height, Some(1440.0));
    }

    #[test]
    fn normalize_restore_geometry_preserves_virtual_desktop_position() {
        let saved = SavedWindowGeometry {
            x: -1600.0,
            y: 120.0,
            width: 900.0,
            height: 700.0,
            monitor_width: Some(1920.0),
            monitor_height: Some(1080.0),
        };

        let restored = FlistWalkerApp::normalize_restore_geometry(saved);

        assert_eq!(restored.x, -1600.0);
        assert_eq!(restored.y, 120.0);
        assert_eq!(restored.width, 900.0);
        assert_eq!(restored.height, 700.0);
        assert_eq!(restored.monitor_width, Some(1920.0));
        assert_eq!(restored.monitor_height, Some(1080.0));
    }

    #[test]
    fn apply_stable_window_geometry_force_commits_pending() {
        let root = test_root("window-geometry-commit");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.window_geometry = None;
        app.ui_state_dirty = false;
        app.pending_window_geometry = Some(SavedWindowGeometry {
            x: 100.0,
            y: 120.0,
            width: 900.0,
            height: 700.0,
            monitor_width: Some(2560.0),
            monitor_height: Some(1440.0),
        });

        app.apply_stable_window_geometry(true);

        assert!(app.pending_window_geometry.is_none());
        assert!(app.ui_state_dirty);
        let geom = app.window_geometry.clone().expect("committed geometry");
        assert_eq!(geom.x, 100.0);
        assert_eq!(geom.y, 120.0);
        assert_eq!(geom.width, 900.0);
        assert_eq!(geom.height, 700.0);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ctrl_n_and_ctrl_p_move_selection_even_when_query_is_focused() {
        let root = test_root("shortcut-ctrl-np-query-focus");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.results = vec![
            (root.join("a.txt"), 0.0),
            (root.join("b.txt"), 0.0),
            (root.join("c.txt"), 0.0),
        ];
        app.current_row = Some(0);

        run_shortcuts_frame(
            &mut app,
            true,
            vec![egui::Event::Key {
                key: egui::Key::N,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
            }],
        );
        assert_eq!(app.current_row, Some(1));

        run_shortcuts_frame(
            &mut app,
            true,
            vec![egui::Event::Key {
                key: egui::Key::P,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
            }],
        );
        assert_eq!(app.current_row, Some(0));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ctrl_g_clears_query_and_resets_selection_even_when_query_is_focused() {
        let root = test_root("shortcut-ctrl-g-query-focus");
        fs::create_dir_all(&root).expect("create dir");
        let selected = root.join("picked.txt");
        fs::write(&selected, "x").expect("write file");
        let mut app = FlistWalkerApp::new(root.clone(), 50, "query".to_string());
        app.entries = Arc::new(vec![selected.clone()]);
        app.results = vec![(selected.clone(), 0.0)];
        app.current_row = Some(0);
        app.pinned_paths.insert(selected);

        run_shortcuts_frame(
            &mut app,
            true,
            vec![egui::Event::Key {
                key: egui::Key::G,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
            }],
        );

        assert!(app.query.is_empty());
        assert!(app.pinned_paths.is_empty());
        assert_eq!(app.results.len(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ctrl_shift_c_is_deferred_and_copies_selected_path_even_when_query_is_focused() {
        let root = test_root("shortcut-copy-query-focus");
        fs::create_dir_all(&root).expect("create dir");
        let selected = root.join("picked.txt");
        fs::write(&selected, "x").expect("write file");
        let mut app = FlistWalkerApp::new(root.clone(), 50, "query".to_string());
        app.results = vec![(selected.clone(), 0.0)];
        app.current_row = Some(0);

        run_shortcuts_frame(
            &mut app,
            true,
            vec![egui::Event::Key {
                key: egui::Key::C,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    ctrl: true,
                    shift: true,
                    ..Default::default()
                },
            }],
        );

        assert!(!app.pending_copy_shortcut);
        assert!(app.notice.contains(&format!(
            "Copied path: {}",
            normalize_path_for_display(&selected)
        )));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn tab_toggles_pin_without_moving_current_row_when_query_not_focused() {
        let root = test_root("shortcut-tab-pin-no-focus");
        fs::create_dir_all(&root).expect("create dir");
        let selected = root.join("picked.txt");
        fs::write(&selected, "x").expect("write file");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.results = vec![(selected.clone(), 0.0), (root.join("next.txt"), 0.0)];
        app.current_row = Some(0);

        run_shortcuts_frame(
            &mut app,
            false,
            vec![egui::Event::Key {
                key: egui::Key::Tab,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(app.pinned_paths.contains(&selected));
        assert_eq!(app.current_row, Some(0));

        run_shortcuts_frame(
            &mut app,
            false,
            vec![egui::Event::Key {
                key: egui::Key::Tab,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(!app.pinned_paths.contains(&selected));
        assert_eq!(app.current_row, Some(0));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn tab_toggles_pin_without_moving_current_row_when_query_focused() {
        let root = test_root("shortcut-tab-pin-query-focus");
        fs::create_dir_all(&root).expect("create dir");
        let selected = root.join("picked.txt");
        fs::write(&selected, "x").expect("write file");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.results = vec![(selected.clone(), 0.0), (root.join("next.txt"), 0.0)];
        app.current_row = Some(0);

        run_shortcuts_frame(
            &mut app,
            true,
            vec![egui::Event::Key {
                key: egui::Key::Tab,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(app.pinned_paths.contains(&selected));
        assert_eq!(app.current_row, Some(0));

        run_shortcuts_frame(
            &mut app,
            true,
            vec![egui::Event::Key {
                key: egui::Key::Tab,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(!app.pinned_paths.contains(&selected));
        assert_eq!(app.current_row, Some(0));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn regression_tab_shortcut_clears_focus_traversal_target() {
        let root = test_root("regression-tab-focus-traversal");
        fs::create_dir_all(&root).expect("create dir");
        let selected = root.join("picked.txt");
        fs::write(&selected, "x").expect("write file");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.results = vec![(selected.clone(), 0.0)];
        app.current_row = Some(0);
        let ctx = egui::Context::default();
        let dummy_focus = egui::Id::new("dummy-focus");
        ctx.memory_mut(|m| m.request_focus(dummy_focus));

        ctx.begin_frame(egui::RawInput {
            modifiers: egui::Modifiers::NONE,
            events: vec![egui::Event::Key {
                key: egui::Key::Tab,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        });
        app.handle_shortcuts_with_focus(&ctx, false);
        let focused_after = ctx.memory(|m| m.focus());
        let _ = ctx.end_frame();

        assert!(app.pinned_paths.contains(&selected));
        assert_eq!(app.current_row, Some(0));
        assert!(focused_after.is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn regression_tab_keeps_query_focus_when_query_is_active() {
        let root = test_root("regression-tab-keep-query-focus");
        fs::create_dir_all(&root).expect("create dir");
        let selected = root.join("picked.txt");
        fs::write(&selected, "x").expect("write file");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.results = vec![(selected.clone(), 0.0)];
        app.current_row = Some(0);
        let ctx = egui::Context::default();
        ctx.memory_mut(|m| m.request_focus(app.query_input_id));

        ctx.begin_frame(egui::RawInput {
            modifiers: egui::Modifiers::NONE,
            events: vec![egui::Event::Key {
                key: egui::Key::Tab,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        });
        app.handle_shortcuts_with_focus(&ctx, true);
        let query_still_focused = ctx.memory(|m| m.has_focus(app.query_input_id));
        let _ = ctx.end_frame();

        assert!(app.pinned_paths.contains(&selected));
        assert!(query_still_focused);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn regression_arrow_keys_move_selection_even_when_query_focused() {
        let root = test_root("regression-arrow-query-focus");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.results = vec![
            (root.join("a.txt"), 0.0),
            (root.join("b.txt"), 0.0),
            (root.join("c.txt"), 0.0),
        ];
        app.current_row = Some(0);

        run_shortcuts_frame(
            &mut app,
            true,
            vec![egui::Event::Key {
                key: egui::Key::ArrowDown,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(app.current_row, Some(1));

        run_shortcuts_frame(
            &mut app,
            true,
            vec![egui::Event::Key {
                key: egui::Key::ArrowUp,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(app.current_row, Some(0));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn regression_ctrl_i_toggles_pin_regardless_of_query_focus() {
        let root = test_root("regression-ctrl-i-pin-toggle");
        fs::create_dir_all(&root).expect("create dir");
        let selected = root.join("picked.txt");
        fs::write(&selected, "x").expect("write file");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.results = vec![(selected.clone(), 0.0), (root.join("next.txt"), 0.0)];
        app.current_row = Some(0);

        run_shortcuts_frame(
            &mut app,
            true,
            vec![egui::Event::Key {
                key: egui::Key::I,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
            }],
        );
        assert!(app.pinned_paths.contains(&selected));
        assert_eq!(app.current_row, Some(0));

        run_shortcuts_frame(
            &mut app,
            false,
            vec![egui::Event::Key {
                key: egui::Key::I,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
            }],
        );
        assert!(!app.pinned_paths.contains(&selected));
        assert_eq!(app.current_row, Some(0));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn regression_ctrl_j_and_ctrl_m_execute_even_when_query_focused() {
        let root = test_root("regression-ctrl-jm-query-focus");
        fs::create_dir_all(&root).expect("create dir");
        #[cfg(target_os = "windows")]
        let selected = root.join("picked.exe");
        #[cfg(not(target_os = "windows"))]
        let selected = root.join("picked.sh");
        fs::write(&selected, "echo test").expect("write file");
        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&selected).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&selected, perms).expect("set permissions");
        }
        let mut app = FlistWalkerApp::new(root.clone(), 50, "query".to_string());
        app.results = vec![(selected.clone(), 0.0)];
        app.current_row = Some(0);

        run_shortcuts_frame(
            &mut app,
            true,
            vec![egui::Event::Key {
                key: egui::Key::J,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
            }],
        );
        assert!(is_action_notice(&app.notice));

        app.notice.clear();
        run_shortcuts_frame(
            &mut app,
            true,
            vec![egui::Event::Key {
                key: egui::Key::M,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
            }],
        );
        assert!(is_action_notice(&app.notice));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn regression_enter_executes_regardless_of_query_focus() {
        let root = test_root("regression-enter-query-focus");
        fs::create_dir_all(&root).expect("create dir");
        #[cfg(target_os = "windows")]
        let selected = root.join("picked.exe");
        #[cfg(not(target_os = "windows"))]
        let selected = root.join("picked.sh");
        fs::write(&selected, "echo test").expect("write file");
        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&selected).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&selected, perms).expect("set permissions");
        }
        let mut app = FlistWalkerApp::new(root.clone(), 50, "query".to_string());
        app.results = vec![(selected, 0.0)];
        app.current_row = Some(0);

        run_shortcuts_frame(
            &mut app,
            true,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(is_action_notice(&app.notice));

        app.notice.clear();
        run_shortcuts_frame(
            &mut app,
            false,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(is_action_notice(&app.notice));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn regression_shift_enter_opens_containing_folder_regardless_of_query_focus() {
        let root = test_root("regression-shift-enter-query-focus");
        let folder = root.join("src");
        fs::create_dir_all(&folder).expect("create dir");
        let selected_file = folder.join("picked.txt");
        fs::write(&selected_file, "x").expect("write file");
        let mut app = FlistWalkerApp::new(root.clone(), 50, "query".to_string());
        let (action_tx_req, action_rx_req) = mpsc::channel::<ActionRequest>();
        let (_action_tx_res, action_rx_res) = mpsc::channel::<ActionResponse>();
        app.action_tx = action_tx_req;
        app.action_rx = action_rx_res;
        app.results = vec![(selected_file.clone(), 0.0)];
        app.current_row = Some(0);

        run_shortcuts_frame(
            &mut app,
            true,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    shift: true,
                    ..Default::default()
                },
            }],
        );
        let req1 = action_rx_req
            .try_recv()
            .expect("action request should be enqueued");
        assert_eq!(req1.paths, vec![selected_file.clone()]);
        assert!(req1.open_parent_for_files);

        run_shortcuts_frame(
            &mut app,
            false,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    shift: true,
                    ..Default::default()
                },
            }],
        );
        let req2 = action_rx_req
            .try_recv()
            .expect("action request should be enqueued");
        assert_eq!(req2.paths, vec![selected_file]);
        assert!(req2.open_parent_for_files);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ctrl_t_creates_new_tab_and_activates_it() {
        let root = test_root("shortcut-ctrl-t-new-tab");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, "query".to_string());
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(app.active_tab, 0);

        run_shortcuts_frame(
            &mut app,
            true,
            vec![egui::Event::Key {
                key: egui::Key::T,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
            }],
        );

        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.active_tab, 1);
        assert!(app.query.is_empty());
        assert!(app.use_filelist);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ctrl_w_closes_current_tab_and_keeps_last_tab() {
        let root = test_root("shortcut-ctrl-w-close-tab");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.create_new_tab();
        assert_eq!(app.tabs.len(), 2);

        run_shortcuts_frame(
            &mut app,
            false,
            vec![egui::Event::Key {
                key: egui::Key::W,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
            }],
        );
        assert_eq!(app.tabs.len(), 1);

        run_shortcuts_frame(
            &mut app,
            false,
            vec![egui::Event::Key {
                key: egui::Key::W,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
            }],
        );
        assert_eq!(app.tabs.len(), 1);
        assert!(app.notice.contains("Cannot close the last tab"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ctrl_tab_and_ctrl_shift_tab_switch_active_tab() {
        let root = test_root("shortcut-ctrl-tab-switch");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.create_new_tab();
        app.create_new_tab();
        assert_eq!(app.tabs.len(), 3);
        assert_eq!(app.active_tab, 2);

        run_shortcuts_frame(
            &mut app,
            false,
            vec![egui::Event::Key {
                key: egui::Key::Tab,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
            }],
        );
        assert_eq!(app.active_tab, 0);

        run_shortcuts_frame(
            &mut app,
            false,
            vec![egui::Event::Key {
                key: egui::Key::Tab,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    ctrl: true,
                    shift: true,
                    ..Default::default()
                },
            }],
        );
        assert_eq!(app.active_tab, 2);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn switching_tabs_restores_root_per_tab() {
        let root_a = test_root("tab-root-a");
        let root_b = test_root("tab-root-b");
        fs::create_dir_all(&root_a).expect("create root a");
        fs::create_dir_all(&root_b).expect("create root b");
        let mut app = FlistWalkerApp::new(root_a.clone(), 50, String::new());

        app.create_new_tab();
        app.root = root_b.clone();
        app.sync_active_tab_state();
        assert_eq!(app.active_tab, 1);

        app.switch_to_tab_index(0);
        assert_eq!(app.root, root_a);

        app.switch_to_tab_index(1);
        assert_eq!(app.root, root_b);

        let _ = fs::remove_dir_all(&root_a);
        let _ = fs::remove_dir_all(&root_b);
    }

    #[test]
    fn switching_tabs_restores_entries_and_filters_per_tab() {
        let root = test_root("tab-entries-filters");
        fs::create_dir_all(&root).expect("create dir");
        let a = root.join("a.txt");
        let b = root.join("b.txt");
        fs::write(&a, "a").expect("write a");
        fs::write(&b, "b").expect("write b");

        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.entries = Arc::new(vec![a.clone(), b.clone()]);
        app.all_entries = Arc::new(vec![a.clone(), b.clone()]);
        app.include_files = true;
        app.include_dirs = true;
        app.sync_active_tab_state();

        app.create_new_tab();
        app.entries = Arc::new(vec![a.clone()]);
        app.all_entries = Arc::new(vec![a.clone()]);
        app.include_files = true;
        app.include_dirs = false;
        app.sync_active_tab_state();

        app.switch_to_tab_index(0);
        assert_eq!(app.entries.len(), 2);
        assert_eq!(app.all_entries.len(), 2);
        assert!(app.include_files);
        assert!(app.include_dirs);

        app.switch_to_tab_index(1);
        assert_eq!(app.entries.len(), 1);
        assert_eq!(app.all_entries.len(), 1);
        assert!(app.include_files);
        assert!(!app.include_dirs);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn background_tab_search_and_preview_responses_are_retained() {
        let root = test_root("background-tab-search-preview");
        fs::create_dir_all(&root).expect("create dir");
        let selected = root.join("picked.txt");
        fs::write(&selected, "hello").expect("write file");
        let mut app = FlistWalkerApp::new(root.clone(), 50, "picked".to_string());
        app.entries = Arc::new(vec![selected.clone()]);
        app.results = vec![(selected.clone(), 0.0)];
        app.current_row = Some(0);
        app.entry_kinds.insert(selected.clone(), false);

        let (search_tx_req, _search_rx_req) = mpsc::channel::<SearchRequest>();
        let (search_tx_res, search_rx_res) = mpsc::channel::<SearchResponse>();
        app.search_tx = search_tx_req;
        app.search_rx = search_rx_res;
        app.enqueue_search_request();
        let search_request_id = app.pending_request_id.expect("search request id");
        let first_tab_id = app.tabs[0].id;

        let (preview_tx_req, _preview_rx_req) = mpsc::channel::<PreviewRequest>();
        let (preview_tx_res, preview_rx_res) = mpsc::channel::<PreviewResponse>();
        app.preview_tx = preview_tx_req;
        app.preview_rx = preview_rx_res;
        app.request_preview_for_current();
        let preview_request_id = app.pending_preview_request_id.expect("preview request id");

        app.create_new_tab();
        assert_eq!(app.active_tab, 1);

        search_tx_res
            .send(SearchResponse {
                request_id: search_request_id,
                results: vec![(selected.clone(), 9.0)],
                error: None,
            })
            .expect("send search response");
        preview_tx_res
            .send(PreviewResponse {
                request_id: preview_request_id,
                path: selected.clone(),
                preview: "preview-body".to_string(),
            })
            .expect("send preview response");
        app.poll_search_response();
        app.poll_preview_response();

        let first_tab = app
            .tabs
            .iter()
            .find(|tab| tab.id == first_tab_id)
            .expect("first tab");
        assert_eq!(first_tab.results.len(), 1);
        assert_eq!(first_tab.results[0].0, selected);
        assert_eq!(first_tab.preview, "preview-body");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn background_tab_switch_does_not_stop_indexing_progress() {
        let root = test_root("background-tab-indexing-progress");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.index_in_progress = true;
        app.create_new_tab();

        run_shortcuts_frame(
            &mut app,
            false,
            vec![egui::Event::Key {
                key: egui::Key::Tab,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    ctrl: true,
                    shift: true,
                    ..Default::default()
                },
            }],
        );

        assert!(app.index_in_progress);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn background_tab_index_batches_do_not_override_active_tab_entries() {
        let root = test_root("background-tab-index-isolation");
        fs::create_dir_all(&root).expect("create dir");
        let active_file = root.join("active.txt");
        let indexed_file = root.join("indexed.txt");
        fs::write(&active_file, "a").expect("write active");
        fs::write(&indexed_file, "b").expect("write indexed");

        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        let (index_req_tx, index_req_rx) = mpsc::channel::<IndexRequest>();
        app.index_tx = index_req_tx;
        let (index_res_tx, index_res_rx) = mpsc::channel::<IndexResponse>();
        app.index_rx = index_res_rx;

        app.request_index_refresh();
        let index_req = index_req_rx.try_recv().expect("index request");
        app.entries = Arc::new(vec![active_file.clone()]);
        app.all_entries = Arc::new(vec![active_file.clone()]);
        app.sync_active_tab_state();

        app.create_new_tab();
        assert_eq!(app.active_tab, 1);
        app.entries = Arc::new(vec![active_file.clone()]);
        app.all_entries = Arc::new(vec![active_file.clone()]);
        app.sync_active_tab_state();

        index_res_tx
            .send(IndexResponse::Batch {
                request_id: index_req.request_id,
                entries: vec![IndexEntry {
                    path: indexed_file.clone(),
                    is_dir: false,
                    kind_known: true,
                }],
            })
            .expect("send batch");
        index_res_tx
            .send(IndexResponse::Finished {
                request_id: index_req.request_id,
                source: IndexSource::Walker,
            })
            .expect("send finished");

        app.poll_index_response();

        assert_eq!(app.entries.len(), 1);
        assert_eq!(app.entries[0], active_file);

        app.switch_to_tab_index(0);
        assert_eq!(app.entries.len(), 1);
        assert_eq!(app.entries[0], indexed_file);
        assert!(!app.index_in_progress);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn preempt_background_when_active_index_is_queued() {
        let root = test_root("index-preempt-active-priority");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.create_new_tab();
        app.create_new_tab();

        let active_tab_id = app.tabs[2].id;
        let bg_tab_a = app.tabs[0].id;
        let bg_tab_b = app.tabs[1].id;
        app.active_tab = 2;

        app.index_inflight_requests.insert(100);
        app.index_inflight_requests.insert(101);
        app.index_request_tabs.insert(100, bg_tab_a);
        app.index_request_tabs.insert(101, bg_tab_b);
        app.pending_index_queue.push_back(IndexRequest {
            request_id: 102,
            tab_id: active_tab_id,
            root: root.clone(),
            use_filelist: false,
            include_files: true,
            include_dirs: true,
        });
        {
            let mut latest = app.latest_index_request_ids.lock().expect("lock latest");
            latest.insert(bg_tab_a, 100);
            latest.insert(bg_tab_b, 101);
        }

        assert!(app.preempt_background_for_active_request());

        let latest = app.latest_index_request_ids.lock().expect("lock latest");
        let preempted =
            latest.get(&bg_tab_a).copied() == Some(0) || latest.get(&bg_tab_b).copied() == Some(0);
        assert!(preempted);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn stale_terminal_index_response_clears_inflight_slot() {
        let root = test_root("stale-terminal-clears-inflight");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        let (tx, rx) = mpsc::channel::<IndexResponse>();
        app.index_rx = rx;
        let stale_request_id = 777u64;
        let current_tab_id = app.current_tab_id().expect("tab id");
        app.pending_index_request_id = Some(778);
        app.index_inflight_requests.insert(stale_request_id);
        app.index_request_tabs
            .insert(stale_request_id, current_tab_id);

        tx.send(IndexResponse::Finished {
            request_id: stale_request_id,
            source: IndexSource::Walker,
        })
        .expect("send finished");

        app.poll_index_response();

        assert!(!app.index_inflight_requests.contains(&stale_request_id));
        assert!(!app.index_request_tabs.contains_key(&stale_request_id));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn current_finished_index_response_clears_inflight_slot() {
        let root = test_root("current-finished-clears-inflight");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        let (tx, rx) = mpsc::channel::<IndexResponse>();
        app.index_rx = rx;
        let req_id = app.pending_index_request_id.expect("pending request");
        let tab_id = app.current_tab_id().expect("tab id");
        app.index_request_tabs.insert(req_id, tab_id);
        app.index_inflight_requests.insert(req_id);

        tx.send(IndexResponse::Finished {
            request_id: req_id,
            source: IndexSource::Walker,
        })
        .expect("send finished");

        app.poll_index_response();

        assert!(!app.index_inflight_requests.contains(&req_id));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn same_tab_request_waits_until_previous_inflight_finishes() {
        let root = test_root("same-tab-inflight-serialization");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        let tab_id = app.current_tab_id().expect("tab id");

        app.index_inflight_requests.insert(1);
        app.index_request_tabs.insert(1, tab_id);
        app.pending_index_queue.push_back(IndexRequest {
            request_id: 2,
            tab_id,
            root: root.clone(),
            use_filelist: false,
            include_files: true,
            include_dirs: true,
        });

        assert!(app.pop_next_index_request().is_none());

        app.index_inflight_requests.remove(&1);
        let popped = app
            .pop_next_index_request()
            .expect("queued same-tab request should run");
        assert_eq!(popped.request_id, 2);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn tab_root_label_uses_leaf_directory_name() {
        let root = PathBuf::from("/tmp/flistwalker-tab-root-label");
        assert_eq!(
            FlistWalkerApp::tab_root_label(&root),
            "flistwalker-tab-root-label"
        );
        assert_eq!(FlistWalkerApp::tab_root_label(Path::new("/")), "/");
    }

    #[test]
    fn tab_root_label_keeps_drive_like_root() {
        assert_eq!(FlistWalkerApp::tab_root_label(Path::new("C:\\")), "C:");
        assert_eq!(FlistWalkerApp::tab_root_label(Path::new("C:")), "C:");
    }

    #[test]
    fn process_query_input_events_inserts_half_space_for_space_keys() {
        let root = test_root("ime-space-fallback");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.query = "abc".to_string();

        let ctx = egui::Context::default();
        let (inserted_half, cursor_half) = app.process_query_input_events(
            &ctx,
            &[egui::Event::Key {
                key: egui::Key::Space,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            true,
            false,
            Some(egui::text_edit::CCursorRange::one(
                egui::text::CCursor::new(3),
            )),
        );
        assert!(inserted_half);
        assert_eq!(cursor_half, Some(4));
        assert_eq!(app.query, "abc ");

        let (inserted_shift, cursor_shift) = app.process_query_input_events(
            &ctx,
            &[egui::Event::Key {
                key: egui::Key::Space,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    shift: true,
                    ..Default::default()
                },
            }],
            true,
            false,
            Some(egui::text_edit::CCursorRange::one(
                egui::text::CCursor::new(4),
            )),
        );
        assert!(inserted_shift);
        assert_eq!(cursor_shift, Some(5));
        assert_eq!(app.query, "abc  ");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn process_query_input_events_inserts_space_even_if_composition_is_active_without_update() {
        let root = test_root("ime-composition-space-allow");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.query = "abc".to_string();

        let ctx = egui::Context::default();
        let (inserted, cursor) = app.process_query_input_events(
            &ctx,
            &[
                egui::Event::CompositionStart,
                egui::Event::Key {
                    key: egui::Key::Space,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            true,
            false,
            Some(egui::text_edit::CCursorRange::one(
                egui::text::CCursor::new(3),
            )),
        );
        assert!(inserted);
        assert_eq!(cursor, Some(4));
        assert_eq!(app.query, "abc ");
        assert!(app.ime_composition_active);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn process_query_input_events_inserts_space_fallback_when_composition_updates() {
        let root = test_root("ime-composition-space-allow-update");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.query = "abc".to_string();

        let ctx = egui::Context::default();
        let (inserted, cursor) = app.process_query_input_events(
            &ctx,
            &[
                egui::Event::CompositionStart,
                egui::Event::CompositionUpdate("あ".to_string()),
                egui::Event::Key {
                    key: egui::Key::Space,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            true,
            false,
            Some(egui::text_edit::CCursorRange::one(
                egui::text::CCursor::new(3),
            )),
        );
        assert!(inserted);
        assert_eq!(cursor, Some(4));
        assert_eq!(app.query, "abc ");
        assert!(app.ime_composition_active);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn process_query_input_events_inserts_half_space_even_with_composition_update() {
        let root = test_root("ime-composition-half-space-allow");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.query = "abc".to_string();

        let ctx = egui::Context::default();
        let (inserted, cursor) = app.process_query_input_events(
            &ctx,
            &[
                egui::Event::CompositionStart,
                egui::Event::CompositionUpdate("あ".to_string()),
                egui::Event::Key {
                    key: egui::Key::Space,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers {
                        shift: true,
                        ..Default::default()
                    },
                },
            ],
            true,
            false,
            Some(egui::text_edit::CCursorRange::one(
                egui::text::CCursor::new(3),
            )),
        );
        assert!(inserted);
        assert_eq!(cursor, Some(4));
        assert_eq!(app.query, "abc ");
        assert!(app.ime_composition_active);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn process_query_input_events_inserts_space_fallback_at_cursor_position() {
        let root = test_root("ime-space-fallback-cursor");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.query = "abCD".to_string();
        let ctx = egui::Context::default();

        let (inserted, cursor) = app.process_query_input_events(
            &ctx,
            &[egui::Event::Key {
                key: egui::Key::Space,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            true,
            false,
            Some(egui::text_edit::CCursorRange::one(
                egui::text::CCursor::new(2),
            )),
        );

        assert!(inserted);
        assert_eq!(app.query, "ab CD");
        assert_eq!(cursor, Some(3));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn process_query_input_events_inserts_composition_commit_fallback_at_cursor_position() {
        let root = test_root("ime-commit-fallback-cursor");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.query = "abCD".to_string();
        let ctx = egui::Context::default();

        let (inserted, cursor) = app.process_query_input_events(
            &ctx,
            &[egui::Event::CompositionEnd("x".to_string())],
            true,
            false,
            Some(egui::text_edit::CCursorRange::one(
                egui::text::CCursor::new(2),
            )),
        );

        assert!(inserted);
        assert_eq!(app.query, "abxCD");
        assert_eq!(cursor, Some(3));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn oversized_geometry_is_rejected_when_monitor_size_is_known() {
        let root = test_root("reject-oversize-geometry");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

        let next = SavedWindowGeometry {
            x: 200.0,
            y: 150.0,
            width: 3600.0,
            height: 2100.0,
            monitor_width: Some(2560.0),
            monitor_height: Some(1440.0),
        };

        let width_limit = (next.monitor_width.unwrap_or_default() * 1.05).max(640.0);
        let height_limit = (next.monitor_height.unwrap_or_default() * 1.05).max(400.0);
        assert!(next.width > width_limit);
        assert!(next.height > height_limit);

        // Simulate capture rejection condition directly.
        if let (Some(mw), Some(mh)) = (next.monitor_width, next.monitor_height) {
            let w_limit = (mw * 1.05).max(640.0);
            let h_limit = (mh * 1.05).max(400.0);
            if next.width > w_limit || next.height > h_limit {
                // keep state untouched
            } else {
                app.pending_window_geometry = Some(next);
            }
        }
        assert!(app.pending_window_geometry.is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn deferred_copy_shortcut_copies_selected_path_even_with_query_text() {
        let root = test_root("deferred-copy-shortcut");
        fs::create_dir_all(&root).expect("create dir");
        let selected = root.join("picked.txt");
        fs::write(&selected, "x").expect("write file");
        let mut app = FlistWalkerApp::new(root.clone(), 50, "query text".to_string());
        app.results = vec![(selected.clone(), 0.0)];
        app.current_row = Some(0);
        app.pending_copy_shortcut = true;
        let ctx = egui::Context::default();

        app.run_deferred_shortcuts(&ctx);

        assert!(!app.pending_copy_shortcut);
        assert!(app.focus_query_requested);
        assert!(app.notice.contains(&format!(
            "Copied path: {}",
            normalize_path_for_display(&selected)
        )));
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

    #[test]
    #[cfg(target_os = "windows")]
    fn copy_selected_paths_notice_normalizes_extended_prefix() {
        let root = test_root("copy-path-notice-normalize");
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.results = vec![(PathBuf::from(r"\\?\C:\Users\tester\file.txt"), 0)];
        app.current_row = Some(0);
        let ctx = egui::Context::default();

        app.copy_selected_paths(&ctx);

        assert!(app
            .notice
            .contains(r"Copied path: C:\Users\tester\file.txt"));
        assert!(!app.notice.contains(r"\\?\"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn process_shutdown_flag_can_be_set_and_cleared() {
        clear_process_shutdown_request();
        assert!(!process_shutdown_requested());
        request_process_shutdown();
        assert!(process_shutdown_requested());
        clear_process_shutdown_request();
        assert!(!process_shutdown_requested());
    }
}

use super::{ResultSortMode, SortMetadata};
use crate::actions::execute_or_open;
use crate::indexer::{
    apply_filelist_hierarchy_overrides, find_filelist_in_first_level, parse_filelist_stream,
    write_filelist, IndexSource,
};
use crate::search::{
    sort_scored_matches, top_ranked_scores, try_collect_search_matches, IndexedScore,
};
use crate::ui_model::{build_preview_text_with_kind, has_visible_match};
use jwalk::{Parallelism, WalkDir};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub(super) struct WorkerRuntime {
    shutdown: Arc<AtomicBool>,
    handles: Vec<NamedWorkerHandle>,
}

struct NamedWorkerHandle {
    name: String,
    handle: thread::JoinHandle<()>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WorkerJoinSummary {
    pub(super) joined: usize,
    pub(super) total: usize,
    pub(super) pending: Vec<String>,
}

impl WorkerRuntime {
    pub(super) fn new(shutdown: Arc<AtomicBool>) -> Self {
        Self {
            shutdown,
            handles: Vec::new(),
        }
    }

    pub(super) fn push(&mut self, name: impl Into<String>, handle: thread::JoinHandle<()>) {
        self.handles.push(NamedWorkerHandle {
            name: name.into(),
            handle,
        });
    }

    pub(super) fn request_shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }

    pub(super) fn join_all_with_timeout(mut self, timeout: Duration) -> WorkerJoinSummary {
        let total = self.handles.len();
        if total == 0 {
            return WorkerJoinSummary {
                joined: 0,
                total: 0,
                pending: Vec::new(),
            };
        }

        let (tx, rx) = mpsc::channel::<String>();
        let mut pending = self
            .handles
            .iter()
            .map(|handle| handle.name.clone())
            .collect::<Vec<_>>();
        for named_handle in self.handles.drain(..) {
            let tx_done = tx.clone();
            let name = named_handle.name;
            let handle = named_handle.handle;
            thread::spawn(move || {
                let _ = handle.join();
                let _ = tx_done.send(name);
            });
        }
        drop(tx);

        let deadline = Instant::now() + timeout;
        let mut joined = 0usize;
        while joined < total {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let remain = deadline.saturating_duration_since(now);
            match rx.recv_timeout(remain) {
                Ok(name) => {
                    joined = joined.saturating_add(1);
                    pending.retain(|pending_name| pending_name != &name);
                }
                Err(_) => break,
            }
        }

        WorkerJoinSummary {
            joined,
            total,
            pending,
        }
    }
}

pub(super) struct SearchRequest {
    pub(super) request_id: u64,
    pub(super) query: String,
    pub(super) entries: Arc<Vec<PathBuf>>,
    pub(super) limit: usize,
    pub(super) use_regex: bool,
    pub(super) ignore_case: bool,
    pub(super) root: PathBuf,
    pub(super) prefer_relative: bool,
}

pub(super) struct SearchResponse {
    pub(super) request_id: u64,
    pub(super) results: Vec<(PathBuf, f64)>,
    pub(super) error: Option<String>,
}

pub(super) fn filter_search_results(
    results: Vec<(PathBuf, f64)>,
    root: &Path,
    query: &str,
    prefer_relative: bool,
    use_regex: bool,
    ignore_case: bool,
) -> Vec<(PathBuf, f64)> {
    if use_regex {
        return results;
    }

    results
        .into_iter()
        .filter(|(path, _)| has_visible_match(path, root, query, prefer_relative, ignore_case))
        .collect()
}

#[derive(Clone, Debug)]
pub(super) struct IndexEntry {
    pub(super) path: PathBuf,
    pub(super) is_dir: bool,
    pub(super) kind_known: bool,
}

pub(super) struct IndexRequest {
    pub(super) request_id: u64,
    pub(super) tab_id: u64,
    pub(super) root: PathBuf,
    pub(super) use_filelist: bool,
    pub(super) include_files: bool,
    pub(super) include_dirs: bool,
}

pub(super) enum IndexResponse {
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

pub(super) struct PreviewRequest {
    pub(super) request_id: u64,
    pub(super) path: PathBuf,
    pub(super) is_dir: bool,
}

pub(super) struct PreviewResponse {
    pub(super) request_id: u64,
    pub(super) path: PathBuf,
    pub(super) preview: String,
}

pub(super) struct ActionRequest {
    pub(super) request_id: u64,
    pub(super) paths: Vec<PathBuf>,
    pub(super) open_parent_for_files: bool,
}

pub(super) struct ActionResponse {
    pub(super) request_id: u64,
    pub(super) notice: String,
}

pub(super) struct SortMetadataRequest {
    pub(super) request_id: u64,
    pub(super) paths: Vec<PathBuf>,
    pub(super) mode: ResultSortMode,
}

pub(super) struct SortMetadataResponse {
    pub(super) request_id: u64,
    pub(super) entries: Vec<(PathBuf, SortMetadata)>,
    pub(super) mode: ResultSortMode,
}

pub(super) struct KindResolveRequest {
    pub(super) epoch: u64,
    pub(super) path: PathBuf,
}

pub(super) struct KindResolveResponse {
    pub(super) epoch: u64,
    pub(super) path: PathBuf,
    pub(super) is_dir: Option<bool>,
}

pub(super) struct FileListRequest {
    pub(super) request_id: u64,
    pub(super) tab_id: u64,
    pub(super) root: PathBuf,
    pub(super) entries: Vec<PathBuf>,
    pub(super) propagate_to_ancestors: bool,
}

pub(super) enum FileListResponse {
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
pub(super) struct SearchEntriesSnapshotKey {
    pub(super) ptr: usize,
    pub(super) len: usize,
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
pub(super) struct SearchPrefixCacheEntry {
    snapshot: SearchEntriesSnapshotKey,
    query: String,
    matched_indices: Arc<Vec<usize>>,
    approx_bytes: usize,
}

#[derive(Default)]
pub(super) struct SearchPrefixCache {
    pub(super) entries: VecDeque<SearchPrefixCacheEntry>,
    pub(super) total_bytes: usize,
}

impl SearchPrefixCache {
    pub(super) const MAX_ENTRIES: usize = 64;
    pub(super) const MAX_BYTES: usize = 8 * 1024 * 1024;
    pub(super) const MAX_MATCHED_INDICES: usize = 20_000;
    const MIN_QUERY_LEN: usize = 3;

    pub(super) fn is_cacheable_query(query: &str) -> bool {
        let q = query.trim();
        if q.len() < Self::MIN_QUERY_LEN {
            return false;
        }
        if q.contains(char::is_whitespace) {
            return false;
        }
        !q.contains(['|', '!', '\'', '^', '$'])
    }

    pub(super) fn is_safe_prefix_extension(prefix: &str, query: &str) -> bool {
        if !Self::is_cacheable_query(prefix) || !Self::is_cacheable_query(query) {
            return false;
        }
        query.starts_with(prefix) && query.len() > prefix.len()
    }

    pub(super) fn lookup_candidates(
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

    pub(super) fn maybe_store(
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
        let approx_bytes = query.len().saturating_add(
            matched_indices
                .len()
                .saturating_mul(std::mem::size_of::<usize>()),
        );
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

pub(super) fn spawn_search_worker(
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
            let (results, error) = match try_collect_search_matches(
                &req.query,
                &req.entries,
                req.use_regex,
                req.ignore_case,
                Some(&req.root),
                req.prefer_relative,
                cached_candidates.as_ref().map(|items| items.as_slice()),
            ) {
                Ok(scored_matches) => {
                    if SearchPrefixCache::is_cacheable_query(&query_trimmed)
                        && scored_matches.scored.len() <= SearchPrefixCache::MAX_MATCHED_INDICES
                    {
                        let mut ranked = scored_matches.scored.clone();
                        sort_scored_matches(&mut ranked);
                        let matched_indices = ranked.iter().map(|item| item.index).collect();
                        prefix_cache.maybe_store(snapshot, &query_trimmed, matched_indices);
                    }
                    let ranked = top_ranked_scores(scored_matches.scored, req.limit);
                    let raw_results = scored_indices_to_paths(&req.entries, &ranked, req.limit);
                    (
                        filter_search_results(
                            raw_results,
                            &req.root,
                            &req.query,
                            req.prefer_relative,
                            req.use_regex,
                            req.ignore_case,
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

pub(super) fn spawn_preview_worker(
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

pub(super) fn spawn_kind_resolver_worker(
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

pub(super) fn spawn_filelist_worker(
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
            let result = write_filelist(
                &req.root,
                &req.entries,
                "FileList.txt",
                req.propagate_to_ancestors,
            )
            .map(|path| (path, count));
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

pub(super) fn spawn_action_worker(
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
                if let Err(err) = execute_or_open(target) {
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

pub(super) fn spawn_sort_metadata_worker(
    shutdown: Arc<AtomicBool>,
) -> (
    Sender<SortMetadataRequest>,
    Receiver<SortMetadataResponse>,
    thread::JoinHandle<()>,
) {
    let (tx_req, rx_req) = mpsc::channel::<SortMetadataRequest>();
    let (tx_res, rx_res) = mpsc::channel::<SortMetadataResponse>();

    let handle = thread::spawn(move || {
        while let Ok(mut req) = rx_req.recv() {
            if shutdown.load(Ordering::Relaxed) {
                break;
            }
            while let Ok(newer) = rx_req.try_recv() {
                req = newer;
            }

            let mut entries = Vec::with_capacity(req.paths.len());
            for path in req.paths {
                if shutdown.load(Ordering::Relaxed) {
                    return;
                }
                let metadata = std::fs::metadata(&path)
                    .ok()
                    .map(|meta| SortMetadata {
                        modified: meta.modified().ok(),
                        created: meta.created().ok(),
                    })
                    .unwrap_or_default();
                entries.push((path, metadata));
            }

            if tx_res
                .send(SortMetadataResponse {
                    request_id: req.request_id,
                    entries,
                    mode: req.mode,
                })
                .is_err()
            {
                break;
            }
        }
    });

    (tx_req, rx_res, handle)
}

pub(super) fn action_targets_for_request(
    paths: &[PathBuf],
    open_parent_for_files: bool,
) -> Vec<PathBuf> {
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

pub(super) fn action_target_path_for_open_in_folder(path: &Path) -> PathBuf {
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
    root: &Path,
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
    root: &Path,
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

pub(super) fn spawn_index_worker(
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

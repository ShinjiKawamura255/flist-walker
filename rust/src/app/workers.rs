use super::worker_support::{action_notice_for_targets, action_targets_for_request};
use super::{ResultSortMode, SortMetadata};
#[cfg(not(test))]
use crate::actions::execute_or_open;
use crate::entry::{Entry, EntryKind};
use crate::indexer::{
    apply_filelist_hierarchy_overrides, find_filelist_in_first_level, parse_filelist_stream,
    write_filelist_cancellable, IndexSource,
};
use crate::search::{rank_search_results, SearchPrefixCache};
use crate::ui_model::build_preview_text_with_kind;
use crate::updater::{check_for_update, prepare_and_start_update, UpdateCandidate};
use jwalk::{Parallelism, WalkDir};
use std::collections::HashMap;
use std::fs::FileType;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

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
    pub(super) entries: Arc<Vec<Entry>>,
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

#[derive(Clone, Debug)]
pub(super) struct IndexEntry {
    pub(super) path: PathBuf,
    pub(super) kind: EntryKind,
    pub(super) kind_known: bool,
}

impl From<IndexEntry> for Entry {
    fn from(value: IndexEntry) -> Self {
        Self::new(value.path, value.kind_known.then_some(value.kind))
    }
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

pub(super) enum UpdateRequestKind {
    Check,
    DownloadAndApply {
        candidate: Box<UpdateCandidate>,
        current_exe: PathBuf,
    },
}

pub(super) struct UpdateRequest {
    pub(super) request_id: u64,
    pub(super) kind: UpdateRequestKind,
}

pub(super) enum UpdateResponse {
    UpToDate {
        request_id: u64,
    },
    CheckFailed {
        request_id: u64,
        error: String,
    },
    Available {
        request_id: u64,
        candidate: Box<UpdateCandidate>,
    },
    ApplyStarted {
        request_id: u64,
        target_version: String,
    },
    Failed {
        request_id: u64,
        error: String,
    },
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
    pub(super) kind: Option<EntryKind>,
}

fn is_windows_shortcut(path: &Path) -> bool {
    #[cfg(windows)]
    {
        return path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("lnk"));
    }

    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}

fn resolve_entry_kind(path: &Path) -> Option<EntryKind> {
    let symlink_meta = std::fs::symlink_metadata(path).ok()?;
    let is_link = symlink_meta.file_type().is_symlink() || is_windows_shortcut(path);

    if symlink_meta.is_dir() {
        return Some(if is_link {
            EntryKind::link(true)
        } else {
            EntryKind::dir()
        });
    }
    if symlink_meta.is_file() {
        return Some(if is_link {
            EntryKind::link(false)
        } else {
            EntryKind::file()
        });
    }

    let meta = std::fs::metadata(path).ok()?;
    if meta.is_dir() {
        Some(if is_link {
            EntryKind::link(true)
        } else {
            EntryKind::dir()
        })
    } else if meta.is_file() {
        Some(if is_link {
            EntryKind::link(false)
        } else {
            EntryKind::file()
        })
    } else {
        None
    }
}

fn classify_walker_entry(
    path: &Path,
    file_type: FileType,
    include_files: bool,
    include_dirs: bool,
) -> Option<(EntryKind, bool)> {
    if file_type.is_dir() {
        return include_dirs.then_some((EntryKind::dir(), true));
    }

    if file_type.is_file() && !is_windows_shortcut(path) {
        return include_files.then_some((EntryKind::file(), true));
    }

    if include_files && include_dirs {
        // Defer expensive metadata/link resolution until after initial indexing so Walker can
        // finish streaming candidates as quickly as possible.
        return Some((EntryKind::file(), false));
    }

    let kind = resolve_entry_kind(path)?;
    if (kind.is_dir && include_dirs) || (!kind.is_dir && include_files) {
        Some((kind, true))
    } else {
        None
    }
}

pub(super) struct FileListRequest {
    pub(super) request_id: u64,
    pub(super) tab_id: u64,
    pub(super) root: PathBuf,
    pub(super) entries: Vec<PathBuf>,
    pub(super) propagate_to_ancestors: bool,
    pub(super) cancel: Arc<AtomicBool>,
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
    Canceled {
        request_id: u64,
        root: PathBuf,
    },
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
            let (results, error) = rank_search_results(
                &req.entries,
                &req.query,
                &req.root,
                req.limit,
                req.use_regex,
                req.ignore_case,
                req.prefer_relative,
                &mut prefix_cache,
            );

            if tx_res
                .send(SearchResponse {
                    request_id: req.request_id,
                    results,
                    error,
                })
                .is_err()
            {
                warn!(request_id = req.request_id, "search worker receiver closed");
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
                warn!(
                    request_id = req.request_id,
                    "preview worker receiver closed"
                );
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
            let kind = resolve_entry_kind(&req.path);
            if tx_res
                .send(KindResolveResponse {
                    epoch: req.epoch,
                    path: req.path,
                    kind,
                })
                .is_err()
            {
                warn!(epoch = req.epoch, "kind resolver receiver closed");
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
            if req.cancel.load(Ordering::Relaxed) {
                if tx_res
                    .send(FileListResponse::Canceled {
                        request_id: req.request_id,
                        root: req.root.clone(),
                    })
                    .is_err()
                {
                    break;
                }
                continue;
            }
            let _tab_id = req.tab_id;
            let count = req.entries.len();
            let result = write_filelist_cancellable(
                &req.root,
                &req.entries,
                "FileList.txt",
                req.propagate_to_ancestors,
                &|| shutdown.load(Ordering::Relaxed) || req.cancel.load(Ordering::Relaxed),
            )
            .map(|path| (path, count));
            let msg = match result {
                Ok((path, count)) => FileListResponse::Finished {
                    request_id: req.request_id,
                    root: req.root.clone(),
                    path,
                    count,
                },
                Err(err) => {
                    if req.cancel.load(Ordering::Relaxed) || shutdown.load(Ordering::Relaxed) {
                        FileListResponse::Canceled {
                            request_id: req.request_id,
                            root: req.root.clone(),
                        }
                    } else {
                        FileListResponse::Failed {
                            request_id: req.request_id,
                            root: req.root.clone(),
                            error: err.to_string(),
                        }
                    }
                }
            };
            if tx_res.send(msg).is_err() {
                warn!(
                    request_id = req.request_id,
                    "filelist worker receiver closed"
                );
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
                if let Err(err) = run_action_target(target) {
                    failure = Some(format!("Action failed: {}", err));
                    break;
                }
            }

            let notice = if let Some(failed) = failure {
                failed
            } else {
                action_notice_for_targets(&targets)
            };

            if tx_res
                .send(ActionResponse {
                    request_id: req.request_id,
                    notice,
                })
                .is_err()
            {
                warn!(request_id = req.request_id, "action worker receiver closed");
                break;
            }
        }
    });

    (tx_req, rx_res, handle)
}

#[cfg(not(test))]
fn run_action_target(path: &Path) -> anyhow::Result<()> {
    execute_or_open(path)
}

#[cfg(test)]
fn run_action_target(_path: &Path) -> anyhow::Result<()> {
    // GUI shortcut / action worker tests only need request/notice behavior.
    // Avoid spawning xdg-open/open during test runs so stderr stays clean.
    Ok(())
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
                warn!(request_id = req.request_id, "sort metadata receiver closed");
                break;
            }
        }
    });

    (tx_req, rx_res, handle)
}

pub(super) fn spawn_update_worker(
    shutdown: Arc<AtomicBool>,
) -> (
    Sender<UpdateRequest>,
    Receiver<UpdateResponse>,
    thread::JoinHandle<()>,
) {
    let (tx_req, rx_req) = mpsc::channel::<UpdateRequest>();
    let (tx_res, rx_res) = mpsc::channel::<UpdateResponse>();

    let handle = thread::spawn(move || {
        while let Ok(req) = rx_req.recv() {
            if shutdown.load(Ordering::Relaxed) {
                break;
            }

            let response = match req.kind {
                UpdateRequestKind::Check => match check_for_update() {
                    Ok(Some(candidate)) => UpdateResponse::Available {
                        request_id: req.request_id,
                        candidate: Box::new(candidate),
                    },
                    Ok(None) => UpdateResponse::UpToDate {
                        request_id: req.request_id,
                    },
                    Err(err) => UpdateResponse::CheckFailed {
                        request_id: req.request_id,
                        error: format!("Update check failed: {err}"),
                    },
                },
                UpdateRequestKind::DownloadAndApply {
                    candidate,
                    current_exe,
                } => match prepare_and_start_update(candidate.as_ref(), &current_exe) {
                    Ok(()) => UpdateResponse::ApplyStarted {
                        request_id: req.request_id,
                        target_version: candidate.target_version.clone(),
                    },
                    Err(err) => UpdateResponse::Failed {
                        request_id: req.request_id,
                        error: format!("Update failed: {err}"),
                    },
                },
            };

            if tx_res.send(response).is_err() {
                warn!(request_id = req.request_id, "update worker receiver closed");
                break;
            }
        }
    });

    (tx_req, rx_res, handle)
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
    info!(
        request_id = req.request_id,
        tab_id = req.tab_id,
        root = %root.display(),
        filelist = %filelist.display(),
        "index worker started filelist stream"
    );
    if tx_res
        .send(IndexResponse::Started {
            request_id: req.request_id,
            source: source.clone(),
        })
        .is_err()
    {
        warn!(
            request_id = req.request_id,
            "index receiver closed before filelist start"
        );
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
                kind: is_dir.map_or_else(EntryKind::file, |is_dir| {
                    if is_dir {
                        EntryKind::dir()
                    } else {
                        EntryKind::file()
                    }
                }),
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
                kind: EntryKind::file(),
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
            warn!(
                request_id = req.request_id,
                "index receiver closed during filelist replace"
            );
            return Err("index receiver closed".to_string());
        }
    }
    debug!(
        request_id = req.request_id,
        source = ?source,
        "index worker finished filelist stream"
    );
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
    info!(
        request_id = req.request_id,
        tab_id = req.tab_id,
        root = %root.display(),
        include_files = req.include_files,
        include_dirs = req.include_dirs,
        "index worker started walker stream"
    );
    if tx_res
        .send(IndexResponse::Started {
            request_id: req.request_id,
            source: source.clone(),
        })
        .is_err()
    {
        warn!(
            request_id = req.request_id,
            "index receiver closed before walker start"
        );
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
        let path = entry.path().to_path_buf();
        let Some((kind, kind_known)) = classify_walker_entry(
            &path,
            entry.file_type(),
            req.include_files,
            req.include_dirs,
        ) else {
            continue;
        };
        if emitted_entries >= max_entries {
            truncated = true;
            break;
        }
        buffer.push(IndexEntry {
            path,
            kind,
            kind_known,
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
        warn!(
            request_id = req.request_id,
            "index receiver closed during truncation notice"
        );
        return Err("index receiver closed".to_string());
    }
    debug!(
        request_id = req.request_id,
        emitted_entries, truncated, "index worker finished walker stream"
    );
    Ok(source)
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("flistwalker-workers-{name}-{nonce}"))
    }

    #[test]
    fn classify_walker_entry_keeps_regular_file_fast_path_known() {
        let root = test_root("file");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create dir");
        let path = root.join("main.rs");
        std::fs::write(&path, "fn main() {}").expect("write file");
        let file_type = std::fs::symlink_metadata(&path)
            .expect("metadata")
            .file_type();

        let classified =
            classify_walker_entry(&path, file_type, true, true).expect("classify walker entry");

        assert_eq!(classified, (EntryKind::file(), true));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn classify_walker_entry_defers_windows_shortcut_when_both_filters_enabled() {
        let root = test_root("shortcut");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create dir");
        let path = root.join("app.lnk");
        std::fs::write(&path, "shortcut").expect("write file");
        let file_type = std::fs::symlink_metadata(&path)
            .expect("metadata")
            .file_type();

        let classified =
            classify_walker_entry(&path, file_type, true, true).expect("classify walker entry");

        #[cfg(windows)]
        assert_eq!(classified, (EntryKind::file(), false));
        #[cfg(not(windows))]
        assert_eq!(classified, (EntryKind::file(), true));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    #[ignore = "perf measurement; run explicitly"]
    fn perf_walker_classification_is_faster_than_eager_metadata_resolution() {
        let root = test_root("perf");
        let _ = std::fs::remove_dir_all(&root);
        let dataset = root.join("dataset");
        std::fs::create_dir_all(&dataset).expect("create dataset");
        for i in 0..20_000usize {
            let dir = dataset.join(format!("dir-{i}"));
            std::fs::create_dir_all(&dir).expect("create dir");
            std::fs::write(dir.join("main.rs"), "fn main() {}").expect("write file");
        }

        let mut eager_best = Duration::MAX;
        let mut fast_best = Duration::MAX;
        let iterations = 3usize;
        let mut eager_count = 0usize;
        let mut fast_count = 0usize;

        for _ in 0..iterations {
            let eager_start = Instant::now();
            eager_count = 0;
            for entry in WalkDir::new(&root)
                .parallelism(Parallelism::Serial)
                .skip_hidden(false)
                .follow_links(false)
                .min_depth(1)
                .into_iter()
                .flatten()
            {
                if resolve_entry_kind(&entry.path()).is_some() {
                    eager_count = eager_count.saturating_add(1);
                }
            }
            eager_best = eager_best.min(eager_start.elapsed());

            let fast_start = Instant::now();
            fast_count = 0;
            for entry in WalkDir::new(&root)
                .parallelism(Parallelism::Serial)
                .skip_hidden(false)
                .follow_links(false)
                .min_depth(1)
                .into_iter()
                .flatten()
            {
                if classify_walker_entry(&entry.path(), entry.file_type(), true, true).is_some() {
                    fast_count = fast_count.saturating_add(1);
                }
            }
            fast_best = fast_best.min(fast_start.elapsed());
        }

        let eager_ms = eager_best.as_secs_f64() * 1000.0;
        let fast_ms = fast_best.as_secs_f64() * 1000.0;
        let speedup = if fast_ms > 0.0 {
            eager_ms / fast_ms
        } else {
            f64::INFINITY
        };

        eprintln!(
            "Walker perf eager_metadata_ms={eager_ms:.3} fast_classify_ms={fast_ms:.3} speedup={speedup:.2}x eager_count={eager_count} fast_count={fast_count}"
        );

        assert_eq!(eager_count, fast_count);
        assert!(
            speedup >= 1.20,
            "walker fast classification did not beat eager metadata enough: {speedup:.2}x"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
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
                    warn!(
                        request_id = req.request_id,
                        "index receiver closed before empty start"
                    );
                    break;
                }
                if tx_res_worker
                    .send(IndexResponse::Finished {
                        request_id: req.request_id,
                        source: IndexSource::None,
                    })
                    .is_err()
                {
                    warn!(
                        request_id = req.request_id,
                        "index receiver closed before empty finish"
                    );
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
                        warn!(
                            request_id = req.request_id,
                            "index receiver closed before finish"
                        );
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
                        warn!(
                            request_id = req.request_id,
                            "index receiver closed before failure"
                        );
                        break;
                    }
                }
            }
        });
        handles.push(handle);
    }

    (tx_req, rx_res, handles)
}

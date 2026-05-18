use super::adaptive_walker::{walk_adaptive, AdaptiveWalkerEntry, AdaptiveWalkerMetrics};
use super::worker_protocol::{IndexEntry, IndexRequest, IndexResponse};
use crate::entry::EntryKind;
use crate::indexer::{
    apply_filelist_hierarchy_overrides, find_filelist_in_first_level, parse_filelist_stream,
    IndexSource,
};
use crate::runtime_config::{current_runtime_config, RuntimeConfig};
use jwalk::{Parallelism, WalkDir};
use std::collections::HashMap;
use std::fs::FileType;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{info, warn};

const WALKER_THREADS_MAX: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WalkerBackend {
    Jwalk,
    Adaptive,
}

#[derive(Debug)]
struct WalkerRuntimeSettings {
    max_entries: usize,
    threads: usize,
    adaptive_initial_limit: usize,
    adaptive_max_limit: usize,
    backend: WalkerBackend,
    metrics_enabled: bool,
    metrics_log_path: String,
}

fn walker_runtime_settings(config: &RuntimeConfig) -> WalkerRuntimeSettings {
    let backend = if config
        .developer
        .walker_backend
        .trim()
        .eq_ignore_ascii_case("adaptive")
    {
        WalkerBackend::Adaptive
    } else {
        WalkerBackend::Jwalk
    };

    let threads = config.walker_threads.max(1).min(WALKER_THREADS_MAX);
    let adaptive_max_limit = config
        .developer
        .walker_adaptive_max_limit
        .unwrap_or(threads)
        .max(1)
        .min(WALKER_THREADS_MAX);
    let adaptive_initial_limit = config
        .developer
        .walker_adaptive_initial_limit
        .unwrap_or(adaptive_max_limit.min(2))
        .max(1)
        .min(adaptive_max_limit);

    WalkerRuntimeSettings {
        max_entries: config.walker_max_entries.max(1),
        threads,
        adaptive_initial_limit,
        adaptive_max_limit,
        backend,
        metrics_enabled: config.developer.walker_metrics,
        metrics_log_path: config.developer.walker_metrics_log_path.clone(),
    }
}

fn walker_parallelism(threads: usize) -> Parallelism {
    if threads <= 1 {
        Parallelism::Serial
    } else {
        Parallelism::RayonNewPool(threads)
    }
}

fn is_windows_shortcut(path: &Path) -> bool {
    #[cfg(windows)]
    {
        path.extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("lnk"))
    }

    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}

pub(super) fn resolve_entry_kind(path: &Path) -> Option<EntryKind> {
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

#[derive(Debug)]
struct WalkerMetrics {
    backend: WalkerBackend,
    started_at: Instant,
    entries_emitted: usize,
    batches_sent: usize,
    dirs_read: usize,
    read_dir_errors: usize,
    max_inflight_read_dirs: usize,
    throttle_events: usize,
    adaptive_limit_min: usize,
    adaptive_limit_max: usize,
    adaptive_limit_final: usize,
    read_dir_total_us: u128,
    read_dir_max_us: u128,
}

impl WalkerMetrics {
    fn new(backend: WalkerBackend, threads: usize) -> Self {
        Self {
            backend,
            started_at: Instant::now(),
            entries_emitted: 0,
            batches_sent: 0,
            dirs_read: 0,
            read_dir_errors: 0,
            max_inflight_read_dirs: if matches!(backend, WalkerBackend::Jwalk) {
                threads
            } else {
                0
            },
            throttle_events: 0,
            adaptive_limit_min: if matches!(backend, WalkerBackend::Jwalk) {
                threads
            } else {
                0
            },
            adaptive_limit_max: if matches!(backend, WalkerBackend::Jwalk) {
                threads
            } else {
                0
            },
            adaptive_limit_final: if matches!(backend, WalkerBackend::Jwalk) {
                threads
            } else {
                0
            },
            read_dir_total_us: 0,
            read_dir_max_us: 0,
        }
    }

    fn record_batch(&mut self) {
        self.batches_sent = self.batches_sent.saturating_add(1);
    }

    fn record_adaptive(&mut self, metrics: AdaptiveWalkerMetrics) {
        self.dirs_read = metrics.dirs_read;
        self.read_dir_errors = metrics.read_dir_errors;
        self.max_inflight_read_dirs = metrics.max_inflight_read_dirs;
        self.throttle_events = metrics.throttle_events;
        self.adaptive_limit_min = metrics.adaptive_limit_min;
        self.adaptive_limit_max = metrics.adaptive_limit_max;
        self.adaptive_limit_final = metrics.adaptive_limit_final;
        self.read_dir_total_us = metrics.read_dir_total_us;
        self.read_dir_max_us = metrics.read_dir_max_us;
    }

    fn read_dir_avg_us(&self) -> u128 {
        if self.dirs_read == 0 {
            0
        } else {
            self.read_dir_total_us / self.dirs_read as u128
        }
    }
}

fn walker_backend_label(backend: WalkerBackend) -> &'static str {
    match backend {
        WalkerBackend::Jwalk => "jwalk",
        WalkerBackend::Adaptive => "adaptive",
    }
}

fn log_walker_metrics(req: &IndexRequest, metrics: &WalkerMetrics, outcome: &str, path: &str) {
    let summary = walker_metrics_summary(req, metrics, outcome);
    info!(
        flow = "index",
        source_kind = "walker",
        event = "metrics",
        request_id = req.request_id,
        tab_id = req.tab_id,
        backend = walker_backend_label(metrics.backend),
        outcome,
        elapsed_ms = metrics.started_at.elapsed().as_millis(),
        entries_emitted = metrics.entries_emitted,
        batches_sent = metrics.batches_sent,
        dirs_read = metrics.dirs_read,
        read_dir_errors = metrics.read_dir_errors,
        max_inflight_read_dirs = metrics.max_inflight_read_dirs,
        throttle_events = metrics.throttle_events,
        adaptive_limit_min = metrics.adaptive_limit_min,
        adaptive_limit_max = metrics.adaptive_limit_max,
        adaptive_limit_final = metrics.adaptive_limit_final,
        read_dir_avg_us = metrics.read_dir_avg_us(),
        read_dir_max_us = metrics.read_dir_max_us,
        "walker metrics summary"
    );
    write_walker_metrics_summary(&summary, path);
}

fn walker_metrics_summary(req: &IndexRequest, metrics: &WalkerMetrics, outcome: &str) -> String {
    format!(
        "flow=index source_kind=walker event=metrics request_id={} tab_id={} backend={} outcome={} elapsed_ms={} entries_emitted={} batches_sent={} dirs_read={} read_dir_errors={} max_inflight_read_dirs={} throttle_events={} adaptive_limit_min={} adaptive_limit_max={} adaptive_limit_final={} read_dir_avg_us={} read_dir_max_us={}",
        req.request_id,
        req.tab_id,
        walker_backend_label(metrics.backend),
        outcome,
        metrics.started_at.elapsed().as_millis(),
        metrics.entries_emitted,
        metrics.batches_sent,
        metrics.dirs_read,
        metrics.read_dir_errors,
        metrics.max_inflight_read_dirs,
        metrics.throttle_events,
        metrics.adaptive_limit_min,
        metrics.adaptive_limit_max,
        metrics.adaptive_limit_final,
        metrics.read_dir_avg_us(),
        metrics.read_dir_max_us
    )
}

fn write_walker_metrics_summary(summary: &str, path: &str) {
    let path = path.trim();
    if path.is_empty() {
        return;
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{summary}");
    }
}

fn flush_walker_batch(
    tx_res: &Sender<IndexResponse>,
    request_id: u64,
    buffer: &mut Vec<IndexEntry>,
    metrics: &mut WalkerMetrics,
) -> bool {
    let had_entries = !buffer.is_empty();
    let ok = flush_batch(tx_res, request_id, buffer);
    if ok && had_entries {
        metrics.record_batch();
    }
    ok
}

fn is_nested_filelist_candidate(path: &Path, root_filelist: &Path, root: &Path) -> bool {
    if path == root_filelist || !path.starts_with(root) {
        return false;
    }
    path.file_name()
        .and_then(|s| s.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("filelist.txt"))
}

fn index_source_kind(source: &IndexSource) -> &'static str {
    match source {
        IndexSource::None => "none",
        IndexSource::Walker => "walker",
        IndexSource::FileList(_) => "filelist",
    }
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
        flow = "index",
        source_kind = "filelist",
        event = "started",
        request_id = req.request_id,
        tab_id = req.tab_id,
        root = %root.display(),
        filelist = %filelist.display(),
        "worker request started"
    );
    if tx_res
        .send(IndexResponse::Started {
            request_id: req.request_id,
            source: source.clone(),
        })
        .is_err()
    {
        warn!(
            flow = "index",
            source_kind = "filelist",
            event = "receiver_closed",
            request_id = req.request_id,
            "worker response receiver closed before start"
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
                flow = "index",
                source_kind = "filelist",
                event = "receiver_closed",
                request_id = req.request_id,
                "worker response receiver closed during replace"
            );
            return Err("index receiver closed".to_string());
        }
    }
    info!(
        flow = "index",
        source_kind = "filelist",
        event = "finished",
        request_id = req.request_id,
        source = ?source,
        "worker request finished"
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
        flow = "index",
        source_kind = "walker",
        event = "started",
        request_id = req.request_id,
        tab_id = req.tab_id,
        root = %root.display(),
        include_files = req.include_files,
        include_dirs = req.include_dirs,
        "worker request started"
    );
    if tx_res
        .send(IndexResponse::Started {
            request_id: req.request_id,
            source: source.clone(),
        })
        .is_err()
    {
        warn!(
            flow = "index",
            source_kind = "walker",
            event = "receiver_closed",
            request_id = req.request_id,
            "worker response receiver closed before start"
        );
        return Err("index receiver closed".to_string());
    }

    let mut buffer: Vec<IndexEntry> = Vec::new();
    let mut last_flush = Instant::now();
    let mut cancel_check_budget = 0usize;
    let mut emitted_entries = 0usize;
    let settings = walker_runtime_settings(&current_runtime_config());
    let max_entries = settings.max_entries;
    let mut truncated = false;
    let mut metrics = WalkerMetrics::new(settings.backend, settings.threads);
    let should_cancel = || {
        if shutdown.load(Ordering::Relaxed) {
            return true;
        }
        latest_request_ids
            .lock()
            .ok()
            .and_then(|m| m.get(&req.tab_id).copied())
            != Some(req.request_id)
    };
    let mut stream_err: Option<String> = None;

    let mut handle_entry = |path: PathBuf, file_type: FileType| -> bool {
        cancel_check_budget = cancel_check_budget.saturating_add(1);
        if cancel_check_budget >= 64 {
            cancel_check_budget = 0;
            if should_cancel() {
                stream_err = Some("superseded".to_string());
                return false;
            }
        }
        let Some((kind, kind_known)) =
            classify_walker_entry(&path, file_type, req.include_files, req.include_dirs)
        else {
            return true;
        };
        if emitted_entries >= max_entries {
            truncated = true;
            return false;
        }
        buffer.push(IndexEntry {
            path,
            kind,
            kind_known,
        });
        emitted_entries = emitted_entries.saturating_add(1);
        metrics.entries_emitted = emitted_entries;
        if buffer.len() >= 256 || last_flush.elapsed() >= Duration::from_millis(100) {
            if !flush_walker_batch(tx_res, req.request_id, &mut buffer, &mut metrics) {
                stream_err = Some("index receiver closed".to_string());
                return false;
            }
            last_flush = Instant::now();
        }
        true
    };

    match settings.backend {
        WalkerBackend::Jwalk => {
            for entry in WalkDir::new(root)
                .parallelism(walker_parallelism(settings.threads))
                .skip_hidden(false)
                .follow_links(false)
                .min_depth(1)
                .into_iter()
                .flatten()
            {
                if !handle_entry(entry.path().to_path_buf(), entry.file_type()) {
                    break;
                }
            }
        }
        WalkerBackend::Adaptive => {
            let should_cancel_for_walk = || {
                if shutdown.load(Ordering::Relaxed) {
                    return true;
                }
                latest_request_ids
                    .lock()
                    .ok()
                    .and_then(|m| m.get(&req.tab_id).copied())
                    != Some(req.request_id)
            };
            let adaptive_metrics = walk_adaptive(
                root,
                settings.adaptive_max_limit,
                settings.adaptive_initial_limit,
                |entry: AdaptiveWalkerEntry| handle_entry(entry.path, entry.file_type),
                should_cancel_for_walk,
            );
            metrics.record_adaptive(adaptive_metrics);
        }
    }

    if let Some(err) = stream_err {
        if settings.metrics_enabled {
            log_walker_metrics(req, &metrics, &err, &settings.metrics_log_path);
        }
        return Err(err);
    }

    if !flush_walker_batch(tx_res, req.request_id, &mut buffer, &mut metrics) {
        if settings.metrics_enabled {
            log_walker_metrics(req, &metrics, "receiver_closed", &settings.metrics_log_path);
        }
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
            flow = "index",
            source_kind = "walker",
            event = "receiver_closed",
            request_id = req.request_id,
            "worker response receiver closed during truncation notice"
        );
        if settings.metrics_enabled {
            log_walker_metrics(req, &metrics, "receiver_closed", &settings.metrics_log_path);
        }
        return Err("index receiver closed".to_string());
    }
    if settings.metrics_enabled {
        log_walker_metrics(
            req,
            &metrics,
            if truncated { "truncated" } else { "finished" },
            &settings.metrics_log_path,
        );
    }
    info!(
        flow = "index",
        source_kind = "walker",
        event = "finished",
        request_id = req.request_id,
        emitted_entries,
        truncated,
        "worker request finished"
    );
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
                    warn!(
                        flow = "index",
                        source_kind = "none",
                        event = "receiver_closed",
                        request_id = req.request_id,
                        "worker response receiver closed before empty start"
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
                        flow = "index",
                        source_kind = "none",
                        event = "receiver_closed",
                        request_id = req.request_id,
                        "worker response receiver closed before empty finish"
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
                    let source_kind = index_source_kind(&source);
                    info!(
                        flow = "index",
                        source_kind,
                        event = "completed",
                        request_id = req.request_id,
                        "worker lifecycle completed"
                    );
                    if tx_res_worker
                        .send(IndexResponse::Finished {
                            request_id: req.request_id,
                            source: source.clone(),
                        })
                        .is_err()
                    {
                        warn!(
                            flow = "index",
                            source_kind,
                            event = "receiver_closed",
                            request_id = req.request_id,
                            "worker response receiver closed before finish"
                        );
                        break;
                    }
                }
                Err(error) => {
                    if error == "superseded" {
                        info!(
                            flow = "index",
                            event = "superseded",
                            request_id = req.request_id,
                            "worker request superseded"
                        );
                        let _ = tx_res_worker.send(IndexResponse::Canceled {
                            request_id: req.request_id,
                        });
                        continue;
                    }
                    warn!(
                        flow = "index",
                        event = "failed",
                        request_id = req.request_id,
                        error = %error,
                        "worker request failed"
                    );
                    if tx_res_worker
                        .send(IndexResponse::Failed {
                            request_id: req.request_id,
                            error,
                        })
                        .is_err()
                    {
                        warn!(
                            flow = "index",
                            event = "receiver_closed",
                            request_id = req.request_id,
                            "worker response receiver closed before failure"
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

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests;

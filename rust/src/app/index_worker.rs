use super::worker_protocol::{IndexEntry, IndexRequest, IndexResponse};
use crate::entry::EntryKind;
use crate::indexer::{
    apply_filelist_hierarchy_overrides, find_filelist_in_first_level, parse_filelist_stream,
    IndexSource,
};
use jwalk::{Parallelism, WalkDir};
use std::collections::HashMap;
use std::fs::FileType;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{info, warn};

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
            flow = "index",
            source_kind = "walker",
            event = "receiver_closed",
            request_id = req.request_id,
            "worker response receiver closed during truncation notice"
        );
        return Err("index receiver closed".to_string());
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
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tracing_subscriber::EnvFilter;

    fn init_test_tracing() {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            let filter =
                EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("off"));
            let _ = tracing_subscriber::fmt()
                .with_env_filter(filter)
                .without_time()
                .with_test_writer()
                .try_init();
        });
    }

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
    fn index_worker_trace_smoke_emits_canonical_fields() {
        init_test_tracing();
        let root = test_root("trace-smoke");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create dir");
        std::fs::write(root.join("main.rs"), "fn main() {}").expect("write file");

        let shutdown = Arc::new(AtomicBool::new(false));
        let latest_request_ids = Arc::new(Mutex::new(HashMap::new()));
        let (tx_req, rx_res, handles) = spawn_index_worker(shutdown.clone(), latest_request_ids);
        let request_id = 41u64;
        let tab_id = 7u64;
        tx_req
            .send(IndexRequest {
                request_id,
                tab_id,
                root: root.clone(),
                use_filelist: false,
                include_files: true,
                include_dirs: true,
            })
            .expect("send request");

        assert!(matches!(
            rx_res.recv().expect("started response"),
            IndexResponse::Started {
                request_id: 41,
                source: IndexSource::Walker,
            }
        ));
        assert!(matches!(
            rx_res.recv().expect("batch response"),
            IndexResponse::Batch { request_id: 41, .. }
        ));
        assert!(matches!(
            rx_res.recv().expect("finished response"),
            IndexResponse::Finished {
                request_id: 41,
                source: IndexSource::Walker,
            }
        ));

        drop(tx_req);
        shutdown.store(true, Ordering::Relaxed);
        for handle in handles {
            handle.join().expect("join index worker");
        }
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
            "Walker perf control_baseline eager_metadata_ms={eager_ms:.3} fast_classify_ms={fast_ms:.3} speedup={speedup:.2}x eager_count={eager_count} fast_count={fast_count}"
        );

        assert_eq!(eager_count, fast_count);
        assert!(
            speedup >= 1.30,
            "walker fast classification did not beat the control baseline enough: {speedup:.2}x"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}

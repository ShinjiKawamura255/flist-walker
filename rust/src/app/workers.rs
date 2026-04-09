use super::index_worker::resolve_entry_kind;
use super::worker_protocol::{
    ActionRequest, ActionResponse, FileListRequest, FileListResponse, KindResolveRequest,
    KindResolveResponse, PreviewRequest, PreviewResponse, SearchRequest, SearchResponse,
    SortMetadataRequest, SortMetadataResponse, UpdateRequest, UpdateRequestKind, UpdateResponse,
};
use super::worker_support::{action_notice_for_targets, action_targets_for_request};
use super::SortMetadata;
#[cfg(not(test))]
use crate::actions::execute_or_open;
use crate::indexer::write_filelist_cancellable;
use crate::search::{rank_search_results, SearchPrefixCache};
use crate::ui_model::build_preview_text_with_kind;
use crate::updater::{check_for_update, prepare_and_start_update};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use tracing::{info, warn};

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

            match &req.kind {
                UpdateRequestKind::Check => {
                    info!(request_id = req.request_id, "update worker starting check");
                }
                UpdateRequestKind::DownloadAndApply { candidate, .. } => {
                    info!(
                        request_id = req.request_id,
                        target_version = %candidate.target_version,
                        "update worker starting install"
                    );
                }
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

            match &response {
                UpdateResponse::UpToDate { request_id } => {
                    info!(
                        request_id = *request_id,
                        "update worker finished check: up to date"
                    );
                }
                UpdateResponse::Available {
                    request_id,
                    candidate,
                } => {
                    info!(
                        request_id = *request_id,
                        target_version = %candidate.target_version,
                        "update worker found candidate"
                    );
                }
                UpdateResponse::ApplyStarted {
                    request_id,
                    target_version,
                } => {
                    info!(
                        request_id = *request_id,
                        target_version = %target_version,
                        "update worker started apply"
                    );
                }
                UpdateResponse::CheckFailed { request_id, error } => {
                    warn!(request_id = *request_id, error = %error, "update worker check failed");
                }
                UpdateResponse::Failed { request_id, error } => {
                    warn!(request_id = *request_id, error = %error, "update worker apply failed");
                }
            }

            if tx_res.send(response).is_err() {
                warn!(request_id = req.request_id, "update worker receiver closed");
                break;
            }
        }
    });

    (tx_req, rx_res, handle)
}

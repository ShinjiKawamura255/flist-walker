use super::action_authorization::{
    authorize_action_targets, reauthorize_action_target, ActionAuthorizationFailure,
};
use super::index_worker::resolve_entry_kind;
use super::worker_channel::{
    bounded_request_channel, trace_worker_snapshot, BoundedSender, WorkerTraceContext,
};
use super::worker_protocol::{
    ActionRequest, ActionResponse, FileListRequest, FileListResponse, KindResolveRequest,
    KindResolveResponse, PreviewRequest, PreviewResponse, SearchRequest, SearchResponse,
    SortMetadataRequest, SortMetadataResponse, UpdateRequest, UpdateRequestKind, UpdateResponse,
};
use super::worker_support::action_notice_for_targets;
use super::SortMetadata;
#[cfg(not(test))]
use crate::actions::execute_or_open;
use crate::entry::EntryKind;
use crate::indexer::write_filelist_cancellable;
use crate::search::{
    rank_search_results, SearchPrefixCache, SearchResultSortMode, SearchResultSortScope,
};
use crate::ui_model::{build_preview_text_with_kind, normalize_path_for_display};
use crate::updater::{check_for_update, prepare_and_start_update};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use tracing::{info, warn};

pub(crate) type SharedKindResolver = Arc<dyn Fn(&Path) -> Option<EntryKind> + Send + Sync>;
pub(crate) type SharedActionExecutor = Arc<dyn Fn(&Path) -> anyhow::Result<()> + Send + Sync>;

fn trace_worker_started(flow: &'static str, request_id: u64) {
    info!(
        flow,
        event = "started",
        request_id,
        "worker request started"
    );
}

fn trace_worker_receiver_closed(flow: &'static str, request_id: u64) {
    warn!(
        flow,
        event = "receiver_closed",
        request_id,
        "worker response receiver closed"
    );
}

fn search_sort_mode(mode: super::ResultSortMode) -> SearchResultSortMode {
    match mode {
        super::ResultSortMode::Score => SearchResultSortMode::Score,
        super::ResultSortMode::NameAsc => SearchResultSortMode::NameAsc,
        super::ResultSortMode::NameDesc => SearchResultSortMode::NameDesc,
        super::ResultSortMode::ModifiedDesc => SearchResultSortMode::ModifiedDesc,
        super::ResultSortMode::ModifiedAsc => SearchResultSortMode::ModifiedAsc,
        super::ResultSortMode::CreatedDesc => SearchResultSortMode::CreatedDesc,
        super::ResultSortMode::CreatedAsc => SearchResultSortMode::CreatedAsc,
        super::ResultSortMode::SizeDesc => SearchResultSortMode::SizeDesc,
        super::ResultSortMode::SizeAsc => SearchResultSortMode::SizeAsc,
    }
}

fn search_sort_scope(scope: super::ResultSortScope) -> SearchResultSortScope {
    match scope {
        super::ResultSortScope::ShownResults => SearchResultSortScope::ShownResults,
        super::ResultSortScope::AllMatches => SearchResultSortScope::AllMatches,
    }
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
            trace_worker_started("search", req.request_id);
            let (result_set, error) = rank_search_results(
                &req.entries,
                &req.query,
                &req.root,
                req.limit,
                req.use_regex,
                req.ignore_case,
                req.prefer_relative,
                &mut prefix_cache,
                search_sort_mode(req.sort_mode),
                search_sort_scope(req.sort_scope),
            );
            info!(
                flow = "search",
                event = "finished",
                request_id = req.request_id,
                result_count = result_set.results.len(),
                total_match_count = result_set.total_match_count,
                has_error = error.is_some(),
                "worker request finished"
            );

            if tx_res
                .send(SearchResponse {
                    request_id: req.request_id,
                    results: result_set.results,
                    total_match_count: result_set.total_match_count,
                    sort_mode: req.sort_mode,
                    sort_scope: req.sort_scope,
                    error,
                })
                .is_err()
            {
                trace_worker_receiver_closed("search", req.request_id);
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
            trace_worker_started("preview", req.request_id);
            let preview = build_preview_text_with_kind(&req.path, req.is_dir);
            info!(
                flow = "preview",
                event = "finished",
                request_id = req.request_id,
                path = %req.path.display(),
                preview_chars = preview.chars().count(),
                "worker request finished"
            );
            if tx_res
                .send(PreviewResponse {
                    request_id: req.request_id,
                    path: req.path,
                    preview,
                })
                .is_err()
            {
                trace_worker_receiver_closed("preview", req.request_id);
                break;
            }
        }
    });

    (tx_req, rx_res, handle)
}

pub(super) fn spawn_kind_resolver_worker(
    shutdown: Arc<AtomicBool>,
    latest_epochs: Arc<Mutex<HashMap<u64, u64>>>,
) -> (
    BoundedSender<KindResolveRequest>,
    Receiver<KindResolveResponse>,
    thread::JoinHandle<()>,
) {
    spawn_kind_resolver_worker_with(shutdown, latest_epochs, Arc::new(resolve_entry_kind))
}

pub(crate) fn spawn_kind_resolver_worker_with(
    shutdown: Arc<AtomicBool>,
    latest_epochs: Arc<Mutex<HashMap<u64, u64>>>,
    resolve: SharedKindResolver,
) -> (
    BoundedSender<KindResolveRequest>,
    Receiver<KindResolveResponse>,
    thread::JoinHandle<()>,
) {
    let (tx_req, rx_req) = bounded_request_channel::<KindResolveRequest>(256);
    let (tx_res, rx_res) = mpsc::channel::<KindResolveResponse>();

    let handle = thread::Builder::new()
        .name("flistwalker-kind-resolver-0".to_string())
        .spawn(move || {
            while let Ok((req, inflight)) = rx_req.recv_tracked() {
                if shutdown.load(Ordering::Relaxed) {
                    trace_worker_snapshot(
                        inflight.load(),
                        "kind_resolver",
                        "terminal",
                        WorkerTraceContext {
                            worker_id: "flistwalker-kind-resolver-0",
                            request_id: None,
                            tab_id: Some(req.tab_id),
                            epoch: Some(req.epoch),
                            outcome: "canceled",
                        },
                    );
                    if tx_res
                        .send(KindResolveResponse {
                            tab_id: req.tab_id,
                            epoch: req.epoch,
                            path: req.path,
                            kind: None,
                        })
                        .is_err()
                    {
                        break;
                    }
                    continue;
                }
                let is_current = latest_epochs
                    .lock()
                    .map(|epochs| epochs.get(&req.tab_id).copied() == Some(req.epoch))
                    .unwrap_or(false);
                let kind = if is_current { resolve(&req.path) } else { None };
                trace_worker_snapshot(
                    inflight.load(),
                    "kind_resolver",
                    "terminal",
                    WorkerTraceContext {
                        worker_id: "flistwalker-kind-resolver-0",
                        request_id: None,
                        tab_id: Some(req.tab_id),
                        epoch: Some(req.epoch),
                        outcome: if is_current { "completed" } else { "stale" },
                    },
                );
                info!(
                    flow = "kind_resolver",
                    event = "finished",
                    tab_id = req.tab_id,
                    epoch = req.epoch,
                    path = %req.path.display(),
                    kind_known = kind.is_some(),
                    "worker request finished"
                );
                if tx_res
                    .send(KindResolveResponse {
                        tab_id: req.tab_id,
                        epoch: req.epoch,
                        path: req.path,
                        kind,
                    })
                    .is_err()
                {
                    warn!(
                        flow = "kind_resolver",
                        event = "receiver_closed",
                        tab_id = req.tab_id,
                        epoch = req.epoch,
                        "worker response receiver closed"
                    );
                    break;
                }
            }
        })
        .expect("spawn kind resolver worker");

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
            trace_worker_started("filelist", req.request_id);
            if req.cancel.load(Ordering::Relaxed) {
                info!(
                    flow = "filelist",
                    event = "canceled",
                    request_id = req.request_id,
                    root = %req.root.display(),
                    "worker request canceled before execution"
                );
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
            match &msg {
                FileListResponse::Finished {
                    request_id,
                    root,
                    path,
                    count,
                } => info!(
                    flow = "filelist",
                    event = "finished",
                    request_id = *request_id,
                    root = %root.display(),
                    path = %path.display(),
                    count = *count,
                    "worker request finished"
                ),
                FileListResponse::Canceled { request_id, root } => info!(
                    flow = "filelist",
                    event = "canceled",
                    request_id = *request_id,
                    root = %root.display(),
                    "worker request canceled"
                ),
                FileListResponse::Failed {
                    request_id,
                    root,
                    error,
                } => warn!(
                    flow = "filelist",
                    event = "failed",
                    request_id = *request_id,
                    root = %root.display(),
                    error = %error,
                    "worker request failed"
                ),
            }
            if tx_res.send(msg).is_err() {
                trace_worker_receiver_closed("filelist", req.request_id);
                break;
            }
        }
    });

    (tx_req, rx_res, handle)
}

pub(super) fn spawn_action_worker(
    shutdown: Arc<AtomicBool>,
) -> (
    BoundedSender<ActionRequest>,
    Receiver<ActionResponse>,
    Vec<thread::JoinHandle<()>>,
) {
    spawn_action_worker_with(shutdown, Arc::new(run_action_target))
}

pub(crate) fn spawn_action_worker_with(
    shutdown: Arc<AtomicBool>,
    execute: SharedActionExecutor,
) -> (
    BoundedSender<ActionRequest>,
    Receiver<ActionResponse>,
    Vec<thread::JoinHandle<()>>,
) {
    const ACTION_WORKERS: usize = 2;
    const ACTION_QUEUE_CAPACITY: usize = 8;

    let (tx_req, rx_req) = bounded_request_channel::<ActionRequest>(ACTION_QUEUE_CAPACITY);
    let (tx_res, rx_res) = mpsc::channel::<ActionResponse>();
    let rx_req = Arc::new(Mutex::new(rx_req));
    let mut handles = Vec::with_capacity(ACTION_WORKERS);
    for worker_index in 0..ACTION_WORKERS {
        let shutdown = Arc::clone(&shutdown);
        let execute = Arc::clone(&execute);
        let tx_res = tx_res.clone();
        let rx_req = Arc::clone(&rx_req);
        let worker_id = format!("flistwalker-action-{worker_index}");
        let handle = thread::Builder::new()
            .name(worker_id.clone())
            .spawn(move || loop {
                let received = {
                    let receiver = rx_req.lock().expect("action request receiver poisoned");
                    receiver.recv_tracked()
                };
                let Ok((req, inflight)) = received else {
                    break;
                };
                if shutdown.load(Ordering::Relaxed) {
                    trace_worker_snapshot(
                        inflight.load(),
                        "action",
                        "terminal",
                        WorkerTraceContext {
                            worker_id: &worker_id,
                            request_id: Some(req.request_id),
                            tab_id: None,
                            epoch: None,
                            outcome: "canceled",
                        },
                    );
                    if tx_res
                        .send(ActionResponse {
                            request_id: req.request_id,
                            notice: "Action canceled: application is shutting down".to_string(),
                        })
                        .is_err()
                    {
                        break;
                    }
                    continue;
                }
                let request_id = req.request_id;
                let outcome = run_action_request_with(req, &tx_res, execute.as_ref());
                trace_worker_snapshot(
                    inflight.load(),
                    "action",
                    "terminal",
                    WorkerTraceContext {
                        worker_id: &worker_id,
                        request_id: Some(request_id),
                        tab_id: None,
                        epoch: None,
                        outcome,
                    },
                );
            })
            .expect("spawn action worker");
        handles.push(handle);
    }
    drop(tx_res);

    (tx_req, rx_res, handles)
}

fn run_action_request_with(
    req: ActionRequest,
    tx_res: &Sender<ActionResponse>,
    execute: &(dyn Fn(&Path) -> anyhow::Result<()> + Send + Sync),
) -> &'static str {
    trace_worker_started("action", req.request_id);
    let (response, outcome) = process_action_request_with_outcome(req, |path| execute(path));
    info!(
        flow = "action",
        event = "finished",
        request_id = response.request_id,
        "worker request finished"
    );

    let request_id = response.request_id;
    if tx_res.send(response).is_err() {
        trace_worker_receiver_closed("action", request_id);
        "disconnected"
    } else {
        outcome.as_str()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ActionTerminalOutcome {
    Completed,
    Failed,
}

impl ActionTerminalOutcome {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

#[cfg(test)]
pub(crate) fn process_action_request_with(
    req: ActionRequest,
    execute: impl FnMut(&Path) -> anyhow::Result<()>,
) -> ActionResponse {
    process_action_request_with_outcome(req, execute).0
}

pub(crate) fn process_action_request_with_outcome(
    req: ActionRequest,
    mut execute: impl FnMut(&Path) -> anyhow::Result<()>,
) -> (ActionResponse, ActionTerminalOutcome) {
    let batch = match authorize_action_targets(&req.root, &req.paths, req.open_parent_for_files) {
        Ok(batch) => batch,
        Err(err) => {
            warn!(
                flow = "action",
                event = "authorization_failed",
                request_id = req.request_id,
                result = "blocked",
                completed = 0,
                total = req.paths.len(),
                error = %err,
                "action request blocked"
            );
            return (
                ActionResponse {
                    request_id: req.request_id,
                    notice: action_blocked_notice(&err),
                },
                ActionTerminalOutcome::Failed,
            );
        }
    };
    let total = batch.targets.len();

    for (completed, target) in batch.targets.iter().enumerate() {
        let execution_path = match reauthorize_action_target(&batch.canonical_root, target) {
            Ok(path) => path,
            Err(err) => {
                let result = if completed == 0 { "blocked" } else { "partial" };
                warn!(
                    flow = "action",
                    event = "reauthorization_failed",
                    request_id = req.request_id,
                    result,
                    completed,
                    total,
                    error = %err,
                    "action target reauthorization failed"
                );
                let notice = if completed == 0 {
                    action_blocked_notice(&err)
                } else {
                    format!(
                        "Action failed after launching {completed} of {total} items while opening {}: authorization changed",
                        normalize_path_for_display(&target.display_path)
                    )
                };
                return (
                    ActionResponse {
                        request_id: req.request_id,
                        notice,
                    },
                    ActionTerminalOutcome::Failed,
                );
            }
        };
        if let Err(err) = execute(&execution_path) {
            let result = if completed == 0 { "failed" } else { "partial" };
            warn!(
                flow = "action",
                event = "executor_failed",
                request_id = req.request_id,
                result,
                completed,
                total,
                error = %err,
                "action executor failed"
            );
            let display_path = normalize_path_for_display(&target.display_path);
            return (
                ActionResponse {
                    request_id: req.request_id,
                    notice: if completed == 0 {
                        format!("Action failed: {display_path}")
                    } else {
                        format!(
                            "Action failed after launching {completed} of {total} items while opening {display_path}"
                        )
                    },
                },
                ActionTerminalOutcome::Failed,
            );
        }
    }

    let display_targets: Vec<PathBuf> = batch
        .targets
        .iter()
        .map(|target| target.display_path.clone())
        .collect();
    info!(
        flow = "action",
        event = "completed",
        request_id = req.request_id,
        result = "success",
        completed = total,
        total,
        "action request completed"
    );
    (
        ActionResponse {
            request_id: req.request_id,
            notice: action_notice_for_targets(&display_targets),
        },
        ActionTerminalOutcome::Completed,
    )
}

fn action_blocked_notice(failure: &ActionAuthorizationFailure) -> String {
    match &failure.display_path {
        Some(path) => format!(
            "Action blocked: {}: {failure}",
            normalize_path_for_display(path)
        ),
        None => format!("Action blocked: {failure}"),
    }
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
            trace_worker_started("sort_metadata", req.request_id);

            let mut entries = Vec::with_capacity(req.paths.len());
            for path in req.paths {
                if shutdown.load(Ordering::Relaxed) {
                    return;
                }
                let metadata = std::fs::metadata(&path)
                    .ok()
                    .map(|meta| {
                        let size_bytes = meta.is_file().then_some(meta.len());
                        SortMetadata {
                            modified: meta.modified().ok(),
                            created: meta.created().ok(),
                            size_bytes,
                        }
                    })
                    .unwrap_or_default();
                entries.push((path, metadata));
            }
            info!(
                flow = "sort_metadata",
                event = "finished",
                request_id = req.request_id,
                entry_count = entries.len(),
                mode = ?req.mode,
                "worker request finished"
            );

            if tx_res
                .send(SortMetadataResponse {
                    request_id: req.request_id,
                    entries,
                    mode: req.mode,
                })
                .is_err()
            {
                trace_worker_receiver_closed("sort_metadata", req.request_id);
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
                    info!(
                        flow = "update",
                        event = "check_started",
                        request_id = req.request_id,
                        "worker request started"
                    );
                }
                UpdateRequestKind::DownloadAndApply { candidate, .. } => {
                    info!(
                        flow = "update",
                        event = "install_started",
                        request_id = req.request_id,
                        target_version = %candidate.target_version,
                        "worker request started"
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
                        flow = "update",
                        event = "check_finished_up_to_date",
                        request_id = *request_id,
                        "worker request finished"
                    );
                }
                UpdateResponse::Available {
                    request_id,
                    candidate,
                } => {
                    info!(
                        flow = "update",
                        event = "check_finished_available",
                        request_id = *request_id,
                        target_version = %candidate.target_version,
                        "worker request finished"
                    );
                }
                UpdateResponse::ApplyStarted {
                    request_id,
                    target_version,
                } => {
                    info!(
                        flow = "update",
                        event = "install_finished_apply_started",
                        request_id = *request_id,
                        target_version = %target_version,
                        "worker request finished"
                    );
                }
                UpdateResponse::CheckFailed { request_id, error } => {
                    warn!(
                        flow = "update",
                        event = "check_failed",
                        request_id = *request_id,
                        error = %error,
                        "worker request failed"
                    );
                }
                UpdateResponse::Failed { request_id, error } => {
                    warn!(
                        flow = "update",
                        event = "install_failed",
                        request_id = *request_id,
                        error = %error,
                        "worker request failed"
                    );
                }
            }

            if tx_res.send(response).is_err() {
                trace_worker_receiver_closed("update", req.request_id);
                break;
            }
        }
    });

    (tx_req, rx_res, handle)
}

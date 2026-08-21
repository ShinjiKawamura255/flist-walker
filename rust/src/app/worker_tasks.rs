use super::worker_channel::{
    bounded_request_channel, trace_worker_snapshot, BoundedSender, WorkerTraceContext,
};
use super::worker_protocol::{
    ActionRequest, ActionResponse, CatalogRequest, CatalogRequestKind, CatalogResponse,
    FileListRequest, FileListResponse, KindResolveRequest, KindResolveResponse, PreviewRequest,
    PreviewResponse, RootValidationIntent, RootValidationRequest, RootValidationResponse,
    SearchRequest, SearchResponse, SortMetadataRequest, SortMetadataResponse, UpdateRequest,
    UpdateRequestKind, UpdateResponse, ValidatedRoot,
};
use super::worker_support::action_notice_for_targets;
use super::SortMetadata;
#[cfg(not(test))]
use crate::actions::execute_or_open;
use crate::actions::{
    execute_authorized_action_request, AuthorizedActionBackend, AuthorizedActionGuard,
    AuthorizedActionMode, AuthorizedActionOutcome, AuthorizedActionReport, AuthorizedActionRequest,
};
use crate::entry::EntryKind;
use crate::indexer::{
    execute_filelist_write_plan, plan_filelist_write_cancellable, FileListWriteOptions,
    FileListWriteStatus,
};
use crate::path_utils::{normalize_windows_path_buf, path_key};
use crate::search::{rank_search_results_cancellable, SearchPrefixCache, SearchRunOutcome};
use crate::search_catalog::{load_search_catalog, search_catalog_file_path, update_search_catalog};
use crate::ui_model::{build_preview_text_with_kind, normalize_path_for_display};
use crate::walker_runtime::resolve_entry_kind;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use tracing::{info, warn};

pub(crate) type SharedKindResolver = Arc<dyn Fn(&Path) -> Option<EntryKind> + Send + Sync>;
pub(crate) type SharedActionExecutor = Arc<dyn Fn(&Path) -> anyhow::Result<()> + Send + Sync>;

fn resolve_named_root_path(root: &Path) -> anyhow::Result<PathBuf> {
    let root = root.canonicalize().map_err(|error| {
        anyhow::anyhow!(
            "failed to canonicalize named root {}: {error}",
            root.display()
        )
    })?;
    if !root.is_dir() {
        anyhow::bail!("named root is not a directory: {}", root.display());
    }
    Ok(root)
}

fn validate_root_request(req: &RootValidationRequest) -> anyhow::Result<ValidatedRoot> {
    let input = req.input.trim();
    if input.is_empty() {
        anyhow::bail!("Enter a folder path.");
    }
    let path = normalize_windows_path_buf(PathBuf::from(input));
    if !path.is_dir() {
        anyhow::bail!("Folder not found: {}", path.display());
    }
    let canonical = normalize_windows_path_buf(path.canonicalize().map_err(|error| {
        anyhow::anyhow!("Failed to resolve folder {}: {error}", path.display())
    })?);
    let key = path_key(&canonical);
    let duplicate = req
        .draft_roots
        .iter()
        .enumerate()
        .filter(|(index, _)| !matches!(req.intent, RootValidationIntent::Edit { index: edit } if *index == edit))
        .any(|(_, candidate)| {
            candidate
                .canonicalize()
                .ok()
                .map(normalize_windows_path_buf)
                .is_some_and(|candidate| path_key(&candidate) == key)
        });
    if duplicate {
        anyhow::bail!("This folder is already in the list.");
    }
    Ok(ValidatedRoot {
        path: canonical,
        key,
    })
}

pub(super) fn spawn_root_validation_worker(
    shutdown: Arc<AtomicBool>,
) -> (
    Sender<RootValidationRequest>,
    Receiver<RootValidationResponse>,
    thread::JoinHandle<()>,
) {
    let (tx_req, rx_req) = mpsc::channel::<RootValidationRequest>();
    let (tx_res, rx_res) = mpsc::channel::<RootValidationResponse>();
    let handle = thread::Builder::new()
        .name("flistwalker-root-validation".to_string())
        .spawn(move || {
            while let Ok(req) = rx_req.recv() {
                if shutdown.load(Ordering::Relaxed) {
                    break;
                }
                let response = RootValidationResponse {
                    request_id: req.request_id,
                    dialog_generation: req.dialog_generation,
                    intent: req.intent,
                    result: validate_root_request(&req).map_err(|error| error.to_string()),
                };
                if tx_res.send(response).is_err() {
                    break;
                }
            }
        })
        .expect("spawn root validation worker");
    (tx_req, rx_res, handle)
}

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
        while let Ok(req) = rx_req.recv() {
            if shutdown.load(Ordering::Relaxed) {
                break;
            }
            trace_worker_started("search", req.request_id);
            let cancellation_requested =
                || shutdown.load(Ordering::Relaxed) || req.cancel.load(Ordering::Acquire);
            let SearchRunOutcome::Completed(result_set, error) = rank_search_results_cancellable(
                &req.entries,
                &req.query,
                &req.root,
                req.limit,
                req.use_regex,
                req.ignore_case,
                req.prefer_relative,
                &mut prefix_cache,
                req.sort_mode,
                req.sort_scope,
                &cancellation_requested,
            ) else {
                info!(
                    flow = "search",
                    event = "canceled",
                    request_id = req.request_id,
                    "worker request canceled"
                );
                continue;
            };
            if cancellation_requested() {
                info!(
                    flow = "search",
                    event = "canceled",
                    request_id = req.request_id,
                    "worker request canceled before response publication"
                );
                continue;
            }
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

pub(super) fn spawn_catalog_worker(
    shutdown: Arc<AtomicBool>,
) -> (
    Sender<CatalogRequest>,
    Receiver<CatalogResponse>,
    thread::JoinHandle<()>,
) {
    let (tx_req, rx_req) = mpsc::channel::<CatalogRequest>();
    let (tx_res, rx_res) = mpsc::channel::<CatalogResponse>();
    let handle = thread::Builder::new()
        .name("flistwalker-search-catalog".to_string())
        .spawn(move || {
            while let Ok(req) = rx_req.recv() {
                if shutdown.load(Ordering::Relaxed) {
                    break;
                }
                trace_worker_started("search_catalog", req.request_id);
                let result = match req.kind {
                    CatalogRequestKind::Load => load_search_catalog(),
                    CatalogRequestKind::AddNamedRoot { name, path } => {
                        resolve_named_root_path(&path)
                            .and_then(|path| {
                                search_catalog_file_path().map(|catalog_path| (path, catalog_path))
                            })
                            .and_then(|(path, catalog_path)| {
                                update_search_catalog(&catalog_path, |catalog| {
                                    catalog.add_named_root(&name, path)
                                })
                            })
                    }
                    CatalogRequestKind::ReplaceNamedRoot {
                        original_name,
                        name,
                        path,
                    } => resolve_named_root_path(&path)
                        .and_then(|path| {
                            search_catalog_file_path().map(|catalog_path| (path, catalog_path))
                        })
                        .and_then(|(path, catalog_path)| {
                            update_search_catalog(&catalog_path, |catalog| {
                                catalog.replace_named_root(&original_name, &name, path)
                            })
                        }),
                    CatalogRequestKind::RemoveNamedRoot { name } => search_catalog_file_path()
                        .and_then(|catalog_path| {
                            update_search_catalog(&catalog_path, |catalog| {
                                catalog.remove_named_root(&name)
                            })
                        }),
                    CatalogRequestKind::AddPreset { preset } => search_catalog_file_path()
                        .and_then(|path| {
                            update_search_catalog(&path, |catalog| catalog.add_preset(preset))
                        }),
                    CatalogRequestKind::ReplacePreset {
                        original_name,
                        preset,
                    } => search_catalog_file_path().and_then(|path| {
                        update_search_catalog(&path, |catalog| {
                            catalog.replace_preset(&original_name, preset)
                        })
                    }),
                    CatalogRequestKind::RemovePreset { name } => search_catalog_file_path()
                        .and_then(|path| {
                            update_search_catalog(&path, |catalog| catalog.remove_preset(&name))
                        }),
                }
                .map_err(|error| error.to_string());
                if tx_res
                    .send(CatalogResponse {
                        request_id: req.request_id,
                        result,
                    })
                    .is_err()
                {
                    trace_worker_receiver_closed("search_catalog", req.request_id);
                    break;
                }
            }
        })
        .expect("spawn search catalog worker");
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
            let cancellation_requested =
                || shutdown.load(Ordering::Relaxed) || req.cancel.load(Ordering::Relaxed);
            let msg = match plan_filelist_write_cancellable(
                &req.root,
                &req.entries,
                FileListWriteOptions {
                    // GUI confirmation is the explicit root-overwrite consent.
                    allow_root_overwrite: true,
                    propagate_to_ancestors: req.propagate_to_ancestors,
                },
                &cancellation_requested,
            ) {
                Ok(plan) => {
                    let path = plan.root_target().to_path_buf();
                    let report = execute_filelist_write_plan(&plan, &cancellation_requested);
                    match report.status {
                        FileListWriteStatus::Completed => FileListResponse::Finished {
                            request_id: req.request_id,
                            root: req.root.clone(),
                            path,
                            count,
                        },
                        FileListWriteStatus::Canceled if report.exit_code() == 130 => {
                            FileListResponse::Canceled {
                                request_id: req.request_id,
                                root: req.root.clone(),
                            }
                        }
                        FileListWriteStatus::Canceled | FileListWriteStatus::Failed => {
                            FileListResponse::Failed {
                                request_id: req.request_id,
                                root: req.root.clone(),
                                error: report.summary(),
                            }
                        }
                    }
                }
                Err(report) if report.status == FileListWriteStatus::Canceled => {
                    FileListResponse::Canceled {
                        request_id: req.request_id,
                        root: req.root.clone(),
                    }
                }
                Err(report) => FileListResponse::Failed {
                    request_id: req.request_id,
                    root: req.root.clone(),
                    error: report.summary(),
                },
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
                let outcome = run_action_request_with(req, &tx_res, execute.as_ref(), &shutdown);
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
    shutdown: &Arc<AtomicBool>,
) -> &'static str {
    trace_worker_started("action", req.request_id);
    let (response, outcome) = process_action_request_with_outcome_and_cancellation(
        req,
        |path| execute(path),
        Arc::clone(shutdown),
    );
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

#[cfg(test)]
pub(crate) fn process_action_request_with_outcome(
    req: ActionRequest,
    execute: impl FnMut(&Path) -> anyhow::Result<()>,
) -> (ActionResponse, ActionTerminalOutcome) {
    process_action_request_with_outcome_and_cancellation(
        req,
        execute,
        Arc::new(AtomicBool::new(false)),
    )
}

fn process_action_request_with_outcome_and_cancellation(
    req: ActionRequest,
    execute: impl FnMut(&Path) -> anyhow::Result<()>,
    cancellation: Arc<AtomicBool>,
) -> (ActionResponse, ActionTerminalOutcome) {
    let request_id = req.request_id;
    let mode = if req.open_parent_for_files {
        AuthorizedActionMode::Reveal
    } else {
        AuthorizedActionMode::ExecuteOrOpen
    };
    let request = AuthorizedActionRequest::new_with_cancellation(
        req.request_id,
        req.root,
        req.paths,
        mode,
        cancellation,
    );
    let backend = WorkerActionBackend::new(execute);
    let report = execute_authorized_action_request(&request, &WorkerActionGuard, &backend);
    let outcome = if report.outcome == AuthorizedActionOutcome::Completed {
        ActionTerminalOutcome::Completed
    } else {
        ActionTerminalOutcome::Failed
    };
    (
        ActionResponse {
            request_id,
            notice: action_notice_for_report(&report),
        },
        outcome,
    )
}

struct WorkerActionGuard;

impl AuthorizedActionGuard for WorkerActionGuard {
    fn is_current(&self, _request_id: u64, _trusted_root: &Path) -> bool {
        true
    }
}

struct WorkerActionBackend<F> {
    execute: Mutex<F>,
}

impl<F> WorkerActionBackend<F> {
    fn new(execute: F) -> Self {
        Self {
            execute: Mutex::new(execute),
        }
    }

    fn dispatch(&self, path: &Path) -> anyhow::Result<()>
    where
        F: FnMut(&Path) -> anyhow::Result<()>,
    {
        (self.execute.lock().expect("action executor mutex poisoned"))(path)
    }
}

impl<F> AuthorizedActionBackend for WorkerActionBackend<F>
where
    F: FnMut(&Path) -> anyhow::Result<()>,
{
    fn execute_or_open(&self, path: &Path) -> anyhow::Result<()> {
        self.dispatch(path)
    }

    fn reveal(&self, path: &Path) -> anyhow::Result<()> {
        self.dispatch(path)
    }
}

fn action_notice_for_report(report: &AuthorizedActionReport) -> String {
    match report.outcome {
        AuthorizedActionOutcome::Completed => action_notice_for_targets(&report.display_targets),
        AuthorizedActionOutcome::Blocked => {
            action_blocked_notice(report.display_path.as_deref(), report.diagnostic.as_deref())
        }
        AuthorizedActionOutcome::Canceled => "Action canceled".to_string(),
        AuthorizedActionOutcome::Superseded => "Action canceled: request superseded".to_string(),
        AuthorizedActionOutcome::Failed => format!(
            "Action failed: {}",
            report
                .display_path
                .as_deref()
                .map(normalize_path_for_display)
                .unwrap_or_else(|| "selected target".to_string())
        ),
        AuthorizedActionOutcome::PartialFailure => format!(
            "Action failed after launching {} of {} items while opening {}",
            report.completed,
            report.total,
            report
                .display_path
                .as_deref()
                .map(normalize_path_for_display)
                .unwrap_or_else(|| "selected target".to_string())
        ),
    }
}

fn action_blocked_notice(display_path: Option<&Path>, diagnostic: Option<&str>) -> String {
    match (display_path, diagnostic) {
        (Some(path), Some(diagnostic)) => format!(
            "Action blocked: {}: {diagnostic}",
            normalize_path_for_display(path)
        ),
        (Some(path), None) => format!("Action blocked: {}", normalize_path_for_display(path)),
        (None, Some(diagnostic)) => format!("Action blocked: {diagnostic}"),
        (None, None) => "Action blocked".to_string(),
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
                UpdateRequestKind::Check => {
                    match crate::updater::check_for_update_with_control(req.control.as_ref()) {
                        Ok(Some(candidate)) => UpdateResponse::Available {
                            request_id: req.request_id,
                            candidate: Box::new(candidate),
                        },
                        Ok(None) => UpdateResponse::UpToDate {
                            request_id: req.request_id,
                        },
                        Err(_) if req.control.cancel_requested() => UpdateResponse::Canceled {
                            request_id: req.request_id,
                        },
                        Err(err) => UpdateResponse::CheckFailed {
                            request_id: req.request_id,
                            error: format!("Update check failed: {err}"),
                        },
                    }
                }
                UpdateRequestKind::DownloadAndApply {
                    candidate,
                    current_exe,
                } => match crate::updater::prepare_and_start_update_with_control(
                    candidate.as_ref(),
                    &current_exe,
                    crate::updater::UpdateRestartMode::Gui,
                    req.control.as_ref(),
                ) {
                    Ok(()) => UpdateResponse::ApplyStarted {
                        request_id: req.request_id,
                        target_version: candidate.target_version.clone(),
                    },
                    Err(_) if req.control.cancel_requested() => UpdateResponse::Canceled {
                        request_id: req.request_id,
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
                UpdateResponse::Canceled { request_id } => {
                    info!(
                        flow = "update",
                        event = "canceled",
                        request_id = *request_id,
                        "worker request finished"
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

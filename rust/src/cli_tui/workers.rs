use super::protocol::{
    IndexRequest, PreviewRequest, SearchRequest, TuiActionBackend, TuiActionFreshness,
    TuiActionRequest, TuiIndexFreshness, TuiSource, WorkerResponse, EVENT_POLL,
    WORKER_JOIN_TIMEOUT,
};
use crate::actions::execute_authorized_action_request;
use crate::entry::Entry;
use crate::indexer::{
    build_index_cancellable, find_filelist_in_first_level, is_index_build_cancelled,
};
use crate::query::{CompiledIgnoreTerms, QueryScope};
use crate::runtime_config::{current_runtime_config, RuntimeConfig};
use crate::search::{rank_search_results, SearchPrefixCache, SearchSortScope};
use crate::ui_model::build_preview_text_with_kind;
#[cfg(not(test))]
use crate::updater::check_for_update;
use crate::updater::UpdateCandidate;
use crate::walker_runtime::{
    classify_walker_entry, walk_adaptive, walker_runtime_settings, AdaptiveWalkerEntry,
};
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;

#[cfg(not(test))]
pub(super) fn spawn_tui_update_check() -> mpsc::Receiver<Option<UpdateCandidate>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let candidate = check_for_update().ok().flatten();
        let _ = tx.send(candidate);
    });
    rx
}

#[cfg(test)]
pub(super) fn spawn_tui_update_check() -> mpsc::Receiver<Option<UpdateCandidate>> {
    let (_tx, rx) = mpsc::channel();
    rx
}

struct WorkerHandle {
    handle: thread::JoinHandle<()>,
    done: mpsc::Receiver<()>,
}

pub(super) struct TuiWorkerSet {
    cancellation: Arc<AtomicBool>,
    _response_tx: mpsc::Sender<WorkerResponse>,
    response_rx: mpsc::Receiver<WorkerResponse>,
    search_tx: mpsc::Sender<SearchRequest>,
    preview_tx: mpsc::Sender<PreviewRequest>,
    action_tx: mpsc::Sender<TuiActionRequest>,
    index_tx: mpsc::Sender<IndexRequest>,
    search: WorkerHandle,
    preview: WorkerHandle,
    action: WorkerHandle,
    index: WorkerHandle,
    index_freshness: Arc<TuiIndexFreshness>,
    action_freshness: Arc<TuiActionFreshness>,
}

impl TuiWorkerSet {
    pub(super) fn start() -> Result<Self> {
        let cancellation = Arc::new(AtomicBool::new(false));
        let (response_tx, response_rx) = mpsc::channel();

        let (search_tx, search_rx) = mpsc::channel::<SearchRequest>();
        let (search_done_tx, search_done_rx) = mpsc::channel();
        let search_cancelled = Arc::clone(&cancellation);
        let search_response_tx = response_tx.clone();
        let search_handle = thread::Builder::new()
            .name("flistwalker-cli-search".to_string())
            .spawn(move || {
                let mut prefix_cache = SearchPrefixCache::default();
                while !search_cancelled.load(Ordering::Relaxed) {
                    let mut request = match search_rx.recv_timeout(EVENT_POLL) {
                        Ok(request) => request,
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    };
                    while let Ok(newer) = search_rx.try_recv() {
                        request = newer;
                    }
                    let (results, error) = search(&request, &mut prefix_cache);
                    if search_cancelled.load(Ordering::Relaxed)
                        || search_response_tx
                            .send(WorkerResponse::Searched {
                                request_id: request.request_id,
                                root: request.root,
                                query: request.query,
                                options: request.options,
                                results,
                                error,
                            })
                            .is_err()
                    {
                        break;
                    }
                }
                let _ = search_done_tx.send(());
            })
            .context("failed to start CLI search worker")?;
        let search = WorkerHandle {
            handle: search_handle,
            done: search_done_rx,
        };

        let (preview_tx, preview_rx) = mpsc::channel::<PreviewRequest>();
        let (preview_done_tx, preview_done_rx) = mpsc::channel();
        let preview_cancelled = Arc::clone(&cancellation);
        let preview_response_tx = response_tx.clone();
        let preview_handle = match thread::Builder::new()
            .name("flistwalker-cli-preview".to_string())
            .spawn(move || {
                while !preview_cancelled.load(Ordering::Relaxed) {
                    let mut request = match preview_rx.recv_timeout(EVENT_POLL) {
                        Ok(request) => request,
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    };
                    while let Ok(newer) = preview_rx.try_recv() {
                        request = newer;
                    }
                    let is_dir = request.path.is_dir();
                    let preview = build_preview_text_with_kind(&request.path, is_dir);
                    if preview_cancelled.load(Ordering::Relaxed)
                        || preview_response_tx
                            .send(WorkerResponse::Previewed {
                                request_id: request.request_id,
                                root: request.root,
                                path: request.path,
                                preview,
                            })
                            .is_err()
                    {
                        break;
                    }
                }
                let _ = preview_done_tx.send(());
            }) {
            Ok(handle) => handle,
            Err(error) => {
                cancellation.store(true, Ordering::Relaxed);
                drop(search_tx);
                finish_worker(search.handle, search.done);
                return Err(error).context("failed to start CLI preview worker");
            }
        };
        let preview = WorkerHandle {
            handle: preview_handle,
            done: preview_done_rx,
        };

        let action_freshness = Arc::new(TuiActionFreshness::new());
        let (action_tx, action_rx) = mpsc::channel::<TuiActionRequest>();
        let (action_done_tx, action_done_rx) = mpsc::channel();
        let action_cancelled = Arc::clone(&cancellation);
        let action_response_tx = response_tx.clone();
        let action_worker_freshness = Arc::clone(&action_freshness);
        let action_handle = match thread::Builder::new()
            .name("flistwalker-cli-action".to_string())
            .spawn(move || {
                while !action_cancelled.load(Ordering::Acquire) {
                    let mut action = match action_rx.recv_timeout(EVENT_POLL) {
                        Ok(action) => action,
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    };
                    while let Ok(newer) = action_rx.try_recv() {
                        action = newer;
                    }
                    let request_id = action.request.request_id;
                    let root = action.request.trusted_root.clone();
                    let selected_path = action.selected_path;
                    let report = execute_authorized_action_request(
                        &action.request,
                        action_worker_freshness.as_ref(),
                        &TuiActionBackend,
                    );
                    if action_cancelled.load(Ordering::Acquire)
                        || action_response_tx
                            .send(WorkerResponse::Actioned {
                                request_id,
                                root,
                                selected_path,
                                report,
                            })
                            .is_err()
                    {
                        break;
                    }
                }
                let _ = action_done_tx.send(());
            }) {
            Ok(handle) => handle,
            Err(error) => {
                cancellation.store(true, Ordering::Release);
                drop(search_tx);
                drop(preview_tx);
                finish_worker(search.handle, search.done);
                finish_worker(preview.handle, preview.done);
                return Err(error).context("failed to start CLI action worker");
            }
        };
        let action = WorkerHandle {
            handle: action_handle,
            done: action_done_rx,
        };

        let (index_tx, index_rx) = mpsc::channel::<IndexRequest>();
        let (index_done_tx, index_done_rx) = mpsc::channel();
        let index_cancelled = Arc::clone(&cancellation);
        let index_response_tx = response_tx.clone();
        let index_freshness = Arc::new(TuiIndexFreshness::new());
        let worker_index_freshness = Arc::clone(&index_freshness);
        let index_handle = match thread::Builder::new()
            .name("flistwalker-cli-index-search".to_string())
            .spawn(move || {
                while !index_cancelled.load(Ordering::Relaxed) {
                    let mut request = match index_rx.recv_timeout(EVENT_POLL) {
                        Ok(request) => request,
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    };
                    while let Ok(newer) = index_rx.try_recv() {
                        request = newer;
                    }
                    let request_id = request.request_id;
                    let should_cancel = || {
                        index_cancelled.load(Ordering::Relaxed)
                            || !worker_index_freshness.is_current(request_id)
                    };
                    process_index_request(request, &should_cancel, |response| {
                        let _ = index_response_tx.send(response);
                    });
                }
                let _ = index_done_tx.send(());
            }) {
            Ok(handle) => handle,
            Err(error) => {
                cancellation.store(true, Ordering::Relaxed);
                drop(search_tx);
                drop(preview_tx);
                drop(action_tx);
                finish_worker(search.handle, search.done);
                finish_worker(preview.handle, preview.done);
                finish_worker(action.handle, action.done);
                return Err(error).context("failed to start CLI index worker");
            }
        };

        Ok(Self {
            cancellation,
            _response_tx: response_tx,
            response_rx,
            search_tx,
            preview_tx,
            action_tx,
            index_tx,
            search,
            preview,
            action,
            index: WorkerHandle {
                handle: index_handle,
                done: index_done_rx,
            },
            index_freshness,
            action_freshness,
        })
    }

    pub(super) fn search_tx(&self) -> &mpsc::Sender<SearchRequest> {
        &self.search_tx
    }

    pub(super) fn preview_tx(&self) -> &mpsc::Sender<PreviewRequest> {
        &self.preview_tx
    }

    pub(super) fn action_tx(&self) -> &mpsc::Sender<TuiActionRequest> {
        &self.action_tx
    }

    pub(super) fn index_tx(&self) -> &mpsc::Sender<IndexRequest> {
        &self.index_tx
    }

    pub(super) fn response_rx(&self) -> &mpsc::Receiver<WorkerResponse> {
        &self.response_rx
    }

    pub(super) fn index_freshness(&self) -> Arc<TuiIndexFreshness> {
        Arc::clone(&self.index_freshness)
    }

    pub(super) fn action_freshness(&self) -> Arc<TuiActionFreshness> {
        Arc::clone(&self.action_freshness)
    }

    pub(super) fn cancellation(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancellation)
    }

    pub(super) fn shutdown(self) {
        let Self {
            cancellation,
            _response_tx,
            response_rx,
            search_tx,
            preview_tx,
            action_tx,
            index_tx,
            search,
            preview,
            action,
            index,
            index_freshness,
            action_freshness,
        } = self;
        cancellation.store(true, Ordering::Release);
        drop(search_tx);
        drop(index_tx);
        drop(preview_tx);
        drop(action_tx);
        finish_worker(search.handle, search.done);
        finish_worker(preview.handle, preview.done);
        finish_worker(action.handle, action.done);
        finish_worker(index.handle, index.done);
        drop((_response_tx, response_rx, index_freshness, action_freshness));
    }
}

pub(super) fn process_index_request<C, S>(request: IndexRequest, should_cancel: &C, send: S)
where
    C: Fn() -> bool,
    S: FnMut(WorkerResponse),
{
    let config = current_runtime_config();
    process_index_request_with_config(request, &config, should_cancel, send);
}

pub(super) fn process_index_request_with_config<C, S>(
    request: IndexRequest,
    config: &RuntimeConfig,
    should_cancel: &C,
    mut send: S,
) where
    C: Fn() -> bool,
    S: FnMut(WorkerResponse),
{
    if should_cancel() {
        return;
    }
    match std::fs::metadata(&request.root) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => {
            send(WorkerResponse::IndexFailed {
                request_id: request.request_id,
                root: request.root,
                has_root_filelist: false,
                error: "selected root is not a directory".to_string(),
            });
            return;
        }
        Err(error) => {
            send(WorkerResponse::IndexFailed {
                request_id: request.request_id,
                root: request.root,
                has_root_filelist: false,
                error: format!("failed to read selected root: {error}"),
            });
            return;
        }
    }
    let has_filelist = find_filelist_in_first_level(&request.root).is_some();
    let use_filelist = match request.source {
        TuiSource::Auto => has_filelist,
        TuiSource::FileList => true,
        TuiSource::Walker => false,
    };
    if request.source == TuiSource::FileList && !has_filelist {
        if !should_cancel() {
            send(WorkerResponse::IndexFailed {
                request_id: request.request_id,
                root: request.root,
                has_root_filelist: false,
                error: "FileList source selected but no FileList was found".to_string(),
            });
        }
        return;
    }

    if use_filelist {
        match build_index_cancellable(
            &request.root,
            true,
            request.include_files,
            request.include_dirs,
            should_cancel,
        ) {
            Ok(paths) => {
                if !paths.is_empty() && !should_cancel() {
                    send(WorkerResponse::IndexedBatch {
                        request_id: request.request_id,
                        root: request.root.clone(),
                        entries: paths,
                    });
                }
            }
            Err(error) if is_index_build_cancelled(&error) => return,
            Err(error) => {
                if !should_cancel() {
                    send(WorkerResponse::IndexFailed {
                        request_id: request.request_id,
                        root: request.root,
                        has_root_filelist: has_filelist,
                        error: error.to_string(),
                    });
                }
                return;
            }
        }
    } else {
        let settings = walker_runtime_settings(config);
        let max_entries = settings.max_entries;
        let mut batch = Vec::with_capacity(256);
        let mut emitted_entries = 0usize;
        let mut truncated = false;
        walk_adaptive(
            &request.root,
            settings.adaptive_max_limit,
            settings.adaptive_initial_limit,
            |entry: AdaptiveWalkerEntry| {
                if should_cancel() {
                    return false;
                }
                if classify_walker_entry(
                    &entry.path,
                    entry.file_type,
                    request.include_files,
                    request.include_dirs,
                )
                .is_none()
                {
                    return true;
                }
                if emitted_entries >= max_entries {
                    truncated = true;
                    return false;
                }
                batch.push(entry.path);
                emitted_entries = emitted_entries.saturating_add(1);
                if batch.len() >= 256 && !should_cancel() {
                    send(WorkerResponse::IndexedBatch {
                        request_id: request.request_id,
                        root: request.root.clone(),
                        entries: std::mem::take(&mut batch),
                    });
                }
                true
            },
            should_cancel,
        );
        if should_cancel() {
            return;
        }
        if !batch.is_empty() {
            send(WorkerResponse::IndexedBatch {
                request_id: request.request_id,
                root: request.root.clone(),
                entries: batch,
            });
        }
        if truncated {
            send(WorkerResponse::IndexTruncated {
                request_id: request.request_id,
                root: request.root.clone(),
                limit: max_entries,
            });
        }
    }

    if !should_cancel() {
        send(WorkerResponse::IndexedFinished {
            request_id: request.request_id,
            root: request.root,
            has_root_filelist: has_filelist,
        });
    }
}

pub(super) fn finish_worker(handle: thread::JoinHandle<()>, done: mpsc::Receiver<()>) {
    if done.recv_timeout(WORKER_JOIN_TIMEOUT).is_ok() {
        let _ = handle.join();
    }
}

pub(super) fn search(
    request: &SearchRequest,
    prefix_cache: &mut SearchPrefixCache,
) -> (Vec<(PathBuf, f64)>, Option<String>) {
    let compiled_ignore = request
        .options
        .ignore_enabled
        .then(|| CompiledIgnoreTerms::compile(&request.ignore_terms, request.options.ignore_case));
    let entries = Arc::new(
        request
            .entries
            .iter()
            .flat_map(|batch| batch.iter())
            .filter(|path| {
                compiled_ignore.as_ref().is_none_or(|compiled| {
                    !compiled.matches_path(
                        path,
                        QueryScope {
                            root: Some(&request.root),
                            prefer_relative: true,
                            ignore_case: request.options.ignore_case,
                        },
                    )
                })
            })
            .cloned()
            .map(Entry::from)
            .collect(),
    );
    let (result_set, error) = rank_search_results(
        &entries,
        &request.query,
        &request.root,
        request.limit,
        request.options.regex,
        request.options.ignore_case,
        true,
        prefix_cache,
        request.options.sort_mode,
        SearchSortScope::AllMatches,
    );
    (result_set.results, error)
}

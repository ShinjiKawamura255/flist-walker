use super::protocol::{IndexRequest, SearchRequest, TuiSource, WorkerResponse};
use super::WORKER_JOIN_TIMEOUT;
use crate::entry::Entry;
use crate::indexer::{
    build_index_cancellable, find_filelist_in_first_level, is_index_build_cancelled,
};
use crate::query::{CompiledIgnoreTerms, QueryScope};
use crate::runtime_config::{current_runtime_config, RuntimeConfig};
use crate::search::{rank_search_results, SearchPrefixCache, SearchSortScope};
use crate::walker_runtime::{
    classify_walker_entry, walk_adaptive, walker_runtime_settings, AdaptiveWalkerEntry,
};
use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use std::thread;

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

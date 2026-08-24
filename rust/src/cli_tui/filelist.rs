use super::input::selected_paths;
use super::protocol::{
    FileListWorkerResult, IndexRequest, TuiActionFreshness, TuiExit, TuiFileListDiscoveryRequest,
    TuiFileListRequest, TuiIndexFreshness,
};
use super::state::{ActiveFileListWorker, PendingFileListIntent, TuiState};
use crate::indexer::{
    build_index_cancellable, execute_filelist_write_plan, is_index_build_cancelled,
    plan_filelist_write_cancellable, FileListWriteOptions, FileListWriteReport,
    FileListWriteStatus,
};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{mpsc, Arc};
use std::thread;

pub(super) fn spawn_filelist_discovery_worker(
    request: TuiFileListDiscoveryRequest,
) -> Result<ActiveFileListWorker> {
    let cancel = Arc::clone(&request.cancel);
    let (done_tx, done) = mpsc::channel();
    let (result_tx, result) = mpsc::channel();
    let handle = thread::Builder::new()
        .name("flistwalker-cli-filelist-discovery".to_string())
        .spawn(move || {
            let request_id = request.request_id;
            let root = request.root.clone();
            let response = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                match crate::indexer::find_filelist_in_first_level_cancellable(
                    &request.root,
                    || request.cancel.load(Ordering::Acquire),
                ) {
                    Ok(discovered) => FileListWorkerResult::DiscoveryFinished {
                        request_id,
                        root: root.clone(),
                        discovered,
                        canceled: false,
                    },
                    Err(_) => FileListWorkerResult::DiscoveryFinished {
                        request_id,
                        root: root.clone(),
                        discovered: None,
                        canceled: true,
                    },
                }
            }))
            .unwrap_or_else(|_| FileListWorkerResult::Failed {
                request_id,
                root,
                error: "FileList discovery worker panicked".to_string(),
            });
            let _ = result_tx.send(response);
            let _ = done_tx.send(());
        })
        .context("failed to start CLI FileList discovery worker")?;
    Ok(ActiveFileListWorker {
        cancel,
        result,
        done,
        handle: Some(handle),
    })
}

pub(super) fn spawn_filelist_worker(request: TuiFileListRequest) -> Result<ActiveFileListWorker> {
    let cancel = Arc::clone(&request.cancel);
    let (done_tx, done) = mpsc::channel();
    let (result_tx, result) = mpsc::channel();
    let handle = thread::Builder::new()
        .name("flistwalker-cli-filelist".to_string())
        .spawn(move || {
            let request_id = request.request_id;
            let root = request.root.clone();
            let response = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let should_cancel = || request.cancel.load(Ordering::Acquire);
                let entries = match build_filelist_snapshot(&request.root, &should_cancel) {
                    Ok(entries) => entries,
                    Err(report) => {
                        return FileListWorkerResult::Finished {
                            request_id,
                            root: root.clone(),
                            report: *report,
                        };
                    }
                };
                let report = match plan_filelist_write_cancellable(
                    &request.root,
                    &entries,
                    FileListWriteOptions {
                        allow_root_overwrite: request.allow_root_overwrite,
                        propagate_to_ancestors: request.propagate_to_ancestors,
                    },
                    &should_cancel,
                ) {
                    Ok(plan) => execute_filelist_write_plan(&plan, &should_cancel),
                    Err(report) => *report,
                };
                FileListWorkerResult::Finished {
                    request_id,
                    root: root.clone(),
                    report,
                }
            }))
            .unwrap_or_else(|_| FileListWorkerResult::Failed {
                request_id,
                root,
                error: "FileList worker panicked".to_string(),
            });
            let _ = result_tx.send(response);
            let _ = done_tx.send(());
        })
        .context("failed to start CLI FileList worker")?;
    Ok(ActiveFileListWorker {
        cancel,
        result,
        done,
        handle: Some(handle),
    })
}

/// FileList creation must never inherit the TUI's currently displayed index: it
/// may be limited by the active source or file-kind filters.  Build the same
/// fresh, walker-only all-kinds snapshot used by the batch path instead.
pub(super) fn build_filelist_snapshot<C>(
    root: &Path,
    should_cancel: &C,
) -> std::result::Result<Vec<PathBuf>, Box<FileListWriteReport>>
where
    C: Fn() -> bool,
{
    let entries = match build_index_cancellable(root, false, true, true, should_cancel) {
        Ok(entries) => entries,
        Err(error) if is_index_build_cancelled(&error) => {
            return Err(Box::new(canceled_filelist_report(root)));
        }
        Err(error) => {
            return Err(Box::new(FileListWriteReport {
                status: FileListWriteStatus::Failed,
                root_target: root.join("FileList.txt"),
                committed: Vec::new(),
                failed: vec![crate::indexer::FileListWriteFailure {
                    path: root.to_path_buf(),
                    error: error.to_string(),
                }],
                rolled_back: Vec::new(),
                rollback_failed: Vec::new(),
            }));
        }
    };
    Ok(entries
        .into_iter()
        .filter(|entry| !is_root_filelist_entry(root, entry))
        .collect())
}

pub(super) fn canceled_filelist_report(root: &Path) -> FileListWriteReport {
    FileListWriteReport {
        status: FileListWriteStatus::Canceled,
        root_target: root.join("FileList.txt"),
        committed: Vec::new(),
        failed: Vec::new(),
        rolled_back: Vec::new(),
        rollback_failed: Vec::new(),
    }
}

pub(super) fn is_root_filelist_entry(root: &Path, entry: &Path) -> bool {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let Ok(relative) = entry.strip_prefix(root) else {
        return false;
    };
    relative.components().count() == 1
        && relative
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("filelist.txt"))
}

pub(super) enum FileListSettlement {
    Completed,
    Canceled,
    Failed(String),
}

pub(super) enum FileListDiscoverySettlement {
    Completed(Option<PathBuf>),
    Canceled,
    Failed(String),
}

pub(super) fn settle_filelist_discovery(
    state: &mut TuiState,
    request_id: u64,
    root: &Path,
    settlement: FileListDiscoverySettlement,
    index_tx: &mpsc::Sender<IndexRequest>,
    index_freshness: &TuiIndexFreshness,
    action_freshness: &TuiActionFreshness,
) -> Option<TuiExit> {
    let active = state.active_filelist.as_ref()?;
    if active.request_id != request_id
        || active.root.as_path() != root
        || active.kind != super::state::ActiveFileListKind::Discovery
    {
        return None;
    }
    // Regression guard: discovery owns the same deferred user intents as creation,
    // but must consume them at this boundary so none can leak into a later write.
    state.active_filelist = None;
    state.filelist_confirmation = None;
    let intent = state.pending_filelist_intent.take();
    match intent {
        Some(PendingFileListIntent::SelectOutput) => {
            return Some(TuiExit::Selected {
                paths: selected_paths(state),
                query: state.query.clone(),
                root: state.root.clone(),
            });
        }
        Some(PendingFileListIntent::SwitchRoot(root)) => {
            state.prepare_root_switch(action_freshness, root);
            if state
                .dispatch_current_index(index_tx, index_freshness)
                .is_err()
            {
                state.status = "Index worker unavailable".to_string();
                state.dirty = true;
            }
            return None;
        }
        Some(PendingFileListIntent::CancelExit) => return Some(TuiExit::Cancelled),
        None => {}
    }
    match settlement {
        FileListDiscoverySettlement::Completed(discovered) => {
            state.root_filelist_known = true;
            state.root_filelist_exists = discovered.is_some();
            state.open_filelist_confirmation();
        }
        FileListDiscoverySettlement::Canceled => {
            state.status = "FileList check canceled".to_string();
            state.dirty = true;
        }
        FileListDiscoverySettlement::Failed(error) => {
            state.status = format!("FileList check failed: {error}");
            state.dirty = true;
        }
    }
    None
}

pub(super) fn filelist_settlement_from_report(
    state: &mut TuiState,
    request_id: u64,
    root: &Path,
    report: FileListWriteReport,
) -> Option<FileListSettlement> {
    let active = state.active_filelist.as_ref()?;
    if active.request_id != request_id || active.root.as_path() != root {
        return None;
    }
    state.active_filelist = None;
    Some(match report.status {
        FileListWriteStatus::Completed => FileListSettlement::Completed,
        FileListWriteStatus::Canceled if report.exit_code() == 130 => FileListSettlement::Canceled,
        FileListWriteStatus::Canceled | FileListWriteStatus::Failed => {
            FileListSettlement::Failed(report.summary())
        }
    })
}

pub(super) fn filelist_worker_failure(
    state: &mut TuiState,
    request_id: u64,
    root: &Path,
    error: String,
) -> Option<FileListSettlement> {
    let active = state.active_filelist.as_ref()?;
    if active.request_id != request_id || active.root.as_path() != root {
        return None;
    }
    state.active_filelist = None;
    Some(FileListSettlement::Failed(error))
}

pub(super) fn settle_filelist(
    state: &mut TuiState,
    settlement: FileListSettlement,
    index_tx: &mpsc::Sender<IndexRequest>,
    index_freshness: &TuiIndexFreshness,
    action_freshness: &TuiActionFreshness,
) -> Option<TuiExit> {
    let intent = state.pending_filelist_intent.take();
    match settlement {
        FileListSettlement::Failed(error) => {
            state.status = format!("FileList creation failed: {error}");
            state.dirty = true;
            if intent == Some(PendingFileListIntent::CancelExit) {
                return Some(TuiExit::Failed(error));
            }
        }
        FileListSettlement::Completed | FileListSettlement::Canceled => {
            let completed = matches!(settlement, FileListSettlement::Completed);
            state.status = if completed {
                "FileList created; refreshing...".to_string()
            } else {
                "FileList creation canceled".to_string()
            };
            state.dirty = true;
            match intent {
                Some(PendingFileListIntent::CancelExit) => return Some(TuiExit::Cancelled),
                Some(PendingFileListIntent::SelectOutput) => {
                    return Some(TuiExit::Selected {
                        paths: selected_paths(state),
                        query: state.query.clone(),

                        root: state.root.clone(),
                    });
                }
                Some(PendingFileListIntent::SwitchRoot(root)) => {
                    state.prepare_root_switch(action_freshness, root);
                    if state
                        .dispatch_current_index(index_tx, index_freshness)
                        .is_err()
                    {
                        state.status = "Index worker unavailable".to_string();
                        state.dirty = true;
                    }
                }
                None if completed => {
                    state.prepare_refresh();
                    if state
                        .dispatch_current_index(index_tx, index_freshness)
                        .is_err()
                    {
                        state.status = "Index worker unavailable".to_string();
                        state.dirty = true;
                    }
                }
                None => {}
            }
        }
    }
    None
}

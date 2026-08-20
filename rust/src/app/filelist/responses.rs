use super::super::{FileListResponse, FlistWalkerApp};
use super::commands::{FileListAppCommand, FileListCommand};
use crate::app::state::{
    FileListResponseContext, FileListResponseScope, PendingFileListIndexCompletionNotice,
};
use crate::path_utils::path_key;
use std::path::{Path, PathBuf};
impl FlistWalkerApp {
    fn set_filelist_completion_notice_for_tab(&mut self, tab_index: usize, notice: String) {
        if tab_index == self.shell.tabs.active_tab_index() {
            self.set_notice(notice);
        } else if let Some(tab) = self.shell.tabs.get_mut(tab_index) {
            tab.notice = notice;
        }
    }

    fn set_filelist_notice_for_tab_id(&mut self, tab_id: Option<u64>, notice: String) {
        let Some(tab_index) = tab_id.and_then(|tab_id| self.find_tab_index_by_id(tab_id)) else {
            return;
        };
        self.set_filelist_completion_notice_for_tab(tab_index, notice);
    }

    fn track_filelist_completion_notice_for_reindex(
        &mut self,
        tab_index: usize,
        root: &Path,
        notice: String,
    ) {
        let active = tab_index == self.shell.tabs.active_tab_index();
        let Some(tab) = self.shell.tabs.get(tab_index) else {
            return;
        };
        let tab_id = tab.id;
        let request_id = if active {
            self.shell.indexing.pending_request_id
        } else {
            tab.index_state.pending_index_request_id
        };
        let Some(request_id) = request_id else {
            return;
        };
        self.shell
            .features
            .filelist
            .workflow
            .pending_index_completion_notices
            .insert(
                request_id,
                PendingFileListIndexCompletionNotice {
                    tab_id,
                    root: root.to_path_buf(),
                    notice,
                },
            );
    }

    fn resolve_filelist_target_tab_index(&self, tab_id: Option<u64>, root: &Path) -> Option<usize> {
        let tab_id = tab_id?;
        let tab_index = self.find_tab_index_by_id(tab_id)?;
        let tab_matches_root = self
            .shell
            .tabs
            .get(tab_index)
            .is_some_and(|tab| path_key(&tab.root) == path_key(root));
        tab_matches_root.then_some(tab_index)
    }

    fn handle_filelist_finished_response(
        &mut self,
        context: FileListResponseContext,
        root: PathBuf,
        path: PathBuf,
        count: usize,
    ) {
        if matches!(
            context.root_scope,
            FileListResponseScope::StaleRequestedRoot
        ) {
            return;
        }
        let target_tab_index = self.resolve_filelist_target_tab_index(context.tab_id, &root);
        if let Some(tab_index) = target_tab_index {
            self.dispatch_filelist_commands(vec![FileListCommand::App(
                FileListAppCommand::SetUseFileListForTab {
                    tab_index,
                    use_filelist: true,
                },
            )]);
        }

        match context.root_scope {
            FileListResponseScope::PreviousRoot => {
                let completion_notice = format!(
                    "Created {}: {} entries (previous root)",
                    path.display(),
                    count
                );
                if let Some(tab_index) = target_tab_index {
                    self.set_filelist_completion_notice_for_tab(
                        tab_index,
                        completion_notice.clone(),
                    );
                    if tab_index != self.shell.tabs.active_tab_index() {
                        self.dispatch_filelist_commands(vec![FileListCommand::App(
                            FileListAppCommand::RequestBackgroundIndexRefreshForTab(tab_index),
                        )]);
                        self.track_filelist_completion_notice_for_reindex(
                            tab_index,
                            &root,
                            completion_notice,
                        );
                    }
                }
            }
            FileListResponseScope::CurrentRoot => {
                let completion_notice = format!("Created {}: {} entries", path.display(), count);
                if let Some(tab_index) = target_tab_index {
                    self.set_filelist_completion_notice_for_tab(
                        tab_index,
                        completion_notice.clone(),
                    );
                    if tab_index == self.shell.tabs.active_tab_index()
                        && self.shell.runtime.use_filelist
                    {
                        self.dispatch_filelist_commands(vec![FileListCommand::App(
                            FileListAppCommand::RequestIndexRefresh,
                        )]);
                        self.track_filelist_completion_notice_for_reindex(
                            tab_index,
                            &root,
                            completion_notice,
                        );
                    } else if tab_index != self.shell.tabs.active_tab_index() {
                        self.dispatch_filelist_commands(vec![FileListCommand::App(
                            FileListAppCommand::RequestBackgroundIndexRefreshForTab(tab_index),
                        )]);
                        self.track_filelist_completion_notice_for_reindex(
                            tab_index,
                            &root,
                            completion_notice,
                        );
                    }
                }
            }
            FileListResponseScope::StaleRequestedRoot => {}
        }
    }

    fn handle_filelist_failed_response(&mut self, context: FileListResponseContext, error: String) {
        let tab_id = context.tab_id;
        match context.root_scope {
            FileListResponseScope::StaleRequestedRoot => {}
            FileListResponseScope::PreviousRoot => {
                self.set_filelist_notice_for_tab_id(
                    tab_id,
                    format!("Create File List failed for previous root: {}", error),
                );
            }
            FileListResponseScope::CurrentRoot => {
                self.set_filelist_notice_for_tab_id(
                    tab_id,
                    format!("Create File List failed: {}", error),
                );
            }
        }
    }

    fn handle_filelist_canceled_response(&mut self, context: FileListResponseContext) {
        if !matches!(
            context.root_scope,
            FileListResponseScope::StaleRequestedRoot
        ) {
            self.set_filelist_notice_for_tab_id(
                context.tab_id,
                "Create File List canceled".to_string(),
            );
        }
    }

    pub(in crate::app) fn poll_filelist_response(&mut self) {
        let current_root = self.shell.runtime.root.clone();
        while let Ok(response) = self.shell.worker_bus.filelist.rx.try_recv() {
            match response {
                FileListResponse::Finished {
                    request_id,
                    root,
                    path,
                    count,
                } => {
                    let Some((context, commands)) = self
                        .shell
                        .features
                        .filelist
                        .settle_response_context_commands(request_id, &root, &current_root)
                    else {
                        continue;
                    };
                    self.dispatch_filelist_commands(commands);
                    self.handle_filelist_finished_response(context, root, path, count);
                }
                FileListResponse::Failed {
                    request_id,
                    root,
                    error,
                } => {
                    let Some((context, commands)) = self
                        .shell
                        .features
                        .filelist
                        .settle_response_context_commands(request_id, &root, &current_root)
                    else {
                        continue;
                    };
                    self.dispatch_filelist_commands(commands);
                    self.handle_filelist_failed_response(context, error);
                }
                FileListResponse::Canceled { request_id, root } => {
                    let Some((context, commands)) = self
                        .shell
                        .features
                        .filelist
                        .settle_response_context_commands(request_id, &root, &current_root)
                    else {
                        continue;
                    };
                    self.dispatch_filelist_commands(commands);
                    self.handle_filelist_canceled_response(context);
                }
            }
        }
    }
}

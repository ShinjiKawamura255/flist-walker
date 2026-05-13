use super::super::FileListRequest;
use super::commands::{FileListCommand, FileListUiCommand, FileListWorkerCommand};
use crate::app::state::{
    FileListManager, FileListRequestContext, FileListResponseContext, FileListResponseScope,
};
use crate::path_utils::path_key;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
impl FileListManager {
    pub(in crate::app::filelist) fn start_request_commands(
        &mut self,
        tab_id: u64,
        root: PathBuf,
        entries: Vec<PathBuf>,
        propagate_to_ancestors: bool,
    ) -> Vec<FileListCommand> {
        let cancel = Arc::new(AtomicBool::new(false));
        let request_id = self.begin_request(tab_id, root.clone(), Arc::clone(&cancel));
        let req = FileListRequest {
            request_id,
            tab_id,
            root,
            entries,
            propagate_to_ancestors,
            cancel,
        };
        vec![
            FileListCommand::Ui(FileListUiCommand::RefreshStatusLine),
            FileListCommand::Worker(FileListWorkerCommand::Start(req)),
        ]
    }

    pub(in crate::app::filelist) fn send_failure_commands(&mut self) -> Vec<FileListCommand> {
        self.clear_request();
        vec![
            FileListCommand::Ui(FileListUiCommand::RefreshStatusLine),
            FileListCommand::Ui(FileListUiCommand::SetNotice(
                "Create File List worker is unavailable".to_string(),
            )),
        ]
    }

    pub(in crate::app::filelist) fn begin_request(
        &mut self,
        tab_id: u64,
        root: PathBuf,
        cancel: Arc<AtomicBool>,
    ) -> u64 {
        self.workflow.pending_after_index = None;
        let request_id = self.workflow.next_request_id;
        self.workflow.next_request_id = self.workflow.next_request_id.saturating_add(1);
        self.workflow.pending_request_id = Some(request_id);
        self.workflow.pending_request_tab_id = Some(tab_id);
        self.workflow.pending_root = Some(root);
        self.workflow.pending_cancel = Some(cancel);
        self.workflow.in_progress = true;
        self.workflow.cancel_requested = false;
        request_id
    }

    pub(in crate::app::filelist) fn clear_request(&mut self) {
        self.workflow.pending_request_id = None;
        self.workflow.pending_request_tab_id = None;
        self.workflow.pending_root = None;
        self.workflow.pending_cancel = None;
        self.workflow.in_progress = false;
        self.workflow.cancel_requested = false;
    }

    pub(in crate::app::filelist) fn settle_response(
        &mut self,
        request_id: u64,
    ) -> Option<FileListRequestContext> {
        if self.workflow.pending_request_id != Some(request_id) {
            return None;
        }
        let context = FileListRequestContext {
            root: self.workflow.pending_root.clone(),
            tab_id: self.workflow.pending_request_tab_id,
        };
        self.clear_request();
        Some(context)
    }

    fn classify_response_scope(
        requested_root: Option<&Path>,
        response_root: &Path,
        current_root: &Path,
    ) -> FileListResponseScope {
        let response_root_key = path_key(response_root);
        let same_requested_root = requested_root
            .map(|root| path_key(root) == response_root_key)
            .unwrap_or(true);
        if !same_requested_root {
            return FileListResponseScope::StaleRequestedRoot;
        }
        if path_key(current_root) == response_root_key {
            FileListResponseScope::CurrentRoot
        } else {
            FileListResponseScope::PreviousRoot
        }
    }

    pub(in crate::app::filelist) fn settle_response_context_commands(
        &mut self,
        request_id: u64,
        response_root: &Path,
        current_root: &Path,
    ) -> Option<(FileListResponseContext, Vec<FileListCommand>)> {
        let context = self.settle_response(request_id)?;
        let response_context = FileListResponseContext {
            tab_id: context.tab_id,
            root_scope: Self::classify_response_scope(
                context.root.as_deref(),
                response_root,
                current_root,
            ),
        };
        Some((
            response_context,
            vec![FileListCommand::Ui(FileListUiCommand::RefreshStatusLine)],
        ))
    }

    pub(in crate::app::filelist) fn request_cancel(&mut self) -> Option<Arc<AtomicBool>> {
        if !self.workflow.in_progress || self.workflow.cancel_requested {
            return None;
        }
        self.workflow.cancel_requested = true;
        self.workflow.pending_cancel.as_ref().map(Arc::clone)
    }

    pub(in crate::app::filelist) fn cancel_stale_pending_confirmation(
        &mut self,
        current_tab_id: u64,
        current_root_key: &str,
    ) -> bool {
        let should_cancel = self
            .workflow
            .pending_confirmation
            .as_ref()
            .is_some_and(|pending| {
                pending.tab_id == current_tab_id && path_key(&pending.root) != current_root_key
            });
        if should_cancel {
            self.workflow.pending_confirmation = None;
        }
        should_cancel
    }

    pub(in crate::app::filelist) fn cancel_stale_pending_ancestor_confirmation(
        &mut self,
        current_tab_id: u64,
        current_root_key: &str,
    ) -> bool {
        let should_cancel = self
            .workflow
            .pending_ancestor_confirmation
            .as_ref()
            .is_some_and(|pending| {
                pending.tab_id == current_tab_id && path_key(&pending.root) != current_root_key
            });
        if should_cancel {
            self.workflow.pending_ancestor_confirmation = None;
        }
        should_cancel
    }

    pub(in crate::app::filelist) fn cancel_stale_pending_use_walker_confirmation(
        &mut self,
        current_tab_id: u64,
        current_root_key: &str,
    ) -> bool {
        let should_cancel = self
            .workflow
            .pending_use_walker_confirmation
            .as_ref()
            .is_some_and(|pending| {
                pending.source_tab_id == current_tab_id
                    && path_key(&pending.root) != current_root_key
            });
        if should_cancel {
            self.workflow.pending_use_walker_confirmation = None;
        }
        should_cancel
    }

    pub(in crate::app) fn clear_pending_for_tab(&mut self, tab_id: u64) {
        if self
            .workflow
            .pending_after_index
            .as_ref()
            .is_some_and(|pending| pending.tab_id == tab_id)
        {
            self.workflow.pending_after_index = None;
        }
        if self
            .workflow
            .pending_confirmation
            .as_ref()
            .is_some_and(|pending| pending.tab_id == tab_id)
        {
            self.workflow.pending_confirmation = None;
        }
        if self
            .workflow
            .pending_ancestor_confirmation
            .as_ref()
            .is_some_and(|pending| pending.tab_id == tab_id)
        {
            self.workflow.pending_ancestor_confirmation = None;
        }
        if self
            .workflow
            .pending_use_walker_confirmation
            .as_ref()
            .is_some_and(|pending| pending.source_tab_id == tab_id)
        {
            self.workflow.pending_use_walker_confirmation = None;
        }
    }
}

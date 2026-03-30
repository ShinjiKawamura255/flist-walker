use super::{
    FileListDialogKind, PendingFileListAfterIndex, PendingFileListAncestorConfirmation,
    PendingFileListConfirmation, PendingFileListUseWalkerConfirmation, UpdatePromptState,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

pub(super) struct FileListWorkflowState {
    pub(super) next_request_id: u64,
    pub(super) pending_request_id: Option<u64>,
    pub(super) pending_request_tab_id: Option<u64>,
    pub(super) pending_root: Option<PathBuf>,
    pub(super) pending_cancel: Option<Arc<AtomicBool>>,
    pub(super) pending_after_index: Option<PendingFileListAfterIndex>,
    pub(super) pending_confirmation: Option<PendingFileListConfirmation>,
    pub(super) pending_ancestor_confirmation: Option<PendingFileListAncestorConfirmation>,
    pub(super) pending_use_walker_confirmation: Option<PendingFileListUseWalkerConfirmation>,
    pub(super) in_progress: bool,
    pub(super) cancel_requested: bool,
    pub(super) active_dialog: Option<FileListDialogKind>,
    pub(super) active_dialog_button: usize,
}

impl Default for FileListWorkflowState {
    fn default() -> Self {
        Self {
            next_request_id: 1,
            pending_request_id: None,
            pending_request_tab_id: None,
            pending_root: None,
            pending_cancel: None,
            pending_after_index: None,
            pending_confirmation: None,
            pending_ancestor_confirmation: None,
            pending_use_walker_confirmation: None,
            in_progress: false,
            cancel_requested: false,
            active_dialog: None,
            active_dialog_button: 0,
        }
    }
}

pub(super) struct UpdateState {
    pub(super) next_request_id: u64,
    pub(super) pending_request_id: Option<u64>,
    pub(super) in_progress: bool,
    pub(super) prompt: Option<UpdatePromptState>,
    pub(super) skipped_target_version: Option<String>,
    pub(super) close_requested_for_install: bool,
}

impl Default for UpdateState {
    fn default() -> Self {
        Self {
            next_request_id: 1,
            pending_request_id: None,
            in_progress: false,
            prompt: None,
            skipped_target_version: None,
            close_requested_for_install: false,
        }
    }
}

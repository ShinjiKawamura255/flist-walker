use super::*;
use crate::app::cache::{HighlightCacheState, PreviewCacheState, SortMetadataCacheState};
use eframe::egui;
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::SystemTime;

#[derive(Default)]
pub(super) struct BackgroundIndexState {
    pub(super) source: Option<IndexSource>,
    pub(super) entries: Vec<Entry>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct SortMetadata {
    pub(super) modified: Option<SystemTime>,
    pub(super) created: Option<SystemTime>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ResultSortMode {
    #[default]
    Score,
    NameAsc,
    NameDesc,
    ModifiedDesc,
    ModifiedAsc,
    CreatedDesc,
    CreatedAsc,
}

impl ResultSortMode {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Score => "Score",
            Self::NameAsc => "Name (A-Z)",
            Self::NameDesc => "Name (Z-A)",
            Self::ModifiedDesc => "Modified (New)",
            Self::ModifiedAsc => "Modified (Old)",
            Self::CreatedDesc => "Created (New)",
            Self::CreatedAsc => "Created (Old)",
        }
    }

    pub(super) fn uses_metadata(self) -> bool {
        matches!(
            self,
            Self::ModifiedDesc | Self::ModifiedAsc | Self::CreatedDesc | Self::CreatedAsc
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TabAccentPalette {
    pub(super) background: egui::Color32,
    pub(super) border: egui::Color32,
    pub(super) foreground: egui::Color32,
}

impl TabAccentPalette {
    pub(super) const fn new(
        background: (u8, u8, u8),
        border: (u8, u8, u8),
        foreground: (u8, u8, u8),
    ) -> Self {
        Self {
            background: egui::Color32::from_rgb(background.0, background.1, background.2),
            border: egui::Color32::from_rgb(border.0, border.1, border.2),
            foreground: egui::Color32::from_rgb(foreground.0, foreground.1, foreground.2),
        }
    }

    pub(super) const fn clear_outline(dark_mode: bool) -> Self {
        if dark_mode {
            Self::new((0x23, 0x27, 0x2E), (0x55, 0x5D, 0x68), (0xD7, 0xDC, 0xE4))
        } else {
            Self::new((0xF2, 0xF4, 0xF7), (0xC8, 0xCF, 0xD8), (0x4E, 0x56, 0x61))
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(super) struct HighlightCacheKey {
    pub(super) path: PathBuf,
    pub(super) prefer_relative: bool,
    pub(super) use_regex: bool,
    pub(super) ignore_case: bool,
}

pub(super) struct PendingFileListConfirmation {
    pub(super) tab_id: u64,
    pub(super) root: PathBuf,
    pub(super) entries: Vec<PathBuf>,
    pub(super) existing_path: PathBuf,
}

pub(super) struct PendingFileListAncestorConfirmation {
    pub(super) tab_id: u64,
    pub(super) root: PathBuf,
    pub(super) entries: Vec<PathBuf>,
}

pub(super) struct PendingFileListAfterIndex {
    pub(super) tab_id: u64,
    pub(super) root: PathBuf,
}

pub(super) struct PendingFileListUseWalkerConfirmation {
    pub(super) source_tab_id: u64,
    pub(super) root: PathBuf,
}

#[derive(Clone, Debug)]
pub(super) struct UpdatePromptState {
    pub(super) candidate: UpdateCandidate,
    pub(super) skip_until_next_version: bool,
    pub(super) install_started: bool,
}

#[derive(Clone, Debug)]
pub(super) struct UpdateCheckFailureState {
    pub(super) error: String,
    pub(super) suppress_future_errors: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FileListDialogKind {
    Overwrite,
    Ancestor,
    UseWalker,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TabDragState {
    pub(super) source_index: usize,
    pub(super) hover_index: usize,
    pub(super) press_pos: egui::Pos2,
    pub(super) dragging: bool,
}

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

pub(super) struct FileListRequestContext {
    pub(super) root: Option<PathBuf>,
    pub(super) tab_id: Option<u64>,
}

pub(super) struct FileListManager {
    workflow: FileListWorkflowState,
}

impl FileListManager {
    pub(super) fn start_request_commands(
        &mut self,
        tab_id: u64,
        root: PathBuf,
        entries: Vec<PathBuf>,
        propagate_to_ancestors: bool,
    ) -> Vec<super::filelist::FileListCommand> {
        let cancel = Arc::new(AtomicBool::new(false));
        let request_id = self.begin_request(tab_id, root.clone(), Arc::clone(&cancel));
        let req = super::FileListRequest {
            request_id,
            tab_id,
            root,
            entries,
            propagate_to_ancestors,
            cancel,
        };
        vec![
            super::filelist::FileListCommand::Ui(
                super::filelist::FileListUiCommand::RefreshStatusLine,
            ),
            super::filelist::FileListCommand::Worker(
                super::filelist::FileListWorkerCommand::Start(req),
            ),
        ]
    }

    pub(super) fn send_failure_commands(&mut self) -> Vec<super::filelist::FileListCommand> {
        self.clear_request();
        vec![
            super::filelist::FileListCommand::Ui(
                super::filelist::FileListUiCommand::RefreshStatusLine,
            ),
            super::filelist::FileListCommand::Ui(
                super::filelist::FileListUiCommand::SetNotice(
                    "Create File List worker is unavailable".to_string(),
                ),
            ),
        ]
    }

    pub(super) fn begin_request(
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

    pub(super) fn clear_request(&mut self) {
        self.workflow.pending_request_id = None;
        self.workflow.pending_request_tab_id = None;
        self.workflow.pending_root = None;
        self.workflow.pending_cancel = None;
        self.workflow.in_progress = false;
        self.workflow.cancel_requested = false;
    }

    pub(super) fn settle_response(
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

    pub(super) fn settle_response_commands(
        &mut self,
        request_id: u64,
    ) -> Option<(FileListRequestContext, Vec<super::filelist::FileListCommand>)> {
        let context = self.settle_response(request_id)?;
        Some((
            context,
            vec![super::filelist::FileListCommand::Ui(
                super::filelist::FileListUiCommand::RefreshStatusLine,
            )],
        ))
    }

    pub(super) fn request_cancel(&mut self) -> Option<Arc<AtomicBool>> {
        if !self.workflow.in_progress || self.workflow.cancel_requested {
            return None;
        }
        self.workflow.cancel_requested = true;
        self.workflow.pending_cancel.as_ref().map(Arc::clone)
    }

    pub(super) fn cancel_stale_pending_confirmation(
        &mut self,
        current_tab_id: u64,
        current_root_key: &str,
    ) -> bool {
        let should_cancel = self
            .workflow
            .pending_confirmation
            .as_ref()
            .is_some_and(|pending| {
                pending.tab_id == current_tab_id
                    && super::FlistWalkerApp::path_key(&pending.root) != current_root_key
            });
        if should_cancel {
            self.workflow.pending_confirmation = None;
        }
        should_cancel
    }

    pub(super) fn cancel_stale_pending_ancestor_confirmation(
        &mut self,
        current_tab_id: u64,
        current_root_key: &str,
    ) -> bool {
        let should_cancel = self
            .workflow
            .pending_ancestor_confirmation
            .as_ref()
            .is_some_and(|pending| {
                pending.tab_id == current_tab_id
                    && super::FlistWalkerApp::path_key(&pending.root) != current_root_key
            });
        if should_cancel {
            self.workflow.pending_ancestor_confirmation = None;
        }
        should_cancel
    }

    pub(super) fn cancel_stale_pending_use_walker_confirmation(
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
                    && super::FlistWalkerApp::path_key(&pending.root) != current_root_key
            });
        if should_cancel {
            self.workflow.pending_use_walker_confirmation = None;
        }
        should_cancel
    }
}

impl Default for FileListManager {
    fn default() -> Self {
        Self {
            workflow: FileListWorkflowState::default(),
        }
    }
}

impl Deref for FileListManager {
    type Target = FileListWorkflowState;

    fn deref(&self) -> &Self::Target {
        &self.workflow
    }
}

impl DerefMut for FileListManager {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.workflow
    }
}

pub(super) struct UpdateState {
    pub(super) next_request_id: u64,
    pub(super) pending_request_id: Option<u64>,
    pub(super) in_progress: bool,
    pub(super) prompt: Option<UpdatePromptState>,
    pub(super) check_failure: Option<UpdateCheckFailureState>,
    pub(super) skipped_target_version: Option<String>,
    pub(super) suppress_check_failure_dialog: bool,
    pub(super) close_requested_for_install: bool,
}

impl Default for UpdateState {
    fn default() -> Self {
        Self {
            next_request_id: 1,
            pending_request_id: None,
            in_progress: false,
            prompt: None,
            check_failure: None,
            skipped_target_version: None,
            suppress_check_failure_dialog: false,
            close_requested_for_install: false,
        }
    }
}

pub(super) struct CacheStateBundle {
    pub(super) preview: PreviewCacheState,
    pub(super) highlight: HighlightCacheState,
    pub(super) sort_metadata: SortMetadataCacheState,
}

pub(super) struct RootBrowserState {
    #[cfg(test)]
    pub(super) browse_dialog_result: Option<Result<Option<PathBuf>, String>>,
    pub(super) saved_roots: Vec<PathBuf>,
    pub(super) default_root: Option<PathBuf>,
}

#[derive(Default)]
pub(super) struct RequestTabRoutingState {
    pub(super) preview: HashMap<u64, u64>,
    pub(super) action: HashMap<u64, u64>,
    pub(super) sort: HashMap<u64, u64>,
}

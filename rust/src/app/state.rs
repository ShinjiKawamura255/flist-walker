use super::*;
use crate::app::cache::{HighlightCacheState, PreviewCacheState, SortMetadataCacheState};
use crate::path_utils::path_key;
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

#[allow(clippy::enum_variant_names)]
pub(super) enum FileListResponseScope {
    CurrentRoot,
    PreviousRoot,
    StaleRequestedRoot,
}

pub(super) struct FileListResponseContext {
    pub(super) tab_id: Option<u64>,
    pub(super) root_scope: FileListResponseScope,
}

#[derive(Default)]
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
            super::filelist::FileListCommand::Ui(super::filelist::FileListUiCommand::SetNotice(
                "Create File List worker is unavailable".to_string(),
            )),
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

    pub(super) fn settle_response(&mut self, request_id: u64) -> Option<FileListRequestContext> {
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

    pub(super) fn settle_response_context_commands(
        &mut self,
        request_id: u64,
        response_root: &Path,
        current_root: &Path,
    ) -> Option<(
        FileListResponseContext,
        Vec<super::filelist::FileListCommand>,
    )> {
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
                pending.tab_id == current_tab_id && path_key(&pending.root) != current_root_key
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
                pending.tab_id == current_tab_id && path_key(&pending.root) != current_root_key
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
                    && path_key(&pending.root) != current_root_key
            });
        if should_cancel {
            self.workflow.pending_use_walker_confirmation = None;
        }
        should_cancel
    }

    pub(super) fn clear_pending_for_tab(&mut self, tab_id: u64) {
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

#[derive(Default)]
pub(super) struct UpdateManager {
    state: UpdateState,
}

impl UpdateManager {
    pub(super) fn from_state(state: UpdateState) -> Self {
        Self { state }
    }

    pub(super) fn request_startup_check_commands(
        &mut self,
        disabled: bool,
    ) -> Vec<super::update::UpdateCommand> {
        if disabled {
            self.clear_for_disabled_update();
            return Vec::new();
        }
        let request_id = self.begin_request();
        vec![
            super::update::UpdateCommand::Worker(super::update::UpdateWorkerCommand::Start(
                super::UpdateRequest {
                    request_id,
                    kind: super::UpdateRequestKind::Check,
                },
            )),
            super::update::UpdateCommand::App(super::update::UpdateAppCommand::AppendWindowTrace {
                event: "update_check_requested",
                details: format!("request_id={request_id}"),
            }),
        ]
    }

    pub(super) fn start_install_commands(
        &mut self,
        current_exe: PathBuf,
    ) -> Result<Vec<super::update::UpdateCommand>, String> {
        let Some(prompt) = self.state.prompt.as_ref() else {
            return Ok(Vec::new());
        };
        if prompt.install_started {
            return Ok(Vec::new());
        }
        let candidate = prompt.candidate.clone();
        if let Some(prompt) = self.state.prompt.as_mut() {
            prompt.install_started = true;
        }
        let request_id = self.begin_request();
        Ok(vec![
            super::update::UpdateCommand::Worker(super::update::UpdateWorkerCommand::Start(
                super::UpdateRequest {
                    request_id,
                    kind: super::UpdateRequestKind::DownloadAndApply {
                        candidate: Box::new(candidate.clone()),
                        current_exe,
                    },
                },
            )),
            super::update::UpdateCommand::Ui(super::update::UpdateUiCommand::SetNotice(format!(
                "Downloading update {}...",
                candidate.target_version
            ))),
            super::update::UpdateCommand::App(super::update::UpdateAppCommand::AppendWindowTrace {
                event: "update_install_requested",
                details: format!(
                    "request_id={request_id} target_version={}",
                    candidate.target_version
                ),
            }),
        ])
    }

    pub(super) fn install_send_failure_commands(&mut self) -> Vec<super::update::UpdateCommand> {
        self.clear_request();
        if let Some(prompt) = self.state.prompt.as_mut() {
            prompt.install_started = false;
        }
        vec![super::update::UpdateCommand::Ui(
            super::update::UpdateUiCommand::SetNotice("Update worker is unavailable".to_string()),
        )]
    }

    pub(super) fn dismiss_prompt(&mut self) {
        self.state.prompt = None;
    }

    pub(super) fn dismiss_check_failure(&mut self) {
        self.state.check_failure = None;
    }

    pub(super) fn set_prompt_skip_until_next_version(&mut self, skip: bool) {
        if let Some(prompt) = self.state.prompt.as_mut() {
            prompt.skip_until_next_version = skip;
        }
    }

    pub(super) fn set_check_failure_suppress_future_errors(&mut self, suppress: bool) {
        if let Some(failure) = self.state.check_failure.as_mut() {
            failure.suppress_future_errors = suppress;
        }
    }

    pub(super) fn suppress_check_failures_commands(&mut self) -> Vec<super::update::UpdateCommand> {
        self.state.suppress_check_failure_dialog = true;
        self.state.check_failure = None;
        vec![
            super::update::UpdateCommand::App(super::update::UpdateAppCommand::MarkUiStateDirty),
            super::update::UpdateCommand::App(super::update::UpdateAppCommand::PersistUiStateNow),
            super::update::UpdateCommand::Ui(super::update::UpdateUiCommand::SetNotice(
                "Startup update check errors will be hidden".to_string(),
            )),
        ]
    }

    pub(super) fn skip_prompt_until_next_version_commands(
        &mut self,
    ) -> Vec<super::update::UpdateCommand> {
        let Some(target_version) = self
            .state
            .prompt
            .as_ref()
            .map(|prompt| prompt.candidate.target_version.clone())
        else {
            return Vec::new();
        };
        self.state.skipped_target_version = Some(target_version.clone());
        self.state.prompt = None;
        vec![
            super::update::UpdateCommand::App(super::update::UpdateAppCommand::MarkUiStateDirty),
            super::update::UpdateCommand::App(super::update::UpdateAppCommand::PersistUiStateNow),
            super::update::UpdateCommand::Ui(super::update::UpdateUiCommand::SetNotice(format!(
                "Update {} hidden until a newer version is available",
                target_version
            ))),
        ]
    }

    pub(super) fn handle_response_commands(
        &mut self,
        response: super::UpdateResponse,
    ) -> Vec<super::update::UpdateCommand> {
        match response {
            super::UpdateResponse::UpToDate { request_id } => {
                if !self.settle_response(request_id) {
                    return Vec::new();
                }
                vec![super::update::UpdateCommand::App(
                    super::update::UpdateAppCommand::AppendWindowTrace {
                        event: "update_up_to_date",
                        details: format!("request_id={request_id}"),
                    },
                )]
            }
            super::UpdateResponse::CheckFailed { request_id, error } => {
                if !self.settle_response(request_id) {
                    return Vec::new();
                }
                let commands = vec![super::update::UpdateCommand::App(
                    super::update::UpdateAppCommand::AppendWindowTrace {
                        event: "update_check_failed",
                        details: format!("request_id={request_id} error={error}"),
                    },
                )];
                if !self.state.suppress_check_failure_dialog
                    || super::forced_update_check_failure_message().is_some()
                {
                    self.state.check_failure = Some(super::UpdateCheckFailureState {
                        error,
                        suppress_future_errors: false,
                    });
                }
                commands
            }
            super::UpdateResponse::Available {
                request_id,
                candidate,
            } => {
                if !self.settle_response(request_id) {
                    return Vec::new();
                }
                let target_version = candidate.target_version.clone();
                if !super::should_skip_update_prompt(
                    &target_version,
                    self.state.skipped_target_version.as_deref(),
                ) {
                    self.state.prompt = Some(super::UpdatePromptState {
                        candidate: *candidate,
                        skip_until_next_version: false,
                        install_started: false,
                    });
                }
                vec![super::update::UpdateCommand::App(
                    super::update::UpdateAppCommand::AppendWindowTrace {
                        event: "update_available",
                        details: format!("request_id={request_id} target_version={target_version}"),
                    },
                )]
            }
            super::UpdateResponse::ApplyStarted {
                request_id,
                target_version,
            } => {
                if !self.settle_response(request_id) {
                    return Vec::new();
                }
                self.state.prompt = None;
                self.state.close_requested_for_install = true;
                vec![
                    super::update::UpdateCommand::Ui(super::update::UpdateUiCommand::SetNotice(
                        format!("Restarting to apply update {}...", target_version),
                    )),
                    super::update::UpdateCommand::App(
                        super::update::UpdateAppCommand::RequestViewportClose,
                    ),
                    super::update::UpdateCommand::App(
                        super::update::UpdateAppCommand::AppendWindowTrace {
                            event: "update_apply_started",
                            details: format!(
                                "request_id={request_id} target_version={target_version}"
                            ),
                        },
                    ),
                ]
            }
            super::UpdateResponse::Failed { request_id, error } => {
                if !self.settle_response(request_id) {
                    return Vec::new();
                }
                let details_error = error.clone();
                if let Some(prompt) = self.state.prompt.as_mut() {
                    prompt.install_started = false;
                }
                vec![
                    super::update::UpdateCommand::Ui(super::update::UpdateUiCommand::SetNotice(
                        error,
                    )),
                    super::update::UpdateCommand::App(
                        super::update::UpdateAppCommand::AppendWindowTrace {
                            event: "update_failed",
                            details: format!("request_id={request_id} error={details_error}"),
                        },
                    ),
                ]
            }
        }
    }

    pub(super) fn clear_request(&mut self) {
        self.state.pending_request_id = None;
        self.state.in_progress = false;
    }

    pub(super) fn clear_for_disabled_update(&mut self) {
        self.clear_request();
    }

    pub(super) fn begin_request(&mut self) -> u64 {
        let request_id = self.state.next_request_id;
        self.state.next_request_id = self.state.next_request_id.saturating_add(1);
        self.state.pending_request_id = Some(request_id);
        self.state.in_progress = true;
        request_id
    }

    pub(super) fn settle_response(&mut self, request_id: u64) -> bool {
        if self.state.pending_request_id != Some(request_id) {
            return false;
        }
        self.clear_request();
        true
    }
}

impl Deref for UpdateManager {
    type Target = UpdateState;

    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl DerefMut for UpdateManager {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}

pub(super) struct CacheStateBundle {
    pub(super) preview: PreviewCacheState,
    pub(super) highlight: HighlightCacheState,
    pub(super) entry_kind: EntryKindCacheState,
    pub(super) sort_metadata: SortMetadataCacheState,
}

pub struct AppRuntimeState {
    pub(super) root: PathBuf,
    pub(super) limit: usize,
    pub(super) query_state: QueryState,
    pub(super) use_filelist: bool,
    pub(super) use_regex: bool,
    pub(super) ignore_case: bool,
    pub(super) include_files: bool,
    pub(super) include_dirs: bool,
    pub(super) index: IndexBuildResult,
    pub(super) all_entries: Arc<Vec<Entry>>,
    pub(super) entries: Arc<Vec<Entry>>,
    pub(super) base_results: Vec<(PathBuf, f64)>,
    pub(super) results: Vec<(PathBuf, f64)>,
    pub(super) result_sort_mode: ResultSortMode,
    pub(super) pinned_paths: HashSet<PathBuf>,
    pub(super) current_row: Option<usize>,
    pub(super) preview: String,
    pub(super) notice: String,
    pub(super) status_line: String,
}

pub struct AppShellState {
    pub(super) runtime: AppRuntimeState,
    pub(super) search: SearchCoordinator,
    pub(super) worker_bus: WorkerBus,
    pub(super) indexing: IndexCoordinator,
    pub(super) ui: RuntimeUiState,
    pub(super) cache: CacheStateBundle,
    pub(super) tabs: TabSessionState,
    pub(super) features: FeatureStateBundle,
    pub(super) worker_runtime: Option<WorkerRuntime>,
}

pub(super) struct RootBrowserState {
    #[cfg(test)]
    pub(super) browse_dialog_result: Option<Result<Option<PathBuf>, String>>,
    pub(super) saved_roots: Vec<PathBuf>,
    pub(super) default_root: Option<PathBuf>,
}

pub(crate) struct FeatureStateBundle {
    pub(super) root_browser: RootBrowserState,
    pub(super) filelist: FileListManager,
    pub(super) update: UpdateManager,
}

#[derive(Default)]
pub(super) struct RequestTabRoutingState {
    pub(super) preview: HashMap<u64, u64>,
    pub(super) action: HashMap<u64, u64>,
    pub(super) sort: HashMap<u64, u64>,
}

impl RequestTabRoutingState {
    pub(super) fn bind_preview(&mut self, request_id: u64, tab_id: u64) {
        self.preview.insert(request_id, tab_id);
    }

    pub(super) fn bind_action(&mut self, request_id: u64, tab_id: u64) {
        self.action.insert(request_id, tab_id);
    }

    pub(super) fn bind_sort(&mut self, request_id: u64, tab_id: u64) {
        self.sort.insert(request_id, tab_id);
    }

    pub(super) fn take_preview(&mut self, request_id: u64) -> Option<u64> {
        self.preview.remove(&request_id)
    }

    pub(super) fn take_action(&mut self, request_id: u64) -> Option<u64> {
        self.action.remove(&request_id)
    }

    pub(super) fn take_sort(&mut self, request_id: u64) -> Option<u64> {
        self.sort.remove(&request_id)
    }

    pub(super) fn clear_preview_for_tab(&mut self, tab_id: u64) {
        self.preview.retain(|_, id| *id != tab_id);
    }

    pub(super) fn clear_action_for_tab(&mut self, tab_id: u64) {
        self.action.retain(|_, id| *id != tab_id);
    }

    pub(super) fn clear_sort_for_tab(&mut self, tab_id: u64) {
        self.sort.retain(|_, id| *id != tab_id);
    }

    #[allow(dead_code)]
    pub(super) fn clear_for_tab(&mut self, tab_id: u64) {
        self.clear_preview_for_tab(tab_id);
        self.clear_action_for_tab(tab_id);
        self.clear_sort_for_tab(tab_id);
    }
}

pub(crate) struct TabSessionState {
    tabs: Vec<AppTabState>,
    pub(super) active_tab: usize,
    next_tab_id: u64,
    pub(super) pending_restore_refresh_tabs: HashSet<u64>,
    request_tab_routing: RequestTabRoutingState,
}

impl Default for TabSessionState {
    fn default() -> Self {
        Self {
            tabs: Vec::new(),
            active_tab: 0,
            next_tab_id: 1,
            pending_restore_refresh_tabs: HashSet::new(),
            request_tab_routing: RequestTabRoutingState::default(),
        }
    }
}

impl TabSessionState {
    pub(super) fn replace_all(&mut self, tabs: Vec<AppTabState>) {
        self.tabs = tabs;
    }

    pub(super) fn active_tab_index(&self) -> usize {
        self.active_tab
    }

    pub(super) fn set_active_tab_index(&mut self, active_tab: usize) {
        self.active_tab = active_tab;
    }

    pub(super) fn take_next_tab_id(&mut self) -> u64 {
        let id = self.next_tab_id;
        self.next_tab_id = self.next_tab_id.saturating_add(1);
        id
    }

    pub(super) fn len(&self) -> usize {
        self.tabs.len()
    }

    #[allow(dead_code)]
    pub(super) fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub(super) fn get(&self, index: usize) -> Option<&AppTabState> {
        self.tabs.get(index)
    }

    pub(super) fn get_mut(&mut self, index: usize) -> Option<&mut AppTabState> {
        self.tabs.get_mut(index)
    }

    pub(super) fn push(&mut self, tab: AppTabState) {
        self.tabs.push(tab);
    }

    pub(super) fn insert(&mut self, index: usize, tab: AppTabState) {
        self.tabs.insert(index, tab);
    }

    pub(super) fn remove(&mut self, index: usize) -> AppTabState {
        self.tabs.remove(index)
    }

    pub(super) fn iter(&self) -> std::slice::Iter<'_, AppTabState> {
        self.tabs.iter()
    }

    pub(super) fn iter_mut(&mut self) -> std::slice::IterMut<'_, AppTabState> {
        self.tabs.iter_mut()
    }

    pub(super) fn mark_pending_restore_refresh_for_tab(&mut self, tab_id: u64) {
        self.pending_restore_refresh_tabs.insert(tab_id);
    }

    pub(super) fn clear_pending_restore_refresh_for_tab(&mut self, tab_id: u64) {
        self.pending_restore_refresh_tabs.remove(&tab_id);
    }

    pub(super) fn clear_pending_restore_refresh_tabs(&mut self) {
        self.pending_restore_refresh_tabs.clear();
    }

    pub(super) fn take_pending_restore_refresh_for_tab(&mut self, tab_id: u64) -> bool {
        self.pending_restore_refresh_tabs.remove(&tab_id)
    }

    pub(super) fn bind_preview_request(&mut self, request_id: u64, tab_id: u64) {
        self.request_tab_routing.bind_preview(request_id, tab_id);
    }

    pub(super) fn take_preview_request_tab(&mut self, request_id: u64) -> Option<u64> {
        self.request_tab_routing.take_preview(request_id)
    }

    #[cfg(test)]
    pub(super) fn preview_request_tab(&self, request_id: u64) -> Option<u64> {
        self.request_tab_routing.preview.get(&request_id).copied()
    }

    pub(super) fn bind_action_request(&mut self, request_id: u64, tab_id: u64) {
        self.request_tab_routing.bind_action(request_id, tab_id);
    }

    pub(super) fn take_action_request_tab(&mut self, request_id: u64) -> Option<u64> {
        self.request_tab_routing.take_action(request_id)
    }

    #[cfg(test)]
    pub(super) fn action_request_tab(&self, request_id: u64) -> Option<u64> {
        self.request_tab_routing.action.get(&request_id).copied()
    }

    pub(super) fn bind_sort_request(&mut self, request_id: u64, tab_id: u64) {
        self.request_tab_routing.bind_sort(request_id, tab_id);
    }

    pub(super) fn take_sort_request_tab(&mut self, request_id: u64) -> Option<u64> {
        self.request_tab_routing.take_sort(request_id)
    }

    #[cfg(test)]
    pub(super) fn sort_request_tab(&self, request_id: u64) -> Option<u64> {
        self.request_tab_routing.sort.get(&request_id).copied()
    }

    pub(super) fn clear_response_routing_for_tab(&mut self, tab_id: u64) {
        self.request_tab_routing.clear_for_tab(tab_id);
    }
}

impl Deref for TabSessionState {
    type Target = [AppTabState];

    fn deref(&self) -> &Self::Target {
        &self.tabs
    }
}

impl DerefMut for TabSessionState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.tabs
    }
}

impl<'a> IntoIterator for &'a TabSessionState {
    type Item = &'a AppTabState;
    type IntoIter = std::slice::Iter<'a, AppTabState>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut TabSessionState {
    type Item = &'a mut AppTabState;
    type IntoIter = std::slice::IterMut<'a, AppTabState>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

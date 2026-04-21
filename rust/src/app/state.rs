use crate::app::cache::{
    EntryKindCacheState, HighlightCacheState, PreviewCacheState, SortMetadataCacheState,
};
use crate::app::index_coordinator::IndexCoordinator;
use crate::app::query_state::QueryState;
use crate::app::search_coordinator::SearchCoordinator;
use crate::app::tab_state::AppTabState;
use crate::app::ui_state::RuntimeUiState;
use crate::app::worker_bus::WorkerBus;
use crate::app::worker_runtime::WorkerRuntime;
use crate::entry::Entry;
use crate::indexer::{IndexBuildResult, IndexSource};
use crate::updater::UpdateCandidate;
use eframe::egui;
use std::collections::{HashMap, HashSet};
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
    pub(super) workflow: FileListWorkflowState,
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
    pub(super) state: UpdateState,
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
    pub(super) ignore_list_terms: Arc<Vec<String>>,
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

impl RootBrowserState {
    pub(super) fn saved_roots(&self) -> &[PathBuf] {
        &self.saved_roots
    }
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

use crate::app::cache::{
    HighlightCacheState, IgnoreMatcherCacheState, PreviewCacheState, SortMetadataCacheState,
};
use crate::app::index_coordinator::IndexCoordinator;
use crate::app::query_state::QueryState;
use crate::app::search_coordinator::SearchCoordinator;
use crate::app::tab_resources::{
    RetiredActiveResources, RetiredTabResources, TabResourceReclaimer,
    TAB_RECENT_INACTIVE_ENGAGEMENT_THRESHOLD, TAB_RECENT_INACTIVE_GRACE,
    TAB_RESOURCE_CACHE_HARD_MAX_COUNT, TAB_RESOURCE_CACHE_HARD_MAX_WEIGHT,
    TAB_RESOURCE_CACHE_MAX_COUNT, TAB_RESOURCE_CACHE_MAX_WEIGHT,
};
use crate::app::tab_state::{AppTabState, TabCommittedPayload};
use crate::app::ui_state::RuntimeUiState;
use crate::app::worker::bus::WorkerBus;
use crate::app::worker::runtime::WorkerRuntime;
use crate::entry::Entry;
use crate::indexer::IndexSource;
pub(super) use crate::search::{
    SearchSortMode as ResultSortMode, SearchSortScope as ResultSortScope,
};
use crate::search_catalog::{PresetEntryType, PresetSortMode, PresetSource, SearchCatalog};
use crate::updater::UpdateCandidate;
use eframe::egui;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Instant, SystemTime};

#[derive(Default)]
pub(super) struct BackgroundIndexState {
    pub(super) source: Option<IndexSource>,
    pub(super) entries: Vec<Entry>,
    pub(super) replaced: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct SortMetadata {
    pub(super) modified: Option<SystemTime>,
    pub(super) created: Option<SystemTime>,
    pub(super) size_bytes: Option<u64>,
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

#[derive(Clone, Debug)]
pub(super) struct PendingFileListIndexCompletionNotice {
    pub(super) tab_id: u64,
    pub(super) root: PathBuf,
    pub(super) notice: String,
}

pub(super) struct PendingFileListUseWalkerConfirmation {
    pub(super) source_tab_id: u64,
    pub(super) root: PathBuf,
}

#[derive(Clone, Debug)]
pub(super) struct PendingActiveIndexFinish {
    pub(super) request_id: u64,
    pub(super) source: IndexSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PendingIndexRefreshMode {
    Normal,
    CreateFileListWalker,
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

#[derive(Clone, Debug)]
pub(super) struct UpdateInstallFailureState {
    pub(super) candidate: Option<UpdateCandidate>,
    pub(super) error: String,
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
    pub(super) pending_index_completion_notices: HashMap<u64, PendingFileListIndexCompletionNotice>,
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
            pending_index_completion_notices: HashMap::new(),
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
    pub(super) install_failure: Option<UpdateInstallFailureState>,
    pub(super) previous_update_failure: Option<String>,
    pub(super) skipped_target_version: Option<String>,
    pub(super) suppress_check_failure_dialog: bool,
    pub(super) close_requested_for_install: bool,
    pub(super) close_after_update_terminal: bool,
    pub(super) active_control: Option<std::sync::Arc<crate::updater::UpdateInstallControl>>,
}

impl Default for UpdateState {
    fn default() -> Self {
        Self {
            next_request_id: 1,
            pending_request_id: None,
            in_progress: false,
            prompt: None,
            check_failure: None,
            install_failure: None,
            previous_update_failure: None,
            skipped_target_version: None,
            suppress_check_failure_dialog: false,
            close_requested_for_install: false,
            close_after_update_terminal: false,
            active_control: None,
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
    pub(super) ignore_matcher: IgnoreMatcherCacheState,
    pub(super) sort_metadata: SortMetadataCacheState,
}

pub(super) struct AppRuntimeState {
    pub(super) root: PathBuf,
    pub(super) limit: usize,
    pub(super) max_depth: crate::indexer::MaxDepth,
    pub(super) query_state: QueryState,
    pub(super) use_filelist: bool,
    pub(super) use_regex: bool,
    pub(super) ignore_case: bool,
    pub(super) ignore_list_terms: Arc<Vec<String>>,
    pub(super) include_files: bool,
    pub(super) include_dirs: bool,
    pub(super) committed: TabCommittedPayload,
    pub(super) result_sort_mode: ResultSortMode,
    pub(super) result_sort_scope: ResultSortScope,
    pub(super) pinned_paths: HashSet<PathBuf>,
    pub(super) evicted_selected_path: Option<PathBuf>,
    pub(super) emacs_keybindings_enabled: bool,
    pub(super) ctrl_w_deletes_word_in_query: bool,
    pub(super) tab_pin_moves_to_next_row: bool,
    pub(super) notice: String,
    pub(super) status_line: String,
}

impl std::ops::Deref for AppRuntimeState {
    type Target = TabCommittedPayload;

    fn deref(&self) -> &Self::Target {
        &self.committed
    }
}

impl std::ops::DerefMut for AppRuntimeState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.committed
    }
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
    #[cfg(test)]
    pub(super) last_browse_dialog_root: Option<PathBuf>,
    pub(super) saved_roots: Vec<PathBuf>,
    pub(super) default_root: Option<PathBuf>,
    pub(super) manage_list: RootListManagerState,
}

impl RootBrowserState {
    pub(super) fn saved_roots(&self) -> &[PathBuf] {
        &self.saved_roots
    }
}

#[derive(Default)]
pub(super) struct RootListManagerState {
    pub(super) open: bool,
    pub(super) input_path: String,
    pub(super) add_error: String,
    pub(super) add_focus_requested: bool,
    pub(super) add_select_all_requested: bool,
    pub(super) draft_roots: Vec<PathBuf>,
    pub(super) draft_default_root: Option<PathBuf>,
    pub(super) selected_index: Option<usize>,
    pub(super) selected_indices: HashSet<usize>,
    pub(super) remove_mode: bool,
    pub(super) editing_index: Option<usize>,
    pub(super) edit_path: String,
    pub(super) edit_error: String,
    pub(super) edit_focus_requested: bool,
    pub(super) edit_select_all_requested: bool,
    pub(super) notice: String,
    pub(super) dialog_generation: u64,
    pub(super) pending_validation_intent: Option<super::RootValidationIntent>,
}

pub(crate) struct FeatureStateBundle {
    pub(super) root_browser: RootBrowserState,
    pub(super) presets: PresetManagerState,
    pub(super) filelist: FileListManager,
    pub(super) update: UpdateManager,
}

#[derive(Default)]
pub(super) struct PresetManagerState {
    pub(super) catalog: SearchCatalog,
    pub(super) picker: PresetPickerState,
}

#[derive(Default)]
pub(super) struct PresetPickerState {
    pub(super) open: bool,
    pub(super) restore_query_focus: bool,
    pub(super) query: String,
    pub(super) matched_catalog_indices: Vec<usize>,
    pub(super) selected_match: Option<usize>,
    pub(super) focus_requested: bool,
    pub(super) error: String,
    pub(super) confirm_delete: bool,
    pub(super) pending_deleted_name: Option<String>,
    pub(super) editor: PresetEditorState,
    pub(super) named_roots: NamedRootManagerState,
}

#[derive(Default)]
pub(super) struct PresetEditorState {
    pub(super) open: bool,
    pub(super) original_name: String,
    pub(super) name: String,
    pub(super) root_name: Option<String>,
    pub(super) root_path: String,
    pub(super) query: String,
    pub(super) entry_type: PresetEntryType,
    pub(super) source: PresetSource,
    pub(super) regex: bool,
    pub(super) ignore_case: bool,
    pub(super) ignore_enabled: bool,
    pub(super) sort: PresetSortMode,
    pub(super) max_depth: crate::indexer::MaxDepth,
    pub(super) extra: BTreeMap<String, serde_json::Value>,
    pub(super) focus_requested: bool,
    pub(super) error: String,
    pub(super) pending_saved_name: Option<String>,
}

#[derive(Default)]
pub(super) struct NamedRootManagerState {
    pub(super) open: bool,
    pub(super) selected_index: Option<usize>,
    pub(super) confirm_delete: bool,
    pub(super) error: String,
    pub(super) editor: NamedRootEditorState,
    pub(super) pending_operation: Option<PendingNamedRootOperation>,
}

#[derive(Default)]
pub(super) struct NamedRootEditorState {
    pub(super) open: bool,
    pub(super) original_name: Option<String>,
    pub(super) name: String,
    pub(super) path: String,
    pub(super) focus_requested: bool,
    pub(super) error: String,
}

pub(super) enum PendingNamedRootOperation {
    Save {
        original_name: Option<String>,
        saved_name: String,
    },
    Delete {
        name: String,
    },
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

    pub(super) fn clear_action(&mut self) {
        self.action.clear();
    }

    pub(super) fn clear_sort_for_tab(&mut self, tab_id: u64) {
        self.sort.retain(|_, id| *id != tab_id);
    }

    pub(super) fn clear_for_tab(&mut self, tab_id: u64) {
        self.clear_preview_for_tab(tab_id);
        self.clear_action_for_tab(tab_id);
        self.clear_sort_for_tab(tab_id);
    }
}

pub(super) struct ClosedTabState {
    pub(super) tab: AppTabState,
    pub(super) original_index: usize,
    pub(super) activation_refresh_pending: bool,
    pub(super) search_refresh_pending: bool,
    pub(super) sort_refresh_pending: bool,
}

#[derive(Clone, Copy, Debug)]
struct ActiveTabEngagement {
    tab_id: u64,
    activated_at: Instant,
    meaningful_interaction: bool,
}

#[derive(Clone, Copy, Debug)]
struct RecentInactiveTab {
    tab_id: u64,
    protected_until: Instant,
}

#[derive(Debug)]
struct RecentInactiveTransition {
    deactivated_engagement: Option<ActiveTabEngagement>,
    candidate: Option<RecentInactiveTab>,
    staged_evictions: Vec<StagedRecentEviction>,
    reclaimer_slot_reserved: bool,
}

#[derive(Debug)]
struct StagedRecentEviction {
    tab_id: u64,
    resources: RetiredTabResources,
}

pub(crate) struct TabSessionState {
    tabs: Vec<AppTabState>,
    pub(super) active_tab: usize,
    next_tab_id: u64,
    closed_tabs: Vec<ClosedTabState>,
    resource_lru: VecDeque<u64>,
    eviction_pending_tabs: HashSet<u64>,
    resource_reclaimer: TabResourceReclaimer,
    request_tab_routing: RequestTabRoutingState,
    pub(super) pending_activation_tab_id: Option<u64>,
    active_tab_engagement: Option<ActiveTabEngagement>,
    recent_inactive: Option<RecentInactiveTab>,
    recent_inactive_transition: Option<RecentInactiveTransition>,
}

impl Default for TabSessionState {
    fn default() -> Self {
        Self {
            tabs: Vec::new(),
            active_tab: 0,
            next_tab_id: 1,
            closed_tabs: Vec::new(),
            resource_lru: VecDeque::new(),
            eviction_pending_tabs: HashSet::new(),
            resource_reclaimer: TabResourceReclaimer::default(),
            request_tab_routing: RequestTabRoutingState::default(),
            pending_activation_tab_id: None,
            active_tab_engagement: None,
            recent_inactive: None,
            recent_inactive_transition: None,
        }
    }
}

impl TabSessionState {
    const CLOSED_TAB_RESTORE_LIMIT: usize = 25;

    pub(super) fn with_resource_reclaimer(resource_reclaimer: TabResourceReclaimer) -> Self {
        Self {
            tabs: Vec::new(),
            active_tab: 0,
            next_tab_id: 1,
            closed_tabs: Vec::new(),
            resource_lru: VecDeque::new(),
            eviction_pending_tabs: HashSet::new(),
            resource_reclaimer,
            request_tab_routing: RequestTabRoutingState::default(),
            pending_activation_tab_id: None,
            active_tab_engagement: None,
            recent_inactive: None,
            recent_inactive_transition: None,
        }
    }

    pub(super) fn replace_all(&mut self, tabs: Vec<AppTabState>) {
        self.tabs = tabs;
        self.active_tab_engagement = None;
        self.recent_inactive = None;
        self.recent_inactive_transition = None;
        self.resource_lru.clear();
        self.eviction_pending_tabs.clear();
    }

    pub(super) fn active_tab_index(&self) -> usize {
        self.active_tab
    }

    pub(super) fn set_active_tab_index(&mut self, active_tab: usize) {
        self.set_active_tab_index_at(active_tab, Instant::now());
    }

    pub(super) fn set_active_tab_index_at(&mut self, active_tab: usize, now: Instant) {
        self.active_tab = active_tab;
        let Some(tab_id) = self.tabs.get(active_tab).map(|tab| tab.id) else {
            self.active_tab_engagement = None;
            return;
        };
        if self
            .active_tab_engagement
            .is_some_and(|engagement| engagement.tab_id == tab_id)
        {
            return;
        }
        if self
            .recent_inactive
            .is_some_and(|recent| recent.tab_id == tab_id)
        {
            self.recent_inactive = None;
        }
        self.active_tab_engagement = Some(ActiveTabEngagement {
            tab_id,
            activated_at: now,
            meaningful_interaction: false,
        });
    }

    pub(super) fn mark_active_tab_meaningfully_engaged(&mut self) {
        let active_tab_id = self.tabs.get(self.active_tab).map(|tab| tab.id);
        if let Some(engagement) = self.active_tab_engagement.as_mut() {
            if Some(engagement.tab_id) == active_tab_id {
                engagement.meaningful_interaction = true;
            }
        }
    }

    #[cfg(test)]
    pub(super) fn active_tab_meaningfully_engaged_for_test(&self) -> bool {
        let active_tab_id = self.tabs.get(self.active_tab).map(|tab| tab.id);
        self.active_tab_engagement.is_some_and(|engagement| {
            Some(engagement.tab_id) == active_tab_id && engagement.meaningful_interaction
        })
    }

    pub(super) fn record_active_tab_deactivation_at(&mut self, tab_id: u64, now: Instant) {
        let engagement = self
            .active_tab_engagement
            .take()
            .filter(|engagement| engagement.tab_id == tab_id);
        let qualifies = engagement.is_some_and(|engagement| {
            engagement.meaningful_interaction
                || now.saturating_duration_since(engagement.activated_at)
                    >= TAB_RECENT_INACTIVE_ENGAGEMENT_THRESHOLD
        });
        let has_reusable_snapshot = self.tabs.iter().any(|tab| {
            tab.id == tab_id
                && tab.index_state.committed_snapshot_present()
                && tab.heavy_resource_weight() > 0
        });
        let candidate = (qualifies && has_reusable_snapshot).then_some(RecentInactiveTab {
            tab_id,
            protected_until: now + TAB_RECENT_INACTIVE_GRACE,
        });
        self.recent_inactive_transition = Some(RecentInactiveTransition {
            deactivated_engagement: engagement,
            candidate,
            staged_evictions: Vec::new(),
            reclaimer_slot_reserved: false,
        });
    }

    pub(super) fn commit_recent_inactive_transition(&mut self) -> bool {
        self.finish_recent_inactive_transition(true)
    }

    pub(super) fn discard_recent_inactive_transition(&mut self) -> bool {
        self.finish_recent_inactive_transition(false)
    }

    pub(super) fn rollback_recent_inactive_transition(&mut self) {
        let Some(transition) = self.recent_inactive_transition.take() else {
            return;
        };
        if transition.reclaimer_slot_reserved {
            self.resource_reclaimer.release_reserved_slot();
        }
        for staged in transition.staged_evictions {
            if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == staged.tab_id) {
                tab.restore_heavy_resources(staged.resources);
            }
        }
        self.active_tab_engagement = transition.deactivated_engagement;
    }

    fn finish_recent_inactive_transition(&mut self, commit_candidate: bool) -> bool {
        let Some(transition) = self.recent_inactive_transition.take() else {
            return true;
        };
        let staged_ids = transition
            .staged_evictions
            .iter()
            .map(|staged| staged.tab_id)
            .collect::<Vec<_>>();
        if transition.reclaimer_slot_reserved {
            let resources = transition
                .staged_evictions
                .into_iter()
                .map(|staged| staged.resources)
                .collect::<Vec<_>>();
            if let Err(resources) = self.resource_reclaimer.try_retire_reserved_tabs(resources) {
                for (tab_id, resources) in staged_ids.into_iter().zip(resources) {
                    if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == tab_id) {
                        tab.restore_heavy_resources(resources);
                    }
                }
                self.active_tab_engagement = transition.deactivated_engagement;
                return false;
            }
            for tab_id in &staged_ids {
                self.remove_resource_tracking(*tab_id);
            }
        }
        if commit_candidate {
            if let Some(candidate) = transition
                .candidate
                .filter(|candidate| !staged_ids.contains(&candidate.tab_id))
            {
                self.recent_inactive = Some(candidate);
            }
        }
        true
    }

    #[cfg(test)]
    pub(super) fn recent_inactive_tab_id(&self) -> Option<u64> {
        self.recent_inactive.map(|recent| recent.tab_id)
    }

    pub(super) fn take_next_tab_id(&mut self) -> u64 {
        let id = self.next_tab_id;
        self.next_tab_id = self.next_tab_id.saturating_add(1);
        id
    }

    pub(super) fn len(&self) -> usize {
        self.tabs.len()
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

    pub(super) fn try_prepare_closed_history_slot(&mut self) -> bool {
        if self.closed_tabs.len() < Self::CLOSED_TAB_RESTORE_LIMIT {
            return true;
        }
        let Some(expired) = self.closed_tabs.first_mut() else {
            return true;
        };
        if expired.tab.heavy_resource_weight() == 0 {
            return true;
        }
        let expired_id = expired.tab.id;
        let resources = expired.tab.take_heavy_resources();
        match self.resource_reclaimer.try_retire(resources) {
            Ok(()) => {
                self.remove_resource_tracking(expired_id);
                self.closed_tabs[0].activation_refresh_pending = true;
                true
            }
            Err(resources) => {
                self.closed_tabs[0].tab.restore_heavy_resources(*resources);
                self.eviction_pending_tabs.insert(expired_id);
                false
            }
        }
    }

    pub(super) fn push_closed_tab(&mut self, closed_tab: ClosedTabState) {
        if self.closed_tabs.len() >= Self::CLOSED_TAB_RESTORE_LIMIT {
            let expired = self.closed_tabs.remove(0);
            debug_assert_eq!(expired.tab.heavy_resource_weight(), 0);
            self.remove_resource_tracking(expired.tab.id);
        }
        self.closed_tabs.push(closed_tab);
    }

    pub(super) fn pop_closed_tab(&mut self) -> Option<ClosedTabState> {
        let closed = self.closed_tabs.pop()?;
        self.remove_resource_tracking(closed.tab.id);
        Some(closed)
    }

    pub(super) fn last_closed_tab_id(&self) -> Option<u64> {
        self.closed_tabs.last().map(|closed| closed.tab.id)
    }

    pub(super) fn touch_heavy_resource(&mut self, tab_id: u64) {
        self.remove_resource_tracking(tab_id);
        self.resource_lru.push_back(tab_id);
    }

    pub(super) fn remove_resource_tracking(&mut self, tab_id: u64) {
        self.resource_lru.retain(|id| *id != tab_id);
        self.eviction_pending_tabs.remove(&tab_id);
        if self
            .recent_inactive
            .is_some_and(|recent| recent.tab_id == tab_id)
        {
            self.recent_inactive = None;
        }
        if let Some(transition) = self.recent_inactive_transition.as_mut() {
            if transition
                .candidate
                .is_some_and(|recent| recent.tab_id == tab_id)
            {
                transition.candidate = None;
            }
        }
    }

    pub(super) fn enforce_resource_budget(
        &mut self,
        active_tab_id: Option<u64>,
        warm_tab_id: Option<u64>,
    ) -> bool {
        self.enforce_resource_budget_at(active_tab_id, warm_tab_id, Instant::now())
    }

    pub(super) fn enforce_resource_budget_at(
        &mut self,
        active_tab_id: Option<u64>,
        warm_tab_id: Option<u64>,
        now: Instant,
    ) -> bool {
        let is_valid_recent = |recent: RecentInactiveTab| {
            now < recent.protected_until
                && self.tabs.iter().any(|tab| {
                    tab.id == recent.tab_id
                        && tab.index_state.committed_snapshot_present()
                        && tab.heavy_resource_weight() > 0
                })
        };
        if let Some(transition) = self.recent_inactive_transition.as_mut() {
            if transition
                .candidate
                .is_some_and(|recent| !is_valid_recent(recent))
            {
                transition.candidate = None;
            }
        }
        if self
            .recent_inactive
            .is_some_and(|recent| !is_valid_recent(recent))
        {
            self.recent_inactive = None;
        }
        let protected_pending_recent = self
            .recent_inactive_transition
            .as_ref()
            .and_then(|transition| transition.candidate)
            .map(|candidate| candidate.tab_id)
            .filter(|tab_id| Some(*tab_id) != active_tab_id && Some(*tab_id) != warm_tab_id);
        let protected_existing_recent = self
            .recent_inactive
            .map(|recent| recent.tab_id)
            .filter(|tab_id| Some(*tab_id) != active_tab_id && Some(*tab_id) != warm_tab_id);
        loop {
            let cached = self
                .tabs
                .iter()
                .filter(|tab| {
                    Some(tab.id) != active_tab_id
                        && Some(tab.id) != warm_tab_id
                        && tab.heavy_resource_weight() > 0
                })
                .map(|tab| tab.heavy_resource_weight())
                .chain(
                    self.closed_tabs
                        .iter()
                        .filter(|closed| {
                            Some(closed.tab.id) != active_tab_id
                                && Some(closed.tab.id) != warm_tab_id
                                && closed.tab.heavy_resource_weight() > 0
                        })
                        .map(|closed| closed.tab.heavy_resource_weight()),
                )
                .collect::<Vec<_>>();
            let count = cached.len();
            let weight = cached
                .iter()
                .copied()
                .fold(0usize, |total, weight| total.saturating_add(weight));
            let cold_cached = self
                .tabs
                .iter()
                .filter(|tab| {
                    Some(tab.id) != active_tab_id
                        && Some(tab.id) != warm_tab_id
                        && Some(tab.id) != protected_pending_recent
                        && Some(tab.id) != protected_existing_recent
                        && tab.heavy_resource_weight() > 0
                })
                .map(|tab| tab.heavy_resource_weight())
                .chain(
                    self.closed_tabs
                        .iter()
                        .filter(|closed| {
                            Some(closed.tab.id) != active_tab_id
                                && Some(closed.tab.id) != warm_tab_id
                                && closed.tab.heavy_resource_weight() > 0
                        })
                        .map(|closed| closed.tab.heavy_resource_weight()),
                )
                .collect::<Vec<_>>();
            let cold_count = cold_cached.len();
            let cold_weight = cold_cached
                .into_iter()
                .fold(0usize, |total, weight| total.saturating_add(weight));
            let hard_pressure = count > TAB_RESOURCE_CACHE_HARD_MAX_COUNT
                || weight > TAB_RESOURCE_CACHE_HARD_MAX_WEIGHT;
            let soft_pressure = cold_count > TAB_RESOURCE_CACHE_MAX_COUNT
                || cold_weight > TAB_RESOURCE_CACHE_MAX_WEIGHT;
            if !hard_pressure && !soft_pressure {
                return true;
            }

            let candidate_is_eligible = |tab_id: &u64| {
                Some(*tab_id) != active_tab_id
                    && Some(*tab_id) != warm_tab_id
                    && self
                        .tabs
                        .iter()
                        .find(|tab| tab.id == *tab_id)
                        .is_none_or(|tab| {
                            tab.index_state.pending_index_finish.is_none()
                                && !tab.index_state.build_reclaim_pending
                        })
                    && self
                        .closed_tabs
                        .iter()
                        .find(|closed| closed.tab.id == *tab_id)
                        .is_none_or(|closed| !closed.tab.index_state.build_reclaim_pending)
            };
            let ordinary_candidate = self.resource_lru.iter().copied().find(|tab_id| {
                Some(*tab_id) != protected_pending_recent
                    && Some(*tab_id) != protected_existing_recent
                    && candidate_is_eligible(tab_id)
            });
            let protected_candidate = hard_pressure
                .then(|| {
                    protected_existing_recent
                        .filter(|tab_id| candidate_is_eligible(tab_id))
                        .filter(|tab_id| {
                            self.tabs
                                .iter()
                                .any(|tab| tab.id == *tab_id && tab.heavy_resource_weight() > 0)
                        })
                        .or_else(|| {
                            protected_pending_recent
                                .filter(|tab_id| candidate_is_eligible(tab_id))
                                .filter(|tab_id| {
                                    self.tabs.iter().any(|tab| {
                                        tab.id == *tab_id && tab.heavy_resource_weight() > 0
                                    })
                                })
                        })
                })
                .flatten();
            if ordinary_candidate.is_none() {
                if let (Some(candidate), Some(transition)) = (
                    protected_candidate,
                    self.recent_inactive_transition.as_ref(),
                ) {
                    if !transition.reclaimer_slot_reserved
                        && !self.resource_reclaimer.try_reserve_slot()
                    {
                        return false;
                    }
                    let resources = self
                        .tabs
                        .iter_mut()
                        .find(|tab| tab.id == candidate)
                        .map(AppTabState::take_heavy_resources);
                    let Some(resources) = resources else {
                        if !transition.reclaimer_slot_reserved {
                            self.resource_reclaimer.release_reserved_slot();
                        }
                        return false;
                    };
                    let transition = self
                        .recent_inactive_transition
                        .as_mut()
                        .expect("transition checked before staging protected eviction");
                    transition.reclaimer_slot_reserved = true;
                    transition.staged_evictions.push(StagedRecentEviction {
                        tab_id: candidate,
                        resources,
                    });
                    continue;
                }
            }
            let candidate = if hard_pressure {
                let existing_candidate = if self.recent_inactive_transition.is_none() {
                    protected_existing_recent.filter(|tab_id| candidate_is_eligible(tab_id))
                } else {
                    None
                };
                ordinary_candidate.or(existing_candidate)
            } else {
                ordinary_candidate
            };
            let Some(candidate) = candidate else {
                return false;
            };

            let mut closed_candidate = false;
            let resources = if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == candidate)
            {
                Some(tab.take_heavy_resources())
            } else if let Some(closed) = self
                .closed_tabs
                .iter_mut()
                .find(|closed| closed.tab.id == candidate)
            {
                closed_candidate = true;
                Some(closed.tab.take_heavy_resources())
            } else {
                None
            };
            let Some(resources) = resources else {
                self.remove_resource_tracking(candidate);
                continue;
            };

            match self.resource_reclaimer.try_retire(resources) {
                Ok(()) => {
                    self.remove_resource_tracking(candidate);
                    if closed_candidate {
                        if let Some(closed) = self
                            .closed_tabs
                            .iter_mut()
                            .find(|closed| closed.tab.id == candidate)
                        {
                            closed.activation_refresh_pending = true;
                        }
                    }
                }
                Err(resources) => {
                    if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == candidate) {
                        tab.restore_heavy_resources(*resources);
                    } else if let Some(closed) = self
                        .closed_tabs
                        .iter_mut()
                        .find(|closed| closed.tab.id == candidate)
                    {
                        closed.tab.restore_heavy_resources(*resources);
                    }
                    self.eviction_pending_tabs.insert(candidate);
                    return false;
                }
            }
        }
    }

    pub(super) fn try_retire_active_resources(
        &self,
        resources: RetiredActiveResources,
    ) -> Result<(), RetiredActiveResources> {
        self.resource_reclaimer.try_retire_active(resources)
    }

    pub(super) fn try_retire_tab_resources(
        &self,
        resources: RetiredTabResources,
    ) -> Result<(), Box<RetiredTabResources>> {
        self.resource_reclaimer.try_retire(resources)
    }

    pub(super) fn try_retire_index_build_resources(
        &self,
        resources: crate::app::tab_resources::RetiredIndexBuildResources,
    ) -> Result<(), Box<crate::app::tab_resources::RetiredIndexBuildResources>> {
        self.resource_reclaimer.try_retire_index_build(resources)
    }

    pub(super) fn take_all_heavy_resources_for_shutdown(&mut self) -> Vec<RetiredTabResources> {
        let mut resources = Vec::new();
        for tab in &mut self.tabs {
            let payload = tab.take_heavy_resources();
            if !payload.is_empty() {
                resources.push(payload);
            }
        }
        for closed in &mut self.closed_tabs {
            let payload = closed.tab.take_heavy_resources();
            if !payload.is_empty() {
                resources.push(payload);
            }
        }
        resources
    }

    pub(super) fn disconnect_resource_reclaimer(&mut self) {
        self.resource_reclaimer.disconnect();
    }

    #[cfg(test)]
    pub(super) fn pause_resource_reclaimer(&mut self) {
        self.resource_reclaimer = TabResourceReclaimer::paused_for_test();
    }

    #[cfg(test)]
    pub(super) fn resume_resource_reclaimer(&mut self) {
        self.resource_reclaimer = TabResourceReclaimer::default();
    }

    #[cfg(test)]
    pub(super) fn retire_tab_resources_for_test(
        &self,
        resources: crate::app::tab_resources::RetiredTabResources,
    ) -> Result<(), Box<crate::app::tab_resources::RetiredTabResources>> {
        self.resource_reclaimer.try_retire(resources)
    }

    #[cfg(test)]
    pub(super) fn cached_heavy_resource_count(
        &self,
        active_tab_id: Option<u64>,
        warm_tab_id: Option<u64>,
    ) -> usize {
        self.tabs
            .iter()
            .filter(|tab| {
                Some(tab.id) != active_tab_id
                    && Some(tab.id) != warm_tab_id
                    && tab.heavy_resource_weight() > 0
            })
            .count()
            + self
                .closed_tabs
                .iter()
                .filter(|closed| closed.tab.heavy_resource_weight() > 0)
                .count()
    }

    #[cfg(test)]
    pub(super) fn reclaimer_pending(&self) -> usize {
        self.resource_reclaimer.pending()
    }

    #[cfg(test)]
    pub(super) fn last_closed_tab_results_compacted(&self) -> Option<bool> {
        self.closed_tabs
            .last()
            .map(|closed| closed.tab.result_state.results_compacted)
    }

    #[cfg(test)]
    pub(super) fn closed_tab_count(&self) -> usize {
        self.closed_tabs.len()
    }

    #[cfg(test)]
    pub(super) fn seed_oldest_closed_snapshot(&mut self, entry: Entry) {
        let closed = self.closed_tabs.first_mut().expect("closed tab fixture");
        closed.tab.index_state.set_resource_state_for_test(
            super::tab_state::TabResourceState::new(super::TabResourceLifecycle::Ready, true),
        );
        closed.tab.result_state.committed.all_entries = Arc::new(vec![entry.clone()]);
        closed.tab.result_state.committed.entries = Arc::new(vec![entry]);
        let tab_id = closed.tab.id;
        self.touch_heavy_resource(tab_id);
    }

    pub(super) fn iter(&self) -> std::slice::Iter<'_, AppTabState> {
        self.tabs.iter()
    }

    pub(super) fn iter_mut(&mut self) -> std::slice::IterMut<'_, AppTabState> {
        self.tabs.iter_mut()
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

    pub(super) fn clear_preview_response_routing_for_tab(&mut self, tab_id: u64) {
        self.request_tab_routing.clear_preview_for_tab(tab_id);
    }

    pub(super) fn clear_action_request_routing(&mut self) {
        self.request_tab_routing.clear_action();
    }

    #[cfg(test)]
    pub(super) fn routed_tab_ids_for_test(&self) -> Vec<u64> {
        let mut tab_ids = self
            .request_tab_routing
            .preview
            .values()
            .chain(self.request_tab_routing.action.values())
            .chain(self.request_tab_routing.sort.values())
            .copied()
            .collect::<Vec<_>>();
        tab_ids.sort_unstable();
        tab_ids
    }

    #[cfg(test)]
    pub(super) fn routed_requests_for_test(&self) -> Vec<(u8, u64, u64)> {
        let mut routes = self
            .request_tab_routing
            .preview
            .iter()
            .map(|(request_id, tab_id)| (0, *request_id, *tab_id))
            .chain(
                self.request_tab_routing
                    .action
                    .iter()
                    .map(|(request_id, tab_id)| (1, *request_id, *tab_id)),
            )
            .chain(
                self.request_tab_routing
                    .sort
                    .iter()
                    .map(|(request_id, tab_id)| (2, *request_id, *tab_id)),
            )
            .collect::<Vec<_>>();
        routes.sort_unstable();
        routes
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

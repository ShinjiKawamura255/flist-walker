use super::*;
use crate::path_utils::normalize_windows_path_buf;

// Phase 1 scaffolding for the root-change reducer split. Later phases will
// move root-change-specific state transitions behind an orchestrator that emits
// these commands instead of mutating FlistWalkerApp directly from mod.rs.
#[allow(dead_code)]
pub(super) enum RootChangeUiCommand {
    SetNotice(String),
}

#[allow(dead_code)]
pub(super) enum RootChangePipelineCommand {
    RequestIndexRefresh,
}

#[allow(dead_code)]
pub(super) enum RootChangeAppCommand {
    MarkUiStateDirty,
}

#[allow(dead_code)]
pub(super) enum RootChangeCommand {
    Ui(RootChangeUiCommand),
    Pipeline(RootChangePipelineCommand),
    App(RootChangeAppCommand),
}

// Phase 1 scaffolding for the shared tab-lifecycle split. Later phases will
// move the common deactivate/activate ordering behind helpers that emit these
// commands instead of open-coding the lifecycle steps in each call site.
#[allow(dead_code)]
pub(super) enum TabLifecycleUiCommand {
    FocusQuery,
}

#[allow(dead_code)]
pub(super) enum TabLifecyclePipelineCommand {
    TriggerRestoreRefresh,
}

#[allow(dead_code)]
pub(super) enum TabLifecycleAppCommand {
    ClearTabDragState,
}

#[allow(dead_code)]
pub(super) enum TabLifecycleCommand {
    Ui(TabLifecycleUiCommand),
    Pipeline(TabLifecyclePipelineCommand),
    App(TabLifecycleAppCommand),
}

// Phase 1 scaffolding for the tab-activation/background-restore split. Later
// phases will move restore-decision and activation-time lazy refresh handling
// behind a dedicated helper that emits these commands instead of open-coding
// pending_restore_refresh transitions across tabs.rs and pipeline.rs.
#[allow(dead_code)]
pub(super) enum TabRestorePipelineCommand {
    RequestIndexRefresh,
}

#[allow(dead_code)]
pub(super) enum TabRestoreAppCommand {
    ConsumePendingRestoreRefresh,
}

#[allow(dead_code)]
pub(super) enum TabRestoreCommand {
    Pipeline(TabRestorePipelineCommand),
    App(TabRestoreAppCommand),
}

// Phase 1 scaffolding for the tab-close cleanup split. Later phases will move
// close-specific pending/routing cleanup behind a dedicated helper that emits
// these commands instead of open-coding subsystem cleanup in close_tab_index().
#[allow(dead_code)]
pub(super) enum TabCloseCleanupAppCommand {
    ClearFileListPendingForTab(u64),
    ClearIndexRoutingForTab(u64),
    ClearSearchRoutingForTab(u64),
    ClearRequestRoutingForTab(u64),
    InvalidateMemorySample,
}

#[allow(dead_code)]
pub(super) enum TabCloseCleanupCommand {
    App(TabCloseCleanupAppCommand),
}

impl FlistWalkerApp {
    #[allow(dead_code)]
    fn dispatch_tab_restore_commands(&mut self, commands: Vec<TabRestoreCommand>) {
        for command in commands {
            match command {
                TabRestoreCommand::Pipeline(TabRestorePipelineCommand::RequestIndexRefresh) => {
                    self.request_index_refresh();
                }
                TabRestoreCommand::App(TabRestoreAppCommand::ConsumePendingRestoreRefresh) => {
                    self.pending_restore_refresh = false;
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        tab.pending_restore_refresh = false;
                    }
                }
            }
        }
    }

    #[allow(dead_code)]
    fn dispatch_tab_close_cleanup_commands(&mut self, commands: Vec<TabCloseCleanupCommand>) {
        for command in commands {
            match command {
                TabCloseCleanupCommand::App(
                    TabCloseCleanupAppCommand::ClearFileListPendingForTab(tab_id),
                ) => {
                    if self
                        .filelist_state
                        .pending_after_index
                        .as_ref()
                        .is_some_and(|pending| pending.tab_id == tab_id)
                    {
                        self.filelist_state.pending_after_index = None;
                    }
                    if self
                        .filelist_state
                        .pending_confirmation
                        .as_ref()
                        .is_some_and(|pending| pending.tab_id == tab_id)
                    {
                        self.filelist_state.pending_confirmation = None;
                    }
                    if self
                        .filelist_state
                        .pending_ancestor_confirmation
                        .as_ref()
                        .is_some_and(|pending| pending.tab_id == tab_id)
                    {
                        self.filelist_state.pending_ancestor_confirmation = None;
                    }
                    if self
                        .filelist_state
                        .pending_use_walker_confirmation
                        .as_ref()
                        .is_some_and(|pending| pending.source_tab_id == tab_id)
                    {
                        self.filelist_state.pending_use_walker_confirmation = None;
                    }
                }
                TabCloseCleanupCommand::App(
                    TabCloseCleanupAppCommand::ClearIndexRoutingForTab(tab_id),
                ) => {
                    self.indexing.request_tabs.retain(|_, id| *id != tab_id);
                    self.indexing.pending_queue.retain(|req| req.tab_id != tab_id);
                    if let Ok(mut latest) = self.indexing.latest_request_ids.lock() {
                        latest.remove(&tab_id);
                    }
                    self.indexing
                        .background_states
                        .retain(|request_id, _| self.indexing.request_tabs.contains_key(request_id));
                }
                TabCloseCleanupCommand::App(
                    TabCloseCleanupAppCommand::ClearSearchRoutingForTab(tab_id),
                ) => {
                    self.search.retain_request_tabs(|_, id| *id != tab_id);
                }
                TabCloseCleanupCommand::App(
                    TabCloseCleanupAppCommand::ClearRequestRoutingForTab(tab_id),
                ) => {
                    self.request_tab_routing.preview.retain(|_, id| *id != tab_id);
                    self.request_tab_routing.action.retain(|_, id| *id != tab_id);
                    self.request_tab_routing.sort.retain(|_, id| *id != tab_id);
                }
                TabCloseCleanupCommand::App(TabCloseCleanupAppCommand::InvalidateMemorySample) => {
                    self.ui.memory_usage_bytes = None;
                }
            }
        }
    }

    fn dispatch_tab_lifecycle_commands(&mut self, commands: Vec<TabLifecycleCommand>) {
        for command in commands {
            match command {
                TabLifecycleCommand::Ui(TabLifecycleUiCommand::FocusQuery) => {
                    self.ui.focus_query_requested = true;
                    self.ui.unfocus_query_requested = false;
                }
                TabLifecycleCommand::Pipeline(
                    TabLifecyclePipelineCommand::TriggerRestoreRefresh,
                ) => {
                    self.dispatch_tab_restore_for_activation(true);
                }
                TabLifecycleCommand::App(TabLifecycleAppCommand::ClearTabDragState) => {
                    self.ui.tab_drag_state = None;
                }
            }
        }
    }

    fn tab_restore_commands_for_activation(
        &self,
        trigger_restore_refresh: bool,
    ) -> Vec<TabRestoreCommand> {
        if !trigger_restore_refresh || !self.pending_restore_refresh {
            return Vec::new();
        }
        vec![
            TabRestoreCommand::App(TabRestoreAppCommand::ConsumePendingRestoreRefresh),
            TabRestoreCommand::Pipeline(TabRestorePipelineCommand::RequestIndexRefresh),
        ]
    }

    fn dispatch_tab_restore_for_activation(&mut self, trigger_restore_refresh: bool) {
        let commands = self.tab_restore_commands_for_activation(trigger_restore_refresh);
        if commands.is_empty() {
            return;
        }
        self.dispatch_tab_restore_commands(commands);
    }

    fn deactivate_active_tab_for_transition(&mut self) -> usize {
        self.dispatch_tab_lifecycle_commands(vec![TabLifecycleCommand::App(
            TabLifecycleAppCommand::ClearTabDragState,
        )]);
        self.shrink_checkpoint_buffers();
        let previous_active = self.active_tab;
        self.sync_active_tab_state();
        if let Some(previous_tab) = self.tabs.get_mut(previous_active) {
            Self::compact_inactive_tab_state(previous_tab);
        }
        previous_active
    }

    fn activate_tab_after_transition(
        &mut self,
        tab: &AppTabState,
        restore_results: bool,
        request_focus: bool,
        trigger_restore_refresh: bool,
    ) {
        self.apply_tab_state(tab);
        if restore_results {
            self.restore_results_from_compacted_tab();
        }
        let mut commands = Vec::new();
        if request_focus {
            commands.push(TabLifecycleCommand::Ui(TabLifecycleUiCommand::FocusQuery));
        }
        if trigger_restore_refresh {
            commands.push(TabLifecycleCommand::Pipeline(
                TabLifecyclePipelineCommand::TriggerRestoreRefresh,
            ));
        }
        if !commands.is_empty() {
            self.dispatch_tab_lifecycle_commands(commands);
        }
    }

    pub(super) fn dispatch_root_change_commands(&mut self, commands: Vec<RootChangeCommand>) {
        for command in commands {
            match command {
                RootChangeCommand::Ui(RootChangeUiCommand::SetNotice(notice)) => {
                    self.set_notice(notice);
                }
                RootChangeCommand::Pipeline(RootChangePipelineCommand::RequestIndexRefresh) => {
                    self.request_index_refresh();
                }
                RootChangeCommand::App(RootChangeAppCommand::MarkUiStateDirty) => {
                    self.mark_ui_state_dirty();
                }
            }
        }
    }

    pub(super) fn root_change_commands(&mut self, new_root: PathBuf) -> Vec<RootChangeCommand> {
        let normalized = normalize_windows_path_buf(new_root);
        if Self::path_key(&normalized) == Self::path_key(&self.root) {
            return Vec::new();
        }

        self.root = normalized;
        self.reset_query_history_navigation();
        self.query_state.query_history_dirty_since = None;
        self.reset_history_search_state();
        // Avoid launching/copying stale selections from the previous root.
        self.pinned_paths.clear();
        self.current_row = None;
        self.preview.clear();
        self.worker_bus.preview.in_progress = false;
        self.worker_bus.preview.pending_request_id = None;
        self.clear_root_scoped_entry_state();
        self.sync_active_tab_state();
        self.cancel_stale_pending_filelist_confirmation();
        self.cancel_stale_pending_filelist_ancestor_confirmation();
        self.cancel_stale_pending_filelist_use_walker_confirmation();

        vec![
            RootChangeCommand::App(RootChangeAppCommand::MarkUiStateDirty),
            RootChangeCommand::Pipeline(RootChangePipelineCommand::RequestIndexRefresh),
            RootChangeCommand::Ui(RootChangeUiCommand::SetNotice(format!(
                "Root changed: {}",
                self.root_display_text()
            ))),
        ]
    }

    pub(super) fn choose_startup_root(
        root: PathBuf,
        root_explicit: bool,
        restore_tabs_enabled: bool,
        restore_session: Option<&(Vec<SavedTabState>, usize)>,
        last_root: Option<PathBuf>,
        default_root: Option<PathBuf>,
    ) -> PathBuf {
        if root_explicit {
            return root;
        }
        if let Some((tabs, active_tab)) = restore_session {
            if let Some(tab_root) = tabs.get(*active_tab).map(|tab| PathBuf::from(&tab.root)) {
                return tab_root;
            }
        }
        if restore_tabs_enabled {
            last_root.or(default_root).unwrap_or(root)
        } else {
            default_root.or(last_root).unwrap_or(root)
        }
    }

    pub(super) fn initialize_tabs(&mut self) {
        let id = self.next_tab_id;
        self.next_tab_id = self.next_tab_id.saturating_add(1);
        self.tabs = vec![self.capture_active_tab_state(id)];
        self.active_tab = 0;
    }

    pub(super) fn restored_tab_state(&self, id: u64, saved: &SavedTabState) -> AppTabState {
        AppTabState {
            id,
            root: normalize_windows_path_buf(PathBuf::from(&saved.root)),
            tab_accent: saved.tab_accent,
            use_filelist: saved.use_filelist,
            use_regex: saved.use_regex,
            ignore_case: saved.ignore_case,
            include_files: saved.include_files,
            include_dirs: saved.include_dirs,
            index_state: TabIndexState {
                index: IndexBuildResult {
                    entries: Vec::new(),
                    source: IndexSource::None,
                },
                all_entries: Arc::new(Vec::new()),
                entries: Arc::new(Vec::new()),
                pending_index_request_id: None,
                index_in_progress: false,
                pending_index_entries: VecDeque::new(),
                pending_index_entries_request_id: None,
                pending_kind_paths: VecDeque::new(),
                pending_kind_paths_set: HashSet::new(),
                in_flight_kind_paths: HashSet::new(),
                kind_resolution_epoch: 1,
                kind_resolution_in_progress: false,
                incremental_filtered_entries: Vec::new(),
                last_incremental_results_refresh: Instant::now(),
                last_search_snapshot_len: 0,
                search_resume_pending: false,
                search_rerun_pending: false,
            },
            query_state: TabQueryState {
                query: saved.query.clone(),
                query_history: self.query_state.query_history.clone(),
                query_history_cursor: None,
                query_history_draft: None,
                query_history_dirty_since: None,
                history_search_active: false,
                history_search_query: String::new(),
                history_search_original_query: String::new(),
                history_search_results: Vec::new(),
                history_search_current: None,
            },
            pending_restore_refresh: true,
            result_state: TabResultState {
                base_results: Vec::new(),
                results: Vec::new(),
                result_sort_mode: ResultSortMode::Score,
                pending_sort_request_id: None,
                sort_in_progress: false,
                pinned_paths: HashSet::new(),
                current_row: Some(0),
                preview: String::new(),
                results_compacted: false,
            },
            notice: "Restored tab".to_string(),
            pending_request_id: None,
            pending_preview_request_id: None,
            pending_action_request_id: None,
            search_in_progress: false,
            preview_in_progress: false,
            action_in_progress: false,
            scroll_to_current: true,
            focus_query_requested: false,
            unfocus_query_requested: false,
        }
    }

    pub(super) fn initialize_tabs_from_saved(
        &mut self,
        saved_tabs: Vec<SavedTabState>,
        active_tab: usize,
    ) {
        self.tabs = saved_tabs
            .iter()
            .map(|saved| {
                let id = self.next_tab_id;
                self.next_tab_id = self.next_tab_id.saturating_add(1);
                self.restored_tab_state(id, saved)
            })
            .collect();
        self.active_tab = active_tab.min(self.tabs.len().saturating_sub(1));
        if let Some(tab) = self.tabs.get(self.active_tab).cloned() {
            self.apply_tab_state(&tab);
            self.ui.focus_query_requested = true;
            self.ui.unfocus_query_requested = false;
            self.dispatch_tab_restore_for_activation(true);
            self.notice = "Restored tab session".to_string();
            self.refresh_status_line();
        }
    }

    pub(super) fn current_tab_id(&self) -> Option<u64> {
        self.tabs.get(self.active_tab).map(|tab| tab.id)
    }

    fn shrink_vec_if_sparse<T>(vec: &mut Vec<T>) {
        let cap = vec.capacity();
        let len = vec.len();
        if cap >= Self::SHRINK_MIN_CAPACITY && cap > len.saturating_mul(2) {
            vec.shrink_to_fit();
        }
    }

    fn shrink_deque_if_sparse<T>(deque: &mut VecDeque<T>) {
        let cap = deque.capacity();
        let len = deque.len();
        if cap >= Self::SHRINK_MIN_CAPACITY && cap > len.saturating_mul(2) {
            deque.shrink_to_fit();
        }
    }

    pub(super) fn shrink_checkpoint_buffers(&mut self) {
        Self::shrink_vec_if_sparse(&mut self.index.entries);
        Self::shrink_vec_if_sparse(&mut self.indexing.incremental_filtered_entries);
        Self::shrink_deque_if_sparse(&mut self.indexing.pending_entries);
        Self::shrink_deque_if_sparse(&mut self.indexing.pending_kind_paths);
    }

    pub(super) fn shrink_tab_checkpoint_buffers(tab: &mut AppTabState) {
        Self::shrink_vec_if_sparse(&mut tab.index_state.index.entries);
        Self::shrink_vec_if_sparse(&mut tab.index_state.incremental_filtered_entries);
        Self::shrink_deque_if_sparse(&mut tab.index_state.pending_index_entries);
        Self::shrink_deque_if_sparse(&mut tab.index_state.pending_kind_paths);
    }

    pub(super) fn compact_inactive_tab_state(tab: &mut AppTabState) {
        let can_compact_results = !tab.index_state.index_in_progress
            && !tab.search_in_progress
            && !tab.result_state.sort_in_progress
            && tab.pending_request_id.is_none()
            && tab.result_state.pending_sort_request_id.is_none();
        if can_compact_results && !tab.result_state.results.is_empty() {
            tab.result_state.results.clear();
            tab.result_state.results.shrink_to_fit();
            tab.result_state.results_compacted = true;
        }
        if !tab.preview_in_progress {
            tab.result_state.preview.clear();
        }
        Self::shrink_tab_checkpoint_buffers(tab);
    }

    pub(super) fn restore_results_from_compacted_tab(&mut self) {
        let was_compacted = self
            .tabs
            .get(self.active_tab)
            .map(|tab| tab.result_state.results_compacted)
            .unwrap_or(false);
        if !was_compacted {
            return;
        }
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.result_state.results_compacted = false;
        }

        if self.base_results.is_empty() {
            if self.query_state.query.trim().is_empty() {
                let results = self
                    .entries
                    .iter()
                    .take(self.limit)
                    .cloned()
                    .map(|entry| (entry.path, 0.0))
                    .collect();
                self.replace_results_snapshot(results, true);
            } else {
                self.refresh_status_line();
            }
            return;
        }

        if self.result_sort_mode == ResultSortMode::Score {
            self.apply_results_with_selection_policy(self.base_results.clone(), true, false);
        } else {
            self.apply_result_sort(true);
        }
    }

    pub(super) fn capture_active_tab_state(&self, id: u64) -> AppTabState {
        AppTabState {
            id,
            root: self.root.clone(),
            tab_accent: self
                .tabs
                .get(self.active_tab)
                .and_then(|tab| tab.tab_accent),
            use_filelist: self.use_filelist,
            use_regex: self.use_regex,
            ignore_case: self.ignore_case,
            include_files: self.include_files,
            include_dirs: self.include_dirs,
            index_state: TabIndexState {
                index: self.index.clone(),
                all_entries: Arc::clone(&self.all_entries),
                entries: Arc::clone(&self.entries),
                pending_index_request_id: self.indexing.pending_request_id,
                index_in_progress: self.indexing.in_progress,
                pending_index_entries: self.indexing.pending_entries.clone(),
                pending_index_entries_request_id: self.indexing.pending_entries_request_id,
                pending_kind_paths: self.indexing.pending_kind_paths.clone(),
                pending_kind_paths_set: self.indexing.pending_kind_paths_set.clone(),
                in_flight_kind_paths: self.indexing.in_flight_kind_paths.clone(),
                kind_resolution_epoch: self.indexing.kind_resolution_epoch,
                kind_resolution_in_progress: self.indexing.kind_resolution_in_progress,
                incremental_filtered_entries: self.indexing.incremental_filtered_entries.clone(),
                last_incremental_results_refresh: self.indexing.last_incremental_results_refresh,
                last_search_snapshot_len: self.indexing.last_search_snapshot_len,
                search_resume_pending: self.indexing.search_resume_pending,
                search_rerun_pending: self.indexing.search_rerun_pending,
            },
            query_state: TabQueryState {
                query: self.query_state.query.clone(),
                query_history: self.query_state.query_history.clone(),
                query_history_cursor: self.query_state.query_history_cursor,
                query_history_draft: self.query_state.query_history_draft.clone(),
                query_history_dirty_since: self.query_state.query_history_dirty_since,
                history_search_active: self.query_state.history_search_active,
                history_search_query: self.query_state.history_search_query.clone(),
                history_search_original_query: self.query_state.history_search_original_query.clone(),
                history_search_results: self.query_state.history_search_results.clone(),
                history_search_current: self.query_state.history_search_current,
            },
            pending_restore_refresh: self.pending_restore_refresh,
            result_state: TabResultState {
                base_results: self.base_results.clone(),
                results: self.results.clone(),
                result_sort_mode: self.result_sort_mode,
                pending_sort_request_id: self.worker_bus.sort.pending_request_id,
                sort_in_progress: self.worker_bus.sort.in_progress,
                pinned_paths: self.pinned_paths.clone(),
                current_row: self.current_row,
                preview: self.preview.clone(),
                results_compacted: false,
            },
            notice: self.notice.clone(),
            pending_request_id: self.search.pending_request_id(),
            pending_preview_request_id: self.worker_bus.preview.pending_request_id,
            pending_action_request_id: self.worker_bus.action.pending_request_id,
            search_in_progress: self.search.in_progress(),
            preview_in_progress: self.worker_bus.preview.in_progress,
            action_in_progress: self.worker_bus.action.in_progress,
            scroll_to_current: self.ui.scroll_to_current,
            focus_query_requested: self.ui.focus_query_requested,
            unfocus_query_requested: self.ui.unfocus_query_requested,
        }
    }

    pub(super) fn apply_tab_state(&mut self, tab: &AppTabState) {
        self.root = tab.root.clone();
        self.use_filelist = tab.use_filelist;
        self.use_regex = tab.use_regex;
        self.ignore_case = tab.ignore_case;
        self.include_files = tab.include_files;
        self.include_dirs = tab.include_dirs;
        self.index = tab.index_state.index.clone();
        self.all_entries = Arc::clone(&tab.index_state.all_entries);
        self.entries = Arc::clone(&tab.index_state.entries);
        self.indexing.pending_request_id = tab.index_state.pending_index_request_id;
        self.indexing.in_progress = tab.index_state.index_in_progress;
        self.indexing.pending_entries = tab.index_state.pending_index_entries.clone();
        self.indexing.pending_entries_request_id = tab.index_state.pending_index_entries_request_id;
        self.indexing.pending_kind_paths = tab.index_state.pending_kind_paths.clone();
        self.indexing.pending_kind_paths_set = tab.index_state.pending_kind_paths_set.clone();
        self.indexing.in_flight_kind_paths = tab.index_state.in_flight_kind_paths.clone();
        self.indexing.kind_resolution_epoch = tab.index_state.kind_resolution_epoch;
        self.indexing.kind_resolution_in_progress = tab.index_state.kind_resolution_in_progress;
        self.indexing.incremental_filtered_entries = tab.index_state.incremental_filtered_entries.clone();
        self.indexing.last_incremental_results_refresh = tab.index_state.last_incremental_results_refresh;
        self.indexing.last_search_snapshot_len = tab.index_state.last_search_snapshot_len;
        self.indexing.search_resume_pending = tab.index_state.search_resume_pending;
        self.indexing.search_rerun_pending = tab.index_state.search_rerun_pending;
        self.query_state.query = tab.query_state.query.clone();
        self.reset_query_history_navigation();
        self.query_state.query_history_dirty_since = None;
        self.reset_history_search_state();
        self.pending_restore_refresh = tab.pending_restore_refresh;
        self.base_results = tab.result_state.base_results.clone();
        self.results = tab.result_state.results.clone();
        self.result_sort_mode = tab.result_state.result_sort_mode;
        self.worker_bus.sort.pending_request_id = tab.result_state.pending_sort_request_id;
        self.worker_bus.sort.in_progress = tab.result_state.sort_in_progress;
        self.pinned_paths = tab.result_state.pinned_paths.clone();
        self.current_row = tab.result_state.current_row;
        self.preview = tab.result_state.preview.clone();
        self.notice = tab.notice.clone();
        self.search.set_pending_request_id(tab.pending_request_id);
        self.worker_bus.preview.pending_request_id = tab.pending_preview_request_id;
        self.worker_bus.action.pending_request_id = tab.pending_action_request_id;
        self.search.set_in_progress(tab.search_in_progress);
        self.worker_bus.preview.in_progress = tab.preview_in_progress;
        self.worker_bus.action.in_progress = tab.action_in_progress;
        self.ui.scroll_to_current = tab.scroll_to_current;
        self.ui.focus_query_requested = tab.focus_query_requested;
        self.ui.unfocus_query_requested = tab.unfocus_query_requested;
        self.refresh_status_line();
    }

    pub(super) fn sync_active_tab_state(&mut self) {
        let Some(id) = self.tabs.get(self.active_tab).map(|tab| tab.id) else {
            return;
        };
        self.commit_query_history_if_needed(true);
        let snapshot = self.capture_active_tab_state(id);
        if let Some(slot) = self.tabs.get_mut(self.active_tab) {
            *slot = snapshot;
        }
    }

    pub(super) fn find_tab_index_by_id(&self, tab_id: u64) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id == tab_id)
    }

    pub(super) fn switch_to_tab_index(&mut self, next_index: usize) {
        if next_index >= self.tabs.len() || next_index == self.active_tab {
            return;
        }
        self.deactivate_active_tab_for_transition();
        if let Some(next_tab) = self.tabs.get_mut(next_index) {
            Self::shrink_tab_checkpoint_buffers(next_tab);
        }
        self.active_tab = next_index;
        if let Some(tab) = self.tabs.get(next_index).cloned() {
            self.activate_tab_after_transition(&tab, true, true, true);
        }
    }

    pub(super) fn set_tab_accent(&mut self, index: usize, accent: Option<TabAccentColor>) {
        let Some(tab) = self.tabs.get_mut(index) else {
            return;
        };
        if tab.tab_accent == accent {
            return;
        }
        tab.tab_accent = accent;
        self.mark_ui_state_dirty();
        self.persist_ui_state_now();
    }

    pub(super) fn create_new_tab(&mut self) {
        self.deactivate_active_tab_for_transition();
        let id = self.next_tab_id;
        self.next_tab_id = self.next_tab_id.saturating_add(1);
        let mut tab = self.capture_active_tab_state(id);
        tab.tab_accent = None;
        tab.use_filelist = true;
        tab.query_state.query.clear();
        tab.query_state.query_history = self.query_state.query_history.clone();
        tab.query_state.query_history_cursor = None;
        tab.query_state.query_history_draft = None;
        tab.query_state.query_history_dirty_since = None;
        tab.query_state.history_search_active = false;
        tab.query_state.history_search_query.clear();
        tab.query_state.history_search_original_query.clear();
        tab.query_state.history_search_results.clear();
        tab.query_state.history_search_current = None;
        tab.pending_restore_refresh = false;
        tab.result_state.base_results = self
            .entries
            .iter()
            .take(self.limit)
            .cloned()
            .map(|entry| (entry.path, 0.0))
            .collect();
        tab.result_state.result_sort_mode = ResultSortMode::Score;
        tab.result_state.pending_sort_request_id = None;
        tab.result_state.sort_in_progress = false;
        tab.result_state.pinned_paths.clear();
        tab.result_state.current_row = None;
        tab.result_state.preview.clear();
        tab.result_state.results_compacted = false;
        tab.notice = "Opened new tab".to_string();
        tab.pending_request_id = None;
        tab.pending_preview_request_id = None;
        tab.pending_action_request_id = None;
        tab.index_state.pending_index_request_id = None;
        tab.search_in_progress = false;
        tab.index_state.index_in_progress = false;
        tab.preview_in_progress = false;
        tab.action_in_progress = false;
        tab.index_state.pending_index_entries.clear();
        tab.index_state.pending_index_entries_request_id = None;
        tab.index_state.pending_kind_paths.clear();
        tab.index_state.pending_kind_paths_set.clear();
        tab.index_state.in_flight_kind_paths.clear();
        tab.index_state.kind_resolution_in_progress = false;
        tab.index_state.kind_resolution_epoch = 1;
        tab.index_state.incremental_filtered_entries.clear();
        tab.index_state.last_search_snapshot_len = tab.index_state.entries.len();
        tab.index_state.last_incremental_results_refresh = Instant::now();
        tab.index_state.search_resume_pending = false;
        tab.index_state.search_rerun_pending = false;
        tab.scroll_to_current = true;
        tab.focus_query_requested = true;
        tab.unfocus_query_requested = false;
        tab.result_state.results = tab.result_state.base_results.clone();
        self.tabs.push(tab.clone());
        self.active_tab = self.tabs.len().saturating_sub(1);
        self.activate_tab_after_transition(&tab, false, true, false);
    }

    pub(super) fn close_active_tab(&mut self) {
        self.close_tab_index(self.active_tab);
    }

    pub(super) fn close_tab_index(&mut self, index: usize) {
        if self.tabs.len() <= 1 || index >= self.tabs.len() {
            if self.tabs.len() <= 1 {
                self.set_notice("Cannot close the last tab");
            }
            return;
        }
        self.ui.tab_drag_state = None;
        self.sync_active_tab_state();
        let removed = self.tabs.remove(index);
        if self
            .filelist_state
            .pending_after_index
            .as_ref()
            .is_some_and(|pending| pending.tab_id == removed.id)
        {
            self.filelist_state.pending_after_index = None;
        }
        if self
            .filelist_state
            .pending_confirmation
            .as_ref()
            .is_some_and(|pending| pending.tab_id == removed.id)
        {
            self.filelist_state.pending_confirmation = None;
        }
        if self
            .filelist_state
            .pending_ancestor_confirmation
            .as_ref()
            .is_some_and(|pending| pending.tab_id == removed.id)
        {
            self.filelist_state.pending_ancestor_confirmation = None;
        }
        if self
            .filelist_state
            .pending_use_walker_confirmation
            .as_ref()
            .is_some_and(|pending| pending.source_tab_id == removed.id)
        {
            self.filelist_state.pending_use_walker_confirmation = None;
        }
        self.indexing.request_tabs
            .retain(|_, tab_id| *tab_id != removed.id);
        self.indexing.pending_queue
            .retain(|req| req.tab_id != removed.id);
        if let Ok(mut latest) = self.indexing.latest_request_ids.lock() {
            latest.remove(&removed.id);
        }
        self.indexing.background_states
            .retain(|request_id, _| self.indexing.request_tabs.contains_key(request_id));
        self.search.retain_request_tabs(|_, tab_id| *tab_id != removed.id);
        self.request_tab_routing
            .preview
            .retain(|_, tab_id| *tab_id != removed.id);
        self.request_tab_routing
            .action
            .retain(|_, tab_id| *tab_id != removed.id);
        self.request_tab_routing
            .sort
            .retain(|_, tab_id| *tab_id != removed.id);
        if index < self.active_tab {
            self.active_tab = self.active_tab.saturating_sub(1);
        }
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len().saturating_sub(1);
        }
        self.ui.memory_usage_bytes = None;
        if let Some(tab) = self.tabs.get(self.active_tab).cloned() {
            self.activate_tab_after_transition(&tab, false, false, false);
        }
    }

    pub(super) fn move_tab(&mut self, from_index: usize, to_index: usize) {
        if from_index >= self.tabs.len() || to_index >= self.tabs.len() || from_index == to_index {
            return;
        }
        self.ui.tab_drag_state = None;
        self.sync_active_tab_state();
        let Some(active_tab_id) = self.tabs.get(self.active_tab).map(|tab| tab.id) else {
            return;
        };
        let moved = self.tabs.remove(from_index);
        self.tabs.insert(to_index, moved);
        if let Some(new_active) = self.find_tab_index_by_id(active_tab_id) {
            self.active_tab = new_active;
        }
        if let Some(tab) = self.tabs.get(self.active_tab).cloned() {
            self.apply_tab_state(&tab);
        }
    }

    pub(super) fn activate_next_tab(&mut self) {
        if self.tabs.len() <= 1 {
            return;
        }
        let next = (self.active_tab + 1) % self.tabs.len();
        self.switch_to_tab_index(next);
    }

    pub(super) fn activate_previous_tab(&mut self) {
        if self.tabs.len() <= 1 {
            return;
        }
        let next = if self.active_tab == 0 {
            self.tabs.len() - 1
        } else {
            self.active_tab - 1
        };
        self.switch_to_tab_index(next);
    }

    pub(super) fn activate_tab_shortcut(&mut self, shortcut_number: usize) {
        let Some(target_index) = shortcut_number.checked_sub(1) else {
            return;
        };
        if target_index >= self.tabs.len() || target_index >= 9 {
            return;
        }
        self.switch_to_tab_index(target_index);
    }

    pub(super) fn tab_root_label(root: &Path) -> String {
        let normalized = normalize_windows_path_buf(root.to_path_buf());
        let raw = normalized.to_string_lossy().to_string();
        let trimmed = raw.trim_end_matches(['/', '\\']);
        if trimmed.is_empty() {
            return "/".to_string();
        }
        if trimmed.len() == 2 && trimmed.as_bytes().get(1) == Some(&b':') {
            return trimmed.to_string();
        }

        if let Some(name) = normalized.file_name().and_then(|s| s.to_str()) {
            if !name.is_empty() {
                return name.to_string();
            }
        }
        raw
    }

    pub(super) fn tab_title(&self, tab: &AppTabState, _index: usize) -> String {
        Self::tab_root_label(&tab.root)
    }

    pub(super) fn any_tab_async_in_progress(&self) -> bool {
        self.tabs.iter().any(|tab| {
            tab.search_in_progress
                || tab.preview_in_progress
                || tab.action_in_progress
                || tab.index_state.index_in_progress
                || tab.result_state.sort_in_progress
        })
    }

    pub(super) fn saved_tab_state_from_app(&self) -> SavedTabState {
        SavedTabState {
            root: self.root.to_string_lossy().to_string(),
            use_filelist: self.use_filelist,
            use_regex: self.use_regex,
            ignore_case: self.ignore_case,
            include_files: self.include_files,
            include_dirs: self.include_dirs,
            query: self.query_state.query.clone(),
            query_history: if Self::history_persist_disabled() {
                Vec::new()
            } else {
                self.query_state.query_history.iter().cloned().collect()
            },
            tab_accent: self
                .tabs
                .get(self.active_tab)
                .and_then(|tab| tab.tab_accent),
        }
    }

    fn saved_tab_state_from_tab(tab: &AppTabState) -> SavedTabState {
        SavedTabState {
            root: tab.root.to_string_lossy().to_string(),
            use_filelist: tab.use_filelist,
            use_regex: tab.use_regex,
            ignore_case: tab.ignore_case,
            include_files: tab.include_files,
            include_dirs: tab.include_dirs,
            query: tab.query_state.query.clone(),
            query_history: if Self::history_persist_disabled() {
                Vec::new()
            } else {
                tab.query_state.query_history.iter().cloned().collect()
            },
            tab_accent: tab.tab_accent,
        }
    }

    pub(super) fn saved_tabs_for_ui_state(&self) -> Vec<SavedTabState> {
        self.tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                if index == self.active_tab {
                    self.saved_tab_state_from_app()
                } else {
                    Self::saved_tab_state_from_tab(tab)
                }
            })
            .collect()
    }
}

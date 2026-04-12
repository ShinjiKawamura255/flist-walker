use super::*;
use crate::path_utils::normalize_windows_path_buf;
use crate::path_utils::path_key;

pub(super) struct BackgroundIndexResponseEffect {
    pub(super) trigger_search: bool,
    pub(super) cleanup_request_id: Option<u64>,
    pub(super) deferred_filelist: Option<(u64, PathBuf, Vec<PathBuf>)>,
}

impl FlistWalkerApp {
    /// root 切り替えに伴う state reset と再 index をまとめて適用する。
    pub(super) fn apply_root_change(&mut self, new_root: PathBuf) {
        self.apply_root_change_direct(new_root);
    }
    fn settle_background_tab_index_failure(tab: &mut AppTabState, notice: Option<String>) {
        tab.index_state.pending_index_request_id = None;
        tab.index_state.index_in_progress = false;
        tab.index_state.pending_index_entries.clear();
        tab.index_state.pending_index_entries_request_id = None;
        tab.index_state.search_resume_pending = false;
        tab.index_state.search_rerun_pending = false;
        tab.pending_restore_refresh = false;
        if let Some(notice) = notice {
            tab.notice = notice;
        } else if tab.notice.is_empty() {
            tab.notice = "Indexing canceled".to_string();
        }
    }

    pub(super) fn apply_background_search_response(
        &mut self,
        tab_id: u64,
        response: SearchResponse,
    ) {
        result_reducer::apply_background_search_response(self, tab_id, response);
    }

    fn clear_tab_drag_state(&mut self) {
        self.ui.tab_drag_state = None;
    }

    fn trigger_pending_restore_refresh(&mut self) {
        if !self.tabs.pending_restore_refresh {
            return;
        }
        self.tabs.pending_restore_refresh = false;
        let active_tab = self.tabs.active_tab;
        if let Some(tab) = self.tabs.get_mut(active_tab) {
            tab.pending_restore_refresh = false;
        }
        self.request_index_refresh();
    }

    fn activate_background_tab_after_transition(&mut self, tab: &AppTabState) {
        self.activate_tab_after_transition(tab, true, true, true);
    }

    fn clear_closed_tab_state(&mut self, tab_id: u64) {
        self.features.filelist.clear_pending_for_tab(tab_id);
        self.indexing.clear_for_tab(tab_id);
        self.search.clear_for_tab(tab_id);
        self.clear_response_routing_for_tab(tab_id);
        self.ui.memory_usage_bytes = None;
    }

    fn reapply_active_tab_state(&mut self) {
        if let Some(tab) = self.tabs.get(self.tabs.active_tab).cloned() {
            self.apply_tab_state(&tab);
        }
    }

    fn deactivate_active_tab_for_transition(&mut self) -> usize {
        self.clear_tab_drag_state();
        self.shrink_checkpoint_buffers();
        let previous_active = self.tabs.active_tab;
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
        if request_focus {
            self.ui.focus_query_requested = true;
            self.ui.unfocus_query_requested = false;
        }
        if trigger_restore_refresh {
            self.trigger_pending_restore_refresh();
        }
    }

    pub(super) fn apply_background_index_response(
        &mut self,
        tab_index: usize,
        msg: IndexResponse,
    ) -> BackgroundIndexResponseEffect {
        let limit = self.runtime.limit;
        let shell = &mut self.shell;
        let (tabs, indexing, features) = (&mut shell.tabs, &mut shell.indexing, &mut shell.features);
        let mut effect = BackgroundIndexResponseEffect {
            trigger_search: false,
            cleanup_request_id: None,
            deferred_filelist: None,
        };

        let Some(tab) = tabs.get_mut(tab_index) else {
            return effect;
        };

        match msg {
            IndexResponse::Started { request_id, source } => {
                if tab.index_state.pending_index_request_id != Some(request_id) {
                    return effect;
                }
                tab.index_state.index.source = source.clone();
                indexing.background_states.entry(request_id).or_default().source = Some(source);
            }
            IndexResponse::Batch {
                request_id,
                entries,
            } => {
                if tab.index_state.pending_index_request_id != Some(request_id) {
                    return effect;
                }
                let state = indexing.background_states.entry(request_id).or_default();
                for entry in entries {
                    state.entries.push(entry.into());
                }
            }
            IndexResponse::ReplaceAll {
                request_id,
                entries,
            } => {
                if tab.index_state.pending_index_request_id != Some(request_id) {
                    return effect;
                }
                let state = indexing.background_states.entry(request_id).or_default();
                state.entries.clear();
                for entry in entries {
                    state.entries.push(entry.into());
                }
            }
            IndexResponse::Finished { request_id, source } => {
                if tab.index_state.pending_index_request_id != Some(request_id) {
                    effect.cleanup_request_id = Some(request_id);
                    return effect;
                }
                let state = indexing.background_states.remove(&request_id).unwrap_or_default();
                tab.index_state.index.source = state.source.unwrap_or(source);
                tab.index_state.index.entries.clear();
                tab.index_state.all_entries = Arc::new(state.entries);
                if tab.include_files && tab.include_dirs {
                    tab.index_state.entries = Arc::clone(&tab.index_state.all_entries);
                } else {
                    let filtered: Vec<Entry> = tab
                        .index_state
                        .all_entries
                        .iter()
                        .filter(|entry| {
                            Self::is_entry_visible_for_flags(
                                entry,
                                tab.include_files,
                                tab.include_dirs,
                            )
                        })
                        .cloned()
                        .collect();
                    tab.index_state.entries = Arc::new(filtered);
                }
                tab.index_state.pending_index_request_id = None;
                tab.index_state.index_in_progress = false;
                tab.index_state.pending_index_entries.clear();
                tab.index_state.pending_index_entries_request_id = None;
                tab.index_state.search_resume_pending = false;
                tab.index_state.search_rerun_pending = false;
                tab.index_state.last_search_snapshot_len = tab.index_state.entries.len();
                tab.index_state.last_incremental_results_refresh = Instant::now();
                if matches!(tab.index_state.index.source, IndexSource::Walker) {
                    for entry in tab.index_state.all_entries.iter() {
                        if entry.kind.is_none()
                            && !tab.index_state.pending_kind_paths_set.contains(&entry.path)
                            && !tab.index_state.in_flight_kind_paths.contains(&entry.path)
                        {
                            tab.index_state
                                .pending_kind_paths_set
                                .insert(entry.path.clone());
                            tab.index_state
                                .pending_kind_paths
                                .push_back(entry.path.clone());
                        }
                    }
                    tab.index_state.kind_resolution_in_progress =
                        !tab.index_state.pending_kind_paths.is_empty()
                            || !tab.index_state.in_flight_kind_paths.is_empty();
                } else {
                    tab.index_state.pending_kind_paths.clear();
                    tab.index_state.pending_kind_paths_set.clear();
                    tab.index_state.in_flight_kind_paths.clear();
                    tab.index_state.kind_resolution_in_progress = false;
                }
                let pending_after_index_matches = features
                    .filelist
                    .pending_after_index
                    .as_ref()
                    .is_some_and(|pending| {
                        pending.tab_id == tab.id && path_key(&pending.root) == path_key(&tab.root)
                    });
                if pending_after_index_matches {
                    effect.deferred_filelist = Some((
                        tab.id,
                        tab.root.clone(),
                        tab.index_state
                            .all_entries
                            .iter()
                            .map(|entry| entry.path.clone())
                            .collect(),
                    ));
                    features.filelist.pending_after_index = None;
                }

                if tab.query_state.query.trim().is_empty() {
                    tab.result_state.results = tab
                        .index_state
                        .entries
                        .iter()
                        .take(limit)
                        .cloned()
                        .map(|entry| (entry.path, 0.0))
                        .collect();
                    if tab.result_state.results.is_empty() {
                        tab.result_state.current_row = None;
                        tab.result_state.preview.clear();
                        tab.pending_preview_request_id = None;
                        tab.preview_in_progress = false;
                    } else {
                        let max_index = tab.result_state.results.len().saturating_sub(1);
                        tab.result_state.current_row =
                            Some(tab.result_state.current_row.unwrap_or(0).min(max_index));
                    }
                } else {
                    effect.trigger_search = true;
                }
                Self::shrink_tab_checkpoint_buffers(tab);
                effect.cleanup_request_id = Some(request_id);
            }
            IndexResponse::Failed { request_id, error } => {
                if tab.index_state.pending_index_request_id != Some(request_id) {
                    effect.cleanup_request_id = Some(request_id);
                    return effect;
                }
                Self::settle_background_tab_index_failure(
                    tab,
                    Some(format!("Indexing failed: {}", error)),
                );
                effect.cleanup_request_id = Some(request_id);
            }
            IndexResponse::Canceled { request_id } => {
                if tab.index_state.pending_index_request_id == Some(request_id) {
                    Self::settle_background_tab_index_failure(tab, None);
                }
                effect.cleanup_request_id = Some(request_id);
            }
            IndexResponse::Truncated { request_id, limit } => {
                if tab.index_state.pending_index_request_id == Some(request_id) {
                    tab.notice = format!(
                        "Walker capped at {} entries (set FLISTWALKER_WALKER_MAX_ENTRIES to adjust)",
                        limit
                    );
                }
            }
        }

        effect
    }
    pub(super) fn apply_root_change_direct(&mut self, new_root: PathBuf) {
        let normalized = normalize_windows_path_buf(new_root);
        if path_key(&normalized) == path_key(&self.runtime.root) {
            return;
        }

        self.runtime.root = normalized;
        self.reset_query_history_navigation();
        self.runtime.query_state.query_history_dirty_since = None;
        self.reset_history_search_state();
        // Avoid launching/copying stale selections from the previous root.
        self.runtime.pinned_paths.clear();
        self.runtime.current_row = None;
        self.runtime.preview.clear();
        self.worker_bus.preview.in_progress = false;
        self.worker_bus.preview.pending_request_id = None;
        self.clear_root_scoped_entry_state();
        self.sync_active_tab_state();
        self.cancel_stale_pending_filelist_confirmations_for_active_root();
        self.mark_ui_state_dirty();
        self.request_index_refresh();
        self.set_notice(format!("Root changed: {}", self.root_display_text()));
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
        let id = self.tabs.next_tab_id;
        self.tabs.next_tab_id = self.tabs.next_tab_id.saturating_add(1);
        *self.tabs = vec![self.capture_active_tab_state(id)];
        self.tabs.active_tab = 0;
    }

    pub(super) fn restored_tab_state(&self, id: u64, saved: &SavedTabState) -> AppTabState {
        AppTabState::from_saved(self, id, saved)
    }

    pub(super) fn initialize_tabs_from_saved(
        &mut self,
        saved_tabs: Vec<SavedTabState>,
        active_tab: usize,
    ) {
        *self.tabs = saved_tabs
            .iter()
            .map(|saved| {
                let id = self.tabs.next_tab_id;
                self.tabs.next_tab_id = self.tabs.next_tab_id.saturating_add(1);
                self.restored_tab_state(id, saved)
            })
            .collect();
        self.tabs.active_tab = active_tab.min(self.tabs.len().saturating_sub(1));
        if let Some(tab) = self.tabs.get(self.tabs.active_tab).cloned() {
            self.apply_tab_state(&tab);
            self.ui.focus_query_requested = true;
            self.ui.unfocus_query_requested = false;
            self.trigger_pending_restore_refresh();
            self.runtime.notice = "Restored tab session".to_string();
            self.refresh_status_line();
        }
    }

    pub(super) fn current_tab_id(&self) -> Option<u64> {
        self.tabs.get(self.tabs.active_tab).map(|tab| tab.id)
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
        Self::shrink_vec_if_sparse(&mut self.runtime.index.entries);
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
            .get(self.tabs.active_tab)
            .map(|tab| tab.result_state.results_compacted)
            .unwrap_or(false);
        if !was_compacted {
            return;
        }
        let active_tab = self.tabs.active_tab;
        if let Some(tab) = self.tabs.get_mut(active_tab) {
            tab.result_state.results_compacted = false;
        }

        if self.runtime.base_results.is_empty() {
            if self.runtime.query_state.query.trim().is_empty() {
                let results = self
                    .runtime
                    .entries
                    .iter()
                    .take(self.runtime.limit)
                    .cloned()
                    .map(|entry| (entry.path, 0.0))
                    .collect();
                self.replace_results_snapshot(results, true);
            } else {
                self.refresh_status_line();
            }
            return;
        }

        if self.runtime.result_sort_mode == ResultSortMode::Score {
            self.apply_results_with_selection_policy(self.runtime.base_results.clone(), true, false);
        } else {
            self.apply_result_sort(true);
        }
    }

    pub(super) fn capture_active_tab_state(&self, id: u64) -> AppTabState {
        AppTabState::from_shell(self, id)
    }

    pub(super) fn apply_tab_state(&mut self, tab: &AppTabState) {
        tab.apply_shell(self);
        self.reset_query_history_navigation();
        self.runtime.query_state.query_history_dirty_since = None;
        self.reset_history_search_state();
        self.rebuild_entry_kind_cache();
        self.refresh_status_line();
    }

    pub(super) fn sync_active_tab_state(&mut self) {
        let Some(id) = self.tabs.get(self.tabs.active_tab).map(|tab| tab.id) else {
            return;
        };
        self.commit_query_history_if_needed(true);
        let snapshot = self.capture_active_tab_state(id);
        let active_tab = self.tabs.active_tab;
        if let Some(slot) = self.tabs.get_mut(active_tab) {
            *slot = snapshot;
        }
    }

    pub(super) fn find_tab_index_by_id(&self, tab_id: u64) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id == tab_id)
    }

    pub(super) fn switch_to_tab_index(&mut self, next_index: usize) {
        if next_index >= self.tabs.len() || next_index == self.tabs.active_tab {
            return;
        }
        self.deactivate_active_tab_for_transition();
        if let Some(next_tab) = self.tabs.get_mut(next_index) {
            Self::shrink_tab_checkpoint_buffers(next_tab);
        }
        self.tabs.active_tab = next_index;
        if let Some(tab) = self.tabs.get(next_index).cloned() {
            self.activate_background_tab_after_transition(&tab);
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
        let id = self.tabs.next_tab_id;
        self.tabs.next_tab_id = self.tabs.next_tab_id.saturating_add(1);
        let mut tab = self.capture_active_tab_state(id);
        tab.tab_accent = None;
        tab.use_filelist = true;
        tab.query_state.query.clear();
        tab.query_state.query_history = self.runtime.query_state.query_history.clone();
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
            .runtime
            .entries
            .iter()
            .take(self.runtime.limit)
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
        self.tabs.active_tab = self.tabs.len().saturating_sub(1);
        self.activate_tab_after_transition(&tab, false, true, false);
    }

    pub(super) fn close_active_tab(&mut self) {
        self.close_tab_index(self.tabs.active_tab);
    }

    pub(super) fn close_tab_index(&mut self, index: usize) {
        if self.tabs.len() <= 1 || index >= self.tabs.len() {
            if self.tabs.len() <= 1 {
                self.set_notice("Cannot close the last tab");
            }
            return;
        }
        self.clear_tab_drag_state();
        self.sync_active_tab_state();
        let removed = self.tabs.remove(index);
        self.clear_closed_tab_state(removed.id);
        if index < self.tabs.active_tab {
            self.tabs.active_tab = self.tabs.active_tab.saturating_sub(1);
        }
        if self.tabs.active_tab >= self.tabs.len() {
            self.tabs.active_tab = self.tabs.len().saturating_sub(1);
        }
        if let Some(tab) = self.tabs.get(self.tabs.active_tab).cloned() {
            self.activate_tab_after_transition(&tab, false, false, false);
        }
    }

    pub(super) fn move_tab(&mut self, from_index: usize, to_index: usize) {
        if from_index >= self.tabs.len() || to_index >= self.tabs.len() || from_index == to_index {
            return;
        }
        self.clear_tab_drag_state();
        self.sync_active_tab_state();
        let Some(active_tab_id) = self.tabs.get(self.tabs.active_tab).map(|tab| tab.id) else {
            return;
        };
        let moved = self.tabs.remove(from_index);
        self.tabs.insert(to_index, moved);
        if let Some(new_active) = self.find_tab_index_by_id(active_tab_id) {
            self.tabs.active_tab = new_active;
        }
        self.reapply_active_tab_state();
    }

    pub(super) fn activate_next_tab(&mut self) {
        if self.tabs.len() <= 1 {
            return;
        }
        let next = (self.tabs.active_tab + 1) % self.tabs.len();
        self.switch_to_tab_index(next);
    }

    pub(super) fn activate_previous_tab(&mut self) {
        if self.tabs.len() <= 1 {
            return;
        }
        let next = if self.tabs.active_tab == 0 {
            self.tabs.len() - 1
        } else {
            self.tabs.active_tab - 1
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
        self.capture_active_tab_state(self.tabs.get(self.tabs.active_tab).map(|tab| tab.id).unwrap_or_default())
            .into_saved(Self::history_persist_disabled())
    }

    fn saved_tab_state_from_tab(tab: &AppTabState) -> SavedTabState {
        tab.clone().into_saved(Self::history_persist_disabled())
    }

    pub(super) fn saved_tabs_for_ui_state(&self) -> Vec<SavedTabState> {
        self.tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                if index == self.tabs.active_tab {
                    self.saved_tab_state_from_app()
                } else {
                    Self::saved_tab_state_from_tab(tab)
                }
            })
            .collect()
    }
}

use super::*;
use crate::path_utils::normalize_windows_path_buf;

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

    /// ダイアログで選んだ root を現在 tab に適用する。
    pub(super) fn browse_for_root(&mut self) {
        let dialog_root = normalize_windows_path_buf(self.root.clone());
        match self.select_root_via_dialog(&dialog_root) {
            Ok(Some(dir)) => self.apply_root_change(dir),
            Ok(None) => {}
            Err(err) => self.set_notice(format!("Browse failed: {}", err)),
        }
    }

    /// ダイアログで選んだ root を新規 tab として開く。
    pub(super) fn browse_for_root_in_new_tab(&mut self) {
        let dialog_root = normalize_windows_path_buf(self.root.clone());
        match self.select_root_via_dialog(&dialog_root) {
            Ok(Some(dir)) => {
                self.create_new_tab();
                self.apply_root_change(dir);
            }
            Ok(None) => {}
            Err(err) => self.set_notice(format!("Browse failed: {}", err)),
        }
    }

    #[cfg(test)]
    fn select_root_via_dialog(&mut self, _dialog_root: &Path) -> Result<Option<PathBuf>, String> {
        self.root_browser
            .browse_dialog_result
            .take()
            .unwrap_or(Ok(None))
    }

    #[cfg(not(test))]
    fn select_root_via_dialog(&mut self, dialog_root: &Path) -> Result<Option<PathBuf>, String> {
        native_dialog::FileDialog::new()
            .set_location(dialog_root)
            .show_open_single_dir()
            .map_err(|err| err.to_string())
    }

    /// root selector popup の stable id を返す。
    pub(super) fn root_selector_popup_id() -> egui::Id {
        egui::Id::new(Self::ROOT_SELECTOR_POPUP_ID)
    }

    pub(super) fn is_root_dropdown_open(&self, ctx: &egui::Context) -> bool {
        ctx.memory(|mem| mem.is_popup_open(Self::root_selector_popup_id()))
    }

    fn current_root_dropdown_index(&self) -> Option<usize> {
        let current_key = Self::path_key(&self.root);
        self.root_browser
            .saved_roots
            .iter()
            .position(|path| Self::path_key(path) == current_key)
    }

    /// dropdown のハイライト位置を保存済み root 一覧に同期する。
    pub(super) fn sync_root_dropdown_highlight(&mut self) {
        let max_index = self.root_browser.saved_roots.len().checked_sub(1);
        self.ui.root_dropdown_highlight = match (self.ui.root_dropdown_highlight, max_index) {
            (_, None) => None,
            (Some(index), Some(max)) => Some(index.min(max)),
            (None, Some(_)) => self.current_root_dropdown_index().or(Some(0usize)),
        };
    }

    /// root dropdown を開き、入力 focus を切り替える。
    pub(super) fn open_root_dropdown(&mut self, ctx: &egui::Context) {
        self.sync_root_dropdown_highlight();
        ctx.memory_mut(|mem| mem.open_popup(Self::root_selector_popup_id()));
        self.ui.focus_query_requested = false;
        self.ui.unfocus_query_requested = true;
    }

    /// root dropdown を閉じる。
    pub(super) fn close_root_dropdown(&mut self, ctx: &egui::Context) {
        ctx.memory_mut(|mem| mem.close_popup());
    }

    /// root dropdown 内の候補選択を上下へ移動する。
    pub(super) fn move_root_dropdown_selection(&mut self, delta: isize) {
        let Some(max_index) = self.root_browser.saved_roots.len().checked_sub(1) else {
            self.ui.root_dropdown_highlight = None;
            return;
        };
        let current = self
            .ui
            .root_dropdown_highlight
            .or_else(|| self.current_root_dropdown_index())
            .unwrap_or(0) as isize;
        let next = (current + delta).clamp(0, max_index as isize) as usize;
        self.ui.root_dropdown_highlight = Some(next);
    }

    /// dropdown で確定した root を現在 tab に反映する。
    pub(super) fn apply_root_dropdown_selection(&mut self, ctx: &egui::Context) {
        let selected = self
            .ui
            .root_dropdown_highlight
            .and_then(|index| self.root_browser.saved_roots.get(index).cloned());
        self.close_root_dropdown(ctx);
        if let Some(root) = selected {
            self.apply_root_change(root);
        }
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

    pub(super) fn bind_action_request_to_tab(&mut self, request_id: u64, tab_id: u64) {
        self.request_tab_routing.bind_action(request_id, tab_id);
    }

    pub(super) fn bind_action_request_to_current_tab(&mut self, request_id: u64) {
        if let Some(tab_id) = self.current_tab_id() {
            self.bind_action_request_to_tab(request_id, tab_id);
        }
    }

    pub(super) fn take_action_request_tab(&mut self, request_id: u64) -> Option<u64> {
        self.request_tab_routing.take_action(request_id)
    }

    pub(super) fn bind_sort_request_to_tab(&mut self, request_id: u64, tab_id: u64) {
        self.request_tab_routing.bind_sort(request_id, tab_id);
    }

    pub(super) fn bind_sort_request_to_current_tab(&mut self, request_id: u64) {
        if let Some(tab_id) = self.current_tab_id() {
            self.bind_sort_request_to_tab(request_id, tab_id);
        }
    }

    pub(super) fn take_sort_request_tab(&mut self, request_id: u64) -> Option<u64> {
        self.request_tab_routing.take_sort(request_id)
    }

    pub(super) fn clear_tab_owned_request_routing(&mut self, tab_id: u64) {
        self.request_tab_routing.clear_action_for_tab(tab_id);
        self.request_tab_routing.clear_sort_for_tab(tab_id);
    }

    #[cfg(test)]
    pub(super) fn action_request_tab(&self, request_id: u64) -> Option<u64> {
        self.request_tab_routing.action.get(&request_id).copied()
    }

    #[cfg(test)]
    pub(super) fn sort_request_tab(&self, request_id: u64) -> Option<u64> {
        self.request_tab_routing.sort.get(&request_id).copied()
    }

    pub(super) fn apply_background_action_response(&mut self, response: ActionResponse) {
        let Some(tab_id) = self.take_action_request_tab(response.request_id) else {
            return;
        };
        let Some(tab_index) = self.find_tab_index_by_id(tab_id) else {
            return;
        };
        let Some(tab) = self.tabs.get_mut(tab_index) else {
            return;
        };
        if Some(response.request_id) != tab.pending_action_request_id {
            return;
        }
        tab.pending_action_request_id = None;
        tab.action_in_progress = false;
        tab.notice = response.notice;
    }

    pub(super) fn apply_background_search_response(
        &mut self,
        tab_id: u64,
        response: SearchResponse,
    ) {
        let Some(tab_index) = self.find_tab_index_by_id(tab_id) else {
            return;
        };
        let Some(tab) = self.tabs.get_mut(tab_index) else {
            return;
        };
        tab.pending_request_id = None;
        tab.search_in_progress = false;
        tab.notice = response
            .error
            .map(|error| format!("Search failed: {error}"))
            .unwrap_or_default();
        tab.result_state.base_results = response.results.clone();
        tab.result_state.results = response.results;
        tab.result_state.results_compacted = false;
        tab.result_state.result_sort_mode = ResultSortMode::Score;
        tab.result_state.pending_sort_request_id = None;
        tab.result_state.sort_in_progress = false;
        if tab.result_state.results.is_empty() {
            tab.result_state.current_row = None;
            tab.result_state.preview.clear();
            tab.pending_preview_request_id = None;
            tab.preview_in_progress = false;
        } else {
            let max_index = tab.result_state.results.len().saturating_sub(1);
            tab.result_state.current_row =
                tab.result_state.current_row.map(|row| row.min(max_index));
        }
        Self::compact_inactive_tab_state(tab);
    }

    pub(super) fn apply_active_action_response(&mut self, response: &ActionResponse) -> bool {
        if Some(response.request_id) != self.worker_bus.action.pending_request_id {
            return false;
        }
        self.take_action_request_tab(response.request_id);
        self.worker_bus.action.pending_request_id = None;
        self.worker_bus.action.in_progress = false;
        self.set_notice(response.notice.clone());
        true
    }

    pub(super) fn apply_background_sort_response(&mut self, response: SortMetadataResponse) {
        let Some(tab_id) = self.take_sort_request_tab(response.request_id) else {
            return;
        };
        let Some(tab_index) = self.find_tab_index_by_id(tab_id) else {
            return;
        };
        let Some(tab) = self.tabs.get_mut(tab_index) else {
            return;
        };
        if Some(response.request_id) != tab.result_state.pending_sort_request_id {
            return;
        }
        tab.result_state.pending_sort_request_id = None;
        tab.result_state.sort_in_progress = false;
        if response.mode == tab.result_state.result_sort_mode {
            tab.result_state.results = Self::build_sorted_results_from(
                &tab.result_state.base_results,
                tab.result_state.result_sort_mode,
                self.cache.sort_metadata.get_map(),
            );
            tab.result_state.results_compacted = false;
            if tab.result_state.results.is_empty() {
                tab.result_state.current_row = None;
                tab.result_state.preview.clear();
                tab.pending_preview_request_id = None;
                tab.preview_in_progress = false;
            } else {
                let max_index = tab.result_state.results.len().saturating_sub(1);
                tab.result_state.current_row =
                    tab.result_state.current_row.map(|row| row.min(max_index));
            }
            Self::compact_inactive_tab_state(tab);
        }
    }

    pub(super) fn apply_active_sort_response(&mut self, response: &SortMetadataResponse) -> bool {
        if Some(response.request_id) != self.worker_bus.sort.pending_request_id {
            return false;
        }
        self.take_sort_request_tab(response.request_id);
        self.worker_bus.sort.pending_request_id = None;
        self.worker_bus.sort.in_progress = false;
        if response.mode == self.result_sort_mode {
            self.apply_result_sort(false);
        } else {
            self.refresh_status_line();
        }
        true
    }

    fn clear_tab_drag_state(&mut self) {
        self.ui.tab_drag_state = None;
    }

    fn trigger_pending_restore_refresh(&mut self) {
        if !self.pending_restore_refresh {
            return;
        }
        self.pending_restore_refresh = false;
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.pending_restore_refresh = false;
        }
        self.request_index_refresh();
    }

    fn activate_background_tab_after_transition(&mut self, tab: &AppTabState) {
        self.activate_tab_after_transition(tab, true, true, true);
    }

    fn clear_closed_tab_state(&mut self, tab_id: u64) {
        self.filelist_state.clear_pending_for_tab(tab_id);
        self.indexing.clear_for_tab(tab_id);
        self.search.clear_for_tab(tab_id);
        self.clear_preview_request_routing_for_tab(tab_id);
        self.clear_tab_owned_request_routing(tab_id);
        self.ui.memory_usage_bytes = None;
    }

    fn reapply_active_tab_state(&mut self) {
        if let Some(tab) = self.tabs.get(self.active_tab).cloned() {
            self.apply_tab_state(&tab);
        }
    }

    fn deactivate_active_tab_for_transition(&mut self) -> usize {
        self.clear_tab_drag_state();
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
        let mut effect = BackgroundIndexResponseEffect {
            trigger_search: false,
            cleanup_request_id: None,
            deferred_filelist: None,
        };

        let Some(tab) = self.tabs.get_mut(tab_index) else {
            return effect;
        };

        match msg {
            IndexResponse::Started { request_id, source } => {
                if tab.index_state.pending_index_request_id != Some(request_id) {
                    return effect;
                }
                tab.index_state.index.source = source.clone();
                self.indexing
                    .background_states
                    .entry(request_id)
                    .or_default()
                    .source = Some(source);
            }
            IndexResponse::Batch {
                request_id,
                entries,
            } => {
                if tab.index_state.pending_index_request_id != Some(request_id) {
                    return effect;
                }
                let state = self
                    .indexing
                    .background_states
                    .entry(request_id)
                    .or_default();
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
                let state = self
                    .indexing
                    .background_states
                    .entry(request_id)
                    .or_default();
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
                let state = self
                    .indexing
                    .background_states
                    .remove(&request_id)
                    .unwrap_or_default();
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
                if self
                    .filelist_state
                    .pending_after_index
                    .as_ref()
                    .is_some_and(|pending| {
                        pending.tab_id == tab.id
                            && Self::path_key(&pending.root) == Self::path_key(&tab.root)
                    })
                {
                    effect.deferred_filelist = Some((
                        tab.id,
                        tab.root.clone(),
                        tab.index_state
                            .all_entries
                            .iter()
                            .map(|entry| entry.path.clone())
                            .collect(),
                    ));
                    self.filelist_state.pending_after_index = None;
                }

                if tab.query_state.query.trim().is_empty() {
                    tab.result_state.results = tab
                        .index_state
                        .entries
                        .iter()
                        .take(self.limit)
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
        if Self::path_key(&normalized) == Self::path_key(&self.root) {
            return;
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
            self.trigger_pending_restore_refresh();
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
                history_search_original_query: self
                    .query_state
                    .history_search_original_query
                    .clone(),
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
        self.indexing.incremental_filtered_entries =
            tab.index_state.incremental_filtered_entries.clone();
        self.indexing.last_incremental_results_refresh =
            tab.index_state.last_incremental_results_refresh;
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
        self.rebuild_entry_kind_cache();
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
        self.clear_tab_drag_state();
        self.sync_active_tab_state();
        let removed = self.tabs.remove(index);
        self.clear_closed_tab_state(removed.id);
        if index < self.active_tab {
            self.active_tab = self.active_tab.saturating_sub(1);
        }
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len().saturating_sub(1);
        }
        if let Some(tab) = self.tabs.get(self.active_tab).cloned() {
            self.activate_tab_after_transition(&tab, false, false, false);
        }
    }

    pub(super) fn move_tab(&mut self, from_index: usize, to_index: usize) {
        if from_index >= self.tabs.len() || to_index >= self.tabs.len() || from_index == to_index {
            return;
        }
        self.clear_tab_drag_state();
        self.sync_active_tab_state();
        let Some(active_tab_id) = self.tabs.get(self.active_tab).map(|tab| tab.id) else {
            return;
        };
        let moved = self.tabs.remove(from_index);
        self.tabs.insert(to_index, moved);
        if let Some(new_active) = self.find_tab_index_by_id(active_tab_id) {
            self.active_tab = new_active;
        }
        self.reapply_active_tab_state();
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

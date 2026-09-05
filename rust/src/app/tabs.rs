use super::{
    result_reducer, AppTabState, ClosedTabState, FlistWalkerApp, IndexResponse, IndexSource,
    PendingBackgroundIndexFinalize, ResultSortMode, ResultSortScope, SavedTabState, SearchResponse,
    TabAccentColor,
};
use crate::app::tab_state::TabResourceTransition;
use crate::path_utils::normalize_windows_path_buf;
use crate::path_utils::path_key;
use crate::walker_runtime::walker_truncated_notice;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(super) struct BackgroundIndexResponseEffect {
    pub(super) trigger_search: bool,
    pub(super) cleanup_request_id: Option<u64>,
    pub(super) deferred_filelist: Option<(u64, PathBuf, Vec<PathBuf>)>,
    pub(super) follow_up: Option<super::PendingIndexRefreshMode>,
    pub(super) reclaim_build_request_id: Option<u64>,
}

impl FlistWalkerApp {
    pub(super) fn advance_request_owned_index_finalization(
        &mut self,
        request_id: u64,
    ) -> Option<bool> {
        let ignore_list_enabled = self.shell.ui.ignore_list_enabled;
        let ignore_terms = Arc::clone(&self.shell.runtime.ignore_list_terms);
        let finalization = self
            .shell
            .indexing
            .background_finalizations
            .get(&request_id)?;
        let finalization_tab_id = finalization.tab_id;
        let previous_policy = (
            finalization.include_files,
            finalization.include_dirs,
            finalization.ignore_case,
        );
        let filter_policy = if self.current_tab_id() == Some(finalization_tab_id) {
            (
                self.shell.runtime.include_files,
                self.shell.runtime.include_dirs,
                self.shell.runtime.ignore_case,
            )
        } else {
            self.shell
                .tabs
                .iter()
                .find(|tab| tab.id == finalization_tab_id)
                .map(|tab| (tab.include_files, tab.include_dirs, tab.ignore_case))
                .unwrap_or(previous_policy)
        };
        let policy_changed = !finalization.filter_policy_matches(
            filter_policy.0,
            filter_policy.1,
            filter_policy.2,
            ignore_list_enabled,
            &ignore_terms,
        );
        if policy_changed {
            let scratch = self
                .shell
                .indexing
                .background_finalizations
                .get_mut(&request_id)
                .expect("validated background finalization")
                .take_filter_scratch();
            if scratch.heavy_resource_weight() > 0 {
                let mut resources = super::tab_resources::RetiredIndexBuildResources::empty();
                resources.set_background_filter_scratch(vec![scratch]);
                if let Err(mut resources) =
                    self.shell.tabs.try_retire_index_build_resources(resources)
                {
                    let scratch = resources
                        .take_background_filter_scratch()
                        .pop()
                        .expect("background filter scratch rollback");
                    self.shell
                        .indexing
                        .background_finalizations
                        .get_mut(&request_id)
                        .expect("validated background finalization")
                        .restore_filter_scratch(scratch);
                    return Some(false);
                }
            }
            self.shell
                .indexing
                .background_finalizations
                .get_mut(&request_id)
                .expect("validated background finalization")
                .apply_filter_policy(
                    filter_policy.0,
                    filter_policy.1,
                    filter_policy.2,
                    ignore_list_enabled,
                    Arc::clone(&ignore_terms),
                );
        }
        let finalization = self
            .shell
            .indexing
            .background_finalizations
            .get_mut(&request_id)?;
        finalization.advance(2_048, Duration::from_millis(1));
        if !finalization.is_complete() {
            return Some(false);
        }
        if finalization.scratch_reclaimed {
            return Some(true);
        }

        let scratch = finalization.take_scratch();
        let mut resources = super::tab_resources::RetiredIndexBuildResources::empty();
        resources.set_background_finalize_scratch(vec![scratch]);
        match self.shell.tabs.try_retire_index_build_resources(resources) {
            Ok(()) => {
                self.shell
                    .indexing
                    .background_finalizations
                    .get_mut(&request_id)
                    .expect("completed background finalization")
                    .scratch_reclaimed = true;
                Some(true)
            }
            Err(mut resources) => {
                let scratch = resources
                    .take_background_finalize_scratch()
                    .pop()
                    .expect("background finalize scratch rollback");
                self.shell
                    .indexing
                    .background_finalizations
                    .get_mut(&request_id)
                    .expect("completed background finalization")
                    .restore_scratch(scratch);
                Some(false)
            }
        }
    }

    /// root 切り替えに伴う state reset と再 index をまとめて適用する。
    pub(super) fn apply_root_change(&mut self, new_root: PathBuf) {
        if path_key(&normalize_windows_path_buf(new_root.clone()))
            != path_key(&self.shell.runtime.root)
        {
            self.shell.tabs.mark_active_tab_meaningfully_engaged();
        }
        self.apply_root_change_direct(new_root);
    }
    fn settle_background_tab_index_failure(tab: &mut AppTabState, notice: Option<String>) {
        let transition = if notice.is_some() {
            TabResourceTransition::Failure
        } else {
            TabResourceTransition::Cancel
        };
        tab.index_state.apply_resource_transition(transition);
        tab.index_state.index_in_progress = false;
        tab.index_state.search_resume_pending = false;
        tab.index_state.search_rerun_pending = false;
        tab.index_state.build_reclaim_pending = true;
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
        self.shell.ui.tab_drag_state = None;
    }

    fn trigger_lifecycle_activation_refresh(&mut self) {
        if self.shell.indexing.pending_request_id.is_none()
            && self.shell.indexing.pending_finish.is_none()
        {
            if let Some(root) = self.shell.indexing.root_after_pending_finish.clone() {
                self.apply_root_change_direct(root);
                return;
            }
            if let Some(mode) = self.shell.indexing.refresh_after_pending_finish.take() {
                match mode {
                    super::PendingIndexRefreshMode::Normal => self.request_index_refresh(),
                    super::PendingIndexRefreshMode::CreateFileListWalker => {
                        self.request_create_filelist_walker_refresh()
                    }
                }
                return;
            }
        }
        if !matches!(
            self.shell.indexing.lifecycle(),
            super::TabResourceLifecycle::Dormant | super::TabResourceLifecycle::Evicted
        ) || self.shell.indexing.pending_request_id.is_some()
        {
            return;
        }
        self.request_index_refresh();
    }

    fn activate_background_tab_after_transition(
        &mut self,
        results_compacted: bool,
        preview_reload_pending: bool,
    ) {
        self.activate_tab_after_transition(
            results_compacted,
            preview_reload_pending,
            true,
            true,
            true,
        );
    }

    fn clear_closed_tab_state(&mut self, tab_id: u64) {
        self.shell.features.filelist.clear_pending_for_tab(tab_id);
        self.shell.indexing.clear_for_tab_after_reclaim(tab_id);
        self.shell.search.clear_for_tab(tab_id);
        self.clear_response_routing_for_tab(tab_id);
        self.shell.ui.memory_usage_bytes = None;
    }

    fn reapply_active_tab_state(&mut self) {
        self.sync_active_tab_state();
        self.ensure_results_cursor_visible();
        self.refresh_status_line();
    }

    fn deactivate_active_tab_for_transition(&mut self) -> usize {
        self.deactivate_active_tab_for_transition_at(Instant::now())
    }

    fn deactivate_active_tab_for_transition_at(&mut self, now: Instant) -> usize {
        self.clear_tab_drag_state();
        let previous_active = self.shell.tabs.active_tab_index();
        self.store_active_tab_payload();
        if let Some(tab_id) = self.shell.tabs.get(previous_active).map(|tab| tab.id) {
            self.shell.tabs.touch_heavy_resource(tab_id);
            self.shell
                .tabs
                .record_active_tab_deactivation_at(tab_id, now);
        }
        if let Some(previous_tab) = self.shell.tabs.get_mut(previous_active) {
            Self::trim_inactive_tab_preview(previous_tab);
        }
        previous_active
    }

    /// Replace the active tab's root only after every root-scoped heavy owner
    /// has moved to the bounded reclaimer. On Full, the original root and
    /// payload are restored as one unit.
    pub(super) fn try_retire_active_root_resources(&mut self, new_root: PathBuf) -> bool {
        let active_tab = self.shell.tabs.active_tab_index();
        if active_tab >= self.shell.tabs.len() {
            return false;
        }
        let active_has_index_owner = self.shell.indexing.build_reclaim_pending
            || self.shell.indexing.pending_request_id.is_some()
            || self
                .current_tab_id()
                .is_some_and(|tab_id| !self.shell.indexing.request_ids_for_tab(tab_id).is_empty());
        if active_has_index_owner && !self.try_retire_active_index_build_resources_for_boundary() {
            return false;
        }
        self.store_active_tab_payload();
        let mut slot = self.shell.tabs.remove(active_tab);
        let resources = slot.take_heavy_resources();
        if let Err(resources) = self.shell.tabs.try_retire_tab_resources(resources) {
            slot.restore_heavy_resources(*resources);
            self.shell.tabs.insert(active_tab, slot);
            let _ = self.load_tab_payload(active_tab);
            self.set_notice("Waiting for background tab resource reclamation");
            return false;
        }

        slot.root = new_root;
        slot.index_state.build.index.source = IndexSource::None;
        slot.index_state
            .apply_resource_transition(TabResourceTransition::Reset);
        slot.index_state.clear_index_request_state();
        slot.index_state.refresh_after_pending_finish = None;
        slot.index_state.root_after_pending_finish = None;
        slot.index_state.clear_kind_resolution_state();
        slot.result_state.clear_sort_request_state();
        slot.result_state.pinned_paths.clear();
        slot.result_state.evicted_selected_path = None;
        slot.clear_search_request_state();
        slot.clear_preview_request_state();
        slot.clear_action_request_state();
        self.shell.tabs.insert(active_tab, slot);
        let _ = self.load_tab_payload(active_tab);
        self.clear_preview_cache();
        self.clear_highlight_cache();
        self.clear_sort_metadata_cache();
        true
    }

    pub(super) fn try_retire_inactive_root_resources(
        &mut self,
        tab_index: usize,
        new_root: PathBuf,
    ) -> bool {
        let inactive_has_index_owner = self.shell.tabs.get(tab_index).is_some_and(|tab| {
            tab.index_state.index_in_progress
                || tab.index_state.build_reclaim_pending
                || !self.shell.indexing.request_ids_for_tab(tab.id).is_empty()
        });
        if inactive_has_index_owner
            && !self.try_retire_tab_index_build_resources_for_boundary(tab_index)
        {
            return false;
        }
        let resources = self
            .shell
            .tabs
            .get_mut(tab_index)
            .expect("validated inactive tab")
            .take_heavy_resources();
        if let Err(resources) = self.shell.tabs.try_retire_tab_resources(resources) {
            let tab = self
                .shell
                .tabs
                .get_mut(tab_index)
                .expect("validated inactive tab");
            tab.restore_heavy_resources(*resources);
            tab.notice = "Waiting for background tab resource reclamation".to_string();
            return false;
        }
        let tab = self
            .shell
            .tabs
            .get_mut(tab_index)
            .expect("validated inactive tab");
        tab.root = new_root;
        tab.index_state.build.index.source = IndexSource::None;
        tab.index_state
            .apply_resource_transition(TabResourceTransition::Reset);
        tab.index_state.clear_index_request_state();
        tab.index_state.refresh_after_pending_finish = None;
        tab.index_state.root_after_pending_finish = None;
        tab.index_state.clear_kind_resolution_state();
        tab.result_state.clear_sort_request_state();
        tab.result_state.pinned_paths.clear();
        tab.result_state.evicted_selected_path = None;
        tab.clear_search_request_state();
        tab.clear_preview_request_state();
        tab.clear_action_request_state();
        true
    }

    fn activate_tab_after_transition(
        &mut self,
        results_compacted: bool,
        preview_reload_pending: bool,
        restore_results: bool,
        request_focus: bool,
        trigger_restore_refresh: bool,
    ) {
        if restore_results {
            self.restore_results_from_compacted_tab(results_compacted);
        }
        self.ensure_results_cursor_visible();
        if trigger_restore_refresh {
            // Regression guard: restore refresh resets preview request ownership.
            // Schedule it before consuming a tab-scoped reload flag so the new
            // async preview request remains live and can settle.
            self.trigger_lifecycle_activation_refresh();
        }
        if preview_reload_pending {
            self.request_preview_for_current();
        }
        if request_focus {
            self.request_focus_query();
            self.clear_unfocus_query_request();
        }
    }

    fn apply_background_index_finished(
        &mut self,
        tab_index: usize,
        request_id: u64,
        source: IndexSource,
    ) -> BackgroundIndexResponseEffect {
        let mut effect = BackgroundIndexResponseEffect {
            trigger_search: false,
            cleanup_request_id: None,
            deferred_filelist: None,
            follow_up: None,
            reclaim_build_request_id: None,
        };
        if self
            .shell
            .tabs
            .get(tab_index)
            .is_none_or(|tab| tab.index_state.pending_index_request_id != Some(request_id))
        {
            effect.cleanup_request_id = Some(request_id);
            return effect;
        }

        let finalization_staged = self
            .shell
            .indexing
            .background_finalizations
            .contains_key(&request_id);
        if !finalization_staged {
            if !self
                .shell
                .indexing
                .background_finalizations
                .has_capacity_for(request_id)
            {
                let requeued = self
                    .shell
                    .indexing
                    .requeue_terminal(IndexResponse::Finished { request_id, source });
                if let Some(tab) = self.shell.tabs.get_mut(tab_index) {
                    tab.index_state.index_in_progress = false;
                    if requeued {
                        tab.notice = "Waiting for background tab finalization slot".to_string();
                    } else {
                        Self::settle_background_tab_index_failure(
                            tab,
                            Some("Index terminal mailbox became unavailable".to_string()),
                        );
                        effect.reclaim_build_request_id = Some(request_id);
                    }
                }
                return effect;
            }
            let mut state = self
                .shell
                .indexing
                .background_states
                .remove(&request_id)
                .unwrap_or_default();
            let pending_after_index_matches = self
                .shell
                .features
                .filelist
                .workflow
                .pending_after_index
                .as_ref()
                .is_some_and(|pending| {
                    self.shell.tabs.get(tab_index).is_some_and(|tab| {
                        pending.tab_id == tab.id
                            && path_key(&pending.root) == path_key(&tab.root)
                            && tab
                                .index_state
                                .root_after_pending_finish
                                .as_ref()
                                .is_none_or(|root| path_key(root) == path_key(&tab.root))
                    })
                });
            let (tab_root, tab_include_files, tab_include_dirs, tab_ignore_case) = self
                .shell
                .tabs
                .get(tab_index)
                .map(|tab| {
                    (
                        tab.root.clone(),
                        tab.include_files,
                        tab.include_dirs,
                        tab.ignore_case,
                    )
                })
                .expect("validated tab");
            let ignore_terms_source = Arc::clone(&self.shell.runtime.ignore_list_terms);
            let ignore_list_enabled =
                self.shell.ui.ignore_list_enabled && !ignore_terms_source.is_empty();
            let tab = self.shell.tabs.get_mut(tab_index).expect("validated tab");
            let source = state.source.take().unwrap_or(source);
            tab.index_state.build.index.source = source.clone();
            let existing_entries = std::mem::take(&mut tab.index_state.build.index.entries);
            let pending_entries =
                if tab.index_state.pending_index_entries_request_id == Some(request_id) {
                    std::mem::take(&mut tab.index_state.build.pending_entries)
                } else {
                    Default::default()
                };
            let state_entries = std::mem::take(&mut state.entries);
            let use_state_only =
                state.replaced || (existing_entries.is_empty() && pending_entries.is_empty());
            let (
                initial_entries,
                selected_pending_entries,
                continuation_entries,
                discarded_entries,
                discarded_pending_entries,
            ) = if use_state_only {
                (
                    state_entries.into(),
                    Default::default(),
                    Default::default(),
                    existing_entries.into(),
                    pending_entries,
                )
            } else {
                (
                    existing_entries.into(),
                    pending_entries,
                    state_entries.into(),
                    Default::default(),
                    Default::default(),
                )
            };
            let finalization = PendingBackgroundIndexFinalize::new(
                super::BackgroundIndexFinalizeIdentity {
                    tab_id: tab.id,
                    request_id,
                    source: source.clone(),
                },
                super::BackgroundIndexFinalizePolicy {
                    include_files: tab_include_files,
                    include_dirs: tab_include_dirs,
                    root: tab_root,
                    prefer_relative: Self::prefer_relative_display_for(&source),
                    ignore_case: tab_ignore_case,
                    ignore_list_enabled,
                    ignore_terms_source,
                },
                super::BackgroundIndexFinalizeInputs {
                    initial_entries,
                    pending_entries: selected_pending_entries,
                    continuation_entries,
                    discarded_entries,
                    discarded_pending_entries,
                    capture_filelist_paths: pending_after_index_matches,
                },
            );
            tab.index_state.pending_index_finish =
                Some(super::PendingActiveIndexFinish { request_id, source });
            tab.index_state.index_in_progress = false;
            self.shell
                .indexing
                .background_finalizations
                .insert(request_id, finalization);
        }

        if !self
            .advance_request_owned_index_finalization(request_id)
            .unwrap_or(false)
        {
            if let Some(tab) = self.shell.tabs.get_mut(tab_index) {
                tab.notice = "Waiting for background tab resource finalization".to_string();
            }
            return effect;
        }

        let deferred_root = self
            .shell
            .tabs
            .get(tab_index)
            .and_then(|tab| tab.index_state.root_after_pending_finish.clone());
        let root_changes = deferred_root.as_ref().is_some_and(|root| {
            self.shell
                .tabs
                .get(tab_index)
                .is_some_and(|tab| path_key(root) != path_key(&tab.root))
        });
        if root_changes {
            let target_root = deferred_root.expect("root change checked");
            let follow_up = self
                .shell
                .tabs
                .get(tab_index)
                .and_then(|tab| tab.index_state.refresh_after_pending_finish)
                .unwrap_or(super::PendingIndexRefreshMode::Normal);
            let mut finalization = self
                .shell
                .indexing
                .background_finalizations
                .remove(&request_id)
                .expect("completed background finalization");
            debug_assert!(finalization.filelist_paths.is_none());
            let had_filtered_entries = finalization.filtered_entries.is_some();
            {
                let tab = self.shell.tabs.get_mut(tab_index).expect("validated tab");
                tab.index_state.build.index.entries =
                    std::mem::take(&mut finalization.completed_entries);
                tab.index_state.build.incremental_filtered_entries =
                    finalization.filtered_entries.take().unwrap_or_default();
                tab.index_state.build.pending_kind_paths =
                    std::mem::take(&mut finalization.unresolved_kind_paths);
                tab.index_state.build.pending_kind_paths_set =
                    std::mem::take(&mut finalization.unresolved_kind_paths_set);
            }
            let resources = self
                .shell
                .tabs
                .get_mut(tab_index)
                .expect("validated tab")
                .take_heavy_resources();
            if let Err(resources) = self.shell.tabs.try_retire_tab_resources(resources) {
                let tab = self.shell.tabs.get_mut(tab_index).expect("validated tab");
                tab.restore_heavy_resources(*resources);
                finalization.completed_entries =
                    std::mem::take(&mut tab.index_state.build.index.entries);
                let filtered_entries =
                    std::mem::take(&mut tab.index_state.build.incremental_filtered_entries);
                finalization.filtered_entries = had_filtered_entries.then_some(filtered_entries);
                finalization.unresolved_kind_paths =
                    std::mem::take(&mut tab.index_state.build.pending_kind_paths);
                finalization.unresolved_kind_paths_set =
                    std::mem::take(&mut tab.index_state.build.pending_kind_paths_set);
                tab.notice = "Waiting for background tab resource reclamation".to_string();
                self.shell
                    .indexing
                    .background_finalizations
                    .insert(request_id, finalization);
                return effect;
            }
            let tab_id = {
                let tab = self.shell.tabs.get_mut(tab_index).expect("validated tab");
                tab.root = target_root;
                tab.index_state.build.index.source = IndexSource::None;
                tab.index_state
                    .apply_resource_transition(TabResourceTransition::Reset);
                tab.index_state.clear_index_request_state();
                tab.index_state.refresh_after_pending_finish = None;
                tab.index_state.root_after_pending_finish = None;
                tab.index_state.clear_kind_resolution_state();
                tab.result_state.clear_sort_request_state();
                tab.result_state.pinned_paths.clear();
                tab.result_state.evicted_selected_path = None;
                tab.clear_search_request_state();
                tab.clear_preview_request_state();
                tab.clear_action_request_state();
                tab.notice = "Root change queued for background indexing".to_string();
                tab.id
            };
            if self
                .shell
                .features
                .filelist
                .workflow
                .pending_after_index
                .as_ref()
                .is_some_and(|pending| pending.tab_id == tab_id)
            {
                self.shell.features.filelist.workflow.pending_after_index = None;
            }
            effect.cleanup_request_id = Some(request_id);
            effect.follow_up = Some(follow_up);
            return effect;
        }

        let previous = {
            let tab = self.shell.tabs.get_mut(tab_index).expect("validated tab");
            tab.index_state
                .committed_snapshot_present()
                .then(|| tab.take_committed_resources())
        };
        if let Some(previous) = previous {
            if !previous.is_empty() {
                if let Err(previous) = self.shell.tabs.try_retire_active_resources(previous) {
                    let tab = self.shell.tabs.get_mut(tab_index).expect("validated tab");
                    tab.restore_committed_resources(previous);
                    tab.notice = "Waiting for background tab resource reclamation".to_string();
                    return effect;
                }
            }
        }

        let limit = self.shell.runtime.limit;
        let mut finalization = self
            .shell
            .indexing
            .background_finalizations
            .remove(&request_id)
            .expect("completed background finalization");
        debug_assert_eq!(finalization.request_id, request_id);
        let shell = &mut self.shell;
        let (tabs, features) = (&mut shell.tabs, &mut shell.features);
        let tab = tabs.get_mut(tab_index).expect("validated tab");
        let follow_up = tab.index_state.refresh_after_pending_finish;
        let pending_finish = tab
            .index_state
            .pending_index_finish
            .take()
            .expect("staged background finish");
        debug_assert_eq!(finalization.tab_id, tab.id);
        tab.index_state.build.index.source = pending_finish.source;
        tab.result_state.committed.all_entries =
            Arc::new(std::mem::take(&mut finalization.completed_entries));
        tab.index_state
            .apply_resource_transition(TabResourceTransition::Success);
        tab.result_state.committed.entries = finalization
            .filtered_entries
            .take()
            .map(Arc::new)
            .unwrap_or_else(|| Arc::clone(&tab.result_state.committed.all_entries));
        tab.index_state.clear_index_request_state();
        tab.index_state.last_search_snapshot_len = tab.result_state.committed.entries.len();
        tab.index_state.last_incremental_results_refresh = Instant::now();
        if matches!(tab.index_state.build.index.source, IndexSource::Walker)
            && (!tab.include_files || !tab.include_dirs)
        {
            tab.index_state.build.pending_kind_paths =
                std::mem::take(&mut finalization.unresolved_kind_paths);
            tab.index_state.build.pending_kind_paths_set =
                std::mem::take(&mut finalization.unresolved_kind_paths_set);
            tab.index_state.refresh_kind_resolution_progress();
        } else {
            tab.index_state.clear_kind_resolution_state();
        }
        let pending_after_index_matches = features
            .filelist
            .workflow
            .pending_after_index
            .as_ref()
            .is_some_and(|pending| {
                pending.tab_id == tab.id && path_key(&pending.root) == path_key(&tab.root)
            });
        if pending_after_index_matches {
            effect.deferred_filelist = Some((
                tab.id,
                tab.root.clone(),
                finalization.filelist_paths.take().unwrap_or_default(),
            ));
            features.filelist.workflow.pending_after_index = None;
        }
        if tab.query_state.query.trim().is_empty() {
            let results = tab
                .result_state
                .committed
                .entries
                .iter()
                .take(limit)
                .cloned()
                .map(|entry| (entry.path, 0.0))
                .collect::<Vec<_>>();
            tab.result_state.clear_sort_request_state();
            tab.result_state.result_sort_mode = ResultSortMode::Score;
            tab.result_state.result_sort_scope = ResultSortScope::ShownResults;
            tab.result_state.committed.base_results = results.clone();
            tab.result_state.committed.base_results_are_score_ranked = true;
            tab.result_state.committed.results = results;
            tab.result_state.results_compacted = false;
            tab.result_state.committed.total_match_count = tab.result_state.committed.entries.len();
            let evicted_selected_path = tab.result_state.evicted_selected_path.take();
            if tab.result_state.committed.results.is_empty() {
                tab.result_state.committed.current_row = None;
                tab.result_state.committed.preview.clear();
                tab.clear_preview_request_state();
            } else if let Some(selected) = evicted_selected_path {
                tab.result_state.committed.current_row = tab
                    .result_state
                    .committed
                    .results
                    .iter()
                    .position(|(path, _)| *path == selected)
                    .or(Some(0));
            } else {
                let max_index = tab.result_state.committed.results.len().saturating_sub(1);
                tab.result_state.committed.current_row = Some(
                    tab.result_state
                        .committed
                        .current_row
                        .unwrap_or(0)
                        .min(max_index),
                );
            }
        } else {
            effect.trigger_search = true;
        }
        effect.cleanup_request_id = Some(request_id);
        effect.follow_up = follow_up;
        effect
    }

    pub(super) fn apply_background_index_response(
        &mut self,
        tab_index: usize,
        msg: IndexResponse,
    ) -> BackgroundIndexResponseEffect {
        if tab_index == self.shell.tabs.active_tab_index() {
            let request_id = super::IndexCoordinator::response_request_id(&msg);
            return BackgroundIndexResponseEffect {
                trigger_search: false,
                cleanup_request_id: super::IndexCoordinator::is_terminal_response(&msg)
                    .then_some(request_id),
                deferred_filelist: None,
                follow_up: None,
                reclaim_build_request_id: None,
            };
        }
        if matches!(&msg, IndexResponse::ReplaceAll { .. }) {
            self.try_apply_replace_all_response(msg);
            return BackgroundIndexResponseEffect {
                trigger_search: false,
                cleanup_request_id: None,
                deferred_filelist: None,
                follow_up: None,
                reclaim_build_request_id: None,
            };
        }
        if let IndexResponse::Finished { request_id, source } = msg {
            return self.apply_background_index_finished(tab_index, request_id, source);
        }
        let shell = &mut self.shell;
        let (tabs, indexing) = (&mut shell.tabs, &mut shell.indexing);
        let mut effect = BackgroundIndexResponseEffect {
            trigger_search: false,
            cleanup_request_id: None,
            deferred_filelist: None,
            follow_up: None,
            reclaim_build_request_id: None,
        };

        let Some(tab) = tabs.get_mut(tab_index) else {
            return effect;
        };

        match msg {
            IndexResponse::Started { request_id, source } => {
                if tab.index_state.pending_index_request_id != Some(request_id) {
                    return effect;
                }
                tab.index_state.build.index.source = source.clone();
                indexing
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
                let state = indexing.background_states.entry(request_id).or_default();
                for entry in entries {
                    state.entries.push(entry.into());
                }
            }
            IndexResponse::ReplaceAll { .. } => unreachable!("replace-all handled above"),
            IndexResponse::Finished { .. } => unreachable!("finished handled before state split"),
            IndexResponse::Failed { request_id, error } => {
                if tab.index_state.pending_index_request_id != Some(request_id) {
                    effect.cleanup_request_id = Some(request_id);
                    return effect;
                }
                Self::settle_background_tab_index_failure(
                    tab,
                    Some(format!("Indexing failed: {}", error)),
                );
                effect.reclaim_build_request_id = Some(request_id);
            }
            IndexResponse::Canceled { request_id } => {
                if tab.index_state.pending_index_request_id == Some(request_id) {
                    Self::settle_background_tab_index_failure(tab, None);
                    effect.reclaim_build_request_id = Some(request_id);
                } else {
                    effect.cleanup_request_id = Some(request_id);
                }
            }
            IndexResponse::Truncated { request_id, limit } => {
                if tab.index_state.pending_index_request_id == Some(request_id) {
                    tab.notice = walker_truncated_notice(limit);
                }
            }
        }

        effect
    }
    pub(super) fn apply_root_change_direct(&mut self, new_root: PathBuf) {
        let normalized = normalize_windows_path_buf(new_root);
        if self.shell.indexing.pending_finish.is_some() {
            self.shell.indexing.root_after_pending_finish = Some(normalized);
            self.shell.indexing.refresh_after_pending_finish =
                Some(super::PendingIndexRefreshMode::Normal);
            self.set_notice("Waiting for background tab resource reclamation");
            return;
        }
        if path_key(&normalized) == path_key(&self.shell.runtime.root) {
            self.shell.indexing.root_after_pending_finish = None;
            self.shell.indexing.refresh_after_pending_finish = None;
            return;
        }

        let refresh_mode = self
            .shell
            .indexing
            .refresh_after_pending_finish
            .unwrap_or(super::PendingIndexRefreshMode::Normal);
        if !self.try_retire_active_root_resources(normalized.clone()) {
            self.shell.indexing.root_after_pending_finish = Some(normalized);
            self.shell.indexing.refresh_after_pending_finish = Some(refresh_mode);
            return;
        }
        self.reset_query_history_navigation();
        self.set_query_history_dirty_since(None);
        self.reset_history_search_state();
        self.sync_active_tab_state();
        self.cancel_stale_pending_filelist_confirmations_for_active_root();
        self.mark_ui_state_dirty();
        match refresh_mode {
            super::PendingIndexRefreshMode::Normal => self.request_index_refresh(),
            super::PendingIndexRefreshMode::CreateFileListWalker => {
                self.request_create_filelist_walker_refresh()
            }
        }
        self.set_notice(format!("Root changed: {}", self.root_display_text()));
    }

    pub(super) fn choose_startup_root(
        root: PathBuf,
        root_explicit: bool,
        restore_session_allowed: bool,
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
        if restore_session_allowed {
            last_root.or(default_root).unwrap_or(root)
        } else {
            default_root.or(last_root).unwrap_or(root)
        }
    }

    pub(super) fn initialize_tabs(&mut self) {
        let id = self.shell.tabs.take_next_tab_id();
        let scratch = AppTabState::scratch_from_shell(self, id);
        self.shell.tabs.replace_all(vec![scratch]);
        self.shell.tabs.set_active_tab_index(0);
    }

    pub(super) fn restored_tab_state(&self, id: u64, saved: &SavedTabState) -> AppTabState {
        AppTabState::from_saved(self, id, saved)
    }

    pub(super) fn initialize_tabs_from_saved(
        &mut self,
        saved_tabs: Vec<SavedTabState>,
        active_tab: usize,
    ) {
        let restored_tabs = saved_tabs
            .iter()
            .map(|saved| {
                let id = self.shell.tabs.take_next_tab_id();
                self.restored_tab_state(id, saved)
            })
            .collect();
        self.shell.tabs.replace_all(restored_tabs);
        self.shell
            .tabs
            .set_active_tab_index(active_tab.min(self.shell.tabs.len().saturating_sub(1)));
        if self.shell.tabs.len() != 0 {
            let active_tab = self.shell.tabs.active_tab_index();
            let (results_compacted, _) = self.load_tab_payload(active_tab);
            self.restore_results_from_compacted_tab(results_compacted);
            self.ensure_results_cursor_visible();
            self.request_focus_query();
            self.clear_unfocus_query_request();
            self.trigger_lifecycle_activation_refresh();
            self.shell.runtime.notice = "Restored tab session".to_string();
            self.refresh_status_line();
        }
    }

    pub(super) fn current_tab_id(&self) -> Option<u64> {
        self.shell
            .tabs
            .get(self.shell.tabs.active_tab_index())
            .map(|tab| tab.id)
    }

    pub(super) fn trim_inactive_tab_preview(tab: &mut AppTabState) {
        if !tab.preview_in_progress {
            // Keep result and index allocations intact: tab activation is a latency-critical
            // ownership transfer and must not perform O(n) drops or allocator compaction.
            if !tab.result_state.committed.preview.is_empty() {
                tab.mark_preview_reload_pending();
            }
            tab.result_state.committed.preview.clear();
        }
    }

    pub(super) fn restore_results_from_compacted_tab(&mut self, results_compacted: bool) {
        if !results_compacted {
            return;
        }

        if self.shell.runtime.query_state.query.trim().is_empty() {
            let total_match_count = self.shell.runtime.entries.len();
            let results = self
                .shell
                .runtime
                .entries
                .iter()
                .take(self.shell.runtime.limit)
                .cloned()
                .map(|entry| (entry.path, 0.0))
                .collect();
            self.replace_results_snapshot(results, true);
            self.shell.runtime.total_match_count = total_match_count;
            return;
        }

        if self.shell.runtime.base_results.is_empty() {
            self.refresh_status_line();
            return;
        }

        if self.shell.runtime.result_sort_mode == ResultSortMode::Score {
            self.apply_results_with_selection_policy(
                self.shell.runtime.base_results.clone(),
                true,
                false,
            );
        } else {
            self.apply_result_sort(true);
        }
    }

    #[cfg(test)]
    pub(super) fn capture_active_tab_state(&self, id: u64) -> AppTabState {
        AppTabState::from_shell(self, id)
    }

    #[cfg(test)]
    pub(super) fn apply_tab_state(&mut self, tab: &AppTabState) {
        tab.apply_shell(self);
        self.finish_tab_payload_load();
    }

    fn finish_tab_payload_load(&mut self) {
        self.reset_query_history_navigation();
        self.set_query_history_dirty_since(None);
        self.reset_history_search_state();
        self.refresh_status_line();
    }

    pub(super) fn sync_active_tab_state(&mut self) {
        self.commit_query_history_if_needed(true);
        let active_tab = self.shell.tabs.active_tab_index();
        if active_tab < self.shell.tabs.len() {
            let mut slot = self.shell.tabs.remove(active_tab);
            slot.sync_small_fields_from_shell(self);
            self.shell.tabs.insert(active_tab, slot);
        }
    }

    fn store_active_tab_payload(&mut self) {
        self.commit_query_history_if_needed(true);
        let active_tab = self.shell.tabs.active_tab_index();
        if active_tab >= self.shell.tabs.len() {
            return;
        }
        let mut slot = self.shell.tabs.remove(active_tab);
        slot.sync_small_fields_from_shell(self);
        slot.swap_payload_with_shell(self);
        slot.result_state.results_compacted = false;
        self.shell.tabs.insert(active_tab, slot);
    }

    fn load_tab_payload(&mut self, index: usize) -> (bool, bool) {
        if let Some(tab_id) = self.shell.tabs.get(index).map(|tab| tab.id) {
            self.shell.tabs.remove_resource_tracking(tab_id);
        }
        let mut slot = self.shell.tabs.remove(index);
        let results_compacted = slot.result_state.results_compacted;
        let preview_reload_pending = slot.take_preview_reload_pending();
        slot.apply_small_fields_to_shell(self);
        slot.swap_payload_with_shell(self);
        slot.result_state.results_compacted = false;
        slot.sync_small_fields_from_shell(self);
        self.shell.tabs.insert(index, slot);
        self.finish_tab_payload_load();
        (results_compacted, preview_reload_pending)
    }

    pub(super) fn enforce_tab_resource_budget(&mut self) {
        let active_tab_id = self.current_tab_id();
        let warm_tab_id = self.shell.indexing.warm_tab_id;
        if !self
            .shell
            .tabs
            .enforce_resource_budget(active_tab_id, warm_tab_id)
            && self.shell.runtime.notice.is_empty()
        {
            self.shell.runtime.notice =
                "Waiting for background tab resource reclamation".to_string();
        }
    }

    pub(super) fn find_tab_index_by_id(&self, tab_id: u64) -> Option<usize> {
        self.shell.tabs.iter().position(|tab| tab.id == tab_id)
    }

    fn tab_terminal_waits_for_finalization(&self, tab_index: usize) -> bool {
        let Some(request_id) = self
            .shell
            .tabs
            .get(tab_index)
            .and_then(|tab| tab.index_state.pending_index_request_id)
        else {
            return false;
        };
        !self
            .shell
            .indexing
            .background_finalizations
            .contains_key(&request_id)
            && self
                .shell
                .indexing
                .background_states
                .contains_key(&request_id)
            && self.shell.indexing.mailbox_has_terminal(request_id)
    }

    fn try_prepare_tab_for_activation(&mut self, tab_index: usize) -> bool {
        let pending_request_id = self
            .shell
            .tabs
            .get(tab_index)
            .and_then(|tab| tab.index_state.pending_index_request_id);
        if pending_request_id
            .is_some_and(|request_id| self.shell.indexing.is_superseded_request(request_id))
            && !self.try_retire_tab_index_build_resources_for_boundary(tab_index)
        {
            self.set_notice("Waiting for background tab resource reclamation");
            return false;
        }
        if self.tab_terminal_waits_for_finalization(tab_index) {
            self.set_notice("Waiting for background tab finalization");
            return false;
        }
        true
    }

    pub(super) fn retry_pending_tab_activation(&mut self) {
        let Some(tab_id) = self.shell.tabs.pending_activation_tab_id else {
            return;
        };
        let Some(tab_index) = self.find_tab_index_by_id(tab_id) else {
            self.shell.tabs.pending_activation_tab_id = None;
            return;
        };
        self.switch_to_tab_index(tab_index);
    }

    pub(super) fn switch_to_tab_index(&mut self, next_index: usize) {
        self.switch_to_tab_index_at(next_index, Instant::now());
    }

    pub(super) fn switch_to_tab_index_at(&mut self, next_index: usize, now: Instant) {
        if next_index >= self.shell.tabs.len() {
            return;
        }
        if next_index == self.shell.tabs.active_tab_index() {
            self.shell.tabs.pending_activation_tab_id = None;
            return;
        }
        if !self.try_prepare_tab_for_activation(next_index) {
            self.shell.tabs.pending_activation_tab_id =
                self.shell.tabs.get(next_index).map(|tab| tab.id);
            return;
        }
        self.shell.tabs.pending_activation_tab_id = None;
        let previous_tab_id = self.current_tab_id();
        let previous_was_indexing = self.shell.indexing.pending_request_id.is_some();
        let next_tab_id = self.shell.tabs.get(next_index).map(|tab| tab.id);
        let promoted_warm = next_tab_id == self.shell.indexing.warm_tab_id;
        let previous_active = self.deactivate_active_tab_for_transition_at(now);
        let future_warm = if promoted_warm {
            previous_was_indexing.then_some(previous_tab_id).flatten()
        } else if previous_was_indexing {
            previous_tab_id
        } else {
            self.shell.indexing.warm_tab_id
        };
        if !self
            .shell
            .tabs
            .enforce_resource_budget(next_tab_id, future_warm)
        {
            self.shell.tabs.rollback_recent_inactive_transition();
            let _ = self.load_tab_payload(previous_active);
            self.shell
                .tabs
                .set_active_tab_index_at(previous_active, now);
            self.set_notice("Waiting for background tab resource reclamation");
            return;
        }
        if !self.shell.tabs.commit_recent_inactive_transition() {
            let _ = self.load_tab_payload(previous_active);
            self.shell
                .tabs
                .set_active_tab_index_at(previous_active, now);
            self.set_notice("Waiting for background tab resource reclamation");
            return;
        }
        self.shell.tabs.set_active_tab_index_at(next_index, now);
        let (results_compacted, preview_reload_pending) = self.load_tab_payload(next_index);
        let previous_warm_candidate = previous_was_indexing.then_some(previous_tab_id).flatten();
        if promoted_warm {
            if let Some(next_tab_id) = next_tab_id {
                self.shell
                    .indexing
                    .promote_warm_tab(next_tab_id, previous_warm_candidate);
            }
        } else if previous_was_indexing {
            self.shell
                .indexing
                .replace_warm_tab(previous_warm_candidate);
        }
        self.activate_background_tab_after_transition(results_compacted, preview_reload_pending);
        self.enforce_tab_resource_budget();
    }

    pub(super) fn set_tab_accent(&mut self, index: usize, accent: Option<TabAccentColor>) {
        let Some(tab) = self.shell.tabs.get_mut(index) else {
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
        self.shell.tabs.pending_activation_tab_id = None;
        let active_tab_id = self.current_tab_id();
        if !self
            .shell
            .tabs
            .enforce_resource_budget(active_tab_id, self.shell.indexing.warm_tab_id)
        {
            self.set_notice("Waiting for background tab resource reclamation");
            return;
        }
        self.commit_query_history_if_needed(true);
        let requires_default_walk_reindex =
            !self.shell.runtime.max_depth.is_unlimited() || self.shell.runtime.follow_links;
        let id = self.shell.tabs.take_next_tab_id();
        let tab = AppTabState::new_tab_from_shell(self, id);
        let previous_active = self.shell.tabs.active_tab_index();
        self.deactivate_active_tab_for_transition();
        if !self
            .shell
            .tabs
            .enforce_resource_budget(Some(id), self.shell.indexing.warm_tab_id)
        {
            self.shell.tabs.rollback_recent_inactive_transition();
            let _ = self.load_tab_payload(previous_active);
            self.set_notice("Waiting for background tab resource reclamation");
            return;
        }
        if !self.shell.tabs.commit_recent_inactive_transition() {
            let _ = self.load_tab_payload(previous_active);
            self.set_notice("Waiting for background tab resource reclamation");
            return;
        }
        self.shell.tabs.push(tab);
        self.shell
            .tabs
            .set_active_tab_index(self.shell.tabs.len().saturating_sub(1));
        let active_tab = self.shell.tabs.active_tab_index();
        let (results_compacted, preview_reload_pending) = self.load_tab_payload(active_tab);
        self.activate_tab_after_transition(
            results_compacted,
            preview_reload_pending,
            false,
            true,
            false,
        );
        if requires_default_walk_reindex {
            self.request_index_refresh();
        }
        // Inherited results produce no index/search completion event. Start their
        // display work explicitly, after any reindex has established its epoch.
        self.queue_unknown_kind_paths_for_visible_results();
        self.pump_kind_resolution_requests();
        self.request_preview_for_current();
        self.enforce_tab_resource_budget();
    }

    pub(super) fn close_active_tab(&mut self) {
        self.close_tab_index(self.shell.tabs.active_tab_index());
    }

    fn prepare_closed_tab_for_restore(tab: &mut AppTabState, id: u64) {
        let preview_reload_pending = tab.preview_reload_pending || tab.preview_in_progress;
        tab.id = id;
        tab.clear_search_request_state();
        tab.clear_preview_request_state();
        if preview_reload_pending {
            tab.result_state.committed.preview.clear();
            tab.mark_preview_reload_pending();
        } else {
            tab.clear_preview_reload_pending();
        }
        tab.clear_action_request_state();
        debug_assert!(tab.index_state.root_after_pending_finish.is_none());
        tab.index_state.clear_index_request_state();
        tab.index_state.refresh_after_pending_finish = None;
        tab.index_state.root_after_pending_finish = None;
        tab.index_state.clear_kind_resolution_state();
        tab.result_state.clear_sort_request_state();
        tab.index_state.kind_resolution_epoch = tab.index_state.kind_resolution_epoch.max(1);
        tab.notice = "Restored closed tab".to_string();
    }

    pub(super) fn close_tab_index(&mut self, index: usize) {
        if self.shell.tabs.len() <= 1 || index >= self.shell.tabs.len() {
            if self.shell.tabs.len() <= 1 {
                self.set_notice("Cannot close the last tab");
            }
            return;
        }
        let closing_active = index == self.shell.tabs.active_tab_index();
        let closing_tab_id = self.shell.tabs.get(index).map(|tab| tab.id);
        if closing_active || self.shell.tabs.pending_activation_tab_id == closing_tab_id {
            self.shell.tabs.pending_activation_tab_id = None;
        }
        let pending_root = if closing_active {
            self.shell.indexing.root_after_pending_finish.clone()
        } else {
            self.shell
                .tabs
                .get(index)
                .and_then(|tab| tab.index_state.root_after_pending_finish.clone())
        };
        let terminal_pending = if closing_active {
            self.shell.indexing.pending_finish.is_some()
        } else {
            self.shell
                .tabs
                .get(index)
                .is_some_and(|tab| tab.index_state.pending_index_finish.is_some())
        };
        let interrupted_index_before_close = if closing_active {
            self.shell.indexing.pending_request_id.is_some()
                || self.shell.indexing.in_progress
                || self.shell.indexing.build_reclaim_pending
                || matches!(
                    self.shell.indexing.lifecycle(),
                    super::TabResourceLifecycle::Dormant
                        | super::TabResourceLifecycle::Loading
                        | super::TabResourceLifecycle::Refreshing
                        | super::TabResourceLifecycle::Evicted
                )
        } else {
            self.shell.tabs.get(index).is_some_and(|tab| {
                tab.index_state.pending_index_request_id.is_some()
                    || tab.index_state.index_in_progress
                    || tab.index_state.build_reclaim_pending
                    || matches!(
                        tab.index_state.lifecycle(),
                        super::TabResourceLifecycle::Dormant
                            | super::TabResourceLifecycle::Loading
                            | super::TabResourceLifecycle::Refreshing
                            | super::TabResourceLifecycle::Evicted
                    )
            })
        };
        if terminal_pending && pending_root.is_none() {
            self.set_notice("Waiting for background tab resource reclamation");
            return;
        }
        let root_reclaim_pending = pending_root.is_some();
        if closing_active {
            let future_active_index = if index + 1 < self.shell.tabs.len() {
                index + 1
            } else {
                index.saturating_sub(1)
            };
            if !self.try_prepare_tab_for_activation(future_active_index) {
                return;
            }
            self.deactivate_active_tab_for_transition();
            let future_active_id = self.shell.tabs.get(future_active_index).map(|tab| tab.id);
            if !self
                .shell
                .tabs
                .enforce_resource_budget(future_active_id, self.shell.indexing.warm_tab_id)
            {
                self.shell.tabs.rollback_recent_inactive_transition();
                let _ = self.load_tab_payload(index);
                self.set_notice("Waiting for background tab resource reclamation");
                return;
            }
            if !self.shell.tabs.try_prepare_closed_history_slot() {
                self.shell.tabs.rollback_recent_inactive_transition();
                let _ = self.load_tab_payload(index);
                self.set_notice("Waiting for background tab resource reclamation");
                return;
            }
        } else {
            let closing_tab_id = self.shell.tabs.get(index).map(|tab| tab.id);
            let warm_after_close = (self.shell.indexing.warm_tab_id != closing_tab_id)
                .then_some(self.shell.indexing.warm_tab_id)
                .flatten();
            if !self
                .shell
                .tabs
                .enforce_resource_budget(self.current_tab_id(), warm_after_close)
            {
                self.set_notice("Waiting for background tab resource reclamation");
                return;
            }
            if !self.shell.tabs.try_prepare_closed_history_slot() {
                self.set_notice("Waiting for background tab resource reclamation");
                self.enforce_tab_resource_budget();
                return;
            }
        }
        if root_reclaim_pending {
            let Some(pending_root) = pending_root else {
                if closing_active {
                    self.shell.tabs.rollback_recent_inactive_transition();
                    let _ = self.load_tab_payload(index);
                }
                self.set_notice("Waiting for background tab resource reclamation");
                return;
            };
            if !self.try_retire_inactive_root_resources(index, pending_root) {
                if closing_active {
                    self.shell.tabs.rollback_recent_inactive_transition();
                    let _ = self.load_tab_payload(index);
                }
                self.set_notice("Waiting for background tab resource reclamation");
                return;
            }
        } else if self.shell.tabs.get(index).is_some_and(|tab| {
            tab.index_state.index_in_progress
                || tab.index_state.build_reclaim_pending
                || !self.shell.indexing.request_ids_for_tab(tab.id).is_empty()
        }) && !self.try_retire_tab_index_build_resources_for_boundary(index)
        {
            if closing_active {
                self.shell.tabs.rollback_recent_inactive_transition();
                let _ = self.load_tab_payload(index);
            }
            self.set_notice("Waiting for background tab resource reclamation");
            return;
        }
        self.clear_tab_drag_state();
        if !closing_active {
            self.sync_active_tab_state();
        } else if !self.shell.tabs.discard_recent_inactive_transition() {
            let _ = self.load_tab_payload(index);
            self.set_notice("Waiting for background tab resource reclamation");
            return;
        }
        let mut removed = self.shell.tabs.remove(index);
        let activation_refresh_pending = interrupted_index_before_close
            || removed.index_state.index_in_progress
            || matches!(
                removed.index_state.lifecycle(),
                super::TabResourceLifecycle::Dormant
                    | super::TabResourceLifecycle::Loading
                    | super::TabResourceLifecycle::Refreshing
                    | super::TabResourceLifecycle::Evicted
            );
        let search_refresh_pending = removed.search_in_progress;
        let sort_refresh_pending = removed.result_state.sort_in_progress;
        self.clear_closed_tab_state(removed.id);
        Self::trim_inactive_tab_preview(&mut removed);
        self.shell.tabs.touch_heavy_resource(removed.id);
        self.shell.tabs.push_closed_tab(ClosedTabState {
            tab: removed,
            original_index: index,
            activation_refresh_pending,
            search_refresh_pending,
            sort_refresh_pending,
        });
        if !closing_active && index < self.shell.tabs.active_tab_index() {
            self.shell
                .tabs
                .set_active_tab_index(self.shell.tabs.active_tab_index().saturating_sub(1));
        }
        if closing_active || self.shell.tabs.active_tab_index() >= self.shell.tabs.len() {
            self.shell
                .tabs
                .set_active_tab_index(index.min(self.shell.tabs.len().saturating_sub(1)));
        }
        if closing_active {
            let active_tab = self.shell.tabs.active_tab_index();
            let (results_compacted, preview_reload_pending) = self.load_tab_payload(active_tab);
            self.activate_tab_after_transition(
                results_compacted,
                preview_reload_pending,
                true,
                false,
                true,
            );
        } else {
            self.reapply_active_tab_state();
        }
        self.refresh_status_line_with_memory_sample();
        self.enforce_tab_resource_budget();
    }

    pub(super) fn restore_recently_closed_tab(&mut self) {
        let Some(closed_tab_id) = self.shell.tabs.last_closed_tab_id() else {
            self.set_notice("No closed tab to restore");
            return;
        };
        self.shell.tabs.pending_activation_tab_id = None;
        let previous_tab_id = self.current_tab_id();
        let previous_was_indexing = self.shell.indexing.pending_request_id.is_some();
        let previous_active = self.deactivate_active_tab_for_transition();
        let future_warm = if previous_was_indexing {
            previous_tab_id
        } else {
            self.shell.indexing.warm_tab_id
        };
        if !self
            .shell
            .tabs
            .enforce_resource_budget(Some(closed_tab_id), future_warm)
        {
            self.shell.tabs.rollback_recent_inactive_transition();
            let _ = self.load_tab_payload(previous_active);
            self.set_notice("Waiting for background tab resource reclamation");
            return;
        }
        if !self.shell.tabs.commit_recent_inactive_transition() {
            let _ = self.load_tab_payload(previous_active);
            self.set_notice("Waiting for background tab resource reclamation");
            return;
        }
        let closed_tab = self
            .shell
            .tabs
            .pop_closed_tab()
            .expect("closed tab checked before restore preflight");
        let activation_refresh_pending = closed_tab.activation_refresh_pending;
        let search_refresh_pending = closed_tab.search_refresh_pending;
        let sort_refresh_pending = closed_tab.sort_refresh_pending;
        let mut tab = closed_tab.tab;
        let interrupted_result_sort = (search_refresh_pending || sort_refresh_pending).then_some((
            tab.result_state.result_sort_mode,
            tab.result_state.result_sort_scope,
        ));
        if previous_was_indexing {
            self.shell.indexing.replace_warm_tab(previous_tab_id);
        }
        let id = self.shell.tabs.take_next_tab_id();
        Self::prepare_closed_tab_for_restore(&mut tab, id);
        if activation_refresh_pending {
            tab.index_state
                .apply_resource_transition(TabResourceTransition::Dormant);
        }
        let restore_index = closed_tab.original_index.min(self.shell.tabs.len());
        self.shell.tabs.insert(restore_index, tab);
        self.shell.tabs.set_active_tab_index(restore_index);
        let (results_compacted, preview_reload_pending) = self.load_tab_payload(restore_index);
        self.activate_tab_after_transition(
            results_compacted,
            preview_reload_pending,
            true,
            true,
            true,
        );
        if let Some((mode, scope)) = interrupted_result_sort {
            // A replacement index refresh resets sorting to Score. Restore the
            // interrupted result request's intent before reissuing it so both
            // the retained snapshot and the final index snapshot use the same
            // user-selected ordering contract.
            self.shell.runtime.result_sort_mode = mode;
            self.shell.runtime.result_sort_scope = scope;
        }
        if search_refresh_pending {
            self.enqueue_search_request();
        } else if sort_refresh_pending {
            self.apply_result_sort(false);
        }
        self.enforce_tab_resource_budget();
    }

    pub(super) fn move_tab(&mut self, from_index: usize, to_index: usize) {
        if from_index >= self.shell.tabs.len()
            || to_index >= self.shell.tabs.len()
            || from_index == to_index
        {
            return;
        }
        self.clear_tab_drag_state();
        self.sync_active_tab_state();
        let Some(active_tab_id) = self
            .shell
            .tabs
            .get(self.shell.tabs.active_tab_index())
            .map(|tab| tab.id)
        else {
            return;
        };
        let moved = self.shell.tabs.remove(from_index);
        self.shell.tabs.insert(to_index, moved);
        if let Some(new_active) = self.find_tab_index_by_id(active_tab_id) {
            self.shell.tabs.set_active_tab_index(new_active);
        }
        self.reapply_active_tab_state();
    }

    pub(super) fn activate_next_tab(&mut self) {
        if self.shell.tabs.len() <= 1 {
            return;
        }
        let next = (self.shell.tabs.active_tab_index() + 1) % self.shell.tabs.len();
        self.switch_to_tab_index(next);
    }

    pub(super) fn activate_previous_tab(&mut self) {
        if self.shell.tabs.len() <= 1 {
            return;
        }
        let next = if self.shell.tabs.active_tab_index() == 0 {
            self.shell.tabs.len() - 1
        } else {
            self.shell.tabs.active_tab_index() - 1
        };
        self.switch_to_tab_index(next);
    }

    pub(super) fn activate_tab_shortcut(&mut self, shortcut_number: usize) {
        let Some(target_index) = shortcut_number.checked_sub(1) else {
            return;
        };
        if target_index >= self.shell.tabs.len() || target_index >= 9 {
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
        let active_in_progress = self.shell.search.in_progress()
            || self.shell.worker_bus.preview.in_progress
            || self.shell.worker_bus.action.in_progress
            || self.shell.indexing.in_progress
            || self.shell.indexing.pending_finish.is_some()
            || self.shell.worker_bus.sort.in_progress;
        active_in_progress
            || self.shell.tabs.iter().enumerate().any(|(index, tab)| {
                index != self.shell.tabs.active_tab_index()
                    && (tab.search_in_progress
                        || tab.preview_in_progress
                        || tab.action_in_progress
                        || tab.index_state.index_in_progress
                        || tab.index_state.pending_index_finish.is_some()
                        || tab.result_state.sort_in_progress)
            })
    }

    pub(super) fn saved_tab_state_from_app(&self) -> SavedTabState {
        AppTabState::saved_from_shell(self, Self::history_persist_disabled())
    }

    fn saved_tab_state_from_tab(tab: &AppTabState) -> SavedTabState {
        tab.to_saved(Self::history_persist_disabled())
    }

    pub(super) fn saved_tabs_for_ui_state(&self) -> Vec<SavedTabState> {
        self.shell
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                if index == self.shell.tabs.active_tab_index() {
                    self.saved_tab_state_from_app()
                } else {
                    Self::saved_tab_state_from_tab(tab)
                }
            })
            .collect()
    }
}

use super::{
    result_reducer, BackgroundIndexState, Entry, FlistWalkerApp, IndexCoordinator, IndexEntry,
    IndexRequest, IndexResponse, IndexSource, PendingActiveIndexFinish, PipelineOwner,
    ResultSortMode,
};
use crate::app::index_coordinator::IndexResponseRoute;
use crate::app::index_response_arbitration::FrameMailboxArbitrator;
use crate::app::index_response_effects::{
    IndexFrameCompletionEffect, IndexResponseApplicationOwner, IndexResponseLoopControl,
    RoutedIndexResponse,
};
use crate::app::tab_state::TabResourceTransition;
use crate::app::tabs::BackgroundIndexResponseEffect;
use crate::path_utils::path_key;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

const MAX_PENDING_INDEX_ENTRIES: usize = 32_768;

impl FlistWalkerApp {
    fn pipeline_owner(&mut self) -> PipelineOwner<'_> {
        PipelineOwner::new(self)
    }

    fn cancel_stale_pending_after_index_for_active_root(&mut self) {
        let current_tab_id = self.current_tab_id().unwrap_or_default();
        if self
            .shell
            .features
            .filelist
            .workflow
            .pending_after_index
            .as_ref()
            .is_some_and(|pending| {
                pending.tab_id == current_tab_id
                    && path_key(&pending.root) != path_key(&self.shell.runtime.root)
            })
        {
            self.shell.features.filelist.workflow.pending_after_index = None;
            self.set_notice("Deferred Create File List canceled because root changed");
        }
    }

    fn reset_active_index_refresh_state(&mut self, reset_kind_resolution: bool) {
        debug_assert_eq!(self.shell.indexing.build.index.entries.capacity(), 0);
        self.shell.indexing.build.index.source = IndexSource::None;
        self.clear_preview_cache();
        self.clear_highlight_cache();
        debug_assert_eq!(
            self.shell
                .indexing
                .build
                .entry_kind_cache
                .entries
                .capacity(),
            0
        );
        debug_assert_eq!(
            self.shell.indexing.build.resolved_kind_updates.capacity(),
            0
        );
        debug_assert_eq!(
            self.shell
                .indexing
                .build
                .incremental_filtered_entries
                .capacity(),
            0
        );
        debug_assert_eq!(self.shell.indexing.build.pending_entries.capacity(), 0);
        self.shell.indexing.pending_entries_request_id = None;
        debug_assert!(self.shell.indexing.pending_finish.is_none());
        if reset_kind_resolution {
            self.reset_kind_resolution_state();
        } else {
            debug_assert_eq!(self.shell.indexing.build.pending_kind_paths.capacity(), 0);
            debug_assert_eq!(
                self.shell.indexing.build.pending_kind_paths_set.capacity(),
                0
            );
            debug_assert_eq!(self.shell.indexing.build.in_flight_kind_paths.capacity(), 0);
            self.shell.indexing.kind_resolution_in_progress = false;
            self.shell.indexing.kind_resolution_epoch =
                self.shell.indexing.kind_resolution_epoch.saturating_add(1);
        }
        self.shell.worker_bus.preview.pending_request_id = None;
        self.shell.worker_bus.preview.in_progress = false;
        self.shell.indexing.last_incremental_results_refresh = Instant::now();
        self.shell.indexing.last_search_snapshot_len = 0;
    }

    fn prepare_active_index_refresh_request(
        &mut self,
        request_id: u64,
        reset_kind_resolution: bool,
    ) {
        let query_non_empty = !self.shell.runtime.query_state.query.trim().is_empty();
        self.shell
            .indexing
            .begin_active_refresh(request_id, query_non_empty);
        self.shell.indexing.refresh_after_pending_finish = None;
        self.shell.indexing.root_after_pending_finish = None;
        self.shell.search.set_pending_request_id(None);
        self.shell.search.set_in_progress(false);
        self.reset_active_index_refresh_state(reset_kind_resolution);
    }

    pub(super) fn request_index_refresh(&mut self) {
        if self.shell.indexing.pending_finish.is_some() {
            self.shell.indexing.refresh_after_pending_finish =
                Some(super::PendingIndexRefreshMode::Normal);
            self.set_notice("Waiting for background tab resource reclamation");
            return;
        }
        if let Some(root) = self.shell.indexing.root_after_pending_finish.clone() {
            self.shell.indexing.refresh_after_pending_finish =
                Some(super::PendingIndexRefreshMode::Normal);
            self.apply_root_change_direct(root);
            return;
        }
        if !self.try_retire_active_index_build_resources() {
            self.shell.indexing.refresh_after_pending_finish =
                Some(super::PendingIndexRefreshMode::Normal);
            return;
        }
        self.ensure_entry_filters();
        self.invalidate_result_sort(true);
        self.clear_sort_metadata_cache();
        self.cancel_stale_pending_filelist_confirmations_for_active_root();
        self.cancel_stale_pending_after_index_for_active_root();
        let tab_id = self.current_tab_id();
        let request_id = self.shell.indexing.allocate_request_id(tab_id);
        self.prepare_active_index_refresh_request(request_id, false);
        self.refresh_status_line();

        let req = IndexRequest {
            request_id,
            tab_id: tab_id.unwrap_or_default(),
            root: self.shell.runtime.root.clone(),
            use_filelist: self.shell.runtime.use_filelist,
            include_files: self.shell.runtime.include_files,
            include_dirs: self.shell.runtime.include_dirs,
            max_depth: self.shell.runtime.max_depth,
            follow_links: self.shell.runtime.follow_links,
        };
        self.enqueue_index_request(req);
        self.dispatch_index_queue();
    }

    pub(super) fn request_create_filelist_walker_refresh(&mut self) {
        if self.shell.indexing.pending_finish.is_some() {
            self.shell.indexing.refresh_after_pending_finish =
                Some(super::PendingIndexRefreshMode::CreateFileListWalker);
            self.set_notice("Waiting for background tab resource reclamation");
            return;
        }
        if let Some(root) = self.shell.indexing.root_after_pending_finish.clone() {
            self.shell.indexing.refresh_after_pending_finish =
                Some(super::PendingIndexRefreshMode::CreateFileListWalker);
            self.apply_root_change_direct(root);
            return;
        }
        if !self.try_retire_active_index_build_resources() {
            self.shell.indexing.refresh_after_pending_finish =
                Some(super::PendingIndexRefreshMode::CreateFileListWalker);
            return;
        }
        self.cancel_stale_pending_filelist_confirmations_for_active_root();
        self.cancel_stale_pending_after_index_for_active_root();
        let tab_id = self.current_tab_id();
        let request_id = self.shell.indexing.allocate_request_id(tab_id);
        self.prepare_active_index_refresh_request(request_id, true);
        self.refresh_status_line();

        let req = IndexRequest {
            request_id,
            tab_id: tab_id.unwrap_or_default(),
            root: self.shell.runtime.root.clone(),
            use_filelist: false,
            include_files: self.shell.runtime.include_files,
            include_dirs: self.shell.runtime.include_dirs,
            max_depth: crate::indexer::MaxDepth::unlimited(),
            follow_links: self.shell.runtime.follow_links,
        };
        self.enqueue_index_request(req);
        self.dispatch_index_queue();
    }

    pub(super) fn request_background_index_refresh_for_tab(&mut self, tab_index: usize) {
        self.request_background_index_refresh_for_tab_with_mode(
            tab_index,
            super::PendingIndexRefreshMode::Normal,
        );
    }

    fn request_background_index_refresh_for_tab_with_mode(
        &mut self,
        tab_index: usize,
        mode: super::PendingIndexRefreshMode,
    ) {
        let Some(tab_id) = self.shell.tabs.get(tab_index).map(|tab| tab.id) else {
            return;
        };
        let pending_root = self
            .shell
            .tabs
            .get(tab_index)
            .and_then(|tab| tab.index_state.root_after_pending_finish.clone());
        if let Some(root) = pending_root {
            if let Some(tab) = self.shell.tabs.get_mut(tab_index) {
                tab.index_state.refresh_after_pending_finish = Some(mode);
            }
            let root_matches = self
                .shell
                .tabs
                .get(tab_index)
                .is_some_and(|tab| path_key(&tab.root) == path_key(&root));
            if root_matches {
                if let Some(tab) = self.shell.tabs.get_mut(tab_index) {
                    tab.index_state.root_after_pending_finish = None;
                }
            } else if !self.try_retire_inactive_root_resources(tab_index, root) {
                return;
            }
        }
        if let Some(tab) = self.shell.tabs.get_mut(tab_index) {
            if tab.index_state.pending_index_finish.is_some() {
                tab.index_state.refresh_after_pending_finish = Some(mode);
                tab.notice = "Waiting for background tab resource reclamation".to_string();
                return;
            }
        }
        if !self
            .shell
            .tabs
            .enforce_resource_budget(self.current_tab_id(), Some(tab_id))
        {
            if let Some(tab) = self.shell.tabs.get_mut(tab_index) {
                tab.index_state.refresh_after_pending_finish = Some(mode);
                tab.notice = "Waiting for background tab resource reclamation".to_string();
            }
            return;
        }
        if !self.try_retire_tab_index_build_resources(tab_index) {
            if let Some(tab) = self.shell.tabs.get_mut(tab_index) {
                tab.index_state.refresh_after_pending_finish = Some(mode);
            }
            return;
        }
        let shell = &mut self.shell;
        let (tabs, indexing) = (&mut shell.tabs, &mut shell.indexing);
        indexing.replace_warm_tab(Some(tab_id));
        let request_id = indexing.allocate_request_id(Some(tab_id));

        let Some(tab) = tabs.get_mut(tab_index) else {
            indexing.cleanup_request(request_id);
            return;
        };
        tab.index_state.refresh_after_pending_finish = None;
        tab.index_state.root_after_pending_finish = None;
        indexing.begin_background_refresh(tab, request_id, "Refreshing from created FileList");

        let req = IndexRequest {
            request_id,
            tab_id,
            root: tab.root.clone(),
            use_filelist: match mode {
                super::PendingIndexRefreshMode::Normal => tab.use_filelist,
                super::PendingIndexRefreshMode::CreateFileListWalker => false,
            },
            include_files: tab.include_files,
            include_dirs: tab.include_dirs,
            follow_links: tab.follow_links,
            max_depth: match mode {
                super::PendingIndexRefreshMode::Normal => tab.max_depth,
                super::PendingIndexRefreshMode::CreateFileListWalker => {
                    crate::indexer::MaxDepth::unlimited()
                }
            },
        };
        self.enqueue_index_request(req);
        self.dispatch_index_queue();
    }

    fn handle_index_worker_unavailable(&mut self) {
        let notice = "Index worker is unavailable".to_string();
        let active_request_id = self.shell.indexing.pending_request_id;
        {
            let shell = &mut self.shell;
            let (tabs, indexing, features) =
                (&mut shell.tabs, &mut shell.indexing, &mut shell.features);
            let affected_tab_ids: HashSet<u64> = indexing.request_tabs.values().copied().collect();

            features.filelist.workflow.pending_after_index = None;
            features
                .filelist
                .workflow
                .pending_index_completion_notices
                .clear();
            indexing.pending_queue.clear();
            indexing.inflight_requests.clear();

            indexing.clear_active_request_state();
            indexing.build_reclaim_pending = active_request_id.is_some();
            indexing.build_reclaim_request_id = active_request_id;
            indexing.apply_resource_transition(TabResourceTransition::Failure);

            for tab in tabs {
                if affected_tab_ids.contains(&tab.id)
                    || tab.index_state.pending_index_request_id.is_some()
                {
                    tab.index_state.index_in_progress = false;
                    tab.index_state.build_reclaim_pending = true;
                    tab.index_state.build_reclaim_request_id =
                        tab.index_state.pending_index_request_id;
                    tab.index_state
                        .apply_resource_transition(TabResourceTransition::Failure);
                    tab.notice = notice.clone();
                }
            }
        }
        if self.shell.indexing.build_reclaim_pending {
            self.try_retire_active_index_build_resources();
        }
        let affected_tabs = self
            .shell
            .tabs
            .iter()
            .enumerate()
            .filter_map(|(index, tab)| tab.index_state.build_reclaim_pending.then_some(index))
            .collect::<Vec<_>>();
        for tab_index in affected_tabs {
            self.try_retire_tab_index_build_resources(tab_index);
        }
        self.set_notice(notice.clone());
    }

    pub(super) fn maybe_reindex_from_filter_toggles(
        &mut self,
        use_filelist_changed: bool,
        files_changed: bool,
        dirs_changed: bool,
    ) {
        let user_changed_filter = use_filelist_changed || files_changed || dirs_changed;
        let mut reindex = use_filelist_changed;
        reindex |= files_changed || dirs_changed;
        if self.use_filelist_requires_locked_filters()
            && (!self.shell.runtime.include_files || !self.shell.runtime.include_dirs)
        {
            self.shell.runtime.include_files = true;
            self.shell.runtime.include_dirs = true;
            reindex = true;
        }
        reindex |= self.ensure_entry_filters();
        if reindex {
            if user_changed_filter {
                self.shell.tabs.mark_active_tab_meaningfully_engaged();
            }
            self.request_index_refresh();
        }
    }

    pub(super) fn enqueue_index_request(&mut self, req: IndexRequest) {
        self.shell
            .features
            .filelist
            .workflow
            .pending_index_completion_notices
            .retain(|_, pending| pending.tab_id != req.tab_id);
        let active_tab_id = self.current_tab_id().unwrap_or_default();
        // Requests accepted by the worker channel remain physically queued or running until
        // their terminal response arrives. Keep them in coordinator accounting even after a
        // replacement becomes latest; otherwise later active-tab work can be accepted behind
        // an invisible FIFO backlog and appear not to start for seconds.
        let stale_queued = self
            .shell
            .indexing
            .pending_queue
            .iter()
            .filter_map(|queued| (queued.tab_id == req.tab_id).then_some(queued.request_id))
            .collect::<Vec<_>>();
        self.shell
            .indexing
            .pending_queue
            .retain(|queued| queued.tab_id != req.tab_id);
        for request_id in stale_queued {
            self.discard_filelist_index_completion_notice(request_id);
            self.shell.indexing.cleanup_request(request_id);
        }
        self.shell.indexing.pending_queue.push_back(req);

        while self.shell.indexing.pending_queue.len() > Self::INDEX_MAX_QUEUE {
            let drop_idx = self
                .shell
                .indexing
                .pending_queue
                .iter()
                .position(|queued| queued.tab_id != active_tab_id)
                .unwrap_or(0);
            let dropped = self.shell.indexing.pending_queue.remove(drop_idx);
            if let Some(dropped) = dropped {
                self.discard_filelist_index_completion_notice(dropped.request_id);
                let dropped_is_latest = self
                    .shell
                    .indexing
                    .latest_request_ids
                    .lock()
                    .map(|latest| latest.get(&dropped.tab_id).copied() == Some(dropped.request_id))
                    .unwrap_or(true);
                let has_queued_replacement = self
                    .shell
                    .indexing
                    .pending_queue
                    .iter()
                    .any(|queued| queued.tab_id == dropped.tab_id);
                if let Some(tab_index) = self.find_tab_index_by_id(dropped.tab_id) {
                    if let Some(tab) = self.shell.tabs.get_mut(tab_index) {
                        if tab.index_state.pending_index_request_id == Some(dropped.request_id) {
                            tab.index_state.pending_index_request_id = None;
                            tab.index_state.index_in_progress = false;
                            tab.notice = "Index request dropped due to queue limit".to_string();
                        }
                    }
                }
                if dropped.tab_id != active_tab_id && dropped_is_latest && !has_queued_replacement {
                    self.settle_tab_canceled_generation(dropped.tab_id);
                }
                self.shell.indexing.cleanup_request(dropped.request_id);
            }
        }
    }

    pub(super) fn queued_request_for_tab_exists(&self, tab_id: u64) -> bool {
        self.shell
            .indexing
            .pending_queue
            .iter()
            .any(|req| req.tab_id == tab_id)
    }

    pub(super) fn has_inflight_for_tab(&self, tab_id: u64) -> bool {
        self.shell
            .indexing
            .inflight_requests
            .iter()
            .any(|request_id| {
                self.shell
                    .indexing
                    .request_tabs
                    .get(request_id)
                    .is_some_and(|rid_tab_id| *rid_tab_id == tab_id)
            })
    }

    pub(super) fn pop_next_index_request(&mut self) -> Option<IndexRequest> {
        let active_tab_id = self.current_tab_id()?;
        if let Some(pos) =
            self.shell.indexing.pending_queue.iter().position(|req| {
                req.tab_id == active_tab_id && !self.has_inflight_for_tab(req.tab_id)
            })
        {
            return self.shell.indexing.pending_queue.remove(pos);
        }
        if self.shell.indexing.background_finalizations.is_full() {
            return None;
        }
        if let Some(pos) = self
            .shell
            .indexing
            .pending_queue
            .iter()
            .position(|req| !self.has_inflight_for_tab(req.tab_id))
        {
            return self.shell.indexing.pending_queue.remove(pos);
        }
        None
    }

    pub(super) fn preempt_background_for_active_request(&mut self) -> bool {
        let Some(active_tab_id) = self.current_tab_id() else {
            return false;
        };
        if !self.queued_request_for_tab_exists(active_tab_id) {
            return false;
        }
        if self.shell.indexing.inflight_requests.len() < Self::INDEX_MAX_CONCURRENT {
            return false;
        }

        let mut victims: Vec<(u64, u64, u64)> = self
            .shell
            .indexing
            .inflight_requests
            .iter()
            .filter_map(|request_id| {
                let tab_id = self.shell.indexing.request_tabs.get(request_id).copied()?;
                if tab_id == active_tab_id {
                    return None;
                }
                let replacement_request_id = self
                    .shell
                    .indexing
                    .pending_queue
                    .iter()
                    .rev()
                    .find(|req| req.tab_id == tab_id)
                    .map(|req| req.request_id)
                    .unwrap_or(0);
                Some((*request_id, tab_id, replacement_request_id))
            })
            .collect();
        victims.sort_unstable_by_key(|(request_id, _, _)| *request_id);
        let victim = victims
            .iter()
            .copied()
            .find(|(_, tab_id, _)| Some(*tab_id) != self.shell.indexing.warm_tab_id)
            .or_else(|| victims.first().copied());
        let Some((_, tab_id, replacement_request_id)) = victim else {
            return false;
        };

        let Ok(mut latest) = self.shell.indexing.latest_request_ids.lock() else {
            return false;
        };
        if latest.get(&tab_id).copied() == Some(replacement_request_id) {
            return false;
        }
        latest.insert(tab_id, replacement_request_id);
        drop(latest);
        if replacement_request_id == 0 {
            self.settle_tab_canceled_generation(tab_id);
        }
        true
    }

    pub(super) fn dispatch_index_queue(&mut self) {
        loop {
            if self.shell.indexing.inflight_requests.len() >= Self::INDEX_MAX_CONCURRENT {
                let _ = self.preempt_background_for_active_request();
                break;
            }
            let Some(req) = self.pop_next_index_request() else {
                break;
            };
            let req_id = req.request_id;
            let req_tab_id = req.tab_id;
            match self.shell.indexing.tx.try_send(req) {
                Ok(()) => {
                    super::worker::channel::trace_worker_load(
                        &self.shell.indexing.tx,
                        "index",
                        "accepted",
                        super::worker::channel::WorkerTraceContext {
                            worker_id: "ui-dispatch",
                            request_id: Some(req_id),
                            tab_id: Some(req_tab_id),
                            epoch: None,
                            outcome: "accepted",
                        },
                    );
                    self.shell.indexing.inflight_requests.insert(req_id);
                }
                Err(std::sync::mpsc::TrySendError::Full(req)) => {
                    super::worker::channel::trace_worker_load(
                        &self.shell.indexing.tx,
                        "index",
                        "full",
                        super::worker::channel::WorkerTraceContext {
                            worker_id: "ui-dispatch",
                            request_id: Some(req_id),
                            tab_id: Some(req_tab_id),
                            epoch: None,
                            outcome: "full",
                        },
                    );
                    self.shell.indexing.pending_queue.push_front(req);
                    break;
                }
                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                    super::worker::channel::trace_worker_load(
                        &self.shell.indexing.tx,
                        "index",
                        "disconnected",
                        super::worker::channel::WorkerTraceContext {
                            worker_id: "ui-dispatch",
                            request_id: Some(req_id),
                            tab_id: Some(req_tab_id),
                            epoch: None,
                            outcome: "disconnected",
                        },
                    );
                    self.handle_index_worker_unavailable();
                    break;
                }
            }
        }
    }

    fn enqueue_search_request_for_tab_index(&mut self, tab_index: usize) {
        self.pipeline_owner()
            .enqueue_search_request_for_tab_index(tab_index);
    }

    pub(super) fn handle_background_index_response(
        &mut self,
        tab_index: usize,
        msg: IndexResponse,
    ) {
        let request_id = IndexCoordinator::response_request_id(&msg);
        let completion_notice_can_apply = self
            .shell
            .tabs
            .get(tab_index)
            .is_some_and(|tab| tab.index_state.pending_index_request_id == Some(request_id));
        let restore_completion_notice = matches!(
            &msg,
            IndexResponse::Finished { .. } | IndexResponse::Canceled { .. }
        );
        let terminal = IndexCoordinator::is_terminal_response(&msg);
        if terminal {
            self.shell.indexing.inflight_requests.remove(&request_id);
        }
        let BackgroundIndexResponseEffect {
            trigger_search,
            cleanup_request_id,
            deferred_filelist,
            follow_up,
            reclaim_build_request_id,
        } = self.apply_background_index_response(tab_index, msg);

        if terminal && cleanup_request_id.is_some() {
            let owner = self
                .shell
                .tabs
                .get(tab_index)
                .map(|tab| (tab.id, tab.root.clone()));
            if completion_notice_can_apply && restore_completion_notice {
                if let Some((tab_id, root)) = owner {
                    if let Some(notice) =
                        self.take_filelist_index_completion_notice(request_id, tab_id, &root)
                    {
                        if let Some(tab) = self.shell.tabs.get_mut(tab_index) {
                            tab.notice = notice;
                        }
                    }
                }
            } else {
                self.discard_filelist_index_completion_notice(request_id);
            }
        }

        if let Some((tab_id, root, entries)) = deferred_filelist {
            self.request_filelist_creation(tab_id, root, entries);
        }
        if trigger_search {
            self.enqueue_search_request_for_tab_index(tab_index);
        }
        if let Some(request_id) = cleanup_request_id {
            self.shell.indexing.cleanup_request(request_id);
        }
        if let Some(request_id) = reclaim_build_request_id {
            self.try_reclaim_failed_background_index_build(tab_index, request_id);
        }
        if let Some(mode) = follow_up {
            self.request_background_index_refresh_for_tab_with_mode(tab_index, mode);
        }
        self.enforce_tab_resource_budget();
        self.dispatch_index_queue();
    }

    fn try_reclaim_failed_background_index_build(
        &mut self,
        tab_index: usize,
        request_id: u64,
    ) -> bool {
        if let Some(tab) = self.shell.tabs.get_mut(tab_index) {
            tab.index_state.build_reclaim_pending = true;
            tab.index_state.build_reclaim_request_id = Some(request_id);
        }
        self.try_retire_tab_index_build_resources(tab_index)
    }

    pub(super) fn retry_pending_background_index_finish(&mut self) {
        let pending = self
            .shell
            .tabs
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != self.shell.tabs.active_tab_index())
            .find_map(|(index, tab)| {
                tab.index_state
                    .pending_index_finish
                    .as_ref()
                    .map(|pending| (index, pending.request_id, pending.source.clone()))
            });
        if let Some((tab_index, request_id, source)) = pending {
            self.handle_background_index_response(
                tab_index,
                IndexResponse::Finished { request_id, source },
            );
            return;
        }
        let pending_build_reclaim = self
            .shell
            .tabs
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != self.shell.tabs.active_tab_index())
            .find_map(|(index, tab)| {
                tab.index_state
                    .build_reclaim_pending
                    .then_some((index, tab.index_state.build_reclaim_request_id))
            });
        if let Some((tab_index, request_id)) = pending_build_reclaim {
            let reclaimed = if let Some(request_id) = request_id {
                self.try_reclaim_failed_background_index_build(tab_index, request_id)
            } else {
                self.try_retire_tab_index_build_resources(tab_index)
            };
            if reclaimed {
                let follow_up = self
                    .shell
                    .tabs
                    .get(tab_index)
                    .and_then(|tab| tab.index_state.refresh_after_pending_finish);
                if let Some(mode) = follow_up {
                    self.request_background_index_refresh_for_tab_with_mode(tab_index, mode);
                }
            }
            return;
        }
        let deferred = self
            .shell
            .tabs
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != self.shell.tabs.active_tab_index())
            .find_map(|(index, tab)| {
                (tab.index_state.pending_index_request_id.is_none()
                    && tab.index_state.pending_index_finish.is_none())
                .then_some(tab.index_state.refresh_after_pending_finish)
                .flatten()
                .map(|mode| {
                    (
                        index,
                        mode,
                        tab.index_state.root_after_pending_finish.clone(),
                    )
                })
            });
        if let Some((tab_index, mode, root)) = deferred {
            if let Some(root) = root {
                if !self.try_retire_inactive_root_resources(tab_index, root) {
                    return;
                }
            }
            self.request_background_index_refresh_for_tab_with_mode(tab_index, mode);
        }
    }

    pub(super) fn retry_pending_active_root_change(&mut self) {
        if self.shell.indexing.pending_finish.is_some() {
            return;
        }
        let Some(root) = self.shell.indexing.root_after_pending_finish.clone() else {
            return;
        };
        self.apply_root_change_direct(root);
    }

    pub(super) fn retry_pending_active_index_build_reclaim(&mut self) {
        if !self.shell.indexing.build_reclaim_pending
            || self.shell.indexing.pending_finish.is_some()
            || self.shell.indexing.root_after_pending_finish.is_some()
        {
            return;
        }
        if !self.try_retire_active_index_build_resources() {
            return;
        }
        if let Some(mode) = self.shell.indexing.refresh_after_pending_finish {
            match mode {
                super::PendingIndexRefreshMode::Normal => self.request_index_refresh(),
                super::PendingIndexRefreshMode::CreateFileListWalker => {
                    self.request_create_filelist_walker_refresh()
                }
            }
        }
    }

    pub(super) fn retry_pending_stale_build_reclaim(&mut self) -> bool {
        let Some((cleanup_request_id, resources)) =
            self.shell.indexing.pending_stale_build_reclaim.take()
        else {
            return true;
        };
        let superseded_tab_id = cleanup_request_id
            .filter(|request_id| self.shell.indexing.is_superseded_request(*request_id))
            .and_then(|request_id| self.shell.indexing.request_tabs.get(&request_id).copied());
        let mailboxes_to_close = resources.mailbox_handles();
        match self.shell.tabs.try_retire_index_build_resources(resources) {
            Ok(()) => {
                for mailbox in mailboxes_to_close {
                    mailbox.close();
                }
                if let Some(request_id) = cleanup_request_id {
                    if let Some(tab_id) = self.shell.indexing.request_tabs.get(&request_id).copied()
                    {
                        if let Some(tab_index) = self.find_tab_index_by_id(tab_id) {
                            if let Some(tab) = self.shell.tabs.get_mut(tab_index) {
                                if tab.index_state.pending_index_request_id == Some(request_id) {
                                    tab.index_state.clear_index_request_state();
                                    tab.index_state.build_reclaim_pending = false;
                                    if Some(tab.id) == superseded_tab_id {
                                        tab.index_state.apply_resource_transition(
                                            TabResourceTransition::Cancel,
                                        );
                                    }
                                }
                            }
                        }
                    }
                    self.shell
                        .indexing
                        .cleanup_stale_terminal_response(request_id);
                }
                true
            }
            Err(resources) => {
                self.shell.indexing.pending_stale_build_reclaim =
                    Some((cleanup_request_id, *resources));
                false
            }
        }
    }

    pub(super) fn stage_stale_data_reclaim(&mut self, entries: Vec<IndexEntry>) -> bool {
        let mut resources = super::tab_resources::RetiredIndexBuildResources::empty();
        resources.set_stale_index_entries(entries);
        match self.shell.tabs.try_retire_index_build_resources(resources) {
            Ok(()) => true,
            Err(resources) => {
                self.shell.indexing.pending_stale_build_reclaim = Some((None, *resources));
                false
            }
        }
    }

    pub(super) fn stage_stale_terminal_reclaim(&mut self, request_id: u64) -> bool {
        let superseded = self.shell.indexing.is_superseded_request(request_id);
        let tab_index =
            self.shell
                .indexing
                .request_tabs
                .get(&request_id)
                .copied()
                .and_then(|tab_id| self.find_tab_index_by_id(tab_id))
                .filter(|tab_index| {
                    self.shell.tabs.get(*tab_index).is_some_and(|tab| {
                        tab.index_state.pending_index_request_id == Some(request_id)
                    })
                });
        let mut resources = tab_index
            .map(|tab_index| {
                let tab = self.shell.tabs.get_mut(tab_index).expect("validated tab");
                tab.index_state.build_reclaim_pending = true;
                tab.take_index_build_resources()
            })
            .unwrap_or_else(super::tab_resources::RetiredIndexBuildResources::empty);
        let background = self
            .shell
            .indexing
            .background_states
            .remove(&request_id)
            .map(|state| vec![(request_id, state)])
            .unwrap_or_default();
        resources.set_background_states(background);
        let finalizations = self
            .shell
            .indexing
            .take_background_finalizations_for_requests(&[request_id]);
        resources.set_background_finalizations(finalizations);
        let mailboxes = self
            .shell
            .indexing
            .take_mailboxes_for_requests(&[request_id]);
        let mailboxes_to_close = mailboxes
            .iter()
            .map(|(_, mailbox)| Arc::clone(mailbox))
            .collect::<Vec<_>>();
        resources.set_mailboxes(mailboxes);
        match self.shell.tabs.try_retire_index_build_resources(resources) {
            Ok(()) => {
                for mailbox in mailboxes_to_close {
                    mailbox.close();
                }
                if let Some(tab_index) = tab_index {
                    let tab = self.shell.tabs.get_mut(tab_index).expect("validated tab");
                    tab.index_state.clear_index_request_state();
                    tab.index_state.build_reclaim_pending = false;
                    if superseded {
                        tab.index_state
                            .apply_resource_transition(TabResourceTransition::Cancel);
                    }
                }
                self.shell
                    .indexing
                    .cleanup_stale_terminal_response(request_id);
                true
            }
            Err(resources) => {
                self.shell.indexing.pending_stale_build_reclaim =
                    Some((Some(request_id), *resources));
                false
            }
        }
    }

    pub(super) fn try_apply_replace_all_response(&mut self, msg: IndexResponse) -> bool {
        let IndexResponse::ReplaceAll {
            request_id,
            entries,
        } = msg
        else {
            unreachable!("replace-all retry received a different response")
        };
        match self.shell.indexing.route_response(request_id) {
            IndexResponseRoute::Active => {
                let background_states = self
                    .shell
                    .indexing
                    .take_background_states_for_requests(&[request_id]);
                let mut resources = self.take_active_index_build_resources();
                resources.set_background_states(background_states);
                match self.shell.tabs.try_retire_index_build_resources(resources) {
                    Ok(()) => {
                        self.shell.indexing.pending_entries_request_id = None;
                        self.queue_index_batch(request_id, entries);
                        true
                    }
                    Err(resources) => {
                        let mut resources = *resources;
                        self.shell
                            .indexing
                            .restore_background_states(resources.take_background_states());
                        self.restore_active_index_build_resources(resources);
                        self.shell.indexing.pending_replace_all = Some(IndexResponse::ReplaceAll {
                            request_id,
                            entries,
                        });
                        self.set_notice("Waiting for background tab resource reclamation");
                        false
                    }
                }
            }
            IndexResponseRoute::Background(tab_id) => {
                let Some(tab_index) = self.find_tab_index_by_id(tab_id) else {
                    return self.stage_stale_data_reclaim(entries);
                };
                if self
                    .shell
                    .tabs
                    .get(tab_index)
                    .is_none_or(|tab| tab.index_state.pending_index_request_id != Some(request_id))
                {
                    return self.stage_stale_data_reclaim(entries);
                }
                let source = self
                    .shell
                    .indexing
                    .background_states
                    .get(&request_id)
                    .and_then(|state| state.source.clone())
                    .or_else(|| {
                        self.shell
                            .tabs
                            .get(tab_index)
                            .map(|tab| tab.index_state.build.index.source.clone())
                    });
                let background_states = self
                    .shell
                    .indexing
                    .take_background_states_for_requests(&[request_id]);
                let mut resources = self
                    .shell
                    .tabs
                    .get_mut(tab_index)
                    .expect("validated replacement tab")
                    .take_index_build_resources();
                resources.set_background_states(background_states);
                match self.shell.tabs.try_retire_index_build_resources(resources) {
                    Ok(()) => {
                        self.shell.indexing.background_states.insert(
                            request_id,
                            BackgroundIndexState {
                                source,
                                entries: entries.into_iter().map(Into::into).collect(),
                                replaced: true,
                            },
                        );
                        true
                    }
                    Err(resources) => {
                        let mut resources = *resources;
                        self.shell
                            .indexing
                            .restore_background_states(resources.take_background_states());
                        self.shell
                            .tabs
                            .get_mut(tab_index)
                            .expect("validated replacement tab")
                            .restore_index_build_resources(resources);
                        self.shell.indexing.pending_replace_all = Some(IndexResponse::ReplaceAll {
                            request_id,
                            entries,
                        });
                        if let Some(tab) = self.shell.tabs.get_mut(tab_index) {
                            tab.notice =
                                "Waiting for background tab resource reclamation".to_string();
                        }
                        false
                    }
                }
            }
            IndexResponseRoute::Stale => self.stage_stale_data_reclaim(entries),
        }
    }

    pub(super) fn poll_index_response(&mut self) {
        self.poll_index_response_with_budget(Duration::from_millis(4));
    }

    #[cfg(test)]
    pub(super) fn poll_index_response_with_budget_for_test(&mut self, budget: Duration) {
        self.poll_index_response_with_budget(budget);
    }

    fn poll_index_response_with_budget(&mut self, frame_budget: Duration) {
        if !IndexResponseApplicationOwner::new(self).prepare_frame() {
            return;
        }
        const MAX_MESSAGES_PER_FRAME: usize = 64;
        #[cfg(test)]
        const MAX_PRIORITY_ROUTE_MESSAGES_PER_FRAME: usize = 4_096;
        const MAX_INDEX_ENTRIES_PER_FRAME: usize = 32_768;
        const MAX_POST_FINISH_INDEX_ENTRIES_PER_FRAME: usize = 2_048;

        let frame_start = Instant::now();
        let mut processed = 0usize;
        #[cfg(test)]
        let mut priority_routed = 0usize;
        let mut mailbox_arbitrator = FrameMailboxArbitrator::default();
        let mut has_index_progress = false;
        loop {
            let active_mailbox_blocked = self
                .shell
                .indexing
                .active_mailbox_blocked(MAX_PENDING_INDEX_ENTRIES);
            let Some(arbitrated) =
                mailbox_arbitrator.try_next(&mut self.shell.indexing, active_mailbox_blocked)
            else {
                break;
            };
            let request_id = IndexCoordinator::response_request_id(&arbitrated.response);
            let route = self.shell.indexing.route_response(request_id);
            let effect = IndexResponseApplicationOwner::new(self).apply(RoutedIndexResponse {
                route,
                response: arbitrated.response,
                #[cfg(test)]
                active_mailbox_blocked,
                #[cfg(test)]
                from_shared_response_queue: arbitrated.from_shared_response_queue,
            });
            processed = processed.saturating_add(effect.processed_messages);
            has_index_progress |= effect.index_progress;

            #[cfg(test)]
            if effect.priority_routed {
                priority_routed = priority_routed.saturating_add(1);
                if priority_routed >= MAX_PRIORITY_ROUTE_MESSAGES_PER_FRAME
                    || frame_start.elapsed() >= frame_budget
                {
                    break;
                }
                continue;
            }

            if effect.control == IndexResponseLoopControl::Break
                || processed >= MAX_MESSAGES_PER_FRAME
                || frame_start.elapsed() >= frame_budget
            {
                break;
            }
        }

        let completion = IndexResponseApplicationOwner::new(self).complete_frame(
            frame_start,
            frame_budget,
            has_index_progress,
            MAX_INDEX_ENTRIES_PER_FRAME,
            MAX_POST_FINISH_INDEX_ENTRIES_PER_FRAME,
        );
        match completion {
            IndexFrameCompletionEffect::DispatchIndexQueue => self.dispatch_index_queue(),
        }
    }

    fn ensure_entry_filters(&mut self) -> bool {
        if !self.shell.runtime.include_files && !self.shell.runtime.include_dirs {
            self.shell.runtime.include_files = true;
            return true;
        }
        false
    }

    pub(super) fn apply_results_with_selection_policy(
        &mut self,
        results: Vec<(PathBuf, f64)>,
        keep_scroll_position: bool,
        preserve_selected_path: bool,
    ) {
        self.pipeline_owner().apply_results_with_selection_policy(
            results,
            keep_scroll_position,
            preserve_selected_path,
        );
    }

    pub(super) fn enqueue_search_request(&mut self) {
        self.pipeline_owner().enqueue_search_request();
    }

    pub(super) fn poll_search_response(&mut self) {
        self.pipeline_owner().poll_search_response();
    }

    pub(super) fn update_results(&mut self) {
        self.pipeline_owner().update_results();
    }

    pub(super) fn queue_index_batch(&mut self, request_id: u64, entries: Vec<IndexEntry>) {
        if self.shell.indexing.pending_entries_request_id != Some(request_id) {
            self.shell.indexing.build.pending_entries.clear();
            self.shell.indexing.pending_entries_request_id = Some(request_id);
        }
        self.shell.indexing.build.pending_entries.extend(entries);
    }

    fn ingest_index_entry(&mut self, entry: IndexEntry) {
        let entry: Entry = entry.into();
        if let Some(kind) = entry.kind {
            self.shell
                .indexing
                .build
                .entry_kind_cache
                .set(entry.path.clone(), kind);
        }
        if entry.kind.is_none_or(|kind| kind.needs_resolution())
            && self.kind_resolution_needed_for_filters()
        {
            self.queue_kind_resolution(entry.path.clone());
        }
        let compiled_ignore_terms = self.compiled_ignore_terms();
        if self.should_track_incremental_filtered_entries()
            && self.is_entry_visible_for_current_filter(&entry, compiled_ignore_terms.as_deref())
        {
            self.shell
                .indexing
                .build
                .incremental_filtered_entries
                .push(entry.clone());
        }
        self.shell.indexing.build.index.entries.push(entry);
    }

    pub(super) fn drain_queued_index_entries(
        &mut self,
        request_id: u64,
        max_entries: usize,
    ) -> bool {
        if self.shell.indexing.pending_entries_request_id != Some(request_id) {
            return false;
        }
        let mut processed = 0usize;
        while processed < max_entries {
            let Some(entry) = self.shell.indexing.build.pending_entries.pop_front() else {
                break;
            };
            self.ingest_index_entry(entry);
            processed = processed.saturating_add(1);
        }
        if self.shell.indexing.build.pending_entries.is_empty() {
            self.shell.indexing.pending_entries_request_id = None;
        }
        processed > 0
    }

    pub(super) fn drain_queued_index_entries_with_budget(
        &mut self,
        request_id: u64,
        frame_start: Instant,
        budget: Duration,
        max_entries: usize,
    ) -> bool {
        if self.shell.indexing.pending_entries_request_id != Some(request_id) {
            return false;
        }
        let mut processed = 0usize;
        while processed < max_entries && frame_start.elapsed() < budget {
            let Some(entry) = self.shell.indexing.build.pending_entries.pop_front() else {
                break;
            };
            self.ingest_index_entry(entry);
            processed = processed.saturating_add(1);
        }
        if self.shell.indexing.build.pending_entries.is_empty() {
            self.shell.indexing.pending_entries_request_id = None;
        }
        processed > 0
    }

    fn should_track_incremental_filtered_entries(&self) -> bool {
        !self.shell.runtime.query_state.query.trim().is_empty()
            || !self.shell.runtime.include_files
            || !self.shell.runtime.include_dirs
            || (self.shell.ui.ignore_list_enabled
                && !self.shell.runtime.ignore_list_terms.is_empty())
    }

    pub(super) fn stage_promoted_background_finalization(
        &mut self,
        request_id: u64,
        source: IndexSource,
    ) -> bool {
        if !self
            .shell
            .indexing
            .background_states
            .contains_key(&request_id)
        {
            return false;
        }
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
            if !requeued {
                self.set_notice("Index terminal mailbox became unavailable");
                self.shell.indexing.build_reclaim_pending = true;
                self.shell.indexing.build_reclaim_request_id = Some(request_id);
            } else {
                self.set_notice("Waiting for background tab finalization slot");
            }
            self.shell.indexing.in_progress = false;
            return true;
        }

        let mut state = self
            .shell
            .indexing
            .background_states
            .remove(&request_id)
            .expect("promoted background state");
        let source = state.source.take().unwrap_or(source);
        let existing_entries = std::mem::take(&mut self.shell.indexing.build.index.entries);
        let pending_entries = if self.shell.indexing.pending_entries_request_id == Some(request_id)
        {
            self.shell.indexing.pending_entries_request_id = None;
            std::mem::take(&mut self.shell.indexing.build.pending_entries)
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
        let tab_id = self.current_tab_id().unwrap_or_default();
        let capture_filelist_paths = self
            .shell
            .features
            .filelist
            .workflow
            .pending_after_index
            .as_ref()
            .is_some_and(|pending| {
                pending.tab_id == tab_id
                    && path_key(&pending.root) == path_key(&self.shell.runtime.root)
            });
        let finalization = super::PendingBackgroundIndexFinalize::new(
            super::BackgroundIndexFinalizeIdentity {
                tab_id,
                request_id,
                source: source.clone(),
            },
            super::BackgroundIndexFinalizePolicy {
                include_files: self.shell.runtime.include_files,
                include_dirs: self.shell.runtime.include_dirs,
                root: self.shell.runtime.root.clone(),
                prefer_relative: Self::prefer_relative_display_for(&source),
                ignore_case: self.shell.runtime.ignore_case,
                ignore_list_enabled: self.shell.ui.ignore_list_enabled,
                ignore_terms_source: Arc::clone(&self.shell.runtime.ignore_list_terms),
            },
            super::BackgroundIndexFinalizeInputs {
                initial_entries,
                pending_entries: selected_pending_entries,
                continuation_entries,
                discarded_entries,
                discarded_pending_entries,
                capture_filelist_paths,
            },
        );
        self.shell
            .indexing
            .background_finalizations
            .insert(request_id, finalization);
        self.shell.indexing.pending_finish = Some(PendingActiveIndexFinish { request_id, source });
        self.shell.indexing.in_progress = false;
        true
    }

    pub(super) fn try_finish_active_index_after_pending_drain(&mut self) -> bool {
        let Some(pending_finish) = self.shell.indexing.pending_finish.clone() else {
            return false;
        };
        if self.shell.indexing.pending_entries_request_id == Some(pending_finish.request_id)
            && !self.shell.indexing.build.pending_entries.is_empty()
        {
            return false;
        }
        if self
            .shell
            .indexing
            .background_finalizations
            .contains_key(&pending_finish.request_id)
            && !self
                .advance_request_owned_index_finalization(pending_finish.request_id)
                .unwrap_or(false)
        {
            self.set_notice("Finalizing background tab snapshot");
            return false;
        }
        if let Some(root) = self.shell.indexing.root_after_pending_finish.clone() {
            if crate::path_utils::path_key(&root)
                != crate::path_utils::path_key(&self.shell.runtime.root)
            {
                let follow_up = self
                    .shell
                    .indexing
                    .refresh_after_pending_finish
                    .unwrap_or(super::PendingIndexRefreshMode::Normal);
                if !self.try_retire_active_root_resources(root) {
                    return false;
                }
                self.shell
                    .indexing
                    .cleanup_request(pending_finish.request_id);
                self.cancel_stale_pending_filelist_confirmations_for_active_root();
                self.cancel_stale_pending_after_index_for_active_root();
                self.mark_ui_state_dirty();
                match follow_up {
                    super::PendingIndexRefreshMode::Normal => self.request_index_refresh(),
                    super::PendingIndexRefreshMode::CreateFileListWalker => {
                        self.request_create_filelist_walker_refresh()
                    }
                }
                self.set_notice(format!("Root changed: {}", self.root_display_text()));
                return true;
            }
        }
        let previous = self.take_active_committed_resources();
        if !previous.is_empty() {
            if let Err(previous) = self.shell.tabs.try_retire_active_resources(previous) {
                self.restore_active_committed_resources(previous);
                self.set_notice("Waiting for background tab resource reclamation");
                return false;
            }
        }
        let refresh_after_finish = self.shell.indexing.refresh_after_pending_finish.take();
        let root_after_finish = self.shell.indexing.root_after_pending_finish.take();
        self.shell.indexing.pending_finish = None;
        let finalization = self
            .shell
            .indexing
            .background_finalizations
            .remove(&pending_finish.request_id);
        self.finish_active_index_request(pending_finish, finalization);
        if let Some(root) = root_after_finish {
            if crate::path_utils::path_key(&root)
                != crate::path_utils::path_key(&self.shell.runtime.root)
            {
                self.apply_root_change_direct(root);
                return true;
            }
        }
        match refresh_after_finish {
            Some(super::PendingIndexRefreshMode::Normal) => self.request_index_refresh(),
            Some(super::PendingIndexRefreshMode::CreateFileListWalker) => {
                self.request_create_filelist_walker_refresh()
            }
            None => {}
        }
        true
    }

    fn finish_active_index_request(
        &mut self,
        pending_finish: PendingActiveIndexFinish,
        mut finalization: Option<super::PendingBackgroundIndexFinalize>,
    ) {
        let request_id = pending_finish.request_id;
        let finalized_filter_snapshot = finalization
            .as_ref()
            .is_some_and(|finalization| finalization.filtered_entries.is_some());
        let mut finalized_filelist_paths = None;
        if let Some(finalization) = finalization.as_mut() {
            debug_assert_eq!(finalization.request_id, request_id);
            debug_assert_eq!(Some(finalization.tab_id), self.current_tab_id());
            self.shell.indexing.build.index.entries =
                std::mem::take(&mut finalization.completed_entries);
            self.shell.indexing.build.incremental_filtered_entries =
                finalization.filtered_entries.take().unwrap_or_default();
            self.shell.indexing.build.pending_kind_paths =
                std::mem::take(&mut finalization.unresolved_kind_paths);
            self.shell.indexing.build.pending_kind_paths_set =
                std::mem::take(&mut finalization.unresolved_kind_paths_set);
            finalized_filelist_paths = finalization.filelist_paths.take();
        }
        self.shell.indexing.build.index.source = pending_finish.source;
        self.shell.runtime.all_entries =
            Arc::new(std::mem::take(&mut self.shell.indexing.build.index.entries));
        self.shell
            .indexing
            .apply_resource_transition(TabResourceTransition::Success);

        let needs_filtering = !self.shell.runtime.include_files
            || !self.shell.runtime.include_dirs
            || (self.shell.ui.ignore_list_enabled
                && !self.shell.runtime.ignore_list_terms.is_empty());
        let has_incremental_filter_snapshot = needs_filtering
            && (finalized_filter_snapshot
                || !self
                    .shell
                    .indexing
                    .build
                    .incremental_filtered_entries
                    .is_empty()
                || !self.shell.indexing.build.index.entries.is_empty());
        self.shell.indexing.settle_active_terminal_state();
        if needs_filtering {
            if has_incremental_filter_snapshot {
                self.shell.runtime.entries = Arc::new(std::mem::take(
                    &mut self.shell.indexing.build.incremental_filtered_entries,
                ));
                self.shell.indexing.last_search_snapshot_len = self.shell.runtime.entries.len();
                self.shell.indexing.search_rerun_pending = false;
                if self.shell.runtime.query_state.query.trim().is_empty() {
                    self.shell.search.clear_active_request_state();
                    // The incremental snapshot replaces the previous result set at
                    // index completion, so its denominator must replace any count
                    // left by the search that was active before the refresh.
                    self.shell.runtime.total_match_count = self.shell.runtime.entries.len();
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
                } else {
                    self.update_results();
                }
            } else {
                self.apply_entry_filters(true);
            }
        } else {
            self.shell.runtime.entries = Arc::clone(&self.shell.runtime.all_entries);
            self.shell
                .indexing
                .build
                .incremental_filtered_entries
                .clear();
            self.shell.indexing.last_search_snapshot_len = self.shell.runtime.entries.len();
            self.shell.indexing.search_rerun_pending = false;
            if self.shell.runtime.query_state.query.trim().is_empty() {
                self.shell.search.clear_active_request_state();
                // The index refresh supersedes the previous result snapshot;
                // keep the Results denominator aligned with the final index.
                self.shell.runtime.total_match_count = self.shell.runtime.entries.len();
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
            } else {
                self.update_results();
            }
        }

        if self.shell.runtime.query_state.query.trim().is_empty() {
            result_reducer::clear_unrestored_evicted_selection(self);
        }

        if self.shell.runtime.query_state.query.trim().is_empty()
            && self.shell.runtime.result_sort_mode != ResultSortMode::Score
            && !self.shell.search.in_progress()
            && !self.shell.worker_bus.sort.in_progress
        {
            // A closed-tab restore may restart interrupted index and result
            // work together. The final index snapshot supersedes the retained
            // snapshot, so reapply the preserved empty-query sort after the
            // replacement index settles.
            self.apply_result_sort(false);
        }

        if matches!(self.shell.indexing.build.index.source, IndexSource::Walker) {
            // Regression guard: keep Walker kind resolution bounded to what is
            // actually visible. Resolving the entire tree eagerly can keep the
            // process hot on huge on-demand roots even after search/index settles.
            self.queue_unknown_kind_paths_for_visible_results();
        } else {
            self.reset_kind_resolution_state();
        }
        let current_tab_id = self.current_tab_id().unwrap_or_default();
        let current_root = self.shell.runtime.root.clone();
        if let Some(notice) =
            self.take_filelist_index_completion_notice(request_id, current_tab_id, &current_root)
        {
            self.set_notice(notice);
        } else {
            self.clear_notice();
        }
        if self
            .shell
            .features
            .filelist
            .workflow
            .pending_after_index
            .as_ref()
            .is_some_and(|pending| {
                pending.tab_id == current_tab_id
                    && path_key(&pending.root) == path_key(&self.shell.runtime.root)
            })
        {
            let root = self.shell.runtime.root.clone();
            let entries = finalized_filelist_paths
                .take()
                .unwrap_or_else(|| self.filelist_entries_snapshot());
            self.shell.features.filelist.workflow.pending_after_index = None;
            self.request_filelist_creation(current_tab_id, root, entries);
        }
        self.shell.indexing.complete_active_request(request_id);
    }

    pub(super) fn take_filelist_index_completion_notice(
        &mut self,
        request_id: u64,
        tab_id: u64,
        root: &Path,
    ) -> Option<String> {
        self.shell
            .features
            .filelist
            .workflow
            .pending_index_completion_notices
            .remove(&request_id)
            .filter(|pending| pending.tab_id == tab_id && path_key(&pending.root) == path_key(root))
            .map(|pending| pending.notice)
    }

    pub(super) fn discard_filelist_index_completion_notice(&mut self, request_id: u64) {
        self.shell
            .features
            .filelist
            .workflow
            .pending_index_completion_notices
            .remove(&request_id);
    }

    pub(super) fn apply_incremental_empty_query_results(&mut self) {
        self.pipeline_owner()
            .apply_incremental_empty_query_results();
    }

    pub(super) fn maybe_refresh_incremental_search(&mut self) {
        self.pipeline_owner().maybe_refresh_incremental_search();
    }

    pub(super) fn should_refresh_incremental_search(&self) -> bool {
        let current_len = self.shell.indexing.build.incremental_filtered_entries.len();
        let delta = current_len.saturating_sub(self.shell.indexing.last_search_snapshot_len);
        if delta == 0 {
            return false;
        }
        if self.shell.indexing.in_progress {
            if delta < Self::INCREMENTAL_SEARCH_MIN_DELTA_DURING_INDEX {
                return false;
            }
            return self
                .shell
                .indexing
                .last_incremental_results_refresh
                .elapsed()
                >= Self::INCREMENTAL_SEARCH_REFRESH_INTERVAL_DURING_INDEX;
        }
        self.shell
            .indexing
            .last_incremental_results_refresh
            .elapsed()
            >= Self::INCREMENTAL_SEARCH_REFRESH_INTERVAL
    }

    pub(super) fn apply_entry_filters(&mut self, keep_scroll_position: bool) {
        self.pipeline_owner()
            .apply_entry_filters(keep_scroll_position);
    }
}

use super::{
    AppTabState, BackgroundIndexState, FlistWalkerApp, IndexEntry, IndexRequest, IndexResponse,
    IndexSource, KindResolveRequest, TabSessionState,
};
use crate::entry::{Entry, EntryKind};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub(super) enum IndexResponseRoute {
    Active,
    Background(u64),
    Stale,
}

pub(super) struct IndexCoordinator {
    pub(super) tx: Sender<IndexRequest>,
    pub(super) rx: Receiver<IndexResponse>,
    pub(super) next_request_id: u64,
    pub(super) pending_request_id: Option<u64>,
    pub(super) latest_request_ids: Arc<Mutex<HashMap<u64, u64>>>,
    pub(super) pending_queue: VecDeque<IndexRequest>,
    pub(super) inflight_requests: HashSet<u64>,
    pub(super) in_progress: bool,
    pub(super) incremental_filtered_entries: Vec<Entry>,
    pub(super) pending_entries: VecDeque<IndexEntry>,
    pub(super) pending_entries_request_id: Option<u64>,
    pub(super) pending_kind_paths: VecDeque<PathBuf>,
    pub(super) pending_kind_paths_set: HashSet<PathBuf>,
    pub(super) in_flight_kind_paths: HashSet<PathBuf>,
    pub(super) kind_resolution_epoch: u64,
    pub(super) kind_resolution_in_progress: bool,
    pub(super) last_incremental_results_refresh: Instant,
    pub(super) last_search_snapshot_len: usize,
    pub(super) search_resume_pending: bool,
    pub(super) search_rerun_pending: bool,
    pub(super) request_tabs: HashMap<u64, u64>,
    pub(super) background_states: HashMap<u64, BackgroundIndexState>,
}

impl IndexCoordinator {
    pub(super) fn new(
        tx: Sender<IndexRequest>,
        rx: Receiver<IndexResponse>,
        latest_request_ids: Arc<Mutex<HashMap<u64, u64>>>,
    ) -> Self {
        Self {
            tx,
            rx,
            next_request_id: 1,
            pending_request_id: None,
            latest_request_ids,
            pending_queue: VecDeque::new(),
            inflight_requests: HashSet::new(),
            in_progress: false,
            incremental_filtered_entries: Vec::new(),
            pending_entries: VecDeque::new(),
            pending_entries_request_id: None,
            pending_kind_paths: VecDeque::new(),
            pending_kind_paths_set: HashSet::new(),
            in_flight_kind_paths: HashSet::new(),
            kind_resolution_epoch: 1,
            kind_resolution_in_progress: false,
            last_incremental_results_refresh: Instant::now(),
            last_search_snapshot_len: 0,
            search_resume_pending: false,
            search_rerun_pending: false,
            request_tabs: HashMap::new(),
            background_states: HashMap::new(),
        }
    }

    pub(super) fn clear_for_tab(&mut self, tab_id: u64) {
        self.request_tabs.retain(|_, id| *id != tab_id);
        self.pending_queue.retain(|req| req.tab_id != tab_id);
        if let Ok(mut latest) = self.latest_request_ids.lock() {
            latest.remove(&tab_id);
        }
        self.background_states
            .retain(|request_id, _| self.request_tabs.contains_key(request_id));
    }

    pub(super) fn allocate_request_id(&mut self, tab_id: Option<u64>) -> u64 {
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1);
        if let Some(tab_id) = tab_id {
            self.request_tabs.insert(request_id, tab_id);
            if let Ok(mut latest) = self.latest_request_ids.lock() {
                latest.insert(tab_id, request_id);
            }
        }
        request_id
    }

    pub(super) fn begin_active_refresh(&mut self, request_id: u64, query_non_empty: bool) {
        self.pending_request_id = Some(request_id);
        self.in_progress = true;
        self.search_resume_pending = query_non_empty;
        self.search_rerun_pending = false;
    }

    pub(super) fn begin_active_refresh_with_inflight(
        &mut self,
        request_id: u64,
        query_non_empty: bool,
    ) {
        self.begin_active_refresh(request_id, query_non_empty);
        self.inflight_requests.insert(request_id);
    }

    pub(super) fn begin_background_refresh(
        &mut self,
        tab: &mut AppTabState,
        request_id: u64,
        notice: &str,
    ) {
        tab.index_state.begin_index_request(request_id);
        tab.pending_request_id = None;
        tab.search_in_progress = false;
        tab.index_state.search_resume_pending = !tab.query_state.query.trim().is_empty();
        tab.index_state.search_rerun_pending = false;
        tab.index_state.index.entries.clear();
        tab.index_state.index.source = IndexSource::None;
        tab.index_state.pending_index_entries.clear();
        tab.index_state.pending_index_entries_request_id = None;
        tab.index_state.pending_kind_paths.clear();
        tab.index_state.pending_kind_paths_set.clear();
        tab.index_state.in_flight_kind_paths.clear();
        tab.index_state.kind_resolution_in_progress = false;
        tab.index_state.kind_resolution_epoch =
            tab.index_state.kind_resolution_epoch.saturating_add(1);
        tab.pending_preview_request_id = None;
        tab.preview_in_progress = false;
        tab.index_state.last_incremental_results_refresh = Instant::now();
        tab.index_state.last_search_snapshot_len = 0;
        tab.notice = notice.to_string();
    }

    pub(super) fn cleanup_request(&mut self, request_id: u64) {
        self.request_tabs.remove(&request_id);
        self.background_states.remove(&request_id);
        self.inflight_requests.remove(&request_id);
    }

    pub(super) fn settle_active_terminal_state(&mut self) {
        self.in_progress = false;
        self.pending_request_id = None;
        self.search_resume_pending = false;
        self.search_rerun_pending = false;
        self.pending_entries.clear();
        self.pending_entries_request_id = None;
    }

    pub(super) fn clear_active_request_state(&mut self, tabs: &mut TabSessionState) {
        self.settle_active_terminal_state();
        tabs.clear_pending_restore_refresh_tabs();
    }

    pub(super) fn route_response(&mut self, request_id: u64) -> IndexResponseRoute {
        if Some(request_id) == self.pending_request_id {
            return IndexResponseRoute::Active;
        }
        match self.request_tabs.get(&request_id).copied() {
            Some(tab_id) => IndexResponseRoute::Background(tab_id),
            None => IndexResponseRoute::Stale,
        }
    }

    pub(super) fn response_request_id(response: &IndexResponse) -> u64 {
        match response {
            IndexResponse::Started { request_id, .. }
            | IndexResponse::Batch { request_id, .. }
            | IndexResponse::ReplaceAll { request_id, .. }
            | IndexResponse::Finished { request_id, .. }
            | IndexResponse::Failed { request_id, .. }
            | IndexResponse::Canceled { request_id }
            | IndexResponse::Truncated { request_id, .. } => *request_id,
        }
    }

    pub(super) fn complete_active_request(&mut self, request_id: u64) {
        self.settle_active_terminal_state();
        self.cleanup_request(request_id);
    }

    pub(super) fn cleanup_stale_terminal_response(&mut self, request_id: u64) {
        self.cleanup_request(request_id);
    }
}

impl FlistWalkerApp {
    /// kind 未確定 entry の遅延解決が必要な filter 状態かを返す。
    pub(super) fn kind_resolution_needed_for_filters(&self) -> bool {
        !self.shell.runtime.include_files || !self.shell.runtime.include_dirs
    }

    /// kind 解決キューと epoch を初期化し直す。
    pub(super) fn reset_kind_resolution_state(&mut self) {
        self.shell.indexing.pending_kind_paths.clear();
        self.shell.indexing.pending_kind_paths_set.clear();
        self.shell.indexing.in_flight_kind_paths.clear();
        self.shell.indexing.kind_resolution_in_progress = false;
        self.shell.indexing.kind_resolution_epoch =
            self.shell.indexing.kind_resolution_epoch.saturating_add(1);
    }

    /// 表示中または incremental index 中の entry から kind 未解決 path を拾う。
    pub(super) fn queue_unknown_kind_paths_for_active_entries(&mut self) {
        if !self.kind_resolution_needed_for_filters() {
            return;
        }
        let source: Vec<PathBuf> =
            if self.shell.indexing.in_progress && !self.shell.runtime.index.entries.is_empty() {
                self.shell
                    .runtime
                    .index
                    .entries
                    .iter()
                    .map(|entry| entry.path.clone())
                    .collect()
            } else {
                self.shell
                    .runtime
                    .all_entries
                    .iter()
                    .map(|entry| entry.path.clone())
                    .collect()
            };
        self.queue_unknown_kind_paths(&source);
    }

    /// walker 完了後の全 entry から kind 未解決 path を拾う。
    pub(super) fn queue_unknown_kind_paths_for_completed_walker_entries(&mut self) {
        for i in 0..self.shell.runtime.all_entries.len() {
            let path = &self.shell.runtime.all_entries[i].path;
            if self.find_entry_kind(path).is_none()
                && !self.shell.indexing.pending_kind_paths_set.contains(path)
                && !self.shell.indexing.in_flight_kind_paths.contains(path)
            {
                let p = path.clone();
                self.shell.indexing.pending_kind_paths_set.insert(p.clone());
                self.shell.indexing.pending_kind_paths.push_back(p);
            }
        }
    }

    /// 指定 path 群から kind 未解決のものだけを queue へ積む。
    pub(super) fn queue_unknown_kind_paths(&mut self, source: &[PathBuf]) {
        for path in source {
            if self.find_entry_kind(path).is_none() {
                self.queue_kind_resolution(path.clone());
            }
        }
    }

    /// kind 解決キューへ重複なしで path を追加する。
    pub(super) fn queue_kind_resolution(&mut self, path: PathBuf) {
        if self.shell.indexing.pending_kind_paths_set.contains(&path)
            || self.shell.indexing.in_flight_kind_paths.contains(&path)
        {
            return;
        }
        self.shell
            .indexing
            .pending_kind_paths_set
            .insert(path.clone());
        self.shell.indexing.pending_kind_paths.push_back(path);
    }

    /// kind resolver worker へ frame 予算内で request を流す。
    pub(super) fn pump_kind_resolution_requests(&mut self) {
        const MAX_DISPATCH_PER_FRAME: usize = 128;
        let mut dispatched = 0usize;
        while dispatched < MAX_DISPATCH_PER_FRAME {
            let Some(path) = self.shell.indexing.pending_kind_paths.pop_front() else {
                break;
            };
            self.shell.indexing.pending_kind_paths_set.remove(&path);
            let req = KindResolveRequest {
                epoch: self.shell.indexing.kind_resolution_epoch,
                path: path.clone(),
            };
            if self.shell.worker_bus.kind.tx.send(req).is_err() {
                break;
            }
            self.shell.indexing.in_flight_kind_paths.insert(path);
            dispatched = dispatched.saturating_add(1);
        }
        self.shell.indexing.kind_resolution_in_progress =
            !self.shell.indexing.pending_kind_paths.is_empty()
                || !self.shell.indexing.in_flight_kind_paths.is_empty();
    }

    /// kind resolver 応答を吸収し filter/preview を必要最小限で更新する。
    pub(super) fn poll_kind_response(&mut self) {
        const MAX_MESSAGES_PER_FRAME: usize = 512;
        let mut processed = 0usize;
        let mut resolved_any = false;
        let mut resolved_current_row = false;
        let mut resolved_updates: Vec<(PathBuf, EntryKind)> = Vec::new();

        while let Ok(response) = self.shell.worker_bus.kind.rx.try_recv() {
            if response.epoch != self.shell.indexing.kind_resolution_epoch {
                continue;
            }
            self.shell
                .indexing
                .in_flight_kind_paths
                .remove(&response.path);
            if let Some(kind) = response.kind {
                if self.shell.runtime.current_row.is_some_and(|row| {
                    self.shell
                        .runtime
                        .results
                        .get(row)
                        .is_some_and(|(path, _)| *path == response.path)
                }) {
                    resolved_current_row = true;
                }
                resolved_updates.push((response.path.clone(), kind));
                resolved_any = true;
            }
            processed = processed.saturating_add(1);
            if processed >= MAX_MESSAGES_PER_FRAME {
                break;
            }
        }

        if !resolved_updates.is_empty() {
            self.apply_entry_kind_updates(&resolved_updates);
        }

        self.shell.indexing.kind_resolution_in_progress =
            !self.shell.indexing.pending_kind_paths.is_empty()
                || !self.shell.indexing.in_flight_kind_paths.is_empty();

        if resolved_any && (!self.shell.runtime.include_files || !self.shell.runtime.include_dirs) {
            self.apply_entry_filters(true);
        }
        if resolved_current_row && self.shell.ui.show_preview {
            self.request_preview_for_current();
        }
    }
}

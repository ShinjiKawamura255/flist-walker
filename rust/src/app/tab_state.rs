use super::{
    normalize_windows_path_buf, EntryKindCacheState, FlistWalkerApp, PendingActiveIndexFinish,
    PendingIndexRefreshMode, ResultSortMode, ResultSortScope, SavedTabState, TabAccentColor,
};
use crate::app::worker::protocol::IndexEntry;
use crate::entry::{Entry, EntryKind};
use crate::indexer::{IndexBuildResult, IndexSource};
use std::collections::{HashSet, VecDeque};
use std::mem;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum TabResourceLifecycle {
    #[default]
    Dormant,
    Loading,
    Ready,
    Refreshing,
    Failed,
    Evicted,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct TabResourceState {
    lifecycle: TabResourceLifecycle,
    committed_snapshot_present: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TabResourceTransition {
    Begin,
    Success,
    Failure,
    Cancel,
    Evict,
    ReclaimFullRollback(TabResourceState),
    SnapshotRemoved,
    SnapshotRestored,
    Dormant,
    Reset,
}

impl TabResourceState {
    pub(super) const fn new(
        lifecycle: TabResourceLifecycle,
        committed_snapshot_present: bool,
    ) -> Self {
        Self {
            lifecycle,
            committed_snapshot_present,
        }
    }

    const fn reduced(self, transition: TabResourceTransition) -> Self {
        match transition {
            TabResourceTransition::Begin => Self::new(
                if self.committed_snapshot_present {
                    TabResourceLifecycle::Refreshing
                } else {
                    TabResourceLifecycle::Loading
                },
                self.committed_snapshot_present,
            ),
            TabResourceTransition::Success => Self::new(TabResourceLifecycle::Ready, true),
            TabResourceTransition::Failure => Self::new(
                TabResourceLifecycle::Failed,
                self.committed_snapshot_present,
            ),
            TabResourceTransition::Cancel => Self::new(
                if self.committed_snapshot_present {
                    TabResourceLifecycle::Ready
                } else {
                    TabResourceLifecycle::Dormant
                },
                self.committed_snapshot_present,
            ),
            TabResourceTransition::Evict => Self::new(TabResourceLifecycle::Evicted, false),
            TabResourceTransition::ReclaimFullRollback(previous) => previous,
            TabResourceTransition::SnapshotRemoved => Self::new(self.lifecycle, false),
            TabResourceTransition::SnapshotRestored => Self::new(self.lifecycle, true),
            TabResourceTransition::Dormant => Self::new(
                TabResourceLifecycle::Dormant,
                self.committed_snapshot_present,
            ),
            TabResourceTransition::Reset => Self::new(TabResourceLifecycle::Dormant, false),
        }
    }

    pub(super) fn apply(&mut self, transition: TabResourceTransition) {
        *self = self.reduced(transition);
    }

    pub(super) const fn lifecycle(self) -> TabResourceLifecycle {
        self.lifecycle
    }

    pub(super) const fn committed_snapshot_present(self) -> bool {
        self.committed_snapshot_present
    }
}

#[derive(Clone, Debug)]
pub(super) struct TabQueryState {
    pub(super) query: String,
    pub(super) query_history: VecDeque<String>,
    pub(super) query_history_cursor: Option<usize>,
    pub(super) query_history_draft: Option<String>,
    pub(super) history_search_active: bool,
    pub(super) history_search_query: String,
    pub(super) history_search_original_query: String,
    pub(super) history_search_results: Vec<String>,
    pub(super) history_search_current: Option<usize>,
}

#[derive(Debug)]
#[cfg_attr(test, derive(Clone))]
pub(super) struct TabBuildPayload {
    pub(super) index: IndexBuildResult,
    pub(super) pending_entries: VecDeque<IndexEntry>,
    pub(super) pending_kind_paths: VecDeque<PathBuf>,
    pub(super) pending_kind_paths_set: HashSet<PathBuf>,
    pub(super) in_flight_kind_paths: HashSet<PathBuf>,
    pub(super) resolved_kind_updates: Vec<(PathBuf, EntryKind)>,
    pub(super) incremental_filtered_entries: Vec<Entry>,
    pub(super) entry_kind_cache: EntryKindCacheState,
}

impl Default for TabBuildPayload {
    fn default() -> Self {
        Self {
            index: IndexBuildResult {
                entries: Vec::new(),
                source: IndexSource::None,
            },
            pending_entries: VecDeque::new(),
            pending_kind_paths: VecDeque::new(),
            pending_kind_paths_set: HashSet::new(),
            in_flight_kind_paths: HashSet::new(),
            resolved_kind_updates: Vec::new(),
            incremental_filtered_entries: Vec::new(),
            entry_kind_cache: EntryKindCacheState::default(),
        }
    }
}

impl TabBuildPayload {
    pub(super) fn take_reclaimable(&mut self) -> Self {
        let mut payload = mem::take(self);
        mem::swap(&mut self.index.source, &mut payload.index.source);
        payload
    }

    pub(super) fn restore_reclaimable(&mut self, mut payload: Self) {
        mem::swap(&mut self.index.source, &mut payload.index.source);
        *self = payload;
    }

    pub(super) fn is_empty(&self) -> bool {
        self.index.entries.capacity() == 0
            && self.pending_entries.capacity() == 0
            && self.pending_kind_paths.capacity() == 0
            && self.pending_kind_paths_set.capacity() == 0
            && self.in_flight_kind_paths.capacity() == 0
            && self.resolved_kind_updates.capacity() == 0
            && self.incremental_filtered_entries.capacity() == 0
            && self.entry_kind_cache.entries.capacity() == 0
    }

    pub(super) fn heavy_resource_weight(&self) -> usize {
        self.index
            .entries
            .capacity()
            .saturating_add(self.pending_entries.capacity())
            .saturating_add(self.pending_kind_paths.capacity())
            .saturating_add(self.pending_kind_paths_set.capacity())
            .saturating_add(self.in_flight_kind_paths.capacity())
            .saturating_add(self.resolved_kind_updates.capacity())
            .saturating_add(self.incremental_filtered_entries.capacity())
            .saturating_add(self.entry_kind_cache.entries.capacity())
    }
}

#[derive(Debug)]
#[cfg_attr(test, derive(Clone))]
pub(super) struct TabCommittedPayload {
    pub(super) all_entries: Arc<Vec<Entry>>,
    pub(super) entries: Arc<Vec<Entry>>,
    pub(super) base_results: Vec<(PathBuf, f64)>,
    // Travels with the snapshot: AllMatches can replace both ranking and membership.
    pub(super) base_results_are_score_ranked: bool,
    pub(super) results: Vec<(PathBuf, f64)>,
    pub(super) preview: String,
    pub(super) total_match_count: usize,
    pub(super) current_row: Option<usize>,
}

impl Default for TabCommittedPayload {
    fn default() -> Self {
        Self {
            all_entries: Arc::new(Vec::new()),
            entries: Arc::new(Vec::new()),
            base_results: Vec::new(),
            base_results_are_score_ranked: true,
            results: Vec::new(),
            preview: String::new(),
            total_match_count: 0,
            current_row: None,
        }
    }
}

impl TabCommittedPayload {
    pub(super) fn is_empty(&self) -> bool {
        self.all_entries.capacity() == 0
            && self.entries.capacity() == 0
            && self.base_results.capacity() == 0
            && self.results.capacity() == 0
            && self.preview.capacity() == 0
    }

    pub(super) fn heavy_resource_weight(&self) -> usize {
        let committed_entries = if Arc::ptr_eq(&self.all_entries, &self.entries) {
            self.all_entries.capacity()
        } else {
            self.all_entries
                .capacity()
                .saturating_add(self.entries.capacity())
        };
        committed_entries
            .saturating_add(self.base_results.capacity())
            .saturating_add(self.results.capacity())
            .saturating_add(self.preview.capacity())
    }
}

#[derive(Debug)]
#[cfg_attr(test, derive(Clone))]
pub(super) struct TabIndexState {
    resource_state: TabResourceState,
    pub(super) build: TabBuildPayload,
    pub(super) pending_index_request_id: Option<u64>,
    pub(super) index_in_progress: bool,
    pub(super) pending_index_entries_request_id: Option<u64>,
    pub(super) pending_index_finish: Option<PendingActiveIndexFinish>,
    pub(super) build_reclaim_pending: bool,
    pub(super) build_reclaim_request_id: Option<u64>,
    pub(super) refresh_after_pending_finish: Option<PendingIndexRefreshMode>,
    pub(super) root_after_pending_finish: Option<PathBuf>,
    pub(super) kind_resolution_epoch: u64,
    pub(super) kind_resolution_in_progress: bool,
    pub(super) last_incremental_results_refresh: Instant,
    pub(super) last_search_snapshot_len: usize,
    pub(super) search_resume_pending: bool,
    pub(super) search_rerun_pending: bool,
}

#[derive(Debug)]
#[cfg_attr(test, derive(Clone))]
pub(super) struct TabResultState {
    pub(super) committed: TabCommittedPayload,
    pub(super) result_sort_mode: ResultSortMode,
    pub(super) result_sort_scope: ResultSortScope,
    pub(super) pending_sort_request_id: Option<u64>,
    pub(super) sort_in_progress: bool,
    pub(super) pinned_paths: HashSet<PathBuf>,
    pub(super) evicted_selected_path: Option<PathBuf>,
    pub(super) results_compacted: bool,
}

#[derive(Debug)]
#[cfg_attr(test, derive(Clone))]
pub(crate) struct AppTabState {
    pub(super) id: u64,
    pub(super) root: PathBuf,
    pub(super) tab_accent: Option<TabAccentColor>,
    pub(super) use_filelist: bool,
    pub(super) use_regex: bool,
    pub(super) ignore_case: bool,
    pub(super) include_files: bool,
    pub(super) include_dirs: bool,
    pub(super) max_depth: crate::indexer::MaxDepth,
    pub(super) index_state: TabIndexState,
    pub(super) query_state: TabQueryState,
    pub(super) result_state: TabResultState,
    pub(super) notice: String,
    pub(super) pending_request_id: Option<u64>,
    pub(super) pending_preview_request_id: Option<u64>,
    pub(super) pending_action_request_id: Option<u64>,
    pub(super) search_in_progress: bool,
    pub(super) preview_in_progress: bool,
    pub(super) preview_reload_pending: bool,
    pub(super) action_in_progress: bool,
}

impl TabIndexState {
    pub(super) const fn lifecycle(&self) -> TabResourceLifecycle {
        self.resource_state.lifecycle()
    }

    pub(super) const fn committed_snapshot_present(&self) -> bool {
        self.resource_state.committed_snapshot_present()
    }

    pub(super) const fn resource_state(&self) -> TabResourceState {
        self.resource_state
    }

    pub(super) fn apply_resource_transition(&mut self, transition: TabResourceTransition) {
        self.resource_state.apply(transition);
    }

    #[cfg(test)]
    pub(super) fn set_resource_state_for_test(&mut self, state: TabResourceState) {
        self.resource_state = state;
    }

    #[cfg(test)]
    pub(super) fn set_lifecycle_for_test(&mut self, lifecycle: TabResourceLifecycle) {
        self.resource_state.lifecycle = lifecycle;
    }

    #[cfg(test)]
    pub(super) fn set_committed_snapshot_present_for_test(&mut self, present: bool) {
        self.resource_state.committed_snapshot_present = present;
    }

    pub(super) fn begin_index_request(&mut self, request_id: u64) {
        self.apply_resource_transition(TabResourceTransition::Begin);
        self.pending_index_request_id = Some(request_id);
        self.index_in_progress = true;
    }

    pub(super) fn clear_index_request_state(&mut self) {
        self.pending_index_request_id = None;
        self.index_in_progress = false;
        self.pending_index_entries_request_id = None;
        self.pending_index_finish = None;
        self.search_resume_pending = false;
        self.search_rerun_pending = false;
    }

    pub(super) fn clear_kind_resolution_state(&mut self) {
        self.build.pending_kind_paths.clear();
        self.build.pending_kind_paths_set.clear();
        self.build.in_flight_kind_paths.clear();
        self.build.resolved_kind_updates.clear();
        self.kind_resolution_in_progress = false;
    }

    pub(super) fn refresh_kind_resolution_progress(&mut self) {
        self.kind_resolution_in_progress = !self.build.pending_kind_paths.is_empty()
            || !self.build.in_flight_kind_paths.is_empty();
    }

    #[cfg(test)]
    pub(super) fn from_shell(shell: &FlistWalkerApp) -> Self {
        Self {
            resource_state: shell.shell.indexing.resource_state(),
            build: shell.shell.indexing.build.clone(),
            pending_index_request_id: shell.shell.indexing.pending_request_id,
            index_in_progress: shell.shell.indexing.in_progress,
            pending_index_entries_request_id: shell.shell.indexing.pending_entries_request_id,
            pending_index_finish: shell.shell.indexing.pending_finish.clone(),
            build_reclaim_pending: shell.shell.indexing.build_reclaim_pending,
            build_reclaim_request_id: shell.shell.indexing.build_reclaim_request_id,
            refresh_after_pending_finish: shell.shell.indexing.refresh_after_pending_finish,
            root_after_pending_finish: shell.shell.indexing.root_after_pending_finish.clone(),
            kind_resolution_epoch: shell.shell.indexing.kind_resolution_epoch,
            kind_resolution_in_progress: shell.shell.indexing.kind_resolution_in_progress,
            last_incremental_results_refresh: shell.shell.indexing.last_incremental_results_refresh,
            last_search_snapshot_len: shell.shell.indexing.last_search_snapshot_len,
            search_resume_pending: shell.shell.indexing.search_resume_pending,
            search_rerun_pending: shell.shell.indexing.search_rerun_pending,
        }
    }

    #[cfg(test)]
    pub(super) fn apply_shell(&self, shell: &mut FlistWalkerApp) {
        shell.shell.indexing.build = self.build.clone();
        shell
            .shell
            .indexing
            .set_resource_state_for_test(self.resource_state);
        shell.shell.indexing.pending_request_id = self.pending_index_request_id;
        shell.shell.indexing.in_progress = self.index_in_progress;
        shell.shell.indexing.pending_entries_request_id = self.pending_index_entries_request_id;
        shell.shell.indexing.pending_finish = self.pending_index_finish.clone();
        shell.shell.indexing.build_reclaim_pending = self.build_reclaim_pending;
        shell.shell.indexing.build_reclaim_request_id = self.build_reclaim_request_id;
        shell.shell.indexing.refresh_after_pending_finish = self.refresh_after_pending_finish;
        shell.shell.indexing.root_after_pending_finish = self.root_after_pending_finish.clone();
        shell.shell.indexing.kind_resolution_epoch = self.kind_resolution_epoch;
        shell.shell.indexing.kind_resolution_in_progress = self.kind_resolution_in_progress;
        shell.shell.indexing.last_incremental_results_refresh =
            self.last_incremental_results_refresh;
        shell.shell.indexing.last_search_snapshot_len = self.last_search_snapshot_len;
        shell.shell.indexing.search_resume_pending = self.search_resume_pending;
        shell.shell.indexing.search_rerun_pending = self.search_rerun_pending;
    }

    pub(super) fn swap_shell(&mut self, shell: &mut FlistWalkerApp) {
        shell
            .shell
            .indexing
            .swap_resource_state(&mut self.resource_state);
        mem::swap(
            &mut self.pending_index_request_id,
            &mut shell.shell.indexing.pending_request_id,
        );
        mem::swap(
            &mut self.index_in_progress,
            &mut shell.shell.indexing.in_progress,
        );
        mem::swap(
            &mut self.pending_index_entries_request_id,
            &mut shell.shell.indexing.pending_entries_request_id,
        );
        mem::swap(
            &mut self.pending_index_finish,
            &mut shell.shell.indexing.pending_finish,
        );
        mem::swap(
            &mut self.build_reclaim_pending,
            &mut shell.shell.indexing.build_reclaim_pending,
        );
        mem::swap(
            &mut self.build_reclaim_request_id,
            &mut shell.shell.indexing.build_reclaim_request_id,
        );
        mem::swap(
            &mut self.refresh_after_pending_finish,
            &mut shell.shell.indexing.refresh_after_pending_finish,
        );
        mem::swap(
            &mut self.root_after_pending_finish,
            &mut shell.shell.indexing.root_after_pending_finish,
        );
        mem::swap(
            &mut self.kind_resolution_epoch,
            &mut shell.shell.indexing.kind_resolution_epoch,
        );
        mem::swap(
            &mut self.kind_resolution_in_progress,
            &mut shell.shell.indexing.kind_resolution_in_progress,
        );
        mem::swap(
            &mut self.last_incremental_results_refresh,
            &mut shell.shell.indexing.last_incremental_results_refresh,
        );
        mem::swap(
            &mut self.last_search_snapshot_len,
            &mut shell.shell.indexing.last_search_snapshot_len,
        );
        mem::swap(
            &mut self.search_resume_pending,
            &mut shell.shell.indexing.search_resume_pending,
        );
        mem::swap(
            &mut self.search_rerun_pending,
            &mut shell.shell.indexing.search_rerun_pending,
        );
    }
}

impl TabQueryState {
    #[cfg(test)]
    pub(super) fn from_shell(shell: &FlistWalkerApp) -> Self {
        Self {
            query: shell.shell.runtime.query_state.query.clone(),
            query_history: shell.shell.runtime.query_state.query_history.clone(),
            query_history_cursor: shell.shell.runtime.query_state.query_history_cursor,
            query_history_draft: shell.shell.runtime.query_state.query_history_draft.clone(),
            history_search_active: shell.shell.runtime.query_state.history_search_active,
            history_search_query: shell.shell.runtime.query_state.history_search_query.clone(),
            history_search_original_query: shell
                .shell
                .runtime
                .query_state
                .history_search_original_query
                .clone(),
            history_search_results: shell
                .shell
                .runtime
                .query_state
                .history_search_results
                .clone(),
            history_search_current: shell.shell.runtime.query_state.history_search_current,
        }
    }

    #[cfg(test)]
    pub(super) fn apply_shell(&self, shell: &mut FlistWalkerApp) {
        shell.shell.runtime.query_state.query = self.query.clone();
        shell.shell.runtime.query_state.query_history = self.query_history.clone();
        shell.shell.runtime.query_state.query_history_cursor = self.query_history_cursor;
        shell.shell.runtime.query_state.query_history_draft = self.query_history_draft.clone();
        shell.shell.runtime.query_state.history_search_active = self.history_search_active;
        shell.shell.runtime.query_state.history_search_query = self.history_search_query.clone();
        shell
            .shell
            .runtime
            .query_state
            .history_search_original_query = self.history_search_original_query.clone();
        shell.shell.runtime.query_state.history_search_results =
            self.history_search_results.clone();
        shell.shell.runtime.query_state.history_search_current = self.history_search_current;
    }

    pub(super) fn swap_shell(&mut self, shell: &mut FlistWalkerApp) {
        let query_state = &mut shell.shell.runtime.query_state;
        mem::swap(&mut self.query, &mut query_state.query);
        mem::swap(&mut self.query_history, &mut query_state.query_history);
        mem::swap(
            &mut self.query_history_cursor,
            &mut query_state.query_history_cursor,
        );
        mem::swap(
            &mut self.query_history_draft,
            &mut query_state.query_history_draft,
        );
        mem::swap(
            &mut self.history_search_active,
            &mut query_state.history_search_active,
        );
        mem::swap(
            &mut self.history_search_query,
            &mut query_state.history_search_query,
        );
        mem::swap(
            &mut self.history_search_original_query,
            &mut query_state.history_search_original_query,
        );
        mem::swap(
            &mut self.history_search_results,
            &mut query_state.history_search_results,
        );
        mem::swap(
            &mut self.history_search_current,
            &mut query_state.history_search_current,
        );
    }
}

impl TabResultState {
    pub(super) fn clear_sort_request_state(&mut self) {
        self.pending_sort_request_id = None;
        self.sort_in_progress = false;
    }

    #[cfg(test)]
    pub(super) fn from_shell(shell: &FlistWalkerApp) -> Self {
        Self {
            committed: shell.shell.runtime.committed.clone(),
            result_sort_mode: shell.shell.runtime.result_sort_mode,
            result_sort_scope: shell.shell.runtime.result_sort_scope,
            pending_sort_request_id: shell.shell.worker_bus.sort.pending_request_id,
            sort_in_progress: shell.shell.worker_bus.sort.in_progress,
            pinned_paths: shell.shell.runtime.pinned_paths.clone(),
            evicted_selected_path: shell.shell.runtime.evicted_selected_path.clone(),
            results_compacted: false,
        }
    }

    #[cfg(test)]
    pub(super) fn apply_shell(&self, shell: &mut FlistWalkerApp) {
        shell.shell.runtime.committed = self.committed.clone();
        shell.shell.runtime.result_sort_mode = self.result_sort_mode;
        shell.shell.runtime.result_sort_scope = self.result_sort_scope;
        shell.shell.worker_bus.sort.pending_request_id = self.pending_sort_request_id;
        shell.shell.worker_bus.sort.in_progress = self.sort_in_progress;
        shell.shell.runtime.pinned_paths = self.pinned_paths.clone();
        shell.shell.runtime.evicted_selected_path = self.evicted_selected_path.clone();
    }

    pub(super) fn swap_shell(&mut self, shell: &mut FlistWalkerApp) {
        mem::swap(
            &mut self.result_sort_mode,
            &mut shell.shell.runtime.result_sort_mode,
        );
        mem::swap(
            &mut self.result_sort_scope,
            &mut shell.shell.runtime.result_sort_scope,
        );
        mem::swap(
            &mut self.pending_sort_request_id,
            &mut shell.shell.worker_bus.sort.pending_request_id,
        );
        mem::swap(
            &mut self.sort_in_progress,
            &mut shell.shell.worker_bus.sort.in_progress,
        );
        mem::swap(
            &mut self.pinned_paths,
            &mut shell.shell.runtime.pinned_paths,
        );
        mem::swap(
            &mut self.evicted_selected_path,
            &mut shell.shell.runtime.evicted_selected_path,
        );
    }
}

impl AppTabState {
    pub(super) fn begin_search_request(&mut self, request_id: u64) {
        self.pending_request_id = Some(request_id);
        self.search_in_progress = true;
    }

    pub(super) fn clear_search_request_state(&mut self) {
        self.pending_request_id = None;
        self.search_in_progress = false;
    }

    pub(super) fn clear_preview_request_state(&mut self) {
        self.pending_preview_request_id = None;
        self.preview_in_progress = false;
    }

    pub(super) fn mark_preview_reload_pending(&mut self) {
        self.preview_reload_pending = true;
    }

    pub(super) fn take_preview_reload_pending(&mut self) -> bool {
        mem::take(&mut self.preview_reload_pending)
    }

    pub(super) fn clear_preview_reload_pending(&mut self) {
        self.preview_reload_pending = false;
    }

    pub(super) fn clear_action_request_state(&mut self) {
        self.pending_action_request_id = None;
        self.action_in_progress = false;
    }

    #[cfg(test)]
    pub(super) fn from_shell(shell: &FlistWalkerApp, id: u64) -> Self {
        Self {
            id,
            root: shell.shell.runtime.root.clone(),
            tab_accent: shell
                .shell
                .tabs
                .get(shell.shell.tabs.active_tab_index())
                .and_then(|tab| tab.tab_accent),
            use_filelist: shell.shell.runtime.use_filelist,
            use_regex: shell.shell.runtime.use_regex,
            ignore_case: shell.shell.runtime.ignore_case,
            include_files: shell.shell.runtime.include_files,
            include_dirs: shell.shell.runtime.include_dirs,
            max_depth: shell.shell.runtime.max_depth,
            index_state: TabIndexState::from_shell(shell),
            query_state: TabQueryState::from_shell(shell),
            result_state: TabResultState::from_shell(shell),
            notice: shell.shell.runtime.notice.clone(),
            pending_request_id: shell.shell.search.pending_request_id(),
            pending_preview_request_id: shell.shell.worker_bus.preview.pending_request_id,
            pending_action_request_id: shell.shell.worker_bus.action.pending_request_id,
            search_in_progress: shell.shell.search.in_progress(),
            preview_in_progress: shell.shell.worker_bus.preview.in_progress,
            preview_reload_pending: false,
            action_in_progress: shell.shell.worker_bus.action.in_progress,
        }
    }

    pub(super) fn from_saved(shell: &FlistWalkerApp, id: u64, saved: &SavedTabState) -> Self {
        Self {
            id,
            root: normalize_windows_path_buf(PathBuf::from(&saved.root)),
            tab_accent: saved.tab_accent,
            use_filelist: saved.use_filelist,
            use_regex: saved.use_regex,
            ignore_case: saved.ignore_case,
            include_files: saved.include_files,
            include_dirs: saved.include_dirs,
            max_depth: saved.max_depth,
            index_state: TabIndexState {
                resource_state: TabResourceState::default(),
                build: TabBuildPayload::default(),
                pending_index_request_id: None,
                index_in_progress: false,
                pending_index_entries_request_id: None,
                pending_index_finish: None,
                build_reclaim_pending: false,
                build_reclaim_request_id: None,
                refresh_after_pending_finish: None,
                root_after_pending_finish: None,
                kind_resolution_epoch: 1,
                kind_resolution_in_progress: false,
                last_incremental_results_refresh: Instant::now(),
                last_search_snapshot_len: 0,
                search_resume_pending: false,
                search_rerun_pending: false,
            },
            query_state: TabQueryState {
                query: saved.query.clone(),
                query_history: shell.shell.runtime.query_state.query_history.clone(),
                query_history_cursor: None,
                query_history_draft: None,
                history_search_active: false,
                history_search_query: String::new(),
                history_search_original_query: String::new(),
                history_search_results: Vec::new(),
                history_search_current: None,
            },
            result_state: TabResultState {
                committed: TabCommittedPayload::default(),
                result_sort_mode: ResultSortMode::Score,
                result_sort_scope: ResultSortScope::ShownResults,
                pending_sort_request_id: None,
                sort_in_progress: false,
                pinned_paths: HashSet::new(),
                evicted_selected_path: None,
                results_compacted: false,
            },
            notice: "Restored tab".to_string(),
            pending_request_id: None,
            pending_preview_request_id: None,
            pending_action_request_id: None,
            search_in_progress: false,
            preview_in_progress: false,
            preview_reload_pending: false,
            action_in_progress: false,
        }
    }

    pub(super) fn scratch_from_shell(shell: &FlistWalkerApp, id: u64) -> Self {
        let saved = Self::saved_from_shell(shell, false);
        Self::from_saved(shell, id, &saved)
    }

    pub(super) fn new_tab_from_shell(shell: &FlistWalkerApp, id: u64) -> Self {
        let base_results = shell
            .shell
            .runtime
            .entries
            .iter()
            .take(shell.shell.runtime.limit)
            .cloned()
            .map(|entry| (entry.path, 0.0))
            .collect::<Vec<_>>();
        let current_row = (!base_results.is_empty()).then_some(0);
        Self {
            id,
            root: shell.shell.runtime.root.clone(),
            tab_accent: None,
            use_filelist: true,
            use_regex: shell.shell.runtime.use_regex,
            ignore_case: shell.shell.runtime.ignore_case,
            include_files: shell.shell.runtime.include_files,
            include_dirs: shell.shell.runtime.include_dirs,
            max_depth: crate::indexer::MaxDepth::unlimited(),
            index_state: TabIndexState {
                resource_state: TabResourceState::new(
                    if shell.shell.indexing.committed_snapshot_present() {
                        TabResourceLifecycle::Ready
                    } else {
                        TabResourceLifecycle::Dormant
                    },
                    shell.shell.indexing.committed_snapshot_present(),
                ),
                build: TabBuildPayload {
                    index: IndexBuildResult {
                        entries: Vec::new(),
                        source: shell.shell.indexing.build.index.source.clone(),
                    },
                    ..TabBuildPayload::default()
                },
                pending_index_request_id: None,
                index_in_progress: false,
                pending_index_entries_request_id: None,
                pending_index_finish: None,
                build_reclaim_pending: false,
                build_reclaim_request_id: None,
                refresh_after_pending_finish: None,
                root_after_pending_finish: None,
                kind_resolution_epoch: 1,
                kind_resolution_in_progress: false,
                last_incremental_results_refresh: Instant::now(),
                last_search_snapshot_len: shell.shell.runtime.entries.len(),
                search_resume_pending: false,
                search_rerun_pending: false,
            },
            query_state: TabQueryState {
                query: String::new(),
                query_history: shell.shell.runtime.query_state.query_history.clone(),
                query_history_cursor: None,
                query_history_draft: None,
                history_search_active: false,
                history_search_query: String::new(),
                history_search_original_query: String::new(),
                history_search_results: Vec::new(),
                history_search_current: None,
            },
            result_state: TabResultState {
                committed: TabCommittedPayload {
                    all_entries: Arc::clone(&shell.shell.runtime.all_entries),
                    entries: Arc::clone(&shell.shell.runtime.entries),
                    results: base_results.clone(),
                    base_results,
                    total_match_count: shell.shell.runtime.entries.len(),
                    current_row,
                    ..TabCommittedPayload::default()
                },
                result_sort_mode: ResultSortMode::Score,
                result_sort_scope: shell.shell.runtime.result_sort_scope,
                pending_sort_request_id: None,
                sort_in_progress: false,
                pinned_paths: HashSet::new(),
                evicted_selected_path: None,
                results_compacted: false,
            },
            notice: "Opened new tab".to_string(),
            pending_request_id: None,
            pending_preview_request_id: None,
            pending_action_request_id: None,
            search_in_progress: false,
            preview_in_progress: false,
            preview_reload_pending: false,
            action_in_progress: false,
        }
    }

    pub(super) fn sync_small_fields_from_shell(&mut self, shell: &FlistWalkerApp) {
        self.root.clone_from(&shell.shell.runtime.root);
        self.use_filelist = shell.shell.runtime.use_filelist;
        self.use_regex = shell.shell.runtime.use_regex;
        self.ignore_case = shell.shell.runtime.ignore_case;
        self.include_files = shell.shell.runtime.include_files;
        self.include_dirs = shell.shell.runtime.include_dirs;
        self.max_depth = shell.shell.runtime.max_depth;
    }

    pub(super) fn apply_small_fields_to_shell(&self, shell: &mut FlistWalkerApp) {
        shell.shell.runtime.root.clone_from(&self.root);
        shell.shell.runtime.use_filelist = self.use_filelist;
        shell.shell.runtime.use_regex = self.use_regex;
        shell.shell.runtime.ignore_case = self.ignore_case;
        shell.shell.runtime.include_files = self.include_files;
        shell.shell.runtime.include_dirs = self.include_dirs;
        shell.shell.runtime.max_depth = self.max_depth;
    }

    pub(super) fn swap_payload_with_shell(&mut self, shell: &mut FlistWalkerApp) {
        mem::swap(&mut self.index_state.build, &mut shell.shell.indexing.build);
        mem::swap(
            &mut self.result_state.committed,
            &mut shell.shell.runtime.committed,
        );
        self.index_state.swap_shell(shell);
        self.query_state.swap_shell(shell);
        self.result_state.swap_shell(shell);
        mem::swap(&mut self.notice, &mut shell.shell.runtime.notice);

        let shell_search_request_id = shell.shell.search.pending_request_id();
        shell
            .shell
            .search
            .set_pending_request_id(self.pending_request_id);
        self.pending_request_id = shell_search_request_id;
        mem::swap(
            &mut self.pending_preview_request_id,
            &mut shell.shell.worker_bus.preview.pending_request_id,
        );
        mem::swap(
            &mut self.pending_action_request_id,
            &mut shell.shell.worker_bus.action.pending_request_id,
        );

        let shell_search_in_progress = shell.shell.search.in_progress();
        shell.shell.search.set_in_progress(self.search_in_progress);
        self.search_in_progress = shell_search_in_progress;
        mem::swap(
            &mut self.preview_in_progress,
            &mut shell.shell.worker_bus.preview.in_progress,
        );
        mem::swap(
            &mut self.action_in_progress,
            &mut shell.shell.worker_bus.action.in_progress,
        );
    }

    #[cfg(test)]
    pub(super) fn apply_shell(&self, shell: &mut FlistWalkerApp) {
        shell.shell.runtime.root = self.root.clone();
        shell.shell.runtime.use_filelist = self.use_filelist;
        shell.shell.runtime.use_regex = self.use_regex;
        shell.shell.runtime.ignore_case = self.ignore_case;
        shell.shell.runtime.include_files = self.include_files;
        shell.shell.runtime.include_dirs = self.include_dirs;
        shell.shell.runtime.max_depth = self.max_depth;
        self.index_state.apply_shell(shell);
        self.query_state.apply_shell(shell);
        self.result_state.apply_shell(shell);
        shell.shell.runtime.notice = self.notice.clone();
        shell
            .shell
            .search
            .set_pending_request_id(self.pending_request_id);
        shell.shell.worker_bus.preview.pending_request_id = self.pending_preview_request_id;
        shell.shell.worker_bus.action.pending_request_id = self.pending_action_request_id;
        shell.shell.search.set_in_progress(self.search_in_progress);
        shell.shell.worker_bus.preview.in_progress = self.preview_in_progress;
        shell.shell.worker_bus.action.in_progress = self.action_in_progress;
    }

    pub(super) fn to_saved(&self, history_persist_disabled: bool) -> SavedTabState {
        SavedTabState {
            root: self.root.to_string_lossy().to_string(),
            use_filelist: self.use_filelist,
            use_regex: self.use_regex,
            ignore_case: self.ignore_case,
            include_files: self.include_files,
            include_dirs: self.include_dirs,
            max_depth: self.max_depth,
            query: self.query_state.query.clone(),
            query_history: if history_persist_disabled {
                Vec::new()
            } else {
                self.query_state.query_history.iter().cloned().collect()
            },
            tab_accent: self.tab_accent,
        }
    }

    pub(super) fn saved_from_shell(
        shell: &FlistWalkerApp,
        history_persist_disabled: bool,
    ) -> SavedTabState {
        SavedTabState {
            root: shell.shell.runtime.root.to_string_lossy().to_string(),
            use_filelist: shell.shell.runtime.use_filelist,
            use_regex: shell.shell.runtime.use_regex,
            ignore_case: shell.shell.runtime.ignore_case,
            include_files: shell.shell.runtime.include_files,
            include_dirs: shell.shell.runtime.include_dirs,
            max_depth: shell.shell.runtime.max_depth,
            query: shell.shell.runtime.query_state.query.clone(),
            query_history: if history_persist_disabled {
                Vec::new()
            } else {
                shell
                    .shell
                    .runtime
                    .query_state
                    .query_history
                    .iter()
                    .cloned()
                    .collect()
            },
            tab_accent: shell
                .shell
                .tabs
                .get(shell.shell.tabs.active_tab_index())
                .and_then(|tab| tab.tab_accent),
        }
    }
}

use super::tab_state::{TabResourceState, TabResourceTransition};
use super::{
    AppTabState, BackgroundIndexFilterScratch, BackgroundIndexFinalizeScratch,
    BackgroundIndexState, Entry, EntryKindCacheState, FlistWalkerApp, IndexEntry,
    PendingBackgroundIndexFinalize,
};
use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread;

#[cfg(test)]
static RECLAIM_DROP_OBSERVER: std::sync::OnceLock<std::sync::Mutex<Option<mpsc::Sender<String>>>> =
    std::sync::OnceLock::new();
#[cfg(test)]
static RECLAIM_DROP_OBSERVER_TEST_LOCK: std::sync::OnceLock<std::sync::Mutex<()>> =
    std::sync::OnceLock::new();

#[cfg(test)]
#[derive(Debug)]
struct ReclaimDropProbe(Option<mpsc::Sender<String>>);

#[cfg(test)]
impl ReclaimDropProbe {
    fn capture() -> Self {
        let sender = RECLAIM_DROP_OBSERVER
            .get_or_init(|| std::sync::Mutex::new(None))
            .lock()
            .ok()
            .and_then(|sender| sender.clone());
        Self(sender)
    }
}

#[cfg(test)]
impl Drop for ReclaimDropProbe {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let name = thread::current().name().unwrap_or("unnamed").to_string();
            let _ = sender.send(name);
        }
    }
}

#[cfg(test)]
pub(super) fn set_reclaim_drop_observer(sender: Option<mpsc::Sender<String>>) {
    if let Ok(mut observer) = RECLAIM_DROP_OBSERVER
        .get_or_init(|| std::sync::Mutex::new(None))
        .lock()
    {
        *observer = sender;
    }
}

#[cfg(test)]
pub(super) fn lock_reclaim_drop_observer_for_test() -> std::sync::MutexGuard<'static, ()> {
    RECLAIM_DROP_OBSERVER_TEST_LOCK
        .get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(super) const TAB_RESOURCE_CACHE_MAX_COUNT: usize = 2;
pub(super) const TAB_RESOURCE_CACHE_MAX_WEIGHT: usize = 1_000_000;
pub(super) const TAB_RESOURCE_RECLAIMER_CAPACITY: usize = 4;

#[derive(Debug)]
pub(super) struct RetiredTabResources {
    #[cfg(test)]
    _drop_probe: ReclaimDropProbe,
    resource_state: TabResourceState,
    build_reclaim_pending: bool,
    build_reclaim_request_id: Option<u64>,
    index_entries: Vec<Entry>,
    all_entries: Arc<Vec<Entry>>,
    entries: Arc<Vec<Entry>>,
    pending_index_entries: VecDeque<IndexEntry>,
    pending_index_entries_request_id: Option<u64>,
    pending_kind_paths: VecDeque<PathBuf>,
    pending_kind_paths_set: HashSet<PathBuf>,
    in_flight_kind_paths: HashSet<PathBuf>,
    resolved_kind_updates: Vec<(PathBuf, crate::entry::EntryKind)>,
    kind_resolution_epoch: u64,
    kind_resolution_in_progress: bool,
    incremental_filtered_entries: Vec<Entry>,
    base_results: Vec<(PathBuf, f64)>,
    results: Vec<(PathBuf, f64)>,
    preview: String,
    total_match_count: usize,
    current_row: Option<usize>,
    results_compacted: bool,
    entry_kind_cache: EntryKindCacheState,
}

impl RetiredTabResources {
    pub(super) fn is_empty(&self) -> bool {
        self.index_entries.capacity() == 0
            && self.all_entries.capacity() == 0
            && self.entries.capacity() == 0
            && self.pending_index_entries.capacity() == 0
            && self.pending_kind_paths.capacity() == 0
            && self.pending_kind_paths_set.capacity() == 0
            && self.in_flight_kind_paths.capacity() == 0
            && self.resolved_kind_updates.capacity() == 0
            && self.incremental_filtered_entries.capacity() == 0
            && self.base_results.capacity() == 0
            && self.results.capacity() == 0
            && self.preview.capacity() == 0
            && self.entry_kind_cache.entries.capacity() == 0
    }
}

pub(super) struct RetiredActiveResources {
    all_entries: Arc<Vec<Entry>>,
    entries: Arc<Vec<Entry>>,
    base_results: Vec<(PathBuf, f64)>,
    results: Vec<(PathBuf, f64)>,
    preview: String,
    total_match_count: usize,
    current_row: Option<usize>,
}

pub(super) struct RetiredIndexBuildResources {
    #[cfg(test)]
    _drop_probe: ReclaimDropProbe,
    index_entries: Vec<Entry>,
    background_states: Vec<(u64, BackgroundIndexState)>,
    background_finalizations: Vec<(u64, PendingBackgroundIndexFinalize)>,
    background_finalize_scratch: Vec<BackgroundIndexFinalizeScratch>,
    background_filter_scratch: Vec<BackgroundIndexFilterScratch>,
    mailboxes: Vec<(u64, Arc<super::index_mailbox::IndexResponseMailbox>)>,
    stale_index_entries: Vec<IndexEntry>,
    pending_index_entries: VecDeque<IndexEntry>,
    pending_kind_paths: VecDeque<PathBuf>,
    pending_kind_paths_set: HashSet<PathBuf>,
    in_flight_kind_paths: HashSet<PathBuf>,
    resolved_kind_updates: Vec<(PathBuf, crate::entry::EntryKind)>,
    incremental_filtered_entries: Vec<Entry>,
    entry_kind_cache: EntryKindCacheState,
}

impl RetiredIndexBuildResources {
    pub(super) fn empty() -> Self {
        Self {
            #[cfg(test)]
            _drop_probe: ReclaimDropProbe::capture(),
            index_entries: Vec::new(),
            background_states: Vec::new(),
            background_finalizations: Vec::new(),
            background_finalize_scratch: Vec::new(),
            background_filter_scratch: Vec::new(),
            mailboxes: Vec::new(),
            stale_index_entries: Vec::new(),
            pending_index_entries: VecDeque::new(),
            pending_kind_paths: VecDeque::new(),
            pending_kind_paths_set: HashSet::new(),
            in_flight_kind_paths: HashSet::new(),
            resolved_kind_updates: Vec::new(),
            incremental_filtered_entries: Vec::new(),
            entry_kind_cache: EntryKindCacheState::default(),
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.index_entries.capacity() == 0
            && self.background_states.capacity() == 0
            && self
                .background_states
                .iter()
                .all(|(_, state)| state.entries.capacity() == 0)
            && self.background_finalizations.capacity() == 0
            && self
                .background_finalizations
                .iter()
                .all(|(_, state)| state.heavy_resource_weight() == 0)
            && self.background_finalize_scratch.capacity() == 0
            && self
                .background_finalize_scratch
                .iter()
                .all(|scratch| scratch.heavy_resource_weight() == 0)
            && self.background_filter_scratch.capacity() == 0
            && self
                .background_filter_scratch
                .iter()
                .all(|scratch| scratch.heavy_resource_weight() == 0)
            && self
                .mailboxes
                .iter()
                .all(|(_, mailbox)| !mailbox.has_payload())
            && self.stale_index_entries.capacity() == 0
            && self.pending_index_entries.capacity() == 0
            && self.pending_kind_paths.capacity() == 0
            && self.pending_kind_paths_set.capacity() == 0
            && self.in_flight_kind_paths.capacity() == 0
            && self.resolved_kind_updates.capacity() == 0
            && self.incremental_filtered_entries.capacity() == 0
            && self.entry_kind_cache.entries.capacity() == 0
    }

    pub(super) fn set_background_states(&mut self, states: Vec<(u64, BackgroundIndexState)>) {
        self.background_states = states;
    }

    pub(super) fn take_background_states(&mut self) -> Vec<(u64, BackgroundIndexState)> {
        std::mem::take(&mut self.background_states)
    }

    pub(super) fn set_background_finalizations(
        &mut self,
        states: Vec<(u64, PendingBackgroundIndexFinalize)>,
    ) {
        self.background_finalizations = states;
    }

    pub(super) fn take_background_finalizations(
        &mut self,
    ) -> Vec<(u64, PendingBackgroundIndexFinalize)> {
        std::mem::take(&mut self.background_finalizations)
    }

    pub(super) fn set_background_finalize_scratch(
        &mut self,
        scratch: Vec<BackgroundIndexFinalizeScratch>,
    ) {
        self.background_finalize_scratch = scratch;
    }

    pub(super) fn take_background_finalize_scratch(
        &mut self,
    ) -> Vec<BackgroundIndexFinalizeScratch> {
        std::mem::take(&mut self.background_finalize_scratch)
    }

    pub(super) fn set_background_filter_scratch(
        &mut self,
        scratch: Vec<BackgroundIndexFilterScratch>,
    ) {
        self.background_filter_scratch = scratch;
    }

    pub(super) fn take_background_filter_scratch(&mut self) -> Vec<BackgroundIndexFilterScratch> {
        std::mem::take(&mut self.background_filter_scratch)
    }

    pub(super) fn set_mailboxes(
        &mut self,
        mailboxes: Vec<(u64, Arc<super::index_mailbox::IndexResponseMailbox>)>,
    ) {
        self.mailboxes = mailboxes;
    }

    pub(super) fn take_mailboxes(
        &mut self,
    ) -> Vec<(u64, Arc<super::index_mailbox::IndexResponseMailbox>)> {
        std::mem::take(&mut self.mailboxes)
    }

    pub(super) fn mailbox_handles(&self) -> Vec<Arc<super::index_mailbox::IndexResponseMailbox>> {
        self.mailboxes
            .iter()
            .map(|(_, mailbox)| Arc::clone(mailbox))
            .collect()
    }

    pub(super) fn set_stale_index_entries(&mut self, entries: Vec<IndexEntry>) {
        self.stale_index_entries = entries;
    }
}

impl RetiredActiveResources {
    pub(super) fn is_empty(&self) -> bool {
        self.all_entries.capacity() == 0
            && self.entries.capacity() == 0
            && self.base_results.capacity() == 0
            && self.results.capacity() == 0
            && self.preview.capacity() == 0
    }
}

enum ReclaimPayload {
    Tab(Box<RetiredTabResources>),
    Active(RetiredActiveResources),
    IndexBuild(Box<RetiredIndexBuildResources>),
}

impl FlistWalkerApp {
    pub(super) fn try_retire_active_index_build_resources(&mut self) -> bool {
        self.try_retire_active_index_build_resources_with_scope(false)
    }

    pub(super) fn try_retire_active_index_build_resources_for_boundary(&mut self) -> bool {
        self.try_retire_active_index_build_resources_with_scope(true)
    }

    fn try_retire_active_index_build_resources_with_scope(
        &mut self,
        include_all_tab_requests: bool,
    ) -> bool {
        let active_tab_id = self.current_tab_id();
        let mut request_ids = Vec::new();
        if include_all_tab_requests {
            if let Some(tab_id) = active_tab_id {
                request_ids.extend(self.shell.indexing.request_ids_for_tab(tab_id));
            }
        }
        request_ids.extend(self.shell.indexing.pending_request_id);
        request_ids.extend(self.shell.indexing.build_reclaim_request_id);
        request_ids.sort_unstable();
        request_ids.dedup();
        let background_states = self
            .shell
            .indexing
            .take_background_states_for_requests(&request_ids);
        let background_finalizations = self
            .shell
            .indexing
            .take_background_finalizations_for_requests(&request_ids);
        let mailboxes = self
            .shell
            .indexing
            .take_mailboxes_for_requests(&request_ids);
        let mailboxes_to_close = mailboxes
            .iter()
            .map(|(_, mailbox)| Arc::clone(mailbox))
            .collect::<Vec<_>>();
        let mut resources = self.take_active_index_build_resources();
        resources.set_background_states(background_states);
        resources.set_background_finalizations(background_finalizations);
        resources.set_mailboxes(mailboxes);
        match self.shell.tabs.try_retire_index_build_resources(resources) {
            Ok(()) => {
                for mailbox in mailboxes_to_close {
                    mailbox.close();
                }
                if !request_ids.is_empty() {
                    self.shell.indexing.settle_active_terminal_state();
                }
                self.shell.indexing.build_reclaim_pending = false;
                self.shell.indexing.build_reclaim_request_id = None;
                for request_id in request_ids {
                    self.shell.indexing.cleanup_request(request_id);
                }
                true
            }
            Err(resources) => {
                let mut resources = *resources;
                self.shell
                    .indexing
                    .restore_background_states(resources.take_background_states());
                self.shell
                    .indexing
                    .restore_background_finalizations(resources.take_background_finalizations());
                self.shell
                    .indexing
                    .restore_mailboxes(resources.take_mailboxes());
                self.restore_active_index_build_resources(resources);
                self.shell.indexing.build_reclaim_pending = true;
                self.shell.indexing.build_reclaim_request_id = self
                    .shell
                    .indexing
                    .build_reclaim_request_id
                    .or(self.shell.indexing.pending_request_id)
                    .or_else(|| request_ids.first().copied());
                self.set_notice("Waiting for background tab resource reclamation");
                false
            }
        }
    }

    pub(super) fn try_retire_tab_index_build_resources(&mut self, tab_index: usize) -> bool {
        self.try_retire_tab_index_build_resources_with_scope(tab_index, false)
    }

    pub(super) fn try_retire_tab_index_build_resources_for_boundary(
        &mut self,
        tab_index: usize,
    ) -> bool {
        self.try_retire_tab_index_build_resources_with_scope(tab_index, true)
    }

    fn try_retire_tab_index_build_resources_with_scope(
        &mut self,
        tab_index: usize,
        include_all_tab_requests: bool,
    ) -> bool {
        let tab_id = self.shell.tabs.get(tab_index).expect("validated tab").id;
        let pending_request_id = self
            .shell
            .tabs
            .get(tab_index)
            .and_then(|tab| tab.index_state.pending_index_request_id);
        let build_reclaim_request_id = self
            .shell
            .tabs
            .get(tab_index)
            .and_then(|tab| tab.index_state.build_reclaim_request_id);
        let mut request_ids = Vec::new();
        if include_all_tab_requests {
            request_ids.extend(self.shell.indexing.request_ids_for_tab(tab_id));
        }
        request_ids.extend(pending_request_id);
        request_ids.extend(build_reclaim_request_id);
        request_ids.sort_unstable();
        request_ids.dedup();
        let retires_superseded_generation = request_ids
            .iter()
            .any(|request_id| self.shell.indexing.is_superseded_request(*request_id));
        let background_states = self
            .shell
            .indexing
            .take_background_states_for_requests(&request_ids);
        let background_finalizations = self
            .shell
            .indexing
            .take_background_finalizations_for_requests(&request_ids);
        let mailboxes = self
            .shell
            .indexing
            .take_mailboxes_for_requests(&request_ids);
        let mailboxes_to_close = mailboxes
            .iter()
            .map(|(_, mailbox)| Arc::clone(mailbox))
            .collect::<Vec<_>>();
        let mut resources = self
            .shell
            .tabs
            .get_mut(tab_index)
            .expect("validated tab")
            .take_index_build_resources();
        resources.set_background_states(background_states);
        resources.set_background_finalizations(background_finalizations);
        resources.set_mailboxes(mailboxes);
        match self.shell.tabs.try_retire_index_build_resources(resources) {
            Ok(()) => {
                for mailbox in mailboxes_to_close {
                    mailbox.close();
                }
                {
                    let tab = self.shell.tabs.get_mut(tab_index).expect("validated tab");
                    tab.index_state.build_reclaim_pending = false;
                    tab.index_state.build_reclaim_request_id = None;
                    if !request_ids.is_empty() {
                        tab.index_state.clear_index_request_state();
                    }
                    if retires_superseded_generation {
                        tab.index_state
                            .resource_state
                            .apply(TabResourceTransition::Cancel);
                    }
                }
                for request_id in request_ids {
                    self.shell.indexing.cleanup_request(request_id);
                }
                true
            }
            Err(resources) => {
                let mut resources = *resources;
                self.shell
                    .indexing
                    .restore_background_states(resources.take_background_states());
                self.shell
                    .indexing
                    .restore_background_finalizations(resources.take_background_finalizations());
                self.shell
                    .indexing
                    .restore_mailboxes(resources.take_mailboxes());
                let tab = self.shell.tabs.get_mut(tab_index).expect("validated tab");
                tab.restore_index_build_resources(resources);
                tab.index_state.build_reclaim_pending = true;
                tab.index_state.build_reclaim_request_id = tab
                    .index_state
                    .build_reclaim_request_id
                    .or(tab.index_state.pending_index_request_id)
                    .or_else(|| request_ids.first().copied());
                tab.notice = "Waiting for background tab resource reclamation".to_string();
                false
            }
        }
    }

    pub(super) fn take_active_index_build_resources(&mut self) -> RetiredIndexBuildResources {
        RetiredIndexBuildResources {
            #[cfg(test)]
            _drop_probe: ReclaimDropProbe::capture(),
            index_entries: std::mem::take(&mut self.shell.runtime.index.entries),
            background_states: Vec::new(),
            background_finalizations: Vec::new(),
            background_finalize_scratch: Vec::new(),
            background_filter_scratch: Vec::new(),
            mailboxes: Vec::new(),
            stale_index_entries: Vec::new(),
            pending_index_entries: std::mem::take(&mut self.shell.indexing.pending_entries),
            pending_kind_paths: std::mem::take(&mut self.shell.indexing.pending_kind_paths),
            pending_kind_paths_set: std::mem::take(&mut self.shell.indexing.pending_kind_paths_set),
            in_flight_kind_paths: std::mem::take(&mut self.shell.indexing.in_flight_kind_paths),
            resolved_kind_updates: std::mem::take(&mut self.shell.indexing.resolved_kind_updates),
            incremental_filtered_entries: std::mem::take(
                &mut self.shell.indexing.incremental_filtered_entries,
            ),
            entry_kind_cache: std::mem::take(&mut self.shell.cache.entry_kind),
        }
    }

    pub(super) fn restore_active_index_build_resources(
        &mut self,
        resources: RetiredIndexBuildResources,
    ) {
        self.shell.runtime.index.entries = resources.index_entries;
        self.shell.indexing.pending_entries = resources.pending_index_entries;
        self.shell.indexing.pending_kind_paths = resources.pending_kind_paths;
        self.shell.indexing.pending_kind_paths_set = resources.pending_kind_paths_set;
        self.shell.indexing.in_flight_kind_paths = resources.in_flight_kind_paths;
        self.shell.indexing.resolved_kind_updates = resources.resolved_kind_updates;
        self.shell.indexing.incremental_filtered_entries = resources.incremental_filtered_entries;
        self.shell.cache.entry_kind = resources.entry_kind_cache;
    }

    pub(super) fn take_active_committed_resources(&mut self) -> RetiredActiveResources {
        self.shell.runtime.evicted_selected_path = self
            .shell
            .runtime
            .current_row
            .and_then(|row| self.shell.runtime.results.get(row))
            .map(|(path, _)| path.clone())
            .or_else(|| self.shell.runtime.evicted_selected_path.clone());
        RetiredActiveResources {
            all_entries: std::mem::replace(
                &mut self.shell.runtime.all_entries,
                Arc::new(Vec::new()),
            ),
            entries: std::mem::replace(&mut self.shell.runtime.entries, Arc::new(Vec::new())),
            base_results: std::mem::take(&mut self.shell.runtime.base_results),
            results: std::mem::take(&mut self.shell.runtime.results),
            preview: std::mem::take(&mut self.shell.runtime.preview),
            total_match_count: std::mem::take(&mut self.shell.runtime.total_match_count),
            current_row: self.shell.runtime.current_row.take(),
        }
    }

    pub(super) fn restore_active_committed_resources(&mut self, resources: RetiredActiveResources) {
        self.shell.runtime.all_entries = resources.all_entries;
        self.shell.runtime.entries = resources.entries;
        self.shell.runtime.base_results = resources.base_results;
        self.shell.runtime.results = resources.results;
        self.shell.runtime.preview = resources.preview;
        self.shell.runtime.total_match_count = resources.total_match_count;
        self.shell.runtime.current_row = resources.current_row;
    }
}

impl AppTabState {
    pub(super) fn take_index_build_resources(&mut self) -> RetiredIndexBuildResources {
        RetiredIndexBuildResources {
            #[cfg(test)]
            _drop_probe: ReclaimDropProbe::capture(),
            index_entries: std::mem::take(&mut self.index_state.index.entries),
            background_states: Vec::new(),
            background_finalizations: Vec::new(),
            background_finalize_scratch: Vec::new(),
            background_filter_scratch: Vec::new(),
            mailboxes: Vec::new(),
            stale_index_entries: Vec::new(),
            pending_index_entries: std::mem::take(&mut self.index_state.pending_index_entries),
            pending_kind_paths: std::mem::take(&mut self.index_state.pending_kind_paths),
            pending_kind_paths_set: std::mem::take(&mut self.index_state.pending_kind_paths_set),
            in_flight_kind_paths: std::mem::take(&mut self.index_state.in_flight_kind_paths),
            resolved_kind_updates: std::mem::take(&mut self.index_state.resolved_kind_updates),
            incremental_filtered_entries: std::mem::take(
                &mut self.index_state.incremental_filtered_entries,
            ),
            entry_kind_cache: std::mem::take(&mut self.entry_kind_cache),
        }
    }

    pub(super) fn restore_index_build_resources(&mut self, resources: RetiredIndexBuildResources) {
        self.index_state.index.entries = resources.index_entries;
        self.index_state.pending_index_entries = resources.pending_index_entries;
        self.index_state.pending_kind_paths = resources.pending_kind_paths;
        self.index_state.pending_kind_paths_set = resources.pending_kind_paths_set;
        self.index_state.in_flight_kind_paths = resources.in_flight_kind_paths;
        self.index_state.resolved_kind_updates = resources.resolved_kind_updates;
        self.index_state.incremental_filtered_entries = resources.incremental_filtered_entries;
        self.entry_kind_cache = resources.entry_kind_cache;
    }

    pub(super) fn take_committed_resources(&mut self) -> RetiredActiveResources {
        self.result_state.evicted_selected_path = self
            .result_state
            .current_row
            .and_then(|row| self.result_state.results.get(row))
            .map(|(path, _)| path.clone())
            .or_else(|| self.result_state.evicted_selected_path.clone());
        self.index_state
            .resource_state
            .apply(TabResourceTransition::SnapshotRemoved);
        RetiredActiveResources {
            all_entries: std::mem::replace(&mut self.index_state.all_entries, Arc::new(Vec::new())),
            entries: std::mem::replace(&mut self.index_state.entries, Arc::new(Vec::new())),
            base_results: std::mem::take(&mut self.result_state.base_results),
            results: std::mem::take(&mut self.result_state.results),
            preview: std::mem::take(&mut self.result_state.preview),
            total_match_count: std::mem::take(&mut self.result_state.total_match_count),
            current_row: self.result_state.current_row.take(),
        }
    }

    pub(super) fn restore_committed_resources(&mut self, resources: RetiredActiveResources) {
        self.index_state
            .resource_state
            .apply(TabResourceTransition::SnapshotRestored);
        self.index_state.all_entries = resources.all_entries;
        self.index_state.entries = resources.entries;
        self.result_state.base_results = resources.base_results;
        self.result_state.results = resources.results;
        self.result_state.preview = resources.preview;
        self.result_state.total_match_count = resources.total_match_count;
        self.result_state.current_row = resources.current_row;
    }
}

impl AppTabState {
    pub(super) fn heavy_resource_weight(&self) -> usize {
        let committed_weight =
            if Arc::ptr_eq(&self.index_state.all_entries, &self.index_state.entries) {
                self.index_state.all_entries.capacity()
            } else {
                self.index_state
                    .all_entries
                    .capacity()
                    .saturating_add(self.index_state.entries.capacity())
            };
        self.index_state
            .index
            .entries
            .capacity()
            .saturating_add(committed_weight)
            .saturating_add(self.index_state.pending_index_entries.capacity())
            .saturating_add(self.index_state.pending_kind_paths.capacity())
            .saturating_add(self.index_state.pending_kind_paths_set.capacity())
            .saturating_add(self.index_state.in_flight_kind_paths.capacity())
            .saturating_add(self.index_state.resolved_kind_updates.capacity())
            .saturating_add(self.index_state.incremental_filtered_entries.capacity())
            .saturating_add(self.result_state.base_results.capacity())
            .saturating_add(self.result_state.results.capacity())
            .saturating_add(self.result_state.preview.capacity())
            .saturating_add(self.entry_kind_cache.entries.capacity())
    }

    pub(super) fn take_heavy_resources(&mut self) -> RetiredTabResources {
        self.result_state.evicted_selected_path = self
            .result_state
            .current_row
            .and_then(|row| self.result_state.results.get(row))
            .map(|(path, _)| path.clone())
            .or_else(|| self.result_state.evicted_selected_path.clone());
        let resources = RetiredTabResources {
            #[cfg(test)]
            _drop_probe: ReclaimDropProbe::capture(),
            resource_state: self.index_state.resource_state,
            build_reclaim_pending: self.index_state.build_reclaim_pending,
            build_reclaim_request_id: self.index_state.build_reclaim_request_id,
            index_entries: std::mem::take(&mut self.index_state.index.entries),
            all_entries: std::mem::replace(&mut self.index_state.all_entries, Arc::new(Vec::new())),
            entries: std::mem::replace(&mut self.index_state.entries, Arc::new(Vec::new())),
            pending_index_entries: std::mem::take(&mut self.index_state.pending_index_entries),
            pending_index_entries_request_id: self
                .index_state
                .pending_index_entries_request_id
                .take(),
            pending_kind_paths: std::mem::take(&mut self.index_state.pending_kind_paths),
            pending_kind_paths_set: std::mem::take(&mut self.index_state.pending_kind_paths_set),
            in_flight_kind_paths: std::mem::take(&mut self.index_state.in_flight_kind_paths),
            resolved_kind_updates: std::mem::take(&mut self.index_state.resolved_kind_updates),
            kind_resolution_epoch: self.index_state.kind_resolution_epoch,
            kind_resolution_in_progress: self.index_state.kind_resolution_in_progress,
            incremental_filtered_entries: std::mem::take(
                &mut self.index_state.incremental_filtered_entries,
            ),
            base_results: std::mem::take(&mut self.result_state.base_results),
            results: std::mem::take(&mut self.result_state.results),
            preview: std::mem::take(&mut self.result_state.preview),
            total_match_count: self.result_state.total_match_count,
            current_row: self.result_state.current_row,
            results_compacted: self.result_state.results_compacted,
            entry_kind_cache: std::mem::take(&mut self.entry_kind_cache),
        };
        self.index_state
            .resource_state
            .apply(TabResourceTransition::Evict);
        self.index_state.build_reclaim_pending = false;
        self.index_state.build_reclaim_request_id = None;
        self.index_state.clear_kind_resolution_state();
        self.result_state.total_match_count = 0;
        self.result_state.current_row = None;
        self.result_state.results_compacted = false;
        resources
    }

    pub(super) fn restore_heavy_resources(&mut self, resources: RetiredTabResources) {
        self.index_state
            .resource_state
            .apply(TabResourceTransition::ReclaimFullRollback(
                resources.resource_state,
            ));
        self.index_state.build_reclaim_pending = resources.build_reclaim_pending;
        self.index_state.build_reclaim_request_id = resources.build_reclaim_request_id;
        self.index_state.index.entries = resources.index_entries;
        self.index_state.all_entries = resources.all_entries;
        self.index_state.entries = resources.entries;
        self.index_state.pending_index_entries = resources.pending_index_entries;
        self.index_state.pending_index_entries_request_id =
            resources.pending_index_entries_request_id;
        self.index_state.pending_kind_paths = resources.pending_kind_paths;
        self.index_state.pending_kind_paths_set = resources.pending_kind_paths_set;
        self.index_state.in_flight_kind_paths = resources.in_flight_kind_paths;
        self.index_state.resolved_kind_updates = resources.resolved_kind_updates;
        self.index_state.kind_resolution_epoch = resources.kind_resolution_epoch;
        self.index_state.kind_resolution_in_progress = resources.kind_resolution_in_progress;
        self.index_state.incremental_filtered_entries = resources.incremental_filtered_entries;
        self.result_state.base_results = resources.base_results;
        self.result_state.results = resources.results;
        self.result_state.preview = resources.preview;
        self.result_state.total_match_count = resources.total_match_count;
        self.result_state.current_row = resources.current_row;
        self.result_state.results_compacted = resources.results_compacted;
        self.entry_kind_cache = resources.entry_kind_cache;
    }
}

pub(super) struct TabResourceReclaimer {
    tx: Option<SyncSender<ReclaimPayload>>,
    pending: Arc<AtomicUsize>,
    handle: Option<thread::JoinHandle<()>>,
    #[cfg(test)]
    _paused_rx: Option<mpsc::Receiver<ReclaimPayload>>,
}

impl Default for TabResourceReclaimer {
    fn default() -> Self {
        let (mut reclaimer, handle) = Self::spawn_managed();
        reclaimer.handle = Some(handle);
        reclaimer
    }
}

impl TabResourceReclaimer {
    pub(super) fn spawn_managed() -> (Self, thread::JoinHandle<()>) {
        let (tx, rx) = mpsc::sync_channel::<ReclaimPayload>(TAB_RESOURCE_RECLAIMER_CAPACITY);
        let pending = Arc::new(AtomicUsize::new(0));
        let worker_pending = Arc::clone(&pending);
        let handle = thread::Builder::new()
            .name("flistwalker-tab-reclaimer".to_string())
            .spawn(move || {
                while let Ok(resources) = rx.recv() {
                    match resources {
                        ReclaimPayload::Tab(resources) => drop(resources),
                        ReclaimPayload::Active(resources) => drop(resources),
                        ReclaimPayload::IndexBuild(resources) => drop(resources),
                    }
                    worker_pending.fetch_sub(1, Ordering::AcqRel);
                }
            })
            .expect("spawn tab resource reclaimer");
        (
            Self {
                tx: Some(tx),
                pending,
                handle: None,
                #[cfg(test)]
                _paused_rx: None,
            },
            handle,
        )
    }

    pub(super) fn disconnect(&mut self) {
        self.tx.take();
    }

    #[cfg(test)]
    pub(super) fn paused_for_test() -> Self {
        let (tx, rx) = mpsc::sync_channel(TAB_RESOURCE_RECLAIMER_CAPACITY);
        Self {
            tx: Some(tx),
            pending: Arc::new(AtomicUsize::new(0)),
            handle: None,
            _paused_rx: Some(rx),
        }
    }

    pub(super) fn try_retire(
        &self,
        resources: RetiredTabResources,
    ) -> Result<(), Box<RetiredTabResources>> {
        if resources.is_empty() {
            return Ok(());
        }
        match self.try_send(ReclaimPayload::Tab(Box::new(resources))) {
            Ok(()) => Ok(()),
            Err(ReclaimPayload::Tab(resources)) => Err(resources),
            Err(ReclaimPayload::Active(_)) => unreachable!("tab payload changed variant"),
            Err(ReclaimPayload::IndexBuild(_)) => unreachable!("tab payload changed variant"),
        }
    }

    pub(super) fn try_retire_active(
        &self,
        resources: RetiredActiveResources,
    ) -> Result<(), RetiredActiveResources> {
        match self.try_send(ReclaimPayload::Active(resources)) {
            Ok(()) => Ok(()),
            Err(ReclaimPayload::Active(resources)) => Err(resources),
            Err(ReclaimPayload::Tab(_)) => unreachable!("active payload changed variant"),
            Err(ReclaimPayload::IndexBuild(_)) => {
                unreachable!("active payload changed variant")
            }
        }
    }

    pub(super) fn try_retire_index_build(
        &self,
        resources: RetiredIndexBuildResources,
    ) -> Result<(), Box<RetiredIndexBuildResources>> {
        if resources.is_empty() {
            return Ok(());
        }
        match self.try_send(ReclaimPayload::IndexBuild(Box::new(resources))) {
            Ok(()) => Ok(()),
            Err(ReclaimPayload::IndexBuild(resources)) => Err(resources),
            Err(ReclaimPayload::Tab(_) | ReclaimPayload::Active(_)) => {
                unreachable!("index build payload changed variant")
            }
        }
    }

    fn try_send(&self, resources: ReclaimPayload) -> Result<(), ReclaimPayload> {
        let Some(tx) = self.tx.as_ref() else {
            return Err(resources);
        };
        self.pending.fetch_add(1, Ordering::AcqRel);
        match tx.try_send(resources) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(resources) | TrySendError::Disconnected(resources)) => {
                self.pending.fetch_sub(1, Ordering::AcqRel);
                Err(resources)
            }
        }
    }

    #[cfg(test)]
    pub(super) fn pending(&self) -> usize {
        self.pending.load(Ordering::Acquire)
    }
}

impl Drop for TabResourceReclaimer {
    fn drop(&mut self) {
        self.tx.take();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

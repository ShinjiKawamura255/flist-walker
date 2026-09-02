use super::{
    AppTabState, Entry, EntryKindCacheState, FlistWalkerApp, IndexEntry, TabResourceLifecycle,
};
use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread;

pub(super) const TAB_RESOURCE_CACHE_MAX_COUNT: usize = 2;
pub(super) const TAB_RESOURCE_CACHE_MAX_WEIGHT: usize = 1_000_000;
pub(super) const TAB_RESOURCE_RECLAIMER_CAPACITY: usize = 4;

#[derive(Debug)]
pub(super) struct RetiredTabResources {
    lifecycle: TabResourceLifecycle,
    index_entries: Vec<Entry>,
    all_entries: Arc<Vec<Entry>>,
    entries: Arc<Vec<Entry>>,
    pending_index_entries: VecDeque<IndexEntry>,
    pending_kind_paths: VecDeque<PathBuf>,
    pending_kind_paths_set: HashSet<PathBuf>,
    in_flight_kind_paths: HashSet<PathBuf>,
    resolved_kind_updates: Vec<(PathBuf, crate::entry::EntryKind)>,
    incremental_filtered_entries: Vec<Entry>,
    base_results: Vec<(PathBuf, f64)>,
    results: Vec<(PathBuf, f64)>,
    pinned_paths: HashSet<PathBuf>,
    preview: String,
    total_match_count: usize,
    current_row: Option<usize>,
    results_compacted: bool,
    entry_kind_cache: EntryKindCacheState,
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

impl RetiredActiveResources {
    pub(super) fn is_empty(&self) -> bool {
        self.all_entries.is_empty()
            && self.entries.is_empty()
            && self.base_results.is_empty()
            && self.results.is_empty()
            && self.preview.is_empty()
    }
}

enum ReclaimPayload {
    Tab(Box<RetiredTabResources>),
    Active(RetiredActiveResources),
}

impl FlistWalkerApp {
    pub(super) fn take_active_committed_resources(&mut self) -> RetiredActiveResources {
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
    pub(super) fn heavy_resource_weight(&self) -> usize {
        let committed_weight =
            if Arc::ptr_eq(&self.index_state.all_entries, &self.index_state.entries) {
                self.index_state.all_entries.len()
            } else {
                self.index_state
                    .all_entries
                    .len()
                    .saturating_add(self.index_state.entries.len())
            };
        self.index_state
            .index
            .entries
            .len()
            .saturating_add(committed_weight)
            .saturating_add(self.index_state.pending_index_entries.len())
            .saturating_add(self.index_state.pending_kind_paths.len())
            .saturating_add(self.index_state.pending_kind_paths_set.len())
            .saturating_add(self.index_state.in_flight_kind_paths.len())
            .saturating_add(self.index_state.resolved_kind_updates.len())
            .saturating_add(self.index_state.incremental_filtered_entries.len())
            .saturating_add(self.result_state.base_results.len())
            .saturating_add(self.result_state.results.len())
            .saturating_add(self.result_state.pinned_paths.len())
            .saturating_add(self.entry_kind_cache.entries.len())
    }

    pub(super) fn take_heavy_resources(&mut self) -> RetiredTabResources {
        let resources = RetiredTabResources {
            lifecycle: self.index_state.lifecycle,
            index_entries: std::mem::take(&mut self.index_state.index.entries),
            all_entries: std::mem::replace(&mut self.index_state.all_entries, Arc::new(Vec::new())),
            entries: std::mem::replace(&mut self.index_state.entries, Arc::new(Vec::new())),
            pending_index_entries: std::mem::take(&mut self.index_state.pending_index_entries),
            pending_kind_paths: std::mem::take(&mut self.index_state.pending_kind_paths),
            pending_kind_paths_set: std::mem::take(&mut self.index_state.pending_kind_paths_set),
            in_flight_kind_paths: std::mem::take(&mut self.index_state.in_flight_kind_paths),
            resolved_kind_updates: std::mem::take(&mut self.index_state.resolved_kind_updates),
            incremental_filtered_entries: std::mem::take(
                &mut self.index_state.incremental_filtered_entries,
            ),
            base_results: std::mem::take(&mut self.result_state.base_results),
            results: std::mem::take(&mut self.result_state.results),
            pinned_paths: std::mem::take(&mut self.result_state.pinned_paths),
            preview: std::mem::take(&mut self.result_state.preview),
            total_match_count: self.result_state.total_match_count,
            current_row: self.result_state.current_row,
            results_compacted: self.result_state.results_compacted,
            entry_kind_cache: std::mem::take(&mut self.entry_kind_cache),
        };
        self.index_state.lifecycle = TabResourceLifecycle::Evicted;
        self.index_state.pending_index_entries_request_id = None;
        self.index_state.pending_index_finish = None;
        self.index_state.clear_kind_resolution_state();
        self.result_state.total_match_count = 0;
        self.result_state.current_row = None;
        self.result_state.results_compacted = false;
        resources
    }

    pub(super) fn restore_heavy_resources(&mut self, resources: RetiredTabResources) {
        self.index_state.lifecycle = resources.lifecycle;
        self.index_state.index.entries = resources.index_entries;
        self.index_state.all_entries = resources.all_entries;
        self.index_state.entries = resources.entries;
        self.index_state.pending_index_entries = resources.pending_index_entries;
        self.index_state.pending_kind_paths = resources.pending_kind_paths;
        self.index_state.pending_kind_paths_set = resources.pending_kind_paths_set;
        self.index_state.in_flight_kind_paths = resources.in_flight_kind_paths;
        self.index_state.resolved_kind_updates = resources.resolved_kind_updates;
        self.index_state.incremental_filtered_entries = resources.incremental_filtered_entries;
        self.result_state.base_results = resources.base_results;
        self.result_state.results = resources.results;
        self.result_state.pinned_paths = resources.pinned_paths;
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
                    }
                    worker_pending.fetch_sub(1, Ordering::AcqRel);
                }
            })
            .expect("spawn tab resource reclaimer");
        Self {
            tx: Some(tx),
            pending,
            handle: Some(handle),
            #[cfg(test)]
            _paused_rx: None,
        }
    }
}

impl TabResourceReclaimer {
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
        match self.try_send(ReclaimPayload::Tab(Box::new(resources))) {
            Ok(()) => Ok(()),
            Err(ReclaimPayload::Tab(resources)) => Err(resources),
            Err(ReclaimPayload::Active(_)) => unreachable!("tab payload changed variant"),
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

use super::tab_state::{
    TabBuildPayload, TabCommittedPayload, TabResourceState, TabResourceTransition,
};
use super::{
    AppTabState, BackgroundIndexFilterScratch, BackgroundIndexFinalizeScratch,
    BackgroundIndexState, FlistWalkerApp, IndexEntry, PendingBackgroundIndexFinalize,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[cfg(test)]
static RECLAIM_DROP_OBSERVER: std::sync::OnceLock<std::sync::Mutex<Option<ReclaimDropObserver>>> =
    std::sync::OnceLock::new();
#[cfg(test)]
static RECLAIM_DROP_OBSERVER_TEST_LOCK: std::sync::OnceLock<std::sync::Mutex<()>> =
    std::sync::OnceLock::new();

#[cfg(test)]
#[derive(Debug)]
struct ReclaimDropProbe(Option<mpsc::Sender<String>>);

#[cfg(test)]
struct ReclaimDropObserver {
    sender: mpsc::Sender<String>,
    capture_thread: thread::ThreadId,
}

#[cfg(test)]
impl ReclaimDropProbe {
    fn capture() -> Self {
        let capture_thread = thread::current().id();
        let sender = RECLAIM_DROP_OBSERVER
            .get_or_init(|| std::sync::Mutex::new(None))
            .lock()
            .ok()
            .and_then(|observer| {
                observer.as_ref().and_then(|observer| {
                    (observer.capture_thread == capture_thread).then(|| observer.sender.clone())
                })
            });
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
        *observer = sender.map(|sender| ReclaimDropObserver {
            sender,
            capture_thread: thread::current().id(),
        });
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
pub(super) const TAB_RESOURCE_CACHE_HARD_MAX_COUNT: usize = 3;
pub(super) const TAB_RESOURCE_CACHE_HARD_MAX_WEIGHT: usize = 4_000_000;
pub(super) const TAB_RECENT_INACTIVE_ENGAGEMENT_THRESHOLD: Duration = Duration::from_secs(2);
pub(super) const TAB_RECENT_INACTIVE_GRACE: Duration = Duration::from_secs(30);
pub(super) const TAB_RESOURCE_RECLAIMER_CAPACITY: usize = 4;

#[derive(Debug)]
pub(super) struct RetiredTabResources {
    #[cfg(test)]
    _drop_probe: ReclaimDropProbe,
    control: TabHeavyControlPayload,
    build: TabBuildPayload,
    committed: TabCommittedPayload,
}

#[derive(Debug)]
struct TabHeavyControlPayload {
    resource_state: TabResourceState,
    build_reclaim_pending: bool,
    build_reclaim_request_id: Option<u64>,
    pending_index_entries_request_id: Option<u64>,
    kind_resolution_epoch: u64,
    kind_resolution_in_progress: bool,
    results_compacted: bool,
}

impl RetiredTabResources {
    pub(super) fn is_empty(&self) -> bool {
        self.build.is_empty() && self.committed.is_empty()
    }
}

pub(super) struct RetiredActiveResources {
    committed: TabCommittedPayload,
}

pub(super) struct RetiredIndexBuildResources {
    #[cfg(test)]
    _drop_probe: ReclaimDropProbe,
    build: TabBuildPayload,
    routing: RetiredRoutingPayload,
}

#[derive(Default)]
struct RetiredRoutingPayload {
    background_states: Vec<(u64, BackgroundIndexState)>,
    background_finalizations: Vec<(u64, PendingBackgroundIndexFinalize)>,
    background_finalize_scratch: Vec<BackgroundIndexFinalizeScratch>,
    background_filter_scratch: Vec<BackgroundIndexFilterScratch>,
    mailboxes: Vec<(u64, Arc<super::index_mailbox::IndexResponseMailbox>)>,
    stale_index_entries: Vec<IndexEntry>,
}

impl RetiredIndexBuildResources {
    pub(super) fn empty() -> Self {
        Self {
            #[cfg(test)]
            _drop_probe: ReclaimDropProbe::capture(),
            build: TabBuildPayload::default(),
            routing: RetiredRoutingPayload::default(),
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.build.is_empty() && self.routing.is_empty()
    }

    pub(super) fn set_background_states(&mut self, states: Vec<(u64, BackgroundIndexState)>) {
        self.routing.background_states = states;
    }

    pub(super) fn take_background_states(&mut self) -> Vec<(u64, BackgroundIndexState)> {
        std::mem::take(&mut self.routing.background_states)
    }

    pub(super) fn set_background_finalizations(
        &mut self,
        states: Vec<(u64, PendingBackgroundIndexFinalize)>,
    ) {
        self.routing.background_finalizations = states;
    }

    pub(super) fn take_background_finalizations(
        &mut self,
    ) -> Vec<(u64, PendingBackgroundIndexFinalize)> {
        std::mem::take(&mut self.routing.background_finalizations)
    }

    pub(super) fn set_background_finalize_scratch(
        &mut self,
        scratch: Vec<BackgroundIndexFinalizeScratch>,
    ) {
        self.routing.background_finalize_scratch = scratch;
    }

    pub(super) fn take_background_finalize_scratch(
        &mut self,
    ) -> Vec<BackgroundIndexFinalizeScratch> {
        std::mem::take(&mut self.routing.background_finalize_scratch)
    }

    pub(super) fn set_background_filter_scratch(
        &mut self,
        scratch: Vec<BackgroundIndexFilterScratch>,
    ) {
        self.routing.background_filter_scratch = scratch;
    }

    pub(super) fn take_background_filter_scratch(&mut self) -> Vec<BackgroundIndexFilterScratch> {
        std::mem::take(&mut self.routing.background_filter_scratch)
    }

    pub(super) fn set_mailboxes(
        &mut self,
        mailboxes: Vec<(u64, Arc<super::index_mailbox::IndexResponseMailbox>)>,
    ) {
        self.routing.mailboxes = mailboxes;
    }

    pub(super) fn take_mailboxes(
        &mut self,
    ) -> Vec<(u64, Arc<super::index_mailbox::IndexResponseMailbox>)> {
        std::mem::take(&mut self.routing.mailboxes)
    }

    pub(super) fn mailbox_handles(&self) -> Vec<Arc<super::index_mailbox::IndexResponseMailbox>> {
        self.routing
            .mailboxes
            .iter()
            .map(|(_, mailbox)| Arc::clone(mailbox))
            .collect()
    }

    pub(super) fn set_stale_index_entries(&mut self, entries: Vec<IndexEntry>) {
        self.routing.stale_index_entries = entries;
    }
}

impl RetiredActiveResources {
    pub(super) fn is_empty(&self) -> bool {
        self.committed.is_empty()
    }
}

impl RetiredRoutingPayload {
    fn is_empty(&self) -> bool {
        self.background_states.capacity() == 0
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
    }
}

enum ReclaimPayload {
    Tab(Box<RetiredTabResources>),
    TabBatch(Vec<RetiredTabResources>),
    Active(RetiredActiveResources),
    IndexBuild(Box<RetiredIndexBuildResources>),
}

fn release_reclaimer_slot(available_slots: &AtomicUsize) {
    let previous = available_slots.fetch_add(1, Ordering::AcqRel);
    debug_assert!(previous < TAB_RESOURCE_RECLAIMER_CAPACITY);
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
                            .apply_resource_transition(TabResourceTransition::Cancel);
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
            build: self.shell.indexing.build.take_reclaimable(),
            routing: RetiredRoutingPayload::default(),
        }
    }

    pub(super) fn restore_active_index_build_resources(
        &mut self,
        resources: RetiredIndexBuildResources,
    ) {
        self.shell
            .indexing
            .build
            .restore_reclaimable(resources.build);
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
            committed: std::mem::take(&mut self.shell.runtime.committed),
        }
    }

    pub(super) fn restore_active_committed_resources(&mut self, resources: RetiredActiveResources) {
        self.shell.runtime.committed = resources.committed;
    }
}

impl AppTabState {
    pub(super) fn take_index_build_resources(&mut self) -> RetiredIndexBuildResources {
        RetiredIndexBuildResources {
            #[cfg(test)]
            _drop_probe: ReclaimDropProbe::capture(),
            build: self.index_state.build.take_reclaimable(),
            routing: RetiredRoutingPayload::default(),
        }
    }

    pub(super) fn restore_index_build_resources(&mut self, resources: RetiredIndexBuildResources) {
        self.index_state.build.restore_reclaimable(resources.build);
    }

    pub(super) fn take_committed_resources(&mut self) -> RetiredActiveResources {
        self.result_state.evicted_selected_path = self
            .result_state
            .committed
            .current_row
            .and_then(|row| self.result_state.committed.results.get(row))
            .map(|(path, _)| path.clone())
            .or_else(|| self.result_state.evicted_selected_path.clone());
        self.index_state
            .apply_resource_transition(TabResourceTransition::SnapshotRemoved);
        RetiredActiveResources {
            committed: std::mem::take(&mut self.result_state.committed),
        }
    }

    pub(super) fn restore_committed_resources(&mut self, resources: RetiredActiveResources) {
        self.index_state
            .apply_resource_transition(TabResourceTransition::SnapshotRestored);
        self.result_state.committed = resources.committed;
    }
}

impl AppTabState {
    pub(super) fn heavy_resource_weight(&self) -> usize {
        self.index_state
            .build
            .heavy_resource_weight()
            .saturating_add(self.result_state.committed.heavy_resource_weight())
    }

    pub(super) fn take_heavy_resources(&mut self) -> RetiredTabResources {
        self.result_state.evicted_selected_path = self
            .result_state
            .committed
            .current_row
            .and_then(|row| self.result_state.committed.results.get(row))
            .map(|(path, _)| path.clone())
            .or_else(|| self.result_state.evicted_selected_path.clone());
        let resources = RetiredTabResources {
            #[cfg(test)]
            _drop_probe: ReclaimDropProbe::capture(),
            control: TabHeavyControlPayload {
                resource_state: self.index_state.resource_state(),
                build_reclaim_pending: self.index_state.build_reclaim_pending,
                build_reclaim_request_id: self.index_state.build_reclaim_request_id,
                pending_index_entries_request_id: self
                    .index_state
                    .pending_index_entries_request_id
                    .take(),
                kind_resolution_epoch: self.index_state.kind_resolution_epoch,
                kind_resolution_in_progress: self.index_state.kind_resolution_in_progress,
                results_compacted: self.result_state.results_compacted,
            },
            build: self.index_state.build.take_reclaimable(),
            committed: std::mem::take(&mut self.result_state.committed),
        };
        self.index_state
            .apply_resource_transition(TabResourceTransition::Evict);
        self.index_state.build_reclaim_pending = false;
        self.index_state.build_reclaim_request_id = None;
        self.index_state.clear_kind_resolution_state();
        self.result_state.committed.total_match_count = 0;
        self.result_state.committed.current_row = None;
        self.result_state.results_compacted = false;
        resources
    }

    pub(super) fn restore_heavy_resources(&mut self, resources: RetiredTabResources) {
        let RetiredTabResources {
            control,
            build,
            committed,
            ..
        } = resources;
        self.index_state
            .apply_resource_transition(TabResourceTransition::ReclaimFullRollback(
                control.resource_state,
            ));
        self.index_state.build_reclaim_pending = control.build_reclaim_pending;
        self.index_state.build_reclaim_request_id = control.build_reclaim_request_id;
        self.index_state.pending_index_entries_request_id =
            control.pending_index_entries_request_id;
        self.index_state.kind_resolution_epoch = control.kind_resolution_epoch;
        self.index_state.kind_resolution_in_progress = control.kind_resolution_in_progress;
        self.result_state.results_compacted = control.results_compacted;
        self.index_state.build.restore_reclaimable(build);
        self.result_state.committed = committed;
    }
}

pub(super) struct TabResourceReclaimer {
    tx: Option<SyncSender<ReclaimPayload>>,
    pending: Arc<AtomicUsize>,
    available_slots: Arc<AtomicUsize>,
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
        let available_slots = Arc::new(AtomicUsize::new(TAB_RESOURCE_RECLAIMER_CAPACITY));
        let worker_available_slots = Arc::clone(&available_slots);
        let handle = thread::Builder::new()
            .name("flistwalker-tab-reclaimer".to_string())
            .spawn(move || {
                while let Ok(resources) = rx.recv() {
                    release_reclaimer_slot(&worker_available_slots);
                    match resources {
                        ReclaimPayload::Tab(resources) => drop(resources),
                        ReclaimPayload::TabBatch(resources) => drop(resources),
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
                available_slots,
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
            available_slots: Arc::new(AtomicUsize::new(TAB_RESOURCE_RECLAIMER_CAPACITY)),
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
            Err(ReclaimPayload::TabBatch(_))
            | Err(ReclaimPayload::Active(_))
            | Err(ReclaimPayload::IndexBuild(_)) => unreachable!("tab payload changed variant"),
        }
    }

    pub(super) fn try_reserve_slot(&self) -> bool {
        self.try_acquire_slot()
    }

    pub(super) fn release_reserved_slot(&self) {
        release_reclaimer_slot(&self.available_slots);
    }

    pub(super) fn try_retire_reserved_tabs(
        &self,
        resources: Vec<RetiredTabResources>,
    ) -> Result<(), Vec<RetiredTabResources>> {
        if resources.is_empty() {
            self.release_reserved_slot();
            return Ok(());
        }
        match self.try_send_with_acquired_slot(ReclaimPayload::TabBatch(resources)) {
            Ok(()) => Ok(()),
            Err(ReclaimPayload::TabBatch(resources)) => Err(resources),
            Err(ReclaimPayload::Tab(_))
            | Err(ReclaimPayload::Active(_))
            | Err(ReclaimPayload::IndexBuild(_)) => {
                unreachable!("tab batch payload changed variant")
            }
        }
    }

    pub(super) fn try_retire_active(
        &self,
        resources: RetiredActiveResources,
    ) -> Result<(), RetiredActiveResources> {
        match self.try_send(ReclaimPayload::Active(resources)) {
            Ok(()) => Ok(()),
            Err(ReclaimPayload::Active(resources)) => Err(resources),
            Err(ReclaimPayload::Tab(_) | ReclaimPayload::TabBatch(_)) => {
                unreachable!("active payload changed variant")
            }
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
            Err(
                ReclaimPayload::Tab(_) | ReclaimPayload::TabBatch(_) | ReclaimPayload::Active(_),
            ) => {
                unreachable!("index build payload changed variant")
            }
        }
    }

    fn try_send(&self, resources: ReclaimPayload) -> Result<(), ReclaimPayload> {
        if !self.try_acquire_slot() {
            return Err(resources);
        }
        self.try_send_with_acquired_slot(resources)
    }

    fn try_acquire_slot(&self) -> bool {
        let mut available = self.available_slots.load(Ordering::Acquire);
        loop {
            if available == 0 {
                return false;
            }
            match self.available_slots.compare_exchange_weak(
                available,
                available - 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(current) => available = current,
            }
        }
    }

    fn try_send_with_acquired_slot(&self, resources: ReclaimPayload) -> Result<(), ReclaimPayload> {
        let Some(tx) = self.tx.as_ref() else {
            release_reclaimer_slot(&self.available_slots);
            return Err(resources);
        };
        self.pending.fetch_add(1, Ordering::AcqRel);
        match tx.try_send(resources) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(resources) | TrySendError::Disconnected(resources)) => {
                self.pending.fetch_sub(1, Ordering::AcqRel);
                release_reclaimer_slot(&self.available_slots);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tc_207_drop_observer_ignores_payloads_captured_by_parallel_test_threads() {
        let _observer_guard = lock_reclaim_drop_observer_for_test();
        let (drop_tx, drop_rx) = mpsc::channel();
        set_reclaim_drop_observer(Some(drop_tx));

        let foreign_probe = thread::spawn(ReclaimDropProbe::capture)
            .join()
            .expect("capture foreign probe");
        drop(foreign_probe);
        assert!(drop_rx.try_recv().is_err());

        let local_probe = ReclaimDropProbe::capture();
        thread::Builder::new()
            .name("flistwalker-tab-reclaimer".to_string())
            .spawn(move || drop(local_probe))
            .expect("spawn probe drop")
            .join()
            .expect("join probe drop");
        assert_eq!(
            drop_rx.recv_timeout(Duration::from_millis(250)).as_deref(),
            Ok("flistwalker-tab-reclaimer")
        );

        set_reclaim_drop_observer(None);
    }
}

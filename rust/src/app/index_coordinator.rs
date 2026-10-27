use super::index_mailbox::IndexResponseMailbox;
use super::tab_resources::RetiredIndexBuildResources;
use super::tab_state::{TabBuildPayload, TabResourceState, TabResourceTransition};
use super::worker::channel::BoundedSender;
use super::{
    AppTabState, BackgroundIndexState, FlistWalkerApp, IndexRequest, IndexResponse, IndexSource,
    KindResolveRequest, PendingActiveIndexFinish, PendingBackgroundIndexFinalize,
    PendingIndexRefreshMode,
};
use crate::entry::EntryKind;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum IndexResponseRoute {
    Active,
    Background(u64),
    Stale,
}

pub(super) const BACKGROUND_FINALIZATION_CAPACITY: usize = 2;

pub(super) struct BackgroundIndexFinalizationSlots {
    slots: [Option<(u64, PendingBackgroundIndexFinalize)>; BACKGROUND_FINALIZATION_CAPACITY],
}

impl Default for BackgroundIndexFinalizationSlots {
    fn default() -> Self {
        Self {
            slots: std::array::from_fn(|_| None),
        }
    }
}

impl BackgroundIndexFinalizationSlots {
    pub(super) fn contains_key(&self, request_id: &u64) -> bool {
        self.get(request_id).is_some()
    }

    pub(super) fn get(&self, request_id: &u64) -> Option<&PendingBackgroundIndexFinalize> {
        self.slots.iter().find_map(|slot| {
            slot.as_ref()
                .and_then(|(id, state)| (id == request_id).then_some(state))
        })
    }

    pub(super) fn get_mut(
        &mut self,
        request_id: &u64,
    ) -> Option<&mut PendingBackgroundIndexFinalize> {
        self.slots.iter_mut().find_map(|slot| {
            slot.as_mut()
                .and_then(|(id, state)| (id == request_id).then_some(state))
        })
    }

    pub(super) fn has_capacity_for(&self, request_id: u64) -> bool {
        self.contains_key(&request_id) || self.slots.iter().any(Option::is_none)
    }

    pub(super) fn is_full(&self) -> bool {
        self.slots.iter().all(Option::is_some)
    }

    pub(super) fn insert(&mut self, request_id: u64, state: PendingBackgroundIndexFinalize) {
        if let Some(slot) = self
            .slots
            .iter_mut()
            .find(|slot| slot.as_ref().is_some_and(|(id, _)| *id == request_id))
        {
            *slot = Some((request_id, state));
            return;
        }
        let slot = self
            .slots
            .iter_mut()
            .find(|slot| slot.is_none())
            .expect("background finalization slots exceed index worker capacity");
        *slot = Some((request_id, state));
    }

    pub(super) fn remove(&mut self, request_id: &u64) -> Option<PendingBackgroundIndexFinalize> {
        let slot = self
            .slots
            .iter_mut()
            .find(|slot| slot.as_ref().is_some_and(|(id, _)| id == request_id))?;
        slot.take().map(|(_, state)| state)
    }

    pub(super) fn keys(&self) -> impl Iterator<Item = &u64> {
        self.slots
            .iter()
            .filter_map(|slot| slot.as_ref().map(|(request_id, _)| request_id))
    }
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub(super) struct AuxPerfObservation {
    pub(super) flow: &'static str,
    pub(super) request_id: u64,
    pub(super) tab_id: u64,
    pub(super) epoch: u64,
    pub(super) path: Option<PathBuf>,
    pub(super) count: usize,
    pub(super) dispatched_at: Instant,
    pub(super) delivered_at: Option<Instant>,
    pub(super) successful: bool,
    pub(super) route: Option<&'static str>,
    pub(super) received_kind_epoch: Option<u64>,
}
#[cfg(test)]
#[derive(Clone, Debug)]
pub(super) struct SearchDispatchBinding {
    pub(super) request_id: u64,
    pub(super) tab_id: u64,
    pub(super) root: PathBuf,
    pub(super) query: String,
    pub(super) candidates: usize,
    pub(super) candidate_ptr: usize,
    pub(super) sort_mode: super::ResultSortMode,
    pub(super) sort_scope: super::ResultSortScope,
    pub(super) epoch: u64,
    pub(super) at: Instant,
}

#[cfg(test)]
pub(super) struct IndexPerfAllocation {
    pub(super) id: u64,
    pub(super) tab: Option<u64>,
    pub(super) at: Instant,
    pub(super) observation: super::index_mailbox::IndexPerfHandle,
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub(super) struct IndexPerfWarmRemoval {
    pub(super) at: Instant,
    pub(super) removed_request_id: u64,
    pub(super) previous_warm_tab: u64,
    pub(super) replacement_warm_tab: Option<u64>,
    pub(super) route_tab: u64,
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub(super) struct IndexPerfPreemption {
    pub(super) at: Instant,
    pub(super) victim_id: u64,
    pub(super) victim_tab: u64,
    pub(super) prior_latest: Option<u64>,
    pub(super) replacement_id: u64,
    pub(super) active_tab: u64,
    pub(super) warm_tab: Option<u64>,
    pub(super) pending_active_id: Option<u64>,
    pub(super) latest_active_id: Option<u64>,
    pub(super) queued_active_ids: Vec<u64>,
    pub(super) inflight_count: usize,
}

pub(super) struct IndexCoordinator {
    pub(super) tx: BoundedSender<IndexRequest>,
    #[cfg(test)]
    pub(super) rx: Receiver<IndexResponse>,
    pub(super) next_request_id: u64,
    pub(super) pending_request_id: Option<u64>,
    resource_state: TabResourceState,
    pub(super) latest_request_ids: Arc<Mutex<HashMap<u64, u64>>>,
    #[cfg(test)]
    pub(super) perf_observe_requests: bool,
    #[cfg(test)]
    pub(super) perf_observe_history: bool,
    #[cfg(test)]
    pub(super) perf_allocations: Vec<IndexPerfAllocation>,
    #[cfg(test)]
    pub(super) perf_preemptions: Vec<IndexPerfPreemption>,
    #[cfg(test)]
    pub(super) perf_preemption_overflow: bool,
    #[cfg(test)]
    pub(super) perf_warm_removals: Vec<IndexPerfWarmRemoval>,
    #[cfg(test)]
    pub(super) perf_warm_removal_overflow: bool,
    #[cfg(test)]
    pub(super) perf_settled_requests: HashMap<u64, Instant>,
    #[cfg(test)]
    pub(super) perf_released_requests: HashMap<u64, Instant>,
    #[cfg(test)]
    pub(super) perf_observe_aux: bool,
    #[cfg(test)]
    pub(super) perf_aux: Vec<AuxPerfObservation>,
    #[cfg(test)]
    pub(super) perf_search_bindings: Vec<SearchDispatchBinding>,
    pub(super) response_mailboxes: Arc<Mutex<HashMap<u64, Arc<IndexResponseMailbox>>>>,
    pub(super) latest_kind_epochs: Arc<Mutex<HashMap<u64, u64>>>,
    pub(super) pending_queue: VecDeque<IndexRequest>,
    pub(super) inflight_requests: HashSet<u64>,
    pub(super) superseded_request_ids: HashSet<u64>,
    pub(super) in_progress: bool,
    pub(super) build: TabBuildPayload,
    pub(super) pending_entries_request_id: Option<u64>,
    #[cfg(test)]
    pub(super) deferred_response: Option<IndexResponse>,
    #[cfg(test)]
    pub(super) deferred_non_active_responses: VecDeque<IndexResponse>,
    #[cfg(test)]
    pub(super) mailbox_selection_trace: Vec<u64>,
    pub(super) pending_finish: Option<PendingActiveIndexFinish>,
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
    pub(super) request_tabs: HashMap<u64, u64>,
    pub(super) background_states: HashMap<u64, BackgroundIndexState>,
    pub(super) background_finalizations: BackgroundIndexFinalizationSlots,
    pub(super) warm_tab_id: Option<u64>,
    pub(super) pending_stale_build_reclaim: Option<(Option<u64>, RetiredIndexBuildResources)>,
    pub(super) pending_replace_all: Option<IndexResponse>,
}

#[cfg(test)]
pub(super) struct PerfSearchSortIdentity<'a> {
    pub(super) tab: u64,
    pub(super) epoch: u64,
    pub(super) root: &'a std::path::Path,
    pub(super) query: &'a str,
}

impl IndexCoordinator {
    #[cfg(test)]
    pub(super) fn perf_search_sort_owned(
        &self,
        response: &super::SearchResponse,
        identity: PerfSearchSortIdentity<'_>,
    ) -> bool {
        !response.results.is_empty()
            && response.error.is_none()
            && response.sort_scope == super::ResultSortScope::AllMatches
            && response.sort_mode != super::ResultSortMode::Score
            && self.perf_search_bindings.iter().any(|b| {
                b.request_id == response.request_id
                    && b.tab_id == identity.tab
                    && b.epoch == identity.epoch
                    && b.root == identity.root
                    && b.query == identity.query
                    && b.sort_mode == response.sort_mode
                    && b.sort_scope == response.sort_scope
                    && b.candidates > 0
            })
    }
    #[cfg(test)]
    pub(super) fn perf_aux_dispatch(
        &mut self,
        flow: &'static str,
        request_id: u64,
        tab_id: u64,
        epoch: u64,
        path: Option<PathBuf>,
        count: usize,
    ) {
        // Bound observation storage; no worker/per-entry observer locks. The
        // basic observer leaves this separate, optional response observer off.
        if !self.perf_observe_aux || self.perf_aux.iter().filter(|o| o.flow == flow).count() >= 256
        {
            return;
        }
        self.perf_aux.push(AuxPerfObservation {
            flow,
            request_id,
            tab_id,
            epoch,
            path,
            count,
            dispatched_at: Instant::now(),
            delivered_at: None,
            successful: false,
            route: None,
            received_kind_epoch: None,
        });
    }
    #[cfg(test)]
    pub(super) fn perf_aux_delivered(
        &mut self,
        flow: &'static str,
        request_id: u64,
        tab_id: u64,
        epoch: u64,
        path: Option<&PathBuf>,
        successful: bool,
    ) {
        if !self.perf_observe_aux {
            return;
        }
        if let Some(observation) = self.perf_aux.iter_mut().rev().find(|o| {
            o.flow == flow
                && o.request_id == request_id
                && o.tab_id == tab_id
                && o.epoch == epoch
                && o.path.as_ref() == path
                && o.delivered_at.is_none()
        }) {
            observation.delivered_at = Some(Instant::now());
            observation.successful = successful;
        }
    }
    pub(super) fn queued_request_for_tab_exists(&self, tab_id: u64) -> bool {
        self.pending_queue.iter().any(|req| req.tab_id == tab_id)
    }

    pub(super) fn has_inflight_for_tab(&self, tab_id: u64) -> bool {
        self.inflight_requests.iter().any(|request_id| {
            self.request_tabs
                .get(request_id)
                .is_some_and(|request_tab_id| *request_tab_id == tab_id)
        })
    }

    pub(super) fn pop_next_request(&mut self, active_tab_id: u64) -> Option<IndexRequest> {
        if let Some(pos) = self
            .pending_queue
            .iter()
            .position(|req| req.tab_id == active_tab_id && !self.has_inflight_for_tab(req.tab_id))
        {
            return self.pending_queue.remove(pos);
        }
        if self.background_finalizations.is_full() {
            return None;
        }
        let pos = self
            .pending_queue
            .iter()
            .position(|req| !self.has_inflight_for_tab(req.tab_id))?;
        self.pending_queue.remove(pos)
    }

    pub(super) const fn lifecycle(&self) -> super::TabResourceLifecycle {
        self.resource_state.lifecycle()
    }

    pub(super) const fn committed_snapshot_present(&self) -> bool {
        self.resource_state.committed_snapshot_present()
    }

    #[cfg(test)]
    pub(super) const fn resource_state(&self) -> TabResourceState {
        self.resource_state
    }

    pub(super) fn apply_resource_transition(&mut self, transition: TabResourceTransition) {
        self.resource_state.apply(transition);
    }

    pub(super) fn swap_resource_state(&mut self, state: &mut TabResourceState) {
        std::mem::swap(&mut self.resource_state, state);
    }

    #[cfg(test)]
    pub(super) fn set_resource_state_for_test(&mut self, state: TabResourceState) {
        self.resource_state = state;
    }

    #[cfg(test)]
    pub(super) fn set_lifecycle_for_test(&mut self, lifecycle: super::TabResourceLifecycle) {
        let committed = self.committed_snapshot_present();
        self.resource_state = TabResourceState::new(lifecycle, committed);
    }

    #[cfg(test)]
    pub(super) fn set_committed_snapshot_present_for_test(&mut self, present: bool) {
        self.resource_state = TabResourceState::new(self.lifecycle(), present);
    }

    pub(super) fn new(
        tx: BoundedSender<IndexRequest>,
        rx: Receiver<IndexResponse>,
        latest_request_ids: Arc<Mutex<HashMap<u64, u64>>>,
        response_mailboxes: Arc<Mutex<HashMap<u64, Arc<IndexResponseMailbox>>>>,
        latest_kind_epochs: Arc<Mutex<HashMap<u64, u64>>>,
    ) -> Self {
        #[cfg(not(test))]
        let _ = rx;
        Self {
            tx,
            #[cfg(test)]
            rx,
            next_request_id: 1,
            pending_request_id: None,
            resource_state: TabResourceState::default(),
            latest_request_ids,
            response_mailboxes,
            #[cfg(test)]
            perf_observe_requests: false,
            #[cfg(test)]
            perf_observe_history: false,
            #[cfg(test)]
            perf_allocations: Vec::new(),
            #[cfg(test)]
            perf_preemptions: Vec::new(),
            #[cfg(test)]
            perf_preemption_overflow: false,
            #[cfg(test)]
            perf_warm_removals: Vec::new(),
            #[cfg(test)]
            perf_warm_removal_overflow: false,
            #[cfg(test)]
            perf_settled_requests: HashMap::new(),
            #[cfg(test)]
            perf_released_requests: HashMap::new(),
            #[cfg(test)]
            perf_observe_aux: false,
            #[cfg(test)]
            perf_aux: Vec::new(),
            #[cfg(test)]
            perf_search_bindings: Vec::new(),
            latest_kind_epochs,
            pending_queue: VecDeque::new(),
            inflight_requests: HashSet::new(),
            superseded_request_ids: HashSet::new(),
            in_progress: false,
            build: TabBuildPayload::default(),
            pending_entries_request_id: None,
            #[cfg(test)]
            deferred_response: None,
            #[cfg(test)]
            deferred_non_active_responses: VecDeque::new(),
            #[cfg(test)]
            mailbox_selection_trace: Vec::new(),
            pending_finish: None,
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
            request_tabs: HashMap::new(),
            background_states: HashMap::new(),
            background_finalizations: BackgroundIndexFinalizationSlots::default(),
            warm_tab_id: None,
            pending_stale_build_reclaim: None,
            pending_replace_all: None,
        }
    }

    pub(super) fn clear_for_tab_after_reclaim(&mut self, tab_id: u64) {
        if self.warm_tab_id == Some(tab_id) {
            self.warm_tab_id = None;
        }
        debug_assert_eq!(self.request_ids_for_tab(tab_id), Vec::<u64>::new());
        self.request_tabs.retain(|_, id| *id != tab_id);
        self.pending_queue.retain(|req| req.tab_id != tab_id);
        if let Ok(mut latest) = self.latest_request_ids.lock() {
            latest.remove(&tab_id);
        }
        if let Ok(mut latest) = self.latest_kind_epochs.lock() {
            latest.remove(&tab_id);
        }
        debug_assert!(self
            .background_states
            .keys()
            .all(|request_id| self.request_tabs.contains_key(request_id)));
        debug_assert!(self
            .background_finalizations
            .keys()
            .all(|request_id| self.request_tabs.contains_key(request_id)));
    }

    #[cfg(test)]
    pub(super) fn clear_for_tab(&mut self, tab_id: u64) {
        let request_ids = self.request_ids_for_tab(tab_id);
        for request_id in request_ids {
            self.cleanup_request(request_id);
        }
        self.clear_for_tab_after_reclaim(tab_id);
    }

    pub(super) fn request_ids_for_tab(&self, tab_id: u64) -> Vec<u64> {
        self.request_tabs
            .iter()
            .filter_map(|(request_id, id)| (*id == tab_id).then_some(*request_id))
            .collect()
    }

    pub(super) fn take_mailboxes_for_requests(
        &mut self,
        request_ids: &[u64],
    ) -> Vec<(u64, Arc<IndexResponseMailbox>)> {
        let Ok(mut mailboxes) = self.response_mailboxes.lock() else {
            return Vec::new();
        };
        request_ids
            .iter()
            .filter_map(|request_id| {
                mailboxes
                    .remove(request_id)
                    .map(|mailbox| (*request_id, mailbox))
            })
            .collect()
    }

    pub(super) fn restore_mailboxes(
        &mut self,
        mailboxes_to_restore: Vec<(u64, Arc<IndexResponseMailbox>)>,
    ) {
        if let Ok(mut mailboxes) = self.response_mailboxes.lock() {
            mailboxes.extend(mailboxes_to_restore);
        }
    }

    pub(super) fn take_all_mailboxes_for_shutdown(
        &mut self,
    ) -> Vec<(u64, Arc<IndexResponseMailbox>)> {
        let Ok(mut mailboxes) = self.response_mailboxes.lock() else {
            return Vec::new();
        };
        mailboxes.drain().collect()
    }

    pub(super) fn take_background_states_for_requests(
        &mut self,
        request_ids: &[u64],
    ) -> Vec<(u64, BackgroundIndexState)> {
        request_ids
            .iter()
            .filter_map(|request_id| {
                self.background_states
                    .remove(request_id)
                    .map(|state| (*request_id, state))
            })
            .collect()
    }

    pub(super) fn restore_background_states(&mut self, states: Vec<(u64, BackgroundIndexState)>) {
        self.background_states.extend(states);
    }

    pub(super) fn take_background_finalizations_for_requests(
        &mut self,
        request_ids: &[u64],
    ) -> Vec<(u64, PendingBackgroundIndexFinalize)> {
        request_ids
            .iter()
            .filter_map(|request_id| {
                self.background_finalizations
                    .remove(request_id)
                    .map(|state| (*request_id, state))
            })
            .collect()
    }

    pub(super) fn restore_background_finalizations(
        &mut self,
        states: Vec<(u64, PendingBackgroundIndexFinalize)>,
    ) {
        for (request_id, state) in states {
            self.background_finalizations.insert(request_id, state);
        }
    }

    pub(super) fn allocate_request_id(&mut self, tab_id: Option<u64>) -> u64 {
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1);
        if let Some(tab_id) = tab_id {
            self.request_tabs.insert(request_id, tab_id);
            if let Ok(mut latest) = self.latest_request_ids.lock() {
                if let Some(previous) = latest.insert(tab_id, request_id) {
                    if previous != request_id && self.request_tabs.contains_key(&previous) {
                        self.superseded_request_ids.insert(previous);
                    }
                }
            }
        }
        if let Ok(mut mailboxes) = self.response_mailboxes.lock() {
            let mailbox = Arc::new(IndexResponseMailbox::new());
            #[cfg(test)]
            if self.perf_observe_requests {
                mailbox.enable_perf_observation();
            }
            #[cfg(test)]
            if self.perf_observe_history {
                assert!(
                    self.perf_allocations.len() < 128,
                    "index allocation observer overflow"
                );
                mailbox.enable_perf_observation();
                let observation = mailbox.perf_handle();
                observation
                    .lock()
                    .expect("index observation")
                    .allocation_observed = true;
                super::index_mailbox::register_perf_request(
                    Arc::as_ptr(&self.latest_request_ids) as usize,
                    request_id,
                    &observation,
                );
                self.perf_allocations.push(IndexPerfAllocation {
                    id: request_id,
                    tab: tab_id,
                    at: Instant::now(),
                    observation,
                });
            }
            mailboxes.insert(request_id, mailbox);
        }
        request_id
    }

    pub(super) fn begin_active_refresh(&mut self, request_id: u64, query_non_empty: bool) {
        self.apply_resource_transition(TabResourceTransition::Begin);
        self.pending_request_id = Some(request_id);
        self.in_progress = true;
        self.search_resume_pending = query_non_empty;
        self.search_rerun_pending = false;
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
        debug_assert_eq!(tab.index_state.build.index.entries.capacity(), 0);
        tab.index_state.build.index.source = IndexSource::None;
        debug_assert_eq!(tab.index_state.build.pending_entries.capacity(), 0);
        tab.index_state.pending_index_entries_request_id = None;
        debug_assert_eq!(tab.index_state.build.pending_kind_paths.capacity(), 0);
        debug_assert_eq!(tab.index_state.build.pending_kind_paths_set.capacity(), 0);
        debug_assert_eq!(tab.index_state.build.in_flight_kind_paths.capacity(), 0);
        debug_assert_eq!(tab.index_state.build.resolved_kind_updates.capacity(), 0);
        tab.index_state.kind_resolution_in_progress = false;
        tab.index_state.kind_resolution_epoch =
            tab.index_state.kind_resolution_epoch.saturating_add(1);
        if let Ok(mut latest) = self.latest_kind_epochs.lock() {
            latest.insert(tab.id, tab.index_state.kind_resolution_epoch);
        }
        tab.pending_preview_request_id = None;
        tab.preview_in_progress = false;
        tab.index_state.last_incremental_results_refresh = Instant::now();
        tab.index_state.last_search_snapshot_len = 0;
        tab.notice = notice.to_string();
    }

    pub(super) fn cleanup_request(&mut self, request_id: u64) {
        #[cfg(test)]
        if self.perf_observe_requests {
            self.perf_released_requests
                .entry(request_id)
                .or_insert_with(Instant::now);
        }
        let tab_id = self.request_tabs.remove(&request_id);
        self.background_states.remove(&request_id);
        self.background_finalizations.remove(&request_id);
        self.inflight_requests.remove(&request_id);
        self.superseded_request_ids.remove(&request_id);
        if let Some(tab_id) = tab_id {
            if let Ok(mut latest) = self.latest_request_ids.lock() {
                if latest.get(&tab_id).copied() == Some(request_id) {
                    latest.remove(&tab_id);
                }
            }
        } else if let Ok(mut latest) = self.latest_request_ids.lock() {
            latest.retain(|_, latest_request_id| *latest_request_id != request_id);
        }
        if let Ok(mut mailboxes) = self.response_mailboxes.lock() {
            if let Some(mailbox) = mailboxes.remove(&request_id) {
                mailbox.close();
            }
        }
    }

    pub(super) fn try_recv_mailbox(
        &self,
        request_id: u64,
        allow_terminal: bool,
    ) -> Option<IndexResponse> {
        let mailbox = self
            .response_mailboxes
            .lock()
            .ok()
            .and_then(|mailboxes| mailboxes.get(&request_id).cloned())?;
        mailbox.try_recv_with_terminal_admission(allow_terminal)
    }

    pub(super) fn active_request_id(&self) -> Option<u64> {
        self.pending_request_id
    }

    pub(super) fn active_mailbox_blocked(&self, max_pending_entries: usize) -> bool {
        self.pending_entries_request_id == self.pending_request_id
            && self.build.pending_entries.len() >= max_pending_entries
    }

    pub(super) fn warm_request_id(&self) -> Option<u64> {
        self.warm_tab_id
            .and_then(|tab_id| self.latest_request_for_tab(tab_id))
    }

    pub(super) fn tracked_request_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.request_tabs.keys().copied()
    }

    #[cfg(test)]
    pub(super) fn record_mailbox_selection_for_test(&mut self, request_id: u64) {
        self.mailbox_selection_trace.push(request_id);
    }

    pub(super) fn mailbox_has_terminal(&self, request_id: u64) -> bool {
        self.response_mailboxes
            .lock()
            .ok()
            .and_then(|mailboxes| mailboxes.get(&request_id).cloned())
            .is_some_and(|mailbox| mailbox.has_terminal_response())
    }

    pub(super) fn can_admit_mailbox_terminal(&self, request_id: u64) -> bool {
        if self.pending_request_id == Some(request_id) {
            return !self.background_states.contains_key(&request_id)
                || self.background_finalizations.has_capacity_for(request_id);
        }
        let Some(tab_id) = self.request_tabs.get(&request_id).copied() else {
            return true;
        };
        let is_latest = self
            .latest_request_ids
            .lock()
            .map(|latest| latest.get(&tab_id).copied() == Some(request_id))
            .unwrap_or(false);
        self.superseded_request_ids.contains(&request_id)
            || !is_latest
            || self.background_finalizations.has_capacity_for(request_id)
    }

    pub(super) fn is_superseded_request(&self, request_id: u64) -> bool {
        self.superseded_request_ids.contains(&request_id)
    }

    pub(super) fn release_published_terminal_inflight(&mut self) {
        let completed = self
            .inflight_requests
            .iter()
            .copied()
            .filter(|request_id| self.mailbox_has_terminal(*request_id))
            .collect::<Vec<_>>();
        for request_id in completed {
            self.inflight_requests.remove(&request_id);
        }
    }

    pub(super) fn requeue_terminal(&self, response: IndexResponse) -> bool {
        let request_id = Self::response_request_id(&response);
        let mailbox = self
            .response_mailboxes
            .lock()
            .ok()
            .and_then(|mailboxes| mailboxes.get(&request_id).cloned());
        mailbox.is_some_and(|mailbox| mailbox.try_publish(response).is_ok())
    }

    pub(super) fn latest_request_for_tab(&self, tab_id: u64) -> Option<u64> {
        self.latest_request_ids
            .lock()
            .ok()
            .and_then(|latest| latest.get(&tab_id).copied())
    }

    pub(super) fn replace_warm_tab(&mut self, tab_id: Option<u64>) {
        if self.warm_tab_id == tab_id {
            return;
        }
        if let Some(previous_warm) = self.warm_tab_id {
            if let Ok(mut latest) = self.latest_request_ids.lock() {
                #[cfg(test)]
                let observation = if self.perf_observe_history {
                    latest
                        .get(&previous_warm)
                        .copied()
                        .filter(|id| *id != 0)
                        .and_then(|id| {
                            self.request_tabs
                                .get(&id)
                                .copied()
                                .filter(|tab| *tab == previous_warm)
                                .map(|tab| (id, tab))
                        })
                        .map(|(id, tab)| IndexPerfWarmRemoval {
                            removed_request_id: id,
                            previous_warm_tab: previous_warm,
                            replacement_warm_tab: tab_id,
                            route_tab: tab,
                            at: Instant::now(),
                        })
                } else {
                    None
                };
                if let Some(request_id) = latest
                    .remove(&previous_warm)
                    .filter(|request_id| self.request_tabs.contains_key(request_id))
                {
                    self.superseded_request_ids.insert(request_id);
                }
                #[cfg(test)]
                {
                    drop(latest);
                    if let Some(observation) = observation {
                        if self.perf_warm_removals.len() >= 128 {
                            self.perf_warm_removal_overflow = true;
                            panic!("Warm removal observation capacity exceeded");
                        }
                        self.perf_warm_removals.push(observation);
                    }
                }
            }
        }
        self.warm_tab_id = tab_id;
    }

    pub(super) fn promote_warm_tab(&mut self, tab_id: u64, replacement: Option<u64>) {
        if self.warm_tab_id == Some(tab_id) {
            self.warm_tab_id = replacement;
        }
    }

    pub(super) fn settle_active_terminal_state(&mut self) {
        #[cfg(test)]
        if self.perf_observe_requests {
            if let Some(request_id) = self.pending_request_id {
                self.perf_settled_requests
                    .insert(request_id, Instant::now());
            }
        }
        self.in_progress = false;
        self.pending_request_id = None;
        self.search_resume_pending = false;
        self.search_rerun_pending = false;
        self.pending_entries_request_id = None;
        self.pending_finish = None;
    }

    pub(super) fn clear_active_request_state(&mut self) {
        self.settle_active_terminal_state();
    }

    pub(super) fn route_response(&mut self, request_id: u64) -> IndexResponseRoute {
        if self.superseded_request_ids.contains(&request_id) {
            return IndexResponseRoute::Stale;
        }
        if Some(request_id) == self.pending_request_id {
            return IndexResponseRoute::Active;
        }
        match self.request_tabs.get(&request_id).copied() {
            Some(tab_id) => {
                // Keep request_tabs until terminal accounting settles, but do not route
                // explicitly superseded payloads back into a live background tab.
                let explicitly_stale = self.superseded_request_ids.contains(&request_id)
                    || self
                        .latest_request_ids
                        .lock()
                        .map(|latest| {
                            latest
                                .get(&tab_id)
                                .is_some_and(|latest_id| *latest_id != request_id)
                        })
                        .unwrap_or(false);
                if explicitly_stale {
                    IndexResponseRoute::Stale
                } else {
                    IndexResponseRoute::Background(tab_id)
                }
            }
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

    pub(super) fn is_terminal_response(response: &IndexResponse) -> bool {
        matches!(
            response,
            IndexResponse::Finished { .. }
                | IndexResponse::Failed { .. }
                | IndexResponse::Canceled { .. }
        )
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
        self.shell.indexing.build.pending_kind_paths.clear();
        self.shell.indexing.build.pending_kind_paths_set.clear();
        self.shell.indexing.build.in_flight_kind_paths.clear();
        self.shell.indexing.build.resolved_kind_updates.clear();
        self.shell.indexing.kind_resolution_in_progress = false;
        self.shell.indexing.kind_resolution_epoch =
            self.shell.indexing.kind_resolution_epoch.saturating_add(1);
        if let Some(tab_id) = self.current_tab_id() {
            if let Ok(mut latest) = self.shell.indexing.latest_kind_epochs.lock() {
                latest.insert(tab_id, self.shell.indexing.kind_resolution_epoch);
            }
        }
    }

    /// 表示中または incremental index 中の entry から kind 未解決 path を拾う。
    #[cfg(test)]
    pub(super) fn queue_unknown_kind_paths_for_active_entries(&mut self) {
        if !self.kind_resolution_needed_for_filters() {
            return;
        }
        let use_live_index =
            self.shell.indexing.in_progress && !self.shell.indexing.build.index.entries.is_empty();
        let TabBuildPayload {
            index,
            pending_kind_paths,
            pending_kind_paths_set,
            in_flight_kind_paths,
            entry_kind_cache,
            ..
        } = &mut self.shell.indexing.build;
        let source = if use_live_index {
            index.entries.as_slice()
        } else {
            self.shell.runtime.all_entries.as_ref()
        };
        for entry in source
            .iter()
            .take(super::active_filter::ACTIVE_FILTER_ENTRY_BUDGET)
        {
            if entry.kind.is_some()
                || entry_kind_cache.get(entry.path()).is_some()
                || pending_kind_paths_set.contains(entry.path())
                || in_flight_kind_paths.contains(entry.path())
            {
                continue;
            }
            let path = entry.path.clone();
            pending_kind_paths_set.insert(path.clone());
            pending_kind_paths.push_back(path);
        }
    }

    /// walker 完了後の表示中結果だけから kind 未解決 path を拾う。
    pub(super) fn queue_unknown_kind_paths_for_visible_results(&mut self) {
        let visible_paths = self
            .shell
            .runtime
            .results
            .iter()
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>();
        self.queue_unknown_kind_paths(&visible_paths);
    }

    /// 指定 path 群から kind 未解決のものだけを queue へ積む。
    pub(super) fn queue_unknown_kind_paths(&mut self, source: &[PathBuf]) {
        for path in source {
            if self
                .find_entry_kind(path)
                .is_none_or(|kind| kind.needs_resolution())
            {
                self.queue_kind_resolution(path.clone());
            }
        }
    }

    /// kind 解決キューへ重複なしで path を追加する。
    pub(super) fn queue_kind_resolution(&mut self, path: PathBuf) {
        if self
            .shell
            .indexing
            .build
            .pending_kind_paths_set
            .contains(&path)
            || self
                .shell
                .indexing
                .build
                .in_flight_kind_paths
                .contains(&path)
        {
            return;
        }
        self.shell
            .indexing
            .build
            .pending_kind_paths_set
            .insert(path.clone());
        self.shell.indexing.build.pending_kind_paths.push_back(path);
    }

    /// kind resolver worker へ frame 予算内で request を流す。
    pub(super) fn pump_kind_resolution_requests(&mut self) {
        const MAX_DISPATCH_PER_FRAME: usize = 128;
        let tab_id = self.current_tab_id().unwrap_or_default();
        let epoch = self.shell.indexing.kind_resolution_epoch;
        if let Ok(mut latest) = self.shell.indexing.latest_kind_epochs.lock() {
            latest.insert(tab_id, epoch);
        }
        let mut dispatched = 0usize;
        while dispatched < MAX_DISPATCH_PER_FRAME {
            let Some(path) = self.shell.indexing.build.pending_kind_paths.pop_front() else {
                break;
            };
            self.shell
                .indexing
                .build
                .pending_kind_paths_set
                .remove(&path);
            let req = KindResolveRequest {
                tab_id,
                epoch,
                path: path.clone(),
            };
            match self.shell.worker_bus.kind.tx.try_send(req) {
                Ok(()) => {
                    #[cfg(test)]
                    self.shell.indexing.perf_aux_dispatch(
                        "kind",
                        0,
                        tab_id,
                        epoch,
                        Some(path.clone()),
                        1,
                    );
                    super::worker::channel::trace_worker_load(
                        &self.shell.worker_bus.kind.tx,
                        "kind_resolver",
                        "accepted",
                        super::worker::channel::WorkerTraceContext {
                            worker_id: "ui-dispatch",
                            request_id: None,
                            tab_id: Some(tab_id),
                            epoch: Some(epoch),
                            outcome: "accepted",
                        },
                    );
                    self.shell.indexing.build.in_flight_kind_paths.insert(path);
                    dispatched = dispatched.saturating_add(1);
                }
                Err(std::sync::mpsc::TrySendError::Full(req)) => {
                    super::worker::channel::trace_worker_load(
                        &self.shell.worker_bus.kind.tx,
                        "kind_resolver",
                        "full",
                        super::worker::channel::WorkerTraceContext {
                            worker_id: "ui-dispatch",
                            request_id: None,
                            tab_id: Some(req.tab_id),
                            epoch: Some(req.epoch),
                            outcome: "full",
                        },
                    );
                    self.shell
                        .indexing
                        .build
                        .pending_kind_paths_set
                        .insert(req.path.clone());
                    self.shell
                        .indexing
                        .build
                        .pending_kind_paths
                        .push_front(req.path);
                    break;
                }
                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                    super::worker::channel::trace_worker_load(
                        &self.shell.worker_bus.kind.tx,
                        "kind_resolver",
                        "disconnected",
                        super::worker::channel::WorkerTraceContext {
                            worker_id: "ui-dispatch",
                            request_id: None,
                            tab_id: Some(tab_id),
                            epoch: Some(epoch),
                            outcome: "disconnected",
                        },
                    );
                    self.shell.indexing.build.pending_kind_paths.clear();
                    self.shell.indexing.build.pending_kind_paths_set.clear();
                    self.shell.indexing.build.in_flight_kind_paths.clear();
                    self.shell.indexing.kind_resolution_in_progress = false;
                    for tab in &mut self.shell.tabs {
                        tab.index_state.clear_kind_resolution_state();
                    }
                    self.set_notice("Kind resolver worker is unavailable");
                    break;
                }
            }
        }
        self.shell.indexing.kind_resolution_in_progress =
            !self.shell.indexing.build.pending_kind_paths.is_empty()
                || !self.shell.indexing.build.in_flight_kind_paths.is_empty();
    }

    /// kind resolver 応答を吸収し filter/preview を必要最小限で更新する。
    pub(super) fn poll_kind_response(&mut self) {
        const MAX_MESSAGES_PER_FRAME: usize = 512;
        let mut processed = 0usize;
        let mut resolved_any = false;
        let mut resolved_current_row = false;
        let mut resolved_updates: Vec<(PathBuf, EntryKind)> = Vec::new();
        let active_tab_id = self.current_tab_id().unwrap_or_default();

        while let Ok(response) = self.shell.worker_bus.kind.rx.try_recv() {
            if response.tab_id != active_tab_id {
                if let Some(tab_index) = self.find_tab_index_by_id(response.tab_id) {
                    if let Some(tab) = self.shell.tabs.get_mut(tab_index) {
                        if tab.index_state.kind_resolution_epoch == response.epoch {
                            tab.index_state
                                .build
                                .in_flight_kind_paths
                                .remove(&response.path);
                            if let Some(kind) = response.kind {
                                tab.index_state
                                    .build
                                    .entry_kind_cache
                                    .set(response.path.clone(), kind);
                                tab.index_state
                                    .build
                                    .resolved_kind_updates
                                    .push((response.path.clone(), kind));
                            }
                            tab.index_state.refresh_kind_resolution_progress();
                        }
                    }
                }
                processed = processed.saturating_add(1);
                if processed >= MAX_MESSAGES_PER_FRAME {
                    break;
                }
                continue;
            }
            if response.epoch != self.shell.indexing.kind_resolution_epoch {
                continue;
            }
            #[cfg(test)]
            self.shell.indexing.perf_aux_delivered(
                "kind",
                0,
                active_tab_id,
                response.epoch,
                Some(&response.path),
                response.kind.is_some()
                    && self
                        .shell
                        .indexing
                        .build
                        .in_flight_kind_paths
                        .contains(&response.path),
            );
            self.shell
                .indexing
                .build
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
            self.shell
                .indexing
                .build
                .resolved_kind_updates
                .extend(resolved_updates);
        }

        self.shell.indexing.kind_resolution_in_progress =
            !self.shell.indexing.build.pending_kind_paths.is_empty()
                || !self.shell.indexing.build.in_flight_kind_paths.is_empty();

        if resolved_any && (!self.shell.runtime.include_files || !self.shell.runtime.include_dirs) {
            self.request_kind_entry_refilter();
        }
        if resolved_current_row && self.shell.ui.show_preview {
            self.request_preview_for_current();
        }
    }
}

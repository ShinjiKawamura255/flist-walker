//! Tab-owned, budgeted candidate preparation. Publication and retirement are one
//! transaction: a full reclaimer never makes the UI destroy the previous owner.
use super::tab_resources::RetiredIndexBuildResources;
use super::{Entry, FlistWalkerApp};
use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

pub(super) const ACTIVE_FILTER_ENTRY_BUDGET: usize = 512;
const ACTIVE_FILTER_KIND_BACKLOG: usize = 4096;

#[derive(Debug)]
#[cfg_attr(test, derive(Clone))]
pub(super) struct ActiveFilterContinuation {
    tab_id: Option<u64>,
    root: PathBuf,
    request_id: Option<u64>,
    source: Option<Arc<Vec<Entry>>>,
    source_len: usize,
    include_files: bool,
    include_dirs: bool,
    ignore_enabled: bool,
    ignore_case: bool,
    ignore_source: Arc<Vec<String>>,
    kind_epoch: u64,
    kind_revision: usize,
    pub(super) cursor: usize,
    entries: Vec<Entry>,
    incremental: Vec<Entry>,
    ready_entries: Option<Arc<Vec<Entry>>>,
    retired_visible: Option<Arc<Vec<Entry>>>,
    keep_scroll: bool,
    restart: bool,
    results_only: bool,
    discarded_kind_paths: VecDeque<PathBuf>,
    discarded_kind_set: HashSet<PathBuf>,
    discarded_inflight: HashSet<PathBuf>,
}

impl ActiveFilterContinuation {
    pub(super) fn weight(&self) -> usize {
        self.entries
            .capacity()
            .saturating_add(self.incremental.capacity())
            .saturating_add(self.source.as_ref().map_or(0, |v| v.len()))
            .saturating_add(self.ready_entries.as_ref().map_or(0, |v| v.len()))
            .saturating_add(self.retired_visible.as_ref().map_or(0, |v| v.len()))
            .saturating_add(self.discarded_kind_paths.len())
            .saturating_add(self.discarded_kind_set.len())
            .saturating_add(self.discarded_inflight.len())
    }

    fn matches(&self, app: &FlistWalkerApp) -> bool {
        !self.restart
            && self.tab_id == app.current_tab_id()
            && self.root == app.shell.runtime.root
            && self.request_id == app.shell.indexing.pending_request_id
            && self.include_files == app.shell.runtime.include_files
            && self.include_dirs == app.shell.runtime.include_dirs
            && self.ignore_enabled == app.shell.ui.ignore_list_enabled
            && self.ignore_case == app.shell.runtime.ignore_case
            && Arc::ptr_eq(&self.ignore_source, &app.shell.runtime.ignore_list_terms)
            && self.kind_epoch == app.shell.indexing.kind_resolution_epoch
            && (!self.results_only
                || !super::result_policy::needs_search_worker(
                    &app.shell.runtime.query_state.query,
                    app.shell.runtime.result_sort_mode,
                    app.shell.runtime.result_sort_scope,
                ))
            && (self.ready_entries.is_none()
                || self.kind_revision == app.shell.indexing.build.resolved_kind_updates.len())
            && match &self.source {
                Some(source) => Arc::ptr_eq(source, &app.shell.runtime.all_entries),
                None => self.source_len == app.shell.indexing.build.index.entries.len(),
            }
    }
}

impl FlistWalkerApp {
    pub(super) fn request_kind_entry_refilter(&mut self) {
        // Let discovery finish before replaying with the resolved cache. Restarting
        // at every response batch repeatedly rescans the same large prefix.
        if !self.active_entry_filter_pending() {
            self.request_active_entry_filter(true);
        }
    }
    pub(super) fn active_entry_filter_pending(&self) -> bool {
        self.shell.indexing.build.active_filter.is_some()
    }

    pub(super) fn request_active_entry_filter(&mut self, keep_scroll: bool) {
        self.shell.search.clear_active_request_state();
        self.shell.worker_bus.sort.clear_request();
        if let Some(pending) = self.shell.indexing.build.active_filter.as_mut() {
            pending.restart = true;
            pending.keep_scroll = keep_scroll;
            return;
        }
        self.start_active_entry_filter(keep_scroll);
        // Small inputs retain their synchronous contract. Large inputs are only
        // scheduled here, and run once per rendered frame through the poll hook.
        if self
            .shell
            .indexing
            .build
            .active_filter
            .as_ref()
            .is_some_and(|pending| {
                pending.source_len <= ACTIVE_FILTER_ENTRY_BUDGET || pending.ready_entries.is_some()
            })
        {
            self.poll_active_entry_filter();
        }
    }

    fn start_active_entry_filter(&mut self, keep_scroll: bool) {
        let (discarded_kind_paths, discarded_kind_set, discarded_inflight) =
            if !self.kind_resolution_needed_for_filters() {
                self.shell.indexing.kind_resolution_epoch =
                    self.shell.indexing.kind_resolution_epoch.saturating_add(1);
                self.shell.indexing.kind_resolution_in_progress = false;
                if let Some(tab_id) = self.current_tab_id() {
                    if let Ok(mut latest) = self.shell.indexing.latest_kind_epochs.lock() {
                        latest.insert(tab_id, self.shell.indexing.kind_resolution_epoch);
                    }
                }
                (
                    std::mem::take(&mut self.shell.indexing.build.pending_kind_paths),
                    std::mem::take(&mut self.shell.indexing.build.pending_kind_paths_set),
                    std::mem::take(&mut self.shell.indexing.build.in_flight_kind_paths),
                )
            } else {
                (VecDeque::new(), HashSet::new(), HashSet::new())
            };
        let live =
            self.shell.indexing.in_progress && !self.shell.indexing.build.index.entries.is_empty();
        let source = (!live).then(|| Arc::clone(&self.shell.runtime.all_entries));
        let source_len = source.as_ref().map_or_else(
            || self.shell.indexing.build.index.entries.len(),
            |source| source.len(),
        );
        let needs_filter = !self.shell.runtime.include_files
            || !self.shell.runtime.include_dirs
            || (self.shell.ui.ignore_list_enabled
                && !self.shell.runtime.ignore_list_terms.is_empty());
        let results_only = live
            && !needs_filter
            && !super::result_policy::needs_search_worker(
                &self.shell.runtime.query_state.query,
                self.shell.runtime.result_sort_mode,
                self.shell.runtime.result_sort_scope,
            );
        let ready_entries = if !needs_filter && !live {
            source.as_ref().map(Arc::clone)
        } else if results_only {
            Some(Arc::new(Vec::new()))
        } else {
            None
        };
        let capacity = if ready_entries.is_some() {
            0
        } else {
            source_len
        };
        self.shell.indexing.build.active_filter = Some(ActiveFilterContinuation {
            tab_id: self.current_tab_id(),
            root: self.shell.runtime.root.clone(),
            request_id: self.shell.indexing.pending_request_id,
            source,
            source_len,
            include_files: self.shell.runtime.include_files,
            include_dirs: self.shell.runtime.include_dirs,
            ignore_enabled: self.shell.ui.ignore_list_enabled,
            ignore_case: self.shell.runtime.ignore_case,
            ignore_source: Arc::clone(&self.shell.runtime.ignore_list_terms),
            kind_epoch: self.shell.indexing.kind_resolution_epoch,
            kind_revision: self.shell.indexing.build.resolved_kind_updates.len(),
            cursor: 0,
            entries: Vec::with_capacity(capacity),
            incremental: Vec::with_capacity(if live { capacity } else { 0 }),
            ready_entries,
            retired_visible: None,
            keep_scroll,
            restart: false,
            results_only,
            discarded_kind_paths,
            discarded_kind_set,
            discarded_inflight,
        });
    }

    fn retire_active_filter(
        &mut self,
        pending: ActiveFilterContinuation,
    ) -> Result<(), Box<ActiveFilterContinuation>> {
        if pending.weight() <= ACTIVE_FILTER_ENTRY_BUDGET {
            return Ok(());
        }
        self.shell
            .tabs
            .try_retire_index_build_resources(RetiredIndexBuildResources::from_active_filter(
                pending,
            ))
            .map_err(|mut resources| Box::new(resources.take_active_filter()))
    }

    pub(super) fn poll_active_entry_filter(&mut self) {
        let Some(mut pending) = self.shell.indexing.build.active_filter.take() else {
            return;
        };
        if !pending.matches(self) || self.shell.indexing.pending_finish.is_some() {
            let keep_scroll = pending.keep_scroll;
            match self.retire_active_filter(pending) {
                Ok(()) if self.shell.indexing.pending_finish.is_none() => {
                    self.start_active_entry_filter(keep_scroll)
                }
                Ok(()) => {}
                Err(pending) => self.shell.indexing.build.active_filter = Some(*pending),
            }
            return;
        }
        if pending.ready_entries.is_none() {
            let compiled = self.compiled_ignore_terms();
            let end = pending
                .source_len
                .min(pending.cursor.saturating_add(ACTIVE_FILTER_ENTRY_BUDGET));
            while pending.cursor < end {
                let entry = match &pending.source {
                    Some(source) => &source[pending.cursor],
                    None => &self.shell.indexing.build.index.entries[pending.cursor],
                };
                let unknown = self.kind_resolution_needed_for_filters()
                    && entry.kind.is_none_or(|kind| kind.needs_resolution())
                    && self
                        .find_entry_kind(entry.path())
                        .is_none_or(|kind| kind.needs_resolution());
                if unknown
                    && self.shell.indexing.build.pending_kind_paths.len()
                        + self.shell.indexing.build.in_flight_kind_paths.len()
                        >= ACTIVE_FILTER_KIND_BACKLOG
                    && !self
                        .shell
                        .indexing
                        .build
                        .pending_kind_paths_set
                        .contains(entry.path())
                    && !self
                        .shell
                        .indexing
                        .build
                        .in_flight_kind_paths
                        .contains(entry.path())
                {
                    break;
                }
                let visible = self.is_entry_visible_for_current_filter(entry, compiled.as_deref());
                if visible {
                    pending.entries.push(entry.clone());
                    if pending.source.is_none() {
                        pending.incremental.push(entry.clone());
                    }
                }
                let path = unknown.then(|| entry.path.clone());
                if let Some(path) = path {
                    self.queue_kind_resolution(path);
                }
                pending.cursor += 1;
            }
            if pending.cursor < pending.source_len {
                self.shell.indexing.build.active_filter = Some(pending);
                return;
            }
            if pending.kind_revision != self.shell.indexing.build.resolved_kind_updates.len() {
                if !self.shell.indexing.build.pending_kind_paths.is_empty()
                    || !self.shell.indexing.build.in_flight_kind_paths.is_empty()
                {
                    self.shell.indexing.build.active_filter = Some(pending);
                    return;
                }
                pending.restart = true;
                self.shell.indexing.build.active_filter = Some(pending);
                return;
            }
            pending.ready_entries = Some(Arc::new(std::mem::take(&mut pending.entries)));
        }
        let keep_scroll = pending.keep_scroll;
        let results_only = pending.results_only;
        if !results_only {
            pending.retired_visible = Some(
                self.shell.runtime.exchange_visible_entries(
                    pending
                        .ready_entries
                        .take()
                        .expect("prepared visible snapshot"),
                ),
            );
        }
        std::mem::swap(
            &mut pending.incremental,
            &mut self.shell.indexing.build.incremental_filtered_entries,
        );
        match self.retire_active_filter(pending) {
            Ok(()) => {}
            Err(mut pending) => {
                std::mem::swap(
                    &mut pending.incremental,
                    &mut self.shell.indexing.build.incremental_filtered_entries,
                );
                if let Some(previous) = pending.retired_visible.take() {
                    pending.ready_entries =
                        Some(self.shell.runtime.exchange_visible_entries(previous));
                }
                self.shell.indexing.build.active_filter = Some(*pending);
                return;
            }
        }
        self.shell.indexing.search_rerun_pending = false;
        self.shell.indexing.search_resume_pending = false;
        self.shell.indexing.last_incremental_results_refresh = std::time::Instant::now();
        if results_only {
            let source = &self.shell.indexing.build.index.entries;
            let count = source.len();
            let results = source
                .iter()
                .take(self.shell.runtime.limit)
                .map(|entry| (entry.path.clone(), 0.0))
                .collect();
            self.shell.indexing.last_search_snapshot_len = count;
            self.publish_prepared_filter_results(results, count, keep_scroll);
        } else {
            self.shell.indexing.last_search_snapshot_len = self.shell.runtime.entries.len();
            if super::result_policy::needs_search_worker(
                &self.shell.runtime.query_state.query,
                self.shell.runtime.result_sort_mode,
                self.shell.runtime.result_sort_scope,
            ) {
                self.update_results();
            } else {
                let results = self
                    .shell
                    .runtime
                    .entries
                    .iter()
                    .take(self.shell.runtime.limit)
                    .map(|entry| (entry.path.clone(), 0.0))
                    .collect();
                let count = self.shell.runtime.entries.len();
                self.publish_prepared_filter_results(results, count, keep_scroll);
            }
        }
    }

    fn publish_prepared_filter_results(
        &mut self,
        results: Vec<(PathBuf, f64)>,
        count: usize,
        keep_scroll: bool,
    ) {
        if self.shell.runtime.result_sort_mode != super::ResultSortMode::Score {
            self.shell.runtime.replace_base_results(results, false);
            let outcome = if self.shell.runtime.base_results.is_empty() {
                self.apply_results_with_selection_policy(Vec::new(), keep_scroll, false);
                super::result_reducer::ResultSortApplyOutcome::Applied
            } else {
                super::result_reducer::apply_result_sort(self, keep_scroll)
            };
            match outcome {
                super::result_reducer::ResultSortApplyOutcome::Applied => {
                    self.shell.runtime.set_total_match_count(count)
                }
                super::result_reducer::ResultSortApplyOutcome::Pending => {
                    self.shell.worker_bus.sort.pending_total_match_count = Some(count)
                }
                super::result_reducer::ResultSortApplyOutcome::Failed => {}
            }
        } else {
            self.shell.runtime.set_total_match_count(count);
            self.replace_results_snapshot(results, keep_scroll);
        }
    }
}

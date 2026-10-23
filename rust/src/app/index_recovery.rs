use super::tab_state::TabResourceTransition;
use super::{FlistWalkerApp, IndexCoordinator, PendingIndexRefreshMode};
use std::collections::HashSet;

/// Session-only admission policy. A global stop defaults future tabs to paused;
/// explicit Refresh opens only its tab. The UI thread owns these sets.
#[derive(Default)]
pub(super) struct IndexRecoveryState {
    all_stopped: bool,
    stopped: HashSet<u64>,
    resumed: HashSet<u64>,
    retiring: HashSet<u64>,
}

impl IndexCoordinator {
    pub(super) fn index_admitted(&self, tab: u64) -> bool {
        !self.recovery.stopped.contains(&tab)
            && (!self.recovery.all_stopped || self.recovery.resumed.contains(&tab))
    }

    pub(super) fn index_start_admitted(&self, tab: u64) -> bool {
        self.index_admitted(tab) && !self.recovery.retiring.contains(&tab)
    }

    pub(super) fn suspend_index_retry(&mut self, tab: u64) {
        self.recovery.stopped.insert(tab);
        self.recovery.resumed.remove(&tab);
    }

    pub(super) fn forget_index_recovery_tab(&mut self, tab: u64) {
        self.recovery.stopped.remove(&tab);
        self.recovery.resumed.remove(&tab);
        self.recovery.retiring.remove(&tab);
    }
}

impl FlistWalkerApp {
    pub(super) fn stop_current_indexing(&mut self) {
        if let Some(tab) = self.current_tab_id() {
            self.shell.indexing.suspend_index_retry(tab);
            self.cancel_index_generation(tab);
            self.set_notice(
                "Indexing stopped; previous completed results retained. Refresh Index to resume",
            );
        }
    }

    pub(super) fn stop_all_indexing(&mut self) {
        self.shell.indexing.recovery.all_stopped = true;
        self.shell.indexing.recovery.resumed.clear();
        let tabs: Vec<_> = self.shell.tabs.iter().map(|tab| tab.id).collect();
        for tab in tabs {
            self.shell.indexing.suspend_index_retry(tab);
            self.cancel_index_generation(tab);
        }
        self.set_notice("All indexing stopped; previous completed results retained. Refresh Index resumes this tab only");
    }

    pub(super) fn restart_current_index(&mut self) {
        self.restart_current_index_with_mode(PendingIndexRefreshMode::Normal);
    }

    pub(super) fn restart_current_index_with_mode(&mut self, mode: PendingIndexRefreshMode) {
        let Some(tab) = self.current_tab_id() else {
            return;
        };
        self.cancel_index_generation(tab);
        self.shell.indexing.recovery.stopped.remove(&tab);
        self.shell.indexing.recovery.resumed.insert(tab);
        if self.shell.indexing.recovery.retiring.contains(&tab) {
            self.shell.indexing.refresh_after_pending_finish = Some(mode);
        } else {
            self.resume_index_refresh(mode);
        }
    }

    fn cancel_index_generation(&mut self, tab_id: u64) {
        let requests = self.shell.indexing.request_ids_for_tab(tab_id);
        // Invalidate before attempting reclamation: Full must never resurrect a
        // delayed terminal or finalization. Supervisors observe latest=0 and stop.
        if let Ok(mut latest) = self.shell.indexing.latest_request_ids.lock() {
            latest.insert(tab_id, 0);
        }
        self.shell
            .indexing
            .pending_queue
            .retain(|req| req.tab_id != tab_id);
        for request in &requests {
            self.shell.indexing.superseded_request_ids.insert(*request);
            self.shell.indexing.inflight_requests.remove(request);
        }
        let workflow = &mut self.shell.features.filelist.workflow;
        if workflow
            .pending_after_index
            .as_ref()
            .is_some_and(|pending| pending.tab_id == tab_id)
        {
            workflow.pending_after_index = None;
        }
        workflow
            .pending_index_completion_notices
            .retain(|_, pending| pending.tab_id != tab_id);
        self.shell.search.clear_for_tab(tab_id);
        let active = self.current_tab_id() == Some(tab_id);
        if active {
            self.shell.search.clear_active_request_state();
            self.shell.indexing.build_reclaim_request_id = self
                .shell
                .indexing
                .pending_request_id
                .or(self.shell.indexing.build_reclaim_request_id);
            self.shell.indexing.settle_active_terminal_state();
            self.shell.indexing.refresh_after_pending_finish = None;
            self.shell.indexing.root_after_pending_finish = None;
            self.shell
                .indexing
                .apply_resource_transition(TabResourceTransition::Cancel);
        } else if let Some(index) = self.find_tab_index_by_id(tab_id) {
            let tab = self.shell.tabs.get_mut(index).expect("validated tab");
            tab.index_state.build_reclaim_request_id = tab
                .index_state
                .pending_index_request_id
                .or(tab.index_state.build_reclaim_request_id);
            tab.index_state.clear_index_request_state();
            tab.clear_search_request_state();
            tab.index_state.refresh_after_pending_finish = None;
            tab.index_state.root_after_pending_finish = None;
            tab.index_state
                .apply_resource_transition(TabResourceTransition::Cancel);
            tab.notice =
                "Indexing stopped; previous completed results retained. Refresh Index to resume"
                    .into();
        }
        self.shell.indexing.recovery.retiring.insert(tab_id);
        self.retire_canceled_index_generation(tab_id);
    }

    fn retire_canceled_index_generation(&mut self, tab_id: u64) -> bool {
        let retired = if self.current_tab_id() == Some(tab_id) {
            self.try_retire_active_index_build_resources_for_boundary()
        } else if let Some(index) = self.find_tab_index_by_id(tab_id) {
            self.try_retire_tab_index_build_resources_for_boundary(index)
        } else {
            true
        };
        if retired {
            self.shell.indexing.recovery.retiring.remove(&tab_id);
            if self.current_tab_id() == Some(tab_id) {
                if self.shell.indexing.index_admitted(tab_id) {
                    // Boundary retirement clears build_reclaim_pending, so the
                    // ordinary failed-build retry no longer owns this intent.
                    // Consume it here exactly once. Inactive tabs retain their
                    // tab-owned mode for retry_pending_background_index_finish.
                    if let Some(mode) = self.shell.indexing.refresh_after_pending_finish.take() {
                        self.resume_index_refresh(mode);
                    }
                } else {
                    self.update_results();
                }
            }
        }
        retired
    }

    pub(super) fn retry_index_recovery_reclamation(&mut self) {
        let tabs: Vec<_> = self
            .shell
            .indexing
            .recovery
            .retiring
            .iter()
            .copied()
            .collect();
        for tab in tabs {
            self.retire_canceled_index_generation(tab);
        }
    }

    pub(super) fn index_recovery_status(&self) -> Option<String> {
        let tab = self.current_tab_id()?;
        if !self.shell.indexing.index_admitted(tab) {
            return Some(
                "Indexing paused; previous completed results retained — Refresh Index to resume"
                    .into(),
            );
        }
        let request = self.shell.indexing.pending_request_id?;
        let mailbox = self
            .shell
            .indexing
            .response_mailboxes
            .lock()
            .ok()?
            .get(&request)
            .cloned()?;
        let (phase, elapsed) = mailbox.worker_phase()?;
        Some(if elapsed.as_secs() >= 5 {
            format!(
                "Index response pending: {phase} ({}s); Stop or Refresh available",
                elapsed.as_secs()
            )
        } else {
            format!("Index: {phase} ({}s)", elapsed.as_secs())
        })
    }
}

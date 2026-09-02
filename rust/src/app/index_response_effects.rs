use super::index_coordinator::{IndexCoordinator, IndexResponseRoute};
use super::tab_state::TabResourceTransition;
use super::{FlistWalkerApp, IndexResponse, PendingActiveIndexFinish};
use crate::walker_runtime::walker_truncated_notice;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum IndexResponseLoopControl {
    Continue,
    Break,
}

pub(super) struct RoutedIndexResponse {
    pub(super) route: IndexResponseRoute,
    pub(super) response: IndexResponse,
    #[cfg(test)]
    pub(super) active_mailbox_blocked: bool,
    #[cfg(test)]
    pub(super) from_shared_response_queue: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct IndexResponseApplicationEffect {
    pub(super) processed_messages: usize,
    pub(super) index_progress: bool,
    pub(super) priority_routed: bool,
    pub(super) control: IndexResponseLoopControl,
}

impl IndexResponseApplicationEffect {
    const fn continue_after_message(index_progress: bool) -> Self {
        Self {
            processed_messages: 1,
            index_progress,
            priority_routed: false,
            control: IndexResponseLoopControl::Continue,
        }
    }

    const fn break_after_message(processed_messages: usize) -> Self {
        Self {
            processed_messages,
            index_progress: false,
            priority_routed: false,
            control: IndexResponseLoopControl::Break,
        }
    }

    #[cfg(test)]
    const fn priority_routed() -> Self {
        Self {
            processed_messages: 0,
            index_progress: false,
            priority_routed: true,
            control: IndexResponseLoopControl::Continue,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum IndexFrameCompletionEffect {
    DispatchIndexQueue,
}

pub(super) struct IndexResponseApplicationOwner<'a> {
    app: &'a mut FlistWalkerApp,
}

impl<'a> IndexResponseApplicationOwner<'a> {
    pub(super) fn new(app: &'a mut FlistWalkerApp) -> Self {
        Self { app }
    }

    pub(super) fn prepare_frame(&mut self) -> bool {
        if !self.app.retry_pending_stale_build_reclaim() {
            return false;
        }
        if let Some(response) = self.app.shell.indexing.pending_replace_all.take() {
            if !self.app.try_apply_replace_all_response(response) {
                return false;
            }
        }
        self.app.retry_pending_active_root_change();
        self.app.retry_pending_active_index_build_reclaim();
        self.app.retry_pending_background_index_finish();
        self.app.retry_pending_tab_activation();
        self.app
            .shell
            .indexing
            .release_published_terminal_inflight();
        true
    }

    pub(super) fn apply(&mut self, routed: RoutedIndexResponse) -> IndexResponseApplicationEffect {
        let route = routed.route;
        #[cfg(test)]
        let active_mailbox_blocked = routed.active_mailbox_blocked;
        #[cfg(test)]
        let from_shared_response_queue = routed.from_shared_response_queue;
        let response = routed.response;
        let request_id = IndexCoordinator::response_request_id(&response);
        if IndexCoordinator::is_terminal_response(&response) {
            self.app
                .shell
                .indexing
                .inflight_requests
                .remove(&request_id);
        }

        #[cfg(test)]
        let response = match self.defer_injected_response(
            route,
            response,
            from_shared_response_queue,
            active_mailbox_blocked,
        ) {
            Ok(response) => response,
            Err(effect) => return effect,
        };

        if matches!(&response, IndexResponse::ReplaceAll { .. }) {
            let accepted = self.app.try_apply_replace_all_response(response);
            return if accepted {
                IndexResponseApplicationEffect::continue_after_message(false)
            } else {
                IndexResponseApplicationEffect::break_after_message(1)
            };
        }

        match route {
            IndexResponseRoute::Background(tab_id) => {
                if let Some(tab_index) = self.app.find_tab_index_by_id(tab_id) {
                    self.app
                        .handle_background_index_response(tab_index, response);
                } else {
                    self.app
                        .discard_filelist_index_completion_notice(request_id);
                    self.app.shell.indexing.cleanup_request(request_id);
                }
                IndexResponseApplicationEffect::continue_after_message(false)
            }
            IndexResponseRoute::Stale => self.apply_stale(request_id, response),
            IndexResponseRoute::Active => self.apply_active(response),
        }
    }

    fn apply_stale(
        &mut self,
        request_id: u64,
        response: IndexResponse,
    ) -> IndexResponseApplicationEffect {
        self.app
            .discard_filelist_index_completion_notice(request_id);
        let reclaimed = match response {
            IndexResponse::Batch { entries, .. } | IndexResponse::ReplaceAll { entries, .. } => {
                self.app.stage_stale_data_reclaim(entries)
            }
            IndexResponse::Finished { .. }
            | IndexResponse::Failed { .. }
            | IndexResponse::Canceled { .. } => self.app.stage_stale_terminal_reclaim(request_id),
            IndexResponse::Started { .. } | IndexResponse::Truncated { .. } => true,
        };
        if reclaimed {
            IndexResponseApplicationEffect::continue_after_message(false)
        } else {
            IndexResponseApplicationEffect::break_after_message(1)
        }
    }

    fn apply_active(&mut self, response: IndexResponse) -> IndexResponseApplicationEffect {
        match response {
            IndexResponse::Started { source, .. } => {
                self.app.shell.indexing.build.index.source = source;
                self.app.refresh_status_line();
                IndexResponseApplicationEffect::continue_after_message(false)
            }
            IndexResponse::Batch {
                request_id,
                entries,
            } => {
                self.app.queue_index_batch(request_id, entries);
                IndexResponseApplicationEffect::continue_after_message(true)
            }
            IndexResponse::ReplaceAll { .. } => unreachable!("replace-all handled before route"),
            IndexResponse::Finished { request_id, source } => {
                if !self
                    .app
                    .stage_promoted_background_finalization(request_id, source.clone())
                {
                    self.app.shell.indexing.pending_finish =
                        Some(PendingActiveIndexFinish { request_id, source });
                    self.app.shell.indexing.in_progress = false;
                }
                IndexResponseApplicationEffect::break_after_message(0)
            }
            IndexResponse::Failed { request_id, error } => {
                self.app
                    .shell
                    .features
                    .filelist
                    .workflow
                    .pending_after_index = None;
                self.app
                    .discard_filelist_index_completion_notice(request_id);
                self.app.set_notice(format!("Indexing failed: {error}"));
                self.app
                    .shell
                    .indexing
                    .apply_resource_transition(TabResourceTransition::Failure);
                self.app.shell.indexing.settle_active_terminal_state();
                self.app.shell.indexing.build_reclaim_pending = true;
                self.app.shell.indexing.build_reclaim_request_id = Some(request_id);
                self.app.try_retire_active_index_build_resources();
                IndexResponseApplicationEffect::continue_after_message(false)
            }
            IndexResponse::Canceled { request_id } => {
                if let Some((tab_id, root)) = self
                    .app
                    .current_tab_id()
                    .map(|tab_id| (tab_id, self.app.shell.runtime.root.clone()))
                {
                    if let Some(notice) = self
                        .app
                        .take_filelist_index_completion_notice(request_id, tab_id, &root)
                    {
                        self.app.set_notice(notice);
                    }
                }
                self.app
                    .shell
                    .indexing
                    .apply_resource_transition(TabResourceTransition::Cancel);
                self.app.shell.indexing.settle_active_terminal_state();
                self.app.shell.indexing.build_reclaim_pending = true;
                self.app.shell.indexing.build_reclaim_request_id = Some(request_id);
                self.app.try_retire_active_index_build_resources();
                IndexResponseApplicationEffect::continue_after_message(false)
            }
            IndexResponse::Truncated { limit, .. } => {
                self.app.set_notice(walker_truncated_notice(limit));
                IndexResponseApplicationEffect::continue_after_message(false)
            }
        }
    }

    #[cfg(test)]
    fn defer_injected_response(
        &mut self,
        route: IndexResponseRoute,
        response: IndexResponse,
        from_shared_response_queue: bool,
        active_mailbox_blocked: bool,
    ) -> Result<IndexResponse, IndexResponseApplicationEffect> {
        let stale_payload = matches!(route, IndexResponseRoute::Stale)
            && matches!(
                &response,
                IndexResponse::Batch { .. } | IndexResponse::ReplaceAll { .. }
            );
        if from_shared_response_queue
            && self.app.shell.indexing.pending_request_id.is_some()
            && (matches!(route, IndexResponseRoute::Background(_)) || stale_payload)
        {
            self.app
                .shell
                .indexing
                .deferred_non_active_responses
                .push_back(response);
            return Err(IndexResponseApplicationEffect::priority_routed());
        }
        if from_shared_response_queue
            && matches!(route, IndexResponseRoute::Active)
            && matches!(
                &response,
                IndexResponse::Batch { .. } | IndexResponse::ReplaceAll { .. }
            )
            && active_mailbox_blocked
        {
            self.app.shell.indexing.deferred_response = Some(response);
            return Err(IndexResponseApplicationEffect::break_after_message(0));
        }
        Ok(response)
    }

    pub(super) fn complete_frame(
        &mut self,
        frame_start: Instant,
        frame_budget: Duration,
        mut has_index_progress: bool,
        max_index_entries: usize,
        max_post_finish_entries: usize,
    ) -> IndexFrameCompletionEffect {
        if let Some(request_id) = self.app.shell.indexing.pending_request_id {
            let remaining_budget = frame_budget.saturating_sub(frame_start.elapsed());
            let consumed = if remaining_budget.is_zero() {
                self.app.drain_queued_index_entries(request_id, 32)
            } else {
                let max_entries = if self.app.shell.indexing.pending_finish.is_some() {
                    max_post_finish_entries
                } else {
                    max_index_entries
                };
                self.app.drain_queued_index_entries_with_budget(
                    request_id,
                    Instant::now(),
                    remaining_budget,
                    max_entries,
                )
            };
            has_index_progress |= consumed;
        }

        let finished_current_request = self.app.try_finish_active_index_after_pending_drain();
        if !finished_current_request
            && self.app.shell.indexing.pending_finish.is_none()
            && has_index_progress
        {
            if self.app.shell.runtime.query_state.query.trim().is_empty() {
                self.app.apply_incremental_empty_query_results();
            } else {
                self.app.maybe_refresh_incremental_search();
            }
        }
        IndexFrameCompletionEffect::DispatchIndexQueue
    }
}

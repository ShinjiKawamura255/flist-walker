use super::index_coordinator::IndexCoordinator;
use super::IndexResponse;

pub(super) const ACTIVE_MAILBOX_QUOTA: usize = 48;
pub(super) const WARM_MAILBOX_QUOTA: usize = 8;

pub(super) struct ArbitratedIndexResponse {
    pub(super) response: IndexResponse,
    #[cfg(test)]
    pub(super) from_shared_response_queue: bool,
}

#[derive(Default)]
pub(super) struct FrameMailboxArbitrator {
    active_processed: usize,
    warm_processed: usize,
}

impl FrameMailboxArbitrator {
    pub(super) fn try_next(
        &mut self,
        coordinator: &mut IndexCoordinator,
        active_mailbox_blocked: bool,
    ) -> Option<ArbitratedIndexResponse> {
        #[cfg(test)]
        if let Some(response) = Self::try_injected_response(coordinator) {
            return Some(response);
        }

        let active_request_id = coordinator.active_request_id();
        let warm_request_id = coordinator.warm_request_id();

        let selected = if self.active_processed < ACTIVE_MAILBOX_QUOTA && !active_mailbox_blocked {
            active_request_id.and_then(|request_id| {
                Self::try_request_mailbox(coordinator, request_id).map(|response| {
                    self.active_processed = self.active_processed.saturating_add(1);
                    (request_id, response)
                })
            })
        } else {
            None
        }
        .or_else(|| {
            if self.warm_processed >= WARM_MAILBOX_QUOTA || warm_request_id == active_request_id {
                return None;
            }
            warm_request_id.and_then(|request_id| {
                Self::try_request_mailbox(coordinator, request_id).map(|response| {
                    self.warm_processed = self.warm_processed.saturating_add(1);
                    (request_id, response)
                })
            })
        })
        .or_else(|| {
            let mut remaining_request_ids = coordinator
                .tracked_request_ids()
                .filter(|request_id| {
                    Some(*request_id) != active_request_id && Some(*request_id) != warm_request_id
                })
                .collect::<Vec<_>>();
            remaining_request_ids.sort_unstable();
            remaining_request_ids.into_iter().find_map(|request_id| {
                Self::try_request_mailbox(coordinator, request_id)
                    .map(|response| (request_id, response))
            })
        })?;

        #[cfg(test)]
        coordinator.record_mailbox_selection_for_test(selected.0);
        Some(ArbitratedIndexResponse {
            response: selected.1,
            #[cfg(test)]
            from_shared_response_queue: false,
        })
    }

    fn try_request_mailbox(
        coordinator: &IndexCoordinator,
        request_id: u64,
    ) -> Option<IndexResponse> {
        let allow_terminal = coordinator.can_admit_mailbox_terminal(request_id);
        coordinator.try_recv_mailbox(request_id, allow_terminal)
    }

    #[cfg(test)]
    fn try_injected_response(
        coordinator: &mut IndexCoordinator,
    ) -> Option<ArbitratedIndexResponse> {
        if let Some(response) = coordinator.deferred_response.take() {
            return Some(ArbitratedIndexResponse {
                response,
                from_shared_response_queue: false,
            });
        }
        if let Ok(response) = coordinator.rx.try_recv() {
            return Some(ArbitratedIndexResponse {
                response,
                from_shared_response_queue: true,
            });
        }
        coordinator
            .deferred_non_active_responses
            .pop_front()
            .map(|response| ArbitratedIndexResponse {
                response,
                from_shared_response_queue: false,
            })
    }
}

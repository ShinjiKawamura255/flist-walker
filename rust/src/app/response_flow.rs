use super::{
    result_reducer, ActionResponse, FlistWalkerApp, PreviewRequest, PreviewResponse,
    SortMetadataResponse,
};
use std::sync::atomic::Ordering;
use std::sync::mpsc::TryRecvError;

impl FlistWalkerApp {
    pub(super) fn bind_preview_request_to_tab(&mut self, request_id: u64, tab_id: u64) {
        self.shell.tabs.bind_preview_request(request_id, tab_id);
    }

    pub(super) fn bind_preview_request_to_current_tab(&mut self, request_id: u64) {
        let Some(tab_id) = self.current_tab_id() else {
            return;
        };
        self.bind_preview_request_to_tab(request_id, tab_id);
    }

    pub(super) fn take_preview_request_tab(&mut self, request_id: u64) -> Option<u64> {
        self.shell.tabs.take_preview_request_tab(request_id)
    }

    pub(super) fn queue_preview_request(&mut self, request: PreviewRequest) -> bool {
        self.flush_parked_preview_requests();
        if self.parked_preview_request.is_some() {
            assert!(
                request.document.is_none(),
                "More admission must wait for preview retirement"
            );
            self.shell
                .worker_bus
                .preview
                .freshness
                .store(request.request_id, std::sync::atomic::Ordering::Release);
            if let Some(superseded) = self.deferred_latest_preview_request.replace(request) {
                self.retire_superseded_preview_request(superseded);
            }
            return true;
        }
        match self.shell.worker_bus.preview.queue_request(request) {
            Ok(Some(superseded)) => {
                self.retire_superseded_preview_request(superseded);
                true
            }
            Ok(None) => true,
            Err(request) => {
                self.retire_superseded_preview_request(request);
                false
            }
        }
    }

    fn retire_superseded_preview_request(&mut self, mut request: PreviewRequest) {
        if let Some(tab_id) = self.take_preview_request_tab(request.request_id) {
            if let Some(index) = self.find_tab_index_by_id(tab_id) {
                if let Some(tab) = self.shell.tabs.get_mut(index) {
                    if tab.pending_preview_request_id == Some(request.request_id) {
                        tab.clear_preview_request_state();
                        tab.mark_preview_reload_pending();
                    }
                }
            }
        }
        if let Some(document) = request.document.take() {
            if let Err(document) = self
                .shell
                .runtime
                .try_retire_external_preview_document(document)
            {
                request.document = Some(document);
                assert!(
                    self.parked_preview_request.is_none(),
                    "only one preview retirement may park"
                );
                self.parked_preview_request = Some(request);
            }
        }
    }

    fn flush_parked_preview_requests(&mut self) {
        if let Some(mut request) = self.parked_preview_request.take() {
            if let Some(document) = request.document.take() {
                if let Err(document) = self
                    .shell
                    .runtime
                    .try_retire_external_preview_document(document)
                {
                    request.document = Some(document);
                    self.parked_preview_request = Some(request);
                    return;
                }
            }
        }
        if let Some(request) = self.deferred_latest_preview_request.take() {
            let _ = self.queue_preview_request(request);
        }
    }

    pub(super) fn clear_response_routing_for_tab(&mut self, tab_id: u64) {
        let action_request_ids = self.shell.tabs.clear_response_routing_for_tab(tab_id);
        for request_id in action_request_ids {
            self.shell.worker_bus.action.invalidate_request(request_id);
        }
    }

    #[cfg(test)]
    pub(super) fn preview_request_tab(&self, request_id: u64) -> Option<u64> {
        self.shell.tabs.preview_request_tab(request_id)
    }

    pub(super) fn bind_action_request_to_tab(&mut self, request_id: u64, tab_id: u64) {
        self.shell.tabs.bind_action_request(request_id, tab_id);
    }

    pub(super) fn bind_action_request_to_current_tab(&mut self, request_id: u64) {
        let Some(tab_id) = self.current_tab_id() else {
            return;
        };
        self.bind_action_request_to_tab(request_id, tab_id);
    }

    pub(super) fn take_action_request_tab(&mut self, request_id: u64) -> Option<u64> {
        self.shell.tabs.take_action_request_tab(request_id)
    }

    pub(super) fn clear_all_action_request_state(&mut self) {
        self.shell.worker_bus.action.invalidate_all();
        self.shell.worker_bus.action.clear_request();
        self.shell.tabs.clear_action_request_routing();
        for tab in &mut self.shell.tabs {
            tab.clear_action_request_state();
        }
    }

    pub(super) fn clear_current_tab_action_request_state(&mut self) {
        let Some(tab_id) = self.current_tab_id() else {
            self.shell.worker_bus.action.clear_request();
            return;
        };
        for request_id in self
            .shell
            .tabs
            .clear_action_response_routing_for_tab(tab_id)
        {
            self.shell.worker_bus.action.invalidate_request(request_id);
        }
        self.shell.worker_bus.action.clear_request();
        if let Some(tab) = self.shell.tabs.iter_mut().find(|tab| tab.id == tab_id) {
            tab.clear_action_request_state();
        }
    }

    pub(super) fn bind_sort_request_to_tab(&mut self, request_id: u64, tab_id: u64) {
        self.shell.tabs.bind_sort_request(request_id, tab_id);
    }

    pub(super) fn bind_sort_request_to_current_tab(&mut self, request_id: u64) {
        let Some(tab_id) = self.current_tab_id() else {
            return;
        };
        self.bind_sort_request_to_tab(request_id, tab_id);
    }

    pub(super) fn take_sort_request_tab(&mut self, request_id: u64) -> Option<u64> {
        self.shell.tabs.take_sort_request_tab(request_id)
    }

    #[cfg(test)]
    pub(super) fn action_request_tab(&self, request_id: u64) -> Option<u64> {
        self.shell.tabs.action_request_tab(request_id)
    }

    #[cfg(test)]
    pub(super) fn sort_request_tab(&self, request_id: u64) -> Option<u64> {
        self.shell.tabs.sort_request_tab(request_id)
    }

    /// action/preview/sort の応答を一括で処理する。
    pub(super) fn poll_routed_worker_responses(&mut self) {
        self.poll_action_response();
        self.poll_sort_response();
        self.poll_preview_response();
    }

    /// action worker の応答を現在 tab または背景 tab に反映する。
    pub(super) fn poll_action_response(&mut self) {
        while let Ok(response) = self.shell.worker_bus.action.rx.try_recv() {
            self.shell
                .worker_bus
                .action
                .finish_request(response.request_id);
            if self.apply_active_action_response(&response) {
                continue;
            }
            self.apply_background_action_response(response);
        }
    }

    pub(super) fn apply_background_action_response(&mut self, response: ActionResponse) {
        let Some(tab_id) = self.take_action_request_tab(response.request_id) else {
            return;
        };
        let Some(tab_index) = self.find_tab_index_by_id(tab_id) else {
            return;
        };
        if tab_index == self.shell.tabs.active_tab_index() {
            return;
        }
        let Some(tab) = self.shell.tabs.get_mut(tab_index) else {
            return;
        };
        if Some(response.request_id) != tab.pending_action_request_id {
            return;
        }
        tab.pending_action_request_id = None;
        tab.action_in_progress = false;
        tab.notice = response.notice;
    }

    pub(super) fn apply_active_action_response(&mut self, response: &ActionResponse) -> bool {
        if Some(response.request_id) != self.shell.worker_bus.action.pending_request_id {
            return false;
        }
        self.take_action_request_tab(response.request_id);
        self.shell.worker_bus.action.pending_request_id = None;
        self.shell.worker_bus.action.in_progress = false;
        self.set_notice(response.notice.clone());
        true
    }

    /// sort worker の応答を cache と tab state へ適用する。
    pub(super) fn poll_sort_response(&mut self) {
        while let Ok(response) = self.shell.worker_bus.sort.rx.try_recv() {
            for (path, metadata) in &response.entries {
                self.cache_sort_metadata(path.clone(), *metadata);
            }

            if self.apply_active_sort_response(&response) {
                continue;
            }
            self.apply_background_sort_response(response);
        }
    }

    pub(super) fn apply_background_sort_response(&mut self, response: SortMetadataResponse) {
        result_reducer::apply_background_sort_response(self, response);
    }

    pub(super) fn apply_active_sort_response(&mut self, response: &SortMetadataResponse) -> bool {
        result_reducer::apply_active_sort_response(self, response)
    }

    pub(super) fn poll_preview_response(&mut self) {
        self.shell.runtime.retire_stale_preview_document();
        self.flush_parked_preview_requests();
        if self.parked_preview_request.is_none() && self.deferred_preview_response.is_none() {
            if let Some((tab_id, path)) = self.deferred_more_intent.take() {
                if self.current_tab_id() == Some(tab_id)
                    && self
                        .paged_preview_for_current()
                        .is_some_and(|document| document.header.path == path)
                {
                    self.paged_preview_view.busy = false;
                    self.request_paged_preview_more();
                }
            }
        }
        if let Some(response) = self.deferred_preview_response.take() {
            if !self.apply_active_preview_response(&response) {
                self.apply_background_preview_response(response);
            }
            if self.deferred_preview_response.is_some() {
                return;
            }
        }
        loop {
            match self.shell.worker_bus.preview.rx.try_recv() {
                Ok(response) => {
                    if !self.apply_active_preview_response(&response) {
                        self.apply_background_preview_response(response);
                    }
                    if self.deferred_preview_response.is_some() {
                        break;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.fail_preview_worker();
                    break;
                }
            }
        }
    }

    pub(super) fn fail_preview_worker(&mut self) {
        let had_pending = self.shell.worker_bus.preview.in_progress
            || self
                .shell
                .worker_bus
                .preview
                .worker_inflight_request_id
                .is_some()
            || self.shell.worker_bus.preview.latest_request.is_some()
            || self.paged_preview_view.busy
            || self.shell.tabs.iter().any(|tab| tab.preview_in_progress);
        if !had_pending {
            return;
        }
        let has_committed_document = self.paged_preview_for_current().is_some();
        self.shell
            .worker_bus
            .preview
            .freshness
            .store(u64::MAX, Ordering::Release);
        if let Some(request) = self.shell.worker_bus.preview.latest_request.take() {
            self.retire_superseded_preview_request(request);
        }
        if let Some(request) = self.deferred_latest_preview_request.take() {
            self.retire_superseded_preview_request(request);
        }
        self.shell.worker_bus.preview.worker_inflight_request_id = None;
        self.shell.worker_bus.preview.worker_input_document_bytes = 0;
        self.shell.worker_bus.preview.clear_request();
        self.deferred_more_intent = None;
        let tab_ids = self.shell.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>();
        for tab in self.shell.tabs.iter_mut() {
            if tab.pending_preview_request_id.is_some() {
                tab.clear_preview_request_state();
                tab.mark_preview_reload_pending();
                tab.notice = "Preview worker is unavailable".into();
            }
        }
        for tab_id in tab_ids {
            self.shell
                .tabs
                .clear_preview_response_routing_for_tab(tab_id);
        }
        self.clear_paged_preview();
        if !has_committed_document {
            self.shell
                .runtime
                .set_preview("<preview unavailable>".into());
        }
        self.paged_preview_view.error = Some(crate::ui_model::PreviewPageError::ReadFailed);
        self.shell
            .runtime
            .set_preview_page_error(self.paged_preview_view.error);
        self.set_notice("Preview worker is unavailable");
    }

    fn settle_preview_worker_response(&mut self, request_id: u64) {
        if let Err(request) = self.shell.worker_bus.preview.settle_response(request_id) {
            self.retire_superseded_preview_request(request);
            self.fail_preview_worker();
        }
    }

    pub(super) fn apply_background_preview_response(&mut self, mut response: PreviewResponse) {
        let target_index = self
            .shell
            .tabs
            .preview_request_tab(response.request_id)
            .and_then(|tab_id| self.find_tab_index_by_id(tab_id))
            .filter(|index| *index != self.shell.tabs.active_tab_index())
            .filter(|index| {
                self.shell.tabs.get(*index).is_some_and(|tab| {
                    tab.pending_preview_request_id == Some(response.request_id)
                        && tab.result_state.committed.current_row.and_then(|row| {
                            let results = if tab.result_state.results_compacted {
                                &tab.result_state.committed.base_results
                            } else {
                                &tab.result_state.committed.results
                            };
                            results.get(row).map(|(path, _)| path)
                        }) == Some(&response.path)
                })
            });
        if let Some(document) = response.document.as_ref() {
            if !self.enforce_preview_payload_budget(Some(document), false) {
                self.deferred_preview_response = Some(response);
                return;
            }
        }
        let replaces_document = !response.canceled
            && (response.document.is_some()
                || response.page_error.is_some() && !response.is_more
                || response.page_error.is_none());
        if let Some(index) = target_index.filter(|_| replaces_document) {
            let old = self
                .shell
                .tabs
                .get_mut(index)
                .and_then(|tab| tab.result_state.committed.preview_document.take());
            if let Some(old) = old {
                if let Err(old) = self.shell.runtime.try_retire_external_preview_document(old) {
                    self.shell
                        .tabs
                        .get_mut(index)
                        .unwrap()
                        .result_state
                        .committed
                        .preview_document = Some(old);
                    self.deferred_preview_response = Some(response);
                    return;
                }
            }
        } else if let Some(document) = response.document.take() {
            if let Err(document) = self
                .shell
                .runtime
                .try_retire_external_preview_document(document)
            {
                response.document = Some(document);
                self.deferred_preview_response = Some(response);
                return;
            }
        }
        let request_id = response.request_id;
        result_reducer::apply_background_preview_response(self, response);
        if self.shell.worker_bus.preview.pending_request_id == Some(request_id) {
            self.shell.worker_bus.preview.clear_request();
            self.clear_paged_preview();
            self.request_preview_for_current();
        }
        self.settle_preview_worker_response(request_id);
    }

    pub(super) fn apply_active_preview_response(&mut self, response: &PreviewResponse) -> bool {
        if self.shell.worker_bus.preview.pending_request_id == Some(response.request_id) {
            if response.document.is_some() || response.page_error.is_some() {
                if !self.paged_preview_response_matches_active(response) {
                    self.restore_paged_preview_view_for_active();
                }
                if !self.paged_preview_response_matches_active(response) {
                    return false;
                }
            }
            if let Some(document) = response.document.as_ref() {
                if !self.enforce_preview_payload_budget(Some(document), true)
                    || !self.shell.runtime.try_retire_preview_document()
                {
                    self.deferred_preview_response = Some(response.clone());
                    return true;
                }
            }
        }
        let applied = result_reducer::apply_active_preview_response(self, response);
        if applied {
            self.settle_preview_worker_response(response.request_id);
        }
        applied
    }
}

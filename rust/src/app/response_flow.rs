use super::*;

impl FlistWalkerApp {
    pub(super) fn bind_preview_request_to_tab(&mut self, request_id: u64, tab_id: u64) {
        self.shell
            .tabs
            .request_tab_routing
            .bind_preview(request_id, tab_id);
    }

    pub(super) fn bind_preview_request_to_current_tab(&mut self, request_id: u64) {
        if let Some(tab_id) = self.current_tab_id() {
            self.bind_preview_request_to_tab(request_id, tab_id);
        }
    }

    pub(super) fn take_preview_request_tab(&mut self, request_id: u64) -> Option<u64> {
        self.shell.tabs.request_tab_routing.take_preview(request_id)
    }

    pub(super) fn clear_response_routing_for_tab(&mut self, tab_id: u64) {
        self.shell.tabs.request_tab_routing.clear_for_tab(tab_id);
    }

    #[cfg(test)]
    pub(super) fn preview_request_tab(&self, request_id: u64) -> Option<u64> {
        self.shell
            .tabs
            .request_tab_routing
            .preview
            .get(&request_id)
            .copied()
    }

    pub(super) fn bind_action_request_to_tab(&mut self, request_id: u64, tab_id: u64) {
        self.shell
            .tabs
            .request_tab_routing
            .bind_action(request_id, tab_id);
    }

    pub(super) fn bind_action_request_to_current_tab(&mut self, request_id: u64) {
        if let Some(tab_id) = self.current_tab_id() {
            self.bind_action_request_to_tab(request_id, tab_id);
        }
    }

    pub(super) fn take_action_request_tab(&mut self, request_id: u64) -> Option<u64> {
        self.shell.tabs.request_tab_routing.take_action(request_id)
    }

    pub(super) fn bind_sort_request_to_tab(&mut self, request_id: u64, tab_id: u64) {
        self.shell
            .tabs
            .request_tab_routing
            .bind_sort(request_id, tab_id);
    }

    pub(super) fn bind_sort_request_to_current_tab(&mut self, request_id: u64) {
        if let Some(tab_id) = self.current_tab_id() {
            self.bind_sort_request_to_tab(request_id, tab_id);
        }
    }

    pub(super) fn take_sort_request_tab(&mut self, request_id: u64) -> Option<u64> {
        self.shell.tabs.request_tab_routing.take_sort(request_id)
    }

    #[cfg(test)]
    pub(super) fn action_request_tab(&self, request_id: u64) -> Option<u64> {
        self.shell
            .tabs
            .request_tab_routing
            .action
            .get(&request_id)
            .copied()
    }

    #[cfg(test)]
    pub(super) fn sort_request_tab(&self, request_id: u64) -> Option<u64> {
        self.shell
            .tabs
            .request_tab_routing
            .sort
            .get(&request_id)
            .copied()
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
        while let Ok(response) = self.shell.worker_bus.preview.rx.try_recv() {
            if self.apply_active_preview_response(&response) {
                continue;
            }
            self.apply_background_preview_response(response);
        }
    }

    fn apply_background_preview_response(&mut self, response: PreviewResponse) {
        result_reducer::apply_background_preview_response(self, response);
    }

    pub(super) fn apply_active_preview_response(&mut self, response: &PreviewResponse) -> bool {
        result_reducer::apply_active_preview_response(self, response)
    }
}

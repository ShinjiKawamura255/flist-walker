use super::*;
use super::worker_protocol::{
    ActionRequest, ActionResponse, FileListRequest, FileListResponse, KindResolveRequest,
    KindResolveResponse, PreviewRequest, PreviewResponse, SortMetadataRequest,
    SortMetadataResponse, UpdateRequest, UpdateResponse,
};
use std::sync::mpsc::{Receiver, Sender};

pub(super) struct PreviewWorkerBus {
    pub(super) tx: Sender<PreviewRequest>,
    pub(super) rx: Receiver<PreviewResponse>,
    pub(super) next_request_id: u64,
    pub(super) pending_request_id: Option<u64>,
    pub(super) in_progress: bool,
}

pub(super) struct ActionWorkerBus {
    pub(super) tx: Sender<ActionRequest>,
    pub(super) rx: Receiver<ActionResponse>,
    pub(super) next_request_id: u64,
    pub(super) pending_request_id: Option<u64>,
    pub(super) in_progress: bool,
}

pub(super) struct SortWorkerBus {
    pub(super) tx: Sender<SortMetadataRequest>,
    pub(super) rx: Receiver<SortMetadataResponse>,
    pub(super) next_request_id: u64,
    pub(super) pending_request_id: Option<u64>,
    pub(super) in_progress: bool,
}

pub(super) struct KindWorkerBus {
    pub(super) tx: Sender<KindResolveRequest>,
    pub(super) rx: Receiver<KindResolveResponse>,
}

pub(super) struct FileListWorkerBus {
    pub(super) tx: Sender<FileListRequest>,
    pub(super) rx: Receiver<FileListResponse>,
}

pub(super) struct UpdateWorkerBus {
    pub(super) tx: Sender<UpdateRequest>,
    pub(super) rx: Receiver<UpdateResponse>,
}

pub(super) struct WorkerBus {
    pub(super) preview: PreviewWorkerBus,
    pub(super) action: ActionWorkerBus,
    pub(super) sort: SortWorkerBus,
    pub(super) kind: KindWorkerBus,
    pub(super) filelist: FileListWorkerBus,
    pub(super) update: UpdateWorkerBus,
}

impl FlistWalkerApp {
    pub(super) fn bind_action_request_to_tab(&mut self, request_id: u64, tab_id: u64) {
        self.tabs
            .request_tab_routing
            .bind_action(request_id, tab_id);
    }

    pub(super) fn bind_action_request_to_current_tab(&mut self, request_id: u64) {
        if let Some(tab_id) = self.current_tab_id() {
            self.bind_action_request_to_tab(request_id, tab_id);
        }
    }

    pub(super) fn take_action_request_tab(&mut self, request_id: u64) -> Option<u64> {
        self.tabs.request_tab_routing.take_action(request_id)
    }

    pub(super) fn bind_sort_request_to_tab(&mut self, request_id: u64, tab_id: u64) {
        self.tabs.request_tab_routing.bind_sort(request_id, tab_id);
    }

    pub(super) fn bind_sort_request_to_current_tab(&mut self, request_id: u64) {
        if let Some(tab_id) = self.current_tab_id() {
            self.bind_sort_request_to_tab(request_id, tab_id);
        }
    }

    pub(super) fn take_sort_request_tab(&mut self, request_id: u64) -> Option<u64> {
        self.tabs.request_tab_routing.take_sort(request_id)
    }

    pub(super) fn clear_tab_owned_request_routing(&mut self, tab_id: u64) {
        self.tabs.request_tab_routing.clear_action_for_tab(tab_id);
        self.tabs.request_tab_routing.clear_sort_for_tab(tab_id);
    }

    #[cfg(test)]
    pub(super) fn action_request_tab(&self, request_id: u64) -> Option<u64> {
        self.tabs
            .request_tab_routing
            .action
            .get(&request_id)
            .copied()
    }

    #[cfg(test)]
    pub(super) fn sort_request_tab(&self, request_id: u64) -> Option<u64> {
        self.tabs.request_tab_routing.sort.get(&request_id).copied()
    }
}

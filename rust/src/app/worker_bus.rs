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

impl PreviewWorkerBus {
    pub(super) fn allocate_request_id(&mut self) -> u64 {
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1);
        request_id
    }

    pub(super) fn begin_request(&mut self) -> u64 {
        let request_id = self.allocate_request_id();
        self.pending_request_id = Some(request_id);
        self.in_progress = true;
        request_id
    }

    pub(super) fn clear_request(&mut self) {
        self.pending_request_id = None;
        self.in_progress = false;
    }
}

pub(super) struct ActionWorkerBus {
    pub(super) tx: Sender<ActionRequest>,
    pub(super) rx: Receiver<ActionResponse>,
    pub(super) next_request_id: u64,
    pub(super) pending_request_id: Option<u64>,
    pub(super) in_progress: bool,
}

impl ActionWorkerBus {
    pub(super) fn allocate_request_id(&mut self) -> u64 {
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1);
        request_id
    }

    pub(super) fn begin_request(&mut self) -> u64 {
        let request_id = self.allocate_request_id();
        self.pending_request_id = Some(request_id);
        self.in_progress = true;
        request_id
    }

    pub(super) fn clear_request(&mut self) {
        self.pending_request_id = None;
        self.in_progress = false;
    }
}

pub(super) struct SortWorkerBus {
    pub(super) tx: Sender<SortMetadataRequest>,
    pub(super) rx: Receiver<SortMetadataResponse>,
    pub(super) next_request_id: u64,
    pub(super) pending_request_id: Option<u64>,
    pub(super) in_progress: bool,
}

impl SortWorkerBus {
    pub(super) fn allocate_request_id(&mut self) -> u64 {
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1);
        request_id
    }

    pub(super) fn begin_request(&mut self) -> u64 {
        let request_id = self.allocate_request_id();
        self.pending_request_id = Some(request_id);
        self.in_progress = true;
        request_id
    }

    pub(super) fn clear_request(&mut self) {
        self.pending_request_id = None;
        self.in_progress = false;
    }
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

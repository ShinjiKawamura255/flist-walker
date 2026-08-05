use super::worker_bus_lifecycle;
use super::worker_channel::BoundedSender;
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
    pub(super) fn begin_request(&mut self) -> u64 {
        worker_bus_lifecycle::begin_request(
            &mut self.next_request_id,
            &mut self.pending_request_id,
            &mut self.in_progress,
        )
    }

    pub(super) fn clear_request(&mut self) {
        worker_bus_lifecycle::clear_request(&mut self.pending_request_id, &mut self.in_progress);
    }
}

pub(super) struct ActionWorkerBus {
    pub(super) tx: BoundedSender<ActionRequest>,
    pub(super) rx: Receiver<ActionResponse>,
    pub(super) next_request_id: u64,
    pub(super) pending_request_id: Option<u64>,
    pub(super) in_progress: bool,
}

impl ActionWorkerBus {
    pub(super) fn allocate_request_id(&mut self) -> u64 {
        worker_bus_lifecycle::allocate_request_id(&mut self.next_request_id)
    }

    pub(super) fn accept_request(&mut self, request_id: u64) {
        self.pending_request_id = Some(request_id);
        self.in_progress = true;
    }

    pub(super) fn clear_request(&mut self) {
        worker_bus_lifecycle::clear_request(&mut self.pending_request_id, &mut self.in_progress);
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
    pub(super) fn begin_request(&mut self) -> u64 {
        worker_bus_lifecycle::begin_request(
            &mut self.next_request_id,
            &mut self.pending_request_id,
            &mut self.in_progress,
        )
    }

    pub(super) fn clear_request(&mut self) {
        worker_bus_lifecycle::clear_request(&mut self.pending_request_id, &mut self.in_progress);
    }
}

pub(super) struct KindWorkerBus {
    pub(super) tx: BoundedSender<KindResolveRequest>,
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

use super::*;

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

use super::channel::BoundedSender;
use super::protocol::{
    ActionRequest, ActionResponse, CatalogRequest, CatalogResponse, FileListRequest,
    FileListResponse, KindResolveRequest, KindResolveResponse, PreviewRequest, PreviewResponse,
    RootValidationRequest, RootValidationResponse, SortMetadataRequest, SortMetadataResponse,
    UpdateRequest, UpdateResponse,
};
use std::sync::mpsc::{Receiver, Sender};

fn allocate_request_id(next_request_id: &mut u64) -> u64 {
    let request_id = *next_request_id;
    *next_request_id = next_request_id.saturating_add(1);
    request_id
}

fn begin_request(
    next_request_id: &mut u64,
    pending_request_id: &mut Option<u64>,
    in_progress: &mut bool,
) -> u64 {
    let request_id = allocate_request_id(next_request_id);
    *pending_request_id = Some(request_id);
    *in_progress = true;
    request_id
}

fn clear_request(pending_request_id: &mut Option<u64>, in_progress: &mut bool) {
    *pending_request_id = None;
    *in_progress = false;
}

pub(in crate::app) struct PreviewWorkerBus {
    pub(in crate::app) tx: Sender<PreviewRequest>,
    pub(in crate::app) rx: Receiver<PreviewResponse>,
    pub(in crate::app) next_request_id: u64,
    pub(in crate::app) pending_request_id: Option<u64>,
    pub(in crate::app) in_progress: bool,
}

impl PreviewWorkerBus {
    pub(in crate::app) fn begin_request(&mut self) -> u64 {
        begin_request(
            &mut self.next_request_id,
            &mut self.pending_request_id,
            &mut self.in_progress,
        )
    }

    pub(in crate::app) fn clear_request(&mut self) {
        clear_request(&mut self.pending_request_id, &mut self.in_progress);
    }
}

pub(in crate::app) struct ActionWorkerBus {
    pub(in crate::app) tx: BoundedSender<ActionRequest>,
    pub(in crate::app) rx: Receiver<ActionResponse>,
    pub(in crate::app) next_request_id: u64,
    pub(in crate::app) pending_request_id: Option<u64>,
    pub(in crate::app) in_progress: bool,
}

impl ActionWorkerBus {
    pub(in crate::app) fn allocate_request_id(&mut self) -> u64 {
        allocate_request_id(&mut self.next_request_id)
    }

    pub(in crate::app) fn accept_request(&mut self, request_id: u64) {
        self.pending_request_id = Some(request_id);
        self.in_progress = true;
    }

    pub(in crate::app) fn clear_request(&mut self) {
        clear_request(&mut self.pending_request_id, &mut self.in_progress);
    }
}

pub(in crate::app) struct SortWorkerBus {
    pub(in crate::app) tx: Sender<SortMetadataRequest>,
    pub(in crate::app) rx: Receiver<SortMetadataResponse>,
    pub(in crate::app) next_request_id: u64,
    pub(in crate::app) pending_request_id: Option<u64>,
    pub(in crate::app) in_progress: bool,
}

impl SortWorkerBus {
    pub(in crate::app) fn begin_request(&mut self) -> u64 {
        begin_request(
            &mut self.next_request_id,
            &mut self.pending_request_id,
            &mut self.in_progress,
        )
    }

    pub(in crate::app) fn clear_request(&mut self) {
        clear_request(&mut self.pending_request_id, &mut self.in_progress);
    }
}

pub(in crate::app) struct KindWorkerBus {
    pub(in crate::app) tx: BoundedSender<KindResolveRequest>,
    pub(in crate::app) rx: Receiver<KindResolveResponse>,
}

pub(in crate::app) struct FileListWorkerBus {
    pub(in crate::app) tx: Sender<FileListRequest>,
    pub(in crate::app) rx: Receiver<FileListResponse>,
}

pub(in crate::app) struct UpdateWorkerBus {
    pub(in crate::app) tx: Sender<UpdateRequest>,
    pub(in crate::app) rx: Receiver<UpdateResponse>,
}

pub(in crate::app) struct CatalogWorkerBus {
    pub(in crate::app) tx: Sender<CatalogRequest>,
    pub(in crate::app) rx: Receiver<CatalogResponse>,
    pub(in crate::app) next_request_id: u64,
    pub(in crate::app) pending_request_id: Option<u64>,
    pub(in crate::app) in_progress: bool,
}

pub(in crate::app) struct RootValidationWorkerBus {
    pub(in crate::app) tx: Sender<RootValidationRequest>,
    pub(in crate::app) rx: Receiver<RootValidationResponse>,
    pub(in crate::app) next_request_id: u64,
    pub(in crate::app) pending_request_id: Option<u64>,
    pub(in crate::app) in_progress: bool,
}

impl RootValidationWorkerBus {
    pub(in crate::app) fn begin_request(&mut self) -> u64 {
        begin_request(
            &mut self.next_request_id,
            &mut self.pending_request_id,
            &mut self.in_progress,
        )
    }

    pub(in crate::app) fn clear_request(&mut self) {
        clear_request(&mut self.pending_request_id, &mut self.in_progress);
    }
}

impl CatalogWorkerBus {
    pub(in crate::app) fn begin_request(&mut self) -> u64 {
        begin_request(
            &mut self.next_request_id,
            &mut self.pending_request_id,
            &mut self.in_progress,
        )
    }

    pub(in crate::app) fn clear_request(&mut self) {
        clear_request(&mut self.pending_request_id, &mut self.in_progress);
    }
}

pub(in crate::app) struct WorkerBus {
    pub(in crate::app) preview: PreviewWorkerBus,
    pub(in crate::app) action: ActionWorkerBus,
    pub(in crate::app) sort: SortWorkerBus,
    pub(in crate::app) kind: KindWorkerBus,
    pub(in crate::app) filelist: FileListWorkerBus,
    pub(in crate::app) update: UpdateWorkerBus,
    pub(in crate::app) catalog: CatalogWorkerBus,
    pub(in crate::app) root_validation: RootValidationWorkerBus,
}

pub(super) fn allocate_request_id(next_request_id: &mut u64) -> u64 {
    let request_id = *next_request_id;
    *next_request_id = next_request_id.saturating_add(1);
    request_id
}

pub(super) fn begin_request(
    next_request_id: &mut u64,
    pending_request_id: &mut Option<u64>,
    in_progress: &mut bool,
) -> u64 {
    let request_id = allocate_request_id(next_request_id);
    *pending_request_id = Some(request_id);
    *in_progress = true;
    request_id
}

pub(super) fn clear_request(pending_request_id: &mut Option<u64>, in_progress: &mut bool) {
    *pending_request_id = None;
    *in_progress = false;
}

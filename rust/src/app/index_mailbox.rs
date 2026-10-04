use super::IndexResponse;
use std::collections::VecDeque;
use std::fmt;
use std::sync::Mutex;

pub(super) const INDEX_MAILBOX_DATA_CAPACITY: usize = 8;

struct SequencedResponse {
    sequence: u64,
    response: IndexResponse,
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub(super) struct IndexPerfStaleFullDataAbort {
    pub(super) request_id: u64,
    pub(super) tab_id: u64,
    pub(super) response_request_id: u64,
    pub(super) data_kind: &'static str,
    pub(super) at: std::time::Instant,
    pub(super) latest_id: Option<u64>,
    pub(super) latest_lookup_succeeded: bool,
    pub(super) shutdown: bool,
}

#[cfg(test)]
#[derive(Clone, Debug, Default)]
pub(super) struct IndexPerfObservation {
    pub(super) stale_full_data_abort: Option<IndexPerfStaleFullDataAbort>,
    pub(super) stale_full_data_abort_duplicate: bool,
    pub(super) batches: usize,
    pub(super) entries_emitted: usize,
    pub(super) replacements: usize,
    pub(super) full_wait: std::time::Duration,
    pub(super) full_retries: usize,
    pub(super) blocked_batches: usize,
    pub(super) data_publish_end: Option<std::time::Instant>,
    pub(super) terminal_published: Option<std::time::Instant>,
    pub(super) terminal_kind: Option<&'static str>,
    pub(super) truncated_limit: Option<usize>,
    pub(super) started_source: Option<&'static str>,
    pub(super) started_published: Option<std::time::Instant>,
    pub(super) terminal_source: Option<&'static str>,
    pub(super) nested_input_reused: Option<bool>,
    pub(super) terminal_offered: Option<std::time::Instant>,
    pub(super) terminal_offer_kind: Option<&'static str>,
    pub(super) terminal_offer_current: Option<bool>,
    pub(super) terminal_offer_error: Option<String>,
    pub(super) terminal_send_returned: Option<std::time::Instant>,
    pub(super) request_processing_returned: Option<std::time::Instant>,
    pub(super) mailbox_closed: bool,
    pub(super) mailbox_closed_at: Option<std::time::Instant>,
    pub(super) allocation_observed: bool,
    pub(super) admitted_at: Option<std::time::Instant>,
    pub(super) admitted_root: Option<std::path::PathBuf>,
    pub(super) started_root: Option<std::path::PathBuf>,
    pub(super) skipped_closed_before_start: bool,
}

#[derive(Default)]
struct MailboxState {
    next_sequence: u64,
    data: VecDeque<SequencedResponse>,
    started: Option<SequencedResponse>,
    truncated: Option<SequencedResponse>,
    terminal: Option<SequencedResponse>,
    closed: bool,
    snapshot: Option<super::freshness::SnapshotFreshness>,
}

pub(super) enum IndexMailboxPublishError {
    Full(IndexResponse),
    Closed(IndexResponse),
    SlotOccupied(IndexResponse),
}

impl fmt::Debug for IndexMailboxPublishError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Full(_) => "IndexMailboxPublishError::Full",
            Self::Closed(_) => "IndexMailboxPublishError::Closed",
            Self::SlotOccupied(_) => "IndexMailboxPublishError::SlotOccupied",
        })
    }
}

pub(super) struct IndexResponseMailbox {
    data_capacity: usize,
    state: Mutex<MailboxState>,
    #[cfg(test)]
    perf_enabled: std::sync::atomic::AtomicBool,
    #[cfg(test)]
    perf_state: std::sync::OnceLock<IndexPerfHandle>,
}

impl IndexResponseMailbox {
    pub(super) fn new() -> Self {
        Self::with_data_capacity(INDEX_MAILBOX_DATA_CAPACITY)
    }

    pub(super) fn with_data_capacity(data_capacity: usize) -> Self {
        Self {
            data_capacity: data_capacity.max(1),
            state: Mutex::new(MailboxState::default()),
            #[cfg(test)]
            perf_enabled: std::sync::atomic::AtomicBool::new(false),
            #[cfg(test)]
            perf_state: std::sync::OnceLock::new(),
        }
    }

    #[cfg(test)]
    pub(super) fn enable_perf_observation(&self) {
        self.perf_state
            .get_or_init(|| std::sync::Arc::new(Mutex::new(IndexPerfObservation::default())));
        self.perf_enabled
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
    #[cfg(test)]
    pub(super) fn perf_enabled(&self) -> bool {
        self.perf_enabled.load(std::sync::atomic::Ordering::Relaxed)
    }
    #[cfg(test)]
    pub(super) fn perf_handle(&self) -> IndexPerfHandle {
        std::sync::Arc::clone(self.perf_state.get().expect("enabled observation"))
    }
    #[cfg(test)]
    pub(super) fn perf_observation(&self) -> IndexPerfObservation {
        self.perf_state
            .get()
            .map_or_else(IndexPerfObservation::default, |p| {
                p.lock().expect("index observation").clone()
            })
    }
    #[cfg(test)]
    fn update_perf(&self, f: impl FnOnce(&mut IndexPerfObservation)) {
        if let Some(p) = self.perf_state.get() {
            f(&mut p.lock().expect("index observation"));
        }
    }
    #[cfg(test)]
    pub(super) fn record_stale_full_data_abort(&self, event: IndexPerfStaleFullDataAbort) {
        self.update_perf(|p| {
            if p.stale_full_data_abort.is_some() {
                p.stale_full_data_abort_duplicate = true;
            } else {
                p.stale_full_data_abort = Some(event);
            }
        });
    }
    #[cfg(test)]
    pub(super) fn record_data_publish_end(&self) {
        self.update_perf(|p| p.data_publish_end = Some(std::time::Instant::now()));
    }
    #[cfg(test)]
    pub(super) fn record_full_wait(&self, elapsed: std::time::Duration, retries: usize) {
        self.update_perf(|p| {
            p.full_wait += elapsed;
            p.full_retries += retries;
            p.blocked_batches += 1;
        });
    }
    #[cfg(test)]
    pub(super) fn record_terminal_offer(&self, response: &super::IndexResponse, current: bool) {
        if !self.perf_enabled() {
            return;
        }
        let (kind, error) = match response {
            super::IndexResponse::Finished { .. } => ("finished", None),
            super::IndexResponse::Canceled { .. } => ("canceled", None),
            super::IndexResponse::Failed { error, .. } => ("failed", Some(error.clone())),
            _ => return,
        };
        self.update_perf(|p| {
            p.terminal_offered = Some(std::time::Instant::now());
            p.terminal_offer_kind = Some(kind);
            p.terminal_offer_current = Some(current);
            p.terminal_offer_error = error;
        });
    }
    #[cfg(test)]
    pub(super) fn record_terminal_send_returned(&self) {
        self.update_perf(|p| p.terminal_send_returned = Some(std::time::Instant::now()));
    }
    #[cfg(test)]
    pub(super) fn record_nested_input_reused(&self, reused: bool) {
        self.update_perf(|p| p.nested_input_reused = Some(reused));
    }

    // Called only by the index worker. Filesystem probes occur outside the mutex.
    pub(super) fn record_snapshot_started(
        &self,
        request_id: u64,
        root: std::path::PathBuf,
        source: crate::indexer::IndexSource,
    ) {
        use super::freshness::{observe_file, FileObservation, SnapshotFreshness};
        // Discovery resolves the root before selecting its root-only FileList.
        // Keep snapshot/check identity in the requested lexical root namespace
        // (e.g. /var vs /private/var on macOS), without resolving paths on UI.
        let source = match source {
            crate::indexer::IndexSource::FileList(path) => {
                match path
                    .file_name()
                    .filter(|name| *name == "FileList.txt" || *name == "filelist.txt")
                {
                    Some(name) => crate::indexer::IndexSource::FileList(root.join(name)),
                    None => crate::indexer::IndexSource::FileList(path),
                }
            }
            other => other,
        };
        let baseline = match &source {
            crate::indexer::IndexSource::FileList(path) => observe_file(path),
            _ => FileObservation::Unavailable,
        };
        let snapshot =
            SnapshotFreshness::acquired(request_id, root, source, baseline.clone(), baseline);
        #[cfg(test)]
        self.update_perf(|p| p.started_root = Some(snapshot.root.clone()));
        if let Ok(mut state) = self.state.lock() {
            state.snapshot = Some(snapshot);
        }
    }

    pub(super) fn record_snapshot_completed(&self) {
        use super::freshness::{observe_file, FileListChange};
        let Some(mut snapshot) = self.snapshot() else {
            return;
        };
        if let crate::indexer::IndexSource::FileList(path) = &snapshot.source {
            snapshot.change = FileListChange::compare(&snapshot.baseline, &observe_file(path));
        }
        snapshot.acquired_at = std::time::SystemTime::now();
        if let Ok(mut state) = self.state.lock() {
            state.snapshot = Some(snapshot);
        }
    }

    pub(super) fn snapshot(&self) -> Option<super::freshness::SnapshotFreshness> {
        self.state.lock().ok()?.snapshot.clone()
    }

    pub(super) fn try_publish(
        &self,
        response: IndexResponse,
    ) -> Result<(), IndexMailboxPublishError> {
        let Ok(mut state) = self.state.lock() else {
            return Err(IndexMailboxPublishError::Closed(response));
        };
        if state.closed || state.terminal.is_some() {
            return Err(IndexMailboxPublishError::Closed(response));
        }
        if matches!(
            response,
            IndexResponse::Batch { .. } | IndexResponse::ReplaceAll { .. }
        ) && state.data.len() >= self.data_capacity
        {
            return Err(IndexMailboxPublishError::Full(response));
        }

        #[cfg(test)]
        if self.perf_enabled() {
            let mut perf = self
                .perf_state
                .get()
                .expect("enabled observation")
                .lock()
                .expect("index observation");
            match &response {
                IndexResponse::Started { source, .. } => {
                    perf.started_published = Some(std::time::Instant::now());
                    perf.started_source = Some(match source {
                        crate::indexer::IndexSource::FileList(_) => "FileList",
                        crate::indexer::IndexSource::Walker => "Walker",
                        crate::indexer::IndexSource::None => "None",
                    });
                }
                IndexResponse::Batch { entries, .. }
                | IndexResponse::ReplaceAll { entries, .. } => {
                    perf.batches += 1;
                    perf.entries_emitted += entries.len();
                    perf.replacements +=
                        usize::from(matches!(&response, IndexResponse::ReplaceAll { .. }));
                }
                IndexResponse::Finished { .. }
                | IndexResponse::Failed { .. }
                | IndexResponse::Canceled { .. } => {
                    perf.terminal_published = Some(std::time::Instant::now());
                    perf.terminal_kind = Some(match &response {
                        IndexResponse::Finished { .. } => "finished",
                        IndexResponse::Failed { .. } => "failed",
                        IndexResponse::Canceled { .. } => "canceled",
                        _ => unreachable!(),
                    });
                    if let IndexResponse::Finished { source, .. } = &response {
                        perf.terminal_source = Some(match source {
                            crate::indexer::IndexSource::FileList(_) => "FileList",
                            crate::indexer::IndexSource::Walker => "Walker",
                            crate::indexer::IndexSource::None => "None",
                        });
                    }
                }
                IndexResponse::Truncated { limit, .. } => perf.truncated_limit = Some(*limit),
            }
        }
        let sequenced = SequencedResponse {
            sequence: state.next_sequence,
            response,
        };
        state.next_sequence = state.next_sequence.saturating_add(1);
        match sequenced.response {
            IndexResponse::Started { .. } => {
                if state.started.is_some() {
                    return Err(IndexMailboxPublishError::SlotOccupied(sequenced.response));
                }
                state.started = Some(sequenced);
            }
            IndexResponse::Batch { .. } | IndexResponse::ReplaceAll { .. } => {
                state.data.push_back(sequenced);
            }
            IndexResponse::Truncated { .. } => {
                if state.truncated.is_some() {
                    return Err(IndexMailboxPublishError::SlotOccupied(sequenced.response));
                }
                state.truncated = Some(sequenced);
            }
            IndexResponse::Finished { .. }
            | IndexResponse::Failed { .. }
            | IndexResponse::Canceled { .. } => {
                state.terminal = Some(sequenced);
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn try_recv(&self) -> Option<IndexResponse> {
        self.try_recv_with_terminal_admission(true)
    }

    pub(super) fn try_recv_with_terminal_admission(
        &self,
        allow_terminal: bool,
    ) -> Option<IndexResponse> {
        let mut state = self.state.lock().ok()?;
        let mut next_kind = None;
        let mut next_sequence = u64::MAX;
        if let Some(response) = state.started.as_ref() {
            next_sequence = response.sequence;
            next_kind = Some(0u8);
        }
        if let Some(response) = state.data.front() {
            if response.sequence < next_sequence {
                next_sequence = response.sequence;
                next_kind = Some(1);
            }
        }
        if let Some(response) = state.truncated.as_ref() {
            if response.sequence < next_sequence {
                next_sequence = response.sequence;
                next_kind = Some(2);
            }
        }
        if let Some(response) = state.terminal.as_ref() {
            if response.sequence < next_sequence {
                next_kind = Some(3);
            }
        }
        match next_kind? {
            0 => state.started.take().map(|response| response.response),
            1 => state.data.pop_front().map(|response| response.response),
            2 => state.truncated.take().map(|response| response.response),
            3 if allow_terminal => state.terminal.take().map(|response| response.response),
            3 => None,
            _ => None,
        }
    }

    pub(super) fn close(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.closed = true;
            #[cfg(test)]
            if self.perf_enabled() {
                self.update_perf(|p| {
                    p.mailbox_closed = true;
                    p.mailbox_closed_at
                        .get_or_insert_with(std::time::Instant::now);
                });
            }
        }
    }

    pub(super) fn has_payload(&self) -> bool {
        self.state.lock().is_ok_and(|state| {
            !state.data.is_empty()
                || state.started.is_some()
                || state.truncated.is_some()
                || state.terminal.is_some()
        })
    }

    pub(super) fn has_terminal_response(&self) -> bool {
        self.state
            .lock()
            .is_ok_and(|state| state.terminal.is_some())
    }
}

impl IndexMailboxPublishError {
    pub(super) fn into_response(self) -> IndexResponse {
        match self {
            Self::Full(response) | Self::Closed(response) | Self::SlotOccupied(response) => {
                response
            }
        }
    }
}

#[cfg(test)]
pub(super) type IndexPerfHandle = std::sync::Arc<Mutex<IndexPerfObservation>>;
#[cfg(test)]
type IndexPerfRegistry =
    std::collections::HashMap<(usize, u64), std::sync::Weak<Mutex<IndexPerfObservation>>>;
#[cfg(test)]
fn perf_registry() -> &'static Mutex<IndexPerfRegistry> {
    static REGISTRY: std::sync::OnceLock<Mutex<IndexPerfRegistry>> = std::sync::OnceLock::new();
    REGISTRY.get_or_init(Default::default)
}
#[cfg(test)]
pub(super) fn register_perf_request(identity: usize, id: u64, handle: &IndexPerfHandle) {
    let mut r = perf_registry().lock().expect("index perf registry");
    r.retain(|_, w| w.strong_count() > 0);
    r.insert((identity, id), std::sync::Arc::downgrade(handle));
}
#[cfg(test)]
pub(super) fn take_perf_request(identity: usize, id: u64) -> Option<IndexPerfHandle> {
    perf_registry()
        .lock()
        .expect("index perf registry")
        .remove(&(identity, id))
        .and_then(|w| w.upgrade())
}

use super::IndexResponse;
use std::collections::VecDeque;
use std::fmt;
use std::sync::Mutex;

pub(super) const INDEX_MAILBOX_DATA_CAPACITY: usize = 8;

struct SequencedResponse {
    sequence: u64,
    response: IndexResponse,
}

#[derive(Default)]
struct MailboxState {
    next_sequence: u64,
    data: VecDeque<SequencedResponse>,
    started: Option<SequencedResponse>,
    truncated: Option<SequencedResponse>,
    terminal: Option<SequencedResponse>,
    closed: bool,
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
}

impl IndexResponseMailbox {
    pub(super) fn new() -> Self {
        Self::with_data_capacity(INDEX_MAILBOX_DATA_CAPACITY)
    }

    pub(super) fn with_data_capacity(data_capacity: usize) -> Self {
        Self {
            data_capacity: data_capacity.max(1),
            state: Mutex::new(MailboxState::default()),
        }
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

    pub(crate) fn try_recv(&self) -> Option<IndexResponse> {
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
            3 => state.terminal.take().map(|response| response.response),
            _ => None,
        }
    }

    pub(super) fn close(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.closed = true;
        }
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

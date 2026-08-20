#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TerminalOutcome {
    Finished,
    Failed,
    Canceled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum IndexData {
    Batch,
    ReplaceAll,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum Event {
    CreateTab,
    CloseTab(usize),
    RestoreTab,
    SwitchTab(usize),
    ReorderTab { from: usize, to: usize },
    ChangeQuery(u8),
    ChangeRoot(usize),
    RefreshIndex,
    DeliverOldestIndexData(IndexData),
    DeliverNewestIndexData(IndexData),
    CompleteOldestIndex(TerminalOutcome),
    CompleteNewestIndex(TerminalOutcome),
    CompleteOldestSearch,
    RequestPreview,
    CompleteOldestPreview,
    RequestAction,
    CompleteOldestAction,
    RequestSort,
    CompleteOldestSort,
    RequestFileList,
    CompleteOldestFileList,
    DeliverStaleIndex,
    DeliverStaleSearch,
}

#[derive(Clone, Copy)]
pub(super) struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub(super) fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    fn index(&mut self, upper: usize) -> usize {
        if upper == 0 {
            return 0;
        }
        (self.next() as usize) % upper
    }

    fn outcome(&mut self) -> TerminalOutcome {
        match self.next() % 3 {
            0 => TerminalOutcome::Finished,
            1 => TerminalOutcome::Failed,
            _ => TerminalOutcome::Canceled,
        }
    }
}

pub(super) fn generate(seed: u64, steps: usize) -> Vec<Event> {
    let mut rng = SplitMix64::new(seed);
    (0..steps)
        .map(|_| match rng.next() % 24 {
            0 => Event::CreateTab,
            1 => Event::CloseTab(rng.index(7)),
            2 => Event::RestoreTab,
            3 => Event::SwitchTab(rng.index(7)),
            4 => Event::ReorderTab {
                from: rng.index(7),
                to: rng.index(7),
            },
            5 => Event::ChangeQuery(rng.index(6) as u8),
            6 => Event::ChangeRoot(rng.index(3)),
            7 => Event::RefreshIndex,
            8 => Event::DeliverOldestIndexData(IndexData::Batch),
            9 => Event::DeliverNewestIndexData(IndexData::ReplaceAll),
            10 => Event::CompleteOldestIndex(rng.outcome()),
            11 => Event::CompleteNewestIndex(rng.outcome()),
            12 => Event::CompleteOldestSearch,
            13 => Event::RequestPreview,
            14 => Event::CompleteOldestPreview,
            15 => Event::RequestAction,
            16 => Event::CompleteOldestAction,
            17 => Event::RequestSort,
            18 => Event::CompleteOldestSort,
            19 => Event::RequestFileList,
            20 => Event::CompleteOldestFileList,
            21 => Event::DeliverStaleIndex,
            22 => Event::DeliverStaleSearch,
            _ => Event::DeliverOldestIndexData(IndexData::ReplaceAll),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tc_181_generator_replays_the_same_seed() {
        let first = generate(0x181, 256);
        let second = generate(0x181, 256);
        let different = generate(0x182, 256);

        assert_eq!(first, second);
        assert_ne!(first, different);
    }
}

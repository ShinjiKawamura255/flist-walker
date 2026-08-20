mod events;
mod harness;
mod invariants;

use events::{generate, Event, TerminalOutcome};
use harness::StatefulHarness;

#[test]
fn tc_182_curated_state_sequence_preserves_app_invariants() {
    let events = vec![
        Event::CreateTab,
        Event::CreateTab,
        Event::RefreshIndex,
        Event::ChangeQuery(1),
        Event::SwitchTab(0),
        Event::ChangeRoot(1),
        Event::CompleteNewestIndex(TerminalOutcome::Finished),
        Event::CompleteOldestIndex(TerminalOutcome::Replaced),
        Event::DeliverStaleSearch,
        Event::ReorderTab { from: 0, to: 2 },
        Event::CloseTab(1),
        Event::DeliverStaleIndex,
        Event::RestoreTab,
        Event::ChangeQuery(0),
    ];
    let mut harness = StatefulHarness::new("stateful-curated");
    harness.run(0x182, &events);
    harness.quiesce(0x182);
    harness.cleanup();
}

#[test]
fn tc_183_seeded_state_sequences_converge() {
    const SEEDS: [u64; 16] = [
        0x1830, 0x1831, 0x1832, 0x1833, 0x1834, 0x1835, 0x1836, 0x1837, 0x1838, 0x1839, 0x183a,
        0x183b, 0x183c, 0x183d, 0x183e, 0x183f,
    ];
    for seed in SEEDS {
        let mut harness = StatefulHarness::new(&format!("stateful-seed-{seed:x}"));
        let events = generate(seed, 128);
        harness.run(seed, &events);
        harness.quiesce(seed);
        harness.cleanup();
    }
}

mod events;
mod harness;
mod invariants;

use crate::app::tests::*;
use events::{generate, Event, TerminalOutcome};
use harness::{snapshot_for_app, StatefulHarness};

fn parse_u64_setting(name: &str, default: u64) -> u64 {
    let Ok(raw) = std::env::var(name) else {
        return default;
    };
    let parsed = raw
        .strip_prefix("0x")
        .map(|hex| u64::from_str_radix(hex, 16))
        .unwrap_or_else(|| raw.parse());
    parsed.unwrap_or_else(|error| panic!("invalid {name}={raw:?}: {error}"))
}

fn run_generated_profile(base_seed: u64, seed_count: usize, steps: usize, label: &str) {
    eprintln!(
        "stateful endurance start: profile={label}; base_seed={base_seed:#x}; seed_count={seed_count}; steps={steps}"
    );
    for offset in 0..seed_count {
        let seed = base_seed.wrapping_add(offset as u64);
        let mut harness = StatefulHarness::new(&format!("{label}-{seed:x}"));
        let events = generate(seed, steps);
        harness.run(seed, &events);
        harness.quiesce(seed);
        harness.cleanup();
    }
    eprintln!(
        "stateful endurance complete: profile={label}; base_seed={base_seed:#x}; seed_count={seed_count}; steps={steps}"
    );
}

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

#[test]
#[ignore = "extended deterministic endurance profile; run explicitly"]
fn tc_184_stateful_endurance_extended() {
    let base_seed = parse_u64_setting("FLISTWALKER_ENDURANCE_BASE_SEED", 0x1840_0000);
    let seed_count = parse_u64_setting("FLISTWALKER_ENDURANCE_SEED_COUNT", 256) as usize;
    let steps = parse_u64_setting("FLISTWALKER_ENDURANCE_STEPS", 1_000) as usize;
    assert!(seed_count > 0, "seed count must be positive");
    assert!(steps > 0, "step count must be positive");
    run_generated_profile(base_seed, seed_count, steps, "stateful-extended");
}

#[test]
#[ignore = "single-seed replay entrypoint; set FLISTWALKER_ENDURANCE_SEED"]
fn stateful_endurance_replay() {
    let seed = parse_u64_setting("FLISTWALKER_ENDURANCE_SEED", 0x1830);
    let steps = parse_u64_setting("FLISTWALKER_ENDURANCE_STEPS", 1_000) as usize;
    assert!(steps > 0, "step count must be positive");
    run_generated_profile(seed, 1, steps, "stateful-replay");
}

#[test]
#[ignore = "real-worker soak profile; run explicitly"]
fn tc_184_stateful_endurance_real_worker_soak() {
    let duration_secs = parse_u64_setting("FLISTWALKER_ENDURANCE_SOAK_SECONDS", 10);
    assert!(duration_secs > 0, "soak duration must be positive");

    let base = test_root("stateful-real-worker-soak");
    let roots = vec![base.join("root-0"), base.join("root-1")];
    for (root_index, root) in roots.iter().enumerate() {
        fs::create_dir_all(root).expect("create real-worker soak root");
        for file_index in 0..256 {
            fs::write(
                root.join(format!("root-{root_index}-file-{file_index:04}.txt")),
                format!("stateful soak fixture {root_index}/{file_index}"),
            )
            .expect("write real-worker soak fixture");
        }
    }

    let mut app = FlistWalkerApp::new(roots[0].clone(), 100, String::new());
    app.shell.runtime.use_filelist = false;
    app.request_index_refresh();

    let started = Instant::now();
    let duration = Duration::from_secs(duration_secs);
    let mut iteration = 0usize;
    while started.elapsed() < duration {
        if iteration.is_multiple_of(3) {
            app.shell.runtime.query_state.query = format!("file-{:02}", iteration % 64);
            app.update_results();
        }
        if iteration.is_multiple_of(11) {
            let root = roots[(iteration / 11) % roots.len()].clone();
            app.apply_root_change_direct(root);
        }
        if iteration.is_multiple_of(17) && app.shell.tabs.len() < 4 {
            app.create_new_tab();
        }
        if iteration.is_multiple_of(7) {
            let next = (app.shell.tabs.active_tab_index() + 1) % app.shell.tabs.len();
            app.switch_to_tab_index(next);
        }
        if iteration.is_multiple_of(23) && app.shell.tabs.len() > 1 {
            app.close_tab_index(iteration % app.shell.tabs.len());
            app.restore_recently_closed_tab();
        }
        if iteration.is_multiple_of(13) {
            app.request_index_refresh();
        }

        app.poll_runtime_events();
        let snapshot = snapshot_for_app(&app, &roots);
        if let Err(error) = invariants::validate(&snapshot) {
            panic!(
                "real-worker endurance invariant failed: {error}; iteration={iteration}; state={}",
                snapshot.digest()
            );
        }
        iteration = iteration.saturating_add(1);
        thread::sleep(Duration::from_millis(1));
    }

    app.shell.runtime.query_state.query.clear();
    app.update_results();
    let settle_deadline = Instant::now() + Duration::from_secs(15);
    loop {
        app.poll_runtime_events();
        let background_busy = app.shell.tabs.iter().any(|tab| {
            tab.index_state.index_in_progress
                || tab.index_state.pending_index_request_id.is_some()
                || tab.search_in_progress
                || tab.pending_request_id.is_some()
        });
        let settled = app.shell.indexing.pending_queue.is_empty()
            && app.shell.indexing.inflight_requests.is_empty()
            && app.shell.indexing.pending_request_id.is_none()
            && !app.shell.indexing.in_progress
            && app.shell.search.pending_request_id().is_none()
            && !app.shell.search.in_progress()
            && app.shell.search.request_routes_for_test().is_empty()
            && !background_busy;
        if settled {
            break;
        }
        assert!(
            Instant::now() < settle_deadline,
            "real-worker endurance did not settle: {}",
            snapshot_for_app(&app, &roots).digest()
        );
        thread::sleep(Duration::from_millis(1));
    }

    eprintln!(
        "stateful real-worker soak complete: duration_secs={duration_secs}; iterations={iteration}; state={}",
        snapshot_for_app(&app, &roots).digest()
    );
    drop(app);
    let _ = fs::remove_dir_all(base);
}

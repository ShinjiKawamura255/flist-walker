use super::*;

#[test]
fn lifecycle_mutation_is_confined_to_owner_transition_apis() {
    let tab_state = include_str!("../tab_state.rs");
    let index_coordinator = include_str!("../index_coordinator.rs");
    assert!(!tab_state.contains("pub(super) lifecycle: TabResourceLifecycle"));
    assert!(!tab_state.contains("pub(super) committed_snapshot_present: bool"));
    assert!(!tab_state.contains("pub(super) resource_state: TabResourceState"));
    assert!(!index_coordinator.contains("pub(super) resource_state: TabResourceState"));
    assert!(!tab_state.contains("impl DerefMut for TabIndexState"));
    assert!(!index_coordinator.contains("impl DerefMut for IndexCoordinator"));

    for (name, source) in [
        ("coordinator.rs", include_str!("../coordinator.rs")),
        ("pipeline.rs", include_str!("../pipeline.rs")),
        ("tabs.rs", include_str!("../tabs.rs")),
        ("tab_resources.rs", include_str!("../tab_resources.rs")),
    ] {
        let compact = source
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .collect::<String>();
        assert!(
            !compact.contains(".resource_state.apply(")
                && !compact.contains(".lifecycle=")
                && !compact.contains(".committed_snapshot_present="),
            "{name} bypasses an owner lifecycle transition API"
        );
    }
}

#[test]
fn tc_207_active_and_inactive_resource_transitions_share_one_reducer() {
    use crate::app::tab_state::{TabResourceState, TabResourceTransition};

    let root = test_root("tc-207-shared-resource-transition-reducer");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let mut inactive = app.capture_active_tab_state(900);
    let lifecycles = [
        TabResourceLifecycle::Dormant,
        TabResourceLifecycle::Loading,
        TabResourceLifecycle::Ready,
        TabResourceLifecycle::Refreshing,
        TabResourceLifecycle::Failed,
        TabResourceLifecycle::Evicted,
    ];

    for lifecycle in lifecycles {
        for committed_snapshot_present in [false, true] {
            let initial = TabResourceState::new(lifecycle, committed_snapshot_present);
            let cases = [
                (
                    TabResourceTransition::Begin,
                    TabResourceState::new(
                        if committed_snapshot_present {
                            TabResourceLifecycle::Refreshing
                        } else {
                            TabResourceLifecycle::Loading
                        },
                        committed_snapshot_present,
                    ),
                ),
                (
                    TabResourceTransition::Success,
                    TabResourceState::new(TabResourceLifecycle::Ready, true),
                ),
                (
                    TabResourceTransition::Failure,
                    TabResourceState::new(TabResourceLifecycle::Failed, committed_snapshot_present),
                ),
                (
                    TabResourceTransition::Cancel,
                    TabResourceState::new(
                        if committed_snapshot_present {
                            TabResourceLifecycle::Ready
                        } else {
                            TabResourceLifecycle::Dormant
                        },
                        committed_snapshot_present,
                    ),
                ),
                (
                    TabResourceTransition::Evict,
                    TabResourceState::new(TabResourceLifecycle::Evicted, false),
                ),
                (
                    TabResourceTransition::Reset,
                    TabResourceState::new(TabResourceLifecycle::Dormant, false),
                ),
                (
                    TabResourceTransition::SnapshotRemoved,
                    TabResourceState::new(lifecycle, false),
                ),
                (
                    TabResourceTransition::SnapshotRestored,
                    TabResourceState::new(lifecycle, true),
                ),
                (
                    TabResourceTransition::Dormant,
                    TabResourceState::new(
                        TabResourceLifecycle::Dormant,
                        committed_snapshot_present,
                    ),
                ),
            ];

            for (transition, expected) in cases {
                app.shell.indexing.set_resource_state_for_test(initial);
                inactive.index_state.set_resource_state_for_test(initial);
                app.shell.indexing.apply_resource_transition(transition);
                inactive.index_state.apply_resource_transition(transition);
                assert_eq!(
                    app.shell.indexing.resource_state(),
                    expected,
                    "active transition {transition:?}"
                );
                assert_eq!(
                    inactive.index_state.resource_state(),
                    app.shell.indexing.resource_state()
                );
            }

            let evicted = TabResourceState::new(TabResourceLifecycle::Evicted, false);
            app.shell.indexing.set_resource_state_for_test(evicted);
            inactive.index_state.set_resource_state_for_test(evicted);
            // A successful reclaim consumes the retired payload and leaves both
            // owners in the post-eviction state; only Full returns ownership.
            assert_eq!(app.shell.indexing.resource_state(), evicted);
            assert_eq!(
                inactive.index_state.resource_state(),
                app.shell.indexing.resource_state()
            );

            app.shell
                .indexing
                .apply_resource_transition(TabResourceTransition::ReclaimFullRollback(initial));
            inactive
                .index_state
                .apply_resource_transition(TabResourceTransition::ReclaimFullRollback(initial));
            assert_eq!(app.shell.indexing.resource_state(), initial);
            assert_eq!(
                inactive.index_state.resource_state(),
                app.shell.indexing.resource_state()
            );
        }
    }

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ctrl_t_creates_new_tab_and_activates_it() {
    let root = test_root("shortcut-ctrl-t-new-tab");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "query".to_string());
    assert_eq!(app.shell.tabs.len(), 1);
    assert_eq!(app.shell.tabs.active_tab, 0);

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::T,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: gui_shortcut_modifiers(false),
        }],
    );

    assert_eq!(app.shell.tabs.len(), 2);
    assert_eq!(app.shell.tabs.active_tab, 1);
    assert!(app.shell.runtime.query_state.query.is_empty());
    assert!(app.shell.runtime.use_filelist);
    assert_eq!(app.shell.tabs.get(1).expect("tab 1").tab_accent, None);
    assert!(app.shell.ui.focus_query_requested);
    assert!(!app.shell.ui.unfocus_query_requested);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_209_meaningful_interaction_protects_large_recent_inactive_snapshot() {
    let root = test_root("tc-209-meaningful-recent-inactive");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.switch_to_tab_index(0);
    app.shell.indexing.pending_request_id = None;
    app.shell.indexing.in_progress = false;
    app.shell.indexing.warm_tab_id = None;
    let (index_tx, index_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = index_tx;
    app.shell.indexing.pending_queue.clear();
    app.shell.indexing.inflight_requests.clear();
    app.shell.indexing.request_tabs.clear();
    {
        let target = app.shell.tabs.get_mut(1).expect("target tab");
        target.index_state.clear_index_request_state();
        target
            .index_state
            .set_lifecycle_for_test(TabResourceLifecycle::Ready);
        target
            .index_state
            .set_committed_snapshot_present_for_test(true);
    }

    let active_id = app.current_tab_id().expect("active tab");
    let retained = Arc::new(Vec::with_capacity(TAB_RESOURCE_CACHE_MAX_WEIGHT + 1));
    app.shell.runtime.all_entries = Arc::clone(&retained);
    app.shell.runtime.entries = retained;
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    app.shell
        .indexing
        .set_committed_snapshot_present_for_test(true);

    let activated_at = Instant::now();
    app.shell.tabs.set_active_tab_index_at(0, activated_at);
    app.mark_query_edited();
    app.switch_to_tab_index_at(1, activated_at + Duration::from_millis(10));

    assert_eq!(app.shell.tabs.recent_inactive_tab_id(), Some(active_id));
    let retained_tab = app.shell.tabs.get(0).expect("retained tab");
    assert_eq!(
        retained_tab.index_state.lifecycle(),
        TabResourceLifecycle::Ready
    );
    assert!(retained_tab.index_state.committed_snapshot_present());
    assert!(retained_tab.heavy_resource_weight() > TAB_RESOURCE_CACHE_MAX_WEIGHT);

    app.switch_to_tab_index_at(0, activated_at + Duration::from_millis(20));
    assert_eq!(app.current_tab_id(), Some(active_id));
    assert_eq!(
        app.shell.runtime.all_entries.capacity(),
        TAB_RESOURCE_CACHE_MAX_WEIGHT + 1
    );
    assert!(
        index_rx.try_recv().is_err(),
        "retained Ready tab must not reindex"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_209_incidental_tab_does_not_replace_recent_inactive() {
    let root = test_root("tc-209-incidental-does-not-replace");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.create_new_tab();
    app.switch_to_tab_index(0);
    app.shell.indexing.pending_request_id = None;
    app.shell.indexing.in_progress = false;
    app.shell.indexing.warm_tab_id = None;
    for index in 1..3 {
        let tab = app.shell.tabs.get_mut(index).expect("inactive tab");
        tab.index_state
            .set_lifecycle_for_test(TabResourceLifecycle::Ready);
        tab.index_state
            .set_committed_snapshot_present_for_test(true);
    }

    let first_id = app.current_tab_id().expect("first tab");
    let incidental_id = app.shell.tabs.get(1).expect("second tab").id;
    let first_entry = file_entry(root.join("first.txt"));
    app.shell.runtime.all_entries = Arc::new(vec![first_entry.clone()]);
    app.shell.runtime.entries = Arc::new(vec![first_entry]);
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    app.shell
        .indexing
        .set_committed_snapshot_present_for_test(true);
    let activated_at = Instant::now();
    app.shell.tabs.set_active_tab_index_at(0, activated_at);
    app.mark_query_edited();
    app.switch_to_tab_index_at(1, activated_at + Duration::from_millis(10));
    assert_eq!(app.shell.tabs.recent_inactive_tab_id(), Some(first_id));

    app.switch_to_tab_index_at(2, activated_at + Duration::from_millis(20));

    assert_ne!(first_id, incidental_id);
    assert_eq!(app.shell.tabs.recent_inactive_tab_id(), Some(first_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_209_recent_inactive_classification_is_orthogonal_to_warm_role() {
    let root = test_root("tc-209-recent-inactive-warm-orthogonal");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.switch_to_tab_index(0);
    let active_id = app.current_tab_id().expect("active tab");
    let entry = file_entry(root.join("partial.txt"));
    app.shell.runtime.all_entries = Arc::new(vec![entry.clone()]);
    app.shell.runtime.entries = Arc::new(vec![entry]);
    app.shell.indexing.pending_request_id = Some(7_001);
    app.shell.indexing.in_progress = true;
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Refreshing);
    app.shell
        .indexing
        .set_committed_snapshot_present_for_test(true);
    {
        let target = app.shell.tabs.get_mut(1).expect("target tab");
        target.index_state.clear_index_request_state();
        target
            .index_state
            .set_lifecycle_for_test(TabResourceLifecycle::Ready);
        target
            .index_state
            .set_committed_snapshot_present_for_test(true);
    }
    app.mark_query_edited();

    app.switch_to_tab_index(1);

    assert_eq!(app.shell.indexing.warm_tab_id, Some(active_id));
    assert_eq!(app.shell.tabs.recent_inactive_tab_id(), Some(active_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_210_active_duration_alone_qualifies_recent_inactive() {
    let root = test_root("tc-210-active-duration-qualifies");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.switch_to_tab_index(0);
    app.shell.indexing.pending_request_id = None;
    app.shell.indexing.in_progress = false;
    app.shell.indexing.warm_tab_id = None;
    let active_id = app.current_tab_id().expect("active tab");
    let entry = file_entry(root.join("retained.txt"));
    app.shell.runtime.all_entries = Arc::new(vec![entry.clone()]);
    app.shell.runtime.entries = Arc::new(vec![entry]);
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    app.shell
        .indexing
        .set_committed_snapshot_present_for_test(true);

    let activated_at = Instant::now();
    app.shell.tabs.set_active_tab_index_at(0, activated_at);
    app.switch_to_tab_index_at(1, activated_at + TAB_RECENT_INACTIVE_ENGAGEMENT_THRESHOLD);

    assert_eq!(app.shell.tabs.recent_inactive_tab_id(), Some(active_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_210_reordering_active_tab_does_not_reset_its_tenure() {
    let root = test_root("tc-210-reorder-keeps-tenure");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.switch_to_tab_index(0);
    app.shell.indexing.pending_request_id = None;
    app.shell.indexing.in_progress = false;
    app.shell.indexing.warm_tab_id = None;
    let active_id = app.current_tab_id().expect("active tab");
    let entry = file_entry(root.join("retained.txt"));
    app.shell.runtime.all_entries = Arc::new(vec![entry.clone()]);
    app.shell.runtime.entries = Arc::new(vec![entry]);
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    app.shell
        .indexing
        .set_committed_snapshot_present_for_test(true);

    let activated_at = Instant::now();
    app.shell.tabs.set_active_tab_index_at(0, activated_at);
    app.move_tab(0, 1);
    app.switch_to_tab_index_at(0, activated_at + TAB_RECENT_INACTIVE_ENGAGEMENT_THRESHOLD);

    assert_eq!(app.shell.tabs.recent_inactive_tab_id(), Some(active_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_210_closing_engaged_active_tab_does_not_replace_existing_recent_inactive() {
    let root = test_root("tc-210-close-keeps-existing-recent");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.create_new_tab();
    app.switch_to_tab_index(0);
    app.shell.indexing.pending_request_id = None;
    app.shell.indexing.in_progress = false;
    app.shell.indexing.warm_tab_id = None;
    for index in 1..3 {
        let tab = app.shell.tabs.get_mut(index).expect("inactive tab");
        tab.index_state.clear_index_request_state();
        tab.index_state
            .set_lifecycle_for_test(TabResourceLifecycle::Ready);
        tab.index_state
            .set_committed_snapshot_present_for_test(true);
    }
    let first_id = app.current_tab_id().expect("first tab");
    let entry = file_entry(root.join("first.txt"));
    app.shell.runtime.all_entries = Arc::new(vec![entry.clone()]);
    app.shell.runtime.entries = Arc::new(vec![entry]);
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    app.shell
        .indexing
        .set_committed_snapshot_present_for_test(true);
    app.mark_query_edited();
    app.switch_to_tab_index(1);
    assert_eq!(app.shell.tabs.recent_inactive_tab_id(), Some(first_id));

    app.mark_query_edited();
    app.close_active_tab();

    assert_eq!(app.shell.tabs.active_tab_index(), 1);
    assert_eq!(app.shell.tabs.recent_inactive_tab_id(), Some(first_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_210_reclaimer_full_rolls_back_recent_and_active_engagement() {
    let root = test_root("tc-210-reclaimer-full-rolls-back-recent");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.create_new_tab();
    app.create_new_tab();
    app.switch_to_tab_index(0);
    app.shell.indexing.pending_request_id = None;
    app.shell.indexing.in_progress = false;
    app.shell.indexing.warm_tab_id = None;

    let large_capacity = TAB_RESOURCE_CACHE_HARD_MAX_WEIGHT / 2 + 1;
    for index in 1..4 {
        let tab = app.shell.tabs.get_mut(index).expect("inactive tab");
        tab.index_state.clear_index_request_state();
        tab.index_state
            .set_lifecycle_for_test(TabResourceLifecycle::Ready);
        tab.index_state
            .set_committed_snapshot_present_for_test(true);
    }
    {
        let second = app.shell.tabs.get_mut(1).expect("second tab");
        let entries = Arc::new(Vec::with_capacity(large_capacity));
        second.result_state.committed.all_entries = Arc::clone(&entries);
        second.result_state.committed.entries = entries;
    }
    let first_id = app.current_tab_id().expect("first tab");
    let second_id = app.shell.tabs.get(1).expect("second tab").id;
    let ordinary_id = app.shell.tabs.get(3).expect("ordinary tab").id;
    let entries = Arc::new(Vec::with_capacity(large_capacity));
    app.shell.runtime.all_entries = Arc::clone(&entries);
    app.shell.runtime.entries = entries;
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    app.shell
        .indexing
        .set_committed_snapshot_present_for_test(true);

    let activated_at = Instant::now();
    app.shell.tabs.set_active_tab_index_at(0, activated_at);
    app.mark_query_edited();
    app.switch_to_tab_index_at(1, activated_at + Duration::from_millis(10));
    assert_eq!(app.shell.tabs.recent_inactive_tab_id(), Some(first_id));

    let ordinary_entry = file_entry(root.join("ordinary.txt"));
    {
        let ordinary = app.shell.tabs.get_mut(3).expect("ordinary tab");
        ordinary.result_state.committed.all_entries = Arc::new(vec![ordinary_entry]);
        ordinary.result_state.committed.entries =
            Arc::clone(&ordinary.result_state.committed.all_entries);
    }
    app.shell.tabs.touch_heavy_resource(ordinary_id);
    app.mark_query_edited();
    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY - 1 {
        let mut queued = app.capture_active_tab_state(8_000 + index as u64);
        let entry = file_entry(root.join(format!("queued-{index}.txt")));
        queued.result_state.committed.all_entries = Arc::new(vec![entry]);
        queued.result_state.committed.entries =
            Arc::clone(&queued.result_state.committed.all_entries);
        app.shell
            .tabs
            .retire_tab_resources_for_test(queued.take_heavy_resources())
            .expect("reserve all but one reclaimer slot");
    }

    app.switch_to_tab_index_at(2, activated_at + Duration::from_millis(20));

    assert_eq!(app.current_tab_id(), Some(second_id));
    assert_eq!(app.shell.tabs.recent_inactive_tab_id(), Some(first_id));
    assert_eq!(
        app.shell
            .tabs
            .get(0)
            .expect("existing recent tab")
            .index_state
            .lifecycle(),
        TabResourceLifecycle::Ready
    );
    assert_eq!(
        app.shell
            .tabs
            .get(3)
            .expect("ordinary tab")
            .index_state
            .lifecycle(),
        TabResourceLifecycle::Evicted
    );
    assert_eq!(
        app.shell.tabs.reclaimer_pending(),
        TAB_RESOURCE_RECLAIMER_CAPACITY
    );

    let small_entries = Arc::new(vec![file_entry(root.join("second-small.txt"))]);
    app.shell.runtime.all_entries = Arc::clone(&small_entries);
    app.shell.runtime.entries = small_entries;
    app.shell.tabs.resume_resource_reclaimer();
    app.switch_to_tab_index_at(2, activated_at + Duration::from_millis(30));

    assert_eq!(app.shell.tabs.recent_inactive_tab_id(), Some(second_id));
    let _ = fs::remove_dir_all(&root);
}

fn recent_rollback_fixture(
    name: &str,
    seed_closed_tab: bool,
) -> (PathBuf, FlistWalkerApp, Instant, u64, u64) {
    let root = test_root(name);
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.create_new_tab();
    if seed_closed_tab {
        app.create_new_tab();
    }
    app.switch_to_tab_index(0);
    app.shell.indexing.pending_request_id = None;
    app.shell.indexing.in_progress = false;
    app.shell.indexing.warm_tab_id = None;
    for index in 1..app.shell.tabs.len() {
        let tab = app.shell.tabs.get_mut(index).expect("inactive tab");
        tab.index_state.clear_index_request_state();
        tab.index_state
            .set_lifecycle_for_test(TabResourceLifecycle::Ready);
        tab.index_state
            .set_committed_snapshot_present_for_test(true);
    }
    if seed_closed_tab {
        app.close_tab_index(app.shell.tabs.len() - 1);
        assert_eq!(app.shell.tabs.closed_tab_count(), 1);
    }

    let large_capacity = TAB_RESOURCE_CACHE_HARD_MAX_WEIGHT / 2 + 1;
    {
        let second = app.shell.tabs.get_mut(1).expect("second tab");
        let entries = Arc::new(Vec::with_capacity(large_capacity));
        second.result_state.committed.all_entries = Arc::clone(&entries);
        second.result_state.committed.entries = entries;
    }
    let first_id = app.current_tab_id().expect("first tab");
    let second_id = app.shell.tabs.get(1).expect("second tab").id;
    let entries = Arc::new(Vec::with_capacity(large_capacity));
    app.shell.runtime.all_entries = Arc::clone(&entries);
    app.shell.runtime.entries = entries;
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    app.shell
        .indexing
        .set_committed_snapshot_present_for_test(true);
    let activated_at = Instant::now();
    app.shell.tabs.set_active_tab_index_at(0, activated_at);
    app.mark_query_edited();
    app.switch_to_tab_index_at(1, activated_at + Duration::from_millis(10));
    assert_eq!(app.shell.tabs.recent_inactive_tab_id(), Some(first_id));
    app.mark_query_edited();
    (root, app, activated_at, first_id, second_id)
}

fn assert_rollback_restored_engagement(
    app: &mut FlistWalkerApp,
    root: &Path,
    activated_at: Instant,
    first_id: u64,
    second_id: u64,
) {
    assert_eq!(app.current_tab_id(), Some(second_id));
    assert_eq!(app.shell.tabs.recent_inactive_tab_id(), Some(first_id));
    let entries = Arc::new(vec![file_entry(root.join("second-small.txt"))]);
    app.shell.runtime.all_entries = Arc::clone(&entries);
    app.shell.runtime.entries = entries;
    app.switch_to_tab_index_at(2, activated_at + Duration::from_millis(30));
    assert_eq!(app.shell.tabs.recent_inactive_tab_id(), Some(second_id));
}

fn fill_paused_reclaimer(app: &mut FlistWalkerApp, root: &Path) {
    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut queued = app.capture_active_tab_state(9_000 + index as u64);
        let entry = file_entry(root.join(format!("full-{index}.txt")));
        queued.result_state.committed.all_entries = Arc::new(vec![entry]);
        queued.result_state.committed.entries =
            Arc::clone(&queued.result_state.committed.all_entries);
        app.shell
            .tabs
            .retire_tab_resources_for_test(queued.take_heavy_resources())
            .expect("fill paused reclaimer");
    }
}

#[test]
fn tc_210_hard_pressure_transition_completes_with_reserved_batch() {
    let (root, mut app, activated_at, first_id, second_id) =
        recent_rollback_fixture("tc-210-hard-transition-reserved-batch", false);

    app.switch_to_tab_index_at(2, activated_at + Duration::from_millis(20));

    assert_ne!(app.current_tab_id(), Some(second_id));
    assert_eq!(app.shell.tabs.recent_inactive_tab_id(), Some(second_id));
    assert_eq!(
        app.shell
            .tabs
            .get(0)
            .expect("superseded recent tab")
            .index_state
            .lifecycle(),
        TabResourceLifecycle::Evicted
    );
    assert_ne!(app.shell.tabs.recent_inactive_tab_id(), Some(first_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_210_failed_create_rolls_back_recent_and_active_engagement() {
    let (root, mut app, activated_at, first_id, second_id) =
        recent_rollback_fixture("tc-210-create-rolls-back-recent", false);
    let tab_count = app.shell.tabs.len();
    fill_paused_reclaimer(&mut app, &root);

    app.create_new_tab();

    assert_eq!(app.shell.tabs.len(), tab_count);
    assert_rollback_restored_engagement(&mut app, &root, activated_at, first_id, second_id);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_210_failed_closed_restore_rolls_back_recent_and_active_engagement() {
    let (root, mut app, activated_at, first_id, second_id) =
        recent_rollback_fixture("tc-210-closed-restore-rolls-back-recent", true);
    fill_paused_reclaimer(&mut app, &root);

    app.restore_recently_closed_tab();

    assert_eq!(app.shell.tabs.closed_tab_count(), 1);
    assert_rollback_restored_engagement(&mut app, &root, activated_at, first_id, second_id);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_210_failed_active_close_rolls_back_recent_and_active_engagement() {
    let (root, mut app, activated_at, first_id, second_id) =
        recent_rollback_fixture("tc-210-active-close-rolls-back-recent", false);
    let tab_count = app.shell.tabs.len();
    fill_paused_reclaimer(&mut app, &root);

    app.close_active_tab();

    assert_eq!(app.shell.tabs.len(), tab_count);
    assert_eq!(app.shell.tabs.closed_tab_count(), 0);
    assert_rollback_restored_engagement(&mut app, &root, activated_at, first_id, second_id);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_211_expired_recent_inactive_becomes_soft_budget_eligible() {
    let root = test_root("tc-211-expired-recent-inactive");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.switch_to_tab_index(0);
    app.shell.indexing.pending_request_id = None;
    app.shell.indexing.in_progress = false;
    app.shell.indexing.warm_tab_id = None;
    let active_id = app.current_tab_id().expect("active tab");
    let retained = Arc::new(Vec::with_capacity(TAB_RESOURCE_CACHE_MAX_WEIGHT + 1));
    app.shell.runtime.all_entries = Arc::clone(&retained);
    app.shell.runtime.entries = retained;
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    app.shell
        .indexing
        .set_committed_snapshot_present_for_test(true);

    let activated_at = Instant::now();
    app.shell.tabs.set_active_tab_index_at(0, activated_at);
    app.mark_query_edited();
    let deactivated_at = activated_at + Duration::from_millis(10);
    app.switch_to_tab_index_at(1, deactivated_at);
    assert_eq!(app.shell.tabs.recent_inactive_tab_id(), Some(active_id));

    assert!(app.shell.tabs.enforce_resource_budget_at(
        app.current_tab_id(),
        None,
        deactivated_at + TAB_RECENT_INACTIVE_GRACE + Duration::from_millis(1),
    ));

    assert_eq!(app.shell.tabs.recent_inactive_tab_id(), None);
    assert_eq!(
        app.shell
            .tabs
            .get(0)
            .expect("expired tab")
            .index_state
            .lifecycle(),
        TabResourceLifecycle::Evicted
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_211_hard_pressure_can_evict_recent_inactive_during_grace() {
    let root = test_root("tc-211-hard-pressure-overrides-grace");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.switch_to_tab_index(0);
    app.shell.indexing.pending_request_id = None;
    app.shell.indexing.in_progress = false;
    app.shell.indexing.warm_tab_id = None;
    let active_id = app.current_tab_id().expect("active tab");
    app.shell.runtime.preview = String::with_capacity(TAB_RESOURCE_CACHE_MAX_WEIGHT + 1);
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    app.shell
        .indexing
        .set_committed_snapshot_present_for_test(true);

    let activated_at = Instant::now();
    app.shell.tabs.set_active_tab_index_at(0, activated_at);
    app.mark_query_edited();
    app.switch_to_tab_index_at(1, activated_at + Duration::from_millis(10));
    assert_eq!(app.shell.tabs.recent_inactive_tab_id(), Some(active_id));
    app.shell
        .tabs
        .get_mut(0)
        .expect("recent tab")
        .result_state
        .committed
        .preview = String::with_capacity(TAB_RESOURCE_CACHE_HARD_MAX_WEIGHT + 1);

    assert!(app.shell.tabs.enforce_resource_budget_at(
        app.current_tab_id(),
        None,
        activated_at + Duration::from_millis(20),
    ));

    assert_eq!(app.shell.tabs.recent_inactive_tab_id(), None);
    assert_eq!(
        app.shell
            .tabs
            .get(0)
            .expect("hard-pressure tab")
            .index_state
            .lifecycle(),
        TabResourceLifecycle::Evicted
    );
    assert_ne!(app.current_tab_id(), Some(active_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_211_hard_count_pressure_can_evict_recent_inactive_during_grace() {
    let root = test_root("tc-211-hard-count-overrides-grace");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    for _ in 0..=TAB_RESOURCE_CACHE_HARD_MAX_COUNT {
        app.create_new_tab();
    }
    app.switch_to_tab_index(0);
    app.shell.indexing.pending_request_id = None;
    app.shell.indexing.in_progress = false;
    app.shell.indexing.warm_tab_id = None;
    for index in 1..app.shell.tabs.len() {
        let tab = app.shell.tabs.get_mut(index).expect("inactive tab");
        tab.index_state.clear_index_request_state();
        tab.index_state
            .set_lifecycle_for_test(TabResourceLifecycle::Ready);
        tab.index_state
            .set_committed_snapshot_present_for_test(true);
    }
    let recent_id = app.current_tab_id().expect("recent tab");
    let entry = file_entry(root.join("recent.txt"));
    app.shell.runtime.all_entries = Arc::new(vec![entry]);
    app.shell.runtime.entries = Arc::clone(&app.shell.runtime.all_entries);
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    app.shell
        .indexing
        .set_committed_snapshot_present_for_test(true);
    app.mark_query_edited();
    let active_index = app.shell.tabs.len() - 1;
    app.switch_to_tab_index(active_index);
    assert_eq!(app.shell.tabs.recent_inactive_tab_id(), Some(recent_id));

    for index in 1..active_index {
        let tab_id = {
            let tab = app.shell.tabs.get_mut(index).expect("in-flight tab");
            let entry = file_entry(root.join(format!("in-flight-{index}.txt")));
            tab.result_state.committed.all_entries = Arc::new(vec![entry]);
            tab.result_state.committed.entries =
                Arc::clone(&tab.result_state.committed.all_entries);
            tab.index_state.build_reclaim_pending = true;
            tab.id
        };
        app.shell.tabs.touch_heavy_resource(tab_id);
    }

    assert!(!app
        .shell
        .tabs
        .enforce_resource_budget_at(app.current_tab_id(), None, Instant::now(),));

    assert_eq!(app.shell.tabs.recent_inactive_tab_id(), None);
    assert_eq!(
        app.shell
            .tabs
            .get(0)
            .expect("hard-count recent tab")
            .index_state
            .lifecycle(),
        TabResourceLifecycle::Evicted
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_open_inactive_snapshots_share_the_bounded_lru() {
    let root = test_root("tc-207-open-lru");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.create_new_tab();
    app.create_new_tab();
    let active_tab_id = app.current_tab_id();
    let inactive_ids = (0..3)
        .map(|index| app.shell.tabs.get(index).expect("inactive tab").id)
        .collect::<Vec<_>>();
    for (index, tab_id) in inactive_ids.iter().copied().enumerate() {
        let entry = file_entry(root.join(format!("cached-{index}.txt")));
        let tab = app.shell.tabs.get_mut(index).expect("inactive tab");
        tab.index_state
            .set_lifecycle_for_test(TabResourceLifecycle::Ready);
        tab.result_state.committed.all_entries = Arc::new(vec![entry.clone()]);
        tab.result_state.committed.entries = Arc::new(vec![entry]);
        app.shell.tabs.touch_heavy_resource(tab_id);
    }

    assert!(app.shell.tabs.enforce_resource_budget(active_tab_id, None));
    assert_eq!(
        app.shell
            .tabs
            .cached_heavy_resource_count(active_tab_id, None),
        TAB_RESOURCE_CACHE_MAX_COUNT
    );
    assert_eq!(
        app.shell
            .tabs
            .get(0)
            .expect("oldest tab")
            .index_state
            .lifecycle(),
        TabResourceLifecycle::Evicted
    );
    assert!(app.shell.tabs.reclaimer_pending() <= TAB_RESOURCE_RECLAIMER_CAPACITY);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_reclaimer_full_returns_heavy_ownership_to_the_caller() {
    let root = test_root("tc-207-reclaimer-full");
    fs::create_dir_all(&root).expect("create root");
    let app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let reclaimer = TabResourceReclaimer::paused_for_test();
    let mut accepted = app.capture_active_tab_state(700);
    accepted
        .index_state
        .set_resource_state_for_test(crate::app::tab_state::TabResourceState::new(
            TabResourceLifecycle::Ready,
            true,
        ));
    accepted.result_state.committed.all_entries =
        Arc::new(vec![file_entry(root.join("queued-0.txt"))]);
    reclaimer
        .try_retire(accepted.take_heavy_resources())
        .expect("reclaimer queue accepts payload");
    assert_eq!(
        accepted.index_state.lifecycle(),
        TabResourceLifecycle::Evicted
    );
    assert!(!accepted.index_state.committed_snapshot_present());

    for index in 1..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut tab = app.capture_active_tab_state(700 + index as u64);
        tab.result_state.committed.all_entries =
            Arc::new(vec![file_entry(root.join(format!("queued-{index}.txt")))]);
        reclaimer
            .try_retire(tab.take_heavy_resources())
            .expect("reclaimer queue accepts capacity");
    }
    let mut overflow = app.capture_active_tab_state(999);
    overflow
        .index_state
        .set_resource_state_for_test(crate::app::tab_state::TabResourceState::new(
            TabResourceLifecycle::Refreshing,
            true,
        ));
    overflow.result_state.committed.all_entries =
        Arc::new(vec![file_entry(root.join("overflow.txt"))]);
    let returned = reclaimer
        .try_retire(overflow.take_heavy_resources())
        .expect_err("full reclaimer must return ownership");
    overflow.restore_heavy_resources(*returned);
    assert_eq!(overflow.result_state.committed.all_entries.len(), 1);
    assert_eq!(
        overflow.index_state.lifecycle(),
        TabResourceLifecycle::Refreshing
    );
    assert!(overflow.index_state.committed_snapshot_present());
    assert_eq!(reclaimer.pending(), TAB_RESOURCE_RECLAIMER_CAPACITY);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_210_reserved_reclaimer_slot_cannot_be_stolen_by_normal_retirement() {
    let root = test_root("tc-210-reserved-reclaimer-slot");
    fs::create_dir_all(&root).expect("create root");
    let app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let reclaimer = TabResourceReclaimer::paused_for_test();
    assert!(reclaimer.try_reserve_slot());

    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY - 1 {
        let mut tab = app.capture_active_tab_state(10_000 + index as u64);
        let entry = file_entry(root.join(format!("queued-{index}.txt")));
        tab.result_state.committed.all_entries = Arc::new(vec![entry]);
        tab.result_state.committed.entries = Arc::clone(&tab.result_state.committed.all_entries);
        reclaimer
            .try_retire(tab.take_heavy_resources())
            .expect("unreserved queue slot remains available");
    }

    let mut rejected = app.capture_active_tab_state(11_000);
    rejected.result_state.committed.all_entries =
        Arc::new(vec![file_entry(root.join("rejected.txt"))]);
    rejected.result_state.committed.entries =
        Arc::clone(&rejected.result_state.committed.all_entries);
    let rejected = reclaimer
        .try_retire(rejected.take_heavy_resources())
        .expect_err("normal retirement must not consume the reserved slot");

    let mut reserved = app.capture_active_tab_state(12_000);
    reserved.result_state.committed.all_entries =
        Arc::new(vec![file_entry(root.join("reserved.txt"))]);
    reserved.result_state.committed.entries =
        Arc::clone(&reserved.result_state.committed.all_entries);
    reclaimer
        .try_retire_reserved_tabs(vec![reserved.take_heavy_resources()])
        .expect("reserved batch must use its pre-admitted slot");

    assert_eq!(reclaimer.pending(), TAB_RESOURCE_RECLAIMER_CAPACITY);
    drop(rejected);

    let mut disconnected = TabResourceReclaimer::paused_for_test();
    assert!(disconnected.try_reserve_slot());
    disconnected.disconnect();
    let mut returned = app.capture_active_tab_state(13_000);
    returned.result_state.committed.all_entries =
        Arc::new(vec![file_entry(root.join("disconnected.txt"))]);
    returned.result_state.committed.entries =
        Arc::clone(&returned.result_state.committed.all_entries);
    let returned = disconnected
        .try_retire_reserved_tabs(vec![returned.take_heavy_resources()])
        .expect_err("disconnected reserved send returns ownership");
    assert!(
        disconnected.try_reserve_slot(),
        "failed reserved send must release its admission slot"
    );
    disconnected.release_reserved_slot();
    drop(returned);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_reclaimer_full_repeated_close_keeps_history_and_heavy_cache_bounded() {
    let root = test_root("tc-207-reclaimer-full-repeated-close");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut tab = app.capture_active_tab_state(1_000 + index as u64);
        tab.result_state.committed.all_entries =
            Arc::new(vec![file_entry(root.join(format!("queued-{index}.txt")))]);
        tab.index_state
            .set_committed_snapshot_present_for_test(true);
        app.shell
            .tabs
            .retire_tab_resources_for_test(tab.take_heavy_resources())
            .expect("fill reclaimer");
    }

    for index in 0..12 {
        app.create_new_tab();
        app.shell.runtime.all_entries =
            Arc::new(vec![file_entry(root.join(format!("closed-{index}.txt")))]);
        app.shell.runtime.entries = Arc::clone(&app.shell.runtime.all_entries);
        app.shell
            .indexing
            .set_committed_snapshot_present_for_test(true);
        app.shell
            .indexing
            .set_lifecycle_for_test(TabResourceLifecycle::Ready);
        app.close_active_tab();
    }

    let active_tab_id = app.current_tab_id();
    assert!(app.shell.tabs.closed_tab_count() <= 25);
    assert!(
        app.shell
            .tabs
            .cached_heavy_resource_count(active_tab_id, app.shell.indexing.warm_tab_id)
            <= TAB_RESOURCE_CACHE_MAX_COUNT
    );
    assert_eq!(
        app.shell.tabs.reclaimer_pending(),
        TAB_RESOURCE_RECLAIMER_CAPACITY
    );
    assert!(app
        .shell
        .runtime
        .notice
        .contains("Waiting for background tab resource reclamation"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_closed_history_stays_exactly_25_when_oldest_heavy_retirement_is_full() {
    let root = test_root("tc-207-closed-history-full-boundary");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    for index in 0..25 {
        app.create_new_tab();
        app.shell.runtime.query_state.query = format!("closed-{index}");
        app.close_active_tab();
    }
    assert_eq!(app.shell.tabs.closed_tab_count(), 25);
    app.shell
        .tabs
        .seed_oldest_closed_snapshot(file_entry(root.join("oldest-heavy.txt")));
    app.create_new_tab();
    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut tab = app.capture_active_tab_state(2_000 + index as u64);
        tab.result_state.committed.all_entries =
            Arc::new(vec![file_entry(root.join(format!("held-{index}.txt")))]);
        app.shell
            .tabs
            .retire_tab_resources_for_test(tab.take_heavy_resources())
            .expect("fill reclaimer");
    }

    app.close_active_tab();

    assert_eq!(app.shell.tabs.len(), 2);
    assert_eq!(app.shell.tabs.closed_tab_count(), 25);
    assert!(app
        .shell
        .runtime
        .notice
        .contains("Waiting for background tab resource reclamation"));

    app.shell.tabs.resume_resource_reclaimer();
    app.close_active_tab();

    assert_eq!(app.shell.tabs.len(), 1);
    assert_eq!(app.shell.tabs.closed_tab_count(), 25);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_eviction_keeps_pin_and_selected_path_as_lightweight_intent() {
    let root = test_root("tc-207-lightweight-selection-intent");
    fs::create_dir_all(&root).expect("create root");
    let selected = root.join("selected.txt");
    let pinned = root.join("pinned.txt");
    let app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let mut tab = app.capture_active_tab_state(2_000);
    tab.result_state.committed.results = vec![(selected.clone(), 1.0)];
    tab.result_state.committed.current_row = Some(0);
    tab.result_state.pinned_paths.insert(pinned.clone());

    let _retired = tab.take_heavy_resources();

    assert!(tab.result_state.pinned_paths.contains(&pinned));
    assert_eq!(
        tab.result_state.evicted_selected_path.as_ref(),
        Some(&selected)
    );
    assert_eq!(tab.index_state.lifecycle(), TabResourceLifecycle::Evicted);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_empty_high_capacity_snapshot_is_accounted_and_reclaimed_off_ui() {
    let root = test_root("tc-207-empty-high-capacity");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let mut tab = app.capture_active_tab_state(2_200);
    let retained_entries = Vec::with_capacity(8_192);
    let retained_entries = Arc::new(retained_entries);
    tab.index_state
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    tab.index_state
        .set_committed_snapshot_present_for_test(true);
    tab.result_state.committed.all_entries = Arc::clone(&retained_entries);
    tab.result_state.committed.entries = retained_entries;
    tab.result_state.committed.results = Vec::with_capacity(4_096);

    assert!(tab.heavy_resource_weight() >= 12_288);
    let resources = tab.take_heavy_resources();
    assert!(!resources.is_empty());
    app.shell.tabs.pause_resource_reclaimer();
    app.shell
        .tabs
        .retire_tab_resources_for_test(resources)
        .expect("retained allocation must enter reclaimer");
    assert_eq!(app.shell.tabs.reclaimer_pending(), 1);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_empty_high_capacity_snapshots_obey_cache_count_limit() {
    let root = test_root("tc-207-empty-high-capacity-cache");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    for _ in 0..3 {
        app.create_new_tab();
    }
    for index in 0..3 {
        let tab_id = app.shell.tabs.get(index).expect("cached tab").id;
        let retained = Arc::new(Vec::with_capacity(4_096));
        let tab = app.shell.tabs.get_mut(index).expect("cached tab");
        tab.index_state
            .set_lifecycle_for_test(TabResourceLifecycle::Ready);
        tab.index_state
            .set_committed_snapshot_present_for_test(true);
        tab.result_state.committed.all_entries = Arc::clone(&retained);
        tab.result_state.committed.entries = retained;
        app.shell.tabs.touch_heavy_resource(tab_id);
    }
    app.shell.tabs.pause_resource_reclaimer();

    assert!(app
        .shell
        .tabs
        .enforce_resource_budget(app.current_tab_id(), None));
    assert_eq!(
        app.shell
            .tabs
            .cached_heavy_resource_count(app.current_tab_id(), None),
        TAB_RESOURCE_CACHE_MAX_COUNT
    );
    assert_eq!(app.shell.tabs.reclaimer_pending(), 1);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_deferred_root_is_a_barrier_for_refresh_close_and_restore() {
    let root_a = test_root("tc-207-deferred-root-close-a");
    let root_b = test_root("tc-207-deferred-root-close-b");
    for root in [&root_a, &root_b] {
        fs::create_dir_all(root).expect("create root");
    }
    let old = file_entry(root_a.join("old.txt"));
    let mut app = FlistWalkerApp::new(root_a.clone(), 50, String::new());
    app.create_new_tab();
    let (request_tx, request_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = request_tx;
    app.shell.indexing.pending_queue.clear();
    app.shell.indexing.inflight_requests.clear();
    app.shell.indexing.request_tabs.clear();
    app.shell.runtime.all_entries = Arc::new(vec![old.clone()]);
    app.shell.runtime.entries = Arc::new(vec![old.clone()]);
    app.shell.runtime.results = vec![(old.path.clone(), 0.0)];
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    app.shell
        .indexing
        .set_committed_snapshot_present_for_test(true);
    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut tab = app.capture_active_tab_state(2_300 + index as u64);
        tab.result_state.committed.all_entries =
            Arc::new(vec![file_entry(root_a.join(format!("held-{index}.txt")))]);
        app.shell
            .tabs
            .retire_tab_resources_for_test(tab.take_heavy_resources())
            .expect("fill reclaimer");
    }

    app.apply_root_change_direct(root_b.clone());
    app.request_index_refresh();
    app.close_active_tab();

    assert_eq!(app.shell.tabs.len(), 2);
    assert_eq!(path_key(&app.shell.runtime.root), path_key(&root_a));
    assert_eq!(app.shell.runtime.all_entries.as_ref(), &[old]);
    assert_eq!(
        app.shell.indexing.root_after_pending_finish.as_ref(),
        Some(&root_b)
    );
    assert_eq!(
        app.shell.indexing.refresh_after_pending_finish,
        Some(super::PendingIndexRefreshMode::Normal)
    );
    assert!(request_rx.try_recv().is_err());

    app.shell.tabs.resume_resource_reclaimer();
    app.close_active_tab();
    assert_eq!(app.shell.tabs.len(), 1);
    app.restore_recently_closed_tab();

    assert_eq!(path_key(&app.shell.runtime.root), path_key(&root_b));
    assert!(app.shell.indexing.root_after_pending_finish.is_none());
    let request = request_rx.try_recv().expect("one target-root request");
    assert_eq!(path_key(&request.root), path_key(&root_b));
    assert!(request_rx.try_recv().is_err());
    for root in [&root_a, &root_b] {
        let _ = fs::remove_dir_all(root);
    }
}

#[test]
fn tc_207_inactive_deferred_root_survives_background_refresh_and_close() {
    let root_a = test_root("tc-207-inactive-deferred-root-a");
    let root_b = test_root("tc-207-inactive-deferred-root-b");
    for root in [&root_a, &root_b] {
        fs::create_dir_all(root).expect("create root");
    }
    let mut app = FlistWalkerApp::new(root_a.clone(), 50, String::new());
    app.create_new_tab();
    let target_index = 0;
    let target_id = app.shell.tabs.get(target_index).expect("target tab").id;
    {
        let target = app.shell.tabs.get_mut(target_index).expect("target tab");
        let old = file_entry(root_a.join("old.txt"));
        target
            .index_state
            .set_lifecycle_for_test(TabResourceLifecycle::Ready);
        target
            .index_state
            .set_committed_snapshot_present_for_test(true);
        target.result_state.committed.all_entries = Arc::new(vec![old.clone()]);
        target.result_state.committed.entries = Arc::new(vec![old]);
        target.index_state.root_after_pending_finish = Some(root_b.clone());
        target.index_state.refresh_after_pending_finish =
            Some(super::PendingIndexRefreshMode::Normal);
    }
    app.shell.tabs.touch_heavy_resource(target_id);
    let (request_tx, request_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = request_tx;
    app.shell.indexing.pending_queue.clear();
    app.shell.indexing.inflight_requests.clear();
    app.shell.indexing.request_tabs.clear();
    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut tab = app.capture_active_tab_state(2_500 + index as u64);
        tab.result_state.committed.all_entries =
            Arc::new(vec![file_entry(root_a.join(format!("held-{index}.txt")))]);
        app.shell
            .tabs
            .retire_tab_resources_for_test(tab.take_heavy_resources())
            .expect("fill reclaimer");
    }

    app.request_background_index_refresh_for_tab(target_index);
    app.close_tab_index(target_index);

    assert_eq!(app.shell.tabs.len(), 2);
    let target = app.shell.tabs.get(target_index).expect("target tab");
    assert_eq!(path_key(&target.root), path_key(&root_a));
    assert_eq!(
        target.index_state.root_after_pending_finish.as_ref(),
        Some(&root_b)
    );
    assert!(request_rx.try_recv().is_err());

    app.shell.tabs.resume_resource_reclaimer();
    app.close_tab_index(target_index);
    assert_eq!(app.shell.tabs.len(), 1);
    app.restore_recently_closed_tab();

    let restored_id = app.current_tab_id().expect("restored tab id");
    assert_ne!(restored_id, target_id);
    assert_eq!(path_key(&app.shell.runtime.root), path_key(&root_b));
    let request = request_rx
        .try_recv()
        .expect("one restored target-root request");
    assert_eq!(request.tab_id, restored_id);
    assert_eq!(path_key(&request.root), path_key(&root_b));
    assert!(request_rx.try_recv().is_err());
    for root in [&root_a, &root_b] {
        let _ = fs::remove_dir_all(root);
    }
}

#[test]
fn tc_207_terminal_settlement_preserves_independent_root_debt() {
    let root_a = test_root("tc-207-root-debt-settlement-a");
    let root_b = test_root("tc-207-root-debt-settlement-b");
    fs::create_dir_all(&root_a).expect("create root");
    let mut app = FlistWalkerApp::new(root_a.clone(), 50, String::new());
    let tab_id = app.current_tab_id();
    let request_id = app.shell.indexing.allocate_request_id(tab_id);
    app.shell.indexing.pending_request_id = Some(request_id);
    app.shell.indexing.root_after_pending_finish = Some(root_b.clone());
    app.shell.indexing.refresh_after_pending_finish = Some(super::PendingIndexRefreshMode::Normal);

    app.shell.indexing.complete_active_request(request_id);

    assert_eq!(
        app.shell.indexing.root_after_pending_finish.as_ref(),
        Some(&root_b)
    );
    assert_eq!(
        app.shell.indexing.refresh_after_pending_finish,
        Some(super::PendingIndexRefreshMode::Normal)
    );
    let _ = fs::remove_dir_all(&root_a);
}

#[test]
fn tc_207_closing_sole_warm_preflights_it_as_unpinned_cache() {
    let root = test_root("tc-207-close-warm-budget");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    for _ in 0..3 {
        app.create_new_tab();
    }
    app.switch_to_tab_index(0);
    let warm_index = 3;
    let warm_id = app.shell.tabs.get(warm_index).expect("warm tab").id;
    app.shell.indexing.warm_tab_id = Some(warm_id);
    for index in 1..=warm_index {
        let tab_id = app.shell.tabs.get(index).expect("cached tab").id;
        let entry = file_entry(root.join(format!("cached-{index}.txt")));
        let tab = app.shell.tabs.get_mut(index).expect("cached tab");
        tab.index_state
            .set_lifecycle_for_test(TabResourceLifecycle::Ready);
        tab.index_state
            .set_committed_snapshot_present_for_test(true);
        tab.result_state.committed.all_entries = Arc::new(vec![entry.clone()]);
        tab.result_state.committed.entries = Arc::new(vec![entry]);
        app.shell.tabs.touch_heavy_resource(tab_id);
    }
    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut tab = app.capture_active_tab_state(2_400 + index as u64);
        tab.result_state.committed.all_entries =
            Arc::new(vec![file_entry(root.join(format!("held-{index}.txt")))]);
        app.shell
            .tabs
            .retire_tab_resources_for_test(tab.take_heavy_resources())
            .expect("fill reclaimer");
    }

    app.close_tab_index(warm_index);

    assert_eq!(app.shell.tabs.len(), 4);
    assert_eq!(app.shell.tabs.closed_tab_count(), 0);
    assert_eq!(app.shell.indexing.warm_tab_id, Some(warm_id));
    assert!(app
        .shell
        .runtime
        .notice
        .contains("Waiting for background tab resource reclamation"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_switch_to_light_tab_rolls_back_before_cache_overflow() {
    let root = test_root("tc-207-switch-role-preflight");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    for _ in 0..3 {
        app.create_new_tab();
    }
    app.switch_to_tab_index(0);
    app.shell.indexing.pending_request_id = None;
    app.shell.indexing.in_progress = false;
    app.shell.indexing.warm_tab_id = None;
    let active_id = app.current_tab_id();
    let active = file_entry(root.join("active.txt"));
    app.shell.runtime.all_entries = Arc::new(vec![active.clone()]);
    app.shell.runtime.entries = Arc::new(vec![active]);
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    app.shell
        .indexing
        .set_committed_snapshot_present_for_test(true);
    for index in 2..=3 {
        let tab_id = app.shell.tabs.get(index).expect("cached tab").id;
        let entry = file_entry(root.join(format!("cached-{index}.txt")));
        let tab = app.shell.tabs.get_mut(index).expect("cached tab");
        tab.index_state
            .set_lifecycle_for_test(TabResourceLifecycle::Ready);
        tab.index_state
            .set_committed_snapshot_present_for_test(true);
        tab.result_state.committed.all_entries = Arc::new(vec![entry.clone()]);
        tab.result_state.committed.entries = Arc::new(vec![entry]);
        app.shell.tabs.touch_heavy_resource(tab_id);
    }
    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut tab = app.capture_active_tab_state(2_600 + index as u64);
        tab.result_state.committed.all_entries =
            Arc::new(vec![file_entry(root.join(format!("held-{index}.txt")))]);
        app.shell
            .tabs
            .retire_tab_resources_for_test(tab.take_heavy_resources())
            .expect("fill reclaimer");
    }

    app.switch_to_tab_index(1);

    assert_eq!(app.current_tab_id(), active_id);
    assert_eq!(app.shell.tabs.active_tab_index(), 0);
    assert_eq!(
        app.shell
            .tabs
            .cached_heavy_resource_count(active_id, app.shell.indexing.warm_tab_id),
        TAB_RESOURCE_CACHE_MAX_COUNT
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_restore_light_closed_tab_rolls_back_before_cache_overflow() {
    let root = test_root("tc-207-restore-role-preflight");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.close_active_tab();
    for _ in 0..2 {
        app.create_new_tab();
    }
    let active_id = app.current_tab_id();
    let active = file_entry(root.join("active.txt"));
    app.shell.runtime.all_entries = Arc::new(vec![active.clone()]);
    app.shell.runtime.entries = Arc::new(vec![active]);
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    app.shell
        .indexing
        .set_committed_snapshot_present_for_test(true);
    for index in 0..2 {
        let tab_id = app.shell.tabs.get(index).expect("cached tab").id;
        let entry = file_entry(root.join(format!("cached-{index}.txt")));
        let tab = app.shell.tabs.get_mut(index).expect("cached tab");
        tab.index_state
            .set_lifecycle_for_test(TabResourceLifecycle::Ready);
        tab.index_state
            .set_committed_snapshot_present_for_test(true);
        tab.result_state.committed.all_entries = Arc::new(vec![entry.clone()]);
        tab.result_state.committed.entries = Arc::new(vec![entry]);
        app.shell.tabs.touch_heavy_resource(tab_id);
    }
    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut tab = app.capture_active_tab_state(2_700 + index as u64);
        tab.result_state.committed.all_entries =
            Arc::new(vec![file_entry(root.join(format!("held-{index}.txt")))]);
        app.shell
            .tabs
            .retire_tab_resources_for_test(tab.take_heavy_resources())
            .expect("fill reclaimer");
    }

    app.restore_recently_closed_tab();

    assert_eq!(app.current_tab_id(), active_id);
    assert_eq!(app.shell.tabs.len(), 3);
    assert_eq!(app.shell.tabs.closed_tab_count(), 1);
    assert_eq!(
        app.shell
            .tabs
            .cached_heavy_resource_count(active_id, app.shell.indexing.warm_tab_id),
        TAB_RESOURCE_CACHE_MAX_COUNT
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_reindexed_results_restore_the_evicted_selected_path() {
    let root = test_root("tc-207-restore-selected-path");
    fs::create_dir_all(&root).expect("create root");
    let first = root.join("first.txt");
    let selected = root.join("selected.txt");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.results = vec![(selected.clone(), 1.0)];
    app.shell.runtime.base_results = app.shell.runtime.results.clone();
    app.shell.runtime.current_row = Some(0);

    let retired = app.take_active_committed_resources();
    drop(retired);
    app.apply_results_with_selection_policy(
        vec![(first, 2.0), (selected.clone(), 1.0)],
        false,
        false,
    );

    assert_eq!(app.shell.runtime.current_row, Some(1));
    assert_eq!(app.shell.runtime.results[1].0, selected);
    assert!(app.shell.runtime.evicted_selected_path.is_none());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_new_tab_is_refused_before_it_can_exceed_the_heavy_cache_bound() {
    let root = test_root("tc-207-new-tab-admission");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.create_new_tab();
    for index in 0..2 {
        let tab_id = app.shell.tabs.get(index).expect("inactive tab").id;
        let entry = file_entry(root.join(format!("inactive-{index}.txt")));
        let tab = app.shell.tabs.get_mut(index).expect("inactive tab");
        tab.index_state
            .set_lifecycle_for_test(TabResourceLifecycle::Ready);
        tab.index_state
            .set_committed_snapshot_present_for_test(true);
        tab.result_state.committed.all_entries = Arc::new(vec![entry.clone()]);
        tab.result_state.committed.entries = Arc::new(vec![entry]);
        app.shell.tabs.touch_heavy_resource(tab_id);
    }
    let active = file_entry(root.join("active.txt"));
    app.shell.runtime.all_entries = Arc::new(vec![active.clone()]);
    app.shell.runtime.entries = Arc::new(vec![active]);
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    app.shell
        .indexing
        .set_committed_snapshot_present_for_test(true);
    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut tab = app.capture_active_tab_state(1_500 + index as u64);
        tab.result_state.committed.all_entries =
            Arc::new(vec![file_entry(root.join(format!("held-{index}.txt")))]);
        app.shell
            .tabs
            .retire_tab_resources_for_test(tab.take_heavy_resources())
            .expect("fill reclaimer");
    }
    let tab_count = app.shell.tabs.len();

    app.create_new_tab();

    assert_eq!(app.shell.tabs.len(), tab_count);
    assert!(app
        .shell
        .runtime
        .notice
        .contains("Waiting for background tab resource reclamation"));
    assert_eq!(
        app.shell
            .tabs
            .cached_heavy_resource_count(app.current_tab_id(), None),
        TAB_RESOURCE_CACHE_MAX_COUNT
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_background_admission_retries_once_after_reclaimer_debt_clears() {
    let root = test_root("tc-207-background-admission-retry");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    for _ in 0..4 {
        app.create_new_tab();
    }
    let target_index = 3;
    let target_id = app.shell.tabs.get(target_index).expect("target tab").id;
    for index in 0..3 {
        let tab_id = app.shell.tabs.get(index).expect("cached tab").id;
        let entry = file_entry(root.join(format!("cached-{index}.txt")));
        let tab = app.shell.tabs.get_mut(index).expect("cached tab");
        tab.index_state
            .set_lifecycle_for_test(TabResourceLifecycle::Ready);
        tab.index_state
            .set_committed_snapshot_present_for_test(true);
        tab.result_state.committed.all_entries = Arc::new(vec![entry.clone()]);
        tab.result_state.committed.entries = Arc::new(vec![entry]);
        app.shell.tabs.touch_heavy_resource(tab_id);
    }
    let (request_tx, request_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = request_tx;
    app.shell.indexing.pending_queue.clear();
    app.shell.indexing.inflight_requests.clear();
    app.shell.indexing.request_tabs.clear();
    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut tab = app.capture_active_tab_state(1_800 + index as u64);
        tab.result_state.committed.all_entries =
            Arc::new(vec![file_entry(root.join(format!("held-{index}.txt")))]);
        app.shell
            .tabs
            .retire_tab_resources_for_test(tab.take_heavy_resources())
            .expect("fill reclaimer");
    }

    app.request_background_index_refresh_for_tab(target_index);

    assert!(request_rx.try_recv().is_err());
    assert_eq!(
        app.shell
            .tabs
            .get(target_index)
            .expect("target tab")
            .index_state
            .refresh_after_pending_finish,
        Some(super::PendingIndexRefreshMode::Normal)
    );

    app.shell.tabs.resume_resource_reclaimer();
    app.poll_index_response();

    let replay = request_rx.try_recv().expect("deferred background request");
    assert_eq!(replay.tab_id, target_id);
    assert!(request_rx.try_recv().is_err());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_root_change_moves_heavy_snapshot_to_reclaimer_before_mutation() {
    let root_a = test_root("tc-207-root-retire-a");
    let root_b = test_root("tc-207-root-retire-b");
    for root in [&root_a, &root_b] {
        fs::create_dir_all(root).expect("create root");
    }
    let old = file_entry(root_a.join("old.txt"));
    let mut app = FlistWalkerApp::new(root_a.clone(), 50, String::new());
    let (request_tx, request_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = request_tx;
    app.shell.indexing.pending_queue.clear();
    app.shell.indexing.inflight_requests.clear();
    app.shell.indexing.request_tabs.clear();
    app.shell.runtime.all_entries = Arc::new(vec![old.clone()]);
    app.shell.runtime.entries = Arc::new(vec![old.clone()]);
    app.shell.runtime.results = vec![(old.path, 0.0)];
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    app.shell
        .indexing
        .set_committed_snapshot_present_for_test(true);
    app.shell.tabs.pause_resource_reclaimer();

    app.apply_root_change_direct(root_b.clone());

    assert_eq!(path_key(&app.shell.runtime.root), path_key(&root_b));
    assert!(!app.shell.indexing.committed_snapshot_present());
    assert!(app.shell.runtime.all_entries.is_empty());
    assert_eq!(app.shell.tabs.reclaimer_pending(), 1);
    let request = request_rx.try_recv().expect("new-root request");
    assert_eq!(path_key(&request.root), path_key(&root_b));
    for root in [&root_a, &root_b] {
        let _ = fs::remove_dir_all(root);
    }
}

#[test]
fn tc_204_ready_activation_reuses_snapshot_but_evicted_activation_requests_once() {
    let root = test_root("tc-204-activation-lifecycle");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    let (index_tx, index_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = index_tx;
    app.shell.indexing.pending_queue.clear();
    app.shell.indexing.inflight_requests.clear();
    app.shell.indexing.pending_request_id = None;
    let target_id = app.shell.tabs.get(0).expect("target tab").id;
    {
        let target = app.shell.tabs.get_mut(0).expect("target tab");
        target.index_state.pending_index_request_id = None;
        target.index_state.index_in_progress = false;
        target
            .index_state
            .set_lifecycle_for_test(TabResourceLifecycle::Ready);
        target
            .index_state
            .set_committed_snapshot_present_for_test(true);
    }

    app.switch_to_tab_index(0);
    assert_eq!(app.shell.indexing.lifecycle(), TabResourceLifecycle::Ready);
    assert_eq!(app.shell.indexing.tx.load().queued, 0);

    app.shell
        .tabs
        .get_mut(1)
        .expect("other tab")
        .index_state
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    app.switch_to_tab_index(1);
    {
        let target = app.shell.tabs.get_mut(0).expect("target tab");
        target.index_state.pending_index_request_id = None;
        target.index_state.index_in_progress = false;
        target
            .index_state
            .set_lifecycle_for_test(TabResourceLifecycle::Evicted);
        target
            .index_state
            .set_committed_snapshot_present_for_test(false);
    }
    app.switch_to_tab_index(0);
    assert_eq!(
        app.shell.indexing.lifecycle(),
        TabResourceLifecycle::Loading
    );
    assert_eq!(app.shell.indexing.tx.load().queued, 1);
    let request = index_rx.try_recv().expect("one evicted refresh request");
    assert_eq!(request.tab_id, target_id);
    assert!(
        index_rx.try_recv().is_err(),
        "activation must not duplicate the request"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_warm_tab_is_pinned_outside_the_inactive_cache_budget() {
    let root = test_root("tc-207-warm-tab-budget");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.create_new_tab();
    app.create_new_tab();
    let active_tab_id = app.current_tab_id();
    let warm_tab_id = app.shell.tabs.get(0).expect("warm tab").id;
    for index in 0..3 {
        let tab_id = app.shell.tabs.get(index).expect("inactive tab").id;
        let entry = file_entry(root.join(format!("cached-{index}.txt")));
        let tab = app.shell.tabs.get_mut(index).expect("inactive tab");
        tab.index_state
            .set_lifecycle_for_test(TabResourceLifecycle::Ready);
        tab.index_state
            .set_committed_snapshot_present_for_test(true);
        tab.result_state.committed.all_entries = Arc::new(vec![entry.clone()]);
        tab.result_state.committed.entries = Arc::new(vec![entry]);
        app.shell.tabs.touch_heavy_resource(tab_id);
    }

    assert!(app
        .shell
        .tabs
        .enforce_resource_budget(active_tab_id, Some(warm_tab_id)));
    assert_eq!(
        app.shell
            .tabs
            .cached_heavy_resource_count(active_tab_id, Some(warm_tab_id)),
        TAB_RESOURCE_CACHE_MAX_COUNT
    );
    assert!(app.shell.tabs.iter().take(3).all(|tab| {
        tab.index_state.lifecycle() == TabResourceLifecycle::Ready
            && tab.index_state.committed_snapshot_present()
    }));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn create_new_tab_resets_total_match_count_to_current_entries() {
    let root = test_root("new-tab-total-count");
    fs::create_dir_all(&root).expect("create dir");
    let first = root.join("first.txt");
    let second = root.join("second.txt");
    fs::write(&first, "a").expect("write first");
    fs::write(&second, "b").expect("write second");

    let mut app = FlistWalkerApp::new(root.clone(), 1, "previous".to_string());
    app.shell.runtime.entries = Arc::new(vec![file_entry(first), file_entry(second)]);
    app.shell.runtime.total_match_count = 99;

    app.create_new_tab();

    assert!(app.shell.runtime.query_state.query.is_empty());
    assert_eq!(app.shell.runtime.results.len(), 1);
    assert_eq!(app.shell.runtime.total_match_count, 2);
    assert!(app.status_line_text().contains("Results: 1 of 2 shown"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ctrl_w_closes_current_tab_and_keeps_last_tab() {
    let root = test_root("shortcut-ctrl-w-close-tab");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    assert_eq!(app.shell.tabs.len(), 2);
    app.shell.ui.focus_query_requested = false;
    app.shell.ui.unfocus_query_requested = true;

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::W,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: gui_shortcut_modifiers(false),
        }],
    );
    assert_eq!(app.shell.tabs.len(), 1);
    assert!(!app.shell.ui.focus_query_requested);
    assert!(app.shell.ui.unfocus_query_requested);

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::W,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: gui_shortcut_modifiers(false),
        }],
    );
    assert_eq!(app.shell.tabs.len(), 1);
    assert!(app
        .shell
        .runtime
        .notice
        .contains("Cannot close the last tab"));
    let _ = fs::remove_dir_all(&root);
}

#[cfg(not(target_os = "macos"))]
#[test]
fn regression_ctrl_w_keeps_tab_open_for_opted_in_focused_query_editing() {
    let root = test_root("regression-ctrl-w-focused-query-editing");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "alpha beta".to_string());
    app.create_new_tab();
    app.shell.runtime.emacs_keybindings_enabled = true;
    app.shell.runtime.ctrl_w_deletes_word_in_query = true;

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::W,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: emacs_shortcut_modifiers(false),
        }],
    );

    assert_eq!(app.shell.tabs.len(), 2);
    let _ = fs::remove_dir_all(&root);
}

#[cfg(not(target_os = "macos"))]
#[test]
fn regression_ctrl_w_still_closes_tab_outside_opted_in_query_editing() {
    for (name, emacs_enabled, option_enabled, query_focused) in [
        ("option-disabled", true, false, true),
        ("emacs-disabled", false, true, true),
        ("query-unfocused", true, true, false),
    ] {
        let root = test_root(&format!("regression-ctrl-w-{name}"));
        fs::create_dir_all(&root).expect("create dir");
        let mut app = FlistWalkerApp::new(root.clone(), 50, "alpha beta".to_string());
        app.create_new_tab();
        app.shell.runtime.emacs_keybindings_enabled = emacs_enabled;
        app.shell.runtime.ctrl_w_deletes_word_in_query = option_enabled;

        run_shortcuts_frame(
            &mut app,
            query_focused,
            vec![egui::Event::Key {
                key: egui::Key::W,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: emacs_shortcut_modifiers(false),
            }],
        );

        assert_eq!(app.shell.tabs.len(), 1, "case: {name}");
        let _ = fs::remove_dir_all(&root);
    }
}

#[cfg(not(target_os = "macos"))]
#[test]
fn regression_ctrl_w_does_not_close_tab_during_opted_in_ime_composition() {
    let root = test_root("regression-ctrl-w-ime-composition");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "変換中".to_string());
    app.create_new_tab();
    app.shell.runtime.emacs_keybindings_enabled = true;
    app.shell.runtime.ctrl_w_deletes_word_in_query = true;
    app.shell.ui.ime_composition_active = true;

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::W,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: emacs_shortcut_modifiers(false),
        }],
    );

    assert_eq!(app.shell.tabs.len(), 2);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn closing_active_tab_retains_restorable_results_for_fast_restore() {
    let root = test_root("close-active-tab-retains-closed-stack");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let path_a = root.join("a.txt");
    let path_b = root.join("b.txt");

    app.create_new_tab();
    app.shell.runtime.entries = Arc::new(vec![
        unknown_entry(path_a.clone()),
        unknown_entry(path_b.clone()),
    ]);
    app.shell.runtime.base_results = vec![(path_a.clone(), 2.0), (path_b.clone(), 1.0)];
    app.shell.runtime.results = app.shell.runtime.base_results.clone();
    app.shell.runtime.total_match_count = 2;
    app.shell.runtime.preview = "preview body".to_string();
    app.sync_active_tab_state();

    app.close_active_tab();

    assert_eq!(app.shell.tabs.len(), 1);
    assert_eq!(
        app.shell.tabs.last_closed_tab_results_compacted(),
        Some(false)
    );
    app.restore_recently_closed_tab();
    assert_eq!(app.shell.tabs.active_tab, 1);
    assert_eq!(app.shell.runtime.results.len(), 2);
    assert_eq!(app.shell.runtime.results[0].0, path_a);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn restoring_closed_tab_prefers_original_position() {
    let root = test_root("tab-restore-original-position");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

    app.create_new_tab();
    app.shell.runtime.query_state.query = "middle".to_string();
    app.sync_active_tab_state();
    let middle_tab_id = app.shell.tabs.get(1).expect("middle tab").id;

    app.create_new_tab();
    app.shell.runtime.query_state.query = "right".to_string();
    app.sync_active_tab_state();
    let right_tab_id = app.shell.tabs.get(2).expect("right tab").id;

    app.close_tab_index(1);
    assert_eq!(app.shell.tabs.len(), 2);
    assert_eq!(app.shell.tabs.get(1).expect("right tab").id, right_tab_id);

    app.restore_recently_closed_tab();

    assert_eq!(app.shell.tabs.len(), 3);
    assert_eq!(app.shell.tabs.active_tab, 1);
    assert_eq!(app.shell.runtime.query_state.query, "middle");
    assert_ne!(
        app.shell.tabs.get(1).expect("restored middle tab").id,
        middle_tab_id
    );
    assert_eq!(app.shell.tabs.get(2).expect("right tab").id, right_tab_id);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ctrl_shift_t_restores_most_recently_closed_tab_as_active() {
    let root_a = test_root("shortcut-ctrl-shift-t-restore-a");
    let root_b = test_root("shortcut-ctrl-shift-t-restore-b");
    fs::create_dir_all(&root_a).expect("create root a");
    fs::create_dir_all(&root_b).expect("create root b");
    let mut app = FlistWalkerApp::new(root_a.clone(), 50, String::new());
    let (index_tx, index_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = index_tx;
    let (index_response_tx, index_response_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = index_response_rx;
    reset_index_request_state_for_test(&mut app);
    let (search_tx, search_rx) = mpsc::channel::<SearchRequest>();
    app.shell.search.tx = search_tx;
    let original_tab_id = app.shell.tabs.get(0).expect("tab 0").id;

    app.create_new_tab();
    app.shell.runtime.root = root_b.clone();
    app.shell.runtime.query_state.query = "needle".to_string();
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    app.shell.runtime.include_dirs = false;
    app.sync_active_tab_state();
    let closed_tab_id = app.shell.tabs.get(1).expect("tab 1").id;
    app.shell.search.set_pending_request_id(Some(31));
    app.shell.search.set_in_progress(true);
    app.shell.indexing.pending_request_id = Some(32);
    app.shell.indexing.in_progress = true;
    app.shell.indexing.request_tabs.insert(32, closed_tab_id);
    app.shell.indexing.inflight_requests.insert(32);
    app.shell.worker_bus.preview.pending_request_id = Some(33);
    app.shell.worker_bus.preview.in_progress = true;
    app.shell.tabs.bind_preview_request(33, closed_tab_id);
    app.shell.worker_bus.action.pending_request_id = Some(34);
    app.shell.worker_bus.action.in_progress = true;
    app.shell.tabs.bind_action_request(34, closed_tab_id);
    app.shell.worker_bus.sort.pending_request_id = Some(35);
    app.shell.worker_bus.sort.in_progress = true;
    app.shell.tabs.bind_sort_request(35, closed_tab_id);
    app.shell.search.bind_request_tab(31, closed_tab_id);

    app.close_active_tab();
    assert_eq!(app.shell.tabs.len(), 1);
    assert_eq!(app.shell.tabs.active_tab, 0);
    assert_eq!(app.shell.tabs.get(0).expect("tab 0").id, original_tab_id);

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::T,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: gui_shortcut_modifiers(true),
        }],
    );

    assert_eq!(app.shell.tabs.len(), 2);
    assert_eq!(app.shell.tabs.active_tab, 1);
    assert_eq!(app.shell.runtime.root, root_b);
    assert_eq!(app.shell.runtime.query_state.query, "needle");
    assert!(!app.shell.runtime.include_dirs);
    assert_ne!(
        app.shell.tabs.get(1).expect("restored tab").id,
        closed_tab_id
    );
    let replacement_search = search_rx
        .try_recv()
        .expect("restored tab must replace interrupted search work during reindex");
    assert_ne!(replacement_search.request_id, 31);
    assert_eq!(replacement_search.query, "needle");
    assert_eq!(
        app.shell.search.pending_request_id(),
        Some(replacement_search.request_id)
    );
    assert!(app.shell.search.in_progress());
    let replacement = index_rx
        .try_recv()
        .expect("restored tab must replace interrupted index work");
    assert_ne!(replacement.request_id, 32);
    assert_eq!(
        replacement.tab_id,
        app.current_tab_id().expect("restored tab id")
    );
    assert_eq!(
        app.shell.indexing.pending_request_id,
        Some(replacement.request_id)
    );
    assert!(app.shell.indexing.in_progress);
    assert_eq!(app.shell.worker_bus.preview.pending_request_id, None);
    assert!(!app.shell.worker_bus.preview.in_progress);
    assert_eq!(app.shell.worker_bus.action.pending_request_id, None);
    assert!(!app.shell.worker_bus.action.in_progress);
    assert_eq!(app.shell.worker_bus.sort.pending_request_id, None);
    assert!(!app.shell.worker_bus.sort.in_progress);
    assert!(matches!(
        app.shell.search.route_response(31),
        SearchResponseRoute::Stale
    ));
    assert_eq!(app.shell.tabs.preview_request_tab(33), None);
    assert_eq!(app.shell.tabs.action_request_tab(34), None);
    assert_eq!(app.shell.tabs.sort_request_tab(35), None);
    index_response_tx
        .send(IndexResponse::Failed {
            request_id: replacement.request_id,
            error: "expected regression failure".to_string(),
        })
        .expect("send replacement index failure");
    app.poll_index_response();
    assert!(!app.shell.indexing.in_progress);
    assert_eq!(
        app.shell.search.pending_request_id(),
        Some(replacement_search.request_id)
    );
    assert_eq!(
        app.shell.search.request_routes_for_test(),
        vec![(
            replacement_search.request_id,
            app.current_tab_id().expect("restored tab id")
        )]
    );
    assert!(app.shell.ui.focus_query_requested);
    assert!(!app.shell.ui.unfocus_query_requested);
    let _ = fs::remove_dir_all(&root_a);
    let _ = fs::remove_dir_all(&root_b);
}

#[test]
fn restoring_closed_tab_reissues_interrupted_search_without_reindex() {
    let root = test_root("closed-tab-interrupted-search");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    let (index_tx, index_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = index_tx;
    reset_index_request_state_for_test(&mut app);
    let (search_tx, search_rx) = mpsc::channel::<SearchRequest>();
    app.shell.search.tx = search_tx;
    app.shell.runtime.query_state.query = "needle".to_string();
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    let closed_id = app.current_tab_id().expect("closed tab id");
    app.shell.search.set_pending_request_id(Some(401));
    app.shell.search.set_in_progress(true);
    app.shell.search.bind_request_tab(401, closed_id);

    app.close_active_tab();
    app.restore_recently_closed_tab();

    let replacement = search_rx
        .try_recv()
        .expect("restored tab must replace interrupted search work");
    assert_ne!(replacement.request_id, 401);
    assert_eq!(
        app.shell.search.request_routes_for_test(),
        vec![(
            replacement.request_id,
            app.current_tab_id().expect("restored tab id")
        )]
    );
    assert_eq!(replacement.query, "needle");
    assert!(
        index_rx.try_recv().is_err(),
        "search-only restore must not reindex"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn restoring_closed_tab_reissues_interrupted_sort_without_reindex() {
    let root = test_root("closed-tab-interrupted-sort");
    fs::create_dir_all(&root).expect("create root");
    let selected = root.join("selected.txt");
    fs::write(&selected, "selected").expect("write fixture");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    let (index_tx, index_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = index_tx;
    reset_index_request_state_for_test(&mut app);
    let (sort_tx, sort_rx) = mpsc::channel::<SortMetadataRequest>();
    app.shell.worker_bus.sort.tx = sort_tx;
    app.shell.runtime.base_results = vec![(selected.clone(), 1.0)];
    app.shell.runtime.results = app.shell.runtime.base_results.clone();
    app.shell.runtime.current_row = Some(0);
    app.shell.runtime.result_sort_mode = ResultSortMode::SizeDesc;
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    app.shell.worker_bus.sort.pending_request_id = Some(402);
    app.shell.worker_bus.sort.in_progress = true;
    let closed_id = app.current_tab_id().expect("closed tab id");
    app.bind_sort_request_to_tab(402, closed_id);

    app.close_active_tab();
    app.restore_recently_closed_tab();

    let replacement = sort_rx
        .try_recv()
        .expect("restored tab must replace interrupted sort work");
    assert_ne!(replacement.request_id, 402);
    assert_eq!(replacement.mode, ResultSortMode::SizeDesc);
    assert_eq!(replacement.paths, vec![selected]);
    assert!(
        index_rx.try_recv().is_err(),
        "sort-only restore must not reindex"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn restoring_closed_tab_reissues_empty_all_matches_sort_after_index_restart() {
    let root = test_root("closed-tab-index-all-matches-sort");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    let (index_tx, index_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = index_tx;
    let (index_response_tx, index_response_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = index_response_rx;
    reset_index_request_state_for_test(&mut app);
    let (search_tx, search_rx) = mpsc::channel::<SearchRequest>();
    app.shell.search.tx = search_tx;
    app.shell.runtime.result_sort_mode = ResultSortMode::SizeDesc;
    app.shell.runtime.result_sort_scope = ResultSortScope::AllMatches;
    let closed_id = app.current_tab_id().expect("closed tab id");
    app.shell.indexing.pending_request_id = Some(501);
    app.shell.indexing.in_progress = true;
    app.shell.indexing.request_tabs.insert(501, closed_id);
    app.shell.indexing.inflight_requests.insert(501);
    app.shell.search.set_pending_request_id(Some(502));
    app.shell.search.set_in_progress(true);
    app.shell.search.bind_request_tab(502, closed_id);

    app.close_active_tab();
    app.restore_recently_closed_tab();

    let replacement_index = index_rx
        .try_recv()
        .expect("restored tab must replace interrupted index work");
    let immediate_search = search_rx
        .try_recv()
        .expect("restored tab must immediately replace all-matches search work");
    assert_eq!(immediate_search.sort_mode, ResultSortMode::SizeDesc);
    assert_eq!(immediate_search.sort_scope, ResultSortScope::AllMatches);
    index_response_tx
        .send(IndexResponse::Finished {
            request_id: replacement_index.request_id,
            source: IndexSource::Walker,
        })
        .expect("send replacement index finish");
    app.poll_index_response();

    let final_search = search_rx
        .try_recv()
        .expect("completed replacement index must reapply all-matches sort");
    assert_ne!(final_search.request_id, immediate_search.request_id);
    assert_eq!(final_search.sort_mode, ResultSortMode::SizeDesc);
    assert_eq!(final_search.sort_scope, ResultSortScope::AllMatches);
    assert!(matches!(
        app.shell.search.route_response(502),
        SearchResponseRoute::Stale
    ));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn restoring_closed_tab_reissues_shown_metadata_sort_after_index_restart() {
    let root = test_root("closed-tab-index-shown-sort");
    fs::create_dir_all(&root).expect("create root");
    let selected = root.join("selected.txt");
    fs::write(&selected, "selected").expect("write fixture");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    let (index_tx, index_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = index_tx;
    let (index_response_tx, index_response_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = index_response_rx;
    reset_index_request_state_for_test(&mut app);
    let (sort_tx, sort_rx) = mpsc::channel::<SortMetadataRequest>();
    app.shell.worker_bus.sort.tx = sort_tx;
    app.shell.runtime.entries = Arc::new(vec![file_entry(selected.clone())]);
    app.shell.runtime.all_entries = Arc::clone(&app.shell.runtime.entries);
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    app.shell.runtime.base_results = vec![(selected.clone(), 1.0)];
    app.shell.runtime.results = app.shell.runtime.base_results.clone();
    app.shell.runtime.current_row = Some(0);
    app.shell.runtime.result_sort_mode = ResultSortMode::SizeDesc;
    app.shell.runtime.result_sort_scope = ResultSortScope::ShownResults;
    let closed_id = app.current_tab_id().expect("closed tab id");
    app.shell.indexing.pending_request_id = Some(601);
    app.shell.indexing.in_progress = true;
    app.shell.indexing.request_tabs.insert(601, closed_id);
    app.shell.indexing.inflight_requests.insert(601);
    app.shell.worker_bus.sort.pending_request_id = Some(602);
    app.shell.worker_bus.sort.in_progress = true;
    app.bind_sort_request_to_tab(602, closed_id);

    app.close_active_tab();
    app.restore_recently_closed_tab();

    let replacement_index = index_rx
        .try_recv()
        .expect("restored tab must replace interrupted index work");
    let immediate_sort = sort_rx
        .try_recv()
        .expect("restored tab must immediately replace shown-results sort work");
    assert_eq!(immediate_sort.mode, ResultSortMode::SizeDesc);
    index_response_tx
        .send(IndexResponse::Batch {
            request_id: replacement_index.request_id,
            entries: vec![IndexEntry {
                path: selected.clone(),
                kind: EntryKind::file(),
                kind_known: true,
            }],
        })
        .expect("send replacement index batch");
    index_response_tx
        .send(IndexResponse::Finished {
            request_id: replacement_index.request_id,
            source: IndexSource::Walker,
        })
        .expect("send replacement index finish");
    for _ in 0..4 {
        app.poll_index_response();
    }

    let final_sort = sort_rx
        .try_recv()
        .expect("completed replacement index must reapply shown-results sort");
    assert_ne!(final_sort.request_id, immediate_sort.request_id);
    assert_eq!(final_sort.mode, ResultSortMode::SizeDesc);
    assert_eq!(final_sort.paths, vec![selected]);
    assert_eq!(app.shell.tabs.sort_request_tab(602), None);
    assert_eq!(
        app.shell.tabs.sort_request_tab(final_sort.request_id),
        app.current_tab_id()
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn restoring_closed_tab_reloads_preview_after_request_ownership_is_cleared() {
    let root = test_root("closed-tab-interrupted-preview");
    fs::create_dir_all(&root).expect("create root");
    let selected = root.join("selected.txt");
    fs::write(&selected, "selected").expect("write fixture");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    reset_index_request_state_for_test(&mut app);
    app.shell.runtime.entries = Arc::new(vec![file_entry(selected.clone())]);
    app.shell.runtime.all_entries = Arc::clone(&app.shell.runtime.entries);
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    app.shell.runtime.base_results = vec![(selected.clone(), 1.0)];
    app.shell.runtime.results = app.shell.runtime.base_results.clone();
    app.shell.runtime.current_row = Some(0);
    app.set_entry_kind(&selected, EntryKind::file());
    let (preview_tx, preview_rx) = mpsc::channel::<PreviewRequest>();
    app.shell.worker_bus.preview.tx = preview_tx;
    app.request_preview_for_current();
    let interrupted = preview_rx.try_recv().expect("interrupted preview request");

    app.close_active_tab();
    app.restore_recently_closed_tab();

    let replacement = preview_rx
        .try_recv()
        .expect("restored tab must reload the selected preview");
    assert_ne!(replacement.request_id, interrupted.request_id);
    assert_eq!(replacement.path, selected);
    assert_eq!(app.shell.runtime.preview, "Loading preview...");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_207_heavy_closed_restore_excludes_target_from_cache_budget_under_full_reclaimer() {
    let root = test_root("tc-207-heavy-closed-target-budget");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    app.shell
        .indexing
        .set_committed_snapshot_present_for_test(true);
    app.shell.runtime.all_entries = Arc::new(vec![file_entry(root.join("closed-heavy.txt"))]);
    app.shell.runtime.entries = Arc::clone(&app.shell.runtime.all_entries);
    app.close_active_tab();
    let cached_id = app.current_tab_id().expect("cached tab");
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    app.shell
        .indexing
        .set_committed_snapshot_present_for_test(true);
    app.shell.runtime.all_entries = Arc::new(vec![file_entry(root.join("cached-heavy.txt"))]);
    app.shell.runtime.entries = Arc::clone(&app.shell.runtime.all_entries);
    app.sync_active_tab_state();
    app.create_new_tab();
    app.shell.tabs.pause_resource_reclaimer();
    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut tab = app.capture_active_tab_state(3_400 + index as u64);
        tab.result_state.committed.all_entries =
            Arc::new(vec![file_entry(root.join(format!("held-{index}.txt")))]);
        app.shell
            .tabs
            .retire_tab_resources_for_test(tab.take_heavy_resources())
            .expect("fill reclaimer");
    }

    app.restore_recently_closed_tab();

    assert_eq!(app.shell.tabs.len(), 3);
    assert_eq!(app.shell.tabs.closed_tab_count(), 0);
    assert_eq!(app.shell.runtime.all_entries.len(), 1);
    assert_eq!(
        app.shell.runtime.all_entries[0].path,
        root.join("closed-heavy.txt")
    );
    let cached = app
        .shell
        .tabs
        .iter()
        .find(|tab| tab.id == cached_id)
        .expect("cached tab retained");
    assert!(cached.index_state.committed_snapshot_present());
    assert_eq!(cached.result_state.committed.all_entries.len(), 1);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn restoring_closed_tab_reloads_trimmed_completed_preview() {
    let root = test_root("closed-tab-completed-preview");
    fs::create_dir_all(&root).expect("create root");
    let selected = root.join("selected.txt");
    fs::write(&selected, "selected").expect("write fixture");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    reset_index_request_state_for_test(&mut app);
    app.shell.runtime.entries = Arc::new(vec![file_entry(selected.clone())]);
    app.shell.runtime.all_entries = Arc::clone(&app.shell.runtime.entries);
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    app.shell.runtime.base_results = vec![(selected.clone(), 1.0)];
    app.shell.runtime.results = app.shell.runtime.base_results.clone();
    app.shell.runtime.current_row = Some(0);
    app.shell.runtime.preview = "completed preview".to_string();
    app.set_entry_kind(&selected, EntryKind::file());
    let (preview_tx, preview_rx) = mpsc::channel::<PreviewRequest>();
    app.shell.worker_bus.preview.tx = preview_tx;

    app.close_active_tab();
    app.restore_recently_closed_tab();

    let replacement = preview_rx
        .try_recv()
        .expect("restored tab must reload its trimmed completed preview");
    assert_eq!(replacement.path, selected);
    assert_eq!(app.shell.runtime.preview, "Loading preview...");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn restoring_closed_tabs_uses_lifo_order_and_empty_stack_is_noop() {
    let root_a = test_root("tab-restore-lifo-a");
    let root_b = test_root("tab-restore-lifo-b");
    let root_c = test_root("tab-restore-lifo-c");
    fs::create_dir_all(&root_a).expect("create root a");
    fs::create_dir_all(&root_b).expect("create root b");
    fs::create_dir_all(&root_c).expect("create root c");
    let mut app = FlistWalkerApp::new(root_a.clone(), 50, String::new());

    app.create_new_tab();
    app.shell.runtime.root = root_b.clone();
    app.shell.runtime.query_state.query = "second".to_string();
    app.sync_active_tab_state();

    app.create_new_tab();
    app.shell.runtime.root = root_c.clone();
    app.shell.runtime.query_state.query = "third".to_string();
    app.sync_active_tab_state();

    app.close_tab_index(1);
    app.close_tab_index(1);
    assert_eq!(app.shell.tabs.len(), 1);

    app.restore_recently_closed_tab();
    assert_eq!(app.shell.tabs.len(), 2);
    assert_eq!(app.shell.tabs.active_tab, 1);
    assert_eq!(app.shell.runtime.root, root_c);
    assert_eq!(app.shell.runtime.query_state.query, "third");

    app.restore_recently_closed_tab();
    assert_eq!(app.shell.tabs.len(), 3);
    assert_eq!(app.shell.tabs.active_tab, 1);
    assert_eq!(app.shell.runtime.root, root_b);
    assert_eq!(app.shell.runtime.query_state.query, "second");

    app.restore_recently_closed_tab();
    assert_eq!(app.shell.tabs.len(), 3);
    assert!(app
        .shell
        .runtime
        .notice
        .contains("No closed tab to restore"));
    let _ = fs::remove_dir_all(&root_a);
    let _ = fs::remove_dir_all(&root_b);
    let _ = fs::remove_dir_all(&root_c);
}

#[test]
fn closed_tab_restore_stack_keeps_only_recent_entries() {
    let root = test_root("tab-restore-stack-limit");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

    for index in 0..26 {
        app.create_new_tab();
        app.shell.runtime.query_state.query = format!("closed-{index}");
        app.sync_active_tab_state();
        app.close_active_tab();
    }

    app.restore_recently_closed_tab();
    assert_eq!(app.shell.runtime.query_state.query, "closed-25");

    for _ in 0..24 {
        app.restore_recently_closed_tab();
    }
    assert_eq!(app.shell.runtime.query_state.query, "closed-1");

    app.restore_recently_closed_tab();
    assert_eq!(app.shell.tabs.len(), 26);
    assert!(app
        .shell
        .runtime
        .notice
        .contains("No closed tab to restore"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ctrl_tab_and_ctrl_shift_tab_switch_active_tab() {
    let root = test_root("shortcut-ctrl-tab-switch");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.create_new_tab();
    assert_eq!(app.shell.tabs.len(), 3);
    assert_eq!(app.shell.tabs.active_tab, 2);

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: tab_switch_shortcut_modifiers(false),
        }],
    );
    assert_eq!(app.shell.tabs.active_tab, 0);
    assert!(app.shell.ui.focus_query_requested);
    assert!(!app.shell.ui.unfocus_query_requested);

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: tab_switch_shortcut_modifiers(true),
        }],
    );
    assert_eq!(app.shell.tabs.active_tab, 2);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ctrl_number_switches_to_matching_tab_from_left() {
    let root = test_root("shortcut-ctrl-number-tab-switch");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.create_new_tab();
    app.create_new_tab();
    assert_eq!(app.shell.tabs.len(), 4);
    assert_eq!(app.shell.tabs.active_tab, 3);

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::Num2,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: gui_shortcut_modifiers(false),
        }],
    );

    assert_eq!(app.shell.tabs.active_tab, 1);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ctrl_number_without_matching_tab_does_not_switch() {
    let root = test_root("shortcut-ctrl-number-no-tab");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    assert_eq!(app.shell.tabs.len(), 2);
    assert_eq!(app.shell.tabs.active_tab, 1);

    run_shortcuts_frame(
        &mut app,
        false,
        vec![egui::Event::Key {
            key: egui::Key::Num3,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: gui_shortcut_modifiers(false),
        }],
    );

    assert_eq!(app.shell.tabs.active_tab, 1);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn switching_tabs_restores_root_per_tab() {
    let root_a = test_root("tab-root-a");
    let root_b = test_root("tab-root-b");
    fs::create_dir_all(&root_a).expect("create root a");
    fs::create_dir_all(&root_b).expect("create root b");
    let mut app = FlistWalkerApp::new(root_a.clone(), 50, String::new());

    app.create_new_tab();
    app.shell.runtime.root = root_b.clone();
    app.sync_active_tab_state();
    assert_eq!(app.shell.tabs.active_tab, 1);

    app.switch_to_tab_index(0);
    assert_eq!(app.shell.runtime.root, root_a);

    app.switch_to_tab_index(1);
    assert_eq!(app.shell.runtime.root, root_b);

    let _ = fs::remove_dir_all(&root_a);
    let _ = fs::remove_dir_all(&root_b);
}

#[test]
fn tc_207_explicit_activation_operations_cancel_older_deferred_tab_intent() {
    let root = test_root("tc-207-latest-activation-intent");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    reset_index_request_state_for_test(&mut app);
    let old_target_id = app.shell.tabs.get(0).expect("old target").id;

    app.shell.tabs.pending_activation_tab_id = Some(old_target_id);
    app.create_new_tab();
    assert_eq!(app.shell.tabs.pending_activation_tab_id, None);

    reset_index_request_state_for_test(&mut app);
    app.shell.tabs.pending_activation_tab_id = Some(old_target_id);
    app.close_active_tab();
    assert_eq!(app.shell.tabs.pending_activation_tab_id, None);
    assert_eq!(app.shell.tabs.closed_tab_count(), 1);

    reset_index_request_state_for_test(&mut app);
    app.shell.tabs.pending_activation_tab_id = Some(old_target_id);
    app.restore_recently_closed_tab();
    assert_eq!(app.shell.tabs.pending_activation_tab_id, None);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn switching_tabs_restores_entries_and_filters_per_tab() {
    let root = test_root("tab-entries-filters");
    fs::create_dir_all(&root).expect("create dir");
    let a = root.join("a.txt");
    let b = root.join("b.txt");
    fs::write(&a, "a").expect("write a");
    fs::write(&b, "b").expect("write b");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.entries = Arc::new(vec![unknown_entry(a.clone()), unknown_entry(b.clone())]);
    app.shell.runtime.all_entries =
        Arc::new(vec![unknown_entry(a.clone()), unknown_entry(b.clone())]);
    app.shell.runtime.include_files = true;
    app.shell.runtime.include_dirs = true;
    app.sync_active_tab_state();

    app.create_new_tab();
    app.shell.runtime.entries = Arc::new(vec![unknown_entry(a.clone())]);
    app.shell.runtime.all_entries = Arc::new(vec![unknown_entry(a.clone())]);
    app.shell.runtime.include_files = true;
    app.shell.runtime.include_dirs = false;
    app.sync_active_tab_state();

    app.switch_to_tab_index(0);
    assert_eq!(app.shell.runtime.entries.len(), 2);
    assert_eq!(app.shell.runtime.all_entries.len(), 2);
    assert!(app.shell.runtime.include_files);
    assert!(app.shell.runtime.include_dirs);

    app.switch_to_tab_index(1);
    assert_eq!(app.shell.runtime.entries.len(), 1);
    assert_eq!(app.shell.runtime.all_entries.len(), 1);
    assert!(app.shell.runtime.include_files);
    assert!(!app.shell.runtime.include_dirs);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn root_dropdown_selection_closes_popup_and_applies_selected_root() {
    let root_a = test_root("root-dropdown-select-a");
    let root_b = test_root("root-dropdown-select-b");
    fs::create_dir_all(&root_a).expect("create root a");
    fs::create_dir_all(&root_b).expect("create root b");
    let mut app = FlistWalkerApp::new(root_a.clone(), 50, String::new());
    app.shell.features.root_browser.saved_roots = vec![root_a.clone(), root_b.clone()];
    let ctx = egui::Context::default();

    app.open_root_dropdown(&ctx);
    app.move_root_dropdown_selection(1);
    assert!(app.is_root_dropdown_open(&ctx));
    assert_eq!(app.shell.ui.root_dropdown_highlight, Some(1));

    app.apply_root_dropdown_selection(&ctx);

    assert!(!app.is_root_dropdown_open(&ctx));
    assert_eq!(app.shell.runtime.root, root_b);
    assert_eq!(app.shell.ui.root_dropdown_highlight, Some(1));
    let _ = fs::remove_dir_all(&root_a);
    let _ = fs::remove_dir_all(&root_b);
}

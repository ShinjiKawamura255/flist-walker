use super::*;
use crate::app::tab_state::AppTabState;
use crate::app::{
    BackgroundIndexFinalizeIdentity, BackgroundIndexFinalizeInputs, BackgroundIndexFinalizePolicy,
    EntryKindCacheState, PendingBackgroundIndexFinalize,
};

const PAYLOAD_LEN: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PayloadAllocations {
    index_entries: (*const Entry, usize),
    all_entries: (*const Vec<Entry>, usize),
    filtered_entries: (*const Vec<Entry>, usize),
    pending_index_entries: (*const IndexEntry, usize),
    pending_kind_paths: (*const PathBuf, usize),
    resolved_kind_updates: (*const (PathBuf, EntryKind), usize),
    incremental_filtered_entries: (*const Entry, usize),
    base_results: (*const (PathBuf, f64), usize),
    results: (*const (PathBuf, f64), usize),
    preview: (*const u8, usize),
    entry_kind_cache: (*const PathBuf, usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ActivationAllocations {
    index_entries: (*const Entry, usize),
    all_entries: (*const Vec<Entry>, usize),
    filtered_entries: (*const Vec<Entry>, usize),
    pending_index_entries: (*const IndexEntry, usize),
    resolved_kind_updates: (*const (PathBuf, EntryKind), usize),
    incremental_filtered_entries: (*const Entry, usize),
    base_results: (*const (PathBuf, f64), usize),
    results: (*const (PathBuf, f64), usize),
    entry_kind_cache: (*const PathBuf, usize),
}

impl PayloadAllocations {
    fn stable_through_activation(self) -> ActivationAllocations {
        ActivationAllocations {
            index_entries: self.index_entries,
            all_entries: self.all_entries,
            filtered_entries: self.filtered_entries,
            pending_index_entries: self.pending_index_entries,
            resolved_kind_updates: self.resolved_kind_updates,
            incremental_filtered_entries: self.incremental_filtered_entries,
            base_results: self.base_results,
            results: self.results,
            entry_kind_cache: self.entry_kind_cache,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PayloadMetadata {
    resource_state: crate::app::tab_state::TabResourceState,
    index_source: IndexSource,
    pending_search_request_id: Option<u64>,
    pending_preview_request_id: Option<u64>,
    pending_action_request_id: Option<u64>,
    search_in_progress: bool,
    preview_in_progress: bool,
    action_in_progress: bool,
    pending_entries_request_id: Option<u64>,
    pending_kind_paths_set: (Vec<PathBuf>, usize),
    in_flight_kind_paths: (Vec<PathBuf>, usize),
    kind_resolution_epoch: u64,
    kind_resolution_in_progress: bool,
    last_search_snapshot_len: usize,
    search_resume_pending: bool,
    search_rerun_pending: bool,
    result_sort_mode: ResultSortMode,
    result_sort_scope: ResultSortScope,
    total_match_count: usize,
    current_row: Option<usize>,
    evicted_selected_path: Option<PathBuf>,
    entry_kind_cache_paths: (Vec<PathBuf>, usize),
    notice: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PayloadInventory {
    allocations: PayloadAllocations,
    metadata: PayloadMetadata,
}

fn entries(prefix: &str) -> Vec<Entry> {
    (0..PAYLOAD_LEN)
        .map(|index| file_entry(PathBuf::from(format!("{prefix}-{index}.txt"))))
        .collect()
}

fn pending_entries(prefix: &str) -> VecDeque<IndexEntry> {
    (0..PAYLOAD_LEN)
        .map(|index| IndexEntry {
            path: PathBuf::from(format!("{prefix}-pending-{index}.txt")),
            kind: EntryKind::file(),
            kind_known: true,
        })
        .collect()
}

fn kind_paths(prefix: &str) -> VecDeque<PathBuf> {
    (0..PAYLOAD_LEN)
        .map(|index| PathBuf::from(format!("{prefix}-kind-{index}.txt")))
        .collect()
}

fn kind_updates(prefix: &str) -> Vec<(PathBuf, EntryKind)> {
    (0..PAYLOAD_LEN)
        .map(|index| {
            (
                PathBuf::from(format!("{prefix}-resolved-{index}.txt")),
                EntryKind::file(),
            )
        })
        .collect()
}

fn results(prefix: &str) -> Vec<(PathBuf, f64)> {
    (0..PAYLOAD_LEN)
        .map(|index| {
            (
                PathBuf::from(format!("{prefix}-result-{index}.txt")),
                index as f64,
            )
        })
        .collect()
}

fn seed_live_payload(app: &mut FlistWalkerApp, prefix: &str, request_id: u64) {
    app.shell
        .indexing
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    app.shell
        .indexing
        .set_committed_snapshot_present_for_test(true);
    app.shell.runtime.index.entries = entries(prefix);
    app.shell.runtime.index.source = IndexSource::Walker;
    app.shell.runtime.all_entries = Arc::new(entries(&format!("{prefix}-all")));
    app.shell.runtime.entries = Arc::new(entries(&format!("{prefix}-filtered")));
    app.shell.indexing.pending_entries = pending_entries(prefix);
    app.shell.indexing.pending_entries_request_id = Some(request_id + 10);
    app.shell.indexing.pending_kind_paths = kind_paths(prefix);
    app.shell.indexing.pending_kind_paths_set = app
        .shell
        .indexing
        .pending_kind_paths
        .iter()
        .cloned()
        .collect();
    app.shell.indexing.in_flight_kind_paths = (0..PAYLOAD_LEN)
        .map(|index| PathBuf::from(format!("{prefix}-in-flight-{index}.txt")))
        .collect();
    app.shell.indexing.resolved_kind_updates = kind_updates(prefix);
    app.shell.indexing.kind_resolution_epoch = request_id + 20;
    app.shell.indexing.kind_resolution_in_progress = true;
    app.shell.indexing.incremental_filtered_entries = entries(&format!("{prefix}-incremental"));
    app.shell.indexing.last_search_snapshot_len = PAYLOAD_LEN - 1;
    app.shell.indexing.search_resume_pending = true;
    app.shell.indexing.search_rerun_pending = true;
    app.shell.runtime.base_results = results(&format!("{prefix}-base"));
    app.shell.runtime.results = results(prefix);
    app.shell.runtime.result_sort_mode = ResultSortMode::NameAsc;
    app.shell.runtime.result_sort_scope = ResultSortScope::AllMatches;
    app.shell.runtime.total_match_count = PAYLOAD_LEN + 7;
    app.shell.runtime.current_row = Some(7);
    app.shell.runtime.evicted_selected_path = Some(PathBuf::from(format!("{prefix}-result-7.txt")));
    app.shell.runtime.preview = format!("{prefix}-preview-").repeat(PAYLOAD_LEN);
    app.shell.runtime.notice = format!("{prefix} notice");
    app.shell.cache.entry_kind.clear();
    for index in 0..PAYLOAD_LEN {
        app.shell.cache.entry_kind.set(
            PathBuf::from(format!("{prefix}-cached-{index}.txt")),
            EntryKind::file(),
        );
    }
    app.shell.search.set_pending_request_id(Some(request_id));
    app.shell.search.set_in_progress(true);
    app.shell.worker_bus.preview.pending_request_id = Some(request_id + 1);
    app.shell.worker_bus.preview.in_progress = true;
    app.shell.worker_bus.action.pending_request_id = Some(request_id + 2);
    app.shell.worker_bus.action.in_progress = true;
}

fn seed_tab_payload(tab: &mut AppTabState, prefix: &str, request_id: u64) {
    tab.index_state
        .set_lifecycle_for_test(TabResourceLifecycle::Ready);
    tab.index_state
        .set_committed_snapshot_present_for_test(true);
    tab.index_state.index.entries = entries(prefix);
    tab.index_state.index.source = IndexSource::Walker;
    tab.index_state.all_entries = Arc::new(entries(&format!("{prefix}-all")));
    tab.index_state.entries = Arc::new(entries(&format!("{prefix}-filtered")));
    tab.index_state.pending_index_entries = pending_entries(prefix);
    tab.index_state.pending_index_entries_request_id = Some(request_id + 10);
    tab.index_state.pending_kind_paths = kind_paths(prefix);
    tab.index_state.pending_kind_paths_set =
        tab.index_state.pending_kind_paths.iter().cloned().collect();
    tab.index_state.in_flight_kind_paths = (0..PAYLOAD_LEN)
        .map(|index| PathBuf::from(format!("{prefix}-in-flight-{index}.txt")))
        .collect();
    tab.index_state.resolved_kind_updates = kind_updates(prefix);
    tab.index_state.kind_resolution_epoch = request_id + 20;
    tab.index_state.kind_resolution_in_progress = true;
    tab.index_state.incremental_filtered_entries = entries(&format!("{prefix}-incremental"));
    tab.index_state.last_search_snapshot_len = PAYLOAD_LEN - 1;
    tab.index_state.search_resume_pending = true;
    tab.index_state.search_rerun_pending = true;
    tab.result_state.base_results = results(&format!("{prefix}-base"));
    tab.result_state.results = results(prefix);
    tab.result_state.result_sort_mode = ResultSortMode::NameAsc;
    tab.result_state.result_sort_scope = ResultSortScope::AllMatches;
    tab.result_state.total_match_count = PAYLOAD_LEN + 7;
    tab.result_state.current_row = Some(7);
    tab.result_state.evicted_selected_path = Some(PathBuf::from(format!("{prefix}-result-7.txt")));
    tab.result_state.preview = format!("{prefix}-preview-").repeat(PAYLOAD_LEN);
    tab.notice = format!("{prefix} notice");
    tab.entry_kind_cache.clear();
    for index in 0..PAYLOAD_LEN {
        tab.entry_kind_cache.set(
            PathBuf::from(format!("{prefix}-cached-{index}.txt")),
            EntryKind::file(),
        );
    }
    tab.pending_request_id = Some(request_id);
    tab.search_in_progress = true;
    tab.pending_preview_request_id = Some(request_id + 1);
    tab.preview_in_progress = true;
    tab.pending_action_request_id = Some(request_id + 2);
    tab.action_in_progress = true;
}

fn deque_allocation<T>(deque: &VecDeque<T>) -> (*const T, usize) {
    let (head, tail) = deque.as_slices();
    assert!(tail.is_empty(), "fixture deque must remain contiguous");
    (head.as_ptr(), deque.capacity())
}

fn live_allocations(app: &FlistWalkerApp) -> PayloadAllocations {
    PayloadAllocations {
        index_entries: (
            app.shell.runtime.index.entries.as_ptr(),
            app.shell.runtime.index.entries.capacity(),
        ),
        all_entries: (
            Arc::as_ptr(&app.shell.runtime.all_entries),
            app.shell.runtime.all_entries.capacity(),
        ),
        filtered_entries: (
            Arc::as_ptr(&app.shell.runtime.entries),
            app.shell.runtime.entries.capacity(),
        ),
        pending_index_entries: deque_allocation(&app.shell.indexing.pending_entries),
        pending_kind_paths: deque_allocation(&app.shell.indexing.pending_kind_paths),
        resolved_kind_updates: (
            app.shell.indexing.resolved_kind_updates.as_ptr(),
            app.shell.indexing.resolved_kind_updates.capacity(),
        ),
        incremental_filtered_entries: (
            app.shell.indexing.incremental_filtered_entries.as_ptr(),
            app.shell.indexing.incremental_filtered_entries.capacity(),
        ),
        base_results: (
            app.shell.runtime.base_results.as_ptr(),
            app.shell.runtime.base_results.capacity(),
        ),
        results: (
            app.shell.runtime.results.as_ptr(),
            app.shell.runtime.results.capacity(),
        ),
        preview: (
            app.shell.runtime.preview.as_ptr(),
            app.shell.runtime.preview.capacity(),
        ),
        entry_kind_cache: (
            app.shell
                .cache
                .entry_kind
                .entries
                .keys()
                .next()
                .expect("live cache key") as *const PathBuf,
            app.shell.cache.entry_kind.entries.capacity(),
        ),
    }
}

fn tab_allocations(tab: &AppTabState) -> PayloadAllocations {
    PayloadAllocations {
        index_entries: (
            tab.index_state.index.entries.as_ptr(),
            tab.index_state.index.entries.capacity(),
        ),
        all_entries: (
            Arc::as_ptr(&tab.index_state.all_entries),
            tab.index_state.all_entries.capacity(),
        ),
        filtered_entries: (
            Arc::as_ptr(&tab.index_state.entries),
            tab.index_state.entries.capacity(),
        ),
        pending_index_entries: deque_allocation(&tab.index_state.pending_index_entries),
        pending_kind_paths: deque_allocation(&tab.index_state.pending_kind_paths),
        resolved_kind_updates: (
            tab.index_state.resolved_kind_updates.as_ptr(),
            tab.index_state.resolved_kind_updates.capacity(),
        ),
        incremental_filtered_entries: (
            tab.index_state.incremental_filtered_entries.as_ptr(),
            tab.index_state.incremental_filtered_entries.capacity(),
        ),
        base_results: (
            tab.result_state.base_results.as_ptr(),
            tab.result_state.base_results.capacity(),
        ),
        results: (
            tab.result_state.results.as_ptr(),
            tab.result_state.results.capacity(),
        ),
        preview: (
            tab.result_state.preview.as_ptr(),
            tab.result_state.preview.capacity(),
        ),
        entry_kind_cache: (
            tab.entry_kind_cache
                .entries
                .keys()
                .next()
                .expect("tab cache key") as *const PathBuf,
            tab.entry_kind_cache.entries.capacity(),
        ),
    }
}

fn sorted_paths(paths: &HashSet<PathBuf>) -> Vec<PathBuf> {
    let mut paths = paths.iter().cloned().collect::<Vec<_>>();
    paths.sort_unstable();
    paths
}

fn sorted_cache_paths(cache: &EntryKindCacheState) -> Vec<PathBuf> {
    let mut paths = cache.entries.keys().cloned().collect::<Vec<_>>();
    paths.sort_unstable();
    paths
}

fn live_inventory(app: &FlistWalkerApp) -> PayloadInventory {
    PayloadInventory {
        allocations: live_allocations(app),
        metadata: PayloadMetadata {
            resource_state: app.shell.indexing.resource_state(),
            index_source: app.shell.runtime.index.source.clone(),
            pending_search_request_id: app.shell.search.pending_request_id(),
            pending_preview_request_id: app.shell.worker_bus.preview.pending_request_id,
            pending_action_request_id: app.shell.worker_bus.action.pending_request_id,
            search_in_progress: app.shell.search.in_progress(),
            preview_in_progress: app.shell.worker_bus.preview.in_progress,
            action_in_progress: app.shell.worker_bus.action.in_progress,
            pending_entries_request_id: app.shell.indexing.pending_entries_request_id,
            pending_kind_paths_set: (
                sorted_paths(&app.shell.indexing.pending_kind_paths_set),
                app.shell.indexing.pending_kind_paths_set.capacity(),
            ),
            in_flight_kind_paths: (
                sorted_paths(&app.shell.indexing.in_flight_kind_paths),
                app.shell.indexing.in_flight_kind_paths.capacity(),
            ),
            kind_resolution_epoch: app.shell.indexing.kind_resolution_epoch,
            kind_resolution_in_progress: app.shell.indexing.kind_resolution_in_progress,
            last_search_snapshot_len: app.shell.indexing.last_search_snapshot_len,
            search_resume_pending: app.shell.indexing.search_resume_pending,
            search_rerun_pending: app.shell.indexing.search_rerun_pending,
            result_sort_mode: app.shell.runtime.result_sort_mode,
            result_sort_scope: app.shell.runtime.result_sort_scope,
            total_match_count: app.shell.runtime.total_match_count,
            current_row: app.shell.runtime.current_row,
            evicted_selected_path: app.shell.runtime.evicted_selected_path.clone(),
            entry_kind_cache_paths: (
                sorted_cache_paths(&app.shell.cache.entry_kind),
                app.shell.cache.entry_kind.entries.capacity(),
            ),
            notice: app.shell.runtime.notice.clone(),
        },
    }
}

fn tab_inventory(tab: &AppTabState) -> PayloadInventory {
    PayloadInventory {
        allocations: tab_allocations(tab),
        metadata: PayloadMetadata {
            resource_state: tab.index_state.resource_state(),
            index_source: tab.index_state.index.source.clone(),
            pending_search_request_id: tab.pending_request_id,
            pending_preview_request_id: tab.pending_preview_request_id,
            pending_action_request_id: tab.pending_action_request_id,
            search_in_progress: tab.search_in_progress,
            preview_in_progress: tab.preview_in_progress,
            action_in_progress: tab.action_in_progress,
            pending_entries_request_id: tab.index_state.pending_index_entries_request_id,
            pending_kind_paths_set: (
                sorted_paths(&tab.index_state.pending_kind_paths_set),
                tab.index_state.pending_kind_paths_set.capacity(),
            ),
            in_flight_kind_paths: (
                sorted_paths(&tab.index_state.in_flight_kind_paths),
                tab.index_state.in_flight_kind_paths.capacity(),
            ),
            kind_resolution_epoch: tab.index_state.kind_resolution_epoch,
            kind_resolution_in_progress: tab.index_state.kind_resolution_in_progress,
            last_search_snapshot_len: tab.index_state.last_search_snapshot_len,
            search_resume_pending: tab.index_state.search_resume_pending,
            search_rerun_pending: tab.index_state.search_rerun_pending,
            result_sort_mode: tab.result_state.result_sort_mode,
            result_sort_scope: tab.result_state.result_sort_scope,
            total_match_count: tab.result_state.total_match_count,
            current_row: tab.result_state.current_row,
            evicted_selected_path: tab.result_state.evicted_selected_path.clone(),
            entry_kind_cache_paths: (
                sorted_cache_paths(&tab.entry_kind_cache),
                tab.entry_kind_cache.entries.capacity(),
            ),
            notice: tab.notice.clone(),
        },
    }
}

#[test]
fn tc_154_tab_switch_transfers_large_payload_allocations_in_both_directions() {
    let root = test_root("tc-154-tab-payload-transfer");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), PAYLOAD_LEN, String::new());
    app.create_new_tab();

    seed_live_payload(&mut app, "active", 1541);
    seed_tab_payload(
        app.shell.tabs.get_mut(0).expect("inactive tab"),
        "inactive",
        1540,
    );
    let active_allocations = live_allocations(&app).stable_through_activation();
    let inactive_allocations =
        tab_allocations(app.shell.tabs.get(0).expect("inactive tab")).stable_through_activation();

    app.switch_to_tab_index(0);

    assert_eq!(
        live_allocations(&app).stable_through_activation(),
        inactive_allocations
    );
    assert_eq!(
        tab_allocations(app.shell.tabs.get(1).expect("outgoing tab")).stable_through_activation(),
        active_allocations
    );

    app.switch_to_tab_index(1);

    assert_eq!(
        live_allocations(&app).stable_through_activation(),
        active_allocations
    );
    assert_eq!(
        tab_allocations(app.shell.tabs.get(0).expect("outgoing tab")).stable_through_activation(),
        inactive_allocations
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_154_raw_payload_swap_preserves_the_complete_transfer_inventory() {
    let root = test_root("tc-154-complete-tab-payload-inventory");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), PAYLOAD_LEN, String::new());
    app.create_new_tab();
    seed_live_payload(&mut app, "active-complete", 15_411);
    seed_tab_payload(
        app.shell.tabs.get_mut(0).expect("inactive tab"),
        "inactive-complete",
        15_410,
    );

    let active_inventory = live_inventory(&app);
    let mut inactive = app.shell.tabs.remove(0);
    let inactive_inventory = tab_inventory(&inactive);

    inactive.swap_payload_with_shell(&mut app);
    assert_eq!(live_inventory(&app), inactive_inventory);
    assert_eq!(tab_inventory(&inactive), active_inventory);

    inactive.swap_payload_with_shell(&mut app);
    assert_eq!(live_inventory(&app), active_inventory);
    assert_eq!(tab_inventory(&inactive), inactive_inventory);

    app.shell.tabs.insert(0, inactive);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_154_tab_heavy_take_restore_preserves_the_complete_inventory() {
    let root = test_root("tc-154-tab-heavy-take-restore-inventory");
    fs::create_dir_all(&root).expect("create dir");
    let app = FlistWalkerApp::new(root.clone(), PAYLOAD_LEN, String::new());
    let mut tab = app.capture_active_tab_state(15_420);
    seed_tab_payload(&mut tab, "take-restore", 15_421);
    let before = tab_inventory(&tab);

    let resources = tab.take_heavy_resources();
    assert_eq!(tab.index_state.lifecycle(), TabResourceLifecycle::Evicted);
    assert!(!tab.index_state.committed_snapshot_present());
    assert_eq!(tab.index_state.index.source, IndexSource::Walker);
    assert_eq!(tab.heavy_resource_weight(), 0);
    assert_eq!(tab.index_state.index.entries.capacity(), 0);
    assert_eq!(tab.index_state.all_entries.capacity(), 0);
    assert_eq!(tab.index_state.entries.capacity(), 0);
    assert_eq!(tab.index_state.pending_index_entries.capacity(), 0);
    assert_eq!(tab.index_state.pending_kind_paths.capacity(), 0);
    assert_eq!(tab.index_state.pending_kind_paths_set.capacity(), 0);
    assert_eq!(tab.index_state.in_flight_kind_paths.capacity(), 0);
    assert_eq!(tab.index_state.resolved_kind_updates.capacity(), 0);
    assert_eq!(tab.index_state.incremental_filtered_entries.capacity(), 0);
    assert_eq!(tab.result_state.base_results.capacity(), 0);
    assert_eq!(tab.result_state.results.capacity(), 0);
    assert_eq!(tab.result_state.preview.capacity(), 0);
    assert_eq!(tab.entry_kind_cache.entries.capacity(), 0);

    tab.restore_heavy_resources(resources);
    assert_eq!(tab_inventory(&tab), before);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_154_reclaimer_full_restores_complete_tab_mailbox_and_finalizer_ownership() {
    let root = test_root("tc-154-full-rollback-complete-inventory");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), PAYLOAD_LEN, String::new());
    app.shell.tabs.pause_resource_reclaimer();

    for index in 0..TAB_RESOURCE_RECLAIMER_CAPACITY {
        let mut resources = RetiredIndexBuildResources::empty();
        resources.set_stale_index_entries(vec![IndexEntry {
            path: root.join(format!("queued-{index}.txt")),
            kind: EntryKind::file(),
            kind_known: true,
        }]);
        assert!(
            app.shell
                .tabs
                .try_retire_index_build_resources(resources)
                .is_ok(),
            "fill paused reclaimer"
        );
    }

    seed_live_payload(&mut app, "full-rollback-active", 15_430);
    let active_tab_id = app.current_tab_id().expect("active tab");
    let request_id = app.shell.indexing.allocate_request_id(Some(active_tab_id));
    app.shell.indexing.pending_request_id = Some(request_id);
    app.shell.indexing.in_progress = true;
    app.shell.indexing.background_states.insert(
        request_id,
        BackgroundIndexState {
            source: Some(IndexSource::Walker),
            entries: entries("full-rollback-background"),
            replaced: true,
        },
    );
    app.shell.indexing.background_finalizations.insert(
        request_id,
        PendingBackgroundIndexFinalize::new(
            BackgroundIndexFinalizeIdentity {
                tab_id: active_tab_id,
                request_id,
                source: IndexSource::Walker,
            },
            BackgroundIndexFinalizePolicy {
                include_files: true,
                include_dirs: true,
                root: root.clone(),
                prefer_relative: false,
                ignore_case: true,
                ignore_list_enabled: false,
                ignore_terms_source: Arc::new(Vec::new()),
            },
            BackgroundIndexFinalizeInputs {
                initial_entries: entries("full-rollback-finalizer").into(),
                pending_entries: VecDeque::new(),
                continuation_entries: VecDeque::new(),
                discarded_entries: VecDeque::new(),
                discarded_pending_entries: VecDeque::new(),
                capture_filelist_paths: false,
            },
        ),
    );
    let mailbox = app
        .shell
        .indexing
        .response_mailboxes
        .lock()
        .expect("mailboxes")
        .get(&request_id)
        .cloned()
        .expect("request mailbox");
    let background_entries = {
        let state = app
            .shell
            .indexing
            .background_states
            .get(&request_id)
            .expect("background state");
        (state.entries.as_ptr(), state.entries.capacity())
    };
    let (finalizer_weight, finalizer_entries) = {
        let finalizer = app
            .shell
            .indexing
            .background_finalizations
            .get(&request_id)
            .expect("background finalizer");
        (
            finalizer.heavy_resource_weight(),
            deque_allocation(&finalizer.initial_entries),
        )
    };
    let mut live_before = live_inventory(&app);

    assert!(!app.try_retire_active_index_build_resources());
    live_before.metadata.notice = "Waiting for background tab resource reclamation".to_string();
    assert_eq!(live_inventory(&app), live_before);
    let restored_mailbox = app
        .shell
        .indexing
        .response_mailboxes
        .lock()
        .expect("mailboxes")
        .get(&request_id)
        .cloned()
        .expect("restored request mailbox");
    assert!(Arc::ptr_eq(&restored_mailbox, &mailbox));
    let restored_background = app
        .shell
        .indexing
        .background_states
        .get(&request_id)
        .expect("restored background state");
    assert_eq!(
        (
            restored_background.entries.as_ptr(),
            restored_background.entries.capacity()
        ),
        background_entries
    );
    let restored_finalizer = app
        .shell
        .indexing
        .background_finalizations
        .get(&request_id)
        .expect("restored background finalizer");
    assert_eq!(restored_finalizer.heavy_resource_weight(), finalizer_weight);
    assert_eq!(
        deque_allocation(&restored_finalizer.initial_entries),
        finalizer_entries
    );
    assert_eq!(
        app.shell.tabs.reclaimer_pending(),
        TAB_RESOURCE_RECLAIMER_CAPACITY
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_154_tab_switch_does_not_compact_sparse_payload_on_ui_path() {
    let root = test_root("tc-154-tab-sparse-payload-transfer");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), PAYLOAD_LEN, String::new());
    app.create_new_tab();

    seed_live_payload(&mut app, "active-sparse", 1543);
    seed_tab_payload(
        app.shell.tabs.get_mut(0).expect("inactive tab"),
        "inactive-sparse",
        1542,
    );
    app.shell.runtime.index.entries.reserve(8_192);
    app.shell.indexing.pending_entries.reserve(8_192);
    app.shell.indexing.pending_kind_paths.reserve(8_192);
    app.shell
        .indexing
        .incremental_filtered_entries
        .reserve(8_192);
    {
        let inactive = app.shell.tabs.get_mut(0).expect("inactive tab");
        inactive.index_state.index.entries.reserve(8_192);
        inactive.index_state.pending_index_entries.reserve(8_192);
        inactive.index_state.pending_kind_paths.reserve(8_192);
        inactive
            .index_state
            .incremental_filtered_entries
            .reserve(8_192);
    }
    let active_allocations = live_allocations(&app);
    let inactive_allocations = tab_allocations(app.shell.tabs.get(0).expect("inactive tab"));

    app.switch_to_tab_index(0);

    assert_eq!(live_allocations(&app), inactive_allocations);
    assert_eq!(
        tab_allocations(app.shell.tabs.get(1).expect("outgoing tab")),
        active_allocations
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
#[ignore = "release-mode tab transition latency guard"]
fn perf_tc_154_tab_transition_coordinator_p95_stays_below_hard_ceiling() {
    const ENTRY_COUNT: usize = 100_000;
    const SAMPLE_COUNT: usize = 50;
    const HARD_CEILING: Duration = Duration::from_millis(50);

    let root = test_root("tc-154-tab-transition-perf");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 1_000, String::new());
    app.create_new_tab();

    app.shell.runtime.index.entries = (0..ENTRY_COUNT)
        .map(|index| file_entry(PathBuf::from(format!("live-{index}.txt"))))
        .collect();
    app.shell.runtime.all_entries = Arc::new(app.shell.runtime.index.entries.clone());
    app.shell.runtime.entries = Arc::clone(&app.shell.runtime.all_entries);
    {
        let inactive = app.shell.tabs.get_mut(0).expect("inactive tab");
        inactive.index_state.index.entries = (0..ENTRY_COUNT)
            .map(|index| file_entry(PathBuf::from(format!("tab-{index}.txt"))))
            .collect();
        inactive.index_state.all_entries = Arc::new(inactive.index_state.index.entries.clone());
        inactive.index_state.entries = Arc::clone(&inactive.index_state.all_entries);
    }

    for _ in 0..6 {
        let next = if app.shell.tabs.active_tab_index() == 0 {
            1
        } else {
            0
        };
        app.switch_to_tab_index(next);
    }

    let mut samples = Vec::with_capacity(SAMPLE_COUNT);
    for _ in 0..SAMPLE_COUNT {
        let next = if app.shell.tabs.active_tab_index() == 0 {
            1
        } else {
            0
        };
        let started = Instant::now();
        app.switch_to_tab_index(next);
        samples.push(started.elapsed());
    }
    samples.sort_unstable();
    let p95_index = ((samples.len() * 95).div_ceil(100)).saturating_sub(1);
    let p95 = samples[p95_index];
    eprintln!(
        "TC-154 tab transition: entries={ENTRY_COUNT} samples={SAMPLE_COUNT} p95_ms={:.3}",
        p95.as_secs_f64() * 1_000.0
    );
    assert!(
        p95 < HARD_CEILING,
        "tab transition p95 {:?} exceeded {:?}",
        p95,
        HARD_CEILING
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_154_active_request_state_moves_to_background_slot_and_back() {
    let root = test_root("tc-154-active-request-transfer");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    let active_id = app.current_tab_id().expect("active tab id");

    app.shell.search.set_pending_request_id(Some(1541));
    app.shell.search.set_in_progress(true);
    app.shell.indexing.pending_request_id = Some(1542);
    app.shell.indexing.in_progress = true;
    app.shell.indexing.request_tabs.insert(1542, active_id);
    app.shell
        .indexing
        .latest_request_ids
        .lock()
        .expect("latest index requests")
        .insert(active_id, 1542);
    app.shell.indexing.request_tabs.insert(1542, active_id);
    app.shell
        .indexing
        .latest_request_ids
        .lock()
        .expect("latest index requests")
        .insert(active_id, 1542);
    app.shell.worker_bus.preview.pending_request_id = Some(1543);
    app.shell.worker_bus.preview.in_progress = true;
    app.shell.worker_bus.action.pending_request_id = Some(1544);
    app.shell.worker_bus.action.in_progress = true;
    app.shell.worker_bus.sort.pending_request_id = Some(1545);
    app.shell.worker_bus.sort.in_progress = true;
    app.bind_preview_request_to_tab(1543, active_id);
    app.bind_action_request_to_tab(1544, active_id);
    app.bind_sort_request_to_tab(1545, active_id);

    assert_eq!(
        app.shell
            .tabs
            .get(1)
            .expect("active scratch")
            .pending_action_request_id,
        None
    );
    app.switch_to_tab_index(0);

    let background = app.shell.tabs.get(1).expect("background tab");
    assert_eq!(background.pending_request_id, Some(1541));
    assert!(background.search_in_progress);
    assert_eq!(background.index_state.pending_index_request_id, Some(1542));
    assert!(background.index_state.index_in_progress);
    assert_eq!(app.shell.indexing.warm_tab_id, Some(active_id));
    assert_eq!(app.shell.indexing.warm_tab_id, Some(active_id));
    assert_eq!(background.pending_preview_request_id, Some(1543));
    assert!(background.preview_in_progress);
    assert_eq!(background.pending_action_request_id, Some(1544));
    assert!(background.action_in_progress);
    assert_eq!(background.result_state.pending_sort_request_id, Some(1545));
    assert!(background.result_state.sort_in_progress);

    app.switch_to_tab_index(1);

    assert_eq!(app.shell.search.pending_request_id(), Some(1541));
    assert!(app.shell.search.in_progress());
    assert_eq!(app.shell.indexing.pending_request_id, Some(1542));
    assert!(app.shell.indexing.in_progress);
    assert_eq!(
        app.shell
            .indexing
            .latest_request_ids
            .lock()
            .expect("latest index requests")
            .get(&active_id)
            .copied(),
        Some(1542),
        "promoting the warm tab must preserve its generation"
    );
    assert_eq!(
        app.shell
            .indexing
            .latest_request_ids
            .lock()
            .expect("latest index requests")
            .get(&active_id)
            .copied(),
        Some(1542),
        "promoting the warm tab must preserve its generation"
    );
    assert_eq!(app.shell.worker_bus.preview.pending_request_id, Some(1543));
    assert!(app.shell.worker_bus.preview.in_progress);
    assert_eq!(app.shell.worker_bus.action.pending_request_id, Some(1544));
    assert!(app.shell.worker_bus.action.in_progress);
    assert_eq!(app.shell.worker_bus.sort.pending_request_id, Some(1545));
    assert!(app.shell.worker_bus.sort.in_progress);
    assert_eq!(app.preview_request_tab(1543), Some(active_id));
    assert_eq!(app.action_request_tab(1544), Some(active_id));
    assert_eq!(app.sort_request_tab(1545), Some(active_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_154_stale_background_routes_never_mutate_active_scratch() {
    let root = test_root("tc-154-stale-active-owner-guard");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    let active_index = app.shell.tabs.active_tab_index();
    let active_id = app.current_tab_id().expect("active tab id");
    let scratch_path = root.join("scratch.txt");
    let stale_path = root.join("stale.txt");

    {
        let scratch = app
            .shell
            .tabs
            .get_mut(active_index)
            .expect("active scratch");
        scratch.index_state.index.entries = vec![file_entry(scratch_path.clone())];
        scratch.result_state.base_results = vec![(scratch_path.clone(), 1.0)];
        scratch.result_state.results = scratch.result_state.base_results.clone();
        scratch.result_state.preview = "scratch preview".to_string();
        scratch.notice = "scratch notice".to_string();
        scratch.pending_request_id = Some(2001);
        scratch.pending_preview_request_id = Some(2002);
        scratch.pending_action_request_id = Some(2003);
        scratch.result_state.pending_sort_request_id = Some(2004);
    }

    for (response, terminal) in [
        (
            IndexResponse::Batch {
                request_id: 2101,
                entries: vec![IndexEntry {
                    path: stale_path.clone(),
                    kind: EntryKind::file(),
                    kind_known: true,
                }],
            },
            false,
        ),
        (
            IndexResponse::ReplaceAll {
                request_id: 2102,
                entries: vec![IndexEntry {
                    path: stale_path.clone(),
                    kind: EntryKind::file(),
                    kind_known: true,
                }],
            },
            false,
        ),
        (
            IndexResponse::Finished {
                request_id: 2103,
                source: IndexSource::Walker,
            },
            true,
        ),
    ] {
        let request_id = match &response {
            IndexResponse::Batch { request_id, .. }
            | IndexResponse::ReplaceAll { request_id, .. }
            | IndexResponse::Finished { request_id, .. } => *request_id,
            _ => unreachable!("fixture only uses batch/replace-all/finished"),
        };
        let effect = app.apply_background_index_response(active_index, response);
        assert_eq!(effect.cleanup_request_id, terminal.then_some(request_id));
    }

    app.apply_background_search_response(
        active_id,
        SearchResponse {
            request_id: 2201,
            results: vec![(stale_path.clone(), 9.0)],
            total_match_count: 1,
            sort_mode: ResultSortMode::Score,
            sort_scope: ResultSortScope::ShownResults,
            error: None,
        },
    );

    app.bind_preview_request_to_tab(2202, active_id);
    app.apply_background_preview_response(PreviewResponse {
        request_id: 2202,
        path: stale_path.clone(),
        preview: "stale preview".to_string(),
    });

    app.bind_action_request_to_tab(2203, active_id);
    app.apply_background_action_response(ActionResponse {
        request_id: 2203,
        notice: "stale notice".to_string(),
    });

    app.bind_sort_request_to_tab(2204, active_id);
    app.apply_background_sort_response(SortMetadataResponse {
        request_id: 2204,
        entries: Vec::new(),
        mode: ResultSortMode::NameAsc,
    });

    let scratch = app.shell.tabs.get(active_index).expect("active scratch");
    assert_eq!(scratch.index_state.index.entries[0], scratch_path);
    assert_eq!(scratch.result_state.base_results[0].0, scratch_path);
    assert_eq!(scratch.result_state.results[0].0, scratch_path);
    assert_eq!(scratch.result_state.preview, "scratch preview");
    assert_eq!(scratch.notice, "scratch notice");
    assert_eq!(scratch.pending_request_id, Some(2001));
    assert_eq!(scratch.pending_preview_request_id, Some(2002));
    assert_eq!(scratch.pending_action_request_id, Some(2003));
    assert_eq!(scratch.result_state.pending_sort_request_id, Some(2004));
    let _ = fs::remove_dir_all(&root);
}

use super::*;
use crate::app::{
    BackgroundIndexFinalizeIdentity, BackgroundIndexFinalizeInputs, BackgroundIndexFinalizePolicy,
    PendingBackgroundIndexFinalize,
};
use std::sync::atomic::{AtomicBool, Ordering};

#[test]
fn deferred_filelist_starts_after_index_finished() {
    let root = test_root("filelist-after-index-finished");
    fs::create_dir_all(&root).expect("create dir");
    let path = root.join("main.rs");
    fs::write(&path, "fn main() {}").expect("write file");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (filelist_tx, filelist_rx) = mpsc::channel::<FileListRequest>();
    app.shell.worker_bus.filelist.tx = filelist_tx;
    let (index_tx, index_rx) = mpsc::channel::<IndexResponse>();
    app.shell.indexing.rx = index_rx;

    app.shell.runtime.use_filelist = false;
    app.shell.indexing.in_progress = true;
    let tab_id = app.current_tab_id().expect("tab id");
    app.create_filelist();
    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_after_index
        .is_some());
    let request_id = app
        .shell
        .indexing
        .pending_request_id
        .expect("pending index request");

    index_tx
        .send(IndexResponse::Batch {
            request_id,
            entries: vec![IndexEntry {
                path: path.clone(),
                kind: EntryKind::file(),
                kind_known: true,
            }],
        })
        .expect("send batch");

    index_tx
        .send(IndexResponse::Finished {
            request_id,
            source: IndexSource::Walker,
        })
        .expect("send finished");
    for _ in 0..8 {
        app.poll_index_response();
        app.try_finish_active_index_after_pending_drain();
    }

    if app
        .shell
        .features
        .filelist
        .workflow
        .pending_ancestor_confirmation
        .is_some()
    {
        app.skip_pending_filelist_ancestor_propagation();
    }

    let req = filelist_rx
        .try_recv()
        .unwrap_or_else(|error| {
            panic!(
                "filelist request should be sent: {error:?}; notice={}; pending_after_index={}; pending_finish={}; refresh_after={:?}",
                app.shell.runtime.notice,
                app.shell
                    .features
                    .filelist
                    .workflow
                    .pending_after_index
                    .is_some(),
                app.shell.indexing.pending_finish.is_some(),
                app.shell.indexing.refresh_after_pending_finish
            )
        });
    assert_eq!(req.tab_id, tab_id);
    assert_eq!(req.root, root);
    assert_eq!(req.entries, Some(vec![path]));
    assert_eq!(req.phase, FileListRequestPhase::Preflight);
    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_after_index
        .is_none());
    assert!(app.shell.features.filelist.workflow.in_progress);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn active_filelist_finalization_waits_when_both_finalizer_slots_are_full() {
    let root = test_root("filelist-active-finalizer-capacity");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (filelist_tx, filelist_rx) = mpsc::channel::<FileListRequest>();
    app.shell.worker_bus.filelist.tx = filelist_tx;
    let tab_id = app.current_tab_id().expect("tab id");
    let active_request_id = 903;
    app.shell.features.filelist.workflow.pending_after_index = Some(PendingFileListAfterIndex {
        tab_id,
        root: root.clone(),
        index_request_id: Some(active_request_id),
    });
    app.shell.indexing.pending_finish = Some(PendingActiveIndexFinish {
        request_id: active_request_id,
        source: IndexSource::Walker,
    });
    app.shell.indexing.build.index.entries = vec![unknown_entry(root.join("entry.txt"))];

    for request_id in [901, 902] {
        let finalizer = PendingBackgroundIndexFinalize::new(
            BackgroundIndexFinalizeIdentity {
                tab_id,
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
                initial_entries: VecDeque::new(),
                pending_entries: VecDeque::new(),
                continuation_entries: VecDeque::new(),
                discarded_entries: VecDeque::new(),
                discarded_pending_entries: VecDeque::new(),
                capture_filelist_paths: false,
            },
        );
        app.shell
            .indexing
            .background_finalizations
            .insert(request_id, finalizer);
    }

    assert!(!app.try_finish_active_index_after_pending_drain());
    assert!(app.shell.indexing.pending_finish.is_some());
    assert_eq!(app.shell.indexing.build.index.entries.len(), 1);
    assert!(app.shell.indexing.background_finalizations.is_full());
    app.shell
        .indexing
        .background_finalizations
        .remove(&901)
        .expect("free one finalizer slot");
    assert!(app.try_finish_active_index_after_pending_drain());
    let preflight = filelist_rx
        .try_recv()
        .expect("capacity release should resume FileList preflight");
    assert_eq!(preflight.phase, FileListRequestPhase::Preflight);
    assert_eq!(preflight.entries.as_ref().map(Vec::len), Some(1));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn unmatched_active_terminal_does_not_supply_create_filelist_snapshot() {
    let root = test_root("filelist-unmatched-active-terminal");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (filelist_tx, filelist_rx) = mpsc::channel::<FileListRequest>();
    app.shell.worker_bus.filelist.tx = filelist_tx;
    let (request_tx, request_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = request_tx;
    reset_index_request_state_for_test(&mut app);
    let tab_id = app.current_tab_id().expect("tab id");
    let old_request_id = 904;
    app.shell.features.filelist.workflow.pending_after_index = Some(PendingFileListAfterIndex {
        tab_id,
        root: root.clone(),
        index_request_id: None,
    });
    app.shell.indexing.pending_finish = Some(PendingActiveIndexFinish {
        request_id: old_request_id,
        source: IndexSource::Walker,
    });
    app.shell.indexing.refresh_after_pending_finish =
        Some(super::PendingIndexRefreshMode::CreateFileListWalker);
    app.shell.indexing.build.index.entries = vec![unknown_entry(root.join("partial.txt"))];

    assert!(app.try_finish_active_index_after_pending_drain());

    assert!(filelist_rx.try_recv().is_err());
    let dedicated = request_rx.try_recv().expect("dedicated Walker request");
    assert!(dedicated.complete_walker_snapshot);
    assert_ne!(dedicated.request_id, old_request_id);
    assert_eq!(
        app.shell
            .features
            .filelist
            .workflow
            .pending_after_index
            .as_ref()
            .and_then(|pending| pending.index_request_id),
        Some(dedicated.request_id)
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn unmatched_background_terminal_does_not_supply_create_filelist_snapshot() {
    let root = test_root("filelist-unmatched-background-terminal");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    let background_tab_index = 0;
    let background_tab_id = app
        .shell
        .tabs
        .get(background_tab_index)
        .expect("background tab")
        .id;
    let (filelist_tx, filelist_rx) = mpsc::channel::<FileListRequest>();
    app.shell.worker_bus.filelist.tx = filelist_tx;
    let (request_tx, request_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = request_tx;
    reset_index_request_state_for_test(&mut app);
    let old_request_id = app
        .shell
        .indexing
        .allocate_request_id(Some(background_tab_id));
    {
        let tab = app
            .shell
            .tabs
            .get_mut(background_tab_index)
            .expect("background tab");
        tab.index_state.pending_index_request_id = Some(old_request_id);
        tab.index_state.index_in_progress = true;
        tab.index_state.refresh_after_pending_finish =
            Some(super::PendingIndexRefreshMode::CreateFileListWalker);
    }
    app.shell.indexing.background_states.insert(
        old_request_id,
        BackgroundIndexState {
            source: Some(IndexSource::Walker),
            entries: vec![file_entry(root.join("partial.txt"))],
            replaced: true,
        },
    );
    app.shell.features.filelist.workflow.pending_after_index = Some(PendingFileListAfterIndex {
        tab_id: background_tab_id,
        root: root.clone(),
        index_request_id: None,
    });

    app.handle_background_index_response(
        background_tab_index,
        IndexResponse::Finished {
            request_id: old_request_id,
            source: IndexSource::Walker,
        },
    );

    assert!(filelist_rx.try_recv().is_err());
    let dedicated = request_rx
        .try_recv()
        .expect("dedicated background Walker request");
    assert_eq!(dedicated.tab_id, background_tab_id);
    assert!(dedicated.complete_walker_snapshot);
    assert_ne!(dedicated.request_id, old_request_id);
    assert_eq!(
        app.shell
            .features
            .filelist
            .workflow
            .pending_after_index
            .as_ref()
            .and_then(|pending| pending.index_request_id),
        Some(dedicated.request_id)
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn deferred_filelist_is_canceled_when_root_changes() {
    let root_old = test_root("filelist-deferred-cancel-old");
    let root_new = test_root("filelist-deferred-cancel-new");
    fs::create_dir_all(&root_old).expect("create old dir");
    fs::create_dir_all(&root_new).expect("create new dir");
    let mut app = FlistWalkerApp::new(root_old.clone(), 50, String::new());
    let (index_tx, _index_rx) = bounded_request_channel::<IndexRequest>(2);
    app.shell.indexing.tx = index_tx;
    let tab_id = app.current_tab_id().expect("tab id");
    app.shell.features.filelist.workflow.pending_after_index = Some(PendingFileListAfterIndex {
        tab_id,
        root: root_old.clone(),
        index_request_id: None,
    });
    app.shell.runtime.root = root_new.clone();

    app.request_index_refresh();

    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_after_index
        .is_none());
    assert!(app
        .shell
        .runtime
        .notice
        .contains("Deferred Create File List canceled"));
    let _ = fs::remove_dir_all(&root_old);
    let _ = fs::remove_dir_all(&root_new);
}

#[test]
fn filelist_finish_reindexes_original_tab_after_tab_switch() {
    let root_a = test_root("filelist-finish-background-tab-a");
    let root_b = test_root("filelist-finish-background-tab-b");
    fs::create_dir_all(&root_a).expect("create root a");
    fs::create_dir_all(&root_b).expect("create root b");

    let mut app = FlistWalkerApp::new(root_a.clone(), 50, String::new());
    let source_tab_id = app.current_tab_id().expect("source tab id");
    app.shell.runtime.use_filelist = false;
    let active_tab = app.shell.tabs.active_tab;
    if let Some(tab) = app.shell.tabs.get_mut(active_tab) {
        tab.use_filelist = false;
    }
    app.create_new_tab();
    app.apply_root_change(root_b.clone());
    app.switch_to_tab_index(1);

    let (index_tx, index_rx) = bounded_request_channel::<IndexRequest>(2);
    let (filelist_tx, filelist_rx) = mpsc::channel::<FileListResponse>();
    app.shell.indexing.tx = index_tx;
    reset_index_request_state_for_test(&mut app);
    app.shell.worker_bus.filelist.rx = filelist_rx;
    app.shell.features.filelist.workflow.pending_request_id = Some(1);
    app.shell.features.filelist.workflow.pending_request_tab_id = Some(source_tab_id);
    app.shell.features.filelist.workflow.pending_root = Some(root_a.clone());
    app.shell.features.filelist.workflow.pending_cancel = Some(Arc::new(AtomicBool::new(false)));
    app.shell.features.filelist.workflow.in_progress = true;

    filelist_tx
        .send(FileListResponse::Finished {
            request_id: 1,
            root: root_a.clone(),
            path: root_a.join("FileList.txt"),
            count: 1,
        })
        .expect("send filelist finished");
    app.poll_filelist_response();

    let req = index_rx
        .try_recv()
        .expect("background reindex request should be sent");
    assert_eq!(req.tab_id, source_tab_id);
    assert_eq!(req.root, root_a);
    assert!(req.use_filelist);
    let source_tab = app
        .shell
        .tabs
        .iter()
        .find(|tab| tab.id == source_tab_id)
        .expect("source tab should remain");
    assert!(source_tab.use_filelist);
    assert!(source_tab.index_state.index_in_progress);
    assert_eq!(app.shell.tabs.active_tab, 1);
    assert_eq!(app.shell.runtime.root, root_b);
    let _ = fs::remove_dir_all(&root_a);
    let _ = fs::remove_dir_all(&root_b);
}

#[test]
fn filelist_finish_ignores_original_tab_when_its_root_changed() {
    let root_old = test_root("filelist-finish-root-changed-old");
    let root_new = test_root("filelist-finish-root-changed-new");
    fs::create_dir_all(&root_old).expect("create old root");
    fs::create_dir_all(&root_new).expect("create new root");

    let mut app = FlistWalkerApp::new(root_old.clone(), 50, String::new());
    let source_tab_id = app.current_tab_id().expect("source tab id");
    app.shell.runtime.use_filelist = false;
    let active_tab = app.shell.tabs.active_tab;
    if let Some(tab) = app.shell.tabs.get_mut(active_tab) {
        tab.use_filelist = false;
        tab.root = root_new.clone();
    }
    app.shell.runtime.root = root_new.clone();

    let (index_tx, index_rx) = bounded_request_channel::<IndexRequest>(2);
    let (filelist_tx, filelist_rx) = mpsc::channel::<FileListResponse>();
    app.shell.indexing.tx = index_tx;
    app.shell.worker_bus.filelist.rx = filelist_rx;
    app.shell.features.filelist.workflow.pending_request_id = Some(2);
    app.shell.features.filelist.workflow.pending_request_tab_id = Some(source_tab_id);
    app.shell.features.filelist.workflow.pending_root = Some(root_old.clone());
    app.shell.features.filelist.workflow.pending_cancel = Some(Arc::new(AtomicBool::new(false)));
    app.shell.features.filelist.workflow.in_progress = true;

    filelist_tx
        .send(FileListResponse::Finished {
            request_id: 2,
            root: root_old.clone(),
            path: root_old.join("FileList.txt"),
            count: 1,
        })
        .expect("send filelist finished");
    app.poll_filelist_response();

    assert!(index_rx.try_recv().is_err());
    let source_tab = app
        .shell
        .tabs
        .iter()
        .find(|tab| tab.id == source_tab_id)
        .expect("source tab should remain");
    assert!(!source_tab.use_filelist);
    assert!(!source_tab.index_state.index_in_progress);
    let _ = fs::remove_dir_all(&root_old);
    let _ = fs::remove_dir_all(&root_new);
}

#[test]
fn background_index_send_failure_clears_pending_state_for_target_tab() {
    let root_a = test_root("background-index-send-failure-a");
    let root_b = test_root("background-index-send-failure-b");
    fs::create_dir_all(&root_a).expect("create root a");
    fs::create_dir_all(&root_b).expect("create root b");

    let mut app = FlistWalkerApp::new(root_a.clone(), 50, String::new());
    app.create_new_tab();
    app.shell.runtime.root = root_b.clone();
    let active_tab = app.shell.tabs.active_tab;
    if let Some(tab) = app.shell.tabs.get_mut(active_tab) {
        tab.root = root_b.clone();
    }
    app.sync_active_tab_state();
    app.switch_to_tab_index(0);
    app.shell
        .tabs
        .get_mut(0)
        .expect("tab 0")
        .index_state
        .set_lifecycle_for_test(TabResourceLifecycle::Dormant);

    let (_, rx) = bounded_request_channel::<IndexRequest>(2);
    let (closed_tx, _) = bounded_request_channel::<IndexRequest>(2);
    drop(rx);
    app.shell.indexing.tx = closed_tx;

    app.request_background_index_refresh_for_tab(1);

    let background_tab = app.shell.tabs.get(1).expect("background tab");
    assert!(!background_tab.index_state.index_in_progress);
    assert_eq!(background_tab.index_state.pending_index_request_id, None);
    assert!(background_tab.index_state.build.pending_entries.is_empty());
    assert_eq!(
        background_tab.index_state.lifecycle(),
        TabResourceLifecycle::Failed
    );
    assert!(background_tab
        .notice
        .contains("Index worker is unavailable"));
    assert!(app
        .shell
        .runtime
        .notice
        .contains("Index worker is unavailable"));

    let _ = fs::remove_dir_all(&root_a);
    let _ = fs::remove_dir_all(&root_b);
}

#[test]
fn cancel_create_filelist_marks_inflight_request() {
    let root = test_root("filelist-cancel-inflight");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let cancel = Arc::new(AtomicBool::new(false));
    app.shell.features.filelist.workflow.pending_request_id = Some(77);
    app.shell.features.filelist.workflow.pending_request_tab_id = app.current_tab_id();
    app.shell.features.filelist.workflow.pending_root = Some(root.clone());
    app.shell.features.filelist.workflow.pending_cancel = Some(Arc::clone(&cancel));
    app.shell.features.filelist.workflow.in_progress = true;
    app.shell.features.filelist.workflow.cancel_requested = false;

    app.cancel_create_filelist();

    assert!(cancel.load(Ordering::Relaxed));
    assert!(app.shell.features.filelist.workflow.cancel_requested);
    assert!(app
        .shell
        .runtime
        .notice
        .contains("Canceling Create File List"));
    let _ = fs::remove_dir_all(&root);
}

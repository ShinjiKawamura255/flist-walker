use super::*;
use crate::app::index_mailbox::IndexResponseMailbox;
use crate::app::{
    BackgroundIndexFinalizeIdentity, BackgroundIndexFinalizeInputs, BackgroundIndexFinalizePolicy,
    PendingBackgroundIndexFinalize,
};

fn install_mailbox(
    app: &mut FlistWalkerApp,
    request_id: u64,
    capacity: usize,
) -> Arc<IndexResponseMailbox> {
    let mailbox = Arc::new(IndexResponseMailbox::with_data_capacity(capacity));
    app.shell
        .indexing
        .response_mailboxes
        .lock()
        .expect("mailboxes")
        .insert(request_id, Arc::clone(&mailbox));
    mailbox
}

fn publish_batches(mailbox: &IndexResponseMailbox, request_id: u64, root: &Path, count: usize) {
    for sequence in 0..count {
        mailbox
            .try_publish(IndexResponse::Batch {
                request_id,
                entries: vec![IndexEntry {
                    path: root.join(format!("{request_id}-{sequence}.txt")),
                    kind: EntryKind::file(),
                    kind_known: true,
                }],
            })
            .expect("publish batch");
    }
}

fn register_background_request(app: &mut FlistWalkerApp, tab_index: usize) -> u64 {
    let tab_id = app.shell.tabs.get(tab_index).expect("background tab").id;
    let request_id = app.shell.indexing.allocate_request_id(Some(tab_id));
    let tab = app.shell.tabs.get_mut(tab_index).expect("background tab");
    tab.index_state.pending_index_request_id = Some(request_id);
    tab.index_state.index_in_progress = true;
    request_id
}

#[test]
fn tc_207_one_frame_mailbox_arbitration_is_active_then_warm_then_sorted_and_retains_tail() {
    let root = test_root("tc-207-one-frame-mailbox-order");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.create_new_tab();
    app.create_new_tab();
    reset_index_request_state_for_test(&mut app);

    let residual_low = register_background_request(&mut app, 0);
    let residual_high = register_background_request(&mut app, 1);
    let warm = register_background_request(&mut app, 2);
    let active_tab_id = app.current_tab_id().expect("active tab");
    let active = app.shell.indexing.allocate_request_id(Some(active_tab_id));
    app.shell.indexing.pending_request_id = Some(active);
    app.shell.indexing.in_progress = true;
    app.shell.indexing.warm_tab_id = app.shell.tabs.get(2).map(|tab| tab.id);

    let mailboxes = [residual_low, residual_high, warm, active]
        .map(|request_id| (request_id, install_mailbox(&mut app, request_id, 64)));
    for (request_id, mailbox) in &mailboxes {
        publish_batches(mailbox, *request_id, &root, 60);
    }

    app.shell.indexing.mailbox_selection_trace.clear();
    app.poll_index_response_with_budget_for_test(Duration::from_secs(1));

    let trace = &app.shell.indexing.mailbox_selection_trace;
    assert_eq!(trace.len(), 64);
    assert_eq!(&trace[..48], vec![active; 48].as_slice());
    assert_eq!(&trace[48..56], vec![warm; 8].as_slice());
    assert_eq!(&trace[56..], vec![residual_low; 8].as_slice());
    assert!(
        mailboxes[0].1.has_payload(),
        "residual tail must stay queued"
    );
    assert!(
        mailboxes[1].1.has_payload(),
        "later residual mailbox must stay queued"
    );
    assert!(mailboxes[2].1.has_payload(), "warm tail must stay queued");
    assert!(mailboxes[3].1.has_payload(), "active tail must stay queued");
}

#[test]
fn tc_207_one_frame_active_backlog_blocks_active_mailbox_without_blocking_warm() {
    let root = test_root("tc-207-one-frame-active-backlog");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    reset_index_request_state_for_test(&mut app);

    let warm = register_background_request(&mut app, 0);
    let active_tab_id = app.current_tab_id().expect("active tab");
    let active = app.shell.indexing.allocate_request_id(Some(active_tab_id));
    app.shell.indexing.pending_request_id = Some(active);
    app.shell.indexing.in_progress = true;
    app.shell.indexing.warm_tab_id = app.shell.tabs.get(0).map(|tab| tab.id);
    app.shell.indexing.pending_entries_request_id = Some(active);
    app.shell.indexing.build.pending_entries = (0..32_768)
        .map(|sequence| IndexEntry {
            path: root.join(format!("backlog-{sequence}.txt")),
            kind: EntryKind::file(),
            kind_known: true,
        })
        .collect();

    let active_mailbox = install_mailbox(&mut app, active, 64);
    let warm_mailbox = install_mailbox(&mut app, warm, 64);
    publish_batches(&active_mailbox, active, &root, 1);
    publish_batches(&warm_mailbox, warm, &root, 1);

    app.shell.indexing.mailbox_selection_trace.clear();
    app.poll_index_response_with_budget_for_test(Duration::ZERO);

    assert_eq!(app.shell.indexing.mailbox_selection_trace, vec![warm]);
    assert!(active_mailbox.has_payload());
    assert!(!warm_mailbox.has_payload());
}

fn empty_finalizer(tab_id: u64, request_id: u64, root: &Path) -> PendingBackgroundIndexFinalize {
    PendingBackgroundIndexFinalize::new(
        BackgroundIndexFinalizeIdentity {
            tab_id,
            request_id,
            source: IndexSource::Walker,
        },
        BackgroundIndexFinalizePolicy {
            include_files: true,
            include_dirs: true,
            root: root.to_path_buf(),
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
    )
}

#[test]
fn tc_207_one_frame_full_finalizers_retain_latest_terminal_but_admit_stale_terminal() {
    let root = test_root("tc-207-one-frame-full-terminal-admission");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.create_new_tab();
    app.create_new_tab();
    reset_index_request_state_for_test(&mut app);

    for tab_index in 0..2 {
        let request_id = register_background_request(&mut app, tab_index);
        let tab_id = app.shell.tabs.get(tab_index).expect("finalizer tab").id;
        app.shell
            .indexing
            .background_finalizations
            .insert(request_id, empty_finalizer(tab_id, request_id, &root));
    }
    assert!(app.shell.indexing.background_finalizations.is_full());

    let stale_tab_id = app.shell.tabs.get(2).expect("stale tab").id;
    let stale = app.shell.indexing.allocate_request_id(Some(stale_tab_id));
    let stale_mailbox = install_mailbox(&mut app, stale, 8);
    stale_mailbox
        .try_publish(IndexResponse::Canceled { request_id: stale })
        .expect("publish stale terminal");

    let latest = register_background_request(&mut app, 2);
    let latest_mailbox = install_mailbox(&mut app, latest, 8);
    latest_mailbox
        .try_publish(IndexResponse::Finished {
            request_id: latest,
            source: IndexSource::Walker,
        })
        .expect("publish latest terminal");

    app.shell.indexing.mailbox_selection_trace.clear();
    app.poll_index_response_with_budget_for_test(Duration::from_secs(1));

    assert_eq!(app.shell.indexing.mailbox_selection_trace, vec![stale]);
    assert!(latest_mailbox.has_terminal_response());
    assert!(!stale_mailbox.has_terminal_response());
}

#[test]
fn tc_207_poll_loop_delegates_response_mutation_to_typed_owner() {
    let source = include_str!("../../pipeline.rs");
    let start = source
        .find("fn poll_index_response_with_budget(&mut self")
        .expect("poll function");
    let end = source[start..]
        .find("\n    fn ensure_entry_filters")
        .map(|offset| start + offset)
        .expect("next pipeline function");
    let body = &source[start..end];

    assert!(body.contains("IndexResponseApplicationOwner"));
    for forbidden in [
        "resource_state",
        "pending_finish",
        "pending_entries",
        "runtime.",
        "set_notice(",
        "stage_stale_",
        "queue_index_batch",
        "incremental_",
        "inflight_requests",
    ] {
        assert!(
            !body.contains(forbidden),
            "poll loop must delegate `{forbidden}` mutation to the response owner"
        );
    }
}

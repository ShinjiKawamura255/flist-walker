use super::*;
use crate::ui_model::{PagedTextPreview, PreviewPageError, PreviewPageState};

fn settle_preview(app: &mut FlistWalkerApp) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while app.shell.worker_bus.preview.in_progress {
        app.poll_preview_response();
        assert!(Instant::now() < deadline, "preview worker did not settle");
        thread::yield_now();
    }
}

#[test]
fn gui_paged_preview_loads_initial_and_more_without_duplication() {
    let root = test_root("paged-preview-gui");
    fs::create_dir_all(&root).expect("create root");
    let path = root.join("sample.txt");
    let body = (1..=601)
        .map(|index| format!("line {index}\n"))
        .collect::<String>();
    fs::write(&path, &body).expect("create fixture");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.ui.show_preview = true;
    app.shell.runtime.committed_for_test_mut().results = vec![(path.clone(), 0.0)];
    app.shell.runtime.committed_for_test_mut().current_row = Some(0);
    app.set_entry_kind(&path, EntryKind::file());
    app.request_preview_for_current();
    settle_preview(&mut app);
    let first = app.paged_preview_for_current().expect("first document");
    assert_eq!(first.line_count(), 100);
    assert_eq!(first.state(), PreviewPageState::More);
    app.request_paged_preview_more();
    settle_preview(&mut app);
    let second = app.paged_preview_for_current().expect("second document");
    assert_eq!(second.line_count(), 600);
    assert_eq!(second.line(100), Some("line 101"));
    app.request_paged_preview_more();
    settle_preview(&mut app);
    let final_page = app.paged_preview_for_current().expect("final document");
    assert_eq!(final_page.body(), body);
    assert_eq!(final_page.state(), PreviewPageState::Eof);
    fs::remove_dir_all(root).expect("cleanup root");
}

#[test]
fn gui_paged_preview_failed_more_keeps_prior_document_and_requires_reload() {
    let root = test_root("paged-preview-late-binary");
    fs::create_dir_all(&root).expect("create root");
    let path = root.join("sample.txt");
    let mut input = "safe\n".repeat(100).into_bytes();
    input.extend_from_slice(b"\0binary\n");
    fs::write(&path, input).expect("create fixture");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.ui.show_preview = true;
    app.shell.runtime.committed_for_test_mut().results = vec![(path.clone(), 0.0)];
    app.shell.runtime.committed_for_test_mut().current_row = Some(0);
    app.set_entry_kind(&path, EntryKind::file());
    app.request_preview_for_current();
    settle_preview(&mut app);
    let before = app
        .paged_preview_for_current()
        .expect("first document")
        .body()
        .to_owned();
    app.request_paged_preview_more();
    settle_preview(&mut app);
    assert_eq!(
        app.paged_preview_for_current()
            .expect("retained document")
            .body(),
        before
    );
    assert_eq!(app.paged_preview_view.error, Some(PreviewPageError::Binary));
    app.request_paged_preview_more();
    assert!(!app.shell.worker_bus.preview.in_progress);
    fs::remove_dir_all(root).expect("cleanup root");
}

#[test]
fn gui_preview_navigation_keeps_one_worker_request_and_only_latest_pending_target() {
    let root = test_root("paged-preview-latest-only");
    fs::create_dir_all(&root).expect("create root");
    let paths = (0..3)
        .map(|index| root.join(format!("{index}.txt")))
        .collect::<Vec<_>>();
    for path in &paths {
        fs::write(path, "content").expect("write fixture");
    }
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.ui.show_preview = true;
    app.shell.runtime.committed_for_test_mut().results =
        paths.iter().cloned().map(|path| (path, 0.0)).collect();
    for path in &paths {
        app.set_entry_kind(path, EntryKind::file());
    }
    let (request_tx, request_rx) = std::sync::mpsc::channel::<PreviewRequest>();
    app.shell.worker_bus.preview.tx = request_tx;
    app.shell.worker_bus.preview.worker_inflight_request_id = None;
    for row in 0..3 {
        app.shell.runtime.committed_for_test_mut().current_row = Some(row);
        app.request_preview_for_current();
    }
    let first = request_rx.try_recv().expect("first dispatched");
    assert_eq!(first.path, paths[0]);
    assert!(request_rx.try_recv().is_err());
    assert_eq!(
        app.shell
            .worker_bus
            .preview
            .latest_request
            .as_ref()
            .map(|request| request.path.as_path()),
        Some(paths[2].as_path())
    );
    assert_eq!(
        app.preview_request_tab(first.request_id),
        Some(app.current_tab_id().unwrap())
    );
    app.apply_background_preview_response(PreviewResponse {
        canceled: true,
        request_id: first.request_id,
        path: paths[0].clone(),
        preview: String::new(),
        document: None,
        page_error: None,
        is_more: false,
    });
    let latest = request_rx
        .try_recv()
        .expect("latest dispatched after first settles");
    assert_eq!(latest.path, paths[2]);
    assert!(request_rx.try_recv().is_err());
    assert!(app.shell.worker_bus.preview.latest_request.is_none());
    fs::remove_dir_all(root).expect("cleanup root");
}

#[test]
fn gui_preview_copy_layout_contains_only_body_and_color_toggle_preserves_text() {
    let root = test_root("paged-preview-copy-layout");
    fs::create_dir_all(&root).expect("create root");
    let path = root.join("sample.rs");
    fs::write(&path, "fn main() { println!(\"日本語\"); }\n").expect("write fixture");
    let document = PagedTextPreview::initial(&path, &|| false).expect("preview");
    let ctx = egui::Context::default();
    let mut colored = None;
    let mut plain = None;
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
        colored = Some(crate::app::render_panels::preview_line_job(
            &document, 0, true, ui,
        ));
        plain = Some(crate::app::render_panels::preview_line_job(
            &document, 0, false, ui,
        ));
    });
    let colored = colored.expect("colored job");
    let plain = plain.expect("plain job");
    assert_eq!(colored.text, document.line(0).unwrap());
    assert_eq!(plain.text, colored.text);
    assert!(
        !colored.text.starts_with("    1"),
        "line number must be a separate nonselectable label"
    );
    assert!(colored.sections.len() > 1);
    assert_eq!(plain.sections.len(), 1);
    fs::remove_dir_all(root).expect("cleanup root");
}

#[test]
fn gui_long_line_copy_layout_excludes_truncation_marker() {
    let root = test_root("paged-preview-long-line-copy");
    fs::create_dir_all(&root).expect("create root");
    let path = root.join("long.txt");
    fs::write(&path, format!("{}尾\n", "a".repeat(4_096))).expect("write fixture");
    let document = PagedTextPreview::initial(&path, &|| false).expect("preview");
    let ctx = egui::Context::default();
    let mut selected_body = None;
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
        selected_body = Some(crate::app::render_panels::preview_line_job(
            &document, 0, false, ui,
        ));
    });
    let selected_body = selected_body.expect("body layout");
    assert_eq!(selected_body.text, "a".repeat(4_096));
    assert!(!selected_body.text.contains("truncated"));
    fs::remove_dir_all(root).expect("cleanup root");
}

#[test]
fn background_more_failure_preserves_tab_document_and_error_is_tab_scoped() {
    let root = test_root("paged-preview-background-error");
    fs::create_dir_all(&root).expect("create root");
    let path = root.join("sample.txt");
    fs::write(&path, "safe\n".repeat(101)).expect("write fixture");
    let document = Arc::new(PagedTextPreview::initial(&path, &|| false).expect("first page"));
    let original_body = document.body().to_owned();
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.ui.show_preview = true;
    app.shell.runtime.committed_for_test_mut().results = vec![(path.clone(), 0.0)];
    app.shell.runtime.committed_for_test_mut().current_row = Some(0);
    app.shell.runtime.committed_for_test_mut().preview_document = Some(document);
    let first_tab_id = app.current_tab_id().expect("tab id");
    app.create_new_tab();
    let request_id = 70_001;
    let first_tab = app.shell.tabs.get_mut(0).expect("background tab");
    first_tab.pending_preview_request_id = Some(request_id);
    first_tab.preview_in_progress = true;
    app.bind_preview_request_to_tab(request_id, first_tab_id);
    app.apply_background_preview_response(PreviewResponse {
        request_id,
        path: path.clone(),
        preview: String::new(),
        document: None,
        page_error: Some(PreviewPageError::Binary),
        canceled: false,
        is_more: true,
    });
    let first_tab = app.shell.tabs.get(0).expect("background tab");
    assert_eq!(
        first_tab
            .result_state
            .committed
            .preview_document
            .as_ref()
            .map(|doc| doc.body()),
        Some(original_body.as_str())
    );
    app.switch_to_tab_index(0);
    assert_eq!(app.paged_preview_view.error, Some(PreviewPageError::Binary));
    assert_eq!(
        app.paged_preview_for_current()
            .expect("retained document")
            .body(),
        original_body
    );
    app.switch_to_tab_index(1);
    assert_eq!(app.paged_preview_view.error, None);
    fs::remove_dir_all(root).expect("cleanup root");
}

#[test]
fn preview_resident_budget_deduplicates_shared_documents_and_evicts_cold_tabs() {
    let root = test_root("paged-preview-resident-budget");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    for _ in 0..4 {
        app.create_new_tab();
    }
    let mut documents = Vec::new();
    for index in 0..5 {
        let path = root.join(format!("{index}.txt"));
        fs::write(&path, "small\n").expect("write fixture");
        let mut document = PagedTextPreview::initial(&path, &|| false).expect("document");
        document.reserve_body_capacity_for_test(7 * 1024 * 1024);
        assert!(document.capacity_bytes() <= 8 * 1024 * 1024);
        documents.push(Arc::new(document));
    }
    app.shell
        .runtime
        .set_preview_document(Arc::clone(&documents[0]))
        .expect("active");
    for (index, document) in documents.iter().enumerate().skip(1) {
        app.shell
            .tabs
            .get_mut(index - 1)
            .expect("inactive tab")
            .result_state
            .committed
            .preview_document = Some(Arc::clone(document));
    }
    let counted = app.shell.tabs.preview_resident_bytes(
        app.shell.runtime.preview_document.as_ref(),
        None,
        &[],
    );
    assert!(counted > 32 * 1024 * 1024);
    assert!(app.enforce_preview_payload_budget(None, false));
    let after = app.shell.tabs.preview_resident_bytes(
        app.shell.runtime.preview_document.as_ref(),
        None,
        &[],
    );
    assert!(after <= 32 * 1024 * 1024);
    assert!(app.shell.tabs.iter().any(|tab| tab.preview_reload_pending));
    // The same allocation held by two tabs is counted once.
    let evicted_index = app
        .shell
        .tabs
        .iter()
        .position(|tab| tab.preview_reload_pending)
        .expect("evicted tab");
    app.shell
        .tabs
        .get_mut(evicted_index)
        .expect("evicted tab")
        .result_state
        .committed
        .preview_document = Some(Arc::clone(&documents[0]));
    let shared = app.shell.tabs.preview_resident_bytes(
        app.shell.runtime.preview_document.as_ref(),
        None,
        &[],
    );
    assert_eq!(shared, after);
    fs::remove_dir_all(root).expect("cleanup root");
}

#[test]
fn worker_input_capacity_participates_in_preview_admission_and_quiesces() {
    const MIB: usize = 1024 * 1024;
    let root = test_root("paged-preview-accounted-worker-input");
    fs::create_dir_all(&root).expect("create root");
    let mut documents = Vec::new();
    for index in 0..4 {
        let path = root.join(format!("{index}.txt"));
        fs::write(&path, "line\n").expect("write fixture");
        let mut document = PagedTextPreview::initial(&path, &|| false).expect("document");
        document.reserve_body_capacity_for_test(7 * MIB);
        assert!(document.capacity_bytes() <= 8 * MIB);
        documents.push(Arc::new(document));
    }
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell
        .runtime
        .set_preview_document(Arc::clone(&documents[0]))
        .expect("active");
    let mut reclaimer = TabResourceReclaimer::paused_for_test();
    let handle = reclaimer.preview_handle();
    app.shell.runtime.install_preview_retirement(handle.clone());
    handle
        .try_retire_preview(Arc::clone(&documents[1]))
        .expect("retire");
    let (tx, rx) = std::sync::mpsc::channel::<PreviewRequest>();
    app.shell.worker_bus.preview.tx = tx;
    app.shell.worker_bus.preview.worker_inflight_request_id = None;
    let request = |id: u64, document: &Arc<PagedTextPreview>| PreviewRequest {
        request_id: id,
        path: document.header.path.clone(),
        is_dir: false,
        document: Some(Arc::clone(document)),
    };
    assert!(app
        .shell
        .worker_bus
        .preview
        .queue_request(request(81_001, &documents[2]))
        .unwrap_or_else(|_| panic!("send"))
        .is_none());
    let first = rx.try_recv().expect("first dispatched");
    assert_eq!(
        app.shell.worker_bus.preview.worker_input_document_bytes,
        documents[2].capacity_bytes()
    );
    let expected = documents[0].capacity_bytes()
        + documents[1].capacity_bytes()
        + documents[2].capacity_bytes()
        + 16 * MIB;
    assert_eq!(app.preview_accounted_payload_bytes(None, false), expected);
    assert!(app.enforce_preview_payload_budget(None, false));
    assert!(expected <= 96 * MIB);

    assert!(app
        .shell
        .worker_bus
        .preview
        .queue_request(request(81_002, &documents[3]))
        .unwrap_or_else(|_| panic!("queue latest"))
        .is_none());
    assert_eq!(
        app.shell.worker_bus.preview.worker_input_document_bytes,
        documents[2].capacity_bytes()
    );
    drop(first); // canceled/stale input can be released before terminal settlement.
    app.shell
        .worker_bus
        .preview
        .settle_response(81_001)
        .unwrap_or_else(|_| panic!("dispatch latest"));
    let second = rx.try_recv().expect("latest dispatched");
    assert_eq!(second.request_id, 81_002);
    assert_eq!(
        app.shell.worker_bus.preview.worker_input_document_bytes,
        documents[3].capacity_bytes()
    );
    assert_eq!(
        app.preview_accounted_payload_bytes(None, false),
        documents[0].capacity_bytes()
            + documents[1].capacity_bytes()
            + documents[3].capacity_bytes()
            + 16 * MIB
    );
    drop(second);
    app.shell
        .worker_bus
        .preview
        .settle_response(81_002)
        .unwrap_or_else(|_| panic!("settle canceled latest"));
    assert_eq!(app.shell.worker_bus.preview.worker_input_document_bytes, 0);
    assert!(reclaimer.drain_one_paused_for_test());
    assert_eq!(reclaimer.preview_retirement_bytes(), 0);
    assert_eq!(
        app.preview_accounted_payload_bytes(None, false),
        documents[0].capacity_bytes() + 16 * MIB
    );
    fs::remove_dir_all(root).expect("cleanup root");
}

#[test]
fn preview_retirement_backpressure_keeps_ui_owned_documents() {
    let root = test_root("paged-preview-retirement-backpressure");
    fs::create_dir_all(&root).expect("create root");
    let path = root.join("sample.txt");
    fs::write(&path, "sample\n").expect("write fixture");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let reclaimer = TabResourceReclaimer::paused_for_test();
    let handle = reclaimer.preview_handle();
    app.shell.runtime.install_preview_retirement(handle.clone());
    let seed = Arc::new(PagedTextPreview::initial(&path, &|| false).expect("seed"));
    for _ in 0..4 {
        handle
            .try_retire_preview(Arc::clone(&seed))
            .expect("fill reclaimer");
    }
    app.shell
        .runtime
        .set_preview_document(Arc::new(
            PagedTextPreview::initial(&path, &|| false).expect("active"),
        ))
        .expect("active document");
    app.shell.runtime.clear_preview();
    assert!(app.shell.runtime.preview_stale);
    assert!(app.shell.runtime.preview_document.is_some());
    assert!(app
        .shell
        .runtime
        .set_preview_document(Arc::new(
            PagedTextPreview::initial(&path, &|| false).expect("next")
        ))
        .is_err());
    assert!(app.shell.runtime.preview_document.is_some());

    let (tx, rx) = std::sync::mpsc::channel::<PreviewRequest>();
    app.shell.worker_bus.preview.tx = tx;
    app.shell.worker_bus.preview.worker_inflight_request_id = None;
    let first = PreviewRequest {
        request_id: 80_001,
        path: path.clone(),
        is_dir: false,
        document: None,
    };
    let pending = PreviewRequest {
        request_id: 80_002,
        path: path.clone(),
        is_dir: false,
        document: Some(Arc::new(
            PagedTextPreview::initial(&path, &|| false).expect("pending"),
        )),
    };
    let latest = PreviewRequest {
        request_id: 80_003,
        path: path.clone(),
        is_dir: false,
        document: None,
    };
    assert!(app.queue_preview_request(first));
    assert!(app.queue_preview_request(pending));
    assert!(app.queue_preview_request(latest));
    assert!(app.parked_preview_request.is_some());
    assert_eq!(
        app.shell
            .worker_bus
            .preview
            .latest_request
            .as_ref()
            .map(|request| request.request_id),
        Some(80_003)
    );
    assert_eq!(rx.try_recv().expect("inflight").request_id, 80_001);
    fs::remove_dir_all(root).expect("cleanup root");
}

#[test]
fn active_more_response_after_tab_roundtrip_keeps_and_extends_prior_body() {
    let root = test_root("paged-preview-tab-roundtrip");
    fs::create_dir_all(&root).expect("create root");
    let path = root.join("sample.txt");
    fs::write(&path, "safe\n".repeat(101)).expect("write fixture");
    let initial = Arc::new(PagedTextPreview::initial(&path, &|| false).expect("initial"));
    let next = Arc::new(initial.read_more(&path, &|| false).expect("more"));
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.ui.show_preview = true;
    app.shell.runtime.committed_for_test_mut().results = vec![(path.clone(), 0.0)];
    app.shell.runtime.committed_for_test_mut().current_row = Some(0);
    app.shell
        .runtime
        .set_preview_document(initial)
        .expect("active document");
    app.create_new_tab();
    app.switch_to_tab_index(0);
    let (tx, _rx) = std::sync::mpsc::channel::<PreviewRequest>();
    app.shell.worker_bus.preview.tx = tx;
    app.shell.worker_bus.preview.worker_inflight_request_id = None;
    app.request_paged_preview_more();
    let request_id = app
        .shell
        .worker_bus
        .preview
        .pending_request_id
        .expect("request");
    app.switch_to_tab_index(1);
    app.switch_to_tab_index(0);
    assert_eq!(
        app.shell.worker_bus.preview.pending_request_id,
        Some(request_id)
    );
    assert!(app.apply_active_preview_response(&PreviewResponse {
        request_id,
        path: path.clone(),
        preview: String::new(),
        document: Some(next),
        page_error: None,
        canceled: false,
        is_more: true,
    }));
    assert_eq!(
        app.paged_preview_for_current()
            .expect("extended document")
            .line_count(),
        101
    );
    fs::remove_dir_all(root).expect("cleanup root");
}

#[test]
fn repeated_more_across_tabs_while_reclaimer_is_full_keeps_one_parked_document() {
    let root = test_root("paged-preview-bounded-parking");
    fs::create_dir_all(&root).expect("create root");
    let paths = [root.join("a.txt"), root.join("b.txt")];
    for path in &paths {
        fs::write(path, "line\n".repeat(101)).expect("fixture");
    }
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.ui.show_preview = true;
    app.shell.runtime.committed_for_test_mut().results = vec![(paths[0].clone(), 0.0)];
    app.shell.runtime.committed_for_test_mut().current_row = Some(0);
    app.shell
        .runtime
        .set_preview_document(Arc::new(
            PagedTextPreview::initial(&paths[0], &|| false).expect("A"),
        ))
        .expect("A document");
    app.create_new_tab();
    app.shell.runtime.committed_for_test_mut().results = vec![(paths[1].clone(), 0.0)];
    app.shell.runtime.committed_for_test_mut().current_row = Some(0);
    app.shell
        .runtime
        .set_preview_document(Arc::new(
            PagedTextPreview::initial(&paths[1], &|| false).expect("B"),
        ))
        .expect("B document");
    let mut reclaimer = TabResourceReclaimer::paused_for_test();
    let handle = reclaimer.preview_handle();
    app.shell.runtime.install_preview_retirement(handle.clone());
    let seed = Arc::new(PagedTextPreview::initial(&paths[0], &|| false).expect("seed"));
    for _ in 0..4 {
        handle.try_retire_preview(Arc::clone(&seed)).expect("fill");
    }
    app.parked_preview_request = Some(PreviewRequest {
        request_id: 99_001,
        path: paths[0].clone(),
        is_dir: false,
        document: Some(Arc::new(
            PagedTextPreview::initial(&paths[0], &|| false).expect("parked"),
        )),
    });
    for index in [1, 0, 1, 0] {
        app.switch_to_tab_index(index);
        for _ in 0..10 {
            app.request_paged_preview_more();
        }
        assert!(app.parked_preview_request.is_some());
        assert!(app
            .deferred_latest_preview_request
            .as_ref()
            .is_none_or(|request| request.document.is_none()));
        assert!(app
            .parked_preview_request
            .as_ref()
            .unwrap()
            .document
            .is_some());
    }
    assert!(reclaimer.drain_one_paused_for_test());
    app.poll_preview_response();
    assert!(
        app.parked_preview_request.is_none(),
        "parked request retires after capacity returns"
    );
    while reclaimer.drain_one_paused_for_test() {}
    app.poll_preview_response();
    assert_eq!(reclaimer.preview_retirement_bytes(), 0);
    assert!(
        app.deferred_more_intent.is_none(),
        "latest retry intent settles"
    );
    fs::remove_dir_all(root).expect("cleanup root");
}

#[test]
fn gui_preview_worker_disconnect_settles_latest_and_future_requests() {
    let root = test_root("paged-preview-worker-disconnect");
    fs::create_dir_all(&root).expect("create root");
    let paths = [root.join("first.txt"), root.join("latest.txt")];
    for path in &paths {
        fs::write(path, "content\n").expect("write fixture");
    }
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.ui.show_preview = true;
    app.shell.runtime.committed_for_test_mut().results =
        paths.iter().cloned().map(|path| (path, 0.0)).collect();
    for path in &paths {
        app.set_entry_kind(path, EntryKind::file());
    }
    let (request_tx, request_rx) = std::sync::mpsc::channel::<PreviewRequest>();
    let (response_tx, response_rx) = std::sync::mpsc::channel::<PreviewResponse>();
    app.shell.worker_bus.preview.tx = request_tx;
    app.shell.worker_bus.preview.rx = response_rx;
    app.shell.runtime.committed_for_test_mut().current_row = Some(0);
    app.request_preview_for_current();
    let first = request_rx.try_recv().expect("first request dispatched");
    app.shell.runtime.committed_for_test_mut().current_row = Some(1);
    app.request_preview_for_current();
    let latest = app
        .shell
        .worker_bus
        .preview
        .latest_request
        .as_ref()
        .expect("latest request queued")
        .request_id;
    assert!(app.paged_preview_view.busy);

    drop(response_tx);
    app.poll_preview_response();
    assert!(!app.shell.worker_bus.preview.in_progress);
    assert_eq!(
        app.shell.worker_bus.preview.worker_inflight_request_id,
        None
    );
    assert!(app.shell.worker_bus.preview.latest_request.is_none());
    assert!(!app.paged_preview_view.busy);
    assert_eq!(app.preview_request_tab(first.request_id), None);
    assert_eq!(app.preview_request_tab(latest), None);
    assert!(app
        .shell
        .runtime
        .notice
        .contains("Preview worker is unavailable"));

    drop(request_rx);
    app.request_preview_for_current();
    assert!(!app.shell.worker_bus.preview.in_progress);
    assert!(!app.paged_preview_view.busy);
    fs::remove_dir_all(root).expect("cleanup root");
}

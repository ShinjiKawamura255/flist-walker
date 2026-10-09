use super::*;
use crate::app::paged_preview_flow::PreviewAction;
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
fn gui_initial_body_errors_keep_file_information_and_reload_updates_it() {
    let root = test_root("preview-file-information");
    fs::create_dir_all(&root).expect("create root");
    let path = root.join("sample.bin");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.ui.show_preview = true;
    app.shell.runtime.committed_for_test_mut().results = vec![(path.clone(), 0.0)];
    app.shell.runtime.committed_for_test_mut().current_row = Some(0);
    app.set_entry_kind(&path, EntryKind::file());
    for (body, size, reason) in [
        (&b"\0binary"[..], "7 B", PreviewPageError::Binary),
        (&b""[..], "0 B", PreviewPageError::Empty),
    ] {
        fs::write(&path, body).expect("write fixture");
        if app.initial_preview_reload_available() {
            app.apply_preview_action(PreviewAction::Reload);
        } else {
            app.reload_paged_preview();
        }
        settle_preview(&mut app);
        assert!(app.initial_preview_reload_available());
        assert_eq!(app.paged_preview_view.error, Some(reason));
        let preview = &app.shell.runtime.preview;
        assert!(
            preview.contains(&format!("File: {}", path.display())),
            "{preview}"
        );
        assert!(preview.contains(&format!("Size: {size}")), "{preview}");
        assert!(preview.contains("Created:"), "{preview}");
        assert!(preview.contains("Updated:"), "{preview}");
        assert!(
            preview.contains(super::super::paged_preview_flow::page_error_label(reason)),
            "{preview}"
        );
    }
    fs::write(&path, "text after reload\n").expect("replace fixture");
    app.reload_paged_preview();
    settle_preview(&mut app);
    assert_eq!(
        app.paged_preview_for_current()
            .expect("text document")
            .body(),
        "text after reload\n"
    );
    fs::remove_file(&path).expect("delete fixture");
    app.reload_paged_preview();
    settle_preview(&mut app);
    assert_eq!(
        app.paged_preview_view.error,
        Some(PreviewPageError::NotFound)
    );
    let preview = &app.shell.runtime.preview;
    assert!(preview.contains("Size: <unavailable>"), "{preview}");
    assert!(preview.contains("Created: <unavailable>"), "{preview}");
    assert!(preview.contains("Updated: <unavailable>"), "{preview}");
    assert!(!preview.contains("text after reload"));
    fs::remove_dir_all(root).expect("cleanup root");
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
fn gui_paged_preview_body_uses_the_cjk_primary_text_style() {
    let root = test_root("paged-preview-cjk-text-style");
    fs::create_dir_all(&root).expect("create root");
    let path = root.join("sample.rs");
    fs::write(&path, "fn main() { let value = \"English 日本語\"; }\n").expect("write fixture");
    let document = PagedTextPreview::initial(&path, &|| false).expect("preview");
    let ctx = egui::Context::default();
    let mut body_font = None;
    let mut plain_fonts = None;
    let mut colored_fonts = None;
    let mut colored_section_count = None;
    let mut body_row_height = None;
    let mut preview_row_height = None;
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.style_mut()
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(18.0));
        ui.style_mut()
            .text_styles
            .insert(egui::TextStyle::Monospace, egui::FontId::monospace(12.0));
        body_font = Some(egui::TextStyle::Body.resolve(ui.style()));
        plain_fonts = Some(
            crate::app::render_panels::preview_line_job(&document, 0, false, ui)
                .sections
                .iter()
                .map(|section| section.format.font_id.clone())
                .collect::<Vec<_>>(),
        );
        let colored = crate::app::render_panels::preview_line_job(&document, 0, true, ui);
        colored_section_count = Some(colored.sections.len());
        colored_fonts = Some(
            colored
                .sections
                .iter()
                .map(|section| section.format.font_id.clone())
                .collect::<Vec<_>>(),
        );
        body_row_height = Some(ui.text_style_height(&egui::TextStyle::Body) + 4.0);
        preview_row_height = Some(crate::app::render_panels::preview_paged_row_height(ui));
    });
    let body_font = body_font.expect("body font");
    for (label, fonts) in [
        ("plain", plain_fonts.expect("plain fonts")),
        ("colored", colored_fonts.expect("colored fonts")),
    ] {
        assert!(
            !fonts.is_empty() && fonts.iter().all(|font| *font == body_font),
            "{label} mixed Japanese/Latin preview text must share the CJK-primary font metrics"
        );
    }
    assert!(
        colored_section_count.expect("colored section count") > 1,
        "syntax-highlighted preview must exercise multiple text sections"
    );
    assert_eq!(
        preview_row_height, body_row_height,
        "virtualized preview row height must use the same text style as its body"
    );
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
    let (preview_request_tx, preview_request_rx) = std::sync::mpsc::channel::<PreviewRequest>();
    let (preview_response_tx, preview_response_rx) = std::sync::mpsc::channel::<PreviewResponse>();
    app.shell.worker_bus.preview.tx = preview_request_tx;
    app.shell.worker_bus.preview.rx = preview_response_rx;
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
    let more_request = preview_request_rx
        .try_recv()
        .expect("deferred More intent reaches the controlled worker");
    assert_eq!(more_request.path, paths[0]);
    let request_document = more_request
        .document
        .as_ref()
        .expect("More request retains its active document");
    let expected_retirement_bytes = request_document.capacity_bytes();
    let more_document = Arc::new(
        request_document
            .read_more(&more_request.path, &|| false)
            .expect("build deterministic More response"),
    );
    preview_response_tx
        .send(PreviewResponse {
            request_id: more_request.request_id,
            path: more_request.path.clone(),
            preview: String::new(),
            document: Some(more_document),
            page_error: None,
            canceled: false,
            is_more: true,
        })
        .expect("controlled worker response");
    drop(more_request);
    while reclaimer.drain_one_paused_for_test() {}
    app.poll_preview_response();
    assert_eq!(
        reclaimer.preview_retirement_bytes(),
        expected_retirement_bytes,
        "the settled More response queues the prior active document for reclamation"
    );
    while reclaimer.drain_one_paused_for_test() {}
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

fn preview_focus_fixture(name: &str) -> (FlistWalkerApp, egui::Context, PathBuf) {
    let root = test_root(name);
    fs::create_dir_all(&root).expect("create root");
    let path = root.join("sample.rs");
    fs::write(&path, "fn main() {}\n".repeat(700)).expect("fixture");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.ui.show_preview = true;
    app.shell.runtime.committed_for_test_mut().results = vec![(path.clone(), 0.0)];
    app.shell.runtime.committed_for_test_mut().current_row = Some(0);
    app.set_entry_kind(&path, EntryKind::file());
    app.request_preview_for_current();
    settle_preview(&mut app);
    let ctx = egui::Context::default();
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| app.run_ui_frame(ui));
    ctx.memory_mut(|memory| memory.request_focus(app.shell.ui.query_input_id));
    (app, ctx, root)
}

fn preview_key_frame(
    app: &mut FlistWalkerApp,
    ctx: &egui::Context,
    key: egui::Key,
    modifiers: egui::Modifiers,
    repeat: bool,
) {
    let _ = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 900.0),
            )),
            modifiers,
            events: if repeat {
                vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat,
                    modifiers,
                }]
            } else {
                vec![
                    egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: false,
                        repeat: false,
                        modifiers,
                    },
                    egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers,
                    },
                ]
            },
            ..Default::default()
        },
        |ui| app.run_ui_frame(ui),
    );
}

#[test]
fn regression_preview_focus_full_frames_toggle_color_and_return_preserve_selection() {
    let (mut app, ctx, root) = preview_focus_fixture("preview-focus-controls");
    let path = app.shell.runtime.results[0].0.clone();
    app.toggle_pin_current_from_tab();
    let before_pins = app.shell.runtime.pinned_paths.clone();
    app.shell.runtime.query_state.query = "sample".to_owned();
    let before_query = app.shell.runtime.query_state.query.clone();
    preview_key_frame(
        &mut app,
        &ctx,
        egui::Key::L,
        gui_shortcut_modifiers(true),
        false,
    );
    assert!(
        !ctx.memory(|memory| memory.has_focus(app.shell.ui.query_input_id)),
        "preview chord must leave focused query"
    );
    preview_key_frame(
        &mut app,
        &ctx,
        egui::Key::Space,
        egui::Modifiers::NONE,
        false,
    );
    assert!(
        !app.paged_preview_view.color_enabled,
        "Space must activate Color without text editing"
    );
    preview_key_frame(
        &mut app,
        &ctx,
        egui::Key::Space,
        egui::Modifiers::NONE,
        true,
    );
    assert!(
        !app.paged_preview_view.color_enabled,
        "repeat must not toggle color"
    );
    preview_key_frame(
        &mut app,
        &ctx,
        egui::Key::Escape,
        egui::Modifiers::NONE,
        false,
    );
    assert!(ctx.memory(|memory| memory.has_focus(app.shell.ui.query_input_id)));
    assert_eq!(app.shell.runtime.query_state.query, before_query);
    assert_eq!(app.shell.runtime.pinned_paths, before_pins);
    assert_eq!(
        app.shell.runtime.results[app.shell.runtime.current_row.unwrap()].0,
        path
    );
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn regression_preview_focus_full_frames_more_busy_emacs_and_pin() {
    let (mut app, ctx, root) = preview_focus_fixture("preview-focus-more");
    preview_key_frame(
        &mut app,
        &ctx,
        egui::Key::L,
        gui_shortcut_modifiers(true),
        false,
    );
    preview_key_frame(
        &mut app,
        &ctx,
        egui::Key::ArrowRight,
        egui::Modifiers::NONE,
        false,
    );
    preview_key_frame(
        &mut app,
        &ctx,
        egui::Key::Enter,
        egui::Modifiers::NONE,
        false,
    );
    assert!(app.paged_preview_view.busy);
    let request_id = app.shell.worker_bus.preview.pending_request_id;
    preview_key_frame(
        &mut app,
        &ctx,
        egui::Key::Enter,
        egui::Modifiers::NONE,
        true,
    );
    assert_eq!(app.shell.worker_bus.preview.pending_request_id, request_id);
    settle_preview(&mut app);
    assert_eq!(app.paged_preview_for_current().unwrap().line_count(), 600);
    preview_key_frame(
        &mut app,
        &ctx,
        egui::Key::J,
        emacs_shortcut_modifiers(false),
        false,
    );
    settle_preview(&mut app);
    assert_eq!(
        app.paged_preview_for_current().unwrap().state(),
        PreviewPageState::Eof
    );
    preview_key_frame(
        &mut app,
        &ctx,
        egui::Key::M,
        emacs_shortcut_modifiers(false),
        false,
    );
    assert!(
        !app.paged_preview_view.busy,
        "EOF disables More for every accept key"
    );
    preview_key_frame(&mut app, &ctx, egui::Key::Tab, egui::Modifiers::NONE, false);
    assert_eq!(app.shell.runtime.pinned_paths.len(), 1);
    preview_key_frame(
        &mut app,
        &ctx,
        egui::Key::Tab,
        egui::Modifiers::SHIFT,
        false,
    );
    assert!(app.shell.runtime.pinned_paths.is_empty());
    preview_key_frame(
        &mut app,
        &ctx,
        egui::Key::ArrowRight,
        egui::Modifiers::NONE,
        false,
    );
    preview_key_frame(
        &mut app,
        &ctx,
        egui::Key::Space,
        egui::Modifiers::NONE,
        false,
    );
    assert!(app.paged_preview_view.busy, "Reload shares preview command");
    settle_preview(&mut app);
    assert_eq!(app.paged_preview_for_current().unwrap().line_count(), 100);
    preview_key_frame(
        &mut app,
        &ctx,
        egui::Key::L,
        gui_shortcut_modifiers(false),
        false,
    );
    assert!(ctx.memory(|memory| memory.has_focus(app.shell.ui.query_input_id)));
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn regression_preview_focus_full_frames_blocks_history_modal_ime_and_missing_document() {
    for block in ["history", "modal", "ime", "missing", "hidden"] {
        let (mut app, ctx, root) = preview_focus_fixture(&format!("preview-focus-block-{block}"));
        match block {
            "history" => app.start_history_search(),
            "modal" => app.shell.ui.help_open = true,
            "ime" => app.shell.ui.ime_composition_active = true,
            "missing" => app.shell.runtime.clear_preview(),
            "hidden" => app.shell.ui.show_preview = false,
            _ => unreachable!(),
        }
        preview_key_frame(
            &mut app,
            &ctx,
            egui::Key::L,
            gui_shortcut_modifiers(true),
            false,
        );
        preview_key_frame(
            &mut app,
            &ctx,
            egui::Key::Space,
            egui::Modifiers::NONE,
            false,
        );
        assert!(
            app.paged_preview_view.color_enabled,
            "{block} must block background preview actions"
        );
        fs::remove_dir_all(root).expect("cleanup");
    }
}

#[test]
fn regression_preview_focus_full_frames_scroll_toggle_repeat_and_disabled_accept() {
    let (mut app, ctx, root) = preview_focus_fixture("preview-focus-scroll-repeat");
    preview_key_frame(
        &mut app,
        &ctx,
        egui::Key::L,
        gui_shortcut_modifiers(true),
        false,
    );
    assert!(app.paged_preview_view.controls_focused);
    preview_key_frame(
        &mut app,
        &ctx,
        egui::Key::L,
        gui_shortcut_modifiers(true),
        true,
    );
    assert!(
        app.paged_preview_view.controls_focused,
        "held focus chord must not toggle back"
    );
    let before_row = app.shell.runtime.current_row;
    let before_scroll = app.paged_preview_view.scroll_offset;
    preview_key_frame(
        &mut app,
        &ctx,
        egui::Key::PageDown,
        egui::Modifiers::NONE,
        false,
    );
    assert!(
        app.paged_preview_view.scroll_offset > before_scroll,
        "PageDown must scroll preview body"
    );
    assert_eq!(app.shell.runtime.current_row, before_row);
    preview_key_frame(
        &mut app,
        &ctx,
        egui::Key::PageUp,
        egui::Modifiers::NONE,
        false,
    );
    assert_eq!(app.paged_preview_view.scroll_offset, before_scroll);
    app.shell.runtime.emacs_keybindings_enabled = false;
    preview_key_frame(
        &mut app,
        &ctx,
        egui::Key::J,
        emacs_shortcut_modifiers(false),
        false,
    );
    assert!(
        app.paged_preview_view.color_enabled,
        "disabled Emacs accept must not activate Color"
    );
    preview_key_frame(
        &mut app,
        &ctx,
        egui::Key::M,
        emacs_shortcut_modifiers(false),
        false,
    );
    assert!(app.paged_preview_view.color_enabled);
    app.shell.ui.help_open = true;
    preview_key_frame(
        &mut app,
        &ctx,
        egui::Key::Space,
        egui::Modifiers::NONE,
        false,
    );
    assert!(
        app.paged_preview_view.color_enabled,
        "modal blocks already-focused preview"
    );
    app.shell.ui.help_open = false;
    app.shell.ui.ime_composition_active = true;
    preview_key_frame(
        &mut app,
        &ctx,
        egui::Key::Space,
        egui::Modifiers::NONE,
        false,
    );
    assert!(
        app.paged_preview_view.color_enabled,
        "IME blocks already-focused preview"
    );
    app.shell.ui.ime_composition_active = false;
    preview_key_frame(
        &mut app,
        &ctx,
        egui::Key::L,
        gui_shortcut_modifiers(true),
        false,
    );
    assert!(!app.paged_preview_view.controls_focused);
    assert!(ctx.memory(|memory| memory.has_focus(app.shell.ui.query_input_id)));
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn preview_control_availability_is_shared_for_busy_eof_error_and_plain_text() {
    use crate::app::paged_preview_flow::PreviewAction;
    let root = test_root("preview-control-availability");
    fs::create_dir_all(&root).expect("root");
    let path = root.join("plain.txt");
    fs::write(&path, "line\n".repeat(200)).expect("fixture");
    let document = PagedTextPreview::initial(&path, &|| false).expect("preview");
    assert!(!PreviewAction::ToggleColor.available(&document, false, None));
    assert!(PreviewAction::More.available(&document, false, None));
    assert!(!PreviewAction::More.available(&document, true, None));
    assert!(!PreviewAction::Reload.available(&document, true, None));
    assert!(!PreviewAction::More.available(&document, false, Some(PreviewPageError::Changed)));
    assert!(PreviewAction::Reload.available(&document, false, Some(PreviewPageError::Changed)));
    let completed = document.read_more(&path, &|| false).expect("append");
    assert!(!PreviewAction::More.available(&completed, false, None));
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn regression_preview_focus_normal_render_shows_focus_and_pointer_uses_same_command() {
    use crate::app::paged_preview_flow::PreviewAction;
    use crate::app::render_panels::{begin_preview_control_probe, take_preview_control_probe};
    let (mut app, ctx, root) = preview_focus_fixture("preview-focus-render-pointer");
    begin_preview_control_probe();
    preview_key_frame(
        &mut app,
        &ctx,
        egui::Key::L,
        gui_shortcut_modifiers(true),
        false,
    );
    let controls = take_preview_control_probe();
    assert_eq!(
        controls.iter().filter(|control| control.selected).count(),
        1
    );
    let color = controls
        .iter()
        .find(|control| control.action == PreviewAction::ToggleColor)
        .unwrap();
    assert!(
        color.enabled && color.selected,
        "normal render must highlight active enabled Color button"
    );
    let position = color.rect.center();
    for pressed in [true, false] {
        let _ = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 900.0),
                )),
                events: vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |ui| app.run_ui_frame(ui),
        );
    }
    assert!(
        !app.paged_preview_view.color_enabled,
        "pointer invokes shared Color command"
    );
    preview_key_frame(
        &mut app,
        &ctx,
        egui::Key::Enter,
        egui::Modifiers::NONE,
        false,
    );
    assert!(
        app.paged_preview_view.color_enabled,
        "keyboard invokes same Color command"
    );
    app.paged_preview_view.busy = true;
    begin_preview_control_probe();
    preview_key_frame(
        &mut app,
        &ctx,
        egui::Key::ArrowRight,
        egui::Modifiers::NONE,
        false,
    );
    let controls = take_preview_control_probe();
    let more = controls
        .iter()
        .find(|control| control.action == PreviewAction::More)
        .unwrap();
    assert!(
        !more.enabled && more.selected,
        "disabled More retains a visible selected control"
    );
    fs::remove_dir_all(root).expect("cleanup");
}

fn assert_preview_exit_hold_isolated(name: &str, key: egui::Key, modifiers: egui::Modifiers) {
    let (mut app, ctx, root) = preview_focus_fixture(name);
    app.toggle_pin_current_from_tab();
    app.shell.runtime.query_state.query = "sample".to_owned();
    let pins = app.shell.runtime.pinned_paths.clone();
    preview_key_frame(
        &mut app,
        &ctx,
        egui::Key::L,
        gui_shortcut_modifiers(true),
        false,
    );
    preview_key_frame(&mut app, &ctx, key, modifiers, false);
    assert!(!app.paged_preview_view.controls_focused);
    assert!(ctx.memory(|memory| memory.has_focus(app.shell.ui.query_input_id)));
    for _ in 0..2 {
        preview_key_frame(&mut app, &ctx, key, modifiers, true);
        assert_eq!(
            app.shell.runtime.query_state.query, "sample",
            "held preview exit must not clear query in subsequent normal frames"
        );
        assert_eq!(
            app.shell.runtime.pinned_paths, pins,
            "held preview exit must not clear pins"
        );
        assert!(
            ctx.memory(|memory| memory.has_focus(app.shell.ui.query_input_id)),
            "held Primary+L must not toggle query focus again"
        );
    }
    let _ = ctx.run_ui(
        egui::RawInput {
            events: vec![egui::Event::Text("x".to_owned())],
            ..Default::default()
        },
        |ui| app.run_ui_frame(ui),
    );
    assert_eq!(
        app.shell.runtime.query_state.query, "samplex",
        "suppressing the held exit must not freeze other query input"
    );
    let _ = ctx.run_ui(
        egui::RawInput {
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        },
        |ui| app.run_ui_frame(ui),
    );
    preview_key_frame(&mut app, &ctx, key, modifiers, false);
    if key == egui::Key::L {
        assert!(
            !ctx.memory(|memory| memory.has_focus(app.shell.ui.query_input_id)),
            "fresh normal Primary+L retains its focus toggle"
        );
        assert_eq!(app.shell.runtime.query_state.query, "samplex");
        assert_eq!(app.shell.runtime.pinned_paths, pins);
    } else {
        assert!(
            app.shell.runtime.query_state.query.is_empty(),
            "fresh normal cancel may clear query"
        );
        assert!(
            app.shell.runtime.pinned_paths.is_empty(),
            "fresh normal cancel may clear pins"
        );
    }
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn regression_preview_focus_exit_escape_repeats_do_not_clear_search() {
    assert_preview_exit_hold_isolated(
        "preview-exit-held-escape",
        egui::Key::Escape,
        egui::Modifiers::NONE,
    );
}

#[test]
fn regression_preview_focus_exit_ctrl_g_repeats_do_not_clear_search() {
    assert_preview_exit_hold_isolated(
        "preview-exit-held-ctrl-g",
        egui::Key::G,
        emacs_shortcut_modifiers(false),
    );
}

#[test]
fn regression_preview_focus_exit_primary_l_repeats_do_not_toggle_search() {
    assert_preview_exit_hold_isolated(
        "preview-exit-held-primary-l",
        egui::Key::L,
        gui_shortcut_modifiers(false),
    );
}

#[test]
fn regression_preview_focus_exit_requires_fresh_press_and_emacs_enabled() {
    for (key, modifiers) in [
        (egui::Key::Escape, egui::Modifiers::NONE),
        (egui::Key::G, emacs_shortcut_modifiers(false)),
        (egui::Key::L, gui_shortcut_modifiers(false)),
    ] {
        let (mut app, ctx, root) = preview_focus_fixture(&format!("preview-exit-fresh-{key:?}"));
        preview_key_frame(
            &mut app,
            &ctx,
            egui::Key::L,
            gui_shortcut_modifiers(true),
            false,
        );
        // The press began in a modal; only its repeat reaches the preview owner.
        app.shell.ui.help_open = true;
        preview_key_frame(&mut app, &ctx, key, modifiers, false);
        app.shell.ui.help_open = false;
        preview_key_frame(&mut app, &ctx, key, modifiers, true);
        assert!(
            app.paged_preview_view.controls_focused,
            "an exit key repeat must not end preview focus: {key:?}"
        );
        if key == egui::Key::G {
            app.shell.runtime.emacs_keybindings_enabled = false;
            preview_key_frame(&mut app, &ctx, key, modifiers, false);
            assert!(
                app.paged_preview_view.controls_focused,
                "disabled Ctrl+G must not exit preview"
            );
        }
        fs::remove_dir_all(root).expect("cleanup");
    }
}

#[test]
fn regression_preview_focus_exit_held_escape_does_not_refocus_behind_modal() {
    let (mut app, ctx, root) = preview_focus_fixture("preview-exit-held-modal-focus");
    app.toggle_pin_current_from_tab();
    app.shell.runtime.query_state.query = "sample".to_owned();
    let pins = app.shell.runtime.pinned_paths.clone();
    preview_key_frame(
        &mut app,
        &ctx,
        egui::Key::L,
        gui_shortcut_modifiers(true),
        false,
    );
    preview_key_frame(
        &mut app,
        &ctx,
        egui::Key::Escape,
        egui::Modifiers::NONE,
        false,
    );
    app.shell.ui.help_open = true;
    preview_key_frame(
        &mut app,
        &ctx,
        egui::Key::Escape,
        egui::Modifiers::NONE,
        true,
    );
    assert!(
        app.shell.ui.help_open,
        "held preview exit must not cancel the new modal"
    );
    assert!(
        !ctx.memory(|memory| memory.has_focus(app.shell.ui.query_input_id)),
        "held-key focus repair must not refocus the query behind a modal"
    );
    assert_eq!(app.shell.runtime.query_state.query, "sample");
    assert_eq!(app.shell.runtime.pinned_paths, pins);
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn regression_short_preview_scroll_reaches_all_controls() {
    use crate::app::render_panels::{begin_preview_control_probe, take_preview_control_probe};
    for (size, width) in [
        (egui::vec2(640.0, 400.0), 440.0),
        (egui::vec2(760.0, 560.0), 220.0),
    ] {
        let (mut app, ctx, root) =
            preview_focus_fixture(&format!("short-preview-{}", "long-".repeat(15)));
        app.shell.ui.set_preview_panel_width(width);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        let pointer = egui::pos2(size.x - 80.0, size.y - 90.0);
        let mut controls = Vec::new();
        for frame in 0..12 {
            begin_preview_control_probe();
            let events = if frame < 3 {
                Vec::new()
            } else {
                vec![
                    egui::Event::PointerMoved(pointer),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        phase: egui::TouchPhase::Move,
                        delta: egui::vec2(0.0, -600.0),
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            };
            let _ = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    time: Some(frame as f64 * 0.1),
                    events,
                    ..Default::default()
                },
                |ui| app.run_ui_frame(ui),
            );
            controls = take_preview_control_probe();
        }
        assert_eq!(controls.len(), 3);
        for control in controls {
            assert!(screen.contains_rect(control.rect), "short/narrow Preview controls must be reachable by scrolling: size={size:?}, width={width}, action={:?}, rect={:?}", control.action, control.rect);
        }
        fs::remove_dir_all(root).expect("cleanup");
    }
}

#[test]
fn regression_short_preview_keyboard_focus_reveals_selected_control() {
    use crate::app::render_panels::{begin_preview_control_probe, take_preview_control_probe};
    let (mut app, ctx, root) = preview_focus_fixture("short-preview-keyboard-control");
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 400.0));
    let mut controls = Vec::new();
    for frame in 0..8 {
        begin_preview_control_probe();
        let events = if frame == 2 {
            vec![egui::Event::Key {
                key: egui::Key::L,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: gui_shortcut_modifiers(true),
            }]
        } else {
            Vec::new()
        };
        let _ = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                time: Some(frame as f64 * 0.1),
                events,
                ..Default::default()
            },
            |ui| app.run_ui_frame(ui),
        );
        controls = take_preview_control_probe();
    }
    let selected = controls
        .iter()
        .find(|control| control.selected)
        .expect("preview focused control");
    assert!(
        screen.contains_rect(selected.rect),
        "application keyboard route must reveal selected Preview control: {:?}",
        selected.rect
    );
    fs::remove_dir_all(root).expect("cleanup");
}

#[cfg(unix)]
#[test]
fn gui_body_permission_error_keeps_metadata_and_broken_link_keeps_target() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let root = test_root("preview-permissions-links");
    fs::create_dir_all(&root).expect("create root");
    let path = root.join("read-denied.bin");
    fs::write(&path, b"\0binary").expect("write fixture");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.ui.show_preview = true;
    app.shell.runtime.committed_for_test_mut().results = vec![(path.clone(), 0.0)];
    app.shell.runtime.committed_for_test_mut().current_row = Some(0);
    app.set_entry_kind(&path, EntryKind::file());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o0)).expect("deny body read");
    app.request_preview_for_current();
    settle_preview(&mut app);
    let error = app.paged_preview_view.error;
    let preview = app.shell.runtime.preview.clone();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("restore permission");
    assert_eq!(error, Some(PreviewPageError::PermissionDenied));
    assert!(preview.contains("Size: 7 B"), "{preview}");
    assert!(preview.contains("Updated:"), "{preview}");
    assert!(preview.contains("Attributes: Read-only"), "{preview}");

    let link = root.join("link.bin");
    symlink("read-denied.bin", &link).expect("create link");
    app.shell.runtime.committed_for_test_mut().results = vec![(link.clone(), 0.0)];
    app.set_entry_kind(&link, EntryKind::link(false));
    app.request_preview_for_current();
    settle_preview(&mut app);
    let preview = &app.shell.runtime.preview;
    assert!(preview.contains("Target Size: 7 B"), "{preview}");
    assert!(preview.contains("Target: read-denied.bin"), "{preview}");
    fs::remove_file(&path).expect("break link");
    app.set_entry_kind(&link, EntryKind::link_unknown());
    app.apply_preview_action(PreviewAction::Reload);
    settle_preview(&mut app);
    assert_eq!(
        app.paged_preview_view.error,
        Some(PreviewPageError::NotFound)
    );
    let preview = &app.shell.runtime.preview;
    assert!(preview.contains("Target Size: <unavailable>"), "{preview}");
    assert!(
        preview.contains("Target Created: <unavailable>"),
        "{preview}"
    );
    assert!(preview.contains("Target: read-denied.bin"), "{preview}");
    fs::remove_dir_all(root).expect("cleanup root");
}

#[test]
fn initial_error_header_is_request_path_and_tab_scoped() {
    let root = test_root("preview-header-ownership");
    fs::create_dir_all(&root).expect("create root");
    let path = root.join("a.bin");
    fs::write(&path, b"\0").expect("write fixture");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.ui.show_preview = true;
    app.shell.runtime.committed_for_test_mut().results = vec![(path.clone(), 0.0)];
    app.shell.runtime.committed_for_test_mut().current_row = Some(0);
    app.shell.runtime.set_preview("current".into());
    app.prepare_paged_preview_initial(&path, 9002);
    let mut response = PreviewResponse {
        request_id: 9001,
        path: path.clone(),
        preview: "File: stale\nSize: 1 B".into(),
        document: None,
        page_error: Some(PreviewPageError::Binary),
        canceled: false,
        is_more: false,
    };
    app.apply_paged_preview_response(&response);
    assert_eq!(app.shell.runtime.preview, "current");
    response.request_id = 9002;
    response.path = root.join("wrong.bin");
    app.apply_paged_preview_response(&response);
    assert_eq!(app.shell.runtime.preview, "current");

    let first_tab = app.current_tab_id().expect("first tab");
    app.create_new_tab();
    let active_preview = app.shell.runtime.preview.clone();
    let background_before = app
        .shell
        .tabs
        .get(0)
        .expect("background tab")
        .result_state
        .committed
        .preview
        .clone();
    for request_id in [9003, 9004] {
        let tab = app.shell.tabs.get_mut(0).expect("background tab");
        tab.pending_preview_request_id = Some(request_id);
        tab.preview_in_progress = true;
        app.bind_preview_request_to_tab(request_id, first_tab);
        response.request_id = request_id;
        response.path = if request_id == 9003 {
            root.join("wrong.bin")
        } else {
            path.clone()
        };
        response.preview = "File: owned\nSize: 7 B\nCreated: <unavailable>".into();
        app.apply_background_preview_response(response.clone());
        assert_eq!(app.shell.runtime.preview, active_preview);
        let background = &app
            .shell
            .tabs
            .get(0)
            .expect("background tab")
            .result_state
            .committed
            .preview;
        if request_id == 9003 {
            assert_eq!(background, &background_before);
        } else {
            assert!(background.contains("File: owned"));
            assert!(background.contains("Size: 7 B"));
            assert!(background.contains("binary content"));
        }
    }
    fs::remove_dir_all(root).expect("cleanup root");
}

#[test]
fn initial_error_reload_button_dispatches_and_rapid_selection_keeps_latest_information() {
    fn text_shape<'a>(shape: &'a egui::Shape, label: &str) -> Option<&'a egui::epaint::TextShape> {
        match shape {
            egui::Shape::Text(text) if text.galley.text() == label => Some(text),
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| text_shape(shape, label)),
            _ => None,
        }
    }
    let root = test_root("preview-header-pointer-reload");
    fs::create_dir_all(&root).expect("create root");
    let paths = [
        root.join("binary.bin"),
        root.join("empty.txt"),
        root.join("text.txt"),
    ];
    fs::write(&paths[0], b"\0binary").expect("binary");
    fs::write(&paths[1], b"").expect("empty");
    fs::write(&paths[2], b"latest text\n").expect("text");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.ui.show_preview = true;
    app.shell.runtime.committed_for_test_mut().results =
        paths.iter().cloned().map(|path| (path, 0.0)).collect();
    app.shell.runtime.committed_for_test_mut().current_row = Some(0);
    for path in &paths {
        app.set_entry_kind(path, EntryKind::file());
    }
    app.request_preview_for_current();
    settle_preview(&mut app);
    assert!(app.initial_preview_reload_available());
    let ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1100.0, 800.0));
    let mut output = egui::FullOutput::default();
    for _ in 0..3 {
        output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ui| {
                crate::app::render_panels::render_results_and_preview(&mut app, ui);
            },
        );
    }
    let button = output
        .shapes
        .iter()
        .find_map(|shape| text_shape(&shape.shape, "Reload"))
        .expect("visible Reload button");
    let pos = button.pos + button.galley.rect.center().to_vec2();
    fs::write(&paths[0], b"\0changed binary").expect("update size");
    for pressed in [true, false] {
        let _ = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                events: vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |ui| crate::app::render_panels::render_results_and_preview(&mut app, ui),
        );
    }
    assert!(
        app.shell.worker_bus.preview.in_progress,
        "pointer Reload must dispatch"
    );
    settle_preview(&mut app);
    assert!(app.shell.runtime.preview.contains("Size: 15 B"));
    for row in (0..30).map(|step| step % 3) {
        app.shell.runtime.committed_for_test_mut().current_row = Some(row);
        app.request_preview_for_current();
    }
    settle_preview(&mut app);
    let document = app
        .paged_preview_for_current()
        .expect("latest text document");
    assert_eq!(document.header.path, paths[2]);
    assert_eq!(document.header.size, Some(12));
    assert_eq!(document.body(), "latest text\n");
    assert!(!app.shell.runtime.preview.contains("changed binary"));
    fs::remove_dir_all(root).expect("cleanup root");
}

#[cfg(unix)]
#[test]
fn unresolved_directory_and_fifo_links_do_not_open_nonregular_bodies() {
    use std::ffi::CString;
    use std::os::unix::fs::symlink;
    let root = test_root("preview-nonregular-links");
    fs::create_dir_all(root.join("folder")).expect("create folder");
    fs::write(root.join("folder/child.txt"), "child").expect("child fixture");
    let fifo = root.join("pipe");
    let fifo_name = CString::new(fifo.as_os_str().as_encoded_bytes()).expect("fifo path");
    assert_eq!(
        unsafe { libc::mkfifo(fifo_name.as_ptr(), 0o600) },
        0,
        "create FIFO"
    );
    let directory_link = root.join("directory-link");
    let fifo_link = root.join("fifo-link");
    symlink("folder", &directory_link).expect("directory link");
    symlink("pipe", &fifo_link).expect("fifo link");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.ui.show_preview = true;
    app.shell.runtime.committed_for_test_mut().results =
        vec![(fifo_link.clone(), 0.0), (directory_link.clone(), 0.0)];
    for path in [&fifo_link, &directory_link] {
        app.set_entry_kind(path, EntryKind::link_unknown());
    }
    app.shell.runtime.committed_for_test_mut().current_row = Some(0);
    app.request_preview_for_current();
    settle_preview(&mut app);
    assert_eq!(
        app.paged_preview_view.error,
        Some(PreviewPageError::ReadFailed)
    );
    assert!(app.shell.runtime.preview.contains("Target: pipe"));
    assert!(app
        .shell
        .runtime
        .preview
        .contains("Target Size: <unavailable>"));
    app.shell.runtime.committed_for_test_mut().current_row = Some(1);
    app.request_preview_for_current();
    settle_preview(&mut app);
    assert!(app.shell.runtime.preview.contains("Directory:"));
    assert!(app.shell.runtime.preview.contains("Target: folder"));
    assert!(app.shell.runtime.preview.contains("child.txt"));
    fs::remove_dir_all(root).expect("cleanup root");
}

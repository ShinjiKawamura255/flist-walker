use super::*;

#[test]
fn freshness_source_is_visible_and_hoverable_at_compact_widths() {
    use crate::app::freshness::{FileListChange, FileObservation, SnapshotFreshness};
    for width in [640.0, 1000.0] {
        for source in [
            IndexSource::Walker,
            IndexSource::FileList(PathBuf::from("fixture/FileList.txt")),
        ] {
            for lifecycle in [
                TabResourceLifecycle::Ready,
                TabResourceLifecycle::Refreshing,
                TabResourceLifecycle::Failed,
            ] {
                let scope = test_settings_scope("freshness-source-visible");
                let root = test_root("freshness-source-visible");
                let mut app = scope.app(root.clone(), 50, String::new());
                reset_index_request_state_for_test(&mut app);
                let mut snapshot = SnapshotFreshness::acquired(
                    1,
                    root,
                    source.clone(),
                    FileObservation::Unavailable,
                    FileObservation::Unavailable,
                );
                snapshot.change = FileListChange::Unchanged;
                app.shell.runtime.set_snapshot_freshness(snapshot);
                app.shell.indexing.set_lifecycle_for_test(lifecycle);
                let ctx = egui::Context::default();
                let mut time = 0.0;
                let mut frame = |app: &mut FlistWalkerApp, pointer: Option<egui::Pos2>| {
                    time += 0.6;
                    ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(width, 700.0),
                            )),
                            time: Some(time),
                            events: pointer
                                .map(|pos| vec![egui::Event::PointerMoved(pos)])
                                .unwrap_or_default(),
                            ..Default::default()
                        },
                        |ui| app.run_ui_frame(ui),
                    )
                };
                for _ in 0..3 {
                    frame(&mut app, None);
                }
                let output = frame(&mut app, None);
                let label = output
                    .shapes
                    .iter()
                    .find_map(|shape| {
                        if let egui::epaint::Shape::Text(text) = &shape.shape {
                            if text.galley.job.text.starts_with("Source:") {
                                return Some((
                                    egui::Rect::from_min_size(text.pos, text.galley.size()),
                                    shape.clip_rect,
                                ));
                            }
                        }
                        None
                    })
                    .expect("source must be painted inside the compact window");
                assert!(
                    label.1.contains_rect(label.0),
                    "source clipped at {width}: {label:?}"
                );
                assert!(
                    label.0.right() <= width,
                    "source extends past window: {label:?}"
                );
                let hover = label.0.left_top() + egui::vec2(5.0, 5.0);
                frame(&mut app, Some(hover));
                frame(&mut app, None);
                let tooltip = frame(&mut app, None);
                let text = tooltip
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::epaint::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                assert!(
                    text.contains("Acquired:"),
                    "source tooltip unreachable: {width}, {text}"
                );
            }
        }
    }
}

#[test]
fn freshness_mailbox_records_loaded_filelist_under_requested_root_alias() {
    let mailbox = crate::app::index_mailbox::IndexResponseMailbox::new();
    let root = PathBuf::from("chosen/link");
    mailbox.record_snapshot_started(
        7,
        root.clone(),
        IndexSource::FileList(PathBuf::from("physical/root/FileList.txt")),
    );
    let snapshot = mailbox.snapshot().unwrap();
    assert_eq!(
        snapshot.source,
        IndexSource::FileList(root.join("FileList.txt"))
    );
    assert_eq!(snapshot.root, root);
}

#[test]
fn freshness_failed_refresh_and_tab_switch_keep_committed_acquisition() {
    let scope = test_settings_scope("freshness-failure-tabs");
    let root = test_root("freshness-failure-tabs");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("FileList.txt"), "alpha.txt\n").unwrap();
    let mut app = scope.app(root.clone(), 50, String::new());
    let settle = |app: &mut FlistWalkerApp| {
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.shell.indexing.in_progress || app.shell.indexing.pending_finish.is_some() {
            app.poll_index_response();
            app.poll_search_response();
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
    };
    settle(&mut app);
    let acquired = app.shell.runtime.freshness.as_ref().unwrap().acquired_at;
    fs::write(root.join("FileList.txt"), [0xff, 0xff]).unwrap();
    app.refresh_changed_filelist();
    settle(&mut app);
    assert_eq!(
        app.shell.runtime.freshness.as_ref().unwrap().acquired_at,
        acquired
    );
    assert!(app.source_text().contains("Refresh failed"));
    app.create_new_tab();
    app.switch_to_tab_index(0);
    assert_eq!(
        app.shell.runtime.freshness.as_ref().unwrap().acquired_at,
        acquired
    );
    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn freshness_notification_refresh_preserves_query_filters_sort_and_selected_path() {
    let scope = test_settings_scope("freshness-preserve");
    let root = test_root("freshness-preserve");
    let mut app = scope.app(root.clone(), 50, "needle".into());
    reset_index_request_state_for_test(&mut app);
    app.shell
        .runtime
        .replace_results(vec![(root.join("a.txt"), 1.0), (root.join("b.txt"), 0.5)]);
    app.shell.runtime.set_current_row(Some(1));
    app.shell.runtime.result_sort_mode = ResultSortMode::NameDesc;
    app.shell.runtime.include_dirs = false;
    app.refresh_changed_filelist();
    assert_eq!(app.shell.runtime.result_sort_mode, ResultSortMode::NameDesc);
    assert_eq!(app.shell.runtime.query_state.query, "needle");
    assert!(!app.shell.runtime.include_dirs);
    super::super::result_reducer::apply_results_with_selection_policy(
        &mut app,
        vec![(root.join("b.txt"), 1.0), (root.join("a.txt"), 0.5)],
        true,
        false,
    );
    assert_eq!(app.shell.runtime.current_row, Some(0));
    assert!(app.shell.runtime.evicted_selected_path.is_none());
}

#[test]
fn freshness_warning_is_nonmodal_and_does_not_automatically_refresh() {
    use crate::app::freshness::{FileListChange, FileObservation, SnapshotFreshness};
    let scope = test_settings_scope("freshness-warning");
    let root = test_root("freshness-warning");
    let mut app = scope.app(root.clone(), 50, "needle".into());
    reset_index_request_state_for_test(&mut app);
    let mut snapshot = SnapshotFreshness::acquired(
        1,
        root.clone(),
        IndexSource::FileList(root.join("FileList.txt")),
        FileObservation::Unavailable,
        FileObservation::Unavailable,
    );
    snapshot.change = FileListChange::Changed;
    app.shell.runtime.set_snapshot_freshness(snapshot);
    let ctx = egui::Context::default();
    let mut texts = String::new();
    let input = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1000.0, 700.0),
        )),
        ..Default::default()
    };
    // egui sizes nested panels on the initial passes before painting them.
    for _ in 0..3 {
        let _ = ctx.run_ui(input(), |ui| app.run_ui_frame(ui));
    }
    let output = ctx.run_ui(input(), |ui| app.run_ui_frame(ui));
    for shape in output.shapes {
        if let egui::epaint::Shape::Text(text) = shape.shape {
            texts.push_str(&text.galley.job.text);
            texts.push('\n');
        }
    }
    assert!(texts.contains("FileList changed since loading"), "{texts}");
    assert!(texts.contains("Refresh Index"), "{texts}");
    assert!(!app.shell.ui.help_open);
    assert_eq!(app.shell.runtime.query_state.query, "needle");
    assert!(
        !app.shell.indexing.in_progress,
        "notification must not automatically refresh"
    );
}

#[test]
fn freshness_banner_pointer_refresh_preserves_state_after_success() {
    use crate::app::freshness::FileListChange;
    let scope = test_settings_scope("freshness-pointer");
    let root = test_root("freshness-pointer");
    fs::create_dir_all(&root).unwrap();
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs::write(root.join(name), name).unwrap();
    }
    fs::write(root.join("FileList.txt"), "a.txt\nb.txt\n").unwrap();
    let mut app = scope.app(root.clone(), 50, "txt".into());
    let settle = |app: &mut FlistWalkerApp| {
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.shell.indexing.in_progress
            || app.shell.indexing.pending_finish.is_some()
            || app.shell.search.in_progress()
            || app.shell.worker_bus.sort.in_progress
        {
            app.poll_index_response();
            app.poll_search_response();
            app.poll_sort_response();
            assert!(Instant::now() < deadline, "refresh did not settle");
            thread::yield_now();
        }
    };
    settle(&mut app);
    app.shell.runtime.ignore_case = false;
    app.set_result_sort_mode(ResultSortMode::NameDesc);
    settle(&mut app);
    let selected = root.join("a.txt");
    let row = app
        .shell
        .runtime
        .results
        .iter()
        .position(|(path, _)| path == &selected)
        .unwrap();
    app.set_current_row(Some(row));
    let acquired = app.shell.runtime.freshness.as_ref().unwrap().request_id;
    fs::write(root.join("FileList.txt"), "a.txt\nb.txt\nc.txt\n").unwrap();
    app.shell.runtime.snapshot_freshness_mut().unwrap().change = FileListChange::Changed;
    let ctx = egui::Context::default();
    let frame = |app: &mut FlistWalkerApp, events| {
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(640.0, 700.0),
                )),
                events,
                ..Default::default()
            },
            |ui| app.run_ui_frame(ui),
        )
    };
    for _ in 0..3 {
        frame(&mut app, vec![]);
    }
    let output = frame(&mut app, vec![]);
    let button = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::epaint::Shape::Text(text) if text.galley.job.text == "Refresh Index" => {
                Some(text.pos + text.galley.size() / 2.0)
            }
            _ => None,
        })
        .next()
        .expect("banner refresh button is painted");
    // The banner comes before the normal action row in the normal render output.
    for pressed in [true, false] {
        frame(
            &mut app,
            vec![
                egui::Event::PointerMoved(button),
                egui::Event::PointerButton {
                    pos: button,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    assert!(
        app.shell.indexing.in_progress,
        "real pointer click must dispatch refresh"
    );
    settle(&mut app);
    assert_eq!(app.shell.runtime.query_state.query, "txt");
    assert!(!app.shell.runtime.ignore_case);
    assert_eq!(app.shell.runtime.result_sort_mode, ResultSortMode::NameDesc);
    let row = app.shell.runtime.current_row.unwrap();
    assert_eq!(app.shell.runtime.results[row].0, selected);
    assert!(app
        .shell
        .runtime
        .results
        .iter()
        .any(|(path, _)| path == &root.join("c.txt")));
    let snapshot = app.shell.runtime.freshness.as_ref().unwrap();
    assert_ne!(snapshot.request_id, acquired);
    assert_eq!(snapshot.change, FileListChange::Unchanged);
    drop(app);
    fs::remove_dir_all(root).unwrap();
}

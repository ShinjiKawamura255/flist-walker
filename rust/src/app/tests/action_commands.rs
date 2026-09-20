use super::*;
use crate::actions::{
    action_target_path_for_open_in_folder, authorize_action_targets, lexical_action_path_precheck,
    ActionPathPrecheck,
};
use crate::app::worker::bus::ActionFreshnessRegistry;
use crate::app::worker::channel::bounded_request_channel;
#[cfg(target_os = "windows")]
use crate::app::worker::tasks::action_notice_for_targets;
use crate::app::worker::tasks::{
    process_action_request_with, process_action_request_with_outcome, spawn_action_worker_with,
    ActionTerminalOutcome, SharedActionExecutor,
};
use std::sync::atomic::AtomicUsize;

#[test]
fn ux_row_activation_ignores_hidden_pins_but_batch_activation_retains_them() {
    let root = test_root("ux-row-action-pins");
    fs::create_dir_all(&root).expect("create root");
    let row_path = root.join("clicked.txt");
    let pinned = root.join("hidden-pin.txt");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = bounded_request_channel::<ActionRequest>(8);
    app.shell.worker_bus.action.tx = tx;
    app.shell.runtime.committed_for_test_mut().results = vec![(row_path.clone(), 0.0)];
    app.shell.runtime.committed_for_test_mut().current_row = Some(0);
    app.shell.runtime.pinned_paths.insert(pinned.clone());

    for open_parent in [false, true] {
        app.execute_result_row_for_activation(0, open_parent);
        let request = rx.try_recv().expect("row action request");
        assert_eq!(request.paths, vec![row_path.clone()]);
        assert_eq!(request.open_parent_for_files, open_parent);
        assert_eq!(request.root, root);
        assert!(app.shell.runtime.pinned_paths.contains(&pinned));
    }
    app.execute_selected();
    assert_eq!(rx.try_recv().expect("batch request").paths, vec![pinned]);
    app.execute_result_row_for_activation(99, false);
    assert!(
        rx.try_recv().is_err(),
        "invalid row must not fall back to pins"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ux_row_activation_keeps_lexical_root_guard() {
    let root = test_root("ux-row-action-root-guard");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = bounded_request_channel::<ActionRequest>(8);
    app.shell.worker_bus.action.tx = tx;
    app.shell.runtime.committed_for_test_mut().results =
        vec![(root.join("..").join("outside.txt"), 0.0)];
    app.shell.runtime.pinned_paths.insert(root.join("safe.txt"));
    app.execute_result_row_for_activation(0, true);
    assert!(rx.try_recv().is_err());
    assert!(app.shell.runtime.notice.starts_with("Action blocked:"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ux_inspector_removes_hidden_pin_and_escape_preserves_remaining_selection() {
    let root = test_root("ux-pin-inspector");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "query".to_string());
    let hidden = root.join("hidden.txt");
    let keep = root.join("keep.txt");
    app.shell.runtime.pinned_paths.insert(hidden.clone());
    app.shell.runtime.pinned_paths.insert(keep.clone());
    app.open_selection_inspector();
    app.remove_inspected_pin(&hidden);
    assert!(!app.shell.runtime.pinned_paths.contains(&hidden));
    assert!(app.shell.runtime.pinned_paths.contains(&keep));

    let ctx = egui::Context::default();
    let _ = ctx.run_ui(
        egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        },
        |ui| {
            assert!(app.handle_selection_inspector_shortcuts(ui.ctx()));
        },
    );
    assert!(app.shell.ui.selection_inspector.is_none());
    assert_eq!(app.shell.runtime.query_state.query, "query");
    assert!(app.shell.runtime.pinned_paths.contains(&keep));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ux_inspector_renders_at_narrow_width_and_closes_on_context_change() {
    let root = test_root("ux-pin-inspector-render");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    for n in 0..10_000 {
        app.shell
            .runtime
            .pinned_paths
            .insert(root.join(format!("item-{n:05}.txt")));
    }
    let ctx = egui::Context::default();
    app.open_selection_inspector();
    let output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, 600.0),
            )),
            ..Default::default()
        },
        |ui| app.render_selection_inspector(ui.ctx()),
    );
    assert!(!output.shapes.is_empty());
    assert!(app.shell.ui.selection_inspector.is_some());
    app.shell.runtime.root = root.join("different-context");
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
        app.render_selection_inspector(ui.ctx())
    });
    assert!(app.shell.ui.selection_inspector.is_none());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ux_inspector_contains_input_on_first_and_closing_frames() {
    fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }
    for close in [None, Some(egui::Key::Escape), Some(egui::Key::G)] {
        let root = test_root("ux-inspector-input-containment");
        fs::create_dir_all(&root).expect("create root");
        let mut app = FlistWalkerApp::new(root.clone(), 50, "keep query".to_string());
        app.shell.runtime.emacs_keybindings_enabled = true;
        app.shell.runtime.pinned_paths.insert(root.join("keep.txt"));
        let (tx, rx) = bounded_request_channel::<ActionRequest>(8);
        app.shell.worker_bus.action.tx = tx;
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| app.run_ui_frame(ui));
        app.open_selection_inspector();
        let mut events = vec![
            egui::Event::Text("leaked text".to_string()),
            egui::Event::Paste("leaked paste".to_string()),
            egui::Event::Copy,
            egui::Event::Cut,
            egui::Event::Ime(egui::ImeEvent::Commit("入力".to_string())),
            key(egui::Key::Enter, egui::Modifiers::NONE),
            key(egui::Key::J, emacs_shortcut_modifiers(false)),
            key(egui::Key::M, emacs_shortcut_modifiers(false)),
            key(egui::Key::Space, egui::Modifiers::NONE),
        ];
        if let Some(close_key) = close {
            let modifiers = if close_key == egui::Key::G {
                emacs_shortcut_modifiers(false)
            } else {
                egui::Modifiers::NONE
            };
            events.push(key(close_key, modifiers));
        }
        let output = ctx.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| {
                app.run_ui_frame(ui);
            },
        );
        assert_eq!(
            app.shell.runtime.query_state.query, "keep query",
            "close={close:?}"
        );
        assert_eq!(app.shell.runtime.pinned_paths.len(), 1);
        assert_eq!(app.shell.ui.selection_inspector.is_some(), close.is_none());
        assert!(
            rx.try_recv().is_err(),
            "modal must block Enter and Emacs activation"
        );
        assert!(!app.shell.ui.pending_copy_shortcut);
        assert!(
            !output
                .platform_output
                .commands
                .iter()
                .any(|command| { matches!(command, egui::OutputCommand::CopyText(_)) }),
            "modal must not copy the backing query"
        );
        let _ = fs::remove_dir_all(&root);
    }
}

#[test]
fn ux_inspector_keyboard_can_remove_and_close_without_activating_files() {
    fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }
    for remove in [true, false] {
        let root = test_root("ux-inspector-keyboard");
        fs::create_dir_all(&root).expect("create root");
        let mut app = FlistWalkerApp::new(root.clone(), 50, "keep query".to_string());
        app.shell.runtime.pinned_paths.insert(root.join("keep.txt"));
        let (tx, rx) = bounded_request_channel::<ActionRequest>(8);
        app.shell.worker_bus.action.tx = tx;
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| app.run_ui_frame(ui));
        app.open_selection_inspector();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| app.run_ui_frame(ui));
        if remove {
            let _ = ctx.run_ui(
                egui::RawInput {
                    events: vec![key(egui::Key::Tab, egui::Modifiers::SHIFT)],
                    ..Default::default()
                },
                |ui| app.run_ui_frame(ui),
            );
        }
        let _ = ctx.run_ui(
            egui::RawInput {
                events: vec![key(
                    if remove {
                        egui::Key::Space
                    } else {
                        egui::Key::Enter
                    },
                    egui::Modifiers::NONE,
                )],
                ..Default::default()
            },
            |ui| app.run_ui_frame(ui),
        );
        if remove {
            assert!(
                app.shell.runtime.pinned_paths.is_empty(),
                "Shift+Tab then Space must activate Remove"
            );
            assert!(app.shell.ui.selection_inspector.is_some());
        } else {
            assert!(
                app.shell.ui.selection_inspector.is_none(),
                "Enter must activate the focused Close button"
            );
            assert_eq!(app.shell.runtime.pinned_paths.len(), 1);
        }
        assert_eq!(app.shell.runtime.query_state.query, "keep query");
        assert!(
            rx.try_recv().is_err(),
            "inspector keys must never activate files"
        );
        let _ = fs::remove_dir_all(&root);
    }
}

#[test]
fn execute_selected_enqueues_action_request_without_sync_io() {
    let root = test_root("async-action-enqueue");
    fs::create_dir_all(&root).expect("create dir");
    let missing = root.join("missing-not-executed");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (action_tx_req, action_rx_req) = bounded_request_channel::<ActionRequest>(8);
    let (_action_tx_res, action_rx_res) = mpsc::channel::<ActionResponse>();
    app.shell.worker_bus.action.tx = action_tx_req;
    app.shell.worker_bus.action.rx = action_rx_res;
    app.shell.runtime.committed_for_test_mut().results = vec![(missing.clone(), 0.0)];
    app.shell.runtime.committed_for_test_mut().current_row = Some(0);

    app.execute_selected();

    let req = action_rx_req
        .try_recv()
        .expect("action request should be enqueued");
    assert_eq!(req.paths, vec![missing]);
    assert_eq!(req.root, root);
    assert!(!req.open_parent_for_files);
    assert!(app.shell.worker_bus.action.pending_request_id.is_some());
    assert!(app.shell.worker_bus.action.in_progress);
    assert!(!app.shell.runtime.notice.starts_with("Action failed:"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn action_freshness_requires_exact_request_identity_and_trusted_root() {
    let freshness = ActionFreshnessRegistry::default();
    let root = PathBuf::from("trusted-root");
    assert!(freshness.activate(17, &root));

    assert!(freshness.is_current(17, &root));
    assert!(!freshness.is_current(18, &root));
    assert!(!freshness.is_current(17, Path::new("different-root")));

    freshness.invalidate(17);
    assert!(!freshness.is_current(17, &root));
}

#[test]
fn tc_164_tab_close_invalidates_routed_action_freshness() {
    let root = test_root("tc-164-tab-close-action");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let closing_tab_id = app.current_tab_id().expect("closing tab id");
    app.create_new_tab();
    let request_id = 71;
    assert!(app
        .shell
        .worker_bus
        .action
        .prepare_request(request_id, &root));
    app.bind_action_request_to_tab(request_id, closing_tab_id);

    app.close_tab_index(0);

    assert!(!app
        .shell
        .worker_bus
        .action
        .freshness
        .is_current(request_id, &root));
    assert_eq!(app.action_request_tab(request_id), None);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_164_current_tab_precheck_rejection_preserves_background_action() {
    let root = test_root("tc-164-tab-scoped-precheck");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let background_tab_id = app.current_tab_id().expect("background tab id");
    let background_request_id = 72;
    assert!(app
        .shell
        .worker_bus
        .action
        .prepare_request(background_request_id, &root));
    app.shell
        .worker_bus
        .action
        .accept_request(background_request_id);
    app.bind_action_request_to_tab(background_request_id, background_tab_id);

    app.create_new_tab();
    let active_tab_id = app.current_tab_id().expect("active tab id");
    assert_ne!(active_tab_id, background_tab_id);
    let escaped = root.join("..").join("outside").join("blocked.txt");
    app.shell.runtime.committed_for_test_mut().results = vec![(escaped, 0.0)];
    app.shell.runtime.committed_for_test_mut().current_row = Some(0);

    app.execute_selected();

    assert!(app
        .shell
        .worker_bus
        .action
        .freshness
        .is_current(background_request_id, &root));
    assert_eq!(
        app.action_request_tab(background_request_id),
        Some(background_tab_id)
    );
    let background = app.shell.tabs.get(0).expect("background tab");
    assert_eq!(
        background.pending_action_request_id,
        Some(background_request_id)
    );
    assert!(background.action_in_progress);
    assert!(app.shell.runtime.notice.starts_with("Action blocked:"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_164_gui_root_switch_stops_backend_calls_after_inflight_target() {
    use std::sync::{Condvar, Mutex};

    let root = test_root("tc-164-gui-action-root-switch");
    let next_root = test_root("tc-164-gui-action-next-root");
    fs::create_dir_all(&root).expect("create root");
    fs::create_dir_all(&next_root).expect("create next root");
    let first = root.join("first.txt");
    let second = root.join("second.txt");
    fs::write(&first, "first").expect("write first");
    fs::write(&second, "second").expect("write second");

    let shutdown = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(AtomicUsize::new(0));
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let (started_tx, started_rx) = mpsc::channel();
    let executor: SharedActionExecutor = {
        let calls = Arc::clone(&calls);
        let gate = Arc::clone(&gate);
        Arc::new(move |_| {
            let call_index = calls.fetch_add(1, Ordering::SeqCst);
            if call_index == 0 {
                started_tx.send(()).expect("signal first backend call");
                let (lock, ready) = &*gate;
                let mut open = lock.lock().expect("lock gate");
                while !*open {
                    open = ready.wait(open).expect("wait gate");
                }
            }
            Ok(())
        })
    };
    let (tx, rx, handles, freshness) = spawn_action_worker_with(Arc::clone(&shutdown), executor);

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.worker_bus.action.tx = tx;
    app.shell.worker_bus.action.rx = rx;
    app.shell.worker_bus.action.freshness = freshness;
    app.shell.runtime.pinned_paths.insert(first);
    app.shell.runtime.pinned_paths.insert(second);
    app.execute_selected();

    started_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("first backend call started");
    assert!(app.try_retire_active_root_resources(next_root.clone()));
    assert_eq!(app.shell.runtime.root, next_root);

    let (lock, ready) = &*gate;
    *lock.lock().expect("lock gate") = true;
    ready.notify_all();
    let response = app
        .shell
        .worker_bus
        .action
        .rx
        .recv_timeout(Duration::from_secs(1))
        .expect("action worker response");

    assert!(response.notice.contains("superseded"));
    assert_eq!(
        app.shell.worker_bus.action.freshness.active_count(),
        0,
        "worker terminal cleanup must revoke the request even before UI routing"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "root switch must prevent every later backend call"
    );

    shutdown.store(true, Ordering::Relaxed);
    drop(app);
    for handle in handles {
        handle.join().expect("join action worker");
    }
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&next_root);
}

#[test]
fn execute_selected_for_activation_uses_open_folder_mode_when_requested() {
    let root = test_root("activation-open-folder");
    let folder = root.join("src");
    fs::create_dir_all(&folder).expect("create dir");
    let selected = folder.join("picked.txt");
    fs::write(&selected, "x").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (action_tx_req, action_rx_req) = bounded_request_channel::<ActionRequest>(8);
    let (_action_tx_res, action_rx_res) = mpsc::channel::<ActionResponse>();
    app.shell.worker_bus.action.tx = action_tx_req;
    app.shell.worker_bus.action.rx = action_rx_res;
    app.shell.runtime.committed_for_test_mut().results = vec![(selected.clone(), 0.0)];
    app.shell.runtime.committed_for_test_mut().current_row = Some(0);

    app.execute_selected_for_activation(true);

    let req = action_rx_req
        .try_recv()
        .expect("action request should be enqueued");
    assert_eq!(req.paths, vec![selected]);
    assert_eq!(req.root, root);
    assert!(req.open_parent_for_files);
    let _ = fs::remove_dir_all(&root);
}

#[test]
#[cfg(target_os = "windows")]
fn execute_selected_notice_normalizes_extended_prefix() {
    let root = test_root("action-notice-normalize");
    fs::create_dir_all(&root).expect("create dir");
    let selected = root.join("file.txt");
    fs::write(&selected, "x").expect("write file");
    let extended = PathBuf::from(format!(
        r"\\?\{}",
        selected.to_string_lossy().replace('/', r"\")
    ));
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (action_tx_req, _action_rx_req) = bounded_request_channel::<ActionRequest>(8);
    let (_action_tx_res, action_rx_res) = mpsc::channel::<ActionResponse>();
    app.shell.worker_bus.action.tx = action_tx_req;
    app.shell.worker_bus.action.rx = action_rx_res;
    app.shell.runtime.committed_for_test_mut().results = vec![(extended, 0.0)];
    app.shell.runtime.committed_for_test_mut().current_row = Some(0);

    app.execute_selected();

    assert_eq!(
        app.shell.runtime.notice,
        format!("Action: {}", selected.display())
    );
    assert!(!app.shell.runtime.notice.contains(r"\\?\"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn execute_selected_defers_absolute_outside_path_to_worker() {
    let root = test_root("action-block-outside-root");
    let outside_root = test_root("action-block-outside-root-other");
    let outside = outside_root.join("tool.exe");
    fs::create_dir_all(&root).expect("create root");
    fs::create_dir_all(outside.parent().expect("outside parent")).expect("create outside parent");
    fs::write(&outside, "x").expect("write outside file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (action_tx_req, action_rx_req) = bounded_request_channel::<ActionRequest>(8);
    let (_action_tx_res, action_rx_res) = mpsc::channel::<ActionResponse>();
    app.shell.worker_bus.action.tx = action_tx_req;
    app.shell.worker_bus.action.rx = action_rx_res;
    app.shell.runtime.committed_for_test_mut().results = vec![(outside.clone(), 0.0)];
    app.shell.runtime.committed_for_test_mut().current_row = Some(0);

    app.execute_selected();

    let request = action_rx_req
        .try_recv()
        .expect("potentially link-resolved path must reach worker authorization");
    assert_eq!(request.root, root);
    assert_eq!(request.paths, vec![outside]);
    assert!(app.shell.worker_bus.action.pending_request_id.is_some());
    assert!(app.shell.worker_bus.action.in_progress);
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&outside_root);
}

#[test]
fn execute_selected_allows_unc_like_path_when_under_current_root() {
    let root = PathBuf::from(r"\\server\share\workspace");
    let child = PathBuf::from(r"\\server\share\workspace\bin\tool.exe");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (action_tx_req, action_rx_req) = bounded_request_channel::<ActionRequest>(8);
    let (_action_tx_res, action_rx_res) = mpsc::channel::<ActionResponse>();
    app.shell.worker_bus.action.tx = action_tx_req;
    app.shell.worker_bus.action.rx = action_rx_res;
    app.shell.runtime.committed_for_test_mut().results = vec![(child.clone(), 0.0)];
    app.shell.runtime.committed_for_test_mut().current_row = Some(0);

    app.execute_selected();

    let req = action_rx_req
        .try_recv()
        .expect("UNC-like child should be enqueued");
    assert_eq!(req.paths, vec![child]);
    assert!(app.shell.worker_bus.action.pending_request_id.is_some());
    assert!(app.shell.worker_bus.action.in_progress);
}

#[test]
fn tc_150_action_worker_uses_two_workers_and_bounds_total_to_ten() {
    use std::sync::{Condvar, Mutex};

    let root = test_root("tc-150-action-worker-bound");
    fs::create_dir_all(&root).expect("create dir");
    let selected = root.join("selected.txt");
    fs::write(&selected, "selected").expect("write selected");
    let shutdown = Arc::new(AtomicBool::new(false));
    let active = Arc::new(AtomicUsize::new(0));
    let max_active = Arc::new(AtomicUsize::new(0));
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let (started_tx, started_rx) = mpsc::channel();
    let executor: SharedActionExecutor = {
        let active = Arc::clone(&active);
        let max_active = Arc::clone(&max_active);
        let gate = Arc::clone(&gate);
        Arc::new(move |_| {
            let now = active.fetch_add(1, Ordering::SeqCst) + 1;
            max_active.fetch_max(now, Ordering::SeqCst);
            started_tx.send(()).expect("signal started");
            let (lock, ready) = &*gate;
            let mut open = lock.lock().expect("lock gate");
            while !*open {
                open = ready.wait(open).expect("wait gate");
            }
            active.fetch_sub(1, Ordering::SeqCst);
            Ok(())
        })
    };
    let (tx, rx, handles, freshness) = spawn_action_worker_with(Arc::clone(&shutdown), executor);
    assert_eq!(
        handles.len(),
        2,
        "action executor must have exactly two workers"
    );

    let request = |request_id| ActionRequest {
        request_id,
        root: root.clone(),
        paths: vec![selected.clone()],
        open_parent_for_files: false,
    };
    assert!(freshness.activate(1, &root));
    tx.send(request(1)).expect("send first action");
    assert!(freshness.activate(2, &root));
    tx.send(request(2)).expect("send second action");
    started_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("first worker started");
    started_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("second worker started");
    assert_eq!(max_active.load(Ordering::SeqCst), 2);

    for request_id in 3..=10 {
        assert!(freshness.activate(request_id, &root));
        tx.send(request(request_id))
            .expect("fill bounded action queue");
    }
    assert_eq!(
        tx.load(),
        crate::app::worker::channel::WorkerLoadSnapshot {
            queued: 8,
            inflight: 2,
            capacity: 8,
        }
    );
    assert!(matches!(
        tx.try_send(request(11)),
        Err(mpsc::TrySendError::Full(_))
    ));

    let (lock, ready) = &*gate;
    *lock.lock().expect("lock gate") = true;
    ready.notify_all();
    for _ in 0..10 {
        rx.recv_timeout(Duration::from_secs(1))
            .expect("receive bounded action response");
    }
    for _ in 0..1_000 {
        if tx.load().inflight == 0 {
            break;
        }
        thread::yield_now();
    }
    assert_eq!(tx.load().queued, 0);
    assert_eq!(tx.load().inflight, 0);
    assert_eq!(freshness.active_count(), 0);

    shutdown.store(true, Ordering::Relaxed);
    drop(tx);
    for handle in handles {
        handle.join().expect("join action worker");
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_153_action_shutdown_drains_accepted_queue_with_terminal_cancellation() {
    let shutdown = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(AtomicUsize::new(0));
    let executor: SharedActionExecutor = {
        let calls = Arc::clone(&calls);
        Arc::new(move |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    };
    let (tx, rx, handles, _freshness) = spawn_action_worker_with(Arc::clone(&shutdown), executor);
    shutdown.store(true, Ordering::Relaxed);
    for request_id in 1..=4 {
        tx.send(ActionRequest {
            request_id,
            root: PathBuf::from("shutdown-root"),
            paths: vec![PathBuf::from("shutdown-root/selected.txt")],
            open_parent_for_files: false,
        })
        .expect("accept action before channel close");
    }
    drop(tx);

    let mut settled = Vec::new();
    for _ in 0..4 {
        let response = rx
            .recv_timeout(Duration::from_secs(1))
            .expect("terminal shutdown action response");
        assert!(response.notice.contains("shutting down"));
        settled.push(response.request_id);
    }
    settled.sort_unstable();
    assert_eq!(settled, vec![1, 2, 3, 4]);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    for handle in handles {
        handle.join().expect("join action worker");
    }
}

#[test]
fn tc_153_action_terminal_outcome_distinguishes_success_and_executor_failure() {
    let root = test_root("tc-153-action-outcome");
    fs::create_dir_all(&root).expect("create root");
    let selected = root.join("selected.txt");
    fs::write(&selected, "selected").expect("write selected");
    let request = || ActionRequest {
        request_id: 41,
        root: root.clone(),
        paths: vec![selected.clone()],
        open_parent_for_files: false,
    };

    let (_response, completed) = process_action_request_with_outcome(request(), |_| Ok(()));
    assert_eq!(completed, ActionTerminalOutcome::Completed);
    assert_eq!(completed.as_str(), "completed");

    let (_response, failed) =
        process_action_request_with_outcome(request(), |_| anyhow::bail!("executor failure"));
    assert_eq!(failed, ActionTerminalOutcome::Failed);
    assert_eq!(failed.as_str(), "failed");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_150_action_full_preserves_prior_accepted_request_state() {
    let root = test_root("tc-150-action-full");
    fs::create_dir_all(&root).expect("create root");
    let selected = root.join("selected.txt");
    fs::write(&selected, "selected").expect("write selected");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = bounded_request_channel::<ActionRequest>(1);
    tx.send(ActionRequest {
        request_id: 1,
        root: root.clone(),
        paths: vec![selected.clone()],
        open_parent_for_files: false,
    })
    .expect("fill action queue");
    app.shell.worker_bus.action.tx = tx;
    let prior_request_id = 41;
    let next_request_id = 42;
    app.shell.worker_bus.action.next_request_id = next_request_id;
    assert!(app
        .shell
        .worker_bus
        .action
        .prepare_request(prior_request_id, &root));
    app.shell.worker_bus.action.pending_request_id = Some(prior_request_id);
    app.shell.worker_bus.action.in_progress = true;
    let tab_id = app.current_tab_id().expect("tab id");
    app.bind_action_request_to_tab(prior_request_id, tab_id);
    app.shell.runtime.committed_for_test_mut().results = vec![(selected, 0.0)];
    app.shell.runtime.committed_for_test_mut().current_row = Some(0);

    app.execute_selected();

    assert_eq!(
        app.shell.worker_bus.action.pending_request_id,
        Some(prior_request_id)
    );
    assert!(app.shell.worker_bus.action.in_progress);
    assert_eq!(app.action_request_tab(prior_request_id), Some(tab_id));
    assert_eq!(app.action_request_tab(next_request_id), None);
    assert!(app
        .shell
        .worker_bus
        .action
        .freshness
        .is_current(prior_request_id, &root));
    assert!(!app
        .shell
        .worker_bus
        .action
        .freshness
        .is_current(next_request_id, &root));
    assert!(app.shell.runtime.notice.contains("busy"));
    drop(rx);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_150_action_disconnect_settles_action_state() {
    let root = test_root("tc-150-action-disconnect");
    fs::create_dir_all(&root).expect("create root");
    let selected = root.join("selected.txt");
    fs::write(&selected, "selected").expect("write selected");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = bounded_request_channel::<ActionRequest>(1);
    drop(rx);
    app.shell.worker_bus.action.tx = tx;
    let prior_request_id = 41;
    assert!(app
        .shell
        .worker_bus
        .action
        .prepare_request(prior_request_id, &root));
    app.shell.worker_bus.action.pending_request_id = Some(prior_request_id);
    app.shell.worker_bus.action.in_progress = true;
    let tab_id = app.current_tab_id().expect("tab id");
    app.bind_action_request_to_tab(prior_request_id, tab_id);
    app.shell.runtime.committed_for_test_mut().results = vec![(selected, 0.0)];
    app.shell.runtime.committed_for_test_mut().current_row = Some(0);

    app.execute_selected();

    assert_eq!(app.shell.worker_bus.action.pending_request_id, None);
    assert!(!app.shell.worker_bus.action.in_progress);
    assert_eq!(app.action_request_tab(prior_request_id), None);
    assert_eq!(app.shell.worker_bus.action.freshness.active_count(), 0);
    assert!(app.shell.runtime.notice.contains("unavailable"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn action_target_path_for_open_in_folder_maps_file_and_directory() {
    let root = test_root("open-folder-target");
    let dir = root.join("dir");
    fs::create_dir_all(&dir).expect("create dir");
    let file = dir.join("main.rs");
    fs::write(&file, "fn main() {}").expect("write file");

    let from_file = action_target_path_for_open_in_folder(&file).expect("file target");
    let from_dir = action_target_path_for_open_in_folder(&dir).expect("directory target");

    assert_eq!(from_file, dir);
    assert_eq!(from_dir, root.join("dir"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn authorized_action_targets_deduplicate_same_parent_directory() {
    let root = test_root("open-folder-target-dedup");
    let dir_a = root.join("dir-a");
    let dir_b = root.join("dir-b");
    fs::create_dir_all(&dir_a).expect("create dir a");
    fs::create_dir_all(&dir_b).expect("create dir b");
    let file_a1 = dir_a.join("main.rs");
    let file_a2 = dir_a.join("lib.rs");
    let file_b = dir_b.join("mod.rs");
    fs::write(&file_a1, "fn main() {}").expect("write file a1");
    fs::write(&file_a2, "pub fn f() {}").expect("write file a2");
    fs::write(&file_b, "pub fn g() {}").expect("write file b");

    let targets = authorize_action_targets(&root, &[file_a1, file_a2, file_b, dir_a.clone()], true)
        .expect("authorize targets")
        .targets
        .into_iter()
        .map(|target| target.display_path)
        .collect::<Vec<_>>();

    assert_eq!(targets, vec![dir_a, dir_b]);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_050_ui_precheck_rejects_parent_escape_and_defers_safe_or_ambiguous_paths() {
    let root = test_root("action-precheck-root");
    let inside = root.join("src").join("..").join("main.rs");
    let outside = root.join("..").join("outside").join("tool.exe");

    assert_eq!(
        lexical_action_path_precheck(&root, &inside),
        ActionPathPrecheck::Defer
    );
    assert_eq!(
        lexical_action_path_precheck(&root, &outside),
        ActionPathPrecheck::Reject
    );
    assert_eq!(
        lexical_action_path_precheck(&root, Path::new("relative/path")),
        ActionPathPrecheck::Defer
    );
}

#[cfg(unix)]
#[test]
fn tc_051_link_root_with_resolved_result_reaches_worker_authorization() {
    use std::os::unix::fs::symlink;

    let container = test_root("action-linked-root-container");
    let actual_root = test_root("action-linked-root-target");
    fs::create_dir_all(&container).expect("create link container");
    fs::create_dir_all(&actual_root).expect("create actual root");
    let linked_root = container.join("linked-root");
    symlink(&actual_root, &linked_root).expect("create root symlink");
    let selected = actual_root.join("selected.txt");
    fs::write(&selected, "selected").expect("write selected target");
    let mut app = FlistWalkerApp::new(linked_root.clone(), 50, String::new());
    let (action_tx_req, action_rx_req) = bounded_request_channel::<ActionRequest>(8);
    let (_action_tx_res, action_rx_res) = mpsc::channel::<ActionResponse>();
    app.shell.worker_bus.action.tx = action_tx_req;
    app.shell.worker_bus.action.rx = action_rx_res;
    app.shell.runtime.committed_for_test_mut().results = vec![(selected.clone(), 0.0)];
    app.shell.runtime.committed_for_test_mut().current_row = Some(0);

    app.execute_selected();

    let request = action_rx_req
        .try_recv()
        .expect("resolved result under a linked root must reach the worker");
    assert_eq!(request.root, linked_root);
    assert_eq!(request.paths, vec![selected.clone()]);
    assert!(app.shell.worker_bus.action.in_progress);
    let mut calls = Vec::new();
    let response = process_action_request_with(request, |path| {
        calls.push(path.to_path_buf());
        Ok(())
    });
    assert_eq!(
        calls,
        vec![selected.canonicalize().expect("canonical target")]
    );
    assert!(!response.notice.starts_with("Action blocked:"));
    let _ = fs::remove_dir_all(&container);
    let _ = fs::remove_dir_all(&actual_root);
}

#[test]
fn tc_050_worker_rejects_mixed_selection_before_executor_call() {
    let root = test_root("action-worker-mixed-root");
    let outside_root = test_root("action-worker-mixed-outside");
    fs::create_dir_all(&root).expect("create root");
    fs::create_dir_all(&outside_root).expect("create outside root");
    let inside = root.join("inside.txt");
    let outside = outside_root.join("outside.txt");
    fs::write(&inside, "inside").expect("write inside");
    fs::write(&outside, "outside").expect("write outside");
    let mut calls = Vec::new();

    let response = process_action_request_with(
        ActionRequest {
            request_id: 100,
            root: root.clone(),
            paths: vec![inside, outside],
            open_parent_for_files: false,
        },
        |path| {
            calls.push(path.to_path_buf());
            Ok(())
        },
    );

    assert!(calls.is_empty(), "preauthorization must be all-or-nothing");
    assert!(response.notice.starts_with("Action blocked:"));
    assert!(response.notice.contains("outside current root"));
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&outside_root);
}

#[test]
fn tc_050_worker_dispatches_only_resolved_path_and_preserves_display_notice() {
    let root = test_root("action-worker-resolved-root");
    fs::create_dir_all(&root).expect("create root");
    fs::create_dir(root.join("sub")).expect("create intermediate directory");
    let selected = root.join("sub").join("..").join("selected.txt");
    let actual = root.join("selected.txt");
    fs::write(&actual, "selected").expect("write selected");
    let canonical = actual.canonicalize().expect("canonical target");
    let mut calls = Vec::new();

    let response = process_action_request_with(
        ActionRequest {
            request_id: 101,
            root: root.clone(),
            paths: vec![selected.clone()],
            open_parent_for_files: false,
        },
        |path| {
            calls.push(path.to_path_buf());
            Ok(())
        },
    );

    assert_eq!(calls, vec![canonical]);
    assert_eq!(
        response.notice,
        format!("Action: {}", normalize_path_for_display(&selected))
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_050_worker_fails_closed_when_target_cannot_be_resolved() {
    let root = test_root("action-worker-missing-root");
    fs::create_dir_all(&root).expect("create root");
    let missing = root.join("missing.txt");
    let mut call_count = 0usize;

    let response = process_action_request_with(
        ActionRequest {
            request_id: 102,
            root: root.clone(),
            paths: vec![missing],
            open_parent_for_files: false,
        },
        |_| {
            call_count += 1;
            Ok(())
        },
    );

    assert_eq!(call_count, 0);
    assert!(response.notice.starts_with("Action blocked:"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_050_worker_fails_closed_when_root_cannot_be_resolved() {
    let root = test_root("action-worker-unresolved-root");
    let selected = root.join("missing.txt");
    let mut call_count = 0usize;

    let response = process_action_request_with(
        ActionRequest {
            request_id: 106,
            root,
            paths: vec![selected.clone()],
            open_parent_for_files: false,
        },
        |_| {
            call_count += 1;
            Ok(())
        },
    );

    assert_eq!(call_count, 0);
    assert!(response
        .notice
        .contains(&normalize_path_for_display(&selected)));
}

#[test]
fn tc_050_executor_failure_notice_uses_display_path_without_execution_error_details() {
    let root = test_root("action-worker-executor-failure");
    fs::create_dir_all(&root).expect("create root");
    let selected = root.join("sub").join("..").join("selected.txt");
    let actual = root.join("selected.txt");
    fs::write(&actual, "selected").expect("write selected");
    let canonical = actual.canonicalize().expect("canonical target");
    let canonical_text = canonical.to_string_lossy().to_string();

    let response = process_action_request_with(
        ActionRequest {
            request_id: 107,
            root: root.clone(),
            paths: vec![selected.clone()],
            open_parent_for_files: false,
        },
        |_| anyhow::bail!("OS failure at {canonical_text}"),
    );

    assert!(response
        .notice
        .contains(&normalize_path_for_display(&selected)));
    assert!(!response.notice.contains(&canonical_text));
    assert!(!response.notice.contains("OS failure"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_050_worker_reports_partial_completion_when_recheck_fails() {
    let root = test_root("action-worker-partial-root");
    let first_parent = root.join("first-parent");
    let second_parent = root.join("second-parent");
    fs::create_dir_all(&first_parent).expect("create first parent");
    fs::create_dir_all(&second_parent).expect("create second parent");
    let first = first_parent.join("first.txt");
    let second = second_parent.join("second.txt");
    fs::write(&first, "first").expect("write first");
    fs::write(&second, "second").expect("write second");
    let mut calls = Vec::new();
    let second_to_remove = second.clone();

    let response = process_action_request_with(
        ActionRequest {
            request_id: 103,
            root: root.clone(),
            paths: vec![first, second],
            open_parent_for_files: true,
        },
        |path| {
            calls.push(path.to_path_buf());
            if calls.len() == 1 {
                fs::remove_file(&second_to_remove).expect("remove second before recheck");
                fs::create_dir(&second_to_remove).expect("replace second file with directory");
            }
            Ok(())
        },
    );

    assert_eq!(calls.len(), 1);
    assert!(response.notice.contains("1 of 2"));
    let _ = fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[test]
fn tc_051_symlink_targets_lexically_under_root_are_allowed() {
    use std::os::unix::fs::symlink;

    let root = test_root("action-worker-symlink-root");
    let outside_root = test_root("action-worker-symlink-outside");
    fs::create_dir_all(&root).expect("create root");
    fs::create_dir_all(&outside_root).expect("create outside root");
    let outside_file = outside_root.join("outside.txt");
    fs::write(&outside_file, "outside").expect("write outside");
    let link = root.join("outside-link.txt");
    symlink(&outside_file, &link).expect("create file symlink");
    let mut direct_calls = Vec::new();

    let direct = process_action_request_with(
        ActionRequest {
            request_id: 104,
            root: root.clone(),
            paths: vec![link.clone()],
            open_parent_for_files: false,
        },
        |path| {
            direct_calls.push(path.to_path_buf());
            Ok(())
        },
    );
    assert_eq!(
        direct_calls,
        vec![outside_file.canonicalize().expect("canonical outside file")]
    );
    assert!(!direct.notice.starts_with("Action blocked:"));

    let mut parent_calls = Vec::new();
    let parent = process_action_request_with(
        ActionRequest {
            request_id: 105,
            root: root.clone(),
            paths: vec![link],
            open_parent_for_files: true,
        },
        |path| {
            parent_calls.push(path.to_path_buf());
            Ok(())
        },
    );
    assert_eq!(
        parent_calls,
        vec![root.canonicalize().expect("canonical root")]
    );
    assert!(!parent.notice.starts_with("Action blocked:"));

    let outside_dir = outside_root.join("outside-dir");
    fs::create_dir_all(&outside_dir).expect("create outside directory");
    let dir_link = root.join("outside-dir-link");
    symlink(&outside_dir, &dir_link).expect("create directory symlink");
    let mut directory_calls = Vec::new();
    let directory_response = process_action_request_with(
        ActionRequest {
            request_id: 108,
            root: root.clone(),
            paths: vec![dir_link],
            open_parent_for_files: true,
        },
        |path| {
            directory_calls.push(path.to_path_buf());
            Ok(())
        },
    );
    assert_eq!(
        directory_calls,
        vec![outside_dir
            .canonicalize()
            .expect("canonical outside directory")]
    );
    assert!(!directory_response.notice.starts_with("Action blocked:"));

    let linked_ancestor = root.join("linked-ancestor");
    symlink(&outside_root, &linked_ancestor).expect("create linked ancestor");
    let through_link = linked_ancestor.join("outside.txt");
    let mut ancestor_calls = Vec::new();
    let ancestor_response = process_action_request_with(
        ActionRequest {
            request_id: 111,
            root: root.clone(),
            paths: vec![through_link],
            open_parent_for_files: false,
        },
        |path| {
            ancestor_calls.push(path.to_path_buf());
            Ok(())
        },
    );
    assert_eq!(
        ancestor_calls,
        vec![outside_file.canonicalize().expect("canonical linked child")]
    );
    assert!(!ancestor_response.notice.starts_with("Action blocked:"));

    let first = root.join("first.txt");
    fs::write(&first, "first").expect("write first target");
    let alternate_file = outside_root.join("alternate.txt");
    fs::write(&alternate_file, "alternate").expect("write alternate target");
    let retargeted_link = root.join("retargeted-link.txt");
    symlink(&outside_file, &retargeted_link).expect("create retargeted link");
    let retargeted_link_for_call = retargeted_link.clone();
    let mut retarget_calls = Vec::new();
    let retarget_response = process_action_request_with(
        ActionRequest {
            request_id: 112,
            root: root.clone(),
            paths: vec![first, retargeted_link],
            open_parent_for_files: false,
        },
        |path| {
            retarget_calls.push(path.to_path_buf());
            if retarget_calls.len() == 1 {
                fs::remove_file(&retargeted_link_for_call).expect("remove original link");
                symlink(&alternate_file, &retargeted_link_for_call).expect("retarget link");
            }
            Ok(())
        },
    );
    assert_eq!(retarget_calls.len(), 1);
    assert!(retarget_response.notice.contains("1 of 2"));

    let broken_link = root.join("broken-link");
    symlink(outside_root.join("missing-target"), &broken_link).expect("create broken symlink");
    let mut broken_calls = Vec::new();
    let broken_response = process_action_request_with(
        ActionRequest {
            request_id: 109,
            root: root.clone(),
            paths: vec![broken_link.clone()],
            open_parent_for_files: true,
        },
        |path| {
            broken_calls.push(path.to_path_buf());
            Ok(())
        },
    );
    assert!(broken_calls.is_empty());
    assert!(broken_response
        .notice
        .contains(&normalize_path_for_display(&broken_link)));
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&outside_root);
}

#[cfg(windows)]
#[test]
fn tc_051_windows_precheck_defers_case_and_verbatim_prefix_forms() {
    let root = Path::new(r"C:\Workspace");
    for candidate in [
        Path::new(r"c:\workspace\Bin\tool.exe"),
        Path::new(r"\\?\C:\Workspace\Bin\tool.exe"),
        Path::new(r"\Workspace\Bin\tool.exe"),
        Path::new(r"C:Workspace\Bin\tool.exe"),
    ] {
        assert_eq!(
            lexical_action_path_precheck(root, candidate),
            ActionPathPrecheck::Defer,
            "ambiguous Windows form must reach worker: {}",
            candidate.display()
        );
    }
}

#[cfg(windows)]
#[test]
fn tc_051_windows_precheck_defers_non_unicode_path_before_normalization() {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    let mut wide: Vec<u16> = r"C:\Workspace\".encode_utf16().collect();
    wide.push(0xD800);
    wide.extend("\\tool.exe".encode_utf16());
    let candidate = PathBuf::from(OsString::from_wide(&wide));

    assert_eq!(
        lexical_action_path_precheck(Path::new(r"C:\Workspace"), &candidate),
        ActionPathPrecheck::Defer
    );
}

#[cfg(windows)]
#[test]
fn tc_051_windows_worker_accepts_case_and_extended_prefix_for_same_target() {
    let root = test_root("action-worker-windows-case");
    fs::create_dir_all(&root).expect("create root");
    let selected = root.join("Tool.EXE");
    fs::write(&selected, "tool").expect("write target");
    let canonical = selected.canonicalize().expect("canonical target");
    let swapped_case = PathBuf::from(
        selected
            .to_string_lossy()
            .chars()
            .map(|character| {
                if character.is_ascii_lowercase() {
                    character.to_ascii_uppercase()
                } else if character.is_ascii_uppercase() {
                    character.to_ascii_lowercase()
                } else {
                    character
                }
            })
            .collect::<String>(),
    );
    let extended = PathBuf::from(format!(
        r"\\?\{}",
        selected.to_string_lossy().replace('/', r"\")
    ));

    for candidate in [swapped_case, extended] {
        let mut calls = Vec::new();
        let response = process_action_request_with(
            ActionRequest {
                request_id: 110,
                root: root.clone(),
                paths: vec![candidate],
                open_parent_for_files: false,
            },
            |path| {
                calls.push(path.to_path_buf());
                Ok(())
            },
        );
        assert_eq!(calls, vec![canonical.clone()]);
        assert!(!response.notice.starts_with("Action blocked:"));
    }
    let _ = fs::remove_dir_all(&root);
}

#[cfg(windows)]
#[test]
#[ignore = "manual Windows junction evidence; requires FLISTWALKER_TC051_* paths"]
fn tc_051_windows_junction_target_manual_evidence() {
    let root =
        PathBuf::from(std::env::var_os("FLISTWALKER_TC051_ROOT").expect("FLISTWALKER_TC051_ROOT"));
    let junction = PathBuf::from(
        std::env::var_os("FLISTWALKER_TC051_JUNCTION").expect("FLISTWALKER_TC051_JUNCTION"),
    );
    let outside = PathBuf::from(
        std::env::var_os("FLISTWALKER_TC051_OUTSIDE").expect("FLISTWALKER_TC051_OUTSIDE"),
    );
    let inside = root.join("inside.txt");
    let canonical_outside = outside.canonicalize().expect("canonical outside target");
    let mut calls = Vec::new();

    let canonical_root = root.canonicalize().expect("canonical junction root");
    let resolved_inside = canonical_root.join("inside.txt");
    let mut resolved_calls = Vec::new();
    let resolved_response = process_action_request_with(
        ActionRequest {
            request_id: 150,
            root: root.clone(),
            paths: vec![resolved_inside.clone()],
            open_parent_for_files: false,
        },
        |path| {
            resolved_calls.push(path.to_path_buf());
            Ok(())
        },
    );
    assert_eq!(resolved_calls, vec![resolved_inside]);
    assert!(!resolved_response.notice.starts_with("Action blocked:"));

    for open_parent_for_files in [false, true] {
        calls.clear();
        let response = process_action_request_with(
            ActionRequest {
                request_id: 151,
                root: root.clone(),
                paths: vec![inside.clone(), junction.clone()],
                open_parent_for_files,
            },
            |path| {
                calls.push(path.to_path_buf());
                Ok(())
            },
        );
        assert_eq!(calls.len(), 2, "both lexical root targets must execute");
        assert!(!response.notice.starts_with("Action blocked:"));
        assert_eq!(calls.last(), Some(&canonical_outside));
    }
}

#[test]
fn stale_action_completion_is_ignored_by_request_id() {
    let root = test_root("stale-action-request-id");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<ActionResponse>();
    app.shell.worker_bus.action.rx = rx;
    app.shell.runtime.notice = "latest notice".to_string();
    app.shell.worker_bus.action.pending_request_id = Some(2);
    app.shell.worker_bus.action.in_progress = true;
    assert!(app.shell.worker_bus.action.prepare_request(1, &root));
    assert!(app.shell.worker_bus.action.prepare_request(2, &root));
    let tab_id = app.current_tab_id().expect("tab id");
    app.bind_action_request_to_tab(1, tab_id);
    app.bind_action_request_to_tab(2, tab_id);
    tx.send(ActionResponse {
        request_id: 1,
        notice: "Action failed: stale".to_string(),
    })
    .expect("send stale action response");
    app.poll_action_response();

    assert_eq!(app.shell.runtime.notice, "latest notice");
    assert_eq!(app.shell.worker_bus.action.pending_request_id, Some(2));
    assert!(app.shell.worker_bus.action.in_progress);
    assert!(!app.shell.worker_bus.action.freshness.is_current(1, &root));
    assert!(app.shell.worker_bus.action.freshness.is_current(2, &root));

    tx.send(ActionResponse {
        request_id: 2,
        notice: "Action: latest".to_string(),
    })
    .expect("send latest action response");
    app.poll_action_response();

    assert_eq!(app.shell.runtime.notice, "Action: latest");
    assert_eq!(app.shell.worker_bus.action.pending_request_id, None);
    assert!(!app.shell.worker_bus.action.in_progress);
    assert_eq!(app.shell.worker_bus.action.freshness.active_count(), 0);
    let _ = fs::remove_dir_all(&root);
}

#[test]
#[cfg(target_os = "windows")]
fn action_notice_for_targets_normalizes_extended_prefix() {
    let notice = action_notice_for_targets(&[PathBuf::from(r"\\?\C:\Users\tester\file.txt")]);
    assert_eq!(notice, r"Action: C:\Users\tester\file.txt");
    assert!(!notice.contains(r"\\?\"));
}

#[test]
fn action_progress_label_is_shown_only_while_action_runs() {
    let root = test_root("action-progress-label");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

    assert_eq!(app.action_progress_label(), None);

    app.shell.worker_bus.action.in_progress = true;
    assert_eq!(app.action_progress_label(), Some("Opening..."));

    let _ = fs::remove_dir_all(&root);
}

#[test]
#[cfg(target_os = "windows")]
fn clipboard_text_normalizes_extended_and_unc_paths() {
    let paths = vec![
        PathBuf::from(r"\\?\C:\Users\tester\file.txt"),
        PathBuf::from(r"\\?\UNC\server\share\folder\file.txt"),
    ];
    let text = FlistWalkerApp::clipboard_paths_text(&paths);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], r"C:\Users\tester\file.txt");
    assert_eq!(lines[1], r"\\server\share\folder\file.txt");
}

#[test]
#[cfg(target_os = "windows")]
fn regression_copy_selected_paths_notice_normalizes_extended_prefix() {
    let root = test_root("copy-path-notice-normalize");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.committed_for_test_mut().results =
        vec![(PathBuf::from(r"\\?\C:\Users\tester\file.txt"), 0.0)];
    app.shell.runtime.committed_for_test_mut().current_row = Some(0);
    let ctx = egui::Context::default();

    app.copy_selected_paths(&ctx);

    // The Windows regression guard must read the live runtime notice, not the old shell field.
    assert!(app
        .shell
        .runtime
        .notice
        .contains(r"Copied path: C:\Users\tester\file.txt"));
    assert!(!app.shell.runtime.notice.contains(r"\\?\"));
    let _ = fs::remove_dir_all(&root);
}

use super::*;
use crate::app::action_authorization::{
    action_target_path_for_open_in_folder, authorize_action_targets, lexical_action_path_precheck,
    ActionPathPrecheck,
};
#[cfg(target_os = "windows")]
use crate::app::worker_support::action_notice_for_targets;
use crate::app::worker_tasks::process_action_request_with;
use crate::app::workers::spawn_action_worker;

#[test]
fn execute_selected_enqueues_action_request_without_sync_io() {
    let root = test_root("async-action-enqueue");
    fs::create_dir_all(&root).expect("create dir");
    let missing = root.join("missing-not-executed");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (action_tx_req, action_rx_req) = mpsc::channel::<ActionRequest>();
    let (_action_tx_res, action_rx_res) = mpsc::channel::<ActionResponse>();
    app.shell.worker_bus.action.tx = action_tx_req;
    app.shell.worker_bus.action.rx = action_rx_res;
    app.shell.runtime.results = vec![(missing.clone(), 0.0)];
    app.shell.runtime.current_row = Some(0);

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
fn execute_selected_for_activation_uses_open_folder_mode_when_requested() {
    let root = test_root("activation-open-folder");
    let folder = root.join("src");
    fs::create_dir_all(&folder).expect("create dir");
    let selected = folder.join("picked.txt");
    fs::write(&selected, "x").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (action_tx_req, action_rx_req) = mpsc::channel::<ActionRequest>();
    let (_action_tx_res, action_rx_res) = mpsc::channel::<ActionResponse>();
    app.shell.worker_bus.action.tx = action_tx_req;
    app.shell.worker_bus.action.rx = action_rx_res;
    app.shell.runtime.results = vec![(selected.clone(), 0.0)];
    app.shell.runtime.current_row = Some(0);

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
    let (action_tx_req, _action_rx_req) = mpsc::channel::<ActionRequest>();
    let (_action_tx_res, action_rx_res) = mpsc::channel::<ActionResponse>();
    app.shell.worker_bus.action.tx = action_tx_req;
    app.shell.worker_bus.action.rx = action_rx_res;
    app.shell.runtime.results = vec![(extended, 0.0)];
    app.shell.runtime.current_row = Some(0);

    app.execute_selected();

    assert_eq!(
        app.shell.runtime.notice,
        format!("Action: {}", selected.display())
    );
    assert!(!app.shell.runtime.notice.contains(r"\\?\"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn execute_selected_blocks_path_outside_current_root() {
    let root = test_root("action-block-outside-root");
    let outside_root = test_root("action-block-outside-root-other");
    let outside = outside_root.join("tool.exe");
    fs::create_dir_all(&root).expect("create root");
    fs::create_dir_all(outside.parent().expect("outside parent")).expect("create outside parent");
    fs::write(&outside, "x").expect("write outside file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (action_tx_req, action_rx_req) = mpsc::channel::<ActionRequest>();
    let (_action_tx_res, action_rx_res) = mpsc::channel::<ActionResponse>();
    app.shell.worker_bus.action.tx = action_tx_req;
    app.shell.worker_bus.action.rx = action_rx_res;
    app.shell.runtime.results = vec![(outside.clone(), 0.0)];
    app.shell.runtime.current_row = Some(0);

    app.execute_selected();

    assert!(
        action_rx_req.try_recv().is_err(),
        "action request must not be enqueued"
    );
    assert!(app.shell.runtime.notice.contains("outside current root"));
    assert!(app.shell.worker_bus.action.pending_request_id.is_none());
    assert!(!app.shell.worker_bus.action.in_progress);
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&outside_root);
}

#[test]
fn execute_selected_allows_unc_like_path_when_under_current_root() {
    let root = PathBuf::from(r"\\server\share\workspace");
    let child = PathBuf::from(r"\\server\share\workspace\bin\tool.exe");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (action_tx_req, action_rx_req) = mpsc::channel::<ActionRequest>();
    let (_action_tx_res, action_rx_res) = mpsc::channel::<ActionResponse>();
    app.shell.worker_bus.action.tx = action_tx_req;
    app.shell.worker_bus.action.rx = action_rx_res;
    app.shell.runtime.results = vec![(child.clone(), 0.0)];
    app.shell.runtime.current_row = Some(0);

    app.execute_selected();

    let req = action_rx_req
        .try_recv()
        .expect("UNC-like child should be enqueued");
    assert_eq!(req.paths, vec![child]);
    assert!(app.shell.worker_bus.action.pending_request_id.is_some());
    assert!(app.shell.worker_bus.action.in_progress);
}

#[test]
fn action_worker_allows_later_open_request_while_previous_request_is_blocked() {
    let root = test_root("action-worker-multiple-open");
    fs::create_dir_all(&root).expect("create dir");
    let first = root.join("block-action-target.txt");
    let second = root.join("second.txt");
    fs::write(&first, "first").expect("write first");
    fs::write(&second, "second").expect("write second");
    let shutdown = Arc::new(AtomicBool::new(false));
    let (tx, rx, handle) = spawn_action_worker(Arc::clone(&shutdown));

    tx.send(ActionRequest {
        request_id: 1,
        root: root.clone(),
        paths: vec![first.clone()],
        open_parent_for_files: false,
    })
    .expect("send first action");
    tx.send(ActionRequest {
        request_id: 2,
        root: root.clone(),
        paths: vec![second.clone()],
        open_parent_for_files: false,
    })
    .expect("send second action");

    let response = rx
        .recv_timeout(Duration::from_millis(500))
        .expect("later action should complete while first action is blocked");
    assert_eq!(response.request_id, 2);

    shutdown.store(true, Ordering::Relaxed);
    drop(tx);
    handle.join().expect("join action worker");
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
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&outside_root);
}

#[test]
fn tc_050_worker_dispatches_only_resolved_path_and_preserves_display_notice() {
    let root = test_root("action-worker-resolved-root");
    fs::create_dir_all(&root).expect("create root");
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
fn tc_051_symlink_escape_is_rejected_but_open_parent_of_file_link_is_allowed() {
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
    assert!(direct_calls.is_empty());
    assert!(direct.notice.starts_with("Action blocked:"));
    assert!(direct.notice.contains(&normalize_path_for_display(&link)));
    assert!(!direct
        .notice
        .contains(&outside_file.to_string_lossy().to_string()));

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
    assert!(directory_calls.is_empty());
    assert!(directory_response.notice.starts_with("Action blocked:"));

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
fn tc_051_windows_junction_escape_manual_evidence() {
    let root =
        PathBuf::from(std::env::var_os("FLISTWALKER_TC051_ROOT").expect("FLISTWALKER_TC051_ROOT"));
    let junction = PathBuf::from(
        std::env::var_os("FLISTWALKER_TC051_JUNCTION").expect("FLISTWALKER_TC051_JUNCTION"),
    );
    let outside = PathBuf::from(
        std::env::var_os("FLISTWALKER_TC051_OUTSIDE").expect("FLISTWALKER_TC051_OUTSIDE"),
    );
    let inside = root.join("inside.txt");
    let mut calls = Vec::new();

    for open_parent_for_files in [false, true] {
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
        assert!(
            calls.is_empty(),
            "junction escape must fail preauthorization"
        );
        assert!(response.notice.starts_with("Action blocked:"));
        assert!(
            !response
                .notice
                .contains(&outside.to_string_lossy().to_string()),
            "notice must not disclose the external canonical destination"
        );
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
    let tab_id = app.current_tab_id().expect("tab id");
    app.bind_action_request_to_tab(1, tab_id);
    app.bind_action_request_to_tab(2, tab_id);
    let active_tab = app.shell.tabs.active_tab;
    app.shell
        .tabs
        .get_mut(active_tab)
        .expect("active tab")
        .pending_action_request_id = Some(2);
    app.shell
        .tabs
        .get_mut(active_tab)
        .expect("active tab")
        .action_in_progress = true;

    tx.send(ActionResponse {
        request_id: 1,
        notice: "Action failed: stale".to_string(),
    })
    .expect("send stale action response");
    app.poll_action_response();

    assert_eq!(app.shell.runtime.notice, "latest notice");
    assert_eq!(app.shell.worker_bus.action.pending_request_id, Some(2));
    assert!(app.shell.worker_bus.action.in_progress);

    tx.send(ActionResponse {
        request_id: 2,
        notice: "Action: latest".to_string(),
    })
    .expect("send latest action response");
    app.poll_action_response();

    assert_eq!(app.shell.runtime.notice, "Action: latest");
    assert_eq!(app.shell.worker_bus.action.pending_request_id, None);
    assert!(!app.shell.worker_bus.action.in_progress);
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
    app.shell.runtime.results = vec![(PathBuf::from(r"\\?\C:\Users\tester\file.txt"), 0.0)];
    app.shell.runtime.current_row = Some(0);
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

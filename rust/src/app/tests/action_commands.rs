use super::*;
#[cfg(target_os = "windows")]
use crate::app::worker_support::action_notice_for_targets;
use crate::app::worker_support::{
    action_target_path_for_open_in_folder, action_targets_for_request,
};

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
fn action_target_path_for_open_in_folder_maps_file_and_directory() {
    let root = test_root("open-folder-target");
    let dir = root.join("dir");
    fs::create_dir_all(&dir).expect("create dir");
    let file = dir.join("main.rs");
    fs::write(&file, "fn main() {}").expect("write file");

    let from_file = action_target_path_for_open_in_folder(&file);
    let from_dir = action_target_path_for_open_in_folder(&dir);

    assert_eq!(from_file, dir);
    assert_eq!(from_dir, root.join("dir"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn action_targets_for_request_deduplicates_same_parent_directory() {
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

    let targets = action_targets_for_request(&[file_a1, file_a2, file_b, dir_a.clone()], true);

    assert_eq!(targets, vec![dir_a, dir_b]);
    let _ = fs::remove_dir_all(&root);
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

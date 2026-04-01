use super::*;

fn test_update_candidate(target_version: &str) -> UpdateCandidate {
    UpdateCandidate {
        current_version: "0.13.0".to_string(),
        target_version: target_version.to_string(),
        release_url: "https://example.invalid/release".to_string(),
        asset_name: format!("FlistWalker-{target_version}-linux-x86_64"),
        asset_url: "https://example.invalid/asset".to_string(),
        readme_asset_name: format!("FlistWalker-{target_version}-linux-x86_64.README.txt"),
        readme_asset_url: "https://example.invalid/readme".to_string(),
        license_asset_name: format!("FlistWalker-{target_version}-linux-x86_64.LICENSE.txt"),
        license_asset_url: "https://example.invalid/license".to_string(),
        notices_asset_name: format!(
            "FlistWalker-{target_version}-linux-x86_64.THIRD_PARTY_NOTICES.txt"
        ),
        notices_asset_url: "https://example.invalid/notices".to_string(),
        checksum_url: "https://example.invalid/SHA256SUMS".to_string(),
        checksum_signature_url: "https://example.invalid/SHA256SUMS.sig".to_string(),
        support: UpdateSupport::Auto,
    }
}

#[test]
fn clear_query_and_selection_clears_state() {
    let root = test_root("clear");
    fs::create_dir_all(&root).expect("create dir");
    let file = root.join("a.txt");
    fs::write(&file, "x").expect("write file");

    let mut app = FlistWalkerApp::new(root.clone(), 50, "abc".to_string());
    app.pinned_paths.insert(file.clone());
    app.current_row = Some(0);
    app.preview = "preview".to_string();

    app.clear_query_and_selection();

    assert!(app.query.is_empty());
    assert!(app.pinned_paths.is_empty());
    assert!(app.focus_query_requested);
    assert!(app.notice.contains("Cleared selection and query"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn startup_requests_query_focus() {
    let root = test_root("startup-focus");
    fs::create_dir_all(&root).expect("create dir");
    let app = FlistWalkerApp::new(root.clone(), 50, String::new());
    assert!(app.focus_query_requested);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn startup_defaults_current_row_to_first_row_regression() {
    let root = test_root("startup-default-row");
    fs::create_dir_all(&root).expect("create dir");
    let app = FlistWalkerApp::new(root.clone(), 50, String::new());

    assert_eq!(app.current_row, Some(0));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn startup_index_request_is_bound_to_active_tab() {
    let root = test_root("startup-index-tab-binding");
    fs::create_dir_all(&root).expect("create dir");
    let app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let req_id = app.indexing.pending_request_id.expect("pending index request");
    let tab_id = app.current_tab_id().expect("active tab id");
    assert_eq!(app.indexing.request_tabs.get(&req_id).copied(), Some(tab_id));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn persist_ui_state_now_saves_preview_visibility_immediately() {
    let root = test_root("persist-preview-visibility");
    let ui_state_dir = test_root("persist-preview-visibility-ui");
    let ui_state_path = ui_state_dir.join(".flistwalker_ui_state.json");
    fs::create_dir_all(&root).expect("create root");
    fs::create_dir_all(&ui_state_dir).expect("create ui state dir");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.show_preview = false;
    app.mark_ui_state_dirty();
    app.persist_ui_state_to_path_now(&ui_state_path);

    let launch = FlistWalkerApp::load_launch_settings_from_path(&ui_state_path);
    assert!(!launch.show_preview);
    assert!(!app.ui_state_dirty);

    let _ = fs::remove_file(&ui_state_path);
    let _ = fs::remove_dir_all(&ui_state_dir);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn persist_ui_state_now_saves_skipped_update_version() {
    let root = test_root("persist-skipped-update-version");
    let ui_state_dir = test_root("persist-skipped-update-version-ui");
    let ui_state_path = ui_state_dir.join(".flistwalker_ui_state.json");
    fs::create_dir_all(&root).expect("create root");
    fs::create_dir_all(&ui_state_dir).expect("create ui state dir");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.update_state.skipped_target_version = Some("0.12.4".to_string());
    app.mark_ui_state_dirty();
    app.persist_ui_state_to_path_now(&ui_state_path);

    let launch = FlistWalkerApp::load_launch_settings_from_path(&ui_state_path);
    assert_eq!(
        launch.skipped_update_target_version.as_deref(),
        Some("0.12.4")
    );

    let _ = fs::remove_file(&ui_state_path);
    let _ = fs::remove_dir_all(&ui_state_dir);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn persist_ui_state_now_saves_update_check_failure_suppression() {
    let root = test_root("persist-update-check-failure-suppression");
    let ui_state_dir = test_root("persist-update-check-failure-suppression-ui");
    let ui_state_path = ui_state_dir.join(".flistwalker_ui_state.json");
    fs::create_dir_all(&root).expect("create root");
    fs::create_dir_all(&ui_state_dir).expect("create ui state dir");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.update_state.suppress_check_failure_dialog = true;
    app.mark_ui_state_dirty();
    app.persist_ui_state_to_path_now(&ui_state_path);

    let launch = FlistWalkerApp::load_launch_settings_from_path(&ui_state_path);
    assert!(launch.suppress_update_check_failure_dialog);

    let _ = fs::remove_file(&ui_state_path);
    let _ = fs::remove_dir_all(&ui_state_dir);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn move_row_sets_scroll_tracking() {
    let root = test_root("scroll");
    fs::create_dir_all(&root).expect("create dir");
    let file1 = root.join("a.txt");
    let file2 = root.join("b.txt");
    fs::write(&file1, "x").expect("write file1");
    fs::write(&file2, "x").expect("write file2");

    let mut app = FlistWalkerApp::new(root.clone(), 50, "".to_string());
    app.results = vec![(file1, 0.0), (file2, 0.0)];
    app.current_row = Some(0);
    app.scroll_to_current = false;

    app.move_row(1);

    assert_eq!(app.current_row, Some(1));
    assert!(app.scroll_to_current);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn execute_selected_enqueues_action_request_without_sync_io() {
    let root = test_root("async-action-enqueue");
    fs::create_dir_all(&root).expect("create dir");
    let missing = root.join("missing-not-executed");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (action_tx_req, action_rx_req) = mpsc::channel::<ActionRequest>();
    let (_action_tx_res, action_rx_res) = mpsc::channel::<ActionResponse>();
    app.action_tx = action_tx_req;
    app.action_rx = action_rx_res;
    app.results = vec![(missing.clone(), 0.0)];
    app.current_row = Some(0);

    app.execute_selected();

    let req = action_rx_req
        .try_recv()
        .expect("action request should be enqueued");
    assert_eq!(req.paths, vec![missing]);
    assert!(!req.open_parent_for_files);
    assert!(app.pending_action_request_id.is_some());
    assert!(app.action_in_progress);
    assert!(!app.notice.starts_with("Action failed:"));
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
    app.action_tx = action_tx_req;
    app.action_rx = action_rx_res;
    app.results = vec![(selected.clone(), 0.0)];
    app.current_row = Some(0);

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
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (action_tx_req, _action_rx_req) = mpsc::channel::<ActionRequest>();
    let (_action_tx_res, action_rx_res) = mpsc::channel::<ActionResponse>();
    app.action_tx = action_tx_req;
    app.action_rx = action_rx_res;
    app.results = vec![(PathBuf::from(r"\\?\C:\Users\tester\file.txt"), 0.0)];
    app.current_row = Some(0);

    app.execute_selected();

    assert_eq!(app.notice, r"Action: C:\Users\tester\file.txt");
    assert!(!app.notice.contains(r"\\?\"));
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
    app.action_tx = action_tx_req;
    app.action_rx = action_rx_res;
    app.results = vec![(outside.clone(), 0.0)];
    app.current_row = Some(0);

    app.execute_selected();

    assert!(
        action_rx_req.try_recv().is_err(),
        "action request must not be enqueued"
    );
    assert!(app.notice.contains("outside current root"));
    assert!(app.pending_action_request_id.is_none());
    assert!(!app.action_in_progress);
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
    app.action_tx = action_tx_req;
    app.action_rx = action_rx_res;
    app.results = vec![(child.clone(), 0.0)];
    app.current_row = Some(0);

    app.execute_selected();

    let req = action_rx_req
        .try_recv()
        .expect("UNC-like child should be enqueued");
    assert_eq!(req.paths, vec![child]);
    assert!(app.pending_action_request_id.is_some());
    assert!(app.action_in_progress);
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
    app.action_rx = rx;
    app.notice = "latest notice".to_string();
    app.pending_action_request_id = Some(2);
    app.action_in_progress = true;
    let tab_id = app.current_tab_id().expect("tab id");
    app.action_request_tabs.insert(1, tab_id);
    app.action_request_tabs.insert(2, tab_id);
    app.tabs[app.active_tab].pending_action_request_id = Some(2);
    app.tabs[app.active_tab].action_in_progress = true;

    tx.send(ActionResponse {
        request_id: 1,
        notice: "Action failed: stale".to_string(),
    })
    .expect("send stale action response");
    app.poll_action_response();

    assert_eq!(app.notice, "latest notice");
    assert_eq!(app.pending_action_request_id, Some(2));
    assert!(app.action_in_progress);

    tx.send(ActionResponse {
        request_id: 2,
        notice: "Action: latest".to_string(),
    })
    .expect("send latest action response");
    app.poll_action_response();

    assert_eq!(app.notice, "Action: latest");
    assert_eq!(app.pending_action_request_id, None);
    assert!(!app.action_in_progress);
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
fn available_update_response_opens_prompt() {
    let root = test_root("available-update-prompt");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<UpdateResponse>();
    app.update_rx = rx;
    app.update_state.pending_request_id = Some(1);
    app.update_state.in_progress = true;

    tx.send(UpdateResponse::Available {
        request_id: 1,
        candidate: Box::new(test_update_candidate("0.12.4")),
    })
    .expect("send update response");

    app.poll_update_response();

    assert!(app.update_state.prompt.is_some());
    assert!(
        !app.update_state
            .prompt
            .as_ref()
            .expect("update prompt")
            .skip_until_next_version
    );
    assert!(
        !app.update_state
            .prompt
            .as_ref()
            .expect("update prompt")
            .install_started
    );
    assert_eq!(app.update_state.pending_request_id, None);
    assert!(!app.update_state.in_progress);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn skipped_update_response_is_not_prompted_again_until_newer_version() {
    let root = test_root("skip-update-prompt");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<UpdateResponse>();
    app.update_rx = rx;
    app.update_state.pending_request_id = Some(1);
    app.update_state.in_progress = true;
    app.update_state.skipped_target_version = Some("0.12.4".to_string());

    tx.send(UpdateResponse::Available {
        request_id: 1,
        candidate: Box::new(test_update_candidate("0.12.4")),
    })
    .expect("send update response");

    app.poll_update_response();

    assert!(app.update_state.prompt.is_none());
    assert_eq!(app.update_state.pending_request_id, None);
    assert!(!app.update_state.in_progress);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn newer_update_response_ignores_previous_skip_version() {
    let root = test_root("newer-update-after-skip");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<UpdateResponse>();
    app.update_rx = rx;
    app.update_state.pending_request_id = Some(1);
    app.update_state.in_progress = true;
    app.update_state.skipped_target_version = Some("0.12.4".to_string());

    tx.send(UpdateResponse::Available {
        request_id: 1,
        candidate: Box::new(test_update_candidate("0.12.5")),
    })
    .expect("send update response");

    app.poll_update_response();

    assert_eq!(
        app.update_state
            .prompt
            .as_ref()
            .expect("newer version should be prompted")
            .candidate
            .target_version,
        "0.12.5"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn failed_update_response_sets_notice_without_closing_app() {
    let root = test_root("failed-update-notice");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<UpdateResponse>();
    app.update_rx = rx;
    app.update_state.pending_request_id = Some(1);
    app.update_state.in_progress = true;

    tx.send(UpdateResponse::Failed {
        request_id: 1,
        error: "Update check failed: offline".to_string(),
    })
    .expect("send update failure");

    app.poll_update_response();

    assert_eq!(app.notice, "Update check failed: offline");
    assert_eq!(app.update_state.pending_request_id, None);
    assert!(!app.update_state.in_progress);
    assert!(!app.update_state.close_requested_for_install);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn update_check_failure_opens_failure_dialog() {
    let root = test_root("update-check-failure-dialog");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<UpdateResponse>();
    app.update_rx = rx;
    app.update_state.pending_request_id = Some(1);
    app.update_state.in_progress = true;
    app.notice = "Existing notice".to_string();

    tx.send(UpdateResponse::CheckFailed {
        request_id: 1,
        error: "Update check failed: offline".to_string(),
    })
    .expect("send update failure");

    app.poll_update_response();

    assert_eq!(app.notice, "Existing notice");
    assert_eq!(
        app.update_state
            .check_failure
            .as_ref()
            .expect("update check failure dialog")
            .error,
        "Update check failed: offline"
    );
    assert_eq!(app.update_state.pending_request_id, None);
    assert!(!app.update_state.in_progress);
    assert!(!app.update_state.close_requested_for_install);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn suppressed_update_check_failure_does_not_open_dialog() {
    let root = test_root("suppressed-update-check-failure-dialog");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<UpdateResponse>();
    app.update_rx = rx;
    app.update_state.pending_request_id = Some(1);
    app.update_state.in_progress = true;
    app.update_state.suppress_check_failure_dialog = true;

    tx.send(UpdateResponse::CheckFailed {
        request_id: 1,
        error: "Update check failed: offline".to_string(),
    })
    .expect("send update failure");

    app.poll_update_response();

    assert!(app.update_state.check_failure.is_none());
    assert_eq!(app.update_state.pending_request_id, None);
    assert!(!app.update_state.in_progress);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn forced_update_check_failure_bypasses_suppression_flag() {
    let _env_lock = crate::env_var_test_lock()
        .lock()
        .expect("env var test lock");
    let root = test_root("forced-update-check-failure-bypasses-suppression");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<UpdateResponse>();
    app.update_rx = rx;
    app.update_state.pending_request_id = Some(1);
    app.update_state.in_progress = true;
    app.update_state.suppress_check_failure_dialog = true;
    unsafe {
        std::env::set_var("FLISTWALKER_FORCE_UPDATE_CHECK_FAILURE", "1");
    }

    tx.send(UpdateResponse::CheckFailed {
        request_id: 1,
        error: "Update check failed: forced startup update check failure for debugging (FLISTWALKER_FORCE_UPDATE_CHECK_FAILURE)".to_string(),
    })
    .expect("send update failure");

    app.poll_update_response();

    assert!(app.update_state.check_failure.is_some());
    unsafe {
        std::env::remove_var("FLISTWALKER_FORCE_UPDATE_CHECK_FAILURE");
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn startup_update_check_is_skipped_when_self_update_is_disabled() {
    let _env_lock = crate::env_var_test_lock()
        .lock()
        .expect("env var test lock");
    let root = test_root("startup-update-check-disabled");
    fs::create_dir_all(&root).expect("create dir");
    unsafe {
        std::env::set_var("FLISTWALKER_DISABLE_SELF_UPDATE", "1");
    }

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.update_state.pending_request_id = Some(99);
    app.update_state.in_progress = true;

    app.request_startup_update_check();

    assert_eq!(app.update_state.pending_request_id, None);
    assert!(!app.update_state.in_progress);

    unsafe {
        std::env::remove_var("FLISTWALKER_DISABLE_SELF_UPDATE");
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn start_update_install_ignores_repeat_requests_after_first_click() {
    let root = test_root("start-update-install-idempotent");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<UpdateRequest>();
    app.update_tx = tx;
    app.update_state.prompt = Some(UpdatePromptState {
        candidate: test_update_candidate("0.13.1"),
        skip_until_next_version: false,
        install_started: false,
    });

    app.start_update_install();
    app.start_update_install();

    let first = rx.recv().expect("first update request");
    assert!(matches!(
        first.kind,
        UpdateRequestKind::DownloadAndApply { .. }
    ));
    assert!(rx.try_recv().is_err());
    assert!(
        app.update_state
            .prompt
            .as_ref()
            .expect("update prompt")
            .install_started
    );
    assert!(app.update_state.in_progress);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn failed_update_response_reenables_update_prompt_actions() {
    let root = test_root("failed-update-response-reenables-actions");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<UpdateResponse>();
    app.update_rx = rx;
    app.update_state.pending_request_id = Some(1);
    app.update_state.in_progress = true;
    app.update_state.prompt = Some(UpdatePromptState {
        candidate: test_update_candidate("0.13.1"),
        skip_until_next_version: false,
        install_started: true,
    });

    tx.send(UpdateResponse::Failed {
        request_id: 1,
        error: "Update failed: offline".to_string(),
    })
    .expect("send update failure");

    app.poll_update_response();

    assert!(
        !app.update_state
            .prompt
            .as_ref()
            .expect("update prompt")
            .install_started
    );
    assert_eq!(app.notice, "Update failed: offline");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn action_progress_label_is_shown_only_while_action_runs() {
    let root = test_root("action-progress-label");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

    assert_eq!(app.action_progress_label(), None);

    app.action_in_progress = true;
    assert_eq!(app.action_progress_label(), Some("Opening..."));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn prefer_relative_display_is_enabled_for_filelist_source() {
    let root = test_root("prefer-relative-filelist");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.index.source = IndexSource::FileList(root.join("FileList.txt"));

    assert!(app.prefer_relative_display());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn results_scroll_is_disabled_during_preview_resize() {
    assert!(!FlistWalkerApp::results_scroll_enabled(true));
}

#[test]
fn results_scroll_is_enabled_when_preview_resize_not_active() {
    assert!(FlistWalkerApp::results_scroll_enabled(false));
}

#[test]
fn regex_query_is_not_filtered_out_by_visible_match_guard() {
    let root = PathBuf::from("/tmp");
    let results = vec![(PathBuf::from("/tmp/src/main.py"), 42.0)];

    let out = filter_search_results(results, &root, "ma.*py", true, true, true);

    assert_eq!(out.len(), 1);
}

#[test]
fn preview_cache_is_bounded() {
    let root = test_root("preview-cache-bounded");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

    let chunk = "x".repeat(1024 * 1024);
    let count = 40usize;
    for i in 0..count {
        let path = root.join(format!("file-{i}.txt"));
        app.cache_preview(path, chunk.clone());
    }

    assert!(app.preview_cache.total_bytes <= FlistWalkerApp::PREVIEW_CACHE_MAX_BYTES);
    assert!(!app.preview_cache.order.is_empty());
    assert_eq!(
        app.preview_cache.entries.len(),
        app.preview_cache.order.len()
    );
    let evicted = root.join("file-0.txt");
    assert!(!app.preview_cache.entries.contains_key(&evicted));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn result_sort_name_can_be_applied_and_score_can_be_restored() {
    let root = test_root("result-sort-name-score");
    fs::create_dir_all(&root).expect("create dir");
    let alpha = root.join("alpha.txt");
    let beta = root.join("beta.txt");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "a".to_string());
    let base = vec![(beta.clone(), 10.0), (alpha.clone(), 9.0)];

    app.replace_results_snapshot(base.clone(), false);
    app.current_row = Some(0);
    app.set_result_sort_mode(ResultSortMode::NameAsc);

    assert_eq!(app.result_sort_mode, ResultSortMode::NameAsc);
    assert_eq!(app.current_row, Some(0));
    assert_eq!(
        app.results
            .iter()
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>(),
        vec![alpha.clone(), beta.clone()]
    );

    app.set_result_sort_mode(ResultSortMode::Score);

    assert_eq!(app.result_sort_mode, ResultSortMode::Score);
    assert_eq!(app.current_row, Some(0));
    assert_eq!(app.results, base);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn search_result_refresh_clamps_cursor_row_instead_of_following_path_regression() {
    let root = test_root("search-refresh-clamp-row");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "abc".to_string());
    app.show_preview = false;
    app.current_row = Some(100);
    app.preview = "stale".to_string();

    let results = vec![
        (root.join("first.txt"), 1.0),
        (root.join("second.txt"), 1.0),
        (root.join("third.txt"), 1.0),
    ];

    app.replace_results_snapshot(results, false);

    assert_eq!(app.current_row, Some(2));
    assert!(app.preview.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn search_result_refresh_does_not_auto_select_first_row_without_user_action_regression() {
    let root = test_root("search-refresh-keep-none");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "abc".to_string());
    app.show_preview = false;
    app.current_row = None;
    app.preview = "stale".to_string();

    let results = vec![
        (root.join("first.txt"), 1.0),
        (root.join("second.txt"), 1.0),
    ];

    app.replace_results_snapshot(results, false);

    assert_eq!(app.current_row, None);
    assert!(app.preview.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn clear_query_and_selection_restores_first_row_regression() {
    let root = test_root("clear-query-row-reset");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "abc".to_string());
    app.show_preview = false;
    app.query = "abc".to_string();
    app.current_row = Some(2);
    app.preview = "stale".to_string();
    app.entries = Arc::new(vec![
        root.join("first.txt"),
        root.join("second.txt"),
        root.join("third.txt"),
    ]);
    app.results = vec![
        (root.join("first.txt"), 1.0),
        (root.join("second.txt"), 1.0),
        (root.join("third.txt"), 1.0),
    ];

    app.clear_query_and_selection();

    assert!(app.query.is_empty());
    assert_eq!(app.current_row, Some(0));
    assert!(app.preview.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn query_edit_invalidates_result_sort_and_cancels_pending_request() {
    let root = test_root("result-sort-reset-on-query-edit");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "abc".to_string());

    app.result_sort_mode = ResultSortMode::ModifiedDesc;
    app.sort_in_progress = true;
    app.pending_sort_request_id = Some(42);

    app.mark_query_edited();

    assert_eq!(app.result_sort_mode, ResultSortMode::Score);
    assert!(!app.sort_in_progress);
    assert!(app.pending_sort_request_id.is_none());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn created_sort_places_missing_timestamps_last() {
    let root = test_root("result-sort-created-none-last");
    fs::create_dir_all(&root).expect("create dir");
    let has_created = root.join("has-created.txt");
    let missing_created = root.join("missing-created.txt");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "a".to_string());
    let base = vec![(missing_created.clone(), 10.0), (has_created.clone(), 9.0)];

    app.replace_results_snapshot(base, false);
    app.cache_sort_metadata(
        missing_created.clone(),
        SortMetadata {
            modified: None,
            created: None,
        },
    );
    app.cache_sort_metadata(
        has_created.clone(),
        SortMetadata {
            modified: None,
            created: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(5)),
        },
    );

    app.set_result_sort_mode(ResultSortMode::CreatedDesc);

    assert_eq!(
        app.results
            .iter()
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>(),
        vec![has_created, missing_created]
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn sort_metadata_cache_is_bounded() {
    let root = test_root("result-sort-cache-bounded");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

    for i in 0..(FlistWalkerApp::SORT_METADATA_CACHE_MAX + 8) {
        app.cache_sort_metadata(
            root.join(format!("entry-{i}.txt")),
            SortMetadata {
                modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(i as u64)),
                created: None,
            },
        );
    }

    assert!(app.sort_metadata_cache.entries.len() <= FlistWalkerApp::SORT_METADATA_CACHE_MAX);
    assert!(app.sort_metadata_cache.order.len() <= FlistWalkerApp::SORT_METADATA_CACHE_MAX);
    assert!(!app
        .sort_metadata_cache
        .entries
        .contains_key(&root.join("entry-0.txt")));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn search_prefix_cache_accepts_only_plain_single_token_queries() {
    assert!(!SearchPrefixCache::is_cacheable_query("ab"));
    assert!(SearchPrefixCache::is_cacheable_query("abc"));
    assert!(!SearchPrefixCache::is_cacheable_query("abc def"));
    assert!(!SearchPrefixCache::is_cacheable_query("abc|def"));
    assert!(!SearchPrefixCache::is_cacheable_query("'abc"));
    assert!(!SearchPrefixCache::is_cacheable_query("!abc"));
    assert!(!SearchPrefixCache::is_cacheable_query("^abc"));
    assert!(!SearchPrefixCache::is_cacheable_query("abc$"));
    assert!(SearchPrefixCache::is_safe_prefix_extension("abc", "abcd"));
    assert!(!SearchPrefixCache::is_safe_prefix_extension("abc", "ab"));
}

#[test]
fn search_prefix_cache_prefers_longest_prefix_and_evicts_old_entries() {
    let snapshot = SearchEntriesSnapshotKey { ptr: 1, len: 100 };
    let mut cache = SearchPrefixCache::default();
    cache.maybe_store(snapshot, "abc", vec![0, 1, 2, 3]);
    cache.maybe_store(snapshot, "abcd", vec![1, 3]);

    let candidates = cache
        .lookup_candidates(snapshot, "abcde")
        .expect("cached candidates");
    assert_eq!(candidates.as_ref(), &vec![1, 3]);

    for idx in 0..(SearchPrefixCache::MAX_ENTRIES + 4) {
        cache.maybe_store(snapshot, &format!("q{:03}", idx), vec![idx]);
    }
    assert!(cache.entries.len() <= SearchPrefixCache::MAX_ENTRIES);
    assert!(cache.total_bytes <= SearchPrefixCache::MAX_BYTES);
}

#[test]
fn search_prefix_cache_skips_oversized_match_sets() {
    let snapshot = SearchEntriesSnapshotKey {
        ptr: 2,
        len: 1_000_000,
    };
    let mut cache = SearchPrefixCache::default();
    let oversized = (0..=SearchPrefixCache::MAX_MATCHED_INDICES).collect::<Vec<_>>();

    cache.maybe_store(snapshot, "oversized", oversized);

    assert!(cache.lookup_candidates(snapshot, "oversizedx").is_none());
    assert_eq!(cache.total_bytes, 0);
}

#[test]
fn request_preview_is_skipped_when_preview_is_hidden() {
    let root = test_root("preview-hidden");
    fs::create_dir_all(&root).expect("create dir");
    let file = root.join("a.txt");
    fs::write(&file, "content").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

    app.show_preview = false;
    app.results = vec![(file.clone(), 0.0)];
    app.current_row = Some(0);
    app.entry_kinds.insert(file, EntryKind::file());
    app.preview = "stale preview".to_string();
    app.pending_preview_request_id = Some(99);
    app.preview_in_progress = true;

    app.request_preview_for_current();

    assert!(app.preview.is_empty());
    assert!(!app.preview_in_progress);
    assert!(app.pending_preview_request_id.is_none());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn request_preview_when_hidden_keeps_post_index_kind_resolution_queue() {
    let root = test_root("preview-hidden-keeps-kind-queue");
    fs::create_dir_all(&root).expect("create dir");
    let file = root.join("a.lnk");
    fs::write(&file, "shortcut").expect("write file");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

    app.show_preview = false;
    app.results = vec![(file.clone(), 0.0)];
    app.current_row = Some(0);
    app.indexing.pending_kind_paths.push_back(file.clone());
    app.indexing.pending_kind_paths_set.insert(file.clone());
    app.indexing.kind_resolution_in_progress = true;

    app.request_preview_for_current();

    assert!(app.indexing.pending_kind_paths.iter().any(|p| *p == file));
    assert!(app.indexing.pending_kind_paths_set.contains(&file));
    assert!(app.indexing.kind_resolution_in_progress);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn close_tab_invalidates_memory_cache_for_immediate_resample() {
    let root = test_root("close-tab-memory-resample");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    assert_eq!(app.tabs.len(), 2);

    let sentinel = u64::MAX;
    app.memory_usage_bytes = Some(sentinel);
    let stale = Instant::now()
        .checked_sub(Duration::from_secs(5))
        .unwrap_or_else(Instant::now);
    app.last_memory_sample = stale;

    app.close_tab_index(1);

    assert_eq!(app.tabs.len(), 1);
    assert_ne!(app.memory_usage_bytes, Some(sentinel));
    assert!(app.last_memory_sample > stale);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn inactive_tab_results_are_compacted_and_restored_on_activation() {
    let root = test_root("inactive-tab-results-compaction");
    fs::create_dir_all(&root).expect("create dir");
    let first = root.join("first.txt");
    let second = root.join("second.txt");
    fs::write(&first, "a").expect("write first");
    fs::write(&second, "b").expect("write second");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.show_preview = false;
    app.indexing.in_progress = false;
    app.indexing.pending_request_id = None;
    app.entries = Arc::new(vec![first.clone(), second.clone()]);
    app.base_results = vec![(first.clone(), 10.0), (second.clone(), 5.0)];
    app.results = app.base_results.clone();
    app.current_row = Some(1);
    app.preview = "preview".to_string();

    app.create_new_tab();

    assert_eq!(app.tabs.len(), 2);
    assert!(app.tabs[0].result_state.results_compacted);
    assert!(app.tabs[0].result_state.results.is_empty());
    assert_eq!(app.tabs[0].result_state.base_results.len(), 2);
    assert!(app.tabs[0].result_state.preview.is_empty());

    app.switch_to_tab_index(0);

    assert_eq!(app.results.len(), 2);
    assert_eq!(app.results[0].0, first);
    assert_eq!(app.results[1].0, second);
    assert_eq!(app.current_row, Some(1));
    assert!(!app.tabs[0].result_state.results_compacted);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn app_defaults_use_filelist_on() {
    let root = test_root("default-use-filelist-on");
    fs::create_dir_all(&root).expect("create dir");
    let app = FlistWalkerApp::new(root.clone(), 50, String::new());
    assert!(app.use_filelist);
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
fn copy_selected_paths_notice_normalizes_extended_prefix() {
    let root = test_root("copy-path-notice-normalize");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.results = vec![(PathBuf::from(r"\\?\C:\Users\tester\file.txt"), 0.0)];
    app.current_row = Some(0);
    let ctx = egui::Context::default();

    app.copy_selected_paths(&ctx);

    assert!(app
        .notice
        .contains(r"Copied path: C:\Users\tester\file.txt"));
    assert!(!app.notice.contains(r"\\?\"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn process_shutdown_flag_can_be_set_and_cleared() {
    clear_process_shutdown_request();
    assert!(!process_shutdown_requested());
    request_process_shutdown();
    assert!(process_shutdown_requested());
    clear_process_shutdown_request();
    assert!(!process_shutdown_requested());
}

#[test]
fn worker_runtime_join_all_with_timeout_returns_joined_when_workers_finish() {
    let shutdown = Arc::new(AtomicBool::new(false));
    let mut runtime = WorkerRuntime::new(Arc::clone(&shutdown));
    runtime.push("worker-a", thread::spawn(|| {}));
    runtime.push("worker-b", thread::spawn(|| {}));

    let summary = runtime.join_all_with_timeout(Duration::from_millis(500));

    assert_eq!(summary.total, 2);
    assert_eq!(summary.joined, 2);
    assert!(summary.pending.is_empty());
}

#[test]
fn worker_runtime_join_all_with_timeout_returns_early_on_timeout() {
    let shutdown = Arc::new(AtomicBool::new(false));
    let mut runtime = WorkerRuntime::new(Arc::clone(&shutdown));
    runtime.push(
        "slow-worker",
        thread::spawn(|| {
            thread::sleep(Duration::from_millis(200));
        }),
    );

    let summary = runtime.join_all_with_timeout(Duration::from_millis(10));

    assert_eq!(summary.total, 1);
    assert_eq!(summary.joined, 0);
    assert_eq!(summary.pending, vec!["slow-worker".to_string()]);
}

#[test]
fn regression_gui_close_uses_short_worker_join_timeout_budget() {
    assert!(FlistWalkerApp::worker_join_timeout() <= Duration::from_millis(250));
}

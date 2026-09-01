use super::*;
use crate::app::update::{UpdateAppCommand, UpdateCommand, UpdateWorkerCommand};
use crate::updater::AutoUpdateAssets;

fn test_update_candidate(target_version: &str) -> UpdateCandidate {
    UpdateCandidate {
        current_version: "0.13.0".to_string(),
        target_version: target_version.to_string(),
        release_url: "https://example.invalid/release".to_string(),
        support: UpdateSupport::Auto,
        auto_assets: Some(AutoUpdateAssets {
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
        }),
    }
}

fn update_trace_details<'a>(commands: &'a [UpdateCommand], event: &'static str) -> Option<&'a str> {
    commands.iter().find_map(|command| match command {
        UpdateCommand::App(UpdateAppCommand::AppendWindowTrace {
            event: trace_event,
            details,
        }) if *trace_event == event => Some(details.as_str()),
        _ => None,
    })
}

#[test]
fn available_update_response_opens_prompt() {
    let root = test_root("available-update-prompt");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<UpdateResponse>();
    app.shell.worker_bus.update.rx = rx;
    app.shell.features.update.state.pending_request_id = Some(1);
    app.shell.features.update.state.in_progress = true;

    tx.send(UpdateResponse::Available {
        request_id: 1,
        candidate: Box::new(test_update_candidate("0.12.4")),
    })
    .expect("send update response");

    app.poll_update_response();

    assert!(app.shell.features.update.state.prompt.is_some());
    assert!(
        !app.shell
            .features
            .update
            .state
            .prompt
            .as_ref()
            .expect("update prompt")
            .skip_until_next_version
    );
    assert!(
        !app.shell
            .features
            .update
            .state
            .prompt
            .as_ref()
            .expect("update prompt")
            .install_started
    );
    assert_eq!(app.shell.features.update.state.pending_request_id, None);
    assert!(!app.shell.features.update.state.in_progress);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn skipped_update_response_is_not_prompted_again_until_newer_version() {
    let root = test_root("skip-update-prompt");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<UpdateResponse>();
    app.shell.worker_bus.update.rx = rx;
    app.shell.features.update.state.pending_request_id = Some(1);
    app.shell.features.update.state.in_progress = true;
    app.shell.features.update.state.skipped_target_version = Some("0.12.4".to_string());

    tx.send(UpdateResponse::Available {
        request_id: 1,
        candidate: Box::new(test_update_candidate("0.12.4")),
    })
    .expect("send update response");

    app.poll_update_response();

    assert!(app.shell.features.update.state.prompt.is_none());
    assert_eq!(app.shell.features.update.state.pending_request_id, None);
    assert!(!app.shell.features.update.state.in_progress);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn newer_update_response_ignores_previous_skip_version() {
    let root = test_root("newer-update-after-skip");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<UpdateResponse>();
    app.shell.worker_bus.update.rx = rx;
    app.shell.features.update.state.pending_request_id = Some(1);
    app.shell.features.update.state.in_progress = true;
    app.shell.features.update.state.skipped_target_version = Some("0.12.4".to_string());

    tx.send(UpdateResponse::Available {
        request_id: 1,
        candidate: Box::new(test_update_candidate("0.12.5")),
    })
    .expect("send update response");

    app.poll_update_response();

    assert_eq!(
        app.shell
            .features
            .update
            .state
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
    app.shell.worker_bus.update.rx = rx;
    app.shell.features.update.state.pending_request_id = Some(1);
    app.shell.features.update.state.in_progress = true;

    tx.send(UpdateResponse::Failed {
        request_id: 1,
        error: "Update check failed: offline".to_string(),
    })
    .expect("send update failure");

    app.poll_update_response();

    assert_eq!(
        app.shell.runtime.notice,
        "Automatic update failed; manual update instructions are available"
    );
    assert_eq!(app.shell.features.update.state.pending_request_id, None);
    assert!(!app.shell.features.update.state.in_progress);
    assert!(!app.shell.features.update.state.close_requested_for_install);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn apply_started_update_response_requests_app_close() {
    let root = test_root("apply-started-update-close-request");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<UpdateResponse>();
    app.shell.worker_bus.update.rx = rx;
    app.shell.features.update.state.pending_request_id = Some(1);
    app.shell.features.update.state.in_progress = true;
    app.shell.features.update.state.prompt = Some(UpdatePromptState {
        candidate: test_update_candidate("0.13.1"),
        skip_until_next_version: false,
        install_started: true,
    });

    tx.send(UpdateResponse::ApplyStarted {
        request_id: 1,
        target_version: "0.13.1".to_string(),
    })
    .expect("send apply started");

    app.poll_update_response();

    assert!(app.shell.features.update.state.close_requested_for_install);
    assert!(app.shell.features.update.state.prompt.is_none());
    assert_eq!(
        app.shell.runtime.notice,
        "Restarting to apply update 0.13.1..."
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn update_check_failure_opens_failure_dialog() {
    let root = test_root("update-check-failure-dialog");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<UpdateResponse>();
    app.shell.worker_bus.update.rx = rx;
    app.shell.features.update.state.pending_request_id = Some(1);
    app.shell.features.update.state.in_progress = true;
    app.shell.runtime.notice = "Existing notice".to_string();

    tx.send(UpdateResponse::CheckFailed {
        request_id: 1,
        error: "Update check failed: offline".to_string(),
    })
    .expect("send update failure");

    app.poll_update_response();

    assert_eq!(app.shell.runtime.notice, "Existing notice");
    assert_eq!(
        app.shell
            .features
            .update
            .state
            .check_failure
            .as_ref()
            .expect("update check failure dialog")
            .error,
        "Update check failed: offline"
    );
    assert_eq!(app.shell.features.update.state.pending_request_id, None);
    assert!(!app.shell.features.update.state.in_progress);
    assert!(!app.shell.features.update.state.close_requested_for_install);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn suppressed_update_check_failure_does_not_open_dialog() {
    let root = test_root("suppressed-update-check-failure-dialog");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<UpdateResponse>();
    app.shell.worker_bus.update.rx = rx;
    app.shell.features.update.state.pending_request_id = Some(1);
    app.shell.features.update.state.in_progress = true;
    app.shell
        .features
        .update
        .state
        .suppress_check_failure_dialog = true;

    tx.send(UpdateResponse::CheckFailed {
        request_id: 1,
        error: "Update check failed: offline".to_string(),
    })
    .expect("send update failure");

    app.poll_update_response();

    assert!(app.shell.features.update.state.check_failure.is_none());
    assert_eq!(app.shell.features.update.state.pending_request_id, None);
    assert!(!app.shell.features.update.state.in_progress);
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
    app.shell.worker_bus.update.rx = rx;
    app.shell.features.update.state.pending_request_id = Some(1);
    app.shell.features.update.state.in_progress = true;
    app.shell
        .features
        .update
        .state
        .suppress_check_failure_dialog = true;
    unsafe {
        std::env::set_var("FLISTWALKER_FORCE_UPDATE_CHECK_FAILURE", "1");
    }

    tx.send(UpdateResponse::CheckFailed {
        request_id: 1,
        error: "Update check failed: forced startup update check failure for debugging (FLISTWALKER_FORCE_UPDATE_CHECK_FAILURE)".to_string(),
    })
    .expect("send update failure");

    app.poll_update_response();

    assert!(app.shell.features.update.state.check_failure.is_some());
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
    app.shell.features.update.state.pending_request_id = Some(99);
    app.shell.features.update.state.in_progress = true;

    app.request_startup_update_check();

    assert_eq!(app.shell.features.update.state.pending_request_id, None);
    assert!(!app.shell.features.update.state.in_progress);

    unsafe {
        std::env::remove_var("FLISTWALKER_DISABLE_SELF_UPDATE");
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn startup_update_check_emits_request_trace_command() {
    let mut manager = UpdateManager::default();
    let commands = manager.request_startup_check_commands(false);

    assert!(commands.iter().any(|command| matches!(
        command,
        UpdateCommand::Worker(UpdateWorkerCommand::Start(_))
    )));
    assert!(commands.iter().any(|command| matches!(
        command,
        UpdateCommand::App(UpdateAppCommand::AppendWindowTrace {
            event: "update_check_requested",
            ..
        })
    )));
}

#[test]
fn update_up_to_date_response_emits_trace_command() {
    let mut manager = UpdateManager::default();
    manager.state.pending_request_id = Some(7);

    let commands = manager.handle_response_commands(UpdateResponse::UpToDate { request_id: 7 });

    let details = update_trace_details(&commands, "update_up_to_date").expect("up to date trace");
    assert!(details.contains("request_id=7"));
}

#[test]
fn update_check_failed_response_emits_trace_command() {
    let mut manager = UpdateManager::default();
    manager.state.pending_request_id = Some(8);

    let commands = manager.handle_response_commands(UpdateResponse::CheckFailed {
        request_id: 8,
        error: "Update check failed: offline".to_string(),
    });

    let details =
        update_trace_details(&commands, "update_check_failed").expect("check failed trace");
    assert!(details.contains("request_id=8"));
    assert!(details.contains("Update check failed: offline"));
}

#[test]
fn update_available_response_emits_trace_command() {
    let mut manager = UpdateManager::default();
    manager.state.pending_request_id = Some(9);

    let commands = manager.handle_response_commands(UpdateResponse::Available {
        request_id: 9,
        candidate: Box::new(test_update_candidate("0.12.4")),
    });

    let details = update_trace_details(&commands, "update_available").expect("available trace");
    assert!(details.contains("request_id=9"));
    assert!(details.contains("target_version=0.12.4"));
}

#[test]
fn start_update_install_ignores_repeat_requests_after_first_click() {
    let root = test_root("start-update-install-idempotent");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<UpdateRequest>();
    app.shell.worker_bus.update.tx = tx;
    app.shell.features.update.state.prompt = Some(UpdatePromptState {
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
        app.shell
            .features
            .update
            .state
            .prompt
            .as_ref()
            .expect("update prompt")
            .install_started
    );
    assert!(app.shell.features.update.state.in_progress);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn start_update_install_emits_trace_command() {
    let mut manager = UpdateManager::default();
    manager.state.prompt = Some(UpdatePromptState {
        candidate: test_update_candidate("0.13.1"),
        skip_until_next_version: false,
        install_started: false,
    });

    let commands = manager
        .start_install_commands(PathBuf::from("flistwalker"))
        .expect("install commands");

    assert!(commands.iter().any(|command| matches!(
        command,
        UpdateCommand::App(UpdateAppCommand::AppendWindowTrace {
            event: "update_install_requested",
            details,
        }) if details.contains("request_id=") && details.contains("target_version=0.13.1")
    )));
}

#[test]
fn tc188_close_deferral_cancels_the_active_update_control() {
    let mut manager = UpdateManager::default();
    let (_request_id, control) = manager.begin_request();

    assert!(manager.defer_close_and_cancel());
    assert!(control.cancel_requested());
    assert!(manager.state.close_after_update_terminal);
}

#[test]
fn tc188_canceled_update_response_releases_deferred_close() {
    let mut manager = UpdateManager::default();
    let (request_id, _control) = manager.begin_request();
    assert!(manager.defer_close_and_cancel());

    let commands = manager.handle_response_commands(UpdateResponse::Canceled { request_id });

    assert!(commands.iter().any(|command| matches!(
        command,
        UpdateCommand::App(UpdateAppCommand::RequestViewportClose)
    )));
    assert!(manager.state.close_requested_for_install);
    assert!(!manager.state.in_progress);
    assert!(!manager.state.close_after_update_terminal);
}

#[test]
fn tc188_update_worker_disconnect_releases_deferred_close() {
    let mut manager = UpdateManager::default();
    let (_request_id, _control) = manager.begin_request();
    assert!(manager.defer_close_and_cancel());

    let commands = manager.worker_disconnected_commands();

    assert!(commands.iter().any(|command| matches!(
        command,
        UpdateCommand::App(UpdateAppCommand::RequestViewportClose)
    )));
    assert!(!manager.state.in_progress);
    assert!(!manager.state.close_after_update_terminal);
}

#[test]
fn tc189_previous_failure_is_not_suppressed_with_startup_check_failures() {
    let root = test_root("previous-update-failure-separation");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.set_previous_update_failure("helper restart failed".to_string());
    app.shell
        .features
        .update
        .state
        .suppress_check_failure_dialog = true;

    assert_eq!(
        app.shell
            .features
            .update
            .state
            .previous_update_failure
            .as_deref(),
        Some("helper restart failed")
    );
    app.dismiss_previous_update_failure();
    assert!(app
        .shell
        .features
        .update
        .state
        .previous_update_failure
        .is_none());
    let _ = fs::remove_dir_all(root);
}

#[test]
#[cfg(target_os = "windows")]
fn tc202_regression_previous_failure_state_hides_verbatim_paths() {
    let root = test_root("previous-update-failure-verbatim-display");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

    app.set_previous_update_failure(
        r"restart failed for \\?\C:\tools\flistwalker.exe via \\?\UNC\server\share\helper.exe"
            .to_string(),
    );

    assert_eq!(
        app.shell
            .features
            .update
            .state
            .previous_update_failure
            .as_deref(),
        Some(r"restart failed for C:\tools\flistwalker.exe via \\server\share\helper.exe")
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn update_apply_started_response_emits_trace_command() {
    let mut manager = UpdateManager::default();
    manager.state.pending_request_id = Some(10);

    let commands = manager.handle_response_commands(UpdateResponse::ApplyStarted {
        request_id: 10,
        target_version: "0.13.1".to_string(),
    });

    let details =
        update_trace_details(&commands, "update_apply_started").expect("apply started trace");
    assert!(details.contains("request_id=10"));
    assert!(details.contains("target_version=0.13.1"));
}

#[test]
fn update_failed_response_emits_trace_command() {
    let mut manager = UpdateManager::default();
    manager.state.pending_request_id = Some(11);

    let commands = manager.handle_response_commands(UpdateResponse::Failed {
        request_id: 11,
        error: "Update failed: offline".to_string(),
    });

    let details = update_trace_details(&commands, "update_failed").expect("failed trace");
    assert!(details.contains("request_id=11"));
    assert!(details.contains("Update failed: offline"));
}

#[test]
#[cfg(target_os = "windows")]
fn tc202_regression_update_failure_response_hides_verbatim_paths() {
    let mut manager = UpdateManager::default();
    manager.state.pending_request_id = Some(12);

    manager.handle_response_commands(UpdateResponse::Failed {
        request_id: 12,
        error: r"Update failed: \\?\C:\tools\flistwalker.exe; backup \\?\UNC\server\share\old.exe"
            .to_string(),
    });

    let failure = manager
        .state
        .install_failure
        .as_ref()
        .expect("update failure state");
    assert_eq!(
        failure.error,
        r"Update failed: C:\tools\flistwalker.exe; backup \\server\share\old.exe"
    );
}

#[test]
fn failed_update_response_replaces_prompt_with_manual_recovery_state() {
    let root = test_root("failed-update-response-reenables-actions");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel::<UpdateResponse>();
    app.shell.worker_bus.update.rx = rx;
    app.shell.features.update.state.pending_request_id = Some(1);
    app.shell.features.update.state.in_progress = true;
    app.shell.features.update.state.prompt = Some(UpdatePromptState {
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

    assert!(app.shell.features.update.state.prompt.is_none());
    let failure = app
        .shell
        .features
        .update
        .state
        .install_failure
        .as_ref()
        .expect("update installation failure");
    assert_eq!(failure.error, "Update failed: offline");
    assert_eq!(
        failure
            .candidate
            .as_ref()
            .expect("failed candidate")
            .release_url,
        "https://example.invalid/release"
    );
    assert_eq!(
        app.shell.runtime.notice,
        "Automatic update failed; manual update instructions are available"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn dismissing_update_install_failure_removes_manual_recovery_state() {
    let mut manager = UpdateManager::default();
    manager.state.install_failure = Some(crate::app::state::UpdateInstallFailureState {
        candidate: None,
        error: "invalid checksum manifest".to_string(),
    });

    manager.dismiss_install_failure();

    assert!(manager.state.install_failure.is_none());
}

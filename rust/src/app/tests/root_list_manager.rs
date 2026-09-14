use super::*;
use crate::app::worker::protocol::{RootValidationIntent, RootValidationResponse, ValidatedRoot};
use crate::app::{PendingSettingsCommit, PendingSettingsOperation};
use std::sync::mpsc;

fn settle_root_validation(app: &mut FlistWalkerApp) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while app.shell.worker_bus.root_validation.in_progress {
        app.poll_root_validation_response();
        assert!(
            std::time::Instant::now() < deadline,
            "root validation worker did not settle"
        );
        std::thread::yield_now();
    }
}

fn settle_settings_commit(app: &mut FlistWalkerApp) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while app.settings_commit_in_progress() {
        app.poll_settings_commit_response();
        assert!(
            std::time::Instant::now() < deadline,
            "settings persistence worker did not settle"
        );
        std::thread::yield_now();
    }
}

#[test]
fn tc_167_saved_root_failure_keeps_live_and_draft_state_for_retry() {
    let root = test_root("saved-root-failure-state");
    let live = root.join("live");
    let draft = root.join("draft");
    fs::create_dir_all(&live).expect("create live root");
    fs::create_dir_all(&draft).expect("create draft root");
    let mut app = FlistWalkerApp::new(live.clone(), 50, String::new());
    app.shell.features.root_browser.saved_roots = vec![live.clone()];
    app.open_manage_root_list();
    app.shell.features.root_browser.manage_list.draft_roots = vec![draft.clone()];

    let (response_tx, response_rx) = mpsc::channel();
    app.shell.features.root_browser.pending_settings_commit = Some(PendingSettingsCommit {
        request_id: 41,
        response: response_rx,
        operation: PendingSettingsOperation::RootList {
            roots: vec![draft.clone()],
            default_root: None,
            close_on_success: true,
        },
    });
    response_tx
        .send(crate::app::session::SettingsCommitResponse {
            request_id: 41,
            result: Err("permission denied".to_string()),
        })
        .expect("send failure response");

    app.poll_settings_commit_response();

    assert_eq!(app.shell.features.root_browser.saved_roots, vec![live]);
    assert_eq!(
        app.shell.features.root_browser.manage_list.draft_roots,
        vec![draft]
    );
    assert!(app.shell.features.root_browser.manage_list.open);
    assert!(app
        .shell
        .features
        .root_browser
        .manage_list
        .notice
        .contains("permission denied"));
    assert!(!app.settings_commit_in_progress());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn tc_167_pending_root_commit_rejects_draft_mutation_until_apply_or_ok_settles() {
    for close_on_success in [false, true] {
        let root = test_root(if close_on_success {
            "pending-root-commit-ok"
        } else {
            "pending-root-commit-apply"
        });
        let saved = root.join("saved");
        let attempted = root.join("attempted");
        fs::create_dir_all(&saved).expect("create saved root");
        fs::create_dir_all(&attempted).expect("create attempted root");
        let mut app = FlistWalkerApp::new(saved.clone(), 50, String::new());
        app.shell.features.root_browser.saved_roots = vec![saved.clone()];
        app.open_manage_root_list();

        let committed_snapshot = vec![saved.clone()];
        let (response_tx, response_rx) = mpsc::channel();
        app.shell.features.root_browser.pending_settings_commit = Some(PendingSettingsCommit {
            request_id: 43,
            response: response_rx,
            operation: PendingSettingsOperation::RootList {
                roots: committed_snapshot.clone(),
                default_root: None,
                close_on_success,
            },
        });

        app.shell.features.root_browser.manage_list.input_path =
            attempted.to_string_lossy().to_string();
        app.add_manage_root_list_input();
        assert!(!app.select_manage_root_list_item(0));
        app.enter_manage_root_list_remove_mode();
        assert_eq!(
            app.shell.features.root_browser.manage_list.draft_roots,
            committed_snapshot
        );
        assert!(!app.shell.worker_bus.root_validation.in_progress);
        assert_eq!(
            app.shell.features.root_browser.manage_list.notice,
            "Wait for settings save to finish"
        );

        response_tx
            .send(crate::app::session::SettingsCommitResponse {
                request_id: 43,
                result: Ok(crate::app::session::SettingsCommitReceipt {
                    canonical_default_root: None,
                }),
            })
            .expect("send success response");
        app.poll_settings_commit_response();

        assert_eq!(
            app.shell.features.root_browser.saved_roots,
            committed_snapshot
        );
        assert_eq!(
            app.shell.features.root_browser.manage_list.open,
            !close_on_success
        );
        if !close_on_success {
            assert_eq!(
                app.shell.features.root_browser.manage_list.draft_roots,
                committed_snapshot
            );
        }
        let _ = fs::remove_dir_all(root);
    }
}

#[test]
fn tc_168_ui_state_autosave_waits_for_observed_settings_commit() {
    let root = test_root("settings-autosave-order");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (response_tx, response_rx) = mpsc::channel();
    app.shell.features.root_browser.pending_settings_commit = Some(PendingSettingsCommit {
        request_id: 42,
        response: response_rx,
        operation: PendingSettingsOperation::DefaultRoot,
    });
    app.shell.ui.ui_state_dirty = true;

    app.maybe_save_ui_state(true);
    assert!(app.shell.ui.ui_state_dirty);

    app.shell.ui.ui_state_dirty = false;
    app.persist_ui_state_now();
    assert!(app.shell.ui.ui_state_dirty);

    response_tx
        .send(crate::app::session::SettingsCommitResponse {
            request_id: 42,
            result: Err("write failed".to_string()),
        })
        .expect("send completion");
    app.poll_settings_commit_response();
    assert!(!app.shell.ui.ui_state_dirty);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn tc192_stale_root_validation_response_cannot_mutate_reopened_dialog() {
    let root = test_root("root-validation-stale-generation");
    fs::create_dir_all(&root).expect("root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.open_manage_root_list();
    let stale_generation = app
        .shell
        .features
        .root_browser
        .manage_list
        .dialog_generation;
    app.cancel_manage_root_list();
    app.open_manage_root_list();
    let current_generation = app
        .shell
        .features
        .root_browser
        .manage_list
        .dialog_generation;
    let (tx, rx) = mpsc::channel();
    app.shell.worker_bus.root_validation.rx = rx;
    app.shell.worker_bus.root_validation.pending_request_id = Some(42);
    app.shell.worker_bus.root_validation.in_progress = true;
    app.shell
        .features
        .root_browser
        .manage_list
        .pending_validation_intent = Some(RootValidationIntent::Add);
    let initial_drafts = app
        .shell
        .features
        .root_browser
        .manage_list
        .draft_roots
        .clone();

    tx.send(RootValidationResponse {
        request_id: 42,
        dialog_generation: stale_generation,
        intent: RootValidationIntent::Add,
        result: Ok(ValidatedRoot {
            path: root.join("stale"),
            key: "stale".to_string(),
        }),
    })
    .expect("stale response");
    app.poll_root_validation_response();
    assert_eq!(
        app.shell.features.root_browser.manage_list.draft_roots,
        initial_drafts
    );
    assert!(app.shell.worker_bus.root_validation.in_progress);

    tx.send(RootValidationResponse {
        request_id: 42,
        dialog_generation: current_generation,
        intent: RootValidationIntent::Add,
        result: Ok(ValidatedRoot {
            path: root.clone(),
            key: crate::path_utils::path_key(&normalize_windows_path_buf(root.clone())),
        }),
    })
    .expect("current response");
    app.poll_root_validation_response();
    let mut expected_drafts = initial_drafts;
    expected_drafts.push(normalize_windows_path_buf(root.clone()));
    expected_drafts.sort_by_key(|path| path.to_string_lossy().to_ascii_lowercase());
    assert_eq!(
        app.shell.features.root_browser.manage_list.draft_roots,
        expected_drafts
    );
    assert!(!app.shell.worker_bus.root_validation.in_progress);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn tc192_root_validation_disconnect_clears_pending_state() {
    let root = test_root("root-validation-disconnect");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.open_manage_root_list();
    app.shell.worker_bus.root_validation.in_progress = true;
    app.shell.worker_bus.root_validation.pending_request_id = Some(42);
    app.shell
        .features
        .root_browser
        .manage_list
        .pending_validation_intent = Some(RootValidationIntent::Add);
    let (tx, rx) = mpsc::channel::<RootValidationResponse>();
    drop(tx);
    app.shell.worker_bus.root_validation.rx = rx;

    app.poll_root_validation_response();

    assert!(!app.shell.worker_bus.root_validation.in_progress);
    assert!(app
        .shell
        .features
        .root_browser
        .manage_list
        .pending_validation_intent
        .is_none());
    assert!(app
        .shell
        .features
        .root_browser
        .manage_list
        .add_error
        .contains("disconnected"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn tc192_input_change_invalidates_pending_root_validation() {
    let root = test_root("root-validation-input-change");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.open_manage_root_list();
    let initial_drafts = app
        .shell
        .features
        .root_browser
        .manage_list
        .draft_roots
        .clone();
    let generation = app
        .shell
        .features
        .root_browser
        .manage_list
        .dialog_generation;
    let (tx, rx) = mpsc::channel();
    app.shell.worker_bus.root_validation.rx = rx;
    app.shell.worker_bus.root_validation.pending_request_id = Some(42);
    app.shell.worker_bus.root_validation.in_progress = true;
    app.shell
        .features
        .root_browser
        .manage_list
        .pending_validation_intent = Some(RootValidationIntent::Add);

    app.clear_manage_root_list_add_error();
    tx.send(RootValidationResponse {
        request_id: 42,
        dialog_generation: generation,
        intent: RootValidationIntent::Add,
        result: Ok(ValidatedRoot {
            path: root.clone(),
            key: crate::path_utils::path_key(&normalize_windows_path_buf(root.clone())),
        }),
    })
    .expect("stale response");
    app.poll_root_validation_response();

    assert_eq!(
        app.shell.features.root_browser.manage_list.draft_roots,
        initial_drafts
    );
    assert!(!app.shell.worker_bus.root_validation.in_progress);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn tc192_apply_and_ok_wait_for_pending_root_validation() {
    let settings = test_settings_scope("root-validation-apply-pending-settings");
    let root = test_root("root-validation-apply-pending");
    let initial = root.join("initial");
    let draft = root.join("draft");
    fs::create_dir_all(&initial).expect("initial");
    fs::create_dir_all(&draft).expect("draft");
    let mut app = settings.app(initial.clone(), 50, String::new());
    app.shell.features.root_browser.saved_roots = vec![initial.clone()];
    app.open_manage_root_list();
    app.shell.features.root_browser.manage_list.draft_roots = vec![draft];
    app.shell.worker_bus.root_validation.in_progress = true;
    app.shell.worker_bus.root_validation.pending_request_id = Some(42);
    app.shell
        .features
        .root_browser
        .manage_list
        .pending_validation_intent = Some(RootValidationIntent::Add);

    app.apply_manage_root_list_changes();
    app.confirm_manage_root_list_changes();

    assert_eq!(app.shell.features.root_browser.saved_roots, vec![initial]);
    assert!(app.shell.features.root_browser.manage_list.open);
    assert_eq!(
        app.shell.features.root_browser.manage_list.notice,
        "Wait for folder validation to finish"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn manage_root_list_uses_stable_native_viewport_contract() {
    let parent_rect =
        egui::Rect::from_min_size(egui::pos2(-1200.0, 80.0), egui::vec2(1000.0, 700.0));
    let builder = FlistWalkerApp::manage_root_list_viewport_builder(Some(parent_rect));

    assert_eq!(
        builder.title.as_deref(),
        Some(FlistWalkerApp::MANAGE_ROOT_LIST_VIEWPORT_TITLE)
    );
    assert_eq!(
        builder.inner_size,
        Some(FlistWalkerApp::MANAGE_ROOT_LIST_VIEWPORT_SIZE)
    );
    assert_eq!(builder.position, Some(egui::pos2(-1060.0, 200.0)));
    assert_eq!(
        FlistWalkerApp::manage_root_list_viewport_id(),
        egui::ViewportId::from_hash_of("flistwalker-manage-root-list")
    );
}

#[test]
fn manage_root_list_uses_os_position_when_parent_geometry_is_unavailable() {
    let builder = FlistWalkerApp::manage_root_list_viewport_builder(None);

    assert_eq!(builder.position, None);
}

#[test]
fn manage_root_list_cancel_discards_draft_changes() {
    let settings = test_settings_scope("manage-root-list-cancel-settings");
    let root = test_root("manage-root-list-cancel");
    let saved = root.join("saved");
    let added = root.join("added");
    fs::create_dir_all(&saved).expect("create saved");
    fs::create_dir_all(&added).expect("create added");
    let mut app = settings.app(saved.clone(), 50, String::new());
    app.shell.features.root_browser.saved_roots = vec![saved.clone()];

    app.open_manage_root_list();
    app.shell.features.root_browser.manage_list.input_path = added.to_string_lossy().to_string();
    app.add_manage_root_list_input();
    settle_root_validation(&mut app);
    app.cancel_manage_root_list();

    assert_eq!(app.shell.features.root_browser.saved_roots, vec![saved]);
    assert!(!app.shell.features.root_browser.manage_list.open);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn manage_root_list_apply_commits_added_and_removed_roots() {
    let settings = test_settings_scope("manage-root-list-apply-settings");
    let root = test_root("manage-root-list-apply");
    let saved = root.join("saved");
    let removed = root.join("removed");
    let added = root.join("added");
    fs::create_dir_all(&saved).expect("create saved");
    fs::create_dir_all(&removed).expect("create removed");
    fs::create_dir_all(&added).expect("create added");
    let added_canonical =
        normalize_windows_path_buf(added.canonicalize().unwrap_or_else(|_| added.clone()));
    let mut app = settings.app(saved.clone(), 50, String::new());
    app.shell.features.root_browser.saved_roots = vec![removed.clone(), saved.clone()];

    app.open_manage_root_list();
    app.shell
        .features
        .root_browser
        .manage_list
        .selected_indices
        .insert(0);
    app.remove_selected_manage_root_list_items();
    app.shell.features.root_browser.manage_list.input_path = added.to_string_lossy().to_string();
    app.add_manage_root_list_input();
    settle_root_validation(&mut app);
    app.apply_manage_root_list_changes();
    settle_settings_commit(&mut app);

    let saved_roots = &app.shell.features.root_browser.saved_roots;
    assert_eq!(saved_roots.len(), 2);
    assert!(saved_roots
        .iter()
        .any(|path| path_key(path) == path_key(&saved)));
    assert!(saved_roots
        .iter()
        .any(|path| path_key(path) == path_key(&added_canonical)));
    assert!(!saved_roots
        .iter()
        .any(|path| path_key(path) == path_key(&removed)));
    assert!(app.shell.features.root_browser.manage_list.open);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn manage_root_list_ok_applies_and_closes() {
    let settings = test_settings_scope("manage-root-list-ok-settings");
    let root = test_root("manage-root-list-ok");
    let saved = root.join("saved");
    let added = root.join("added");
    fs::create_dir_all(&saved).expect("create saved");
    fs::create_dir_all(&added).expect("create added");
    let added_canonical =
        normalize_windows_path_buf(added.canonicalize().unwrap_or_else(|_| added.clone()));
    let mut app = settings.app(saved.clone(), 50, String::new());
    app.shell.features.root_browser.saved_roots = vec![saved];

    app.open_manage_root_list();
    app.shell.features.root_browser.manage_list.input_path = added.to_string_lossy().to_string();
    app.add_manage_root_list_input();
    settle_root_validation(&mut app);
    app.confirm_manage_root_list_changes();
    settle_settings_commit(&mut app);

    assert!(app
        .shell
        .features
        .root_browser
        .saved_roots
        .iter()
        .any(|path| path_key(path) == path_key(&added_canonical)));
    assert!(!app.shell.features.root_browser.manage_list.open);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn manage_root_list_removing_default_root_clears_default_on_apply() {
    let settings = test_settings_scope("manage-root-list-default-settings");
    let root = test_root("manage-root-list-default");
    let saved = root.join("saved");
    let kept = root.join("kept");
    fs::create_dir_all(&saved).expect("create saved");
    fs::create_dir_all(&kept).expect("create kept");
    let mut app = settings.app(saved.clone(), 50, String::new());
    app.shell.features.root_browser.saved_roots = vec![saved.clone(), kept];
    app.shell.features.root_browser.default_root = Some(saved.clone());

    app.open_manage_root_list();
    app.shell
        .features
        .root_browser
        .manage_list
        .selected_indices
        .insert(0);
    app.remove_selected_manage_root_list_items();
    app.apply_manage_root_list_changes();
    settle_settings_commit(&mut app);

    assert!(app.shell.features.root_browser.default_root.is_none());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn manage_root_list_edit_replaces_selected_draft_root_only() {
    let settings = test_settings_scope("manage-root-list-edit-settings");
    let root = test_root("manage-root-list-edit");
    let original = root.join("original");
    let replacement = root.join("replacement");
    fs::create_dir_all(&original).expect("create original");
    fs::create_dir_all(&replacement).expect("create replacement");
    let replacement_canonical =
        normalize_windows_path_buf(replacement.canonicalize().unwrap_or(replacement.clone()));
    let mut app = settings.app(original.clone(), 50, String::new());
    app.shell.features.root_browser.saved_roots = vec![original.clone()];

    app.open_manage_root_list();
    app.select_manage_root_list_item(0);
    app.start_editing_manage_root_list_item();
    app.shell.features.root_browser.manage_list.edit_path =
        replacement.to_string_lossy().to_string();
    app.save_manage_root_list_edit();
    settle_root_validation(&mut app);

    assert_eq!(
        app.shell.features.root_browser.manage_list.draft_roots,
        vec![replacement_canonical]
    );
    assert_eq!(app.shell.features.root_browser.saved_roots, vec![original]);
    assert!(app
        .shell
        .features
        .root_browser
        .manage_list
        .editing_index
        .is_none());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn manage_root_list_edit_requests_focus_and_select_all() {
    let settings = test_settings_scope("manage-root-list-edit-focus-settings");
    let root = test_root("manage-root-list-edit-focus");
    let original = root.join("original");
    fs::create_dir_all(&original).expect("create original");
    let mut app = settings.app(original.clone(), 50, String::new());
    app.shell.features.root_browser.saved_roots = vec![original];

    app.open_manage_root_list();
    app.select_manage_root_list_item(0);
    app.start_editing_manage_root_list_item();

    let manage = &app.shell.features.root_browser.manage_list;
    assert!(manage.edit_focus_requested);
    assert!(manage.edit_select_all_requested);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn manage_root_list_selecting_another_root_cancels_clean_edit() {
    let settings = test_settings_scope("manage-root-list-switch-clean-edit-settings");
    let root = test_root("manage-root-list-switch-clean-edit");
    let first = root.join("first");
    let second = root.join("second");
    fs::create_dir_all(&first).expect("create first");
    fs::create_dir_all(&second).expect("create second");
    let mut app = settings.app(first.clone(), 50, String::new());
    app.shell.features.root_browser.saved_roots = vec![first, second];

    app.open_manage_root_list();
    app.select_manage_root_list_item(0);
    app.start_editing_manage_root_list_item();

    assert!(app.select_manage_root_list_item(1));

    let manage = &app.shell.features.root_browser.manage_list;
    assert_eq!(manage.selected_index, Some(1));
    assert!(manage.editing_index.is_none());
    assert!(manage.edit_path.is_empty());
    assert!(manage.notice.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn manage_root_list_switching_to_another_root_can_start_editing_it() {
    let settings = test_settings_scope("manage-root-list-switch-edit-settings");
    let root = test_root("manage-root-list-switch-edit");
    let first = root.join("first");
    let second = root.join("second");
    fs::create_dir_all(&first).expect("create first");
    fs::create_dir_all(&second).expect("create second");
    let mut app = settings.app(first.clone(), 50, String::new());
    app.shell.features.root_browser.saved_roots = vec![first, second.clone()];

    app.open_manage_root_list();
    app.select_manage_root_list_item(0);
    app.start_editing_manage_root_list_item();

    assert!(app.select_manage_root_list_item(1));
    app.start_editing_manage_root_list_item();

    let manage = &app.shell.features.root_browser.manage_list;
    assert_eq!(manage.selected_index, Some(1));
    assert_eq!(manage.editing_index, Some(1));
    assert_eq!(manage.edit_path, second.to_string_lossy());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn manage_root_list_does_not_switch_away_from_dirty_edit() {
    let settings = test_settings_scope("manage-root-list-switch-dirty-edit-settings");
    let root = test_root("manage-root-list-switch-dirty-edit");
    let first = root.join("first");
    let second = root.join("second");
    fs::create_dir_all(&first).expect("create first");
    fs::create_dir_all(&second).expect("create second");
    let mut app = settings.app(first.clone(), 50, String::new());
    app.shell.features.root_browser.saved_roots = vec![first, second];

    app.open_manage_root_list();
    app.select_manage_root_list_item(0);
    app.start_editing_manage_root_list_item();
    app.shell.features.root_browser.manage_list.edit_path = "unsaved change".to_string();

    assert!(!app.select_manage_root_list_item(1));

    let manage = &app.shell.features.root_browser.manage_list;
    assert_eq!(manage.selected_index, Some(0));
    assert_eq!(manage.editing_index, Some(0));
    assert_eq!(manage.edit_path, "unsaved change");
    assert_eq!(
        manage.notice,
        "Save or Cancel the current edit before selecting another root"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn manage_root_list_edit_rejects_duplicate_and_keeps_editor_open() {
    let settings = test_settings_scope("manage-root-list-edit-duplicate-settings");
    let root = test_root("manage-root-list-edit-duplicate");
    let first = root.join("first");
    let second = root.join("second");
    fs::create_dir_all(&first).expect("create first");
    fs::create_dir_all(&second).expect("create second");
    let mut app = settings.app(first.clone(), 50, String::new());
    app.shell.features.root_browser.saved_roots = vec![first.clone(), second.clone()];

    app.open_manage_root_list();
    app.select_manage_root_list_item(0);
    app.start_editing_manage_root_list_item();
    app.shell.features.root_browser.manage_list.edit_path = second.to_string_lossy().to_string();
    app.save_manage_root_list_edit();
    settle_root_validation(&mut app);

    assert_eq!(
        app.shell.features.root_browser.manage_list.draft_roots,
        vec![first, second]
    );
    assert_eq!(
        app.shell.features.root_browser.manage_list.editing_index,
        Some(0)
    );
    assert_eq!(
        app.shell.features.root_browser.manage_list.edit_error,
        "Couldn't update the root. This folder is already in the list."
    );
    assert!(
        app.shell
            .features
            .root_browser
            .manage_list
            .edit_focus_requested
    );
    assert!(
        app.shell
            .features
            .root_browser
            .manage_list
            .edit_select_all_requested
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn manage_root_list_add_invalid_path_uses_field_error_and_refocuses_input() {
    let settings = test_settings_scope("manage-root-list-add-invalid-settings");
    let root = test_root("manage-root-list-add-invalid");
    fs::create_dir_all(&root).expect("create root");
    let invalid = root.join("missing");
    let mut app = settings.app(root.clone(), 50, String::new());

    app.open_manage_root_list();
    app.shell.features.root_browser.manage_list.input_path = invalid.to_string_lossy().to_string();
    app.add_manage_root_list_input();
    settle_root_validation(&mut app);

    let manage = &app.shell.features.root_browser.manage_list;
    assert_eq!(
        manage.add_error,
        format!(
            "Couldn't add the root. Folder not found: {}",
            invalid.display()
        )
    );
    assert!(manage.add_focus_requested);
    assert!(manage.add_select_all_requested);
    assert!(manage.notice.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn manage_root_list_input_change_clears_only_its_field_error() {
    let root = test_root("manage-root-list-clear-field-error");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.open_manage_root_list();
    app.shell.features.root_browser.manage_list.add_error = "add error".to_string();
    app.shell.features.root_browser.manage_list.edit_error = "edit error".to_string();

    app.clear_manage_root_list_add_error();

    assert!(app
        .shell
        .features
        .root_browser
        .manage_list
        .add_error
        .is_empty());
    assert_eq!(
        app.shell.features.root_browser.manage_list.edit_error,
        "edit error"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn manage_root_list_remove_mode_is_explicit_and_cancelable() {
    let settings = test_settings_scope("manage-root-list-remove-mode-settings");
    let root = test_root("manage-root-list-remove-mode");
    let first = root.join("first");
    let second = root.join("second");
    fs::create_dir_all(&first).expect("create first");
    fs::create_dir_all(&second).expect("create second");
    let mut app = settings.app(first.clone(), 50, String::new());
    app.shell.features.root_browser.saved_roots = vec![first, second];

    app.open_manage_root_list();
    app.enter_manage_root_list_remove_mode();
    app.shell
        .features
        .root_browser
        .manage_list
        .selected_indices
        .insert(0);
    app.cancel_manage_root_list_remove_mode();

    let manage = &app.shell.features.root_browser.manage_list;
    assert!(!manage.remove_mode);
    assert!(manage.selected_indices.is_empty());
    assert_eq!(manage.draft_roots.len(), 2);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn manage_root_list_editing_default_root_follows_replacement_on_apply() {
    let settings = test_settings_scope("manage-root-list-edit-default-settings");
    let root = test_root("manage-root-list-edit-default");
    let original = root.join("original");
    let replacement = root.join("replacement");
    fs::create_dir_all(&original).expect("create original");
    fs::create_dir_all(&replacement).expect("create replacement");
    let replacement_canonical =
        normalize_windows_path_buf(replacement.canonicalize().unwrap_or(replacement.clone()));
    let mut app = settings.app(original.clone(), 50, String::new());
    app.shell.features.root_browser.saved_roots = vec![original.clone()];
    app.shell.features.root_browser.default_root = Some(original);

    app.open_manage_root_list();
    app.select_manage_root_list_item(0);
    app.start_editing_manage_root_list_item();
    app.shell.features.root_browser.manage_list.edit_path =
        replacement.to_string_lossy().to_string();
    app.save_manage_root_list_edit();
    settle_root_validation(&mut app);
    app.apply_manage_root_list_changes();
    settle_settings_commit(&mut app);

    assert_eq!(
        app.shell.features.root_browser.default_root,
        Some(replacement_canonical)
    );
    let _ = fs::remove_dir_all(&root);
}

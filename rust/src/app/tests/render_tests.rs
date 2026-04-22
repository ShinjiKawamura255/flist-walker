use super::*;
use crate::app::render::{
    RenderCommand, RenderFileListDialogCommand, RenderTabBarCommand, RenderTopActionCommand,
    RenderUpdateDialogCommand,
};
use crate::app::render_theme;
use crate::app::{render_dialogs, render_panels};
use crate::entry::EntryDisplayKind;
use crate::updater::UpdateCandidate;
use serde_json::json;

#[test]
fn filelist_use_walker_dialog_lines_are_stable() {
    let lines = FlistWalkerApp::filelist_use_walker_dialog_lines();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("Walker indexing"));
    assert!(lines[1].contains("裏で一時的に Walker"));
}

#[test]
fn top_action_labels_show_history_actions_while_history_search_is_active() {
    let root = test_root("render-history-actions");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.query_state.history_search_active = true;

    assert_eq!(
        app.top_action_labels(),
        vec!["Apply History", "Cancel History Search"]
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn top_action_labels_show_default_create_label_when_idle() {
    let root = test_root("render-default-actions");
    fs::create_dir_all(&root).expect("create dir");
    let app = FlistWalkerApp::new(root.clone(), 50, String::new());

    assert_eq!(
        app.top_action_labels(),
        vec![
            "Open / Execute",
            "Copy Path(s)",
            "Clear Selected",
            "Create File List",
            "Refresh Index",
        ]
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn top_action_labels_show_running_create_label_when_filelist_is_in_progress() {
    let root = test_root("render-running-actions");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.features.filelist.workflow.in_progress = true;

    assert_eq!(app.top_action_labels()[3], "Create File List (Running...)");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn dispatch_render_commands_consumes_top_action_queue() {
    let root = test_root("render-command-top-action");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.pinned_paths.insert(root.join("keep.txt"));
    let ctx = egui::Context::default();

    app.queue_render_command(RenderCommand::TopAction(
        RenderTopActionCommand::ClearPinned,
    ));
    app.dispatch_render_commands(&ctx);

    assert!(app.shell.runtime.pinned_paths.is_empty());
    assert!(app.shell.ui.pending_render_commands.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn dispatch_render_commands_consumes_filelist_dialog_queue() {
    let root = test_root("render-command-filelist-dialog");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let tab_id = app.current_tab_id().expect("active tab id");
    app.shell.features.filelist.workflow.pending_confirmation = Some(PendingFileListConfirmation {
        tab_id,
        root: root.clone(),
        entries: vec![root.join("entry.txt")],
        existing_path: root.join("FileList.txt"),
    });
    let ctx = egui::Context::default();

    app.queue_render_command(RenderCommand::FileListDialog(
        RenderFileListDialogCommand::CancelOverwrite,
    ));
    app.dispatch_render_commands(&ctx);

    assert!(app
        .shell
        .features
        .filelist
        .workflow
        .pending_confirmation
        .is_none());
    assert_eq!(app.shell.runtime.notice, "Create File List canceled");
    assert!(app.shell.ui.pending_render_commands.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn dispatch_render_commands_consumes_update_dialog_queue() {
    let root = test_root("render-command-update-dialog");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.features.update.state.check_failure = Some(UpdateCheckFailureState {
        error: "network timeout".to_string(),
        suppress_future_errors: false,
    });
    let ctx = egui::Context::default();

    app.queue_render_command(RenderCommand::UpdateDialog(
        RenderUpdateDialogCommand::DismissCheckFailure,
    ));
    app.dispatch_render_commands(&ctx);

    assert!(app.shell.features.update.state.check_failure.is_none());
    assert!(app.shell.ui.pending_render_commands.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn dispatch_render_commands_consumes_tab_bar_close_queue() {
    let root = test_root("render-command-tab-bar-close");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    let ctx = egui::Context::default();

    app.queue_render_command(RenderCommand::TabBar(RenderTabBarCommand::CloseTab(0)));
    app.dispatch_render_commands(&ctx);

    assert_eq!(app.shell.tabs.len(), 1);
    assert_eq!(app.shell.tabs.active_tab, 0);
    assert!(app.shell.ui.pending_render_commands.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn dispatch_render_commands_consumes_tab_bar_move_queue() {
    let root = test_root("render-command-tab-bar-move");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.create_new_tab();
    let active_root = app.shell.runtime.root.clone();
    let middle_root = app.shell.tabs.get(1).expect("tab 1").root.clone();
    let last_root = app.shell.tabs.get(2).expect("tab 2").root.clone();
    let ctx = egui::Context::default();

    app.queue_render_command(RenderCommand::TabBar(RenderTabBarCommand::MoveTab {
        from_index: 2,
        to_index: 0,
    }));
    app.dispatch_render_commands(&ctx);

    assert_eq!(app.shell.tabs.get(0).expect("tab 0").root, active_root);
    assert_eq!(app.shell.tabs.get(1).expect("tab 1").root, middle_root);
    assert_eq!(app.shell.tabs.get(2).expect("tab 2").root, last_root);
    assert_eq!(app.shell.tabs.active_tab, 0);
    assert!(app.shell.ui.pending_render_commands.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn results_scroll_enabled_is_true_when_preview_resize_is_inactive() {
    assert!(FlistWalkerApp::results_scroll_enabled(false));
}

#[test]
fn results_scroll_enabled_is_false_when_preview_resize_is_active() {
    assert!(!FlistWalkerApp::results_scroll_enabled(true));
}

#[test]
fn result_row_text_pos_is_left_aligned_and_vertically_centered() {
    let inner = egui::Rect::from_min_max(egui::pos2(8.0, 10.0), egui::pos2(208.0, 34.0));
    let galley_size = egui::vec2(120.0, 14.0);

    let pos = FlistWalkerApp::result_row_text_pos(inner, galley_size);

    assert_eq!(pos.x, inner.left());
    assert_eq!(pos.y, inner.center().y - (galley_size.y * 0.5));
}

#[test]
fn render_theme_selected_fill_preserves_light_and_dark_rgb_contract() {
    assert_eq!(
        render_theme::selected_fill(true),
        egui::Color32::from_rgb(48, 53, 62)
    );
    assert_eq!(
        render_theme::selected_fill(false),
        egui::Color32::from_rgb(228, 232, 238)
    );
}

#[test]
fn render_theme_entry_kind_colors_preserve_rgb_contract() {
    assert_eq!(
        render_theme::entry_kind_color(EntryDisplayKind::Dir),
        egui::Color32::from_rgb(52, 211, 153)
    );
    assert_eq!(
        render_theme::entry_kind_color(EntryDisplayKind::File),
        egui::Color32::from_rgb(96, 165, 250)
    );
    assert_eq!(
        render_theme::entry_kind_color(EntryDisplayKind::Link),
        egui::Color32::from_rgb(250, 204, 21)
    );
}

#[test]
fn render_theme_highlight_color_preserves_rgb_contract() {
    assert_eq!(
        render_theme::highlight_text_color(),
        egui::Color32::from_rgb(245, 158, 11)
    );
}

#[test]
fn gui_surface_snapshot_for_idle_app_is_stable() {
    let root = test_root("render-snapshot-idle");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.status_line = "idle status".to_string();

    let snapshot = serde_json::to_value(app.gui_surface_snapshot()).expect("serialize snapshot");
    assert_eq!(
        snapshot,
        json!({
            "history_search_active": false,
            "show_preview": true,
            "preview_panel_width": 440,
            "top_actions": [
                "Open / Execute",
                "Copy Path(s)",
                "Clear Selected",
                "Create File List",
                "Refresh Index"
            ],
            "status_line": "idle status",
            "filelist_dialogs": [],
            "update_dialogs": [],
        })
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn gui_surface_snapshot_for_dialog_state_is_stable() {
    let root = test_root("render-snapshot-dialogs");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.status_line = "dialog status".to_string();

    let tab_id = app.current_tab_id().expect("tab id");
    app.shell.features.filelist.workflow.pending_confirmation = Some(PendingFileListConfirmation {
        tab_id,
        root: root.clone(),
        entries: vec![root.join("entry.txt")],
        existing_path: root.join("FileList.txt"),
    });
    app.shell.features.update.state.prompt = Some(UpdatePromptState {
        candidate: UpdateCandidate {
            current_version: "0.16.1".to_string(),
            target_version: "0.16.2".to_string(),
            release_url: "https://example.invalid/release".to_string(),
            asset_name: "flistwalker".to_string(),
            asset_url: "https://example.invalid/bin".to_string(),
            readme_asset_name: "README.txt".to_string(),
            readme_asset_url: "https://example.invalid/readme".to_string(),
            license_asset_name: "LICENSE.txt".to_string(),
            license_asset_url: "https://example.invalid/license".to_string(),
            notices_asset_name: "THIRD_PARTY_NOTICES.txt".to_string(),
            notices_asset_url: "https://example.invalid/notices".to_string(),
            ignore_sample_asset_name: Some("flistwalker.ignore.txt.example".to_string()),
            ignore_sample_asset_url: Some("https://example.invalid/ignore-sample".to_string()),
            checksum_url: "https://example.invalid/sums".to_string(),
            checksum_signature_url: "https://example.invalid/sums.sig".to_string(),
            support: UpdateSupport::Auto,
        },
        skip_until_next_version: false,
        install_started: false,
    });

    let snapshot = serde_json::to_value(app.gui_surface_snapshot()).expect("serialize snapshot");
    assert_eq!(
        snapshot,
        json!({
            "history_search_active": false,
            "show_preview": true,
            "preview_panel_width": 440,
            "top_actions": [
                "Open / Execute",
                "Copy Path(s)",
                "Clear Selected",
                "Create File List",
                "Refresh Index"
            ],
            "status_line": "dialog status",
            "filelist_dialogs": [
                {
                    "title": "Overwrite FileList?",
                    "lines": [
                        format!("{} already exists. Overwrite it?", root.join("FileList.txt").display())
                    ],
                    "buttons": ["Overwrite", "Cancel"]
                }
            ],
            "update_dialogs": [
                {
                    "title": "Update Available",
                    "lines": [
                        "FlistWalker 0.16.2 is available. Current version is 0.16.1.",
                        "Download the new release, replace the current binary, and restart?"
                    ],
                    "buttons": ["Download and Restart", "Later"]
                }
            ]
        })
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn render_panels_and_dialogs_execute_in_headless_frame() {
    let root = test_root("render-headless-frame");
    fs::create_dir_all(&root).expect("create dir");
    fs::write(root.join("FileList.txt"), "existing").expect("write filelist");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.query_state.query = "abc".to_string();
    app.shell.runtime.query_state.history_search_active = true;
    app.shell.runtime.query_state.history_search_query = "history".to_string();
    app.shell.runtime.query_state.history_search_results = vec!["history".to_string()];
    app.shell.runtime.status_line = "headless status".to_string();
    app.shell.runtime.results = vec![(root.join("entry.txt"), 0.0)];
    app.shell.runtime.current_row = Some(0);
    app.shell.runtime.preview = "preview".to_string();
    app.shell.ui.set_show_preview(true);
    let tab_id = app.current_tab_id().expect("tab id");
    app.shell.features.filelist.workflow.pending_confirmation = Some(PendingFileListConfirmation {
        tab_id,
        root: root.clone(),
        entries: vec![root.join("entry.txt")],
        existing_path: root.join("FileList.txt"),
    });
    app.shell
        .features
        .filelist
        .workflow
        .pending_ancestor_confirmation = Some(PendingFileListAncestorConfirmation {
        tab_id,
        root: root.clone(),
        entries: vec![root.join("entry.txt")],
    });
    app.shell
        .features
        .filelist
        .workflow
        .pending_use_walker_confirmation = Some(PendingFileListUseWalkerConfirmation {
        source_tab_id: tab_id,
        root: root.clone(),
    });
    app.shell.features.update.state.prompt = Some(UpdatePromptState {
        candidate: UpdateCandidate {
            current_version: "0.16.1".to_string(),
            target_version: "0.16.2".to_string(),
            release_url: "https://example.invalid/release".to_string(),
            asset_name: "flistwalker".to_string(),
            asset_url: "https://example.invalid/bin".to_string(),
            readme_asset_name: "README.txt".to_string(),
            readme_asset_url: "https://example.invalid/readme".to_string(),
            license_asset_name: "LICENSE.txt".to_string(),
            license_asset_url: "https://example.invalid/license".to_string(),
            notices_asset_name: "THIRD_PARTY_NOTICES.txt".to_string(),
            notices_asset_url: "https://example.invalid/notices".to_string(),
            ignore_sample_asset_name: Some("flistwalker.ignore.txt.example".to_string()),
            ignore_sample_asset_url: Some("https://example.invalid/ignore-sample".to_string()),
            checksum_url: "https://example.invalid/sums".to_string(),
            checksum_signature_url: "https://example.invalid/sums.sig".to_string(),
            support: UpdateSupport::Auto,
        },
        skip_until_next_version: false,
        install_started: false,
    });
    app.shell.features.update.state.check_failure = Some(UpdateCheckFailureState {
        error: "network timeout".to_string(),
        suppress_future_errors: false,
    });

    let ctx = egui::Context::default();
    ctx.begin_pass(egui::RawInput::default());
    render_panels::render_top_panel(&mut app, &ctx);
    render_panels::render_status_panel(&mut app, &ctx);
    render_panels::render_central_panel(&mut app, &ctx);
    render_dialogs::render_filelist_dialogs(&mut app, &ctx);
    render_dialogs::render_update_dialog(&mut app, &ctx);
    let _ = ctx.end_pass();

    assert!(app.shell.ui.pending_render_commands.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tab_drop_index_returns_none_for_empty_tabs() {
    assert_eq!(
        super::render_tabs::tab_drop_index(&[], egui::pos2(10.0, 10.0)),
        None
    );
}

#[test]
fn tab_drop_index_chooses_first_tab_before_first_center() {
    let rects = vec![
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(100.0, 24.0)),
        egui::Rect::from_min_max(egui::pos2(110.0, 0.0), egui::pos2(210.0, 24.0)),
    ];

    assert_eq!(
        super::render_tabs::tab_drop_index(&rects, egui::pos2(20.0, 12.0)),
        Some(0)
    );
}

#[test]
fn tab_drop_index_chooses_middle_tab_when_pointer_is_between_centers() {
    let rects = vec![
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(100.0, 24.0)),
        egui::Rect::from_min_max(egui::pos2(110.0, 0.0), egui::pos2(210.0, 24.0)),
        egui::Rect::from_min_max(egui::pos2(220.0, 0.0), egui::pos2(320.0, 24.0)),
    ];

    assert_eq!(
        super::render_tabs::tab_drop_index(&rects, egui::pos2(170.0, 12.0)),
        Some(2)
    );
}

#[test]
fn tab_drop_index_returns_last_tab_after_all_centers() {
    let rects = vec![
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(100.0, 24.0)),
        egui::Rect::from_min_max(egui::pos2(110.0, 0.0), egui::pos2(210.0, 24.0)),
    ];

    assert_eq!(
        super::render_tabs::tab_drop_index(&rects, egui::pos2(260.0, 12.0)),
        Some(1)
    );
}

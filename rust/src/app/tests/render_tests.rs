use super::*;
use crate::app::render::{
    RenderCommand, RenderFileListDialogCommand, RenderHelpDialogCommand,
    RenderRootListDialogCommand, RenderTabBarCommand, RenderTopActionCommand,
    RenderUpdateDialogCommand,
};
use crate::app::render_theme;
use crate::app::{render_dialogs, render_panels};
use crate::entry::EntryDisplayKind;
use crate::search_catalog::{PresetEntryType, PresetSortMode, PresetSource, SearchPreset};
use crate::updater::UpdateCandidate;
use serde_json::json;
use std::collections::BTreeMap;

fn unmodified_key_event(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

#[test]
fn filelist_use_walker_dialog_lines_are_stable() {
    let lines = FlistWalkerApp::filelist_use_walker_dialog_lines();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("Walker indexing"));
    assert!(lines[1].contains("裏で一時的に Walker"));
}

#[test]
fn manage_root_list_action_button_rects_share_same_axis() {
    let row = egui::Rect::from_min_size(egui::pos2(8.4, 467.6), egui::vec2(700.0, 22.0));
    let rects = FlistWalkerApp::manage_root_list_action_button_rects(row, 22.0, 8.0);

    for rect in rects {
        assert_eq!(rect.top(), 468.0);
        assert_eq!(rect.bottom(), 490.0);
        assert_eq!(rect.height(), 22.0);
    }
    assert_eq!(rects[1].left() - rects[0].left(), 80.0);
    assert_eq!(rects[2].left() - rects[1].left(), 80.0);
}

#[test]
fn top_panel_checkbox_icon_and_label_share_center_axis() {
    let row = egui::Rect::from_min_size(egui::pos2(4.0, 12.0), egui::vec2(120.0, 24.0));
    let (checkbox_rect, text_pos) =
        render_panels::centered_checkbox_layout(row, 14.0, 4.0, egui::vec2(68.0, 13.0), 2.0, -1.0);

    assert_eq!(checkbox_rect.center().y, row.center().y - 1.0);
    assert_eq!(text_pos.y + (13.0 / 2.0), row.center().y + 2.0);
}

#[test]
fn tc_180_unlimited_toggle_restores_all_and_reenables_at_depth_one() {
    let mut draft = 4;

    render_panels::update_max_depth_draft_for_unlimited(&mut draft, true);
    assert_eq!(draft, 0, "Unlimited must keep the all-depth sentinel");
    assert!(crate::indexer::MaxDepth::limited(draft)
        .unwrap_or_default()
        .is_unlimited());

    render_panels::update_max_depth_draft_for_unlimited(&mut draft, false);
    assert_eq!(draft, 1, "leaving Unlimited starts at the minimum depth");
}

#[test]
fn selectable_row_uses_full_available_width() {
    let ctx = egui::Context::default();
    let mut measured = None;

    let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
        let available_width = ui.available_width();
        let response = FlistWalkerApp::selectable_row(ui, false, "C:\\saved-root");
        measured = Some((response.rect.width(), available_width));
    });

    let (row_width, available_width) = measured.expect("row measurement");
    assert!((row_width - available_width).abs() < f32::EPSILON);
}

#[test]
fn top_action_labels_show_history_actions_while_history_search_is_active() {
    let root = test_root("render-history-actions");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.query_state.history_search_active = true;

    assert_eq!(
        app.top_action_labels(),
        vec!["Apply History", "Cancel History Search", "Help"]
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
            "Help",
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
fn dispatch_render_commands_opens_and_closes_help() {
    let root = test_root("render-command-help");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let ctx = egui::Context::default();

    app.queue_render_command(RenderCommand::TopAction(RenderTopActionCommand::OpenHelp));
    app.dispatch_render_commands(&ctx);
    assert!(app.shell.ui.help_open);

    app.queue_render_command(RenderCommand::HelpDialog(RenderHelpDialogCommand::Close));
    app.dispatch_render_commands(&ctx);
    assert!(!app.shell.ui.help_open);
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
fn regression_update_check_failure_enter_closes_without_executing_selection() {
    let root = test_root("update-failure-enter-owns-input");
    fs::create_dir_all(&root).expect("create dir");
    let selected = root.join("selected.txt");
    fs::write(&selected, "fixture").expect("write fixture");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.results = vec![(selected, 0.0)];
    app.shell.runtime.current_row = Some(0);
    app.shell.features.update.state.check_failure = Some(UpdateCheckFailureState {
        error: "network timeout".to_string(),
        suppress_future_errors: false,
    });

    let ctx = egui::Context::default();
    let input = egui::RawInput {
        events: vec![unmodified_key_event(egui::Key::Enter)],
        ..Default::default()
    };
    let _ = ctx.run_ui(input, |ui| {
        ui.ctx()
            .memory_mut(|m| m.request_focus(app.shell.ui.query_input_id));
        app.run_ui_frame(ui);
    });

    assert!(app.shell.features.update.state.check_failure.is_none());
    assert_eq!(app.shell.worker_bus.action.pending_request_id, None);
    assert!(!app.shell.worker_bus.action.in_progress);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn regression_update_check_failure_escape_closes_without_clearing_query() {
    let root = test_root("update-failure-escape-owns-input");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "draft query".to_string());
    app.shell.features.update.state.check_failure = Some(UpdateCheckFailureState {
        error: "network timeout".to_string(),
        suppress_future_errors: false,
    });

    let ctx = egui::Context::default();
    let input = egui::RawInput {
        events: vec![unmodified_key_event(egui::Key::Escape)],
        ..Default::default()
    };
    let _ = ctx.run_ui(input, |ui| {
        ui.ctx()
            .memory_mut(|m| m.request_focus(app.shell.ui.query_input_id));
        app.run_ui_frame(ui);
    });

    assert!(app.shell.features.update.state.check_failure.is_none());
    assert_eq!(app.shell.runtime.query_state.query, "draft query");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn regression_update_check_failure_blocks_text_input_to_query() {
    let root = test_root("update-failure-text-owns-input");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "draft query".to_string());
    app.shell.features.update.state.check_failure = Some(UpdateCheckFailureState {
        error: "network timeout".to_string(),
        suppress_future_errors: false,
    });

    let ctx = egui::Context::default();
    let input = egui::RawInput {
        events: vec![egui::Event::Text("leaked text".to_string())],
        ..Default::default()
    };
    let _ = ctx.run_ui(input, |ui| {
        ui.ctx()
            .memory_mut(|m| m.request_focus(app.shell.ui.query_input_id));
        app.run_ui_frame(ui);
    });

    assert!(app.shell.features.update.state.check_failure.is_some());
    assert_eq!(app.shell.runtime.query_state.query, "draft query");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn regression_update_check_failure_blocks_background_selection_shortcuts() {
    let root = test_root("update-failure-selection-owns-input");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.results = vec![(root.join("a.txt"), 0.0), (root.join("b.txt"), 0.0)];
    app.shell.runtime.current_row = Some(0);
    app.shell.features.update.state.check_failure = Some(UpdateCheckFailureState {
        error: "network timeout".to_string(),
        suppress_future_errors: false,
    });

    let ctx = egui::Context::default();
    let input = egui::RawInput {
        events: vec![unmodified_key_event(egui::Key::ArrowDown)],
        ..Default::default()
    };
    let _ = ctx.run_ui(input, |ui| app.run_ui_frame(ui));

    assert!(app.shell.features.update.state.check_failure.is_some());
    assert_eq!(app.shell.runtime.current_row, Some(0));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn filelist_dialog_command_selection_preserves_button_precedence() {
    assert!(matches!(
        render_dialogs::filelist::overwrite_command(true, true),
        Some(RenderFileListDialogCommand::ConfirmOverwrite)
    ));
    assert!(matches!(
        render_dialogs::filelist::ancestor_command(true, true, true),
        Some(RenderFileListDialogCommand::ConfirmAncestorPropagation)
    ));
    assert!(matches!(
        render_dialogs::filelist::ancestor_command(false, true, true),
        Some(RenderFileListDialogCommand::SkipAncestorPropagation)
    ));
    assert!(matches!(
        render_dialogs::filelist::use_walker_command(false, true),
        Some(RenderFileListDialogCommand::CancelUseWalker)
    ));
}

#[test]
fn update_dialog_command_selection_preserves_skip_and_suppress_state() {
    assert!(matches!(
        render_dialogs::update::prompt_command(true, true, true),
        Some(RenderUpdateDialogCommand::StartInstall)
    ));
    assert!(matches!(
        render_dialogs::update::prompt_command(false, true, true),
        Some(RenderUpdateDialogCommand::SkipPromptUntilNextVersion)
    ));
    assert!(matches!(
        render_dialogs::update::prompt_command(false, true, false),
        Some(RenderUpdateDialogCommand::DismissPrompt)
    ));
    assert!(matches!(
        render_dialogs::update::check_failure_command(true, true),
        Some(RenderUpdateDialogCommand::SuppressCheckFailures)
    ));
    assert!(matches!(
        render_dialogs::update::check_failure_command(true, false),
        Some(RenderUpdateDialogCommand::DismissCheckFailure)
    ));
}

#[test]
fn dispatch_render_commands_consumes_root_list_cancel_queue() {
    let root = test_root("render-command-root-list-dialog");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let ctx = egui::Context::default();
    app.open_manage_root_list();
    assert!(app.shell.features.root_browser.manage_list.open);

    app.queue_render_command(RenderCommand::RootListDialog(
        RenderRootListDialogCommand::Cancel,
    ));
    app.dispatch_render_commands(&ctx);

    assert!(!app.shell.features.root_browser.manage_list.open);
    assert!(app.shell.ui.pending_render_commands.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn root_list_render_actions_map_every_ui_intent_to_one_command() {
    use crate::app::render_dialogs::root_list::{root_list_commands, RootListRenderActions};

    let commands = root_list_commands(RootListRenderActions {
        add_input: true,
        browse_and_add: true,
        start_edit: true,
        save_edit: true,
        cancel_edit: true,
        enter_remove_mode: true,
        remove_selected: true,
        cancel_remove_mode: true,
        apply: true,
        ok: true,
        cancel: true,
    });

    assert_eq!(commands.len(), 11);
    assert!(matches!(
        commands[0],
        RenderRootListDialogCommand::BrowseAndAdd
    ));
    assert!(matches!(commands[1], RenderRootListDialogCommand::AddInput));
    assert!(matches!(
        commands[2],
        RenderRootListDialogCommand::StartEdit
    ));
    assert!(matches!(commands[3], RenderRootListDialogCommand::SaveEdit));
    assert!(matches!(
        commands[4],
        RenderRootListDialogCommand::CancelEdit
    ));
    assert!(matches!(
        commands[5],
        RenderRootListDialogCommand::EnterRemoveMode
    ));
    assert!(matches!(
        commands[6],
        RenderRootListDialogCommand::RemoveSelected
    ));
    assert!(matches!(
        commands[7],
        RenderRootListDialogCommand::CancelRemoveMode
    ));
    assert!(matches!(commands[8], RenderRootListDialogCommand::Apply));
    assert!(matches!(commands[9], RenderRootListDialogCommand::Ok));
    assert!(matches!(commands[10], RenderRootListDialogCommand::Cancel));

    let commands = root_list_commands(RootListRenderActions {
        cancel: true,
        ..RootListRenderActions::default()
    });
    assert_eq!(commands.len(), 1);
    assert!(matches!(commands[0], RenderRootListDialogCommand::Cancel));
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
            "root": root.display().to_string(),
            "query": "",
            "use_filelist": true,
            "use_regex": false,
            "ignore_case": true,
            "ignore_list_enabled": true,
            "include_files": true,
            "include_dirs": true,
            "max_depth": "Depth: All",
            "result_sort_mode": "Score",
            "result_sort_scope": "Shown results",
            "result_count": 0,
            "total_match_count": 0,
            "current_result": null,
            "pinned_count": 0,
            "tab_count": 1,
            "active_tab": 0,
            "history_search_active": false,
            "show_preview": true,
            "preview_panel_width": 440,
            "top_actions": [
                "Open / Execute",
                "Copy Path(s)",
                "Clear Selected",
                "Create File List",
                "Refresh Index",
                "Help"
            ],
            "status_line": "idle status",
            "help_dialogs": [],
            "preset_picker_dialogs": [],
            "filelist_dialogs": [],
            "update_dialogs": [],
        })
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_180_gui_surface_snapshot_exposes_active_tab_max_depth() {
    let root = test_root("render-snapshot-max-depth");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.max_depth = crate::indexer::MaxDepth::limited(4).expect("valid depth");

    let snapshot = serde_json::to_value(app.gui_surface_snapshot()).expect("serialize snapshot");
    assert_eq!(snapshot["max_depth"], json!("Depth: ≤ 4"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn gui_surface_snapshot_does_not_expose_permanent_preset_controls() {
    let root = test_root("render-snapshot-no-presets");
    fs::create_dir_all(&root).expect("create dir");
    let app = FlistWalkerApp::new(root.clone(), 50, String::new());

    let snapshot = serde_json::to_value(app.gui_surface_snapshot()).expect("serialize snapshot");
    assert_eq!(snapshot["preset_picker_dialogs"], json!([]));
    for key in [
        "preset_names",
        "selected_preset",
        "preset_name_input",
        "root_name_input",
        "preset_actions",
    ] {
        assert!(snapshot.get(key).is_none(), "GUI still exposes {key}");
    }

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn gui_surface_snapshot_exposes_preset_picker_only_while_open() {
    let root = test_root("render-snapshot-preset-picker");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell
        .features
        .presets
        .catalog
        .save_preset(SearchPreset {
            name: "Rust sources".to_string(),
            root_name: None,
            root_path: root.clone(),
            query: "ext:rs".to_string(),
            entry_type: PresetEntryType::File,
            source: PresetSource::Walker,
            regex: false,
            ignore_case: true,
            ignore_enabled: true,
            sort: PresetSortMode::Score,
            max_depth: crate::indexer::MaxDepth::unlimited(),
            extra: BTreeMap::new(),
        })
        .expect("save preset");
    app.shell.features.presets.picker.open = true;
    app.refresh_preset_picker_matches();

    let snapshot = serde_json::to_value(app.gui_surface_snapshot()).expect("serialize snapshot");
    assert_eq!(
        snapshot["preset_picker_dialogs"][0]["title"],
        json!("Presets")
    );
    assert_eq!(
        snapshot["preset_picker_dialogs"][0]["lines"],
        json!(["Rust sources"])
    );
    assert_eq!(
        snapshot["preset_picker_dialogs"][0]["buttons"],
        json!(["Manage named roots...", "Add", "Edit", "Delete", "Close"])
    );
    assert_eq!(
        snapshot["top_actions"],
        json!([
            "Open / Execute",
            "Copy Path(s)",
            "Clear Selected",
            "Create File List",
            "Refresh Index",
            "Help"
        ])
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn gui_surface_snapshot_exposes_preset_editor_as_contextual_picker_state() {
    let root = test_root("render-snapshot-preset-editor");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "current".to_string());
    app.shell
        .features
        .presets
        .catalog
        .save_preset(SearchPreset {
            name: "Rust sources".to_string(),
            root_name: None,
            root_path: root.clone(),
            query: "ext:rs".to_string(),
            entry_type: PresetEntryType::File,
            source: PresetSource::Walker,
            regex: false,
            ignore_case: true,
            ignore_enabled: true,
            sort: PresetSortMode::Score,
            max_depth: crate::indexer::MaxDepth::unlimited(),
            extra: BTreeMap::new(),
        })
        .expect("save preset");
    app.shell.features.presets.picker.open = true;
    app.refresh_preset_picker_matches();
    app.start_selected_preset_edit();

    let snapshot = serde_json::to_value(app.gui_surface_snapshot()).expect("serialize snapshot");
    assert_eq!(
        snapshot["preset_picker_dialogs"][0]["title"],
        json!("Edit preset")
    );
    assert_eq!(
        snapshot["preset_picker_dialogs"][0]["lines"],
        json!(["Name: Rust sources", "Query: ext:rs", "Max depth: All"])
    );
    assert_eq!(
        snapshot["preset_picker_dialogs"][0]["buttons"],
        json!(["Browse...", "Save", "Cancel"])
    );
    assert_eq!(app.shell.runtime.query_state.query, "current");

    let ctx = egui::Context::default();
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| app.run_ui_frame(ui));
    assert!(app.shell.features.presets.picker.editor.open);
    assert!(app.shell.ui.pending_render_commands.is_empty());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn gui_surface_snapshot_exposes_preset_add_and_delete_confirmation_states() {
    let root = test_root("render-snapshot-preset-add-delete");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "ext:rs".to_string());
    app.shell
        .features
        .presets
        .catalog
        .save_preset(SearchPreset {
            name: "Rust sources".to_string(),
            root_name: None,
            root_path: root.clone(),
            query: "ext:rs".to_string(),
            entry_type: PresetEntryType::File,
            source: PresetSource::Walker,
            regex: false,
            ignore_case: true,
            ignore_enabled: true,
            sort: PresetSortMode::Score,
            max_depth: crate::indexer::MaxDepth::unlimited(),
            extra: BTreeMap::new(),
        })
        .expect("save preset");
    app.shell.features.presets.picker.open = true;
    app.refresh_preset_picker_matches();

    app.start_add_preset();
    let add_snapshot = serde_json::to_value(app.gui_surface_snapshot()).expect("add snapshot");
    assert_eq!(
        add_snapshot["preset_picker_dialogs"][0]["title"],
        json!("Add preset")
    );
    app.cancel_preset_edit();

    app.start_selected_preset_delete();
    let delete_snapshot =
        serde_json::to_value(app.gui_surface_snapshot()).expect("delete snapshot");
    assert_eq!(
        delete_snapshot["preset_picker_dialogs"][0]["title"],
        json!("Delete preset?")
    );
    assert_eq!(
        delete_snapshot["preset_picker_dialogs"][0]["buttons"],
        json!(["Delete preset", "Cancel"])
    );
    assert_eq!(app.shell.runtime.query_state.query, "ext:rs");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn gui_surface_snapshot_exposes_named_root_manager_and_editor() {
    let root = test_root("render-snapshot-named-root-manager");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell
        .features
        .presets
        .catalog
        .add_named_root("work", root.join("workspace"))
        .expect("add named root");
    app.shell.features.presets.picker.open = true;
    app.open_named_root_manager();

    let snapshot = serde_json::to_value(app.gui_surface_snapshot()).expect("serialize snapshot");
    assert_eq!(
        snapshot["preset_picker_dialogs"][0]["title"],
        json!("Named roots")
    );
    assert_eq!(
        snapshot["preset_picker_dialogs"][0]["lines"],
        json!([format!("work — {}", root.join("workspace").display())])
    );
    assert_eq!(
        snapshot["preset_picker_dialogs"][0]["buttons"],
        json!(["Add", "Edit", "Delete", "Back"])
    );

    app.start_selected_named_root_edit();
    let snapshot = serde_json::to_value(app.gui_surface_snapshot()).expect("serialize snapshot");
    assert_eq!(
        snapshot["preset_picker_dialogs"][0]["title"],
        json!("Edit named root")
    );
    assert_eq!(
        snapshot["preset_picker_dialogs"][0]["lines"],
        json!([
            "Name: work",
            format!("Path: {}", root.join("workspace").display())
        ])
    );
    assert_eq!(
        snapshot["preset_picker_dialogs"][0]["buttons"],
        json!(["Browse...", "Use current root", "Save", "Cancel"])
    );

    let ctx = egui::Context::default();
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| app.run_ui_frame(ui));
    assert!(app.shell.features.presets.picker.named_roots.editor.open);
    assert!(app.shell.ui.pending_render_commands.is_empty());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn preset_picker_layout_uses_responsive_width_and_separate_management_copy() {
    use crate::app::render_dialogs::preset_picker::{
        preset_picker_modal_width, MANAGE_NAMED_ROOTS_LABEL, PRESET_PICKER_FOOTER_HINT,
    };

    assert_eq!(preset_picker_modal_width(634.0), 610.0);
    assert_eq!(preset_picker_modal_width(1000.0), 720.0);
    assert_eq!(preset_picker_modal_width(500.0), 600.0);
    assert_eq!(MANAGE_NAMED_ROOTS_LABEL, "Manage named roots...");
    assert_eq!(
        PRESET_PICKER_FOOTER_HINT,
        "Type to filter · Up/Down to select · Enter to apply · F2 to edit · Esc to close"
    );
}

#[test]
fn preset_picker_option_labels_cover_every_persisted_variant() {
    use crate::app::render_dialogs::preset_picker::{entry_type_label, sort_label, source_label};

    assert_eq!(entry_type_label(PresetEntryType::All), "Files and folders");
    assert_eq!(entry_type_label(PresetEntryType::File), "Files");
    assert_eq!(entry_type_label(PresetEntryType::Folder), "Folders");
    assert_eq!(source_label(PresetSource::Auto), "Auto");
    assert_eq!(source_label(PresetSource::Filelist), "FileList");
    assert_eq!(source_label(PresetSource::Walker), "Walker");

    let labels = [
        (PresetSortMode::Score, "Score"),
        (PresetSortMode::NameAsc, "Name ascending"),
        (PresetSortMode::NameDesc, "Name descending"),
        (PresetSortMode::ModifiedDesc, "Modified newest"),
        (PresetSortMode::ModifiedAsc, "Modified oldest"),
        (PresetSortMode::CreatedDesc, "Created newest"),
        (PresetSortMode::CreatedAsc, "Created oldest"),
        (PresetSortMode::SizeDesc, "Size largest"),
        (PresetSortMode::SizeAsc, "Size smallest"),
    ];
    for (value, expected) in labels {
        assert_eq!(sort_label(value), expected);
    }
}

#[test]
fn gui_surface_snapshot_covers_query_results_filters_and_tabs() {
    let root = test_root("render-snapshot-rich-state");
    fs::create_dir_all(&root).expect("create dir");
    let selected = root.join("docs").join("alpha.txt");
    let other = root.join("beta.txt");
    fs::create_dir_all(selected.parent().expect("selected parent")).expect("create docs");
    fs::write(&selected, "alpha").expect("write selected");
    fs::write(&other, "beta").expect("write other");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();
    app.shell.runtime.query_state.query = "alpha".to_string();
    app.shell.runtime.use_filelist = false;
    app.shell.runtime.use_regex = true;
    app.shell.runtime.ignore_case = false;
    app.shell.ui.set_ignore_list_enabled(false);
    app.shell.runtime.include_dirs = false;
    app.shell.runtime.result_sort_mode = ResultSortMode::NameAsc;
    app.shell.runtime.result_sort_scope = ResultSortScope::AllMatches;
    app.shell.runtime.results = vec![(selected.clone(), 9.0), (other, 3.0)];
    app.shell.runtime.total_match_count = 12;
    app.shell.runtime.current_row = Some(0);
    app.shell.runtime.pinned_paths.insert(selected.clone());

    let snapshot = serde_json::to_value(app.gui_surface_snapshot()).expect("serialize snapshot");
    assert_eq!(snapshot["root"], json!(root.display().to_string()));
    assert_eq!(snapshot["query"], json!("alpha"));
    assert_eq!(snapshot["use_filelist"], json!(false));
    assert_eq!(snapshot["use_regex"], json!(true));
    assert_eq!(snapshot["ignore_case"], json!(false));
    assert_eq!(snapshot["ignore_list_enabled"], json!(false));
    assert_eq!(snapshot["include_files"], json!(true));
    assert_eq!(snapshot["include_dirs"], json!(false));
    assert_eq!(snapshot["result_sort_mode"], json!("Name (A-Z)"));
    assert_eq!(snapshot["result_sort_scope"], json!("All matches"));
    assert_eq!(snapshot["result_count"], json!(2));
    assert_eq!(snapshot["total_match_count"], json!(12));
    assert_eq!(
        snapshot["current_result"],
        json!(selected.display().to_string())
    );
    assert_eq!(snapshot["pinned_count"], json!(1));
    assert_eq!(snapshot["tab_count"], json!(2));
    assert_eq!(snapshot["active_tab"], json!(1));
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
            "root": root.display().to_string(),
            "query": "",
            "use_filelist": true,
            "use_regex": false,
            "ignore_case": true,
            "ignore_list_enabled": true,
            "include_files": true,
            "include_dirs": true,
            "max_depth": "Depth: All",
            "result_sort_mode": "Score",
            "result_sort_scope": "Shown results",
            "result_count": 0,
            "total_match_count": 0,
            "current_result": null,
            "pinned_count": 0,
            "tab_count": 1,
            "active_tab": 0,
            "history_search_active": false,
            "show_preview": true,
            "preview_panel_width": 440,
            "top_actions": [
                "Open / Execute",
                "Copy Path(s)",
                "Clear Selected",
                "Create File List",
                "Refresh Index",
                "Help"
            ],
            "status_line": "dialog status",
            "help_dialogs": [],
            "preset_picker_dialogs": [],
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
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
        let ctx = ui.ctx().clone();
        render_panels::render_top_panel(&mut app, ui);
        render_panels::render_status_panel(&mut app, ui);
        render_panels::render_central_panel(&mut app, ui);
        render_dialogs::render_filelist_dialogs(&mut app, &ctx);
        render_dialogs::render_update_dialog(&mut app, &ctx);
    });

    assert!(app.shell.ui.pending_render_commands.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn gui_surface_snapshot_exposes_contextual_help_dialog() {
    let root = test_root("render-snapshot-help");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.ui.help_open = true;

    let snapshot = serde_json::to_value(app.gui_surface_snapshot()).expect("serialize snapshot");
    assert_eq!(snapshot["help_dialogs"][0]["title"], json!("Help"));
    assert_eq!(snapshot["help_dialogs"][0]["buttons"], json!(["Close"]));
    let lines = snapshot["help_dialogs"][0]["lines"]
        .as_array()
        .expect("help lines");
    assert!(lines
        .iter()
        .any(|line| line == "Ctrl+N / Ctrl+P — Move the current row"));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn disabled_emacs_keybindings_prevent_textedit_ctrl_k_from_editing_query() {
    let root = test_root("render-disabled-ctrl-k");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.emacs_keybindings_enabled = false;
    app.shell.runtime.query_state.query = "alpha beta".to_string();
    let ctx = egui::Context::default();
    let input = egui::RawInput {
        modifiers: egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        },
        events: vec![egui::Event::Key {
            key: egui::Key::K,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                ctrl: true,
                command: true,
                ..Default::default()
            },
        }],
        ..Default::default()
    };
    let _ = ctx.run_ui(input, |ui| {
        ui.ctx()
            .memory_mut(|m| m.request_focus(app.shell.ui.query_input_id));
        render_panels::render_top_panel(&mut app, ui);
    });

    assert_eq!(app.shell.runtime.query_state.query, "alpha beta");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn run_ui_frame_executes_render_facade_in_headless_frame() {
    let root = test_root("render-run-ui-frame");
    fs::create_dir_all(&root).expect("create dir");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.query_state.query = "entry".to_string();
    app.shell.runtime.status_line = "facade status".to_string();
    app.shell.runtime.results = vec![(root.join("entry.txt"), 0.0)];
    app.shell.runtime.current_row = Some(0);
    app.shell.runtime.preview = "preview".to_string();
    app.shell.ui.set_show_preview(true);

    let ctx = egui::Context::default();
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| app.run_ui_frame(ui));

    assert!(app.shell.ui.pending_render_commands.is_empty());
    assert_eq!(app.shell.runtime.results.len(), 1);
    assert_eq!(app.shell.runtime.current_row, Some(0));
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

#[test]
fn tab_close_button_hover_visuals_make_close_hit_area_visible() {
    let fallback = egui::Color32::from_rgb(220, 220, 220);
    let palette = TabAccentColor::Teal.palette(false);

    let idle = super::render_tabs::tab_close_button_visuals(
        false,
        true,
        false,
        true,
        Some(palette),
        fallback,
    );
    assert_eq!(idle.fill, egui::Color32::TRANSPARENT);
    assert_eq!(idle.stroke, egui::Stroke::NONE);

    let hovered = super::render_tabs::tab_close_button_visuals(
        false,
        true,
        true,
        true,
        Some(palette),
        fallback,
    );
    assert_ne!(hovered.fill, egui::Color32::TRANSPARENT);
    assert!(hovered.stroke.width > 0.0);
    assert!(hovered.stroke.width < 0.8);
    assert_eq!(hovered.stroke.color, palette.border);
    assert_eq!(hovered.text, palette.foreground);

    let disabled = super::render_tabs::tab_close_button_visuals(
        false,
        false,
        true,
        true,
        Some(palette),
        fallback,
    );
    assert_eq!(disabled.fill, egui::Color32::TRANSPARENT);
    assert_eq!(disabled.stroke, egui::Stroke::NONE);
    assert!(disabled.text.r() < palette.foreground.r());
}

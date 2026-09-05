use super::*;
use crate::app::render_panels;
use crate::search_catalog::{
    PresetEntryType, PresetSortMode, PresetSource, SearchCatalog, SearchPreset,
};
use std::collections::BTreeMap;

fn preset(name: &str, root: &Path, query: &str) -> SearchPreset {
    SearchPreset {
        name: name.to_string(),
        root_name: None,
        root_path: root.to_path_buf(),
        query: query.to_string(),
        entry_type: PresetEntryType::File,
        source: PresetSource::Walker,
        regex: false,
        ignore_case: true,
        ignore_enabled: true,
        sort: PresetSortMode::Score,
        max_depth: crate::indexer::MaxDepth::unlimited(),
        extra: BTreeMap::new(),
    }
}

fn key_event(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

#[test]
fn ctrl_shift_p_opens_picker_without_changing_current_search() {
    let root = test_root("preset-picker-shortcut");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "draft".to_string());

    run_shortcuts_frame(
        &mut app,
        true,
        vec![key_event(egui::Key::P, gui_shortcut_modifiers(true))],
    );

    assert!(app.shell.features.presets.picker.open);
    assert_eq!(app.shell.runtime.query_state.query, "draft");
    assert!(!app.shell.runtime.query_state.history_search_active);
    assert!(app.shell.worker_bus.catalog.in_progress);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn picker_fuzzy_filters_names_and_resets_selection() {
    let root = test_root("preset-picker-filter");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let mut catalog = SearchCatalog::default();
    catalog
        .save_preset(preset("Rust sources", &root, "ext:rs"))
        .expect("save rust preset");
    catalog
        .save_preset(preset("Documentation", &root, "ext:md"))
        .expect("save docs preset");
    catalog
        .save_preset(preset("Release assets", &root, "dir:dist"))
        .expect("save release preset");
    app.shell.features.presets.catalog = catalog;
    app.shell.features.presets.picker.query = "rso".to_string();

    app.refresh_preset_picker_matches();

    assert_eq!(app.preset_picker_match_names(), vec!["Rust sources"]);
    assert_eq!(app.shell.features.presets.picker.selected_match, Some(0));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn regression_emacs_navigation_and_accept_apply_to_the_preset_picker() {
    let root = test_root("preset-picker-emacs-regression");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "before".to_string());
    for (name, query) in [("Alpha", "alpha"), ("Beta", "beta"), ("Gamma", "gamma")] {
        app.shell
            .features
            .presets
            .catalog
            .save_preset(preset(name, &root, query))
            .expect("save preset");
    }
    app.shell.features.presets.picker.open = true;
    app.refresh_preset_picker_matches();

    run_shortcuts_frame(
        &mut app,
        true,
        vec![key_event(egui::Key::N, emacs_shortcut_modifiers(false))],
    );
    assert_eq!(app.shell.features.presets.picker.selected_match, Some(1));

    run_shortcuts_frame(
        &mut app,
        true,
        vec![key_event(egui::Key::P, emacs_shortcut_modifiers(false))],
    );
    assert_eq!(app.shell.features.presets.picker.selected_match, Some(0));

    run_shortcuts_frame(
        &mut app,
        true,
        vec![key_event(egui::Key::M, emacs_shortcut_modifiers(false))],
    );
    assert!(!app.shell.features.presets.picker.open);
    assert_eq!(app.shell.runtime.query_state.query, "alpha");
    assert!(!app.shell.worker_bus.action.in_progress);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn regression_emacs_preset_picker_shortcuts_respect_the_runtime_setting() {
    let root = test_root("preset-picker-emacs-disabled-regression");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "before".to_string());
    app.shell.runtime.emacs_keybindings_enabled = false;
    for (name, query) in [("Alpha", "alpha"), ("Beta", "beta")] {
        app.shell
            .features
            .presets
            .catalog
            .save_preset(preset(name, &root, query))
            .expect("save preset");
    }
    app.shell.features.presets.picker.open = true;
    app.refresh_preset_picker_matches();

    run_shortcuts_frame(
        &mut app,
        true,
        vec![key_event(egui::Key::N, emacs_shortcut_modifiers(false))],
    );
    assert_eq!(app.shell.features.presets.picker.selected_match, Some(0));

    run_shortcuts_frame(
        &mut app,
        true,
        vec![key_event(egui::Key::J, emacs_shortcut_modifiers(false))],
    );
    assert!(app.shell.features.presets.picker.open);
    assert_eq!(app.shell.runtime.query_state.query, "before");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn regression_emacs_navigation_applies_to_the_named_root_manager() {
    let root = test_root("named-root-emacs-regression");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell
        .features
        .presets
        .catalog
        .add_named_root("alpha", root.join("alpha"))
        .expect("add root");
    app.shell
        .features
        .presets
        .catalog
        .add_named_root("beta", root.join("beta"))
        .expect("add root");
    app.shell.features.presets.picker.open = true;
    app.open_named_root_manager();

    run_shortcuts_frame(
        &mut app,
        true,
        vec![key_event(egui::Key::N, emacs_shortcut_modifiers(false))],
    );
    assert_eq!(
        app.shell.features.presets.picker.named_roots.selected_index,
        Some(1)
    );

    run_shortcuts_frame(
        &mut app,
        true,
        vec![key_event(egui::Key::P, emacs_shortcut_modifiers(false))],
    );
    assert_eq!(
        app.shell.features.presets.picker.named_roots.selected_index,
        Some(0)
    );

    run_shortcuts_frame(
        &mut app,
        true,
        vec![key_event(egui::Key::G, emacs_shortcut_modifiers(false))],
    );
    assert!(!app.shell.features.presets.picker.named_roots.open);
    assert!(app.shell.features.presets.picker.open);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn regression_emacs_ctrl_a_and_ctrl_d_edit_the_preset_filter() {
    let root = test_root("preset-filter-emacs-edit-regression");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.features.presets.picker.open = true;
    app.shell.features.presets.picker.query = "abcd".to_string();
    app.shell.features.presets.picker.focus_requested = true;
    let ctx = egui::Context::default();
    let input_id = egui::Id::new(FlistWalkerApp::PRESET_PICKER_QUERY_ID);

    let _ = ctx.run_ui(egui::RawInput::default(), |ui| app.run_ui_frame(ui));
    app.clear_focus_query_request();
    ctx.memory_mut(|memory| memory.request_focus(input_id));
    let mut state =
        egui::widgets::text_edit::TextEditState::load(&ctx, input_id).expect("preset filter state");
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(2),
        )));
    state.store(&ctx, input_id);

    let modifiers = emacs_shortcut_modifiers(false);
    let _ = ctx.run_ui(
        egui::RawInput {
            modifiers,
            events: vec![key_event(egui::Key::A, modifiers)],
            ..Default::default()
        },
        |ui| app.run_ui_frame(ui),
    );
    let state = egui::widgets::text_edit::TextEditState::load(&ctx, input_id)
        .expect("preset filter state after Ctrl+A");
    let range = state.cursor.char_range().expect("preset filter cursor");
    assert_eq!(range.primary.index.0, 0);
    assert_eq!(range.secondary.index.0, 0);

    let _ = ctx.run_ui(
        egui::RawInput {
            modifiers,
            events: vec![key_event(egui::Key::D, modifiers)],
            ..Default::default()
        },
        |ui| app.run_ui_frame(ui),
    );
    assert_eq!(app.shell.features.presets.picker.query, "bcd");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn regression_emacs_ctrl_e_and_ctrl_h_edit_preset_editor_fields() {
    let root = test_root("preset-editor-emacs-edit-regression");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.features.presets.picker.open = true;
    app.start_add_preset();
    app.shell.features.presets.picker.editor.name = "abcd".to_string();
    app.shell.features.presets.picker.editor.focus_requested = true;
    let ctx = egui::Context::default();
    let input_id = egui::Id::new("preset-editor-name");

    let _ = ctx.run_ui(egui::RawInput::default(), |ui| app.run_ui_frame(ui));
    app.clear_focus_query_request();
    ctx.memory_mut(|memory| memory.request_focus(input_id));
    let mut state = egui::widgets::text_edit::TextEditState::load(&ctx, input_id)
        .expect("preset editor name state");
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(1),
        )));
    state.store(&ctx, input_id);

    let modifiers = emacs_shortcut_modifiers(false);
    let _ = ctx.run_ui(
        egui::RawInput {
            modifiers,
            events: vec![key_event(egui::Key::E, modifiers)],
            ..Default::default()
        },
        |ui| app.run_ui_frame(ui),
    );
    let state = egui::widgets::text_edit::TextEditState::load(&ctx, input_id)
        .expect("preset editor name state after Ctrl+E");
    let range = state.cursor.char_range().expect("preset editor cursor");
    assert_eq!(range.primary.index.0, 4);
    assert_eq!(range.secondary.index.0, 4);

    let _ = ctx.run_ui(
        egui::RawInput {
            modifiers,
            events: vec![key_event(egui::Key::H, modifiers)],
            ..Default::default()
        },
        |ui| app.run_ui_frame(ui),
    );
    assert_eq!(app.shell.features.presets.picker.editor.name, "abc");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn regression_macos_native_ctrl_b_and_ctrl_f_move_preset_filter_once() {
    let root = test_root("preset-filter-macos-native-emacs-motion");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.features.presets.picker.open = true;
    app.shell.features.presets.picker.query = "abcd".to_string();
    app.shell.features.presets.picker.focus_requested = true;
    let ctx = egui::Context::default();
    ctx.set_os(egui::os::OperatingSystem::Mac);
    let input_id = egui::Id::new(FlistWalkerApp::PRESET_PICKER_QUERY_ID);

    let _ = ctx.run_ui(egui::RawInput::default(), |ui| app.run_ui_frame(ui));
    app.clear_focus_query_request();
    ctx.memory_mut(|memory| memory.request_focus(input_id));
    let mut state =
        egui::widgets::text_edit::TextEditState::load(&ctx, input_id).expect("preset filter state");
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(2),
        )));
    state.store(&ctx, input_id);

    let modifiers = emacs_shortcut_modifiers(false);
    let _ = ctx.run_ui(
        egui::RawInput {
            modifiers,
            events: vec![key_event(egui::Key::B, modifiers)],
            ..Default::default()
        },
        |ui| app.run_ui_frame(ui),
    );
    let state = egui::widgets::text_edit::TextEditState::load(&ctx, input_id)
        .expect("preset filter state after Ctrl+B");
    let range = state.cursor.char_range().expect("preset filter cursor");
    assert_eq!(range.primary.index.0, 1);
    assert_eq!(range.secondary.index.0, 1);

    let _ = ctx.run_ui(
        egui::RawInput {
            modifiers,
            events: vec![key_event(egui::Key::F, modifiers)],
            ..Default::default()
        },
        |ui| app.run_ui_frame(ui),
    );
    let state = egui::widgets::text_edit::TextEditState::load(&ctx, input_id)
        .expect("preset filter state after Ctrl+F");
    let range = state.cursor.char_range().expect("preset filter cursor");
    assert_eq!(range.primary.index.0, 2);
    assert_eq!(range.secondary.index.0, 2);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn regression_emacs_ctrl_k_and_ctrl_y_share_the_kill_buffer_in_preset_fields() {
    let root = test_root("preset-editor-emacs-kill-yank-regression");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.features.presets.picker.open = true;
    app.start_add_preset();
    app.shell.features.presets.picker.editor.name = "abcd".to_string();
    app.shell.features.presets.picker.editor.focus_requested = true;
    let ctx = egui::Context::default();
    let input_id = egui::Id::new("preset-editor-name");

    let _ = ctx.run_ui(egui::RawInput::default(), |ui| app.run_ui_frame(ui));
    app.clear_focus_query_request();
    ctx.memory_mut(|memory| memory.request_focus(input_id));
    let mut state = egui::widgets::text_edit::TextEditState::load(&ctx, input_id)
        .expect("preset editor name state");
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(2),
        )));
    state.store(&ctx, input_id);

    let modifiers = emacs_shortcut_modifiers(false);
    let _ = ctx.run_ui(
        egui::RawInput {
            modifiers,
            events: vec![key_event(egui::Key::K, modifiers)],
            ..Default::default()
        },
        |ui| app.run_ui_frame(ui),
    );
    assert_eq!(app.shell.features.presets.picker.editor.name, "ab");
    assert_eq!(app.shell.runtime.query_state.kill_buffer, "cd");

    let _ = ctx.run_ui(
        egui::RawInput {
            modifiers,
            events: vec![key_event(egui::Key::Y, modifiers)],
            ..Default::default()
        },
        |ui| app.run_ui_frame(ui),
    );
    assert_eq!(app.shell.features.presets.picker.editor.name, "abcd");

    let mut state = egui::widgets::text_edit::TextEditState::load(&ctx, input_id)
        .expect("preset editor name state before Ctrl+U");
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(2),
        )));
    state.store(&ctx, input_id);
    let _ = ctx.run_ui(
        egui::RawInput {
            modifiers,
            events: vec![key_event(egui::Key::U, modifiers)],
            ..Default::default()
        },
        |ui| app.run_ui_frame(ui),
    );
    assert_eq!(app.shell.features.presets.picker.editor.name, "cd");
    assert_eq!(app.shell.runtime.query_state.kill_buffer, "ab");

    let _ = ctx.run_ui(
        egui::RawInput {
            modifiers,
            events: vec![key_event(egui::Key::Y, modifiers)],
            ..Default::default()
        },
        |ui| app.run_ui_frame(ui),
    );
    assert_eq!(app.shell.features.presets.picker.editor.name, "abcd");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn regression_disabled_emacs_setting_prevents_native_ctrl_k_in_preset_fields() {
    let root = test_root("preset-editor-disabled-emacs-regression");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.emacs_keybindings_enabled = false;
    app.shell.features.presets.picker.open = true;
    app.start_add_preset();
    app.shell.features.presets.picker.editor.name = "abcd".to_string();
    app.shell.features.presets.picker.editor.focus_requested = true;
    let ctx = egui::Context::default();
    let input_id = egui::Id::new("preset-editor-name");

    let _ = ctx.run_ui(egui::RawInput::default(), |ui| app.run_ui_frame(ui));
    app.clear_focus_query_request();
    ctx.memory_mut(|memory| memory.request_focus(input_id));
    let mut state = egui::widgets::text_edit::TextEditState::load(&ctx, input_id)
        .expect("preset editor name state");
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(2),
        )));
    state.store(&ctx, input_id);
    let modifiers = emacs_shortcut_modifiers(false);
    let _ = ctx.run_ui(
        egui::RawInput {
            modifiers,
            events: vec![key_event(egui::Key::K, modifiers)],
            ..Default::default()
        },
        |ui| app.run_ui_frame(ui),
    );

    assert_eq!(app.shell.features.presets.picker.editor.name, "abcd");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn enter_applies_selected_pure_search_preset_without_executing_results() {
    let root = test_root("preset-picker-apply");
    let preset_root = root.join("source");
    fs::create_dir_all(&preset_root).expect("create preset root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "before".to_string());
    let mut selected = preset("Rust", &preset_root, "ext:rs");
    selected.ignore_case = false;
    selected.ignore_enabled = false;
    selected.sort = PresetSortMode::NameAsc;
    app.shell
        .features
        .presets
        .catalog
        .save_preset(selected)
        .expect("save preset");
    app.shell.features.presets.picker.open = true;
    app.refresh_preset_picker_matches();

    run_shortcuts_frame(
        &mut app,
        true,
        vec![key_event(egui::Key::Enter, egui::Modifiers::NONE)],
    );

    assert!(!app.shell.features.presets.picker.open);
    assert_eq!(app.shell.runtime.root, preset_root);
    assert_eq!(app.shell.runtime.query_state.query, "ext:rs");
    assert!(!app.shell.runtime.use_filelist);
    assert!(!app.shell.runtime.ignore_case);
    assert!(!app.shell.ui.ignore_list_enabled);
    assert!(app.shell.runtime.include_files);
    assert!(!app.shell.runtime.include_dirs);
    assert_eq!(app.shell.runtime.result_sort_mode, ResultSortMode::NameAsc);
    assert!(app.shell.indexing.in_progress);
    assert!(app.shell.runtime.notice.contains("Applied preset: Rust"));
    assert!(!app.shell.worker_bus.action.in_progress);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn applying_preset_moves_query_cursor_to_end() {
    let root = test_root("preset-picker-query-cursor");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "before".to_string());
    app.shell
        .features
        .presets
        .catalog
        .save_preset(preset("After", &root, "after"))
        .expect("save preset");
    app.shell.features.presets.picker.open = true;
    app.shell.features.presets.picker.restore_query_focus = true;
    app.refresh_preset_picker_matches();

    let ctx = egui::Context::default();
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.ctx()
            .memory_mut(|memory| memory.request_focus(app.shell.ui.query_input_id));
        render_panels::render_top_panel(&mut app, ui);
    });
    let mut query_state =
        egui::widgets::text_edit::TextEditState::load(&ctx, app.shell.ui.query_input_id)
            .expect("query text edit state");
    query_state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(0),
        )));
    query_state.store(&ctx, app.shell.ui.query_input_id);

    app.apply_selected_preset();

    let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
        render_panels::render_top_panel(&mut app, ui);
    });
    let query_state =
        egui::widgets::text_edit::TextEditState::load(&ctx, app.shell.ui.query_input_id)
            .expect("query text edit state after preset");
    assert_eq!(
        query_state
            .cursor
            .char_range()
            .expect("query cursor")
            .primary
            .index,
        egui::text::CharIndex("after".chars().count())
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn regression_same_root_preset_applies_filters_and_sort_before_fresh_search() {
    let root = test_root("preset-picker-same-root-regression");
    fs::create_dir_all(&root).expect("create root");
    // Regression guard: the ignore sentinel must not occur in platform-owned temp
    // ancestors (for example macOS `/var/folders`); keep it filename-specific.
    let ignored = root.join("preset-ignore-sentinel-result.txt");
    let kept = root.join("kept-result.txt");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "before".to_string());
    reset_index_request_state_for_test(&mut app);
    app.shell.runtime.use_filelist = false;
    app.shell.runtime.include_files = true;
    app.shell.runtime.include_dirs = false;
    app.shell.runtime.all_entries =
        Arc::new(vec![file_entry(ignored.clone()), file_entry(kept.clone())]);
    app.shell.runtime.entries = Arc::clone(&app.shell.runtime.all_entries);
    app.shell.runtime.ignore_list_terms = Arc::new(vec!["preset-ignore-sentinel".to_string()]);
    app.shell.ui.ignore_list_enabled = false;
    app.shell.runtime.result_sort_mode = ResultSortMode::ModifiedDesc;
    app.shell.runtime.result_sort_scope = ResultSortScope::AllMatches;

    let mut selected = preset("Same root", &root, "result");
    selected.ignore_enabled = true;
    selected.sort = PresetSortMode::NameAsc;
    app.shell
        .features
        .presets
        .catalog
        .save_preset(selected)
        .expect("save preset");
    app.shell.features.presets.picker.open = true;
    app.refresh_preset_picker_matches();

    let (request_tx, request_rx) = mpsc::channel::<SearchRequest>();
    let (response_tx, response_rx) = mpsc::channel::<SearchResponse>();
    app.shell.search = SearchCoordinator::new(request_tx, response_rx);
    let (stale_request_id, stale_cancel) =
        app.shell.search.begin_active_request(app.current_tab_id());

    app.apply_selected_preset();

    let fresh = request_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("fresh search request");
    assert!(stale_cancel.load(Ordering::Acquire));
    assert_ne!(fresh.request_id, stale_request_id);
    assert!(request_rx.try_recv().is_err(), "preset must enqueue once");
    assert_eq!(fresh.sort_mode, ResultSortMode::NameAsc);
    assert_eq!(fresh.sort_scope, ResultSortScope::AllMatches);
    assert_eq!(
        fresh
            .entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>(),
        vec![kept.clone()]
    );
    assert!(!app.shell.indexing.in_progress);

    response_tx
        .send(SearchResponse {
            request_id: stale_request_id,
            results: vec![(ignored.clone(), 1.0)],
            total_match_count: 1,
            sort_mode: ResultSortMode::Score,
            sort_scope: ResultSortScope::ShownResults,
            error: None,
        })
        .expect("send stale response");
    app.poll_search_response();
    assert_eq!(app.shell.runtime.result_sort_mode, ResultSortMode::NameAsc);
    assert_eq!(
        app.shell.runtime.result_sort_scope,
        ResultSortScope::AllMatches
    );

    response_tx
        .send(SearchResponse {
            request_id: fresh.request_id,
            results: vec![(kept.clone(), 1.0)],
            total_match_count: 1,
            sort_mode: fresh.sort_mode,
            sort_scope: fresh.sort_scope,
            error: None,
        })
        .expect("send fresh response");
    app.poll_search_response();
    assert_eq!(app.shell.runtime.results, vec![(kept, 1.0)]);
    assert_eq!(app.shell.runtime.result_sort_mode, ResultSortMode::NameAsc);
    assert_eq!(
        app.shell.runtime.result_sort_scope,
        ResultSortScope::AllMatches
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn tc_180_preset_max_depth_is_tab_local_persistent_and_new_tabs_start_unlimited() {
    let root = test_root("preset-picker-max-depth-tab-local");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.create_new_tab();

    let mut selected = preset("Depth three", &root, "");
    selected.max_depth = crate::indexer::MaxDepth::limited(3).expect("valid depth");
    app.shell
        .features
        .presets
        .catalog
        .save_preset(selected)
        .expect("save preset");
    app.shell.features.presets.picker.open = true;
    app.refresh_preset_picker_matches();

    app.apply_selected_preset();

    assert_eq!(app.shell.runtime.max_depth.value(), Some(3));
    assert!(app
        .shell
        .tabs
        .get(0)
        .expect("first tab")
        .max_depth
        .is_unlimited());
    assert_eq!(
        app.shell.tabs.get(1).expect("active tab").max_depth.value(),
        Some(3)
    );

    app.switch_to_tab_index(0);
    assert!(app.shell.runtime.max_depth.is_unlimited());
    app.switch_to_tab_index(1);
    assert_eq!(app.shell.runtime.max_depth.value(), Some(3));

    app.create_new_tab();
    assert!(app.shell.runtime.max_depth.is_unlimited());
    assert_eq!(
        app.shell.tabs.get(1).expect("preset tab").max_depth.value(),
        Some(3)
    );
    assert!(app
        .shell
        .tabs
        .get(2)
        .expect("new tab")
        .max_depth
        .is_unlimited());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn tc_180_unlimited_preset_restores_all_only_on_the_active_tab() {
    let root = test_root("preset-picker-unlimited-depth-tab-local");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.max_depth = crate::indexer::MaxDepth::limited(2).expect("valid depth");
    app.sync_active_tab_state();
    app.create_new_tab();
    app.shell.runtime.max_depth = crate::indexer::MaxDepth::limited(5).expect("valid depth");
    app.sync_active_tab_state();

    app.shell
        .features
        .presets
        .catalog
        .save_preset(preset("All depths", &root, ""))
        .expect("save preset");
    app.shell.features.presets.picker.open = true;
    app.refresh_preset_picker_matches();

    app.apply_selected_preset();

    assert!(app.shell.runtime.max_depth.is_unlimited());
    assert_eq!(
        app.shell.tabs.get(0).expect("other tab").max_depth.value(),
        Some(2)
    );
    assert!(app
        .shell
        .tabs
        .get(1)
        .expect("active tab")
        .max_depth
        .is_unlimited());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn regression_same_root_preset_disabling_ignore_restores_all_search_entries() {
    let root = test_root("preset-picker-disable-ignore-regression");
    fs::create_dir_all(&root).expect("create root");
    let ignored = root.join("preset-ignore-sentinel-result.txt");
    let kept = root.join("kept-result.txt");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "before".to_string());
    reset_index_request_state_for_test(&mut app);
    app.shell.runtime.use_filelist = false;
    app.shell.runtime.include_files = true;
    app.shell.runtime.include_dirs = false;
    app.shell.runtime.all_entries =
        Arc::new(vec![file_entry(ignored.clone()), file_entry(kept.clone())]);
    app.shell.runtime.entries = Arc::new(vec![file_entry(kept)]);
    app.shell.runtime.ignore_list_terms = Arc::new(vec!["preset-ignore-sentinel".to_string()]);
    app.shell.ui.ignore_list_enabled = true;

    let mut selected = preset("Show ignored", &root, "result");
    selected.ignore_enabled = false;
    app.shell
        .features
        .presets
        .catalog
        .save_preset(selected)
        .expect("save preset");
    app.shell.features.presets.picker.open = true;
    app.refresh_preset_picker_matches();

    let (request_tx, request_rx) = mpsc::channel::<SearchRequest>();
    let (_response_tx, response_rx) = mpsc::channel::<SearchResponse>();
    app.shell.search = SearchCoordinator::new(request_tx, response_rx);

    app.apply_selected_preset();

    let request = request_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("search request");
    assert_eq!(request.entries.len(), 2);
    assert!(request.entries.iter().any(|entry| entry.path == ignored));
    assert!(request_rx.try_recv().is_err(), "preset must enqueue once");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn open_picker_consumes_background_shortcuts_and_escape_preserves_search() {
    let root = test_root("preset-picker-modal-input");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "keep".to_string());
    app.shell.ui.focus_query_requested = false;

    run_shortcuts_frame(
        &mut app,
        true,
        vec![key_event(egui::Key::P, gui_shortcut_modifiers(true))],
    );

    run_shortcuts_frame(
        &mut app,
        true,
        vec![key_event(egui::Key::C, gui_shortcut_modifiers(true))],
    );
    assert!(app.shell.features.presets.picker.open);
    assert!(!app.shell.ui.pending_copy_shortcut);

    run_shortcuts_frame(
        &mut app,
        true,
        vec![key_event(egui::Key::Escape, egui::Modifiers::NONE)],
    );
    assert!(!app.shell.features.presets.picker.open);
    assert_eq!(app.shell.runtime.query_state.query, "keep");
    assert!(app.shell.ui.focus_query_requested);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn stale_catalog_response_cannot_replace_latest_picker_catalog() {
    use crate::app::worker::protocol::CatalogResponse;

    let root = test_root("preset-picker-stale-catalog");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let (tx, rx) = mpsc::channel();
    app.shell.worker_bus.catalog.rx = rx;
    app.shell.worker_bus.catalog.pending_request_id = Some(2);
    app.shell.worker_bus.catalog.in_progress = true;

    let mut stale = SearchCatalog::default();
    stale
        .save_preset(preset("Stale", &root, "old"))
        .expect("save stale preset");
    let mut latest = SearchCatalog::default();
    latest
        .save_preset(preset("Latest", &root, "new"))
        .expect("save latest preset");
    tx.send(CatalogResponse {
        request_id: 1,
        result: Ok(stale),
    })
    .expect("send stale response");
    tx.send(CatalogResponse {
        request_id: 2,
        result: Ok(latest),
    })
    .expect("send latest response");

    app.poll_catalog_response();

    assert!(app
        .shell
        .features
        .presets
        .catalog
        .preset("Latest")
        .is_some());
    assert!(app.shell.features.presets.catalog.preset("Stale").is_none());
    assert!(!app.shell.worker_bus.catalog.in_progress);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn f2_opens_selected_preset_as_a_draft_without_applying_it() {
    let root = test_root("preset-editor-open");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "current query".to_string());
    let mut selected = preset("Rust", &root.join("saved"), "ext:rs");
    selected.source = PresetSource::Auto;
    selected.sort = PresetSortMode::ModifiedDesc;
    app.shell
        .features
        .presets
        .catalog
        .save_preset(selected)
        .expect("save preset");
    app.shell.features.presets.picker.open = true;
    app.refresh_preset_picker_matches();

    run_shortcuts_frame(
        &mut app,
        true,
        vec![key_event(egui::Key::F2, egui::Modifiers::NONE)],
    );

    let editor = &app.shell.features.presets.picker.editor;
    assert!(editor.open);
    assert_eq!(editor.original_name, "Rust");
    assert_eq!(editor.name, "Rust");
    assert_eq!(editor.root_path, root.join("saved").display().to_string());
    assert_eq!(editor.query, "ext:rs");
    assert_eq!(editor.source, PresetSource::Auto);
    assert_eq!(editor.sort, PresetSortMode::ModifiedDesc);
    assert_eq!(app.shell.runtime.query_state.query, "current query");
    assert!(!app.shell.worker_bus.action.in_progress);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn preset_root_browse_updates_only_the_draft_and_uses_its_existing_parent() {
    let root = test_root("preset-editor-browse-root");
    let selected = root.join("selected");
    fs::create_dir_all(&selected).expect("create selected root");
    let snapshot = root.join("missing").join("child");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "current query".to_string());
    app.shell
        .features
        .presets
        .catalog
        .save_preset(preset("Rust", &snapshot, "ext:rs"))
        .expect("save preset");
    app.shell.features.presets.picker.open = true;
    app.refresh_preset_picker_matches();
    app.start_selected_preset_edit();
    app.shell.features.root_browser.browse_dialog_result = Some(Ok(Some(selected.clone())));

    app.browse_for_preset_editor_root();

    assert_eq!(
        app.shell.features.root_browser.last_browse_dialog_root,
        Some(root.clone())
    );
    assert_eq!(
        app.shell.features.presets.picker.editor.root_path,
        selected.display().to_string()
    );
    assert_eq!(
        app.shell
            .features
            .presets
            .catalog
            .preset("Rust")
            .expect("preset")
            .root_path,
        snapshot
    );
    assert_eq!(app.shell.runtime.root, root);
    assert_eq!(app.shell.runtime.query_state.query, "current query");
    assert!(!app.shell.worker_bus.catalog.in_progress);

    let manual = root.join("manual");
    fs::create_dir_all(&manual).expect("create manual root");
    app.shell.features.presets.picker.editor.root_path = manual.display().to_string();
    app.shell.features.root_browser.browse_dialog_result = Some(Ok(None));
    app.browse_for_preset_editor_root();
    assert_eq!(
        app.shell.features.presets.picker.editor.root_path,
        manual.display().to_string()
    );

    app.shell.features.root_browser.browse_dialog_result =
        Some(Err("dialog unavailable".to_string()));
    app.browse_for_preset_editor_root();
    assert_eq!(
        app.shell.features.presets.picker.editor.root_path,
        manual.display().to_string()
    );
    assert!(app
        .shell
        .features
        .presets
        .picker
        .editor
        .error
        .contains("Browse failed: dialog unavailable"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn named_root_browse_supports_selection_cancel_and_local_error() {
    let root = test_root("named-root-editor-browse-root");
    let selected = root.join("selected");
    let manual = root.join("manual");
    fs::create_dir_all(&selected).expect("create selected root");
    fs::create_dir_all(&manual).expect("create manual root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.features.presets.picker.open = true;
    app.open_named_root_manager();
    app.start_add_named_root();
    app.shell.features.presets.picker.named_roots.editor.name = "work".to_string();
    app.shell.features.root_browser.browse_dialog_result = Some(Ok(Some(selected.clone())));

    app.browse_for_named_root_editor_path();

    assert_eq!(
        app.shell.features.root_browser.last_browse_dialog_root,
        Some(root.clone())
    );
    assert_eq!(
        app.shell.features.presets.picker.named_roots.editor.path,
        selected.display().to_string()
    );
    assert!(app.shell.features.presets.catalog.named_roots.is_empty());

    app.shell.features.presets.picker.named_roots.editor.path = manual.display().to_string();
    app.shell.features.root_browser.browse_dialog_result = Some(Ok(None));
    app.browse_for_named_root_editor_path();
    assert_eq!(
        app.shell.features.presets.picker.named_roots.editor.path,
        manual.display().to_string()
    );

    app.shell.features.root_browser.browse_dialog_result =
        Some(Err("dialog unavailable".to_string()));
    app.browse_for_named_root_editor_path();
    assert_eq!(
        app.shell.features.presets.picker.named_roots.editor.path,
        manual.display().to_string()
    );
    assert!(app
        .shell
        .features
        .presets
        .picker
        .named_roots
        .editor
        .error
        .contains("Browse failed: dialog unavailable"));
    assert_eq!(
        app.shell.features.presets.picker.named_roots.editor.name,
        "work"
    );
    let _ = fs::remove_dir_all(root);
}

#[cfg(target_os = "windows")]
#[test]
fn preset_catalog_surfaces_hide_windows_verbatim_prefixes_from_existing_entries() {
    use crate::app::render_dialogs::preset_picker::preset_summary;
    use crate::search_catalog::NamedRoot;
    use std::path::PathBuf;

    let root = test_root("preset-catalog-verbatim-prefix-display");
    fs::create_dir_all(&root).expect("create root");
    let extended_path = PathBuf::from(r"\\?\C:\Users\tester\Documents");
    let expected_path = r"C:\Users\tester\Documents";
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell
        .features
        .presets
        .catalog
        .named_roots
        .push(NamedRoot {
            name: "Documents".to_string(),
            path: extended_path.clone(),
            extra: BTreeMap::new(),
        });
    let mut saved = preset("documents", &extended_path, "");
    saved.root_name = Some("Documents".to_string());
    app.shell.features.presets.catalog.presets.push(saved);
    app.shell.features.presets.picker.open = true;
    app.refresh_preset_picker_matches();

    assert_eq!(
        preset_summary(&app, 0),
        Some(format!("{expected_path}  —  (empty query)  —  Depth: All"))
    );

    app.start_selected_preset_edit();
    assert_eq!(
        app.shell.features.presets.picker.editor.root_path,
        expected_path
    );

    app.open_named_root_manager();
    let snapshot = serde_json::to_value(app.gui_surface_snapshot()).expect("serialize snapshot");
    assert_eq!(
        snapshot["preset_picker_dialogs"][0]["lines"],
        serde_json::json!([format!("Documents — {expected_path}")])
    );

    app.start_selected_named_root_edit();
    assert_eq!(
        app.shell.features.presets.picker.named_roots.editor.path,
        expected_path
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn escape_discards_the_preset_draft_and_returns_to_the_picker() {
    let root = test_root("preset-editor-cancel");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell
        .features
        .presets
        .catalog
        .save_preset(preset("Rust", &root, "ext:rs"))
        .expect("save preset");
    app.shell.features.presets.picker.open = true;
    app.refresh_preset_picker_matches();
    app.start_selected_preset_edit();
    app.shell.features.presets.picker.editor.query = "changed".to_string();

    run_shortcuts_frame(
        &mut app,
        true,
        vec![key_event(egui::Key::Escape, egui::Modifiers::NONE)],
    );

    assert!(app.shell.features.presets.picker.open);
    assert!(!app.shell.features.presets.picker.editor.open);
    assert_eq!(
        app.shell
            .features
            .presets
            .catalog
            .preset("Rust")
            .expect("preset")
            .query,
        "ext:rs"
    );
    assert!(!app.shell.worker_bus.catalog.in_progress);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn saving_a_preset_draft_queues_an_atomic_replace_without_applying_it() {
    use crate::app::worker::protocol::CatalogRequestKind;

    let root = test_root("preset-editor-save-request");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "current query".to_string());
    app.shell
        .features
        .presets
        .catalog
        .save_preset(preset("Rust", &root, "ext:rs"))
        .expect("save preset");
    app.shell.features.presets.picker.open = true;
    app.refresh_preset_picker_matches();
    app.start_selected_preset_edit();
    app.shell.features.presets.picker.editor.name = "Rust source".to_string();
    app.shell.features.presets.picker.editor.query = "dir:src ext:rs".to_string();
    app.shell.features.presets.picker.editor.entry_type = PresetEntryType::Folder;
    app.shell.features.presets.picker.editor.source = PresetSource::Filelist;
    app.shell.features.presets.picker.editor.regex = true;
    app.shell.features.presets.picker.editor.ignore_case = false;
    app.shell.features.presets.picker.editor.ignore_enabled = false;
    app.shell.features.presets.picker.editor.sort = PresetSortMode::SizeAsc;
    let (tx, rx) = mpsc::channel();
    app.shell.worker_bus.catalog.tx = tx;

    app.request_save_preset_edit();

    let request = rx.try_recv().expect("catalog replace request");
    match request.kind {
        CatalogRequestKind::ReplacePreset {
            original_name,
            preset,
        } => {
            assert_eq!(original_name, "Rust");
            assert_eq!(preset.name, "Rust source");
            assert_eq!(preset.query, "dir:src ext:rs");
            assert_eq!(preset.entry_type, PresetEntryType::Folder);
            assert_eq!(preset.source, PresetSource::Filelist);
            assert!(preset.regex);
            assert!(!preset.ignore_case);
            assert!(!preset.ignore_enabled);
            assert_eq!(preset.sort, PresetSortMode::SizeAsc);
        }
        _ => panic!("expected replace request"),
    }
    assert!(app.shell.worker_bus.catalog.in_progress);
    assert_eq!(app.shell.runtime.query_state.query, "current query");
    assert!(!app.shell.worker_bus.action.in_progress);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn adding_a_preset_starts_a_draft_from_the_current_pure_search_state() {
    let root = test_root("preset-editor-add-draft");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "dir:src ext:rs".to_string());
    app.shell.runtime.use_filelist = false;
    app.shell.runtime.use_regex = true;
    app.shell.runtime.ignore_case = false;
    app.shell.runtime.include_files = false;
    app.shell.runtime.include_dirs = true;
    app.shell.ui.ignore_list_enabled = false;
    app.shell.runtime.result_sort_mode = ResultSortMode::SizeAsc;
    app.shell.features.presets.picker.open = true;

    app.start_add_preset();

    let editor = &app.shell.features.presets.picker.editor;
    assert!(editor.open);
    assert!(editor.original_name.is_empty());
    assert!(editor.name.is_empty());
    assert_eq!(editor.root_path, normalize_path_for_display(&root));
    assert_eq!(editor.query, "dir:src ext:rs");
    assert_eq!(editor.entry_type, PresetEntryType::Folder);
    assert_eq!(editor.source, PresetSource::Walker);
    assert!(editor.regex);
    assert!(!editor.ignore_case);
    assert!(!editor.ignore_enabled);
    assert_eq!(editor.sort, PresetSortMode::SizeAsc);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn saving_a_new_preset_draft_queues_an_atomic_add_without_applying_it() {
    use crate::app::worker::protocol::CatalogRequestKind;

    let root = test_root("preset-editor-add-request");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "current query".to_string());
    app.shell.features.presets.picker.open = true;
    app.start_add_preset();
    app.shell.features.presets.picker.editor.name = "Current search".to_string();
    let (tx, rx) = mpsc::channel();
    app.shell.worker_bus.catalog.tx = tx;

    app.request_save_preset_edit();

    let request = rx.try_recv().expect("catalog add request");
    match request.kind {
        CatalogRequestKind::AddPreset { preset } => {
            assert_eq!(preset.name, "Current search");
            assert_eq!(preset.query, "current query");
            assert_eq!(preset.root_path, root);
        }
        _ => panic!("expected add request"),
    }
    assert!(app.shell.worker_bus.catalog.in_progress);
    assert_eq!(app.shell.runtime.query_state.query, "current query");
    assert!(!app.shell.worker_bus.action.in_progress);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn deleting_a_selected_preset_requires_confirmation_and_queues_atomic_remove() {
    use crate::app::worker::protocol::CatalogRequestKind;

    let root = test_root("preset-delete-request");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "current query".to_string());
    app.shell
        .features
        .presets
        .catalog
        .save_preset(preset("Rust", &root, "ext:rs"))
        .expect("save preset");
    app.shell.features.presets.picker.open = true;
    app.refresh_preset_picker_matches();
    let (tx, rx) = mpsc::channel();
    app.shell.worker_bus.catalog.tx = tx;

    app.start_selected_preset_delete();
    assert!(app.shell.features.presets.picker.confirm_delete);
    assert!(rx.try_recv().is_err());

    app.confirm_delete_preset();

    let request = rx.try_recv().expect("catalog remove request");
    match request.kind {
        CatalogRequestKind::RemovePreset { name } => assert_eq!(name, "Rust"),
        _ => panic!("expected remove request"),
    }
    assert!(app.shell.worker_bus.catalog.in_progress);
    assert!(app.shell.features.presets.catalog.preset("Rust").is_some());
    assert_eq!(app.shell.runtime.query_state.query, "current query");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn successful_preset_delete_response_updates_the_picker_without_changing_the_current_search() {
    use crate::app::worker::protocol::CatalogResponse;

    let root = test_root("preset-delete-success");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "current query".to_string());
    app.shell
        .features
        .presets
        .catalog
        .save_preset(preset("Rust", &root, "ext:rs"))
        .expect("save rust preset");
    app.shell
        .features
        .presets
        .catalog
        .save_preset(preset("Docs", &root, "ext:md"))
        .expect("save docs preset");
    app.shell.features.presets.picker.open = true;
    app.refresh_preset_picker_matches();
    app.shell.features.presets.picker.confirm_delete = true;
    app.shell.features.presets.picker.pending_deleted_name = Some("Rust".to_string());
    let (tx, rx) = mpsc::channel();
    app.shell.worker_bus.catalog.rx = rx;
    app.shell.worker_bus.catalog.pending_request_id = Some(9);
    app.shell.worker_bus.catalog.in_progress = true;
    let mut updated = SearchCatalog::default();
    updated
        .save_preset(preset("Docs", &root, "ext:md"))
        .expect("save remaining preset");
    tx.send(CatalogResponse {
        request_id: 9,
        result: Ok(updated),
    })
    .expect("send response");

    app.poll_catalog_response();

    assert!(!app.shell.features.presets.picker.confirm_delete);
    assert_eq!(app.preset_picker_match_names(), vec!["Docs"]);
    assert_eq!(app.shell.runtime.query_state.query, "current query");
    assert!(app.shell.runtime.notice.contains("Deleted preset: Rust"));
    assert!(!app.shell.worker_bus.action.in_progress);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn failed_preset_delete_response_keeps_confirmation_and_surfaces_the_error() {
    use crate::app::worker::protocol::CatalogResponse;

    let root = test_root("preset-delete-failure");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell
        .features
        .presets
        .catalog
        .save_preset(preset("Rust", &root, "ext:rs"))
        .expect("save preset");
    app.shell.features.presets.picker.open = true;
    app.refresh_preset_picker_matches();
    app.shell.features.presets.picker.confirm_delete = true;
    app.shell.features.presets.picker.pending_deleted_name = Some("Rust".to_string());
    let (tx, rx) = mpsc::channel();
    app.shell.worker_bus.catalog.rx = rx;
    app.shell.worker_bus.catalog.pending_request_id = Some(10);
    app.shell.worker_bus.catalog.in_progress = true;
    tx.send(CatalogResponse {
        request_id: 10,
        result: Err("catalog is read-only".to_string()),
    })
    .expect("send response");

    app.poll_catalog_response();

    assert!(app.shell.features.presets.picker.confirm_delete);
    assert!(app.shell.features.presets.catalog.preset("Rust").is_some());
    assert!(app
        .shell
        .features
        .presets
        .picker
        .error
        .contains("read-only"));
    assert!(!app.shell.worker_bus.catalog.in_progress);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn invalid_preset_draft_stays_local_and_does_not_start_the_worker() {
    let root = test_root("preset-editor-invalid");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell
        .features
        .presets
        .catalog
        .save_preset(preset("Rust", &root, "ext:rs"))
        .expect("save preset");
    app.shell.features.presets.picker.open = true;
    app.refresh_preset_picker_matches();
    app.start_selected_preset_edit();
    app.shell.features.presets.picker.editor.root_path = "relative/path".to_string();
    let (tx, rx) = mpsc::channel();
    app.shell.worker_bus.catalog.tx = tx;

    app.request_save_preset_edit();

    assert!(rx.try_recv().is_err());
    assert!(!app.shell.worker_bus.catalog.in_progress);
    assert!(app
        .shell
        .features
        .presets
        .picker
        .editor
        .error
        .contains("absolute"));
    assert!(app.shell.features.presets.picker.editor.open);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn successful_preset_edit_response_returns_to_picker_and_selects_renamed_preset() {
    use crate::app::worker::protocol::CatalogResponse;

    let root = test_root("preset-editor-save-success");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "current query".to_string());
    app.shell
        .features
        .presets
        .catalog
        .save_preset(preset("Rust", &root, "ext:rs"))
        .expect("save preset");
    app.shell.features.presets.picker.open = true;
    app.refresh_preset_picker_matches();
    app.start_selected_preset_edit();
    app.shell.features.presets.picker.editor.pending_saved_name = Some("Rust source".to_string());
    let (tx, rx) = mpsc::channel();
    app.shell.worker_bus.catalog.rx = rx;
    app.shell.worker_bus.catalog.pending_request_id = Some(7);
    app.shell.worker_bus.catalog.in_progress = true;
    let mut updated = SearchCatalog::default();
    updated
        .save_preset(preset("Rust source", &root, "dir:src ext:rs"))
        .expect("save renamed preset");
    tx.send(CatalogResponse {
        request_id: 7,
        result: Ok(updated),
    })
    .expect("send response");

    app.poll_catalog_response();

    assert!(!app.shell.features.presets.picker.editor.open);
    assert_eq!(app.preset_picker_match_names(), vec!["Rust source"]);
    assert_eq!(app.shell.features.presets.picker.selected_match, Some(0));
    assert_eq!(app.shell.runtime.query_state.query, "current query");
    assert!(app
        .shell
        .runtime
        .notice
        .contains("Saved preset: Rust source"));
    assert!(!app.shell.worker_bus.action.in_progress);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn failed_preset_edit_response_keeps_the_draft_for_correction() {
    use crate::app::worker::protocol::CatalogResponse;

    let root = test_root("preset-editor-save-failure");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell
        .features
        .presets
        .catalog
        .save_preset(preset("Rust", &root, "ext:rs"))
        .expect("save preset");
    app.shell.features.presets.picker.open = true;
    app.refresh_preset_picker_matches();
    app.start_selected_preset_edit();
    app.shell.features.presets.picker.editor.query = "edited".to_string();
    app.shell.features.presets.picker.editor.pending_saved_name = Some("Rust".to_string());
    let (tx, rx) = mpsc::channel();
    app.shell.worker_bus.catalog.rx = rx;
    app.shell.worker_bus.catalog.pending_request_id = Some(8);
    app.shell.worker_bus.catalog.in_progress = true;
    tx.send(CatalogResponse {
        request_id: 8,
        result: Err("search preset already exists: Docs".to_string()),
    })
    .expect("send response");

    app.poll_catalog_response();

    assert!(app.shell.features.presets.picker.editor.open);
    assert_eq!(app.shell.features.presets.picker.editor.query, "edited");
    assert!(app
        .shell
        .features
        .presets
        .picker
        .editor
        .error
        .contains("already exists"));
    assert!(!app.shell.worker_bus.catalog.in_progress);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn named_root_manager_adds_an_absolute_root_without_changing_the_current_search() {
    use crate::app::worker::protocol::CatalogRequestKind;

    let root = test_root("named-root-manager-add");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, "current query".to_string());
    app.shell.features.presets.picker.open = true;
    app.open_named_root_manager();
    app.start_add_named_root();
    app.shell.features.presets.picker.named_roots.editor.name = "work".to_string();
    app.shell.features.presets.picker.named_roots.editor.path =
        root.join("workspace").display().to_string();
    let (tx, rx) = mpsc::channel();
    app.shell.worker_bus.catalog.tx = tx;

    app.request_save_named_root();

    let request = rx.try_recv().expect("add named root request");
    match request.kind {
        CatalogRequestKind::AddNamedRoot { name, path } => {
            assert_eq!(name, "work");
            assert_eq!(path, root.join("workspace"));
        }
        _ => panic!("expected add named root request"),
    }
    assert!(app.shell.worker_bus.catalog.in_progress);
    assert_eq!(app.shell.runtime.query_state.query, "current query");
    assert!(!app.shell.worker_bus.action.in_progress);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn named_root_manager_edits_and_deletes_the_selected_root_through_catalog_requests() {
    use crate::app::worker::protocol::CatalogRequestKind;

    let root = test_root("named-root-manager-edit-delete");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell
        .features
        .presets
        .catalog
        .add_named_root("work", root.join("old"))
        .expect("add named root");
    app.shell.features.presets.picker.open = true;
    app.open_named_root_manager();
    app.start_selected_named_root_edit();
    app.shell.features.presets.picker.named_roots.editor.name = "workspace".to_string();
    app.shell.features.presets.picker.named_roots.editor.path =
        root.join("new").display().to_string();
    let (edit_tx, edit_rx) = mpsc::channel();
    app.shell.worker_bus.catalog.tx = edit_tx;

    app.request_save_named_root();

    match edit_rx.try_recv().expect("replace named root request").kind {
        CatalogRequestKind::ReplaceNamedRoot {
            original_name,
            name,
            path,
        } => {
            assert_eq!(original_name, "work");
            assert_eq!(name, "workspace");
            assert_eq!(path, root.join("new"));
        }
        _ => panic!("expected replace named root request"),
    }

    app.shell.worker_bus.catalog.clear_request();
    app.shell.features.presets.picker.named_roots.editor = Default::default();
    app.shell.features.presets.picker.named_roots.selected_index = Some(0);
    let (delete_tx, delete_rx) = mpsc::channel();
    app.shell.worker_bus.catalog.tx = delete_tx;
    app.start_selected_named_root_delete();
    assert!(app.shell.features.presets.picker.named_roots.confirm_delete);

    app.confirm_delete_named_root();

    match delete_rx
        .try_recv()
        .expect("remove named root request")
        .kind
    {
        CatalogRequestKind::RemoveNamedRoot { name } => assert_eq!(name, "work"),
        _ => panic!("expected remove named root request"),
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn named_root_manager_rejects_relative_paths_without_starting_the_worker() {
    let root = test_root("named-root-manager-invalid");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.features.presets.picker.open = true;
    app.open_named_root_manager();
    app.start_add_named_root();
    app.shell.features.presets.picker.named_roots.editor.name = "work".to_string();
    app.shell.features.presets.picker.named_roots.editor.path = "relative/path".to_string();
    let (tx, rx) = mpsc::channel();
    app.shell.worker_bus.catalog.tx = tx;

    app.request_save_named_root();

    assert!(rx.try_recv().is_err());
    assert!(!app.shell.worker_bus.catalog.in_progress);
    assert!(app
        .shell
        .features
        .presets
        .picker
        .named_roots
        .editor
        .error
        .contains("absolute"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn successful_named_root_rename_updates_an_open_preset_draft_reference() {
    use crate::app::state::PendingNamedRootOperation;
    use crate::app::worker::protocol::CatalogResponse;

    let root = test_root("named-root-manager-success");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell
        .features
        .presets
        .catalog
        .add_named_root("work", root.join("old"))
        .expect("add root");
    let mut linked = preset("Rust", &root.join("snapshot"), "ext:rs");
    linked.root_name = Some("work".to_string());
    app.shell
        .features
        .presets
        .catalog
        .save_preset(linked)
        .expect("save preset");
    app.shell.features.presets.picker.open = true;
    app.refresh_preset_picker_matches();
    app.start_selected_preset_edit();
    app.open_named_root_manager();
    app.shell
        .features
        .presets
        .picker
        .named_roots
        .pending_operation = Some(PendingNamedRootOperation::Save {
        original_name: Some("work".to_string()),
        saved_name: "workspace".to_string(),
    });
    let (tx, rx) = mpsc::channel();
    app.shell.worker_bus.catalog.rx = rx;
    app.shell.worker_bus.catalog.pending_request_id = Some(9);
    app.shell.worker_bus.catalog.in_progress = true;
    let mut updated = app.shell.features.presets.catalog.clone();
    updated
        .replace_named_root("work", "workspace", root.join("new"))
        .expect("rename root");
    tx.send(CatalogResponse {
        request_id: 9,
        result: Ok(updated),
    })
    .expect("send response");

    app.poll_catalog_response();

    assert_eq!(
        app.shell
            .features
            .presets
            .picker
            .editor
            .root_name
            .as_deref(),
        Some("workspace")
    );
    assert_eq!(
        app.shell.features.presets.picker.named_roots.selected_index,
        Some(0)
    );
    assert!(!app.shell.features.presets.picker.named_roots.editor.open);
    assert!(app
        .shell
        .runtime
        .notice
        .contains("Saved named root: workspace"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn escape_unwinds_named_root_editor_then_manager_without_closing_the_picker() {
    let root = test_root("named-root-manager-escape");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.features.presets.picker.open = true;
    app.open_named_root_manager();
    app.start_add_named_root();

    run_shortcuts_frame(
        &mut app,
        true,
        vec![key_event(egui::Key::Escape, egui::Modifiers::NONE)],
    );
    assert!(app.shell.features.presets.picker.named_roots.open);
    assert!(!app.shell.features.presets.picker.named_roots.editor.open);

    run_shortcuts_frame(
        &mut app,
        true,
        vec![key_event(egui::Key::Escape, egui::Modifiers::NONE)],
    );
    assert!(app.shell.features.presets.picker.open);
    assert!(!app.shell.features.presets.picker.named_roots.open);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn regression_gui_list_preset_and_named_root_selection_scrolls_into_view() {
    for named in [false, true] {
        let root = test_root("picker-list-scroll");
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        app.shell.features.presets.picker.open = true;
        app.shell.features.presets.picker.named_roots.open = named;
        for i in 0..60 {
            if named {
                app.shell.features.presets.catalog.named_roots.push(
                    crate::search_catalog::NamedRoot {
                        name: format!("Root {i:02}"),
                        path: root.join(i.to_string()),
                        extra: BTreeMap::new(),
                    },
                );
            } else {
                app.shell.features.presets.catalog.presets.push(preset(
                    &format!("Preset {i:02}"),
                    &root,
                    "",
                ));
            }
        }
        app.refresh_preset_picker_matches();
        app.shell.features.presets.picker.named_roots.selected_index = Some(0);
        let ctx = egui::Context::default();
        ctx.all_styles_mut(|style| style.scroll_animation = egui::style::ScrollAnimation::none());
        let surface = if named {
            "named-root-results"
        } else {
            "preset-picker-results"
        };
        let frame = |app: &mut FlistWalkerApp| {
            let _ = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000.0, 800.0),
                    )),
                    ..Default::default()
                },
                |ui| crate::app::render_dialogs::render_preset_picker_dialog(app, ui.ctx()),
            );
            ctx.data(|data| data.get_temp::<render_panels::ListScrollProbe>(egui::Id::new(surface)))
                .unwrap()
        };
        frame(&mut app);
        frame(&mut app);
        if named {
            app.move_named_root_selection(50);
        } else {
            app.move_preset_picker_selection(50);
        }
        frame(&mut app);
        let moved = frame(&mut app);
        assert!(moved.offset.y > 0.0, "{surface}");
        assert!(
            moved.viewport.intersects(moved.selected.unwrap()),
            "{surface} {moved:?}"
        );
    }
}

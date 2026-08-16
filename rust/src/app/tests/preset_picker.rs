use super::*;
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
    use crate::app::worker_protocol::CatalogResponse;

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
    use crate::app::worker_protocol::CatalogRequestKind;

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
        CatalogRequestKind::Load => panic!("expected replace request"),
    }
    assert!(app.shell.worker_bus.catalog.in_progress);
    assert_eq!(app.shell.runtime.query_state.query, "current query");
    assert!(!app.shell.worker_bus.action.in_progress);
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
    use crate::app::worker_protocol::CatalogResponse;

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
    use crate::app::worker_protocol::CatalogResponse;

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

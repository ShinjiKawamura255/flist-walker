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

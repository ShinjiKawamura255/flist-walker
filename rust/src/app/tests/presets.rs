use super::*;
use crate::search_catalog::{PresetEntryType, PresetSortMode, PresetSource, SearchPreset};
use std::collections::BTreeMap;

#[test]
fn tc_174_gui_applies_pure_search_preset_through_existing_state_transition() {
    let root = test_root("gui-preset-apply");
    std::fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell
        .features
        .presets
        .catalog
        .save_preset(SearchPreset {
            name: "Rust".to_string(),
            root_name: None,
            root_path: root.clone(),
            query: "ext:rs".to_string(),
            entry_type: PresetEntryType::File,
            source: PresetSource::Walker,
            regex: false,
            ignore_case: false,
            ignore_enabled: false,
            sort: PresetSortMode::NameAsc,
            extra: BTreeMap::new(),
        })
        .expect("save in-memory preset");
    app.shell.features.presets.selected_name = Some("rust".to_string());

    app.apply_selected_preset();

    assert_eq!(app.shell.runtime.query_state.query, "ext:rs");
    assert!(!app.shell.runtime.use_filelist);
    assert!(!app.shell.runtime.ignore_case);
    assert!(!app.shell.ui.ignore_list_enabled);
    assert!(app.shell.runtime.include_files);
    assert!(!app.shell.runtime.include_dirs);
    assert_eq!(app.shell.runtime.result_sort_mode, ResultSortMode::NameAsc);
    assert!(app.shell.indexing.in_progress);
    assert!(app.shell.runtime.notice.contains("Applied preset"));

    let _ = std::fs::remove_dir_all(root);
}

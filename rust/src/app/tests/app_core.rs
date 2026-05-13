use super::*;
use crate::search::filter_search_results;

#[test]
fn clear_query_and_selection_clears_state() {
    let root = test_root("clear");
    fs::create_dir_all(&root).expect("create dir");
    let file = root.join("a.txt");
    fs::write(&file, "x").expect("write file");

    let mut app = FlistWalkerApp::new(root.clone(), 50, "abc".to_string());
    app.shell.runtime.pinned_paths.insert(file.clone());
    app.shell.runtime.current_row = Some(0);
    app.shell.runtime.preview = "preview".to_string();

    app.clear_query_and_selection();

    assert!(app.shell.runtime.query_state.query.is_empty());
    assert!(app.shell.runtime.pinned_paths.is_empty());
    assert!(app.shell.ui.focus_query_requested);
    assert!(app
        .shell
        .runtime
        .notice
        .contains("Cleared selection and query"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn startup_requests_query_focus() {
    let root = test_root("startup-focus");
    fs::create_dir_all(&root).expect("create dir");
    let app = FlistWalkerApp::new(root.clone(), 50, String::new());
    assert!(app.shell.ui.focus_query_requested);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn startup_defaults_current_row_to_first_row_regression() {
    let root = test_root("startup-default-row");
    fs::create_dir_all(&root).expect("create dir");
    let app = FlistWalkerApp::new(root.clone(), 50, String::new());

    assert_eq!(app.shell.runtime.current_row, Some(0));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn startup_index_request_is_bound_to_active_tab() {
    let root = test_root("startup-index-tab-binding");
    fs::create_dir_all(&root).expect("create dir");
    let app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let req_id = app
        .shell
        .indexing
        .pending_request_id
        .expect("pending index request");
    let tab_id = app.current_tab_id().expect("active tab id");
    assert_eq!(
        app.shell.indexing.request_tabs.get(&req_id).copied(),
        Some(tab_id)
    );
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
    app.shell.ui.show_preview = false;
    app.mark_ui_state_dirty();
    app.persist_ui_state_to_path_now(&ui_state_path);

    let launch = FlistWalkerApp::load_launch_settings_from_path(&ui_state_path);
    assert!(!launch.show_preview);
    assert!(!app.shell.ui.ui_state_dirty);

    let _ = fs::remove_file(&ui_state_path);
    let _ = fs::remove_dir_all(&ui_state_dir);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn persist_ui_state_now_saves_ignore_list_enabled() {
    let root = test_root("persist-ignore-list-enabled");
    let ui_state_dir = test_root("persist-ignore-list-enabled-ui");
    let ui_state_path = ui_state_dir.join(".flistwalker_ui_state.json");
    fs::create_dir_all(&root).expect("create root");
    fs::create_dir_all(&ui_state_dir).expect("create ui state dir");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.ui.set_ignore_list_enabled(false);
    app.mark_ui_state_dirty();
    app.persist_ui_state_to_path_now(&ui_state_path);

    let launch = FlistWalkerApp::load_launch_settings_from_path(&ui_state_path);
    assert!(!launch.ignore_list_enabled);

    let _ = fs::remove_file(&ui_state_path);
    let _ = fs::remove_dir_all(&ui_state_dir);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn load_launch_settings_defaults_ignore_list_enabled_when_field_missing() {
    let ui_state_dir = test_root("load-ignore-list-enabled-default");
    let ui_state_path = ui_state_dir.join(".flistwalker_ui_state.json");
    fs::create_dir_all(&ui_state_dir).expect("create ui state dir");
    fs::write(&ui_state_path, "{}").expect("write minimal ui state");

    let launch = FlistWalkerApp::load_launch_settings_from_path(&ui_state_path);
    assert!(launch.ignore_list_enabled);

    let _ = fs::remove_file(&ui_state_path);
    let _ = fs::remove_dir_all(&ui_state_dir);
}

#[test]
fn persist_ui_state_now_saves_skipped_update_version() {
    let root = test_root("persist-skipped-update-version");
    let ui_state_dir = test_root("persist-skipped-update-version-ui");
    let ui_state_path = ui_state_dir.join(".flistwalker_ui_state.json");
    fs::create_dir_all(&root).expect("create root");
    fs::create_dir_all(&ui_state_dir).expect("create ui state dir");

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.features.update.state.skipped_target_version = Some("0.12.4".to_string());
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
    app.shell
        .features
        .update
        .state
        .suppress_check_failure_dialog = true;
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
    app.shell.runtime.results = vec![(file1, 0.0), (file2, 0.0)];
    app.shell.runtime.current_row = Some(0);
    app.shell.ui.scroll_to_current = false;

    app.move_row(1);

    assert_eq!(app.shell.runtime.current_row, Some(1));
    assert!(app.shell.ui.scroll_to_current);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn prefer_relative_display_is_enabled_for_filelist_source() {
    let root = test_root("prefer-relative-filelist");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.index.source = IndexSource::FileList(root.join("FileList.txt"));

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
fn app_defaults_use_filelist_on() {
    let root = test_root("default-use-filelist-on");
    fs::create_dir_all(&root).expect("create dir");
    let app = FlistWalkerApp::new(root.clone(), 50, String::new());
    assert!(app.shell.runtime.use_filelist);
    let _ = fs::remove_dir_all(&root);
}

use super::*;

#[test]
fn ctrl_r_starts_history_search_with_recent_entries_first() {
    let root = test_root("query-history-search-start");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.query_state.query = "first".to_string();
    app.mark_query_edited();
    app.update_results();
    commit_query_history_for_test(&mut app);
    app.shell.runtime.query_state.query = "second".to_string();
    app.mark_query_edited();
    app.update_results();
    commit_query_history_for_test(&mut app);
    app.shell.runtime.query_state.query = "draft".to_string();

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::R,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                ctrl: true,
                ..Default::default()
            },
        }],
    );

    assert!(app.shell.runtime.query_state.history_search_active);
    assert_eq!(
        app.shell.runtime.query_state.history_search_original_query,
        "draft"
    );
    assert_eq!(
        app.shell.runtime.query_state.history_search_results,
        vec!["second".to_string(), "first".to_string()]
    );
    assert_eq!(
        app.shell.runtime.query_state.history_search_current,
        Some(0)
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn query_history_skips_empty_and_consecutive_duplicates() {
    let root = test_root("query-history-dedup");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

    app.shell.runtime.query_state.query = String::new();
    app.mark_query_edited();
    app.update_results();
    commit_query_history_for_test(&mut app);
    app.shell.runtime.query_state.query = "same".to_string();
    app.mark_query_edited();
    app.update_results();
    commit_query_history_for_test(&mut app);
    app.shell.runtime.query_state.query = "same".to_string();
    app.mark_query_edited();
    app.update_results();
    commit_query_history_for_test(&mut app);
    app.shell.runtime.query_state.query = "same ".to_string();
    app.mark_query_edited();
    app.update_results();
    commit_query_history_for_test(&mut app);
    app.shell.runtime.query_state.query = "other".to_string();
    app.mark_query_edited();
    app.update_results();
    commit_query_history_for_test(&mut app);

    assert_eq!(
        app.shell
            .runtime
            .query_state
            .query_history
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["same".to_string(), "other".to_string()]
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn query_history_is_shared_across_tabs() {
    let root = test_root("query-history-tab-scoped");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.query_state.query = "tab-a".to_string();
    app.mark_query_edited();
    app.update_results();
    commit_query_history_for_test(&mut app);

    app.create_new_tab();
    assert_eq!(
        app.shell
            .runtime
            .query_state
            .query_history
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["tab-a".to_string()]
    );
    app.shell.runtime.query_state.query = "tab-b".to_string();
    app.mark_query_edited();
    app.update_results();
    commit_query_history_for_test(&mut app);

    app.switch_to_tab_index(0);
    assert_eq!(
        app.shell
            .runtime
            .query_state
            .query_history
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["tab-a".to_string(), "tab-b".to_string()]
    );
    assert_eq!(app.shell.runtime.query_state.query, "tab-a");

    app.switch_to_tab_index(1);
    assert_eq!(
        app.shell
            .runtime
            .query_state
            .query_history
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["tab-a".to_string(), "tab-b".to_string()]
    );
    assert_eq!(app.shell.runtime.query_state.query, "tab-b");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn root_change_resets_query_history_navigation_state() {
    let root_a = test_root("query-history-root-a");
    let root_b = test_root("query-history-root-b");
    fs::create_dir_all(&root_a).expect("create root a");
    fs::create_dir_all(&root_b).expect("create root b");
    let mut app = FlistWalkerApp::new(root_a.clone(), 50, String::new());
    app.shell.runtime.query_state.query = "first".to_string();
    app.mark_query_edited();
    app.update_results();
    commit_query_history_for_test(&mut app);
    app.shell.runtime.query_state.query = "second".to_string();
    app.mark_query_edited();
    app.update_results();
    commit_query_history_for_test(&mut app);
    app.shell.runtime.query_state.query = "draft".to_string();

    app.start_history_search();
    assert!(app.shell.runtime.query_state.history_search_active);

    app.apply_root_change(root_b.clone());

    assert_eq!(app.shell.runtime.root, root_b);
    assert!(app.shell.runtime.query_state.query_history_cursor.is_none());
    assert!(app.shell.runtime.query_state.query_history_draft.is_none());
    assert!(!app.shell.runtime.query_state.history_search_active);
    assert_eq!(
        app.shell
            .runtime
            .query_state
            .query_history
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["first".to_string(), "second".to_string()]
    );
    let _ = fs::remove_dir_all(&root_a);
    let _ = fs::remove_dir_all(&root_b);
}

#[test]
fn query_history_commits_only_final_query_after_typing_burst() {
    let root = test_root("query-history-burst");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

    for query in ["t", "te", "tes", "test"] {
        app.shell.runtime.query_state.query = query.to_string();
        app.mark_query_edited();
        app.update_results();
    }
    commit_query_history_for_test(&mut app);

    assert_eq!(
        app.shell
            .runtime
            .query_state
            .query_history
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["test".to_string()]
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn query_history_skips_ime_intermediate_text_until_composition_ends() {
    let root = test_root("query-history-ime");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

    app.shell.runtime.query_state.query = "t".to_string();
    app.shell.ui.ime_composition_active = true;
    app.mark_query_edited();
    app.update_results();
    commit_query_history_for_test(&mut app);

    app.shell.runtime.query_state.query = "て".to_string();
    app.mark_query_edited();
    app.update_results();
    commit_query_history_for_test(&mut app);

    assert!(app.shell.runtime.query_state.query_history.is_empty());

    app.shell.ui.ime_composition_active = false;
    app.shell.runtime.query_state.query = "テスト".to_string();
    app.mark_query_edited();
    app.update_results();
    commit_query_history_for_test(&mut app);

    assert_eq!(
        app.shell
            .runtime
            .query_state
            .query_history
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["テスト".to_string()]
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn query_history_keeps_only_the_latest_hundred_entries() {
    let root = test_root("query-history-max");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

    for i in 0..105 {
        app.shell.runtime.query_state.query = format!("query-{i:03}");
        app.mark_query_edited();
        app.update_results();
        commit_query_history_for_test(&mut app);
    }

    assert_eq!(
        app.shell.runtime.query_state.query_history.len(),
        FlistWalkerApp::QUERY_HISTORY_MAX
    );
    assert_eq!(
        app.shell
            .runtime
            .query_state
            .query_history
            .front()
            .map(String::as_str),
        Some("query-005")
    );
    assert_eq!(
        app.shell
            .runtime
            .query_state
            .query_history
            .back()
            .map(String::as_str),
        Some("query-104")
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn query_history_is_persisted_in_saved_tab_state() {
    let root = test_root("query-history-persist");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    for query in ["alpha", "beta", "gamma"] {
        app.shell.runtime.query_state.query = query.to_string();
        app.mark_query_edited();
        app.update_results();
        commit_query_history_for_test(&mut app);
    }

    let saved = app.saved_tab_state_from_app();
    assert_eq!(
        saved.query_history,
        vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()]
    );

    let restored = app.restored_tab_state(42, &saved);
    assert_eq!(
        restored
            .query_state
            .query_history
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        saved.query_history
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn query_history_is_saved_and_loaded_via_ui_state() {
    let root = test_root("query-history-ui-state");
    let ui_state_dir = test_root("query-history-ui-state-dir");
    let ui_state_path = ui_state_dir.join(".flistwalker_ui_state.json");
    fs::create_dir_all(&root).expect("create root");
    fs::create_dir_all(&ui_state_dir).expect("create ui state dir");
    let _ = fs::remove_file(&ui_state_path);

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    for query in ["alpha", "beta", "gamma"] {
        app.shell.runtime.query_state.query = query.to_string();
        app.mark_query_edited();
        app.update_results();
        commit_query_history_for_test(&mut app);
    }
    assert_eq!(
        app.shell
            .runtime
            .query_state
            .query_history
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()]
    );

    app.save_ui_state_to_path(&ui_state_path);
    assert!(ui_state_path.exists(), "ui state file should exist");

    let saved = FlistWalkerApp::load_ui_state_from_path(&ui_state_path);
    assert_eq!(
        saved.query_history,
        vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()]
    );

    let launch = FlistWalkerApp::load_launch_settings_from_path(&ui_state_path);
    assert_eq!(
        launch.query_history,
        vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()]
    );

    let _ = fs::remove_file(&ui_state_path);
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&ui_state_dir);
}

#[test]
fn query_history_persist_can_be_disabled_via_env() {
    let root = test_root("query-history-disabled");
    let ui_state_dir = test_root("query-history-disabled-dir");
    let ui_state_path = ui_state_dir.join(".flistwalker_ui_state.json");
    fs::create_dir_all(&root).expect("create root");
    fs::create_dir_all(&ui_state_dir).expect("create ui state dir");
    let _ = fs::remove_file(&ui_state_path);

    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    for query in ["alpha", "beta"] {
        app.shell.runtime.query_state.query = query.to_string();
        app.mark_query_edited();
        app.update_results();
        commit_query_history_for_test(&mut app);
    }
    app.save_ui_state_to_path_with_history_persist_disabled(&ui_state_path, true);

    let saved = FlistWalkerApp::load_ui_state_from_path(&ui_state_path);
    assert!(saved.query_history.is_empty());

    let launch = FlistWalkerApp::load_launch_settings_from_path_with_history_persist_disabled(
        &ui_state_path,
        true,
    );
    assert!(launch.query_history.is_empty());

    let _ = fs::remove_file(&ui_state_path);
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&ui_state_dir);
}

use super::*;

#[test]
fn query_history_shortcuts_navigate_previous_and_next() {
    let root = test_root("query-history-shortcuts");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.entries = Arc::new(vec![root.join("alpha.txt")]);
    app.query = "first".to_string();
    app.mark_query_edited();
    app.update_results();
    commit_query_history_for_test(&mut app);
    app.query = "second".to_string();
    app.mark_query_edited();
    app.update_results();
    commit_query_history_for_test(&mut app);
    app.query = "draft".to_string();

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::R,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                ctrl: true,
                ..Default::default()
            },
        }],
    );
    assert_eq!(app.query, "second");
    assert_eq!(app.query_history_cursor, Some(1));

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::R,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                ctrl: true,
                ..Default::default()
            },
        }],
    );
    assert_eq!(app.query, "first");
    assert_eq!(app.query_history_cursor, Some(0));

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::R,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                ctrl: true,
                shift: true,
                ..Default::default()
            },
        }],
    );
    assert_eq!(app.query, "second");
    assert_eq!(app.query_history_cursor, Some(1));

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::R,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                ctrl: true,
                shift: true,
                ..Default::default()
            },
        }],
    );
    assert_eq!(app.query, "draft");
    assert_eq!(app.query_history_cursor, None);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn query_history_skips_empty_and_consecutive_duplicates() {
    let root = test_root("query-history-dedup");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

    app.query = String::new();
    app.mark_query_edited();
    app.update_results();
    commit_query_history_for_test(&mut app);
    app.query = "same".to_string();
    app.mark_query_edited();
    app.update_results();
    commit_query_history_for_test(&mut app);
    app.query = "same".to_string();
    app.mark_query_edited();
    app.update_results();
    commit_query_history_for_test(&mut app);
    app.query = "same ".to_string();
    app.mark_query_edited();
    app.update_results();
    commit_query_history_for_test(&mut app);
    app.query = "other".to_string();
    app.mark_query_edited();
    app.update_results();
    commit_query_history_for_test(&mut app);

    assert_eq!(
        app.query_history.iter().cloned().collect::<Vec<_>>(),
        vec!["same".to_string(), "other".to_string()]
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn query_history_is_tab_scoped() {
    let root = test_root("query-history-tab-scoped");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.query = "tab-a".to_string();
    app.mark_query_edited();
    app.update_results();
    commit_query_history_for_test(&mut app);

    app.create_new_tab();
    assert!(app.query_history.is_empty());
    app.query = "tab-b".to_string();
    app.mark_query_edited();
    app.update_results();
    commit_query_history_for_test(&mut app);

    app.switch_to_tab_index(0);
    assert_eq!(
        app.query_history.iter().cloned().collect::<Vec<_>>(),
        vec!["tab-a".to_string()]
    );
    assert_eq!(app.query, "tab-a");

    app.switch_to_tab_index(1);
    assert_eq!(
        app.query_history.iter().cloned().collect::<Vec<_>>(),
        vec!["tab-b".to_string()]
    );
    assert_eq!(app.query, "tab-b");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn root_change_resets_query_history_navigation_state() {
    let root_a = test_root("query-history-root-a");
    let root_b = test_root("query-history-root-b");
    fs::create_dir_all(&root_a).expect("create root a");
    fs::create_dir_all(&root_b).expect("create root b");
    let mut app = FlistWalkerApp::new(root_a.clone(), 50, String::new());
    app.query = "first".to_string();
    app.mark_query_edited();
    app.update_results();
    commit_query_history_for_test(&mut app);
    app.query = "second".to_string();
    app.mark_query_edited();
    app.update_results();
    commit_query_history_for_test(&mut app);
    app.query = "draft".to_string();

    app.mark_query_edited();

    run_shortcuts_frame(
        &mut app,
        true,
        vec![egui::Event::Key {
            key: egui::Key::R,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                ctrl: true,
                ..Default::default()
            },
        }],
    );
    assert!(app.query_history_cursor.is_some());

    app.apply_root_change(root_b.clone());

    assert_eq!(app.root, root_b);
    assert!(app.query_history_cursor.is_none());
    assert!(app.query_history_draft.is_none());
    assert_eq!(
        app.query_history.iter().cloned().collect::<Vec<_>>(),
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
        app.query = query.to_string();
        app.mark_query_edited();
        app.update_results();
    }
    commit_query_history_for_test(&mut app);

    assert_eq!(
        app.query_history.iter().cloned().collect::<Vec<_>>(),
        vec!["test".to_string()]
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn query_history_skips_ime_intermediate_text_until_composition_ends() {
    let root = test_root("query-history-ime");
    fs::create_dir_all(&root).expect("create dir");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

    app.query = "t".to_string();
    app.ime_composition_active = true;
    app.mark_query_edited();
    app.update_results();
    commit_query_history_for_test(&mut app);

    app.query = "て".to_string();
    app.mark_query_edited();
    app.update_results();
    commit_query_history_for_test(&mut app);

    assert!(app.query_history.is_empty());

    app.ime_composition_active = false;
    app.query = "テスト".to_string();
    app.mark_query_edited();
    app.update_results();
    commit_query_history_for_test(&mut app);

    assert_eq!(
        app.query_history.iter().cloned().collect::<Vec<_>>(),
        vec!["テスト".to_string()]
    );
    let _ = fs::remove_dir_all(&root);
}

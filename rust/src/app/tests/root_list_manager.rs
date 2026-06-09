use super::*;
use std::sync::{Mutex, OnceLock};

static SAVED_ROOTS_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn saved_roots_test_scope(name: &str) -> (std::sync::MutexGuard<'static, ()>, PathBuf) {
    let guard = SAVED_ROOTS_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("saved roots test lock");
    let base = test_root(name);
    fs::create_dir_all(&base).expect("create saved roots test dir");
    FlistWalkerApp::set_saved_roots_file_path_override_for_test(Some(
        FlistWalkerApp::saved_roots_file_path_in(&base),
    ));
    (guard, base)
}

fn cleanup_saved_roots_test_scope(base: PathBuf) {
    FlistWalkerApp::set_saved_roots_file_path_override_for_test(None);
    let _ = fs::remove_dir_all(base);
}

#[test]
fn manage_root_list_uses_stable_native_viewport_contract() {
    let builder = FlistWalkerApp::manage_root_list_viewport_builder();

    assert_eq!(
        builder.title.as_deref(),
        Some(FlistWalkerApp::MANAGE_ROOT_LIST_VIEWPORT_TITLE)
    );
    assert_eq!(
        builder.inner_size,
        Some(FlistWalkerApp::MANAGE_ROOT_LIST_VIEWPORT_SIZE)
    );
    assert_eq!(
        FlistWalkerApp::manage_root_list_viewport_id(),
        egui::ViewportId::from_hash_of("flistwalker-manage-root-list")
    );
}

#[test]
fn manage_root_list_cancel_discards_draft_changes() {
    let (_guard, settings_base) = saved_roots_test_scope("manage-root-list-cancel-settings");
    let root = test_root("manage-root-list-cancel");
    let saved = root.join("saved");
    let added = root.join("added");
    fs::create_dir_all(&saved).expect("create saved");
    fs::create_dir_all(&added).expect("create added");
    let mut app = FlistWalkerApp::new(saved.clone(), 50, String::new());
    app.shell.features.root_browser.saved_roots = vec![saved.clone()];

    app.open_manage_root_list();
    app.shell.features.root_browser.manage_list.input_path = added.to_string_lossy().to_string();
    app.add_manage_root_list_input();
    app.cancel_manage_root_list();

    assert_eq!(app.shell.features.root_browser.saved_roots, vec![saved]);
    assert!(!app.shell.features.root_browser.manage_list.open);
    let _ = fs::remove_dir_all(&root);
    cleanup_saved_roots_test_scope(settings_base);
}

#[test]
fn manage_root_list_apply_commits_added_and_removed_roots() {
    let (_guard, settings_base) = saved_roots_test_scope("manage-root-list-apply-settings");
    let root = test_root("manage-root-list-apply");
    let saved = root.join("saved");
    let removed = root.join("removed");
    let added = root.join("added");
    fs::create_dir_all(&saved).expect("create saved");
    fs::create_dir_all(&removed).expect("create removed");
    fs::create_dir_all(&added).expect("create added");
    let mut app = FlistWalkerApp::new(saved.clone(), 50, String::new());
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
    app.apply_manage_root_list_changes();

    let saved_roots = &app.shell.features.root_browser.saved_roots;
    assert_eq!(saved_roots.len(), 2);
    assert!(saved_roots
        .iter()
        .any(|path| path_key(path) == path_key(&saved)));
    assert!(saved_roots
        .iter()
        .any(|path| path_key(path) == path_key(&added)));
    assert!(!saved_roots
        .iter()
        .any(|path| path_key(path) == path_key(&removed)));
    assert!(app.shell.features.root_browser.manage_list.open);
    let _ = fs::remove_dir_all(&root);
    cleanup_saved_roots_test_scope(settings_base);
}

#[test]
fn manage_root_list_ok_applies_and_closes() {
    let (_guard, settings_base) = saved_roots_test_scope("manage-root-list-ok-settings");
    let root = test_root("manage-root-list-ok");
    let saved = root.join("saved");
    let added = root.join("added");
    fs::create_dir_all(&saved).expect("create saved");
    fs::create_dir_all(&added).expect("create added");
    let mut app = FlistWalkerApp::new(saved.clone(), 50, String::new());
    app.shell.features.root_browser.saved_roots = vec![saved];

    app.open_manage_root_list();
    app.shell.features.root_browser.manage_list.input_path = added.to_string_lossy().to_string();
    app.add_manage_root_list_input();
    app.confirm_manage_root_list_changes();

    assert!(app
        .shell
        .features
        .root_browser
        .saved_roots
        .iter()
        .any(|path| path_key(path) == path_key(&added)));
    assert!(!app.shell.features.root_browser.manage_list.open);
    let _ = fs::remove_dir_all(&root);
    cleanup_saved_roots_test_scope(settings_base);
}

#[test]
fn manage_root_list_removing_default_root_clears_default_on_apply() {
    let (_guard, settings_base) = saved_roots_test_scope("manage-root-list-default-settings");
    let root = test_root("manage-root-list-default");
    let saved = root.join("saved");
    let kept = root.join("kept");
    fs::create_dir_all(&saved).expect("create saved");
    fs::create_dir_all(&kept).expect("create kept");
    let mut app = FlistWalkerApp::new(saved.clone(), 50, String::new());
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

    assert!(app.shell.features.root_browser.default_root.is_none());
    let _ = fs::remove_dir_all(&root);
    cleanup_saved_roots_test_scope(settings_base);
}

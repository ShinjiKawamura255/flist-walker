use super::*;
use std::sync::{Mutex, OnceLock};

static SAVED_ROOTS_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

struct SavedRootsTestScope {
    _guard: std::sync::MutexGuard<'static, ()>,
    settings_base: PathBuf,
}

impl Drop for SavedRootsTestScope {
    fn drop(&mut self) {
        FlistWalkerApp::set_saved_roots_file_path_override_for_test(None);
        let _ = fs::remove_dir_all(&self.settings_base);
    }
}

fn saved_roots_test_scope(name: &str) -> SavedRootsTestScope {
    let guard = SAVED_ROOTS_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let base = test_root(name);
    fs::create_dir_all(&base).expect("create saved roots test dir");
    FlistWalkerApp::set_saved_roots_file_path_override_for_test(Some(
        FlistWalkerApp::saved_roots_file_path_in(&base),
    ));
    SavedRootsTestScope {
        _guard: guard,
        settings_base: base,
    }
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
    let _scope = saved_roots_test_scope("manage-root-list-cancel-settings");
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
}

#[test]
fn manage_root_list_apply_commits_added_and_removed_roots() {
    let _scope = saved_roots_test_scope("manage-root-list-apply-settings");
    let root = test_root("manage-root-list-apply");
    let saved = root.join("saved");
    let removed = root.join("removed");
    let added = root.join("added");
    fs::create_dir_all(&saved).expect("create saved");
    fs::create_dir_all(&removed).expect("create removed");
    fs::create_dir_all(&added).expect("create added");
    let added_canonical =
        normalize_windows_path_buf(added.canonicalize().unwrap_or_else(|_| added.clone()));
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
        .any(|path| path_key(path) == path_key(&added_canonical)));
    assert!(!saved_roots
        .iter()
        .any(|path| path_key(path) == path_key(&removed)));
    assert!(app.shell.features.root_browser.manage_list.open);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn manage_root_list_ok_applies_and_closes() {
    let _scope = saved_roots_test_scope("manage-root-list-ok-settings");
    let root = test_root("manage-root-list-ok");
    let saved = root.join("saved");
    let added = root.join("added");
    fs::create_dir_all(&saved).expect("create saved");
    fs::create_dir_all(&added).expect("create added");
    let added_canonical =
        normalize_windows_path_buf(added.canonicalize().unwrap_or_else(|_| added.clone()));
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
        .any(|path| path_key(path) == path_key(&added_canonical)));
    assert!(!app.shell.features.root_browser.manage_list.open);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn manage_root_list_removing_default_root_clears_default_on_apply() {
    let _scope = saved_roots_test_scope("manage-root-list-default-settings");
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
}

#[test]
fn manage_root_list_edit_replaces_selected_draft_root_only() {
    let _scope = saved_roots_test_scope("manage-root-list-edit-settings");
    let root = test_root("manage-root-list-edit");
    let original = root.join("original");
    let replacement = root.join("replacement");
    fs::create_dir_all(&original).expect("create original");
    fs::create_dir_all(&replacement).expect("create replacement");
    let replacement_canonical =
        normalize_windows_path_buf(replacement.canonicalize().unwrap_or(replacement.clone()));
    let mut app = FlistWalkerApp::new(original.clone(), 50, String::new());
    app.shell.features.root_browser.saved_roots = vec![original.clone()];

    app.open_manage_root_list();
    app.select_manage_root_list_item(0);
    app.start_editing_manage_root_list_item();
    app.shell.features.root_browser.manage_list.edit_path =
        replacement.to_string_lossy().to_string();
    app.save_manage_root_list_edit();

    assert_eq!(
        app.shell.features.root_browser.manage_list.draft_roots,
        vec![replacement_canonical]
    );
    assert_eq!(app.shell.features.root_browser.saved_roots, vec![original]);
    assert!(app
        .shell
        .features
        .root_browser
        .manage_list
        .editing_index
        .is_none());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn manage_root_list_edit_requests_focus_and_select_all() {
    let _scope = saved_roots_test_scope("manage-root-list-edit-focus-settings");
    let root = test_root("manage-root-list-edit-focus");
    let original = root.join("original");
    fs::create_dir_all(&original).expect("create original");
    let mut app = FlistWalkerApp::new(original.clone(), 50, String::new());
    app.shell.features.root_browser.saved_roots = vec![original];

    app.open_manage_root_list();
    app.select_manage_root_list_item(0);
    app.start_editing_manage_root_list_item();

    let manage = &app.shell.features.root_browser.manage_list;
    assert!(manage.edit_focus_requested);
    assert!(manage.edit_select_all_requested);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn manage_root_list_edit_rejects_duplicate_and_keeps_editor_open() {
    let _scope = saved_roots_test_scope("manage-root-list-edit-duplicate-settings");
    let root = test_root("manage-root-list-edit-duplicate");
    let first = root.join("first");
    let second = root.join("second");
    fs::create_dir_all(&first).expect("create first");
    fs::create_dir_all(&second).expect("create second");
    let mut app = FlistWalkerApp::new(first.clone(), 50, String::new());
    app.shell.features.root_browser.saved_roots = vec![first.clone(), second.clone()];

    app.open_manage_root_list();
    app.select_manage_root_list_item(0);
    app.start_editing_manage_root_list_item();
    app.shell.features.root_browser.manage_list.edit_path = second.to_string_lossy().to_string();
    app.save_manage_root_list_edit();

    assert_eq!(
        app.shell.features.root_browser.manage_list.draft_roots,
        vec![first, second]
    );
    assert_eq!(
        app.shell.features.root_browser.manage_list.editing_index,
        Some(0)
    );
    assert_eq!(
        app.shell.features.root_browser.manage_list.edit_error,
        "Couldn't update the root. This folder is already in the list."
    );
    assert!(
        app.shell
            .features
            .root_browser
            .manage_list
            .edit_focus_requested
    );
    assert!(
        app.shell
            .features
            .root_browser
            .manage_list
            .edit_select_all_requested
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn manage_root_list_add_invalid_path_uses_field_error_and_refocuses_input() {
    let _scope = saved_roots_test_scope("manage-root-list-add-invalid-settings");
    let root = test_root("manage-root-list-add-invalid");
    fs::create_dir_all(&root).expect("create root");
    let invalid = root.join("missing");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());

    app.open_manage_root_list();
    app.shell.features.root_browser.manage_list.input_path = invalid.to_string_lossy().to_string();
    app.add_manage_root_list_input();

    let manage = &app.shell.features.root_browser.manage_list;
    assert_eq!(
        manage.add_error,
        format!(
            "Couldn't add the root. Folder not found: {}",
            invalid.display()
        )
    );
    assert!(manage.add_focus_requested);
    assert!(manage.add_select_all_requested);
    assert!(manage.notice.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn manage_root_list_input_change_clears_only_its_field_error() {
    let root = test_root("manage-root-list-clear-field-error");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.open_manage_root_list();
    app.shell.features.root_browser.manage_list.add_error = "add error".to_string();
    app.shell.features.root_browser.manage_list.edit_error = "edit error".to_string();

    app.clear_manage_root_list_add_error();

    assert!(app
        .shell
        .features
        .root_browser
        .manage_list
        .add_error
        .is_empty());
    assert_eq!(
        app.shell.features.root_browser.manage_list.edit_error,
        "edit error"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn manage_root_list_remove_mode_is_explicit_and_cancelable() {
    let _scope = saved_roots_test_scope("manage-root-list-remove-mode-settings");
    let root = test_root("manage-root-list-remove-mode");
    let first = root.join("first");
    let second = root.join("second");
    fs::create_dir_all(&first).expect("create first");
    fs::create_dir_all(&second).expect("create second");
    let mut app = FlistWalkerApp::new(first.clone(), 50, String::new());
    app.shell.features.root_browser.saved_roots = vec![first, second];

    app.open_manage_root_list();
    app.enter_manage_root_list_remove_mode();
    app.shell
        .features
        .root_browser
        .manage_list
        .selected_indices
        .insert(0);
    app.cancel_manage_root_list_remove_mode();

    let manage = &app.shell.features.root_browser.manage_list;
    assert!(!manage.remove_mode);
    assert!(manage.selected_indices.is_empty());
    assert_eq!(manage.draft_roots.len(), 2);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn manage_root_list_editing_default_root_follows_replacement_on_apply() {
    let _scope = saved_roots_test_scope("manage-root-list-edit-default-settings");
    let root = test_root("manage-root-list-edit-default");
    let original = root.join("original");
    let replacement = root.join("replacement");
    fs::create_dir_all(&original).expect("create original");
    fs::create_dir_all(&replacement).expect("create replacement");
    let replacement_canonical =
        normalize_windows_path_buf(replacement.canonicalize().unwrap_or(replacement.clone()));
    let mut app = FlistWalkerApp::new(original.clone(), 50, String::new());
    app.shell.features.root_browser.saved_roots = vec![original.clone()];
    app.shell.features.root_browser.default_root = Some(original);

    app.open_manage_root_list();
    app.select_manage_root_list_item(0);
    app.start_editing_manage_root_list_item();
    app.shell.features.root_browser.manage_list.edit_path =
        replacement.to_string_lossy().to_string();
    app.save_manage_root_list_edit();
    app.apply_manage_root_list_changes();

    assert_eq!(
        app.shell.features.root_browser.default_root,
        Some(replacement_canonical)
    );
    let _ = fs::remove_dir_all(&root);
}

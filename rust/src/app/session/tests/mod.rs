use super::*;
use std::env;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn ui_state_file_path_in_joins_base_directory() {
    let base = PathBuf::from("/tmp/flistwalker-settings");
    assert_eq!(
        FlistWalkerApp::ui_state_file_path_in(&base),
        base.join(".flistwalker_ui_state.json")
    );
}

#[test]
fn saved_roots_file_path_in_joins_base_directory() {
    let base = PathBuf::from("/tmp/flistwalker-settings");
    assert_eq!(
        FlistWalkerApp::saved_roots_file_path_in(&base),
        base.join(".flistwalker_roots.txt")
    );
}

fn temp_dir(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    env::temp_dir().join(format!("flistwalker-session-{name}-{nonce}"))
}

#[test]
fn migrate_or_legacy_ui_state_path_prefers_current_and_moves_legacy_when_missing() {
    let base = temp_dir("ui-state");
    let legacy_base = base.join("legacy");
    let current_base = base.join("current");
    fs::create_dir_all(&legacy_base).expect("create legacy");
    fs::create_dir_all(&current_base).expect("create current");
    let current_path = FlistWalkerApp::ui_state_file_path_in(&current_base);
    let legacy_path = FlistWalkerApp::ui_state_file_path_in(&legacy_base);
    fs::write(&legacy_path, "{\"ignore_list_enabled\":false}").expect("write legacy");

    let resolved =
        FlistWalkerApp::migrate_or_legacy_path(&current_path, std::slice::from_ref(&legacy_path));
    assert_eq!(resolved, current_path);
    assert!(current_path.exists());
    assert!(!legacy_path.exists());

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn migrate_or_legacy_saved_roots_path_leaves_existing_current_file_untouched() {
    let base = temp_dir("saved-roots");
    let legacy_base = base.join("legacy");
    let current_base = base.join("current");
    fs::create_dir_all(&legacy_base).expect("create legacy");
    fs::create_dir_all(&current_base).expect("create current");
    let current_path = FlistWalkerApp::saved_roots_file_path_in(&current_base);
    let legacy_path = FlistWalkerApp::saved_roots_file_path_in(&legacy_base);
    fs::write(&legacy_path, "legacy-root").expect("write legacy");
    fs::write(&current_path, "current-root").expect("write current");

    let resolved = FlistWalkerApp::migrate_or_legacy_saved_roots_path(&current_path);
    assert_eq!(resolved, current_path);
    assert!(current_path.exists());
    assert!(legacy_path.exists());

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn migrate_or_legacy_path_skips_missing_legacy_and_uses_next_one() {
    let base = temp_dir("migration-priority");
    let current_base = base.join("current");
    let missing_legacy_base = base.join("missing-legacy");
    let legacy_base = base.join("legacy");
    fs::create_dir_all(&current_base).expect("create current");
    fs::create_dir_all(&legacy_base).expect("create legacy");
    let current_path = FlistWalkerApp::ui_state_file_path_in(&current_base);
    let missing_legacy_path = FlistWalkerApp::ui_state_file_path_in(&missing_legacy_base);
    let legacy_path = FlistWalkerApp::ui_state_file_path_in(&legacy_base);
    fs::write(&legacy_path, "{\"ignore_list_enabled\":false}").expect("write legacy");

    let resolved = FlistWalkerApp::migrate_or_legacy_path(
        &current_path,
        &[missing_legacy_path, legacy_path.clone()],
    );
    assert_eq!(resolved, current_path);
    assert!(current_path.exists());
    assert!(!legacy_path.exists());

    let _ = fs::remove_dir_all(&base);
}

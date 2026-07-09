use super::*;
use crate::env_var_test_lock;
use std::ffi::OsString;
use std::sync::MutexGuard;
use std::time::{SystemTime, UNIX_EPOCH};

struct EnvRestore {
    vars: Vec<(&'static str, Option<OsString>)>,
}

impl EnvRestore {
    fn capture(names: &[&'static str]) -> Self {
        let vars = names
            .iter()
            .map(|name| (*name, env::var_os(name)))
            .collect::<Vec<_>>();
        Self { vars }
    }
}

impl Drop for EnvRestore {
    fn drop(&mut self) {
        for (name, value) in &self.vars {
            match value {
                Some(value) => env::set_var(name, value),
                None => env::remove_var(name),
            }
        }
    }
}

fn test_home(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    env::temp_dir().join(format!("fff-rs-runtime-config-{name}-{nonce}"))
}

fn locked_env() -> MutexGuard<'static, ()> {
    env_var_test_lock().lock().expect("env lock")
}

#[test]
fn seeds_and_writes_config_when_missing() {
    let _guard = locked_env();
    let home = test_home("seed");
    fs::create_dir_all(&home).expect("create home");
    let _restore = EnvRestore::capture(&[
        "HOME",
        "USERPROFILE",
        SEARCH_PARALLEL_THRESHOLD_ENV,
        SEARCH_THREADS_ENV,
        RESTORE_TABS_ENV,
        WALKER_MAX_ENTRIES_ENV,
        WINDOW_TRACE_PATH_ENV,
        WINDOW_TRACE_ENV,
        WINDOW_TRACE_VERBOSE_ENV,
        HISTORY_PERSIST_ENV,
        UPDATE_FEED_URL_ENV,
        UPDATE_ALLOW_SAME_VERSION_ENV,
        UPDATE_ALLOW_DOWNGRADE_ENV,
        DISABLE_SELF_UPDATE_ENV,
        FORCE_UPDATE_CHECK_FAILURE_ENV,
    ]);
    env::set_var("HOME", &home);
    env::set_var("USERPROFILE", &home);
    env::set_var(SEARCH_PARALLEL_THRESHOLD_ENV, "111");
    env::remove_var(SEARCH_THREADS_ENV);
    env::set_var(RESTORE_TABS_ENV, "1");
    env::set_var(WALKER_MAX_ENTRIES_ENV, "222");
    env::remove_var(WINDOW_TRACE_ENV);
    env::remove_var(WINDOW_TRACE_VERBOSE_ENV);
    env::remove_var(WINDOW_TRACE_PATH_ENV);
    env::remove_var(HISTORY_PERSIST_ENV);
    env::remove_var(UPDATE_FEED_URL_ENV);
    env::remove_var(UPDATE_ALLOW_SAME_VERSION_ENV);
    env::remove_var(UPDATE_ALLOW_DOWNGRADE_ENV);
    env::remove_var(DISABLE_SELF_UPDATE_ENV);
    env::remove_var(FORCE_UPDATE_CHECK_FAILURE_ENV);

    let path = runtime_config_file_path_in(&home);
    let config = RuntimeConfig::load_or_seed_at(Some(path.clone()));

    assert!(path.exists());
    assert_eq!(config.search_parallel_threshold, 111);
    assert!(config.restore_tabs_enabled);
    assert_eq!(config.walker_max_entries, 222);
    assert_eq!(
        env::var(SEARCH_PARALLEL_THRESHOLD_ENV).expect("env set"),
        "111"
    );
    assert_eq!(env::var(RESTORE_TABS_ENV).expect("env set"), "1");
    assert_eq!(env::var(WALKER_MAX_ENTRIES_ENV).expect("env set"), "222");
    let text = fs::read_to_string(&path).expect("read config");
    let saved_json: serde_json::Value = serde_json::from_str(&text).expect("parse config");
    let saved = saved_json.as_object().expect("object config");
    assert_eq!(
        saved
            .get("search_parallel_threshold")
            .and_then(|value| value.as_u64()),
        Some(111)
    );
    assert!(!saved.contains_key("search_threads"));
    assert_eq!(
        saved
            .get("walker_max_entries")
            .and_then(|value| value.as_u64()),
        Some(222)
    );
    assert_eq!(
        saved
            .get("restore_tabs_enabled")
            .and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        saved
            .get("history_persist_disabled")
            .and_then(|value| value.as_bool()),
        Some(false)
    );
    assert_eq!(
        saved
            .get("emacs_keybindings_enabled")
            .and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        saved
            .get("tab_pin_moves_to_next_row")
            .and_then(|value| value.as_bool()),
        Some(false)
    );
    assert!(!saved.contains_key("search_threads"));
    assert!(!saved.contains_key("walker_threads"));
    assert!(!saved.contains_key("window_trace_enabled"));
    assert!(!saved.contains_key("window_trace_verbose"));
    assert!(!saved.contains_key("window_trace_path"));
    assert!(!saved.contains_key("update_feed_url"));
    assert!(!saved.contains_key("update_allow_same_version"));
    assert!(!saved.contains_key("update_allow_downgrade"));
    assert!(!saved.contains_key("disable_self_update"));
    assert!(!saved.contains_key("force_update_check_failure"));
    assert!(!saved.contains_key("developer"));

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn seeds_default_user_config_values_when_missing() {
    let _guard = locked_env();
    let home = test_home("seed-user-defaults");
    fs::create_dir_all(&home).expect("create home");
    let _restore = EnvRestore::capture(&[
        "HOME",
        "USERPROFILE",
        SEARCH_PARALLEL_THRESHOLD_ENV,
        SEARCH_THREADS_ENV,
        WALKER_MAX_ENTRIES_ENV,
        WINDOW_TRACE_PATH_ENV,
        WINDOW_TRACE_ENV,
        WINDOW_TRACE_VERBOSE_ENV,
        HISTORY_PERSIST_ENV,
        RESTORE_TABS_ENV,
        UPDATE_FEED_URL_ENV,
        UPDATE_ALLOW_SAME_VERSION_ENV,
        UPDATE_ALLOW_DOWNGRADE_ENV,
        DISABLE_SELF_UPDATE_ENV,
        FORCE_UPDATE_CHECK_FAILURE_ENV,
    ]);
    env::set_var("HOME", &home);
    env::set_var("USERPROFILE", &home);
    env::remove_var(SEARCH_PARALLEL_THRESHOLD_ENV);
    env::remove_var(SEARCH_THREADS_ENV);
    env::remove_var(WALKER_MAX_ENTRIES_ENV);
    env::remove_var(WINDOW_TRACE_ENV);
    env::remove_var(WINDOW_TRACE_VERBOSE_ENV);
    env::remove_var(WINDOW_TRACE_PATH_ENV);
    env::remove_var(HISTORY_PERSIST_ENV);
    env::remove_var(RESTORE_TABS_ENV);
    env::remove_var(UPDATE_FEED_URL_ENV);
    env::remove_var(UPDATE_ALLOW_SAME_VERSION_ENV);
    env::remove_var(UPDATE_ALLOW_DOWNGRADE_ENV);
    env::remove_var(DISABLE_SELF_UPDATE_ENV);
    env::remove_var(FORCE_UPDATE_CHECK_FAILURE_ENV);

    let path = runtime_config_file_path_in(&home);
    let _config = RuntimeConfig::load_or_seed_at(Some(path.clone()));

    let text = fs::read_to_string(&path).expect("read config");
    let saved_json: serde_json::Value = serde_json::from_str(&text).expect("parse config");
    let saved = saved_json.as_object().expect("object config");
    assert_eq!(
        saved
            .get("walker_max_entries")
            .and_then(|value| value.as_u64()),
        Some(WALKER_MAX_ENTRIES_DEFAULT as u64)
    );
    assert_eq!(
        saved
            .get("history_persist_disabled")
            .and_then(|value| value.as_bool()),
        Some(false)
    );
    assert_eq!(
        saved
            .get("restore_tabs_enabled")
            .and_then(|value| value.as_bool()),
        Some(false)
    );
    assert_eq!(
        saved
            .get("emacs_keybindings_enabled")
            .and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        saved
            .get("tab_pin_moves_to_next_row")
            .and_then(|value| value.as_bool()),
        Some(false)
    );
    assert_eq!(saved.len(), 5);

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn seeds_keep_explicit_default_env_values_in_generated_config() {
    let _guard = locked_env();
    let home = test_home("seed-defaults");
    fs::create_dir_all(&home).expect("create home");
    let _restore = EnvRestore::capture(&[
        "HOME",
        "USERPROFILE",
        SEARCH_PARALLEL_THRESHOLD_ENV,
        SEARCH_THREADS_ENV,
        WINDOW_TRACE_PATH_ENV,
        WINDOW_TRACE_ENV,
        RESTORE_TABS_ENV,
        UPDATE_FEED_URL_ENV,
    ]);
    env::set_var("HOME", &home);
    env::set_var("USERPROFILE", &home);
    env::set_var(
        SEARCH_PARALLEL_THRESHOLD_ENV,
        SEARCH_PARALLEL_THRESHOLD_DEFAULT.to_string(),
    );
    env::set_var(SEARCH_THREADS_ENV, default_search_threads().to_string());
    env::set_var(WINDOW_TRACE_ENV, "0");
    env::remove_var(WINDOW_TRACE_PATH_ENV);
    env::set_var(RESTORE_TABS_ENV, "false");
    env::set_var(UPDATE_FEED_URL_ENV, DEFAULT_UPDATE_FEED_URL);

    let path = runtime_config_file_path_in(&home);
    let _config = RuntimeConfig::load_or_seed_at(Some(path.clone()));
    let text = fs::read_to_string(&path).expect("read config");
    let saved_json: serde_json::Value = serde_json::from_str(&text).expect("parse config");
    let saved = saved_json.as_object().expect("object config");
    assert_eq!(
        saved
            .get("search_parallel_threshold")
            .and_then(|value| value.as_u64()),
        Some(SEARCH_PARALLEL_THRESHOLD_DEFAULT as u64)
    );
    assert_eq!(
        saved.get("search_threads").and_then(|value| value.as_u64()),
        Some(default_search_threads() as u64)
    );
    assert_eq!(
        saved
            .get("window_trace_enabled")
            .and_then(|value| value.as_bool()),
        Some(false)
    );
    assert_eq!(
        saved
            .get("restore_tabs_enabled")
            .and_then(|value| value.as_bool()),
        Some(false)
    );
    assert_eq!(
        saved
            .get("update_feed_url")
            .and_then(|value| value.as_str()),
        Some(DEFAULT_UPDATE_FEED_URL)
    );

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn existing_config_overrides_current_env_values() {
    let _guard = locked_env();
    let home = test_home("override");
    fs::create_dir_all(&home).expect("create home");
    let _restore = EnvRestore::capture(&[
        "HOME",
        "USERPROFILE",
        SEARCH_PARALLEL_THRESHOLD_ENV,
        RESTORE_TABS_ENV,
        WINDOW_TRACE_PATH_ENV,
    ]);
    env::set_var("HOME", &home);
    env::set_var("USERPROFILE", &home);
    env::set_var(SEARCH_PARALLEL_THRESHOLD_ENV, "999");
    env::set_var(RESTORE_TABS_ENV, "1");

    let config = RuntimeConfig {
        search_parallel_threshold: 7,
        restore_tabs_enabled: false,
        ..RuntimeConfig::default()
    };
    let path = runtime_config_file_path_in(&home);
    config.save_to_path(&path).expect("save config");

    let loaded = RuntimeConfig::load_or_seed_at(Some(path));
    assert_eq!(loaded.search_parallel_threshold, 7);
    assert!(!loaded.restore_tabs_enabled);
    assert_eq!(
        env::var(SEARCH_PARALLEL_THRESHOLD_ENV).expect("env set"),
        "7"
    );
    assert_eq!(env::var(RESTORE_TABS_ENV).expect("env set"), "0");

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn settings_base_dir_uses_platform_specific_settings_directory() {
    let _guard = locked_env();
    let home = test_home("base-dir");
    fs::create_dir_all(&home).expect("create home");
    let _restore = EnvRestore::capture(&["HOME", "USERPROFILE", "LOCALAPPDATA", "APPDATA"]);
    env::set_var("HOME", &home);
    env::set_var("USERPROFILE", &home);
    env::set_var("LOCALAPPDATA", &home);
    env::set_var("APPDATA", &home);

    #[cfg(windows)]
    {
        let expected = home.join(WINDOWS_SETTINGS_DIR_NAME);
        assert_eq!(settings_base_dir().as_deref(), Some(expected.as_path()));
    }

    #[cfg(not(windows))]
    {
        assert_eq!(settings_base_dir(), Some(home.join(UNIX_SETTINGS_DIR_NAME)));
    }

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn legacy_settings_base_dirs_include_home_directory_for_transition_migration() {
    let _guard = locked_env();
    let home = test_home("legacy-base-dirs");
    fs::create_dir_all(&home).expect("create home");
    let _restore = EnvRestore::capture(&["HOME", "USERPROFILE", "LOCALAPPDATA", "APPDATA"]);
    env::set_var("HOME", &home);
    env::set_var("USERPROFILE", &home);
    env::set_var("LOCALAPPDATA", &home);
    env::set_var("APPDATA", &home);

    #[cfg(windows)]
    let legacy_base = home.join(WINDOWS_SETTINGS_DIR_NAME);
    #[cfg(not(windows))]
    let legacy_base = home.join(UNIX_SETTINGS_DIR_NAME);
    let legacy_paths = legacy_runtime_config_file_paths(&runtime_config_file_path_in(&legacy_base));
    assert!(legacy_paths
        .iter()
        .any(|path| path == &runtime_config_file_path_in(&home)));

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn load_runtime_config_from_path_handles_missing_field_defaults() {
    let _guard = locked_env();
    let home = test_home("defaults");
    fs::create_dir_all(&home).expect("create home");
    let path = home.join(RUNTIME_CONFIG_FILE_NAME);
    fs::write(&path, "{}").expect("write config");

    let loaded = load_runtime_config_from_path(&path).expect("load config");
    assert_eq!(
        loaded.search_parallel_threshold,
        SEARCH_PARALLEL_THRESHOLD_DEFAULT
    );
    assert_eq!(loaded.walker_max_entries, WALKER_MAX_ENTRIES_DEFAULT);
    assert!(loaded.emacs_keybindings_enabled);
    assert!(!loaded.tab_pin_moves_to_next_row);
    assert_eq!(loaded.developer, DeveloperRuntimeConfig::default());

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn load_runtime_config_adds_missing_user_config_values_to_existing_file() {
    let _guard = locked_env();
    let home = test_home("backfill-user-defaults");
    fs::create_dir_all(&home).expect("create home");
    let path = home.join(RUNTIME_CONFIG_FILE_NAME);
    fs::write(&path, "{}").expect("write config");

    let loaded = load_runtime_config_from_path(&path).expect("load config");

    assert_eq!(loaded.walker_max_entries, WALKER_MAX_ENTRIES_DEFAULT);
    assert!(!loaded.history_persist_disabled);
    assert!(!loaded.restore_tabs_enabled);
    assert!(loaded.emacs_keybindings_enabled);
    assert!(!loaded.tab_pin_moves_to_next_row);
    let text = fs::read_to_string(&path).expect("read backfilled config");
    let saved_json: serde_json::Value = serde_json::from_str(&text).expect("parse config");
    let saved = saved_json.as_object().expect("object config");
    assert_eq!(
        saved
            .get("walker_max_entries")
            .and_then(|value| value.as_u64()),
        Some(WALKER_MAX_ENTRIES_DEFAULT as u64)
    );
    assert_eq!(
        saved
            .get("history_persist_disabled")
            .and_then(|value| value.as_bool()),
        Some(false)
    );
    assert_eq!(
        saved
            .get("restore_tabs_enabled")
            .and_then(|value| value.as_bool()),
        Some(false)
    );
    assert_eq!(
        saved
            .get("emacs_keybindings_enabled")
            .and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        saved
            .get("tab_pin_moves_to_next_row")
            .and_then(|value| value.as_bool()),
        Some(false)
    );
    assert_eq!(saved.len(), 5);

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn developer_config_loads_but_is_not_seeded() {
    let _guard = locked_env();
    let home = test_home("developer-config");
    fs::create_dir_all(&home).expect("create home");
    let path = home.join(RUNTIME_CONFIG_FILE_NAME);
    fs::write(
        &path,
        r#"{
  "developer": {
    "walker_metrics": true,
    "walker_metrics_log_path": "D:/tmp/flistwalker-walker-metrics.log",
    "walker_adaptive_initial_limit": 4,
    "walker_adaptive_max_limit": 8
  }
}"#,
    )
    .expect("write config");

    let loaded = load_runtime_config_from_path(&path).expect("load config");

    assert!(loaded.developer.walker_metrics);
    assert_eq!(
        loaded.developer.walker_metrics_log_path,
        "D:/tmp/flistwalker-walker-metrics.log"
    );
    assert_eq!(loaded.developer.walker_adaptive_initial_limit, Some(4));
    assert_eq!(loaded.developer.walker_adaptive_max_limit, Some(8));

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn migrate_file_if_needed_moves_legacy_file_into_current_location() {
    let _guard = locked_env();
    let base = test_home("migrate");
    let legacy_base = base.join("legacy");
    let current_base = base.join("current");
    fs::create_dir_all(&legacy_base).expect("create legacy dir");
    fs::create_dir_all(&current_base).expect("create current dir");
    let legacy_path = runtime_config_file_path_in(&legacy_base);
    let current_path = runtime_config_file_path_in(&current_base);
    fs::write(&legacy_path, "{\"walker_threads\":7}").expect("write legacy config");

    assert!(migrate_file_if_needed(&current_path, &legacy_path));
    assert!(current_path.exists());
    assert!(!legacy_path.exists());
    let loaded = load_runtime_config_from_path(&current_path).expect("load migrated config");
    assert_eq!(loaded.walker_max_entries, WALKER_MAX_ENTRIES_DEFAULT);
    let migrated_text = fs::read_to_string(&current_path).expect("read migrated config");
    assert!(!migrated_text.contains("walker_threads"));

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn migrate_file_if_needed_does_not_overwrite_existing_current_file() {
    let _guard = locked_env();
    let base = test_home("migrate-existing");
    let legacy_base = base.join("legacy");
    let current_base = base.join("current");
    fs::create_dir_all(&legacy_base).expect("create legacy dir");
    fs::create_dir_all(&current_base).expect("create current dir");
    let legacy_path = runtime_config_file_path_in(&legacy_base);
    let current_path = runtime_config_file_path_in(&current_base);
    fs::write(&legacy_path, "{\"walker_threads\":7}").expect("write legacy config");
    fs::write(&current_path, "{\"walker_threads\":9}").expect("write current config");

    assert!(!migrate_file_if_needed(&current_path, &legacy_path));
    assert!(current_path.exists());
    assert!(legacy_path.exists());
    let loaded = load_runtime_config_from_path(&current_path).expect("load current config");
    assert_eq!(loaded.walker_max_entries, WALKER_MAX_ENTRIES_DEFAULT);
    let current_text = fs::read_to_string(&current_path).expect("read current config");
    assert!(!current_text.contains("walker_threads"));

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn load_runtime_config_removes_deprecated_walker_options_from_existing_file() {
    let _guard = locked_env();
    let home = test_home("deprecated-walker-options");
    fs::create_dir_all(&home).expect("create home");
    let path = home.join(RUNTIME_CONFIG_FILE_NAME);
    fs::write(
        &path,
        r#"{
  "walker_threads": 7,
  "walker_max_entries": 321,
  "developer": {
    "walker_backend": "jwalk",
    "walker_metrics": true
  }
}"#,
    )
    .expect("write config");

    let loaded = load_runtime_config_from_path(&path).expect("load config");

    assert_eq!(loaded.walker_max_entries, 321);
    assert!(loaded.developer.walker_metrics);
    let text = fs::read_to_string(&path).expect("read cleaned config");
    assert!(!text.contains("walker_threads"));
    assert!(!text.contains("walker_backend"));
    assert!(text.contains("walker_metrics"));

    let _ = fs::remove_dir_all(&home);
}

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

#[test]
fn editable_settings_save_preserves_unknown_json_and_defers_effective_config() {
    let base = test_home("editable-preserve");
    fs::create_dir_all(&base).expect("create fixture directory");
    let path = base.join("settings.json");
    fs::write(
        &path,
        r#"{"restore_tabs_enabled":false,"walker_max_entries":500000,"developer":{"walker_metrics":true,"future_option":"keep"},"future_root":{"nested":[1,2,3]}}"#,
    )
    .expect("write fixture");
    let effective_before = current_runtime_config();
    let original = read_editable_settings(&path).expect("read draft");
    assert!(!original.values.restore_tabs_enabled);
    let mut updated = original.values.clone();
    updated.restore_tabs_enabled = true;
    updated.history_persist_disabled = true;
    updated.walker_max_entries = 17;
    let saved = save_editable_settings(&path, &original, &updated).expect("save draft");
    assert_eq!(saved.values, updated);
    let json: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).expect("saved bytes")).expect("saved JSON");
    assert_eq!(json["future_root"]["nested"], serde_json::json!([1, 2, 3]));
    assert_eq!(json["developer"]["future_option"], "keep");
    assert_eq!(json["developer"]["walker_metrics"], true);
    assert_eq!(json["restore_tabs_enabled"], true);
    assert_eq!(json["history_persist_disabled"], true);
    assert_eq!(json["walker_max_entries"], 17);
    assert_eq!(current_runtime_config(), effective_before);
    let next_launch = load_runtime_config_from_path(&path).expect("next launch config");
    assert!(next_launch.restore_tabs_enabled);
    assert!(next_launch.history_persist_disabled);
    assert_eq!(next_launch.walker_max_entries, 17);
    fs::remove_dir_all(base).expect("cleanup fixture");
}

#[test]
fn editable_settings_rejects_oversized_valid_json_without_changing_it() {
    let base = test_home("editable-oversized");
    fs::create_dir_all(&base).expect("create fixture directory");
    let path = base.join("settings.json");
    let prefix = r#"{"future_blob":""#;
    let suffix = r#""}"#;
    let bytes = format!(
        "{prefix}{}{suffix}",
        "x".repeat(64 * 1024 + 1 - prefix.len() - suffix.len())
    )
    .into_bytes();
    assert_eq!(bytes.len(), 64 * 1024 + 1);
    fs::write(&path, &bytes).expect("write oversized valid JSON");
    let Err(error) = read_editable_settings(&path) else {
        panic!("GUI editor must reject oversized JSON");
    };
    assert!(error.to_string().contains("64 KiB"), "{error:#}");
    assert_eq!(fs::read(&path).expect("original bytes"), bytes);
    fs::remove_dir_all(base).expect("cleanup fixture");
}

#[test]
fn editable_settings_near_limit_keeps_unknown_key_and_rejects_oversized_save() {
    let base = test_home("editable-size-boundary");
    fs::create_dir_all(&base).expect("create fixture directory");
    let path = base.join("settings.json");
    let prefix = r#"{"future_blob":""#;
    let suffix = r#""}"#;
    let bytes = format!(
        "{prefix}{}{suffix}",
        "x".repeat(64 * 1024 - 512 - prefix.len() - suffix.len())
    )
    .into_bytes();
    fs::write(&path, &bytes).expect("write near-limit JSON");
    let baseline = read_editable_settings(&path).expect("near-limit JSON is accepted");
    let mut draft = baseline.values.clone();
    draft.restore_tabs_enabled = true;
    let saved = save_editable_settings(&path, &baseline, &draft)
        .expect("save near-limit JSON with unknown key");
    assert_eq!(saved.values, draft);
    let current: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).expect("saved JSON")).expect("valid JSON");
    assert_eq!(
        current["future_blob"].as_str().unwrap().len(),
        64 * 1024 - 512 - prefix.len() - suffix.len()
    );
    assert_eq!(current["restore_tabs_enabled"], true);

    let exact_cap = format!(
        "{prefix}{}{suffix}",
        "y".repeat(64 * 1024 - prefix.len() - suffix.len())
    )
    .into_bytes();
    assert_eq!(exact_cap.len(), 64 * 1024);
    fs::write(&path, &exact_cap).expect("write exact-limit JSON");
    let baseline = read_editable_settings(&path).expect("exact-limit JSON is accepted for reading");
    let mut draft = baseline.values.clone();
    draft.restore_tabs_enabled = true;
    let Err(error) = save_editable_settings(&path, &baseline, &draft) else {
        panic!("expanded JSON must not exceed GUI editor limit");
    };
    assert!(error.to_string().contains("64 KiB"), "{error:#}");
    assert_eq!(fs::read(&path).expect("original bytes"), exact_cap);
    fs::remove_dir_all(base).expect("cleanup fixture");
}

#[test]
fn editable_settings_detects_external_change_without_overwriting_it() {
    let base = test_home("editable-conflict");
    fs::create_dir_all(&base).expect("create fixture directory");
    let path = base.join("settings.json");
    fs::write(&path, r#"{"restore_tabs_enabled":false}"#).expect("write fixture");
    let original = read_editable_settings(&path).expect("read draft");
    let external = r#"{"restore_tabs_enabled":false,"external":"new"}"#;
    fs::write(&path, external).expect("external edit");
    let mut updated = original.values.clone();
    updated.restore_tabs_enabled = true;
    assert!(save_editable_settings(&path, &original, &updated).is_err());
    assert_eq!(
        fs::read_to_string(&path).expect("read after conflict"),
        external
    );
    fs::remove_dir_all(base).expect("cleanup fixture");
}

#[test]
fn editable_settings_rejects_invalid_json_and_zero_limit() {
    let base = test_home("editable-invalid");
    fs::create_dir_all(&base).expect("create fixture directory");
    let path = base.join("settings.json");
    fs::write(&path, "{").expect("write invalid fixture");
    assert!(read_editable_settings(&path).is_err());
    assert_eq!(fs::read_to_string(&path).expect("still invalid"), "{");
    fs::write(&path, "{}").expect("write valid fixture");
    let original = read_editable_settings(&path).expect("read draft");
    let mut invalid = original.values.clone();
    invalid.walker_max_entries = 0;
    assert!(save_editable_settings(&path, &original, &invalid).is_err());
    assert_eq!(fs::read_to_string(&path).expect("unchanged"), "{}");
    fs::remove_dir_all(base).expect("cleanup fixture");
}

#[test]
fn editable_settings_stale_snapshot_cannot_replace_another_save_or_deleted_file() {
    let base = test_home("editable-stale");
    fs::create_dir_all(&base).expect("create fixture directory");
    let path = base.join("settings.json");
    fs::write(&path, "{}").expect("write fixture");
    let first = read_editable_settings(&path).expect("first snapshot");
    let stale = read_editable_settings(&path).expect("second snapshot");
    let mut first_draft = first.values.clone();
    first_draft.restore_tabs_enabled = true;
    save_editable_settings(&path, &first, &first_draft).expect("first save");
    let mut stale_draft = stale.values.clone();
    stale_draft.history_persist_disabled = true;
    assert!(save_editable_settings(&path, &stale, &stale_draft).is_err());
    assert_eq!(
        read_editable_settings(&path).expect("read current").values,
        first_draft
    );
    fs::remove_file(&path).expect("delete fixture");
    assert!(save_editable_settings(&path, &stale, &stale_draft).is_err());
    assert!(!path.exists());
    fs::remove_dir_all(base).expect("cleanup fixture");
}

#[test]
fn editable_settings_rejects_duplicate_known_keys_like_normal_bootstrap() {
    let base = test_home("editable-duplicate");
    fs::create_dir_all(&base).expect("create fixture directory");
    let path = base.join("settings.json");
    fs::write(
        &path,
        r#"{"restore_tabs_enabled":false,"restore_tabs_enabled":true}"#,
    )
    .expect("write fixture");
    assert!(read_editable_settings(&path).is_err());
    assert!(load_runtime_config_from_path(&path).is_none());
    fs::remove_dir_all(base).expect("cleanup fixture");
}

#[test]
fn editable_settings_restores_original_after_post_replace_sync_failure() {
    let base = test_home("editable-durability");
    fs::create_dir_all(&base).expect("create fixture directory");
    let path = base.join("settings.json");
    let original = br#"{"restore_tabs_enabled":false,"unknown":{"keep":1}}"#;
    fs::write(&path, original).expect("write fixture");
    let baseline = read_editable_settings(&path).expect("read snapshot");
    let mut draft = baseline.values.clone();
    draft.restore_tabs_enabled = true;
    let failure = save_editable_settings_with_writer(&path, &baseline, &draft, |path, bytes, _| {
        crate::fs_atomic::write_bytes_atomic_with_sync_for_test(path, bytes, |_| {
            Err(std::io::Error::other("injected directory sync failure"))
        })
    });
    assert!(failure.is_err());
    assert_eq!(fs::read(&path).expect("rolled back bytes"), original);
    fs::remove_dir_all(base).expect("cleanup fixture");
}

#[test]
fn editable_settings_pre_replace_permission_failure_preserves_original() {
    let base = test_home("editable-permission");
    fs::create_dir_all(&base).expect("create fixture directory");
    let path = base.join("settings.json");
    fs::write(&path, "{}").expect("write fixture");
    let baseline = read_editable_settings(&path).expect("read snapshot");
    let mut draft = baseline.values.clone();
    draft.restore_tabs_enabled = true;
    let failure = save_editable_settings_with_writer(&path, &baseline, &draft, |_, _, _| {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "injected permission failure",
        ))
    });
    assert!(failure.is_err());
    assert_eq!(fs::read_to_string(&path).expect("original remains"), "{}");
    fs::remove_dir_all(base).expect("cleanup fixture");
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
            .get("ctrl_w_deletes_word_in_query")
            .and_then(|value| value.as_bool()),
        Some(false)
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
            .get("ctrl_w_deletes_word_in_query")
            .and_then(|value| value.as_bool()),
        Some(false)
    );
    assert_eq!(
        saved
            .get("tab_pin_moves_to_next_row")
            .and_then(|value| value.as_bool()),
        Some(false)
    );
    assert_eq!(saved.len(), 6);

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
    assert!(!loaded.ctrl_w_deletes_word_in_query);
    assert!(!loaded.tab_pin_moves_to_next_row);
    assert_eq!(loaded.developer, DeveloperRuntimeConfig::default());

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn ctrl_w_query_word_delete_option_loads_when_explicitly_enabled() {
    let _guard = locked_env();
    let home = test_home("ctrl-w-query-word-delete-enabled");
    fs::create_dir_all(&home).expect("create home");
    let path = home.join(RUNTIME_CONFIG_FILE_NAME);
    fs::write(&path, r#"{"ctrl_w_deletes_word_in_query":true}"#).expect("write config");

    let loaded = load_runtime_config_from_path(&path).expect("load config");

    assert!(loaded.ctrl_w_deletes_word_in_query);
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
    assert!(!loaded.ctrl_w_deletes_word_in_query);
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
            .get("ctrl_w_deletes_word_in_query")
            .and_then(|value| value.as_bool()),
        Some(false)
    );
    assert_eq!(
        saved
            .get("tab_pin_moves_to_next_row")
            .and_then(|value| value.as_bool()),
        Some(false)
    );
    assert_eq!(saved.len(), 6);

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
fn migrate_file_if_needed_keeps_current_that_appears_before_fallback_promotion() {
    let _guard = locked_env();
    let base = test_home("migrate-race-current-wins");
    let legacy_base = base.join("legacy");
    let current_base = base.join("current");
    fs::create_dir_all(&legacy_base).expect("create legacy dir");
    fs::create_dir_all(&current_base).expect("create current dir");
    let legacy_path = runtime_config_file_path_in(&legacy_base);
    let current_path = runtime_config_file_path_in(&current_base);
    fs::write(&legacy_path, b"legacy").expect("write legacy");
    let promoted = std::cell::Cell::new(false);

    let migrated = migrate_file_if_needed_with(
        &current_path,
        &legacy_path,
        |_, destination| {
            fs::write(destination, b"current-winner").expect("create racing current");
            Err(std::io::Error::other("force fallback"))
        },
        |_, _| {
            promoted.set(true);
            Ok(())
        },
    );

    assert!(!migrated);
    assert!(!promoted.get());
    assert_eq!(
        fs::read(&current_path).expect("read current"),
        b"current-winner"
    );
    assert_eq!(fs::read(&legacy_path).expect("read legacy"), b"legacy");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn migrate_file_if_needed_failed_atomic_fallback_leaves_no_partial_current() {
    let _guard = locked_env();
    let base = test_home("migrate-fallback-failure");
    let legacy_base = base.join("legacy");
    let current_base = base.join("current");
    fs::create_dir_all(&legacy_base).expect("create legacy dir");
    fs::create_dir_all(&current_base).expect("create current dir");
    let legacy_path = runtime_config_file_path_in(&legacy_base);
    let current_path = runtime_config_file_path_in(&current_base);
    fs::write(&legacy_path, b"legacy-complete").expect("write legacy");

    let migrated = migrate_file_if_needed_with(
        &current_path,
        &legacy_path,
        |_, _| Err(std::io::Error::other("force fallback")),
        |destination, _| {
            fs::write(destination, b"partial")?;
            fs::remove_file(destination)?;
            Err(std::io::Error::other("injected atomic promotion failure"))
        },
    );

    assert!(!migrated);
    assert!(!current_path.exists());
    assert_eq!(
        fs::read(&legacy_path).expect("read legacy"),
        b"legacy-complete"
    );
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn migrate_file_if_needed_atomic_fallback_copies_exact_bytes_and_removes_legacy() {
    let _guard = locked_env();
    let base = test_home("migrate-fallback-success");
    let legacy_base = base.join("legacy");
    let current_base = base.join("current");
    fs::create_dir_all(&legacy_base).expect("create legacy dir");
    fs::create_dir_all(&current_base).expect("create current dir");
    let legacy_path = runtime_config_file_path_in(&legacy_base);
    let current_path = runtime_config_file_path_in(&current_base);
    let legacy_bytes = b"{\"exact\":true}\n";
    fs::write(&legacy_path, legacy_bytes).expect("write legacy");

    let migrated = migrate_file_if_needed_with(
        &current_path,
        &legacy_path,
        |_, _| Err(std::io::Error::other("force fallback")),
        write_bytes_atomic,
    );

    assert!(migrated);
    assert_eq!(fs::read(&current_path).expect("read current"), legacy_bytes);
    assert!(!legacy_path.exists());
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn load_or_seed_rechecks_current_after_waiting_for_sidecar_lock() {
    let _guard = locked_env();
    let base = test_home("seed-lock-recheck");
    fs::create_dir_all(&base).expect("create base");
    let current_path = runtime_config_file_path_in(&base);
    let lock = acquire_sidecar_lock(&current_path, Duration::from_millis(100))
        .expect("hold runtime config lock");
    let child_path = current_path.clone();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let child = std::thread::spawn(move || {
        started_tx.send(()).expect("signal start");
        let loaded = RuntimeConfig::load_or_seed_at(Some(child_path));
        done_tx.send(loaded).expect("send loaded config");
    });
    started_rx.recv().expect("child started");
    std::thread::sleep(Duration::from_millis(30));
    assert!(
        done_rx.try_recv().is_err(),
        "loader must wait for active lock"
    );

    let winner = RuntimeConfig {
        walker_max_entries: 12_345,
        ..RuntimeConfig::default()
    };
    save_runtime_config_to_path(&current_path, &winner).expect("write winning current config");
    drop(lock);

    let loaded = done_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("loader completes after lock release");
    child.join().expect("join loader");
    assert_eq!(loaded.walker_max_entries, 12_345);
    let persisted = load_runtime_config_from_path(&current_path).expect("read persisted config");
    assert_eq!(persisted.walker_max_entries, 12_345);
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn ensure_runtime_config_rechecks_current_after_waiting_for_sidecar_lock() {
    let base = test_home("ensure-lock-recheck");
    fs::create_dir_all(&base).expect("create base");
    let current_path = runtime_config_file_path_in(&base);
    let lock = acquire_sidecar_lock(&current_path, Duration::from_millis(100))
        .expect("hold runtime config lock");
    let candidate = RuntimeConfig {
        walker_max_entries: 99_999,
        ..RuntimeConfig::default()
    };
    let child_path = current_path.clone();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let child = std::thread::spawn(move || {
        started_tx.send(()).expect("signal start");
        let result = ensure_runtime_config_file_at(&child_path, &candidate);
        done_tx.send(result).expect("send ensure result");
    });
    started_rx.recv().expect("child started");
    std::thread::sleep(Duration::from_millis(30));
    assert!(
        done_rx.try_recv().is_err(),
        "ensure must wait for active lock"
    );

    let winner = RuntimeConfig {
        walker_max_entries: 12_345,
        ..RuntimeConfig::default()
    };
    save_runtime_config_to_path(&current_path, &winner).expect("write winning current config");
    drop(lock);

    done_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("ensure completes after lock release")
        .expect("ensure succeeds");
    child.join().expect("join ensure worker");
    let persisted = load_runtime_config_from_path(&current_path).expect("read persisted config");
    assert_eq!(persisted.walker_max_entries, 12_345);
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

#[test]
fn startup_normalization_keeps_successful_concurrent_settings_save() {
    let base = test_home("normalization-save-race");
    fs::create_dir_all(&base).expect("create fixture directory");
    let path = base.join("settings.json");
    fs::write(
        &path,
        r#"{"walker_threads":2,"restore_tabs_enabled":false,"walker_max_entries":500000,"future_option":"keep"}"#,
    )
    .expect("write legacy fixture");

    // Reproduce startup reading a pre-save snapshot before normalization writes.
    let stale_text = fs::read_to_string(&path).expect("startup snapshot");
    let stale_config = serde_json::from_str::<RuntimeConfig>(&stale_text).expect("parse snapshot");
    let baseline = read_editable_settings(&path).expect("settings baseline");
    let mut draft = baseline.values.clone();
    draft.restore_tabs_enabled = true;
    draft.walker_max_entries = 17;
    save_editable_settings(&path, &baseline, &draft).expect("successful GUI save");

    normalize_runtime_config_file(&path, &stale_text, &stale_config);
    let json: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).expect("read final settings"))
            .expect("parse final settings");
    assert_eq!(json["restore_tabs_enabled"], true);
    assert_eq!(json["walker_max_entries"], 17);
    assert_eq!(json["future_option"], "keep");
    assert!(json.get("walker_threads").is_none());
    fs::remove_dir_all(base).expect("cleanup fixture");
}

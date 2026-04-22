use crate::fs_atomic::write_text_atomic;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::warn;

pub const RUNTIME_CONFIG_FILE_NAME: &str = ".flistwalker_config.json";
pub const DEFAULT_UPDATE_FEED_URL: &str =
    "https://api.github.com/repos/ShinjiKawamura255/flist-walker/releases/latest";

const SEARCH_PARALLEL_THRESHOLD_DEFAULT: usize = 25_000;
const WALKER_MAX_ENTRIES_DEFAULT: usize = 500_000;
const WALKER_THREADS_DEFAULT: usize = 2;
const WINDOW_TRACE_LOG_NAME: &str = ".flistwalker_window_trace.log";

const SEARCH_PARALLEL_THRESHOLD_ENV: &str = "FLISTWALKER_SEARCH_PARALLEL_THRESHOLD";
const SEARCH_THREADS_ENV: &str = "FLISTWALKER_SEARCH_THREADS";
const WALKER_MAX_ENTRIES_ENV: &str = "FLISTWALKER_WALKER_MAX_ENTRIES";
const WALKER_THREADS_ENV: &str = "FLISTWALKER_WALKER_THREADS";
const WINDOW_TRACE_ENV: &str = "FLISTWALKER_WINDOW_TRACE";
const WINDOW_TRACE_VERBOSE_ENV: &str = "FLISTWALKER_WINDOW_TRACE_VERBOSE";
const WINDOW_TRACE_PATH_ENV: &str = "FLISTWALKER_WINDOW_TRACE_PATH";
const HISTORY_PERSIST_ENV: &str = "FLISTWALKER_DISABLE_HISTORY_PERSIST";
const RESTORE_TABS_ENV: &str = "FLISTWALKER_RESTORE_TABS";
const UPDATE_FEED_URL_ENV: &str = "FLISTWALKER_UPDATE_FEED_URL";
const UPDATE_ALLOW_SAME_VERSION_ENV: &str = "FLISTWALKER_UPDATE_ALLOW_SAME_VERSION";
const UPDATE_ALLOW_DOWNGRADE_ENV: &str = "FLISTWALKER_UPDATE_ALLOW_DOWNGRADE";
const DISABLE_SELF_UPDATE_ENV: &str = "FLISTWALKER_DISABLE_SELF_UPDATE";
const FORCE_UPDATE_CHECK_FAILURE_ENV: &str = "FLISTWALKER_FORCE_UPDATE_CHECK_FAILURE";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RuntimeConfig {
    #[serde(skip_serializing_if = "is_default_search_parallel_threshold")]
    pub search_parallel_threshold: usize,
    #[serde(skip_serializing_if = "is_default_search_threads")]
    pub search_threads: usize,
    #[serde(skip_serializing_if = "is_default_walker_max_entries")]
    pub walker_max_entries: usize,
    #[serde(skip_serializing_if = "is_default_walker_threads")]
    pub walker_threads: usize,
    #[serde(skip_serializing_if = "is_false")]
    pub window_trace_enabled: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub window_trace_verbose: bool,
    #[serde(skip_serializing_if = "is_default_window_trace_path")]
    pub window_trace_path: String,
    #[serde(skip_serializing_if = "is_false")]
    pub history_persist_disabled: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub restore_tabs_enabled: bool,
    #[serde(skip_serializing_if = "is_default_update_feed_url")]
    pub update_feed_url: String,
    #[serde(skip_serializing_if = "is_false")]
    pub update_allow_same_version: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub update_allow_downgrade: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub disable_self_update: bool,
    #[serde(skip_serializing_if = "is_default_force_update_check_failure")]
    pub force_update_check_failure: String,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            search_parallel_threshold: SEARCH_PARALLEL_THRESHOLD_DEFAULT,
            search_threads: default_search_threads(),
            walker_max_entries: WALKER_MAX_ENTRIES_DEFAULT,
            walker_threads: WALKER_THREADS_DEFAULT,
            window_trace_enabled: false,
            window_trace_verbose: false,
            window_trace_path: default_window_trace_path(),
            history_persist_disabled: false,
            restore_tabs_enabled: false,
            update_feed_url: DEFAULT_UPDATE_FEED_URL.to_string(),
            update_allow_same_version: false,
            update_allow_downgrade: false,
            disable_self_update: false,
            force_update_check_failure: String::new(),
        }
    }
}

impl RuntimeConfig {
    pub fn from_current_env() -> Self {
        Self {
            search_parallel_threshold: env_usize(SEARCH_PARALLEL_THRESHOLD_ENV)
                .unwrap_or(SEARCH_PARALLEL_THRESHOLD_DEFAULT),
            search_threads: env_usize(SEARCH_THREADS_ENV).unwrap_or_else(default_search_threads),
            walker_max_entries: env_usize(WALKER_MAX_ENTRIES_ENV)
                .unwrap_or(WALKER_MAX_ENTRIES_DEFAULT),
            walker_threads: env_usize(WALKER_THREADS_ENV).unwrap_or(WALKER_THREADS_DEFAULT),
            window_trace_enabled: env_bool(WINDOW_TRACE_ENV),
            window_trace_verbose: env_bool(WINDOW_TRACE_VERBOSE_ENV),
            window_trace_path: env_string(WINDOW_TRACE_PATH_ENV)
                .unwrap_or_else(default_window_trace_path),
            history_persist_disabled: env_bool(HISTORY_PERSIST_ENV),
            restore_tabs_enabled: env_bool(RESTORE_TABS_ENV),
            update_feed_url: env_string(UPDATE_FEED_URL_ENV)
                .unwrap_or_else(|| DEFAULT_UPDATE_FEED_URL.to_string()),
            update_allow_same_version: env_bool(UPDATE_ALLOW_SAME_VERSION_ENV),
            update_allow_downgrade: env_bool(UPDATE_ALLOW_DOWNGRADE_ENV),
            disable_self_update: env_bool(DISABLE_SELF_UPDATE_ENV),
            force_update_check_failure: env_string(FORCE_UPDATE_CHECK_FAILURE_ENV)
                .unwrap_or_default(),
        }
    }

    pub fn apply_to_process_env(&self) {
        set_env_value(
            SEARCH_PARALLEL_THRESHOLD_ENV,
            self.search_parallel_threshold.to_string(),
        );
        set_env_value(SEARCH_THREADS_ENV, self.search_threads.to_string());
        set_env_value(WALKER_MAX_ENTRIES_ENV, self.walker_max_entries.to_string());
        set_env_value(WALKER_THREADS_ENV, self.walker_threads.to_string());
        set_env_bool(WINDOW_TRACE_ENV, self.window_trace_enabled);
        set_env_bool(WINDOW_TRACE_VERBOSE_ENV, self.window_trace_verbose);
        set_env_value(WINDOW_TRACE_PATH_ENV, self.window_trace_path.clone());
        set_env_bool(HISTORY_PERSIST_ENV, self.history_persist_disabled);
        set_env_bool(RESTORE_TABS_ENV, self.restore_tabs_enabled);
        set_env_value(UPDATE_FEED_URL_ENV, self.update_feed_url.clone());
        set_env_bool(
            UPDATE_ALLOW_SAME_VERSION_ENV,
            self.update_allow_same_version,
        );
        set_env_bool(UPDATE_ALLOW_DOWNGRADE_ENV, self.update_allow_downgrade);
        set_env_bool(DISABLE_SELF_UPDATE_ENV, self.disable_self_update);
        if self.force_update_check_failure.trim().is_empty() {
            env::remove_var(FORCE_UPDATE_CHECK_FAILURE_ENV);
        } else {
            set_env_value(
                FORCE_UPDATE_CHECK_FAILURE_ENV,
                self.force_update_check_failure.clone(),
            );
        }
    }

    pub fn load_or_seed() -> Self {
        Self::load_or_seed_at(runtime_config_file_path())
    }

    fn load_or_seed_at(path: Option<PathBuf>) -> Self {
        let Some(path) = path else {
            let config = Self::from_current_env();
            config.apply_to_process_env();
            return config;
        };

        let legacy_path = legacy_runtime_config_file_path(&path);
        if let Some(config) = load_runtime_config_from_path(&path) {
            config.apply_to_process_env();
            return config;
        }

        if let Some(config) = try_load_or_migrate_runtime_config(&path, legacy_path.as_deref()) {
            config.apply_to_process_env();
            return config;
        }

        let config = Self::from_current_env();
        if !path.exists() {
            if let Err(err) = save_runtime_config_to_path(&path, &config) {
                warn!(
                    "failed to create runtime config at {}: {}",
                    path.display(),
                    err
                );
            }
        } else {
            warn!(
                "failed to read runtime config at {}; using current environment values",
                path.display()
            );
        }
        config.apply_to_process_env();
        config
    }

    pub fn save_to_path(&self, path: &Path) -> Result<()> {
        save_runtime_config_to_path(path, self)
    }
}

pub fn runtime_config_file_path() -> Option<PathBuf> {
    settings_base_dir().map(|base| runtime_config_file_path_in(&base))
}

pub fn initialize_runtime_config() -> RuntimeConfig {
    RuntimeConfig::load_or_seed()
}

pub fn settings_base_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        if let Some(base) = current_exe_dir() {
            return Some(base);
        }
    }
    home_dir()
}

pub fn legacy_settings_base_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        home_dir()
    }

    #[cfg(not(windows))]
    {
        None
    }
}

pub fn runtime_config_file_path_in(base: &Path) -> PathBuf {
    base.join(RUNTIME_CONFIG_FILE_NAME)
}

pub fn legacy_runtime_config_file_path(current_path: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let legacy_base = home_dir()?;
        let legacy_path = runtime_config_file_path_in(&legacy_base);
        if legacy_path == current_path {
            return None;
        }
        Some(legacy_path)
    }

    #[cfg(not(windows))]
    {
        let _ = current_path;
        None
    }
}

pub(crate) fn migrate_file_if_needed(current_path: &Path, legacy_path: &Path) -> bool {
    if current_path.exists() || !legacy_path.exists() {
        return false;
    }
    if let Some(parent) = current_path.parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            warn!(
                "failed to prepare destination directory for {}: {}",
                current_path.display(),
                err
            );
            return false;
        }
    }
    match fs::rename(legacy_path, current_path) {
        Ok(_) => true,
        Err(rename_err) => match fs::copy(legacy_path, current_path) {
            Ok(_) => {
                if let Err(remove_err) = remove_file_best_effort(legacy_path) {
                    warn!(
                        "copied legacy file from {} to {}, but failed to remove original: {}",
                        legacy_path.display(),
                        current_path.display(),
                        remove_err
                    );
                }
                true
            }
            Err(copy_err) => {
                warn!(
                    "failed to migrate legacy file from {} to {}: rename error: {}; copy error: {}",
                    legacy_path.display(),
                    current_path.display(),
                    rename_err,
                    copy_err
                );
                false
            }
        },
    }
}

fn remove_file_best_effort(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) => {
            if let Ok(metadata) = fs::metadata(path) {
                let mut permissions = metadata.permissions();
                if permissions.readonly() {
                    permissions.set_readonly(false);
                    let _ = fs::set_permissions(path, permissions);
                    return fs::remove_file(path);
                }
            }
            Err(err)
        }
    }
}

pub fn load_runtime_config_from_path(path: &Path) -> Option<RuntimeConfig> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str::<RuntimeConfig>(&text).ok()
}

pub fn save_runtime_config_to_path(path: &Path, config: &RuntimeConfig) -> Result<()> {
    let text =
        serde_json::to_string_pretty(config).context("failed to serialize runtime config")?;
    write_text_atomic(path, &text).context("failed to write runtime config")
}

fn try_load_or_migrate_runtime_config(
    current_path: &Path,
    legacy_path: Option<&Path>,
) -> Option<RuntimeConfig> {
    let legacy_path = legacy_path?;
    if current_path.exists() {
        return None;
    }
    if migrate_file_if_needed(current_path, legacy_path) {
        return load_runtime_config_from_path(current_path);
    }
    if legacy_path.exists() {
        return load_runtime_config_from_path(legacy_path);
    }
    None
}

fn default_search_threads() -> usize {
    std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(1)
        .clamp(1, 32)
}

fn default_window_trace_path() -> String {
    settings_base_dir()
        .map(|base| {
            base.join(WINDOW_TRACE_LOG_NAME)
                .to_string_lossy()
                .to_string()
        })
        .unwrap_or_default()
}

fn env_bool(name: &str) -> bool {
    env::var(name)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn env_usize(name: &str) -> Option<usize> {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
}

fn env_string(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn set_env_value(name: &str, value: String) {
    env::set_var(name, value);
}

fn set_env_bool(name: &str, value: bool) {
    env::set_var(name, if value { "1" } else { "0" });
}

fn is_default_search_parallel_threshold(value: &usize) -> bool {
    *value == SEARCH_PARALLEL_THRESHOLD_DEFAULT
}

fn is_default_search_threads(value: &usize) -> bool {
    *value == default_search_threads()
}

fn is_default_walker_max_entries(value: &usize) -> bool {
    *value == WALKER_MAX_ENTRIES_DEFAULT
}

fn is_default_walker_threads(value: &usize) -> bool {
    *value == WALKER_THREADS_DEFAULT
}

fn is_default_window_trace_path(value: &String) -> bool {
    *value == default_window_trace_path()
}

fn is_default_update_feed_url(value: &String) -> bool {
    value == DEFAULT_UPDATE_FEED_URL
}

fn is_default_force_update_check_failure(value: &String) -> bool {
    value.is_empty()
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[cfg(windows)]
fn current_exe_dir() -> Option<PathBuf> {
    env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
}

fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        if let Some(base) = env::var_os("USERPROFILE") {
            return Some(PathBuf::from(base));
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(base) = env::var_os("HOME") {
            return Some(PathBuf::from(base));
        }
    }
    None
}

#[cfg(test)]
mod tests {
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
            RESTORE_TABS_ENV,
            WALKER_MAX_ENTRIES_ENV,
            WINDOW_TRACE_PATH_ENV,
        ]);
        env::set_var("HOME", &home);
        env::set_var("USERPROFILE", &home);
        env::set_var(SEARCH_PARALLEL_THRESHOLD_ENV, "111");
        env::set_var(RESTORE_TABS_ENV, "1");
        env::set_var(WALKER_MAX_ENTRIES_ENV, "222");

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
        assert!(!saved.contains_key("search_threads"));
        assert!(!saved.contains_key("walker_threads"));
        assert!(!saved.contains_key("window_trace_enabled"));
        assert!(!saved.contains_key("window_trace_verbose"));
        assert!(!saved.contains_key("window_trace_path"));
        assert!(!saved.contains_key("history_persist_disabled"));
        assert!(!saved.contains_key("update_feed_url"));
        assert!(!saved.contains_key("update_allow_same_version"));
        assert!(!saved.contains_key("update_allow_downgrade"));
        assert!(!saved.contains_key("disable_self_update"));
        assert!(!saved.contains_key("force_update_check_failure"));

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
    fn settings_base_dir_prefers_current_exe_on_windows_or_home_elsewhere() {
        let _guard = locked_env();
        let home = test_home("base-dir");
        fs::create_dir_all(&home).expect("create home");
        let _restore = EnvRestore::capture(&["HOME", "USERPROFILE"]);
        env::set_var("HOME", &home);
        env::set_var("USERPROFILE", &home);

        #[cfg(windows)]
        {
            let expected = env::current_exe()
                .expect("current exe")
                .parent()
                .expect("exe dir")
                .to_path_buf();
            assert_eq!(settings_base_dir().as_deref(), Some(expected.as_path()));
        }

        #[cfg(not(windows))]
        {
            assert_eq!(settings_base_dir(), Some(home.clone()));
        }

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
        assert_eq!(loaded.walker_threads, WALKER_THREADS_DEFAULT);

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
        assert_eq!(loaded.walker_threads, 7);

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
        assert_eq!(loaded.walker_threads, 9);

        let _ = fs::remove_dir_all(&base);
    }
}

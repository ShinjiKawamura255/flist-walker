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
    pub search_parallel_threshold: usize,
    pub search_threads: usize,
    pub walker_max_entries: usize,
    pub walker_threads: usize,
    pub window_trace_enabled: bool,
    pub window_trace_verbose: bool,
    pub window_trace_path: String,
    pub history_persist_disabled: bool,
    pub restore_tabs_enabled: bool,
    pub update_feed_url: String,
    pub update_allow_same_version: bool,
    pub update_allow_downgrade: bool,
    pub disable_self_update: bool,
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
        let mut config = Self::default();
        config.search_parallel_threshold = env_usize(SEARCH_PARALLEL_THRESHOLD_ENV)
            .unwrap_or(SEARCH_PARALLEL_THRESHOLD_DEFAULT);
        config.search_threads = env_usize(SEARCH_THREADS_ENV).unwrap_or_else(default_search_threads);
        config.walker_max_entries =
            env_usize(WALKER_MAX_ENTRIES_ENV).unwrap_or(WALKER_MAX_ENTRIES_DEFAULT);
        config.walker_threads = env_usize(WALKER_THREADS_ENV).unwrap_or(WALKER_THREADS_DEFAULT);
        config.window_trace_enabled = env_bool(WINDOW_TRACE_ENV);
        config.window_trace_verbose = env_bool(WINDOW_TRACE_VERBOSE_ENV);
        config.window_trace_path = env_string(WINDOW_TRACE_PATH_ENV)
            .unwrap_or_else(default_window_trace_path);
        config.history_persist_disabled = env_bool(HISTORY_PERSIST_ENV);
        config.restore_tabs_enabled = env_bool(RESTORE_TABS_ENV);
        config.update_feed_url = env_string(UPDATE_FEED_URL_ENV)
            .unwrap_or_else(|| DEFAULT_UPDATE_FEED_URL.to_string());
        config.update_allow_same_version = env_bool(UPDATE_ALLOW_SAME_VERSION_ENV);
        config.update_allow_downgrade = env_bool(UPDATE_ALLOW_DOWNGRADE_ENV);
        config.disable_self_update = env_bool(DISABLE_SELF_UPDATE_ENV);
        config.force_update_check_failure = env_string(FORCE_UPDATE_CHECK_FAILURE_ENV)
            .unwrap_or_default();
        config
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
        set_env_bool(UPDATE_ALLOW_SAME_VERSION_ENV, self.update_allow_same_version);
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
        let Some(path) = runtime_config_file_path() else {
            let config = Self::from_current_env();
            config.apply_to_process_env();
            return config;
        };

        if let Some(config) = load_runtime_config_from_path(&path) {
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
    home_dir().map(|base| base.join(RUNTIME_CONFIG_FILE_NAME))
}

pub fn initialize_runtime_config() -> RuntimeConfig {
    RuntimeConfig::load_or_seed()
}

pub fn load_runtime_config_from_path(path: &Path) -> Option<RuntimeConfig> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str::<RuntimeConfig>(&text).ok()
}

pub fn save_runtime_config_to_path(path: &Path, config: &RuntimeConfig) -> Result<()> {
    let text = serde_json::to_string_pretty(config).context("failed to serialize runtime config")?;
    write_text_atomic(path, &text).context("failed to write runtime config")
}

fn default_search_threads() -> usize {
    std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(1)
        .min(32)
        .max(1)
}

fn default_window_trace_path() -> String {
    home_dir()
        .map(|base| base.join(WINDOW_TRACE_LOG_NAME).to_string_lossy().to_string())
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
        ]);
        env::set_var("HOME", &home);
        env::set_var("USERPROFILE", &home);
        env::set_var(SEARCH_PARALLEL_THRESHOLD_ENV, "111");
        env::set_var(RESTORE_TABS_ENV, "1");
        env::set_var(WALKER_MAX_ENTRIES_ENV, "222");

        let config = RuntimeConfig::load_or_seed();
        let path = home.join(RUNTIME_CONFIG_FILE_NAME);

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
        let saved: RuntimeConfig = serde_json::from_str(&text).expect("parse config");
        assert_eq!(saved.search_parallel_threshold, 111);
        assert!(saved.restore_tabs_enabled);
        assert_eq!(saved.walker_max_entries, 222);

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
        config
            .save_to_path(&home.join(RUNTIME_CONFIG_FILE_NAME))
            .expect("save config");

        let loaded = RuntimeConfig::load_or_seed();
        assert_eq!(loaded.search_parallel_threshold, 7);
        assert!(!loaded.restore_tabs_enabled);
        assert_eq!(env::var(SEARCH_PARALLEL_THRESHOLD_ENV).expect("env set"), "7");
        assert_eq!(env::var(RESTORE_TABS_ENV).expect("env set"), "0");

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
        assert_eq!(loaded.search_parallel_threshold, SEARCH_PARALLEL_THRESHOLD_DEFAULT);
        assert_eq!(loaded.walker_max_entries, WALKER_MAX_ENTRIES_DEFAULT);
        assert_eq!(loaded.walker_threads, WALKER_THREADS_DEFAULT);

        let _ = fs::remove_dir_all(&home);
    }
}

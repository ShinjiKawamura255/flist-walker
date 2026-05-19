use crate::fs_atomic::write_text_atomic;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tracing::warn;

pub const RUNTIME_CONFIG_FILE_NAME: &str = ".flistwalker_config.json";
pub const DEFAULT_UPDATE_FEED_URL: &str =
    "https://api.github.com/repos/ShinjiKawamura255/flist-walker/releases/latest";

#[cfg(windows)]
const WINDOWS_SETTINGS_DIR_NAME: &str = "flistwalker";
#[cfg(not(windows))]
const UNIX_SETTINGS_DIR_NAME: &str = ".flistwalker";
const SEARCH_PARALLEL_THRESHOLD_DEFAULT: usize = 25_000;
const WALKER_MAX_ENTRIES_DEFAULT: usize = 500_000;
const WALKER_THREADS_MAX_DEFAULT: usize = 8;
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
    #[serde(skip_serializing_if = "DeveloperRuntimeConfig::is_default")]
    pub developer: DeveloperRuntimeConfig,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeveloperRuntimeConfig {
    pub walker_backend: String,
    pub walker_metrics: bool,
    pub walker_metrics_log_path: String,
    pub walker_adaptive_initial_limit: Option<usize>,
    pub walker_adaptive_max_limit: Option<usize>,
}

impl DeveloperRuntimeConfig {
    fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

#[derive(Clone, Debug, Default, Serialize)]
struct RuntimeConfigSeed {
    #[serde(skip_serializing_if = "Option::is_none")]
    search_parallel_threshold: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    search_threads: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    walker_max_entries: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    walker_threads: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    window_trace_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    window_trace_verbose: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    window_trace_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    history_persist_disabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    restore_tabs_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    update_feed_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    update_allow_same_version: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    update_allow_downgrade: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    disable_self_update: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    force_update_check_failure: Option<String>,
}

fn process_runtime_config() -> &'static Mutex<RuntimeConfig> {
    static CONFIG: OnceLock<Mutex<RuntimeConfig>> = OnceLock::new();
    CONFIG.get_or_init(|| Mutex::new(RuntimeConfig::default()))
}

pub fn current_runtime_config() -> RuntimeConfig {
    process_runtime_config()
        .lock()
        .map(|config| config.clone())
        .unwrap_or_else(|_| RuntimeConfig::from_current_env())
}

pub fn set_process_runtime_config(config: RuntimeConfig) {
    if let Ok(mut current) = process_runtime_config().lock() {
        *current = config;
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            search_parallel_threshold: SEARCH_PARALLEL_THRESHOLD_DEFAULT,
            search_threads: default_search_threads(),
            walker_max_entries: WALKER_MAX_ENTRIES_DEFAULT,
            walker_threads: default_walker_threads(),
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
            developer: DeveloperRuntimeConfig::default(),
        }
    }
}

impl RuntimeConfig {
    pub fn from_current_env() -> Self {
        Self::seed_from_current_env().0
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
        set_process_runtime_config(self.clone());
    }

    pub fn load_or_seed() -> Self {
        Self::load_or_seed_at(runtime_config_file_path())
    }

    fn load_or_seed_at(path: Option<PathBuf>) -> Self {
        let Some(path) = path else {
            let (config, _) = Self::seed_from_current_env();
            config.apply_to_process_env();
            return config;
        };

        let legacy_paths = legacy_runtime_config_file_paths(&path);
        if let Some(config) = load_runtime_config_from_path(&path) {
            config.apply_to_process_env();
            return config;
        }

        if let Some(config) = try_load_or_migrate_runtime_config(&path, &legacy_paths) {
            config.apply_to_process_env();
            return config;
        }

        let (config, seed) = Self::seed_from_current_env();
        if !path.exists() {
            if let Err(err) = save_seeded_runtime_config_to_path(&path, &seed) {
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
        local_app_data_dir().map(|base| base.join(WINDOWS_SETTINGS_DIR_NAME))
    }

    #[cfg(not(windows))]
    {
        home_dir().map(|base| base.join(UNIX_SETTINGS_DIR_NAME))
    }
}

pub fn legacy_settings_base_dirs() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        let mut dirs = Vec::new();
        if let Some(base) = current_exe_dir() {
            dirs.push(base);
        }
        if let Some(base) = home_dir() {
            if dirs.iter().all(|existing| existing != &base) {
                dirs.push(base);
            }
        }
        dirs
    }

    #[cfg(not(windows))]
    {
        home_dir().into_iter().collect()
    }
}

pub fn runtime_config_file_path_in(base: &Path) -> PathBuf {
    base.join(RUNTIME_CONFIG_FILE_NAME)
}

pub fn legacy_runtime_config_file_paths(current_path: &Path) -> Vec<PathBuf> {
    legacy_settings_base_dirs()
        .into_iter()
        .map(|base| runtime_config_file_path_in(&base))
        .filter(|path| path != current_path)
        .collect()
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

#[allow(clippy::permissions_set_readonly_false)]
fn remove_file_best_effort(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) => {
            #[cfg(windows)]
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

fn save_seeded_runtime_config_to_path(path: &Path, seed: &RuntimeConfigSeed) -> Result<()> {
    let text =
        serde_json::to_string_pretty(seed).context("failed to serialize runtime config seed")?;
    write_text_atomic(path, &text).context("failed to write runtime config")
}

fn try_load_or_migrate_runtime_config(
    current_path: &Path,
    legacy_paths: &[PathBuf],
) -> Option<RuntimeConfig> {
    if current_path.exists() {
        return None;
    }
    for legacy_path in legacy_paths {
        if migrate_file_if_needed(current_path, legacy_path) {
            return load_runtime_config_from_path(current_path);
        }
    }
    for legacy_path in legacy_paths {
        if legacy_path.exists() {
            return load_runtime_config_from_path(legacy_path);
        }
    }
    None
}

fn default_search_threads() -> usize {
    std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(1)
        .clamp(1, 32)
}

fn default_walker_threads() -> usize {
    let half_logical = std::thread::available_parallelism()
        .map(|value| value.get() / 2)
        .unwrap_or(1)
        .max(1);
    half_logical.min(WALKER_THREADS_MAX_DEFAULT)
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

fn env_bool_with_presence(name: &str) -> (bool, bool) {
    match env::var(name) {
        Ok(value) => (
            true,
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            ),
        ),
        Err(_) => (false, false),
    }
}

fn env_usize_with_presence(name: &str) -> (bool, Option<usize>) {
    match env::var(name) {
        Ok(value) => (true, value.parse::<usize>().ok().filter(|value| *value > 0)),
        Err(_) => (false, None),
    }
}

fn env_string_with_presence(name: &str) -> (bool, Option<String>) {
    match env::var(name) {
        Ok(value) => {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                (true, None)
            } else {
                (true, Some(trimmed))
            }
        }
        Err(_) => (false, None),
    }
}

fn set_env_value(name: &str, value: String) {
    env::set_var(name, value);
}

fn set_env_bool(name: &str, value: bool) {
    env::set_var(name, if value { "1" } else { "0" });
}

impl RuntimeConfig {
    fn seed_from_current_env() -> (Self, RuntimeConfigSeed) {
        let (_, search_parallel_threshold) = env_usize_with_presence(SEARCH_PARALLEL_THRESHOLD_ENV);
        let (_, search_threads) = env_usize_with_presence(SEARCH_THREADS_ENV);
        let (_, walker_max_entries) = env_usize_with_presence(WALKER_MAX_ENTRIES_ENV);
        let (_, walker_threads) = env_usize_with_presence(WALKER_THREADS_ENV);
        let (window_trace_enabled_set, window_trace_enabled) =
            env_bool_with_presence(WINDOW_TRACE_ENV);
        let (window_trace_verbose_set, window_trace_verbose) =
            env_bool_with_presence(WINDOW_TRACE_VERBOSE_ENV);
        let (_, window_trace_path) = env_string_with_presence(WINDOW_TRACE_PATH_ENV);
        let (history_persist_disabled_set, history_persist_disabled) =
            env_bool_with_presence(HISTORY_PERSIST_ENV);
        let (restore_tabs_enabled_set, restore_tabs_enabled) =
            env_bool_with_presence(RESTORE_TABS_ENV);
        let (_, update_feed_url) = env_string_with_presence(UPDATE_FEED_URL_ENV);
        let (update_allow_same_version_set, update_allow_same_version) =
            env_bool_with_presence(UPDATE_ALLOW_SAME_VERSION_ENV);
        let (update_allow_downgrade_set, update_allow_downgrade) =
            env_bool_with_presence(UPDATE_ALLOW_DOWNGRADE_ENV);
        let (disable_self_update_set, disable_self_update) =
            env_bool_with_presence(DISABLE_SELF_UPDATE_ENV);
        let (_, force_update_check_failure) =
            env_string_with_presence(FORCE_UPDATE_CHECK_FAILURE_ENV);

        let config = Self {
            search_parallel_threshold: search_parallel_threshold
                .unwrap_or(SEARCH_PARALLEL_THRESHOLD_DEFAULT),
            search_threads: search_threads.unwrap_or_else(default_search_threads),
            walker_max_entries: walker_max_entries.unwrap_or(WALKER_MAX_ENTRIES_DEFAULT),
            walker_threads: walker_threads.unwrap_or_else(default_walker_threads),
            window_trace_enabled,
            window_trace_verbose,
            window_trace_path: window_trace_path
                .as_ref()
                .cloned()
                .unwrap_or_else(default_window_trace_path),
            history_persist_disabled,
            restore_tabs_enabled,
            update_feed_url: update_feed_url
                .as_ref()
                .cloned()
                .unwrap_or_else(|| DEFAULT_UPDATE_FEED_URL.to_string()),
            update_allow_same_version,
            update_allow_downgrade,
            disable_self_update,
            force_update_check_failure: force_update_check_failure
                .as_ref()
                .cloned()
                .unwrap_or_default(),
            developer: DeveloperRuntimeConfig::default(),
        };

        let seed = RuntimeConfigSeed {
            search_parallel_threshold: search_parallel_threshold
                .map(|_| config.search_parallel_threshold),
            search_threads: search_threads.map(|_| config.search_threads),
            walker_max_entries: walker_max_entries.map(|_| config.walker_max_entries),
            walker_threads: walker_threads.map(|_| config.walker_threads),
            window_trace_enabled: window_trace_enabled_set.then_some(config.window_trace_enabled),
            window_trace_verbose: window_trace_verbose_set.then_some(config.window_trace_verbose),
            window_trace_path: window_trace_path.map(|_| config.window_trace_path.clone()),
            history_persist_disabled: history_persist_disabled_set
                .then_some(config.history_persist_disabled),
            restore_tabs_enabled: restore_tabs_enabled_set.then_some(config.restore_tabs_enabled),
            update_feed_url: update_feed_url.map(|_| config.update_feed_url.clone()),
            update_allow_same_version: update_allow_same_version_set
                .then_some(config.update_allow_same_version),
            update_allow_downgrade: update_allow_downgrade_set
                .then_some(config.update_allow_downgrade),
            disable_self_update: disable_self_update_set.then_some(config.disable_self_update),
            force_update_check_failure: force_update_check_failure
                .map(|_| config.force_update_check_failure.clone()),
        };

        (config, seed)
    }
}

#[cfg(windows)]
fn current_exe_dir() -> Option<PathBuf> {
    env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
}

#[cfg(windows)]
fn local_app_data_dir() -> Option<PathBuf> {
    env::var_os("LOCALAPPDATA")
        .or_else(|| env::var_os("APPDATA"))
        .map(PathBuf::from)
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
mod tests;

//! CLI/TUI settings have their own file; the existing file remains GUI-owned.
use super::*;

pub const CLI_RUNTIME_CONFIG_FILE_NAME: &str = ".flistwalker_cli_config.json";

pub fn cli_runtime_config_file_path() -> Option<PathBuf> {
    settings_base_dir().map(|base| base.join(CLI_RUNTIME_CONFIG_FILE_NAME))
}

pub fn initialize_cli_runtime_config() -> RuntimeConfig {
    RuntimeConfig::load_or_seed_with(cli_runtime_config_file_path(), load_cli_config_locked)
}

fn load_cli_config_locked(path: &Path) -> RuntimeConfig {
    // The caller owns the CLI sidecar. A concurrent CLI startup may have won
    // while it waited; an existing (even invalid) CLI file must never be replaced.
    if let Some(config) = load_runtime_config_from_path_locked(path) {
        return config;
    }
    if path.exists() {
        return seed_runtime_config_locked(path);
    }
    match inherit_gui_config_locked(path) {
        Ok(Some(config)) => config,
        Ok(None) => seed_runtime_config_locked(path),
        Err(error) => {
            warn!(
                "failed to initialize CLI runtime config at {}: {}; using current environment values without writing",
                path.display(), error
            );
            RuntimeConfig::seed_from_current_env().0
        }
    }
}

fn inherit_gui_config_locked(path: &Path) -> Result<Option<RuntimeConfig>> {
    let base = path
        .parent()
        .context("CLI config has no settings directory")?;
    let gui_path = runtime_config_file_path_in(base);
    // Lock order is CLI -> GUI. GUI writers never acquire the CLI lock. This
    // also serializes copying against GUI migration out of an older location.
    let _gui_lock = acquire_sidecar_lock(&gui_path, Duration::from_secs(5))
        .context("failed to lock GUI config while copying CLI settings")?;
    let mut sources = vec![gui_path.clone()];
    sources.extend(legacy_runtime_config_file_paths(&gui_path));
    copy_first_config_locked(path, &sources)
}

fn copy_first_config_locked(path: &Path, sources: &[PathBuf]) -> Result<Option<RuntimeConfig>> {
    for source in sources {
        let text = match fs::read_to_string(source) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error).with_context(|| format!("failed to read {}", source.display()))
            }
        };
        let config: RuntimeConfig = serde_json::from_str(&text)
            .with_context(|| format!("invalid runtime config at {}", source.display()))?;
        // Copy raw JSON, not RuntimeConfig: unknown keys and developer settings
        // must survive. Never rename, normalize, or write back to the source.
        write_text_atomic(path, &text).context("failed to copy GUI settings to CLI config")?;
        normalize_runtime_config_file_locked(path, &text, &config);
        return Ok(Some(config));
    }
    Ok(None)
}

#[cfg(test)]
mod tests;

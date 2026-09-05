mod adaptive;

use crate::entry::EntryKind;
use crate::runtime_config::RuntimeConfig;
use std::fs::FileType;
use std::path::Path;

#[cfg(test)]
pub(crate) use adaptive::{
    adaptive_shared_frontier_soft_limit, next_limit_from_throughput, walk_adaptive,
    walk_adaptive_filtered, walk_adaptive_filtered_deferred, walk_adaptive_filtered_unbounded,
    walk_adaptive_filtered_with_frontier_limits,
    walk_adaptive_filtered_with_frontier_limits_and_max_depth, walk_adaptive_with_max_depth,
    LimitDirection,
};
pub(crate) use adaptive::{walk_adaptive_with_options, AdaptiveWalkerEntry, AdaptiveWalkerMetrics};

const ADAPTIVE_WALKER_MAX_LIMIT_CAP: usize = 64;
const ADAPTIVE_WALKER_MAX_LIMIT_DEFAULT_CAP: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WalkerBackend {
    Adaptive,
}

#[derive(Debug)]
pub(crate) struct WalkerRuntimeSettings {
    pub(crate) max_entries: usize,
    pub(crate) adaptive_initial_limit: usize,
    pub(crate) adaptive_max_limit: usize,
    pub(crate) backend: WalkerBackend,
    pub(crate) metrics_enabled: bool,
    pub(crate) metrics_log_path: String,
}

pub(crate) fn walker_runtime_settings(config: &RuntimeConfig) -> WalkerRuntimeSettings {
    let adaptive_max_limit = config
        .developer
        .walker_adaptive_max_limit
        .unwrap_or_else(default_adaptive_max_limit)
        .max(1);
    let adaptive_initial_limit = config
        .developer
        .walker_adaptive_initial_limit
        .unwrap_or_else(|| default_adaptive_initial_limit(adaptive_max_limit))
        .max(1)
        .min(adaptive_max_limit);

    WalkerRuntimeSettings {
        max_entries: config.walker_max_entries.max(1),
        adaptive_initial_limit,
        adaptive_max_limit,
        backend: WalkerBackend::Adaptive,
        metrics_enabled: config.developer.walker_metrics,
        metrics_log_path: config.developer.walker_metrics_log_path.clone(),
    }
}

fn default_adaptive_max_limit() -> usize {
    let logical_cores = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(1);
    default_adaptive_max_limit_from_logical_cores(logical_cores)
}

pub(crate) fn default_adaptive_max_limit_from_logical_cores(logical_cores: usize) -> usize {
    logical_cores
        .max(1)
        .div_ceil(2)
        .min(ADAPTIVE_WALKER_MAX_LIMIT_DEFAULT_CAP)
        .clamp(1, ADAPTIVE_WALKER_MAX_LIMIT_CAP)
}

fn default_adaptive_initial_limit(max_limit: usize) -> usize {
    max_limit.div_ceil(2).max(1)
}

fn is_windows_shortcut(path: &Path) -> bool {
    #[cfg(windows)]
    {
        path.extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("lnk"))
    }

    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}

pub(crate) fn resolve_entry_kind(path: &Path) -> Option<EntryKind> {
    let symlink_meta = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return Some(EntryKind::other()),
    };
    let is_link = symlink_meta.file_type().is_symlink() || is_windows_shortcut(path);

    if symlink_meta.is_dir() {
        return Some(if is_link {
            EntryKind::link(true)
        } else {
            EntryKind::dir()
        });
    }
    if symlink_meta.is_file() {
        return Some(if is_link {
            EntryKind::link(false)
        } else {
            EntryKind::file()
        });
    }

    let meta = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return is_link.then_some(EntryKind::link_unknown()),
    };
    if meta.is_dir() {
        Some(if is_link {
            EntryKind::link(true)
        } else {
            EntryKind::dir()
        })
    } else if meta.is_file() {
        Some(if is_link {
            EntryKind::link(false)
        } else {
            EntryKind::file()
        })
    } else if is_link {
        Some(EntryKind::link_unknown())
    } else {
        Some(EntryKind::other())
    }
}

pub(crate) fn classify_walker_entry(
    path: &Path,
    file_type: FileType,
    include_files: bool,
    include_dirs: bool,
) -> Option<(EntryKind, bool)> {
    if file_type.is_dir() {
        return include_dirs.then_some((EntryKind::dir(), true));
    }

    if file_type.is_file() && !is_windows_shortcut(path) {
        return include_files.then_some((EntryKind::file(), true));
    }

    if file_type.is_symlink() || is_windows_shortcut(path) {
        if include_files && include_dirs {
            // Link identity is available from FileType/extension. The target
            // directory state is intentionally resolved after the fast stream.
            return Some((EntryKind::link_unknown(), false));
        }
    } else {
        // Special files are neither searchable files nor directories in the
        // current product contract. Exclude them without metadata probing.
        return None;
    }

    let kind = resolve_entry_kind(path)?;
    kind.is_visible_for_flags(include_files, include_dirs)
        .then_some((kind, true))
}

pub(crate) fn walker_truncated_notice(limit: usize) -> String {
    format!(
        "Walker capped at {limit} entries (set walker_max_entries in the config file to adjust)"
    )
}

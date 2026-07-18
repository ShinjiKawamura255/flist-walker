#[cfg(windows)]
use crate::path_utils::strip_windows_extended_prefix;
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActionPathPrecheck {
    Reject,
    Defer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuthorizedActionTarget {
    pub(crate) display_path: PathBuf,
    pub(crate) execution_path: PathBuf,
    source_paths: Vec<PathBuf>,
    open_parent_for_files: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuthorizedActionBatch {
    pub(crate) canonical_root: PathBuf,
    pub(crate) targets: Vec<AuthorizedActionTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ActionAuthorizationFailure {
    pub(crate) display_path: Option<PathBuf>,
    message: &'static str,
}

impl ActionAuthorizationFailure {
    fn new(display_path: Option<PathBuf>, message: &'static str) -> Self {
        Self {
            display_path,
            message,
        }
    }
}

impl fmt::Display for ActionAuthorizationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message)
    }
}

impl std::error::Error for ActionAuthorizationFailure {}

fn normalize_absolute_lexically(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return None;
                }
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    Some(normalized)
}

#[cfg(windows)]
fn lexically_within_root(root: &Path, path: &Path) -> Option<bool> {
    let mut root_key = strip_windows_extended_prefix(root.to_str()?).replace('\\', "/");
    let mut path_key = strip_windows_extended_prefix(path.to_str()?).replace('\\', "/");
    while root_key.len() > 1 && root_key.ends_with('/') {
        root_key.pop();
    }
    while path_key.len() > 1 && path_key.ends_with('/') {
        path_key.pop();
    }
    root_key.make_ascii_lowercase();
    path_key.make_ascii_lowercase();
    Some(
        path_key == root_key
            || path_key
                .strip_prefix(&root_key)
                .is_some_and(|suffix| suffix.starts_with('/')),
    )
}

#[cfg(not(windows))]
fn lexically_within_root(root: &Path, path: &Path) -> Option<bool> {
    Some(path.starts_with(root))
}

/// UI-only defense in depth. `Defer` is not authorization; the worker remains authoritative.
pub(crate) fn lexical_action_path_precheck(root: &Path, path: &Path) -> ActionPathPrecheck {
    let Some(root) = normalize_absolute_lexically(root) else {
        return ActionPathPrecheck::Defer;
    };
    let Some(path) = normalize_absolute_lexically(path) else {
        return ActionPathPrecheck::Defer;
    };

    match lexically_within_root(&root, &path) {
        Some(false) => ActionPathPrecheck::Reject,
        Some(true) | None => ActionPathPrecheck::Defer,
    }
}

pub(crate) fn action_target_path_for_open_in_folder(
    path: &Path,
) -> Result<PathBuf, ActionAuthorizationFailure> {
    let link_metadata = fs::symlink_metadata(path).map_err(|_| {
        ActionAuthorizationFailure::new(
            Some(path.to_path_buf()),
            "selected path type could not be determined",
        )
    })?;
    let metadata = if link_metadata.file_type().is_symlink() {
        fs::metadata(path).map_err(|_| {
            ActionAuthorizationFailure::new(
                Some(path.to_path_buf()),
                "selected link target could not be resolved",
            )
        })?
    } else {
        link_metadata
    };

    if metadata.is_dir() {
        return Ok(path.to_path_buf());
    }
    if !metadata.is_file() {
        return Err(ActionAuthorizationFailure::new(
            Some(path.to_path_buf()),
            "selected path type is unsupported",
        ));
    }
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            ActionAuthorizationFailure::new(
                Some(path.to_path_buf()),
                "containing folder could not be determined",
            )
        })
}

fn raw_action_target(
    path: &Path,
    open_parent_for_files: bool,
) -> Result<PathBuf, ActionAuthorizationFailure> {
    if !open_parent_for_files {
        return Ok(path.to_path_buf());
    }
    action_target_path_for_open_in_folder(path)
}

fn resolve_within_root(
    canonical_root: &Path,
    raw_path: &Path,
) -> Result<PathBuf, ActionAuthorizationFailure> {
    let resolved = raw_path.canonicalize().map_err(|_| {
        ActionAuthorizationFailure::new(
            Some(raw_path.to_path_buf()),
            "path could not be resolved within current root",
        )
    })?;
    if !resolved.starts_with(canonical_root) {
        return Err(ActionAuthorizationFailure::new(
            Some(raw_path.to_path_buf()),
            "path is outside current root",
        ));
    }
    Ok(resolved)
}

pub(crate) fn authorize_action_targets(
    root: &Path,
    paths: &[PathBuf],
    open_parent_for_files: bool,
) -> Result<AuthorizedActionBatch, ActionAuthorizationFailure> {
    let canonical_root = root.canonicalize().map_err(|_| {
        ActionAuthorizationFailure::new(
            paths.first().cloned(),
            "current root could not be resolved",
        )
    })?;
    let mut target_indices: HashMap<PathBuf, usize> = HashMap::with_capacity(paths.len());
    let mut targets: Vec<AuthorizedActionTarget> = Vec::with_capacity(paths.len());

    for source_path in paths {
        let raw_path = raw_action_target(source_path, open_parent_for_files)?;
        let execution_path = resolve_within_root(&canonical_root, &raw_path)?;
        if let Some(index) = target_indices.get(&execution_path).copied() {
            targets[index].source_paths.push(source_path.clone());
            continue;
        }
        target_indices.insert(execution_path.clone(), targets.len());
        targets.push(AuthorizedActionTarget {
            display_path: raw_path,
            execution_path,
            source_paths: vec![source_path.clone()],
            open_parent_for_files,
        });
    }

    Ok(AuthorizedActionBatch {
        canonical_root,
        targets,
    })
}

pub(crate) fn reauthorize_action_target(
    canonical_root: &Path,
    target: &AuthorizedActionTarget,
) -> Result<PathBuf, ActionAuthorizationFailure> {
    let mut final_execution_path = None;
    for source_path in &target.source_paths {
        let raw_path = raw_action_target(source_path, target.open_parent_for_files)?;
        let execution_path = resolve_within_root(canonical_root, &raw_path)?;
        if execution_path != target.execution_path {
            return Err(ActionAuthorizationFailure::new(
                Some(target.display_path.clone()),
                "authorization changed",
            ));
        }
        final_execution_path = Some(execution_path);
    }
    final_execution_path.ok_or_else(|| {
        ActionAuthorizationFailure::new(
            Some(target.display_path.clone()),
            "action target is unavailable",
        )
    })
}

//! Commits settings files in order and rolls back observable write failures.
use super::document::build_ui_state_document;
use super::{PendingUiStateWrite, SettingsCommitReceipt, SettingsCommitRequest};
use crate::fs_atomic::{
    acquire_sidecar_lock, atomic_write_replaced_destination, write_bytes_atomic,
};
use crate::path_utils::normalize_windows_path_buf;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(super) fn commit_settings(
    ui_state_path: &Path,
    pending: &[PendingUiStateWrite],
    history_persist_disabled: bool,
    lock_timeout: Duration,
    request: SettingsCommitRequest,
) -> std::io::Result<SettingsCommitReceipt> {
    commit_settings_with_writer(
        ui_state_path,
        pending,
        history_persist_disabled,
        lock_timeout,
        request,
        write_bytes_atomic,
    )
}

pub(super) fn commit_settings_with_writer<W>(
    ui_state_path: &Path,
    pending: &[PendingUiStateWrite],
    history_persist_disabled: bool,
    lock_timeout: Duration,
    request: SettingsCommitRequest,
    mut write_atomic: W,
) -> std::io::Result<SettingsCommitReceipt>
where
    W: FnMut(&Path, &[u8]) -> std::io::Result<()>,
{
    let _lock = acquire_sidecar_lock(ui_state_path, lock_timeout)?;
    let ui_state_previous = read_optional_file(ui_state_path)?;
    let document = build_ui_state_document(
        ui_state_path,
        pending,
        Some(&request.patch),
        history_persist_disabled,
    )?;
    let serialized = serde_json::to_string_pretty(&document).map_err(std::io::Error::other)?;
    let canonical_default_root = document
        .get("default_root")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(normalize_windows_path_buf);

    let mut roots_rollback = None;
    if let Some((roots_path, roots_text)) = request.saved_roots.as_ref() {
        let previous = read_optional_file(roots_path)?;
        if let Err(error) = write_atomic(roots_path, roots_text.as_bytes()) {
            if atomic_write_replaced_destination(&error) {
                if let Err(rollback_error) =
                    restore_atomic_target(roots_path, previous.as_deref(), &mut write_atomic)
                {
                    return Err(std::io::Error::other(format!(
                        "{error}; saved-roots rollback failed: {rollback_error}"
                    )));
                }
            }
            return Err(error);
        }
        roots_rollback = Some((roots_path, previous));
    }

    if let Err(error) = write_atomic(ui_state_path, serialized.as_bytes()) {
        let mut rollback_errors = Vec::new();
        if atomic_write_replaced_destination(&error) {
            if let Err(rollback_error) = restore_atomic_target(
                ui_state_path,
                ui_state_previous.as_deref(),
                &mut write_atomic,
            ) {
                rollback_errors.push(format!("UI-state rollback failed: {rollback_error}"));
            }
        }
        if let Some((roots_path, previous)) = roots_rollback {
            if let Err(rollback_error) =
                restore_atomic_target(roots_path, previous.as_deref(), &mut write_atomic)
            {
                rollback_errors.push(format!("saved-roots rollback failed: {rollback_error}"));
            }
        }
        if !rollback_errors.is_empty() {
            return Err(std::io::Error::other(format!(
                "{error}; {}",
                rollback_errors.join("; ")
            )));
        }
        return Err(error);
    }

    Ok(SettingsCommitReceipt {
        canonical_default_root,
    })
}

fn read_optional_file(path: &Path) -> std::io::Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn restore_atomic_target<W>(
    path: &Path,
    previous: Option<&[u8]>,
    write_atomic: &mut W,
) -> std::io::Result<()>
where
    W: FnMut(&Path, &[u8]) -> std::io::Result<()>,
{
    match previous {
        Some(bytes) => write_atomic(path, bytes),
        None => fs::remove_file(path).or_else(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(error)
            }
        }),
    }
}

//! Validates and merges a persisted document without controlling worker admission.
use super::{PendingUiStateWrite, UiStatePatch};
use crate::fs_atomic::{acquire_sidecar_lock, write_text_atomic};
use crate::persistence::schema::UiState;
use crate::query_history::MAX_QUERY_HISTORY_ENTRIES;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(super) fn build_ui_state_document(
    path: &Path,
    pending: &[PendingUiStateWrite],
    extra_patch: Option<&UiStatePatch>,
    history_persist_disabled: bool,
) -> std::io::Result<Value> {
    // Startup may fall back to defaults, but a writer must not turn a failed
    // read into permission to replace existing data. Only absence permits seed.
    let mut document = match fs::read_to_string(path) {
        Ok(text) => {
            let document = serde_json::from_str::<Value>(&text).map_err(|error| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("UI-state JSON is invalid; existing file was not changed: {error}"),
                )
            })?;
            if !document.is_object() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "UI-state JSON must be an object; existing file was not changed",
                ));
            }
            validate_typed_document(&document)?;
            document
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Value::Object(Default::default())
        }
        Err(error) => {
            return Err(std::io::Error::new(
                error.kind(),
                format!("UI-state read failed; existing file was not changed: {error}"),
            ));
        }
    };
    for write in pending {
        merge_json_leaves(&mut document, &write.patch.0);
    }
    if let Some(patch) = extra_patch {
        merge_json_leaves(&mut document, &patch.0);
    }
    if !history_persist_disabled {
        let history = document
            .get("query_history")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut history = normalize_history_recency(history);
        for delta in pending.iter().flat_map(|write| write.history_delta.iter()) {
            append_history_delta(&mut history, delta.clone());
        }
        if let Value::Object(map) = &mut document {
            map.insert(
                "query_history".to_string(),
                Value::Array(history.into_iter().map(Value::String).collect()),
            );
        }
    }
    validate_typed_document(&document)?;
    canonicalize_last_root_for_persistence(&mut document);
    canonicalize_default_root_for_persistence(&mut document);
    Ok(document)
}

fn validate_typed_document(document: &Value) -> std::io::Result<()> {
    serde_json::from_value::<UiState>(document.clone())
        .map(|_| ())
        .map_err(|error| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("UI-state fields are invalid; existing files were not changed: {error}"),
            )
        })
}

pub(super) fn write_pending_ui_state(
    path: &Path,
    pending: &[PendingUiStateWrite],
    history_persist_disabled: bool,
    lock_timeout: Duration,
    #[cfg(test)] diagnostic: Option<&super::FlushDiagnostic>,
) -> std::io::Result<()> {
    debug_assert!(pending
        .windows(2)
        .all(|writes| writes[0].generation < writes[1].generation));
    #[cfg(test)]
    if let Some(trace) = diagnostic {
        trace.record("sidecar-acquire-enter", None);
    }
    let lock = acquire_sidecar_lock(path, lock_timeout);
    #[cfg(test)]
    if let Some(trace) = diagnostic {
        trace.record_io("sidecar-acquire-return", &lock);
    }
    let _lock = lock?;
    #[cfg(test)]
    if let Some(trace) = diagnostic {
        trace.record("document-enter", None);
    }
    let document = build_ui_state_document(path, pending, None, history_persist_disabled);
    #[cfg(test)]
    if let Some(trace) = diagnostic {
        trace.record_io("document-return", &document);
    }
    let document = document?;
    let text = serde_json::to_string_pretty(&document).map_err(std::io::Error::other)?;
    #[cfg(test)]
    if let Some(trace) = diagnostic {
        trace.record("atomic-write-enter", None);
    }
    let written = write_text_atomic(path, &text);
    #[cfg(test)]
    if let Some(trace) = diagnostic {
        trace.record_io("atomic-write-return", &written);
    }
    written
}

pub(crate) fn canonicalize_last_root_for_persistence(document: &mut Value) {
    let Some(last_root) = document
        .get("last_root")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    let canonical = PathBuf::from(last_root)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(last_root));
    if let Value::Object(map) = document {
        map.insert(
            "last_root".to_string(),
            Value::String(canonical.to_string_lossy().to_string()),
        );
    }
}

fn canonicalize_default_root_for_persistence(document: &mut Value) {
    let Some(default_root) = document
        .get("default_root")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    let canonical = PathBuf::from(default_root)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(default_root));
    if let Value::Object(map) = document {
        map.insert(
            "default_root".to_string(),
            Value::String(canonical.to_string_lossy().to_string()),
        );
    }
}

fn merge_json_leaves(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(target), Value::Object(patch)) => {
            for (key, value) in patch {
                match target.get_mut(key) {
                    Some(existing) => merge_json_leaves(existing, value),
                    None => {
                        target.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (Value::Array(target), Value::Array(patch)) => {
            for (index, value) in patch.iter().enumerate() {
                if let Some(existing) = target.get_mut(index) {
                    merge_json_leaves(existing, value);
                } else {
                    target.push(value.clone());
                }
            }
            target.truncate(patch.len());
        }
        (target, patch) => *target = patch.clone(),
    }
}

fn normalize_history_delta(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

pub(super) fn normalize_history_recency(values: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for value in normalize_history_delta(values) {
        append_history_delta(&mut normalized, value);
    }
    normalized
}

pub(super) fn append_history_delta(history: &mut Vec<String>, delta: String) {
    history.retain(|existing| existing != &delta);
    history.push(delta);
    if history.len() > MAX_QUERY_HISTORY_ENTRIES {
        let trim = history.len() - MAX_QUERY_HISTORY_ENTRIES;
        history.drain(..trim);
    }
}

pub(super) fn history_delta_from_snapshot(previous: &[String], current: &[String]) -> Vec<String> {
    let current = normalize_history_recency(current.to_vec());
    for start in (0..=current.len()).rev() {
        let mut replayed = normalize_history_recency(previous.to_vec());
        for delta in &current[start..] {
            append_history_delta(&mut replayed, delta.clone());
        }
        if replayed == current {
            return current[start..].to_vec();
        }
    }
    current
}

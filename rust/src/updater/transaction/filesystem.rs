use super::model::{
    Phase, TargetRecord, TargetRole, TargetState, TransactionMarker, MARKER_VERSION,
};
#[cfg(target_os = "windows")]
use super::platform::windows_file_replace;
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub(super) const MARKER_FILE_NAME: &str = ".flistwalker-update.marker.json";
pub(super) const LOCK_FILE_NAME: &str = ".flistwalker-update.lock";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TargetClassification {
    Old,
    New,
    Unknown,
}

pub(super) fn classify_target(
    install_dir: &Path,
    marker: &TransactionMarker,
    record: &TargetRecord,
) -> Result<TargetClassification> {
    let hash = hash_if_regular(&target_path(install_dir, marker, record.role))?;
    if hash.as_deref() == Some(record.new_hash.as_str()) {
        return Ok(TargetClassification::New);
    }
    if record.originally_present && hash == record.old_hash {
        return Ok(TargetClassification::Old);
    }
    if !record.originally_present && hash.is_none() {
        return Ok(TargetClassification::Old);
    }
    Ok(TargetClassification::Unknown)
}

pub(super) fn verify_bundle_hashes(
    install_dir: &Path,
    marker: &TransactionMarker,
    expect_new: bool,
) -> Result<()> {
    for record in &marker.targets {
        let actual = hash_if_regular(&target_path(install_dir, marker, record.role))?;
        let valid = if expect_new {
            actual.as_deref() == Some(record.new_hash.as_str())
        } else if record.originally_present {
            actual == record.old_hash
        } else {
            actual.is_none()
        };
        if !valid {
            bail!(
                "bundle hash verification failed for {}",
                record.role.label()
            );
        }
    }
    Ok(())
}

pub(super) fn read_lock_record(install_dir: &Path) -> Result<(String, u32)> {
    let lock = install_dir.join(LOCK_FILE_NAME);
    validate_regular_file(&lock, "transaction lock")?;
    let contents = fs::read_to_string(&lock)
        .with_context(|| format!("failed to read transaction lock {}", lock.display()))?;
    let mut lines = contents.lines();
    let transaction_id = lines.next().context("orphan transaction ID is missing")?;
    validate_transaction_id(transaction_id)?;
    let parent_pid = lines
        .next()
        .context("orphan transaction owner PID is missing")?
        .parse::<u32>()
        .context("orphan transaction owner PID is invalid")?;
    if parent_pid == 0 || lines.next().is_some() {
        bail!("orphan transaction lock format is invalid");
    }
    Ok((transaction_id.to_string(), parent_pid))
}

pub(super) fn cleanup_committed(install_dir: &Path, marker: &TransactionMarker) -> Result<()> {
    verify_bundle_hashes(install_dir, marker, true)?;
    cleanup_transaction_artifacts(install_dir, marker, true)
}
pub(super) fn cleanup_rolled_back(install_dir: &Path, marker: &TransactionMarker) -> Result<()> {
    verify_bundle_hashes(install_dir, marker, false)?;
    cleanup_transaction_artifacts(install_dir, marker, true)
}
pub(super) fn cleanup_transaction_artifacts(
    install_dir: &Path,
    marker: &TransactionMarker,
    include_marker_and_lock: bool,
) -> Result<()> {
    validate_cleanup_artifacts(install_dir, marker, include_marker_and_lock)?;
    let transaction_id = &marker.transaction_id;
    for role in TargetRole::ORDER {
        remove_file_if_present(&new_path(install_dir, transaction_id, role))?;
        remove_file_if_present(&backup_path(install_dir, transaction_id, role))?;
        remove_file_if_present(&failed_path(install_dir, transaction_id, role))?;
    }
    remove_file_if_present(&ack_path(install_dir, transaction_id))?;
    remove_file_if_present(&helper_path(install_dir, transaction_id))?;
    remove_file_if_present(&marker_temp_path(install_dir, transaction_id))?;
    remove_file_if_present(
        &marker_temp_path(install_dir, transaction_id).with_extension("previous"),
    )?;
    if include_marker_and_lock {
        remove_file_if_present(&install_dir.join(MARKER_FILE_NAME))?;
        remove_file_if_present(&install_dir.join(LOCK_FILE_NAME))?;
    }
    sync_parent(install_dir)
}
pub(super) fn validate_cleanup_artifacts(
    install_dir: &Path,
    marker: &TransactionMarker,
    include_marker_and_lock: bool,
) -> Result<()> {
    validate_directory(install_dir, "transaction directory")?;
    for record in &marker.targets {
        validate_optional_hash(
            &new_path(install_dir, &marker.transaction_id, record.role),
            Some(&record.new_hash),
            "prepared update cleanup artifact",
        )?;
        validate_optional_hash(
            &backup_path(install_dir, &marker.transaction_id, record.role),
            record.old_hash.as_ref(),
            "backup cleanup artifact",
        )?;
        validate_optional_hash(
            &failed_path(install_dir, &marker.transaction_id, record.role),
            Some(&record.new_hash),
            "failed replacement cleanup artifact",
        )?;
    }
    let acknowledgement = ack_path(install_dir, &marker.transaction_id);
    if acknowledgement.try_exists().unwrap_or(false) {
        let token = marker
            .helper_start_token
            .as_deref()
            .context("acknowledgement exists without a helper token")?;
        validate_ack(install_dir, marker, token)?;
    }
    validate_optional_hash(
        &helper_path(install_dir, &marker.transaction_id),
        Some(&marker.helper_hash),
        "helper cleanup artifact",
    )?;
    for artifact in [
        marker_temp_path(install_dir, &marker.transaction_id),
        marker_temp_path(install_dir, &marker.transaction_id).with_extension("previous"),
    ] {
        if artifact.try_exists().unwrap_or(false) {
            validate_regular_file(&artifact, "marker cleanup artifact")?;
            let artifact_marker: TransactionMarker = serde_json::from_slice(
                &fs::read(&artifact)
                    .with_context(|| format!("failed to read {}", artifact.display()))?,
            )
            .context("failed to parse marker cleanup artifact")?;
            validate_marker(&artifact_marker)?;
            if artifact_marker.transaction_id != marker.transaction_id {
                bail!("marker cleanup artifact belongs to another transaction");
            }
        }
    }
    if include_marker_and_lock {
        validate_regular_file(
            &install_dir.join(MARKER_FILE_NAME),
            "transaction marker cleanup artifact",
        )?;
        let (lock_transaction_id, lock_parent_pid) = read_lock_record(install_dir)?;
        if lock_transaction_id != marker.transaction_id || lock_parent_pid != marker.parent_pid {
            bail!("transaction lock identity does not match marker");
        }
    }
    Ok(())
}
pub(super) fn validate_optional_hash(
    path: &Path,
    expected: Option<&String>,
    label: &str,
) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => {
            let expected = expected.with_context(|| format!("unexpected {label}"))?;
            validate_regular_file(path, label)?;
            if sha256_file(path)? != *expected {
                bail!("{label} hash mismatch: {}", path.display());
            }
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("failed to inspect {}", path.display())),
    }
}
pub(super) fn remove_file_if_present(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("failed to remove {}", path.display())),
    }
}

pub(super) fn read_marker(path: &Path) -> Result<TransactionMarker> {
    validate_regular_file(path, "transaction marker")?;
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let marker: TransactionMarker =
        serde_json::from_slice(&bytes).context("failed to parse update transaction marker")?;
    validate_marker(&marker)?;
    Ok(marker)
}
#[cfg(any(not(target_os = "macos"), test))]
pub(super) fn write_marker_new(path: &Path, marker: &TransactionMarker) -> Result<()> {
    validate_marker(marker)?;
    create_new_synced(
        path,
        &serde_json::to_vec(marker).context("failed to serialize marker")?,
    )
}
pub(super) fn write_marker_atomic(path: &Path, marker: &TransactionMarker) -> Result<()> {
    validate_marker(marker)?;
    let install_dir = path.parent().context("transaction marker has no parent")?;
    revalidate_transaction_directory(install_dir)?;
    if path.file_name().and_then(|value| value.to_str()) != Some(MARKER_FILE_NAME) {
        bail!("transaction marker path is not fixed");
    }
    validate_regular_file(path, "transaction marker replacement target")?;
    let temp = marker_temp_path(install_dir, &marker.transaction_id);
    create_new_synced(
        &temp,
        &serde_json::to_vec(marker).context("failed to serialize marker")?,
    )?;
    let result = replace_marker_file(&temp, path);
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result?;
    sync_parent(install_dir)
}

#[cfg(target_os = "windows")]
pub(super) fn replace_marker_file(source: &Path, destination: &Path) -> Result<()> {
    let backup = source.with_extension("previous");
    reject_existing(&backup, "marker replacement backup")?;
    windows_file_replace(source, destination, Some(&backup))?;
    fs::remove_file(&backup)
        .with_context(|| format!("failed to remove marker backup {}", backup.display()))
}
#[cfg(not(target_os = "windows"))]
pub(super) fn replace_marker_file(source: &Path, destination: &Path) -> Result<()> {
    fs::rename(source, destination).with_context(|| {
        format!(
            "failed to replace marker {} with {}",
            destination.display(),
            source.display()
        )
    })
}
#[cfg(target_os = "windows")]
pub(super) fn replace_existing(source: &Path, target: &Path, backup: &Path) -> Result<()> {
    windows_file_replace(source, target, Some(backup))
}
#[cfg(not(target_os = "windows"))]
pub(super) fn replace_existing(source: &Path, target: &Path, backup: &Path) -> Result<()> {
    copy_new_synced(target, backup)?;
    sync_parent(target.parent().context("target has no parent")?)?;
    fs::rename(source, target)
        .with_context(|| format!("failed to replace {}", target.display()))?;
    sync_parent(target.parent().context("target has no parent")?)
}
#[cfg(target_os = "windows")]
pub(super) fn restore_existing(
    backup: &Path,
    target: &Path,
    install_dir: &Path,
    role: TargetRole,
) -> Result<()> {
    let marker = read_marker(&install_dir.join(MARKER_FILE_NAME))?;
    let failed = failed_path(install_dir, &marker.transaction_id, role);
    windows_file_replace(backup, target, Some(&failed))?;
    fs::remove_file(&failed).with_context(|| format!("failed to remove {}", failed.display()))
}
#[cfg(not(target_os = "windows"))]
pub(super) fn restore_existing(
    backup: &Path,
    target: &Path,
    install_dir: &Path,
    _role: TargetRole,
) -> Result<()> {
    fs::rename(backup, target)
        .with_context(|| format!("failed to restore {}", target.display()))?;
    sync_parent(install_dir)
}

pub(super) fn promote_absent_no_overwrite(
    source: &Path,
    target: &Path,
    install_dir: &Path,
) -> Result<()> {
    fs::hard_link(source, target).with_context(|| {
        format!(
            "failed to promote absent target without overwrite {}",
            target.display()
        )
    })?;
    sync_parent(install_dir)?;
    fs::remove_file(source)
        .with_context(|| format!("failed to remove promoted source {}", source.display()))?;
    sync_parent(install_dir)
}

pub(super) fn revalidate_operation_paths(
    install_dir: &Path,
    target: &Path,
    prepared: &Path,
    backup: &Path,
    record: &TargetRecord,
) -> Result<()> {
    revalidate_transaction_directory(install_dir)?;
    for path in [target, prepared, backup] {
        if path.parent() != Some(install_dir) {
            bail!("transaction path escaped executable directory");
        }
    }
    validate_regular_file(prepared, "prepared update file")?;
    validate_target_if_present(target, "installation target")?;
    if backup.try_exists().unwrap_or(false) {
        bail!("update backup already exists");
    }
    if target.try_exists().unwrap_or(false) != record.originally_present {
        bail!("installation target presence changed during update");
    }
    if sha256_file(prepared)? != record.new_hash {
        bail!(
            "prepared update new hash changed for {}",
            record.role.label()
        );
    }
    if record.originally_present {
        let expected = record
            .old_hash
            .as_deref()
            .context("existing target is missing its old hash")?;
        if sha256_file(target)? != expected {
            bail!(
                "installation target old hash changed for {}",
                record.role.label()
            );
        }
    }
    Ok(())
}

pub(super) fn revalidate_rollback_paths(
    install_dir: &Path,
    marker: &TransactionMarker,
    target: &Path,
    backup: &Path,
    record: &TargetRecord,
) -> Result<()> {
    revalidate_transaction_directory(install_dir)?;
    let failed = failed_path(install_dir, &marker.transaction_id, record.role);
    for path in [target, backup, &failed] {
        if path.parent() != Some(install_dir) {
            bail!("rollback path escaped executable directory");
        }
    }
    validate_target_if_present(target, "rollback target")?;
    if backup.try_exists().unwrap_or(false) {
        validate_regular_file(backup, "rollback backup")?;
    }
    if failed.try_exists().unwrap_or(false) {
        validate_regular_file(&failed, "rollback failed-target evidence")?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn revalidate_rollback_hashes(
    install_dir: &Path,
    marker: &TransactionMarker,
    target: &Path,
    backup: &Path,
    record: &TargetRecord,
    expected_target: Option<&str>,
    expected_backup: Option<&str>,
) -> Result<()> {
    revalidate_rollback_paths(install_dir, marker, target, backup, record)?;
    if hash_if_regular(target)?.as_deref() != expected_target {
        bail!(
            "rollback target hash changed immediately before mutation for {}",
            record.role.label()
        );
    }
    if hash_if_regular(backup)?.as_deref() != expected_backup {
        bail!(
            "rollback backup hash changed immediately before mutation for {}",
            record.role.label()
        );
    }
    Ok(())
}

pub(super) fn revalidate_transaction_directory(install_dir: &Path) -> Result<()> {
    validate_directory(install_dir, "transaction directory")?;
    if install_dir
        .canonicalize()
        .context("failed to revalidate transaction directory")?
        != install_dir
    {
        bail!("transaction directory identity changed");
    }
    Ok(())
}
pub(super) fn validate_ack(
    install_dir: &Path,
    marker: &TransactionMarker,
    token: &str,
) -> Result<()> {
    validate_regular_file(
        &ack_path(install_dir, &marker.transaction_id),
        "helper acknowledgement",
    )?;
    let expected = format!("{}\n{}\n", marker.transaction_id, token);
    let actual = fs::read_to_string(ack_path(install_dir, &marker.transaction_id))
        .context("helper acknowledgement is missing")?;
    if actual != expected {
        bail!("helper acknowledgement does not match transaction");
    }
    Ok(())
}
pub(super) fn validated_marker_parent(path: &Path, marker: &TransactionMarker) -> Result<PathBuf> {
    validate_marker(marker)?;
    if path.file_name().and_then(|value| value.to_str()) != Some(MARKER_FILE_NAME) {
        bail!("transaction marker path is not fixed");
    }
    let canonical = path
        .parent()
        .context("transaction marker has no parent")?
        .canonicalize()
        .context("failed to canonicalize transaction directory")?;
    validate_directory(&canonical, "transaction directory")?;
    Ok(canonical)
}
pub(super) fn validate_marker(marker: &TransactionMarker) -> Result<()> {
    if marker.version != MARKER_VERSION {
        bail!("unsupported transaction marker version");
    }
    validate_transaction_id(&marker.transaction_id)?;
    if !is_safe_basename(&marker.binary_name) {
        bail!("invalid marker binary name");
    }
    if marker.targets.len() != TargetRole::ORDER.len()
        || marker
            .targets
            .iter()
            .map(|record| record.role)
            .ne(TargetRole::ORDER)
    {
        bail!("invalid marker target role order");
    }
    if marker.parent_pid == 0 || !is_sha256(&marker.helper_hash) {
        bail!("invalid marker process or helper identity");
    }
    for record in &marker.targets {
        if !is_sha256(&record.new_hash)
            || record.originally_present != record.old_hash.is_some()
            || record
                .old_hash
                .as_deref()
                .is_some_and(|hash| !is_sha256(hash))
        {
            bail!("invalid marker target hash contract");
        }
    }
    match marker.phase {
        Phase::PreparedParentOwned => {
            if marker.helper_pid.is_some()
                || marker.helper_start_token.is_some()
                || !all_states(&marker.targets, TargetState::Prepared)
            {
                bail!("invalid parent-owned transaction state");
            }
        }
        Phase::HelperRegistered => {
            validate_registered_helper(marker)?;
            if !all_states(&marker.targets, TargetState::Prepared) {
                bail!("registered helper cannot have mutated targets");
            }
        }
        Phase::ApplyingSidecars => {
            validate_registered_helper(marker)?;
            if marker.targets[3].state != TargetState::Prepared
                || !is_forward_prefix(&marker.targets[..3])
            {
                bail!("invalid sidecar application state");
            }
        }
        Phase::BinaryIntent => {
            validate_registered_helper(marker)?;
            if !marker.targets[..3]
                .iter()
                .all(|record| record.state == TargetState::Applied)
                || !matches!(
                    marker.targets[3].state,
                    TargetState::Prepared | TargetState::Intent | TargetState::Applied
                )
            {
                bail!("invalid binary commit-intent state");
            }
        }
        Phase::BinaryCommitted => {
            validate_registered_helper(marker)?;
            if !all_states(&marker.targets, TargetState::Applied) {
                bail!("committed transaction must contain only applied targets");
            }
        }
        Phase::RollingBack => {
            validate_optional_helper(marker)?;
            if !is_rollback_suffix(&marker.targets) {
                bail!("invalid rollback transition");
            }
        }
        Phase::RolledBack => {
            validate_optional_helper(marker)?;
            if !all_states(&marker.targets, TargetState::RolledBack) {
                bail!("rolled-back transaction has incomplete target state");
            }
        }
    }
    Ok(())
}
pub(super) fn validate_registered_helper(marker: &TransactionMarker) -> Result<()> {
    if marker.helper_pid.is_none()
        || marker.helper_pid == Some(0)
        || marker
            .helper_start_token
            .as_deref()
            .is_none_or(|token| validate_start_token(token).is_err())
    {
        bail!("invalid registered helper identity");
    }
    Ok(())
}
pub(super) fn validate_optional_helper(marker: &TransactionMarker) -> Result<()> {
    match (&marker.helper_pid, &marker.helper_start_token) {
        (None, None) => Ok(()),
        (Some(_), Some(_)) => validate_registered_helper(marker),
        _ => bail!("incomplete helper identity"),
    }
}
pub(super) fn all_states(targets: &[TargetRecord], expected: TargetState) -> bool {
    targets.iter().all(|record| record.state == expected)
}
pub(super) fn is_forward_prefix(targets: &[TargetRecord]) -> bool {
    let mut stage = 0u8;
    for record in targets {
        stage = match (stage, record.state) {
            (0, TargetState::Applied) => 0,
            (0, TargetState::Intent) => 1,
            (0 | 1, TargetState::Prepared) => 2,
            (2, TargetState::Prepared) => 2,
            _ => return false,
        };
    }
    true
}
pub(super) fn is_rollback_suffix(targets: &[TargetRecord]) -> bool {
    let split = targets
        .iter()
        .position(|record| record.state == TargetState::RolledBack)
        .unwrap_or(targets.len());
    is_forward_prefix(&targets[..split])
        && targets[split..]
            .iter()
            .all(|record| record.state == TargetState::RolledBack)
}
pub(super) fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
pub(super) fn validate_transaction_id(value: &str) -> Result<()> {
    if value.len() != 32
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("transaction ID must be 32 lowercase hexadecimal characters");
    }
    Ok(())
}
pub(super) fn validate_start_token(value: &str) -> Result<()> {
    if value.len() < 16
        || value.len() > 128
        || !value.is_ascii()
        || value.bytes().any(|byte| byte.is_ascii_whitespace())
    {
        bail!("invalid helper start token");
    }
    Ok(())
}
pub(super) fn is_safe_basename(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && !value.contains('/')
        && !value.contains('\\')
        && !value.bytes().any(|byte| byte.is_ascii_control())
}
pub(super) fn validate_directory(path: &Path, label: &str) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {label} {}", path.display()))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
        bail!("{label} must be a non-link directory");
    }
    Ok(())
}
pub(super) fn validate_regular_file(path: &Path, label: &str) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {label} {}", path.display()))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
        bail!("{label} must be a non-link regular file");
    }
    Ok(())
}
pub(super) fn validate_target_if_present(path: &Path, label: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata)
            if metadata.is_file()
                && !metadata.file_type().is_symlink()
                && !is_reparse_point(&metadata) =>
        {
            Ok(())
        }
        Ok(_) => bail!("{label} must be a non-link regular file when present"),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("failed to inspect {}", path.display())),
    }
}
pub(super) fn reject_existing(path: &Path, label: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => bail!("{label} already exists"),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("failed to inspect {}", path.display())),
    }
}
#[cfg(target_os = "windows")]
pub(super) fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}
#[cfg(not(target_os = "windows"))]
pub(super) fn is_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
}

pub(super) fn create_new_synced(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("failed to create {}", path.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("failed to write {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("failed to sync {}", path.display()))
}
pub(super) fn copy_new_synced(source: &Path, destination: &Path) -> Result<String> {
    let mut input =
        File::open(source).with_context(|| format!("failed to open {}", source.display()))?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = input
            .read(&mut buffer)
            .with_context(|| format!("failed to read {}", source.display()))?;
        if count == 0 {
            break;
        }
        output
            .write_all(&buffer[..count])
            .with_context(|| format!("failed to write {}", destination.display()))?;
        hasher.update(&buffer[..count]);
    }
    let permissions = fs::metadata(source)
        .with_context(|| format!("failed to read permissions {}", source.display()))?
        .permissions();
    fs::set_permissions(destination, permissions)
        .with_context(|| format!("failed to set permissions {}", destination.display()))?;
    output
        .sync_all()
        .with_context(|| format!("failed to sync {}", destination.display()))?;
    Ok(format!("{:x}", hasher.finalize()))
}
pub(super) fn sha256_file(path: &Path) -> Result<String> {
    let mut input =
        File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = input
            .read(&mut buffer)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
pub(super) fn hash_if_regular(path: &Path) -> Result<Option<String>> {
    match fs::symlink_metadata(path) {
        Ok(metadata)
            if metadata.is_file()
                && !metadata.file_type().is_symlink()
                && !is_reparse_point(&metadata) =>
        {
            Ok(Some(sha256_file(path)?))
        }
        Ok(_) => bail!(
            "transaction target is not a regular file: {}",
            path.display()
        ),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err).with_context(|| format!("failed to inspect {}", path.display())),
    }
}
#[cfg(all(unix, not(target_os = "macos")))]
pub(super) fn sync_parent(path: &Path) -> Result<()> {
    File::open(path)
        .with_context(|| format!("failed to open directory {}", path.display()))?
        .sync_all()
        .with_context(|| format!("failed to sync directory {}", path.display()))
}
#[cfg(not(all(unix, not(target_os = "macos"))))]
pub(super) fn sync_parent(_path: &Path) -> Result<()> {
    Ok(())
}

pub(super) fn new_path(dir: &Path, transaction_id: &str, role: TargetRole) -> PathBuf {
    dir.join(format!(
        ".flistwalker-update-{transaction_id}-{}.new",
        role.label()
    ))
}
pub(super) fn backup_path(dir: &Path, transaction_id: &str, role: TargetRole) -> PathBuf {
    dir.join(format!(
        ".flistwalker-update-{transaction_id}-{}.bak",
        role.label()
    ))
}
pub(super) fn failed_path(dir: &Path, transaction_id: &str, role: TargetRole) -> PathBuf {
    dir.join(format!(
        ".flistwalker-update-{transaction_id}-{}.failed",
        role.label()
    ))
}
pub(super) fn helper_path(dir: &Path, transaction_id: &str) -> PathBuf {
    let extension = if cfg!(target_os = "windows") {
        ".exe"
    } else {
        ""
    };
    dir.join(format!(
        ".flistwalker-update-{transaction_id}-helper{extension}"
    ))
}
pub(super) fn ack_path(dir: &Path, transaction_id: &str) -> PathBuf {
    dir.join(format!(".flistwalker-update-{transaction_id}.ack"))
}
pub(super) fn marker_temp_path(dir: &Path, transaction_id: &str) -> PathBuf {
    dir.join(format!(".flistwalker-update-{transaction_id}.marker.tmp"))
}
pub(super) fn target_path(dir: &Path, marker: &TransactionMarker, role: TargetRole) -> PathBuf {
    dir.join(role.target_name(&marker.binary_name))
}

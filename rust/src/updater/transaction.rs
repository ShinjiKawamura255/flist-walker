#[cfg(test)]
mod tests;

mod filesystem;
mod model;
mod platform;

use filesystem::*;
pub(super) use model::RecoveryOutcome;
#[cfg(any(not(target_os = "macos"), test))]
pub(super) use model::TransactionSources;
use model::{Phase, TargetRole, TargetState, TransactionMarker};
#[cfg(any(not(target_os = "macos"), test))]
use model::{TargetRecord, MARKER_VERSION};
#[cfg(target_os = "windows")]
pub(super) use platform::windows_hidden_child_command;
use platform::*;

#[cfg(any(not(target_os = "macos"), test))]
use crate::updater::BinaryVariant;
use crate::updater::UpdateRestartMode;
use anyhow::{bail, Context, Result};
#[cfg(not(target_os = "macos"))]
use rand_core::{OsRng, RngCore};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const FAILURE_RECORD_FILE_NAME: &str = ".flistwalker-update-failure.json";
const FAILURE_RECORD_VERSION: u32 = 1;
const FAILURE_RECORD_MAX_BYTES: u64 = 16 * 1024;
const RECOVERY_RETRY_INTERVAL: Duration = Duration::from_millis(25);
const STARTUP_RECOVERY_WAIT: Duration = Duration::from_secs(5);
const INTERNAL_RESTART_RECOVERY_WAIT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy)]
enum RecoveryWaitMode {
    Startup,
    InternalRestartHandoff,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct UpdateFailureRecord {
    version: u32,
    transaction_id: String,
    message: String,
}

#[cfg(any(not(target_os = "macos"), test))]
pub(super) struct PreparedTransaction {
    install_dir: PathBuf,
    marker_path: PathBuf,
    #[cfg(test)]
    lock_path: PathBuf,
    ack_path: PathBuf,
    helper_path: PathBuf,
    transaction_id: String,
    armed: bool,
}

#[cfg(any(not(target_os = "macos"), test))]
impl PreparedTransaction {
    #[cfg(test)]
    pub(super) fn install_dir(&self) -> &Path {
        &self.install_dir
    }
    pub(super) fn marker_path(&self) -> &Path {
        &self.marker_path
    }
    #[cfg(test)]
    pub(super) fn lock_path(&self) -> &Path {
        &self.lock_path
    }
    #[cfg(test)]
    pub(super) fn ack_path(&self) -> &Path {
        &self.ack_path
    }
    pub(super) fn helper_path(&self) -> &Path {
        &self.helper_path
    }
    #[cfg(test)]
    pub(super) fn target_roles(&self) -> [TargetRole; 4] {
        TargetRole::ORDER
    }
    #[cfg(test)]
    pub(super) fn new_paths(&self) -> Vec<PathBuf> {
        TargetRole::ORDER
            .into_iter()
            .map(|role| new_path(&self.install_dir, &self.transaction_id, role))
            .collect()
    }
    pub(super) fn register_helper(&mut self, helper_pid: u32, start_token: &str) -> Result<()> {
        validate_start_token(start_token)?;
        let mut marker = read_marker(&self.marker_path)?;
        if marker.phase != Phase::PreparedParentOwned || marker.helper_pid.is_some() {
            bail!("helper registration requires prepared parent-owned transaction");
        }
        marker.helper_pid = Some(helper_pid);
        marker.helper_start_token = Some(start_token.to_string());
        marker.phase = Phase::HelperRegistered;
        write_marker_atomic(&self.marker_path, &marker)
    }
    pub(super) fn disarm(&mut self) {
        self.armed = false;
    }
}

#[cfg(any(not(target_os = "macos"), test))]
impl Drop for PreparedTransaction {
    fn drop(&mut self) {
        if self.armed {
            if let Ok(marker) = read_marker(&self.marker_path) {
                let _ = cleanup_transaction_artifacts(&self.install_dir, &marker, true);
            }
        }
    }
}

#[cfg(test)]
pub(super) fn prepare_transaction_with_id(
    current_exe: &Path,
    sources: TransactionSources<'_>,
    transaction_id: &str,
    parent_pid: u32,
) -> Result<PreparedTransaction> {
    prepare_transaction_with_id_for_variant(
        current_exe,
        sources,
        transaction_id,
        parent_pid,
        BinaryVariant::Universal,
    )
}

#[cfg(any(not(target_os = "macos"), test))]
pub(super) fn prepare_transaction_with_id_for_variant(
    current_exe: &Path,
    sources: TransactionSources<'_>,
    transaction_id: &str,
    parent_pid: u32,
    variant: BinaryVariant,
) -> Result<PreparedTransaction> {
    validate_transaction_id(transaction_id)?;
    validate_regular_file(current_exe, "current executable")?;
    let canonical_exe = current_exe
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", current_exe.display()))?;
    let install_dir = canonical_exe
        .parent()
        .context("current executable has no parent")?
        .canonicalize()
        .context("failed to canonicalize executable directory")?;
    validate_directory(&install_dir, "executable directory")?;
    let binary_name = canonical_exe
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| is_safe_basename(value))
        .context("current executable filename is not a safe basename")?
        .to_string();
    let sidecar_prefix = match variant {
        BinaryVariant::Universal => None,
        BinaryVariant::Cli => Some("fw.".to_string()),
    };
    let marker_path = install_dir.join(MARKER_FILE_NAME);
    let lock_path = install_dir.join(LOCK_FILE_NAME);
    reject_existing(&marker_path, "transaction marker")?;
    reject_existing(&lock_path, "transaction lock")?;

    let mut owned = OwnedPreparation::default();
    create_new_synced(
        &lock_path,
        format!("{transaction_id}\n{parent_pid}\n").as_bytes(),
    )
    .context("failed to acquire updater transaction lock")?;
    owned.paths.push(lock_path.clone());

    let helper_path = helper_path(&install_dir, transaction_id);
    reject_existing(&helper_path, "helper executable")?;
    copy_new_synced(&canonical_exe, &helper_path).context("failed to prepare updater helper")?;
    owned.paths.push(helper_path.clone());
    let helper_hash = sha256_file(&helper_path)?;

    let mut targets = Vec::with_capacity(TargetRole::ORDER.len());
    for role in TargetRole::ORDER {
        let source = sources.for_role(role);
        validate_regular_file(source, "verified update source")?;
        let target = install_dir.join(role.target_name(&binary_name, sidecar_prefix.as_deref()));
        validate_target_if_present(&target, "installation target")?;
        let prepared_path = new_path(&install_dir, transaction_id, role);
        let backup = backup_path(&install_dir, transaction_id, role);
        reject_existing(&prepared_path, "prepared update file")?;
        reject_existing(&backup, "update backup")?;
        let source_hash = sha256_file(source)?;
        let copied_hash = copy_new_synced(source, &prepared_path)?;
        if source_hash != copied_hash {
            bail!("prepared update hash mismatch for {}", role.label());
        }
        owned.paths.push(prepared_path);
        let originally_present = target.try_exists().unwrap_or(false);
        let old_hash = if originally_present {
            Some(sha256_file(&target)?)
        } else {
            None
        };
        targets.push(TargetRecord {
            role,
            originally_present,
            old_hash,
            new_hash: copied_hash,
            state: TargetState::Prepared,
        });
    }
    let marker = TransactionMarker {
        version: MARKER_VERSION,
        transaction_id: transaction_id.to_string(),
        binary_name: binary_name.clone(),
        sidecar_prefix,
        parent_pid,
        helper_pid: None,
        helper_start_token: None,
        helper_hash,
        phase: Phase::PreparedParentOwned,
        targets,
    };
    write_marker_new(&marker_path, &marker)?;
    owned.paths.push(marker_path.clone());
    sync_parent(&install_dir)?;
    let prepared = PreparedTransaction {
        install_dir: install_dir.clone(),
        marker_path,
        #[cfg(test)]
        lock_path,
        ack_path: ack_path(&install_dir, transaction_id),
        helper_path,
        transaction_id: transaction_id.to_string(),
        armed: true,
    };
    owned.disarm();
    Ok(prepared)
}

#[cfg(any(not(target_os = "macos"), test))]
#[derive(Default)]
struct OwnedPreparation {
    paths: Vec<PathBuf>,
}
#[cfg(any(not(target_os = "macos"), test))]
impl OwnedPreparation {
    fn disarm(&mut self) {
        self.paths.clear();
    }
}
#[cfg(any(not(target_os = "macos"), test))]
impl Drop for OwnedPreparation {
    fn drop(&mut self) {
        for path in self.paths.iter().rev() {
            let _ = fs::remove_file(path);
        }
    }
}

pub(super) fn acknowledge_registered_helper(
    marker_path: &Path,
    helper_pid: u32,
    start_token: &str,
    actual_helper_path: &Path,
) -> Result<PathBuf> {
    let marker = read_marker(marker_path)?;
    let install_dir = validated_marker_parent(marker_path, &marker)?;
    if marker.phase != Phase::HelperRegistered
        || marker.helper_pid != Some(helper_pid)
        || marker.helper_start_token.as_deref() != Some(start_token)
    {
        bail!("helper registration does not match durable marker");
    }
    let expected_helper = helper_path(&install_dir, &marker.transaction_id);
    if actual_helper_path.canonicalize().ok() != expected_helper.canonicalize().ok()
        || sha256_file(actual_helper_path)? != marker.helper_hash
    {
        bail!("helper registration executable identity mismatch");
    }
    let path = ack_path(&install_dir, &marker.transaction_id);
    create_new_synced(
        &path,
        format!("{}\n{}\n", marker.transaction_id, start_token).as_bytes(),
    )?;
    sync_parent(&install_dir)?;
    Ok(path)
}

pub(super) trait FailureInjector {
    fn after_applied(&mut self, _role: TargetRole) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
pub(super) fn execute_registered_transaction(
    marker_path: &Path,
    start_token: &str,
    process: &mut impl ProcessControl,
    failures: &mut impl FailureInjector,
) -> Result<()> {
    execute_registered_transaction_with_restart_mode(
        marker_path,
        start_token,
        process,
        failures,
        UpdateRestartMode::Headless,
    )
}

fn execute_registered_transaction_with_restart_mode(
    marker_path: &Path,
    start_token: &str,
    process: &mut impl ProcessControl,
    failures: &mut impl FailureInjector,
    restart_mode: UpdateRestartMode,
) -> Result<()> {
    let mut marker = read_marker(marker_path)?;
    let install_dir = validated_marker_parent(marker_path, &marker)?;
    recover_marker_update_artifacts(&install_dir, &marker)?;
    if marker.phase != Phase::HelperRegistered
        || marker.helper_start_token.as_deref() != Some(start_token)
    {
        bail!("transaction helper registration is not valid");
    }
    validate_ack(&install_dir, &marker, start_token)?;
    if !process.wait_for_exit(marker.parent_pid, Duration::from_secs(30))? {
        rollback_transaction(&install_dir, marker_path, &mut marker)?;
        cleanup_rolled_back(&install_dir, &marker)?;
        bail!("parent process did not exit within 30 seconds");
    }
    marker.phase = Phase::ApplyingSidecars;
    write_marker_atomic(marker_path, &marker)?;
    let apply_result = (|| {
        for index in 0..marker.targets.len() {
            let role = marker.targets[index].role;
            if role == TargetRole::Binary {
                marker.phase = Phase::BinaryIntent;
                write_marker_atomic(marker_path, &marker)?;
            }
            marker.targets[index].state = TargetState::Intent;
            write_marker_atomic(marker_path, &marker)?;
            apply_one_target(&install_dir, &marker, index)?;
            marker.targets[index].state = TargetState::Applied;
            write_marker_atomic(marker_path, &marker)?;
            failures.after_applied(role)?;
        }
        verify_bundle_hashes(&install_dir, &marker, true)?;
        marker.phase = Phase::BinaryCommitted;
        write_marker_atomic(marker_path, &marker)
    })();
    if let Err(err) = apply_result {
        rollback_transaction(&install_dir, marker_path, &mut marker)?;
        return Err(err).context("update activation failed and was rolled back");
    }
    let binary = target_path(&install_dir, &marker, TargetRole::Binary);
    if let Err(err) = process.restart(&binary, restart_mode) {
        rollback_transaction(&install_dir, marker_path, &mut marker)?;
        let restart_error =
            anyhow::anyhow!("failed to restart updated application; old bundle restored: {err:#}");
        // Persist before launching the restored binary. The restarted process can reach startup
        // diagnostics immediately after spawn, so writing only after restart returns would race
        // and defer the message until a later application launch.
        let _ = write_failure_record(&install_dir, &marker.transaction_id, &restart_error);
        // Regression guard: rollback must preserve the invoking variant's launch mode;
        // restarting `fw` as GUI can run an unintended headless search with no arguments.
        if let Err(rollback_restart_error) = process.restart(&binary, restart_mode) {
            return Err(anyhow::anyhow!(
                "failed to restart updated application: {err:#}; old bundle restored but its restart also failed: {rollback_restart_error:#}"
            ));
        }
        return Err(restart_error);
    }
    Ok(())
}

fn recover_marker_update_artifacts(install_dir: &Path, marker: &TransactionMarker) -> Result<()> {
    let temp = marker_temp_path(install_dir, &marker.transaction_id);
    let previous = temp.with_extension("previous");
    for artifact in [temp, previous] {
        match fs::symlink_metadata(&artifact) {
            Ok(_) => {
                validate_regular_file(&artifact, "interrupted marker artifact")?;
                let bytes = fs::read(&artifact)
                    .with_context(|| format!("failed to read {}", artifact.display()))?;
                let artifact_marker: TransactionMarker = serde_json::from_slice(&bytes)
                    .context("failed to parse interrupted marker artifact")?;
                validate_marker(&artifact_marker)?;
                if artifact_marker.transaction_id != marker.transaction_id {
                    bail!("interrupted marker artifact belongs to another transaction");
                }
                fs::remove_file(&artifact).with_context(|| {
                    format!("failed to remove marker artifact {}", artifact.display())
                })?;
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("failed to inspect {}", artifact.display()));
            }
        }
    }
    sync_parent(install_dir)
}

fn apply_one_target(install_dir: &Path, marker: &TransactionMarker, index: usize) -> Result<()> {
    let record = &marker.targets[index];
    let target = target_path(install_dir, marker, record.role);
    let prepared = new_path(install_dir, &marker.transaction_id, record.role);
    let backup = backup_path(install_dir, &marker.transaction_id, record.role);
    revalidate_operation_paths(install_dir, &target, &prepared, &backup, record)?;
    if record.originally_present {
        replace_existing(&prepared, &target, &backup)?;
    } else {
        promote_absent_no_overwrite(&prepared, &target, install_dir)?;
    }
    if sha256_file(&target)? != record.new_hash {
        bail!("installed target hash mismatch for {}", record.role.label());
    }
    Ok(())
}

fn rollback_transaction(
    install_dir: &Path,
    marker_path: &Path,
    marker: &mut TransactionMarker,
) -> Result<()> {
    if marker.phase != Phase::RollingBack {
        marker.phase = Phase::RollingBack;
        write_marker_atomic(marker_path, marker)?;
    }
    for index in (0..marker.targets.len()).rev() {
        let record = marker.targets[index].clone();
        let target = target_path(install_dir, marker, record.role);
        let backup = backup_path(install_dir, &marker.transaction_id, record.role);
        revalidate_rollback_paths(install_dir, marker, &target, &backup, &record)?;
        let target_hash = hash_if_regular(&target)?;
        if record.originally_present {
            let old_hash = record
                .old_hash
                .as_deref()
                .context("missing old target hash")?;
            if target_hash.as_deref() == Some(old_hash) {
                match hash_if_regular(&backup)? {
                    None => {}
                    Some(hash) if hash == old_hash => {
                        revalidate_rollback_hashes(
                            install_dir,
                            marker,
                            &target,
                            &backup,
                            &record,
                            Some(old_hash),
                            Some(old_hash),
                        )?;
                        fs::remove_file(&backup).with_context(|| {
                            format!("failed to remove verified backup {}", backup.display())
                        })?;
                        sync_parent(install_dir)?;
                    }
                    Some(_) => {
                        bail!("ambiguous rollback backup for {}", record.role.label());
                    }
                }
            } else if target_hash.as_deref() == Some(record.new_hash.as_str())
                && hash_if_regular(&backup)?.as_deref() == Some(old_hash)
            {
                revalidate_rollback_hashes(
                    install_dir,
                    marker,
                    &target,
                    &backup,
                    &record,
                    Some(&record.new_hash),
                    Some(old_hash),
                )?;
                restore_existing(&backup, &target, install_dir, record.role)?;
            } else if record.state != TargetState::Prepared {
                bail!("ambiguous rollback state for {}", record.role.label());
            }
        } else if target_hash.as_deref() == Some(record.new_hash.as_str()) {
            revalidate_rollback_hashes(
                install_dir,
                marker,
                &target,
                &backup,
                &record,
                Some(&record.new_hash),
                None,
            )?;
            fs::remove_file(&target)
                .with_context(|| format!("failed to remove {}", target.display()))?;
            sync_parent(install_dir)?;
        } else if target_hash.is_some() {
            bail!("ambiguous rollback state for {}", record.role.label());
        }
        marker.targets[index].state = TargetState::RolledBack;
        write_marker_atomic(marker_path, marker)?;
    }
    verify_bundle_hashes(install_dir, marker, false)?;
    marker.phase = Phase::RolledBack;
    write_marker_atomic(marker_path, marker)
}

pub(super) fn recover_transaction(
    marker_path: &Path,
    process_probe: &impl ProcessProbe,
) -> Result<RecoveryOutcome> {
    let mut marker = match read_marker(marker_path) {
        Ok(marker) => marker,
        Err(_) => return Ok(RecoveryOutcome::Ambiguous),
    };
    let install_dir = validated_marker_parent(marker_path, &marker)?;
    if process_probe.is_alive(marker.parent_pid)
        && !process_probe.is_current_process(marker.parent_pid)
    {
        return Ok(RecoveryOutcome::Deferred);
    }
    if let Some(pid) = marker.helper_pid {
        if process_probe.is_alive(pid) && !process_probe.is_current_process(pid) {
            let helper = helper_path(&install_dir, &marker.transaction_id);
            let helper_file_matches = hash_if_regular(&helper).ok().flatten().as_deref()
                == Some(marker.helper_hash.as_str());
            if !helper_file_matches || !process_probe.executable_matches(pid, &helper) {
                return Ok(RecoveryOutcome::Ambiguous);
            }
            let acknowledgement = ack_path(&install_dir, &marker.transaction_id);
            match fs::symlink_metadata(&acknowledgement) {
                Ok(_) => {
                    if validate_ack(
                        &install_dir,
                        &marker,
                        marker.helper_start_token.as_deref().unwrap_or_default(),
                    )
                    .is_err()
                    {
                        return Ok(RecoveryOutcome::Ambiguous);
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    if marker.phase != Phase::HelperRegistered {
                        return Ok(RecoveryOutcome::Ambiguous);
                    }
                }
                Err(_) => return Ok(RecoveryOutcome::Ambiguous),
            }
            return Ok(RecoveryOutcome::Deferred);
        }
    }
    recover_marker_update_artifacts(&install_dir, &marker)?;
    let classifications = marker
        .targets
        .iter()
        .map(|record| classify_target(&install_dir, &marker, record))
        .collect::<Result<Vec<_>>>()?;
    if classifications.contains(&TargetClassification::Unknown) {
        return Ok(RecoveryOutcome::Ambiguous);
    }
    if marker.phase == Phase::BinaryCommitted
        || (marker.phase == Phase::BinaryIntent
            && classifications
                .iter()
                .all(|value| *value == TargetClassification::New))
    {
        if classifications
            .iter()
            .all(|value| *value == TargetClassification::New)
        {
            marker.phase = Phase::BinaryCommitted;
            write_marker_atomic(marker_path, &marker)?;
            if cleanup_committed(&install_dir, &marker).is_err() {
                return Ok(RecoveryOutcome::Ambiguous);
            }
            return Ok(RecoveryOutcome::Committed);
        }
        return Ok(RecoveryOutcome::Ambiguous);
    }
    if rollback_transaction(&install_dir, marker_path, &mut marker).is_err() {
        return Ok(RecoveryOutcome::Ambiguous);
    }
    if cleanup_rolled_back(&install_dir, &marker).is_err() {
        return Ok(RecoveryOutcome::Ambiguous);
    }
    Ok(RecoveryOutcome::RolledBack)
}

#[cfg(not(target_os = "macos"))]
pub(super) fn prepare_transaction(
    current_exe: &Path,
    sources: TransactionSources<'_>,
    variant: BinaryVariant,
) -> Result<PreparedTransaction> {
    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    prepare_transaction_with_id_for_variant(
        current_exe,
        sources,
        &hex_bytes(&bytes),
        std::process::id(),
        variant,
    )
}

#[cfg(not(target_os = "macos"))]
pub(super) fn new_start_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex_bytes(&bytes)
}

#[cfg(not(target_os = "macos"))]
impl PreparedTransaction {
    pub(super) fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    pub(super) fn acknowledgement_matches(&self, token: &str) -> bool {
        fs::read_to_string(&self.ack_path)
            .ok()
            .is_some_and(|value| value == format!("{}\n{}\n", self.transaction_id, token))
    }
}

pub(super) fn run_internal_helper(
    marker_path: &Path,
    transaction_id: &str,
    start_token: &str,
    restart_mode: UpdateRestartMode,
) -> Result<()> {
    validate_transaction_id(transaction_id)?;
    validate_start_token(start_token)?;
    let actual_helper = std::env::current_exe().context("failed to resolve helper executable")?;
    let deadline = std::time::Instant::now()
        .checked_add(Duration::from_secs(10))
        .context("helper registration deadline overflow")?;
    let probe = RealProcessControl;
    loop {
        let marker = read_marker(marker_path)?;
        if marker.transaction_id != transaction_id {
            bail!("helper transaction ID does not match marker");
        }
        match marker.phase {
            Phase::PreparedParentOwned => {
                if !probe.is_alive(marker.parent_pid) {
                    bail!("parent exited before durable helper registration");
                }
            }
            Phase::HelperRegistered => {
                if marker.helper_pid != Some(std::process::id())
                    || marker.helper_start_token.as_deref() != Some(start_token)
                {
                    bail!("durable helper registration identity mismatch");
                }
                let acknowledgement = acknowledge_registered_helper(
                    marker_path,
                    std::process::id(),
                    start_token,
                    &actual_helper,
                )?;
                let mut process = RealProcessControl;
                let mut failures = NoFailure;
                let result = execute_registered_transaction_with_restart_mode(
                    marker_path,
                    start_token,
                    &mut process,
                    &mut failures,
                    restart_mode,
                );
                if let Err(error) = &result {
                    if let Some(install_dir) = acknowledgement.parent() {
                        // The acknowledgement path is derived only after marker, transaction,
                        // helper PID/token, executable identity, and helper hash validation.
                        // Diagnostic persistence is best-effort and must never mask rollback.
                        let _ = write_failure_record(install_dir, transaction_id, error);
                    }
                }
                return result;
            }
            _ => bail!("helper observed an invalid pre-ack transaction phase"),
        }
        if std::time::Instant::now() >= deadline {
            bail!("timed out waiting for durable helper registration");
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn sanitize_failure_message(error: &anyhow::Error) -> String {
    let source = format!("{error:#}");
    let mut sanitized = String::new();
    for ch in source.chars() {
        let ch = if ch.is_control() && ch != '\n' && ch != '\t' {
            ' '
        } else {
            ch
        };
        if sanitized.len() + ch.len_utf8() > 8 * 1024 {
            break;
        }
        sanitized.push(ch);
    }
    // Regression guard: updater errors compose paths deep in the transaction stack.
    // Normalize the final display text so no new error context can leak a verbatim prefix.
    crate::path_utils::normalize_text_for_display(&sanitized)
}

pub(super) fn write_failure_record(
    install_dir: &Path,
    transaction_id: &str,
    error: &anyhow::Error,
) -> Result<()> {
    validate_transaction_id(transaction_id)?;
    validate_directory(install_dir, "diagnostic installation directory")?;
    let record = UpdateFailureRecord {
        version: FAILURE_RECORD_VERSION,
        transaction_id: transaction_id.to_string(),
        message: sanitize_failure_message(error),
    };
    let bytes =
        serde_json::to_vec(&record).context("failed to serialize updater failure record")?;
    if bytes.len() as u64 > FAILURE_RECORD_MAX_BYTES {
        bail!("updater failure record exceeds size limit");
    }
    let target = install_dir.join(FAILURE_RECORD_FILE_NAME);
    reject_existing(&target, "updater failure record")?;
    let temp = install_dir.join(format!(".flistwalker-update-failure-{transaction_id}.tmp"));
    reject_existing(&temp, "updater failure record temporary file")?;
    create_new_synced(&temp, &bytes)?;
    if let Err(error) = promote_absent_no_overwrite(&temp, &target, install_dir) {
        let _ = fs::remove_file(&temp);
        return Err(error).context("failed to publish updater failure record");
    }
    Ok(())
}

pub(super) fn take_failure_record(current_exe: &Path) -> Result<Option<String>> {
    validate_regular_file(current_exe, "current executable")?;
    let canonical = current_exe
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", current_exe.display()))?;
    let install_dir = canonical
        .parent()
        .context("current executable has no parent")?
        .canonicalize()
        .context("failed to canonicalize executable directory")?;
    validate_directory(&install_dir, "executable directory")?;
    take_failure_record_from_install_dir(&install_dir)
}

pub(super) fn take_failure_record_from_install_dir(install_dir: &Path) -> Result<Option<String>> {
    let path = install_dir.join(FAILURE_RECORD_FILE_NAME);
    match fs::symlink_metadata(&path) {
        Ok(_) => validate_regular_file(&path, "updater failure record")?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to inspect {}", path.display()))
        }
    }
    let mut bytes = Vec::new();
    fs::File::open(&path)?
        .take(FAILURE_RECORD_MAX_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > FAILURE_RECORD_MAX_BYTES {
        bail!("updater failure record exceeds size limit");
    }
    let record: UpdateFailureRecord =
        serde_json::from_slice(&bytes).context("failed to parse updater failure record")?;
    if record.version != FAILURE_RECORD_VERSION {
        bail!("unsupported updater failure record version");
    }
    validate_transaction_id(&record.transaction_id)?;
    if record.message.is_empty() || record.message.len() > 8 * 1024 {
        bail!("invalid updater failure record message");
    }
    fs::remove_file(&path).with_context(|| {
        format!(
            "failed to consume updater failure record {}",
            path.display()
        )
    })?;
    sync_parent(install_dir)?;
    Ok(Some(record.message))
}

pub(super) fn recover_current_installation(current_exe: &Path) -> Result<Option<RecoveryOutcome>> {
    recover_current_installation_with_wait(
        current_exe,
        STARTUP_RECOVERY_WAIT,
        RecoveryWaitMode::Startup,
    )
}

pub(super) fn recover_current_installation_after_internal_restart(
    current_exe: &Path,
) -> Result<Option<RecoveryOutcome>> {
    // Regression guard: a helper-started GUI/headless child is the cleanup owner after spawn.
    // It must outlive a slow helper exit instead of reporting a false startup failure or
    // orphaning a committed transaction at the normal GUI recovery deadline.
    recover_current_installation_with_wait(
        current_exe,
        INTERNAL_RESTART_RECOVERY_WAIT,
        RecoveryWaitMode::InternalRestartHandoff,
    )
}

fn recover_current_installation_with_wait(
    current_exe: &Path,
    recovery_wait: Duration,
    mode: RecoveryWaitMode,
) -> Result<Option<RecoveryOutcome>> {
    validate_regular_file(current_exe, "current executable")?;
    let canonical = current_exe
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", current_exe.display()))?;
    let install_dir = canonical
        .parent()
        .context("current executable has no parent")?;
    let marker_path = install_dir.join(MARKER_FILE_NAME);
    let probe = RealProcessControl;
    if !marker_path.try_exists().unwrap_or(false) {
        if install_dir
            .join(LOCK_FILE_NAME)
            .try_exists()
            .unwrap_or(false)
        {
            return recover_orphan_preparation(install_dir, &probe).map(Some);
        }
        return Ok(None);
    }
    match mode {
        RecoveryWaitMode::Startup => recover_transaction_until_deadline(
            &marker_path,
            &probe,
            recovery_wait,
            Instant::now,
            std::thread::sleep,
        ),
        RecoveryWaitMode::InternalRestartHandoff => {
            recover_internal_restart_handoff_until_deadline(
                &marker_path,
                &probe,
                recovery_wait,
                Instant::now,
                std::thread::sleep,
            )
        }
    }
    .map(Some)
}

fn recover_transaction_until_deadline<N, S>(
    marker_path: &Path,
    process_probe: &impl ProcessProbe,
    recovery_wait: Duration,
    mut now: N,
    mut pause: S,
) -> Result<RecoveryOutcome>
where
    N: FnMut() -> Instant,
    S: FnMut(Duration),
{
    let deadline = now()
        .checked_add(recovery_wait)
        .context("updater recovery deadline overflow")?;
    recover_transaction_until_instant(marker_path, process_probe, deadline, &mut now, &mut pause)
}

fn recover_transaction_until_instant<N, S>(
    marker_path: &Path,
    process_probe: &impl ProcessProbe,
    deadline: Instant,
    now: &mut N,
    pause: &mut S,
) -> Result<RecoveryOutcome>
where
    N: FnMut() -> Instant,
    S: FnMut(Duration),
{
    loop {
        let outcome = recover_transaction(marker_path, process_probe)?;
        if outcome == RecoveryOutcome::Ambiguous {
            bail!(
                "ambiguous updater transaction preserved for operator recovery: marker={}, lock={}",
                marker_path.display(),
                marker_path
                    .parent()
                    .context("transaction marker has no parent")?
                    .join(LOCK_FILE_NAME)
                    .display()
            );
        }
        if outcome != RecoveryOutcome::Deferred {
            return Ok(outcome);
        }
        if now() >= deadline {
            return Ok(RecoveryOutcome::Deferred);
        }
        pause(RECOVERY_RETRY_INTERVAL);
    }
}

struct ExitedHelperProbe<'a, P> {
    inner: &'a P,
    helper_pid: u32,
}

impl<P: ProcessProbe> ProcessProbe for ExitedHelperProbe<'_, P> {
    fn is_alive(&self, pid: u32) -> bool {
        pid != self.helper_pid && self.inner.is_alive(pid)
    }

    fn executable_matches(&self, pid: u32, expected: &Path) -> bool {
        pid != self.helper_pid && self.inner.executable_matches(pid, expected)
    }

    fn is_current_process(&self, pid: u32) -> bool {
        self.inner.is_current_process(pid)
    }
}

fn recover_internal_restart_handoff_until_deadline<N, S>(
    marker_path: &Path,
    process_probe: &impl ProcessProbe,
    recovery_wait: Duration,
    mut now: N,
    mut pause: S,
) -> Result<RecoveryOutcome>
where
    N: FnMut() -> Instant,
    S: FnMut(Duration),
{
    let deadline = now()
        .checked_add(recovery_wait)
        .context("updater recovery deadline overflow")?;
    let marker = read_marker(marker_path)?;
    let _install_dir = validated_marker_parent(marker_path, &marker)?;
    let terminal_handoff = matches!(marker.phase, Phase::BinaryCommitted | Phase::RolledBack);
    let Some(helper_pid) = marker.helper_pid.filter(|_| terminal_handoff) else {
        return recover_transaction_until_instant(
            marker_path,
            process_probe,
            deadline,
            &mut now,
            &mut pause,
        );
    };
    if marker.parent_pid == helper_pid {
        return recover_transaction_until_instant(
            marker_path,
            process_probe,
            deadline,
            &mut now,
            &mut pause,
        );
    }
    if process_probe.is_current_process(helper_pid) {
        return recover_transaction_until_instant(
            marker_path,
            process_probe,
            deadline,
            &mut now,
            &mut pause,
        );
    }

    // Regression guard: QueryFullProcessImageNameW can transiently fail for the
    // live helper immediately after restart spawn. This internal, terminal-phase
    // handoff waits without mutating artifacts or weakening normal startup's
    // fail-closed identity check, then re-runs full hash/classification recovery.
    while process_probe.is_alive(helper_pid) {
        if now() >= deadline {
            return Ok(RecoveryOutcome::Deferred);
        }
        pause(RECOVERY_RETRY_INTERVAL);
    }
    let settled_marker = read_marker(marker_path)?;
    let _install_dir = validated_marker_parent(marker_path, &settled_marker)?;
    if settled_marker.transaction_id != marker.transaction_id
        || settled_marker.helper_pid != Some(helper_pid)
        || !matches!(
            settled_marker.phase,
            Phase::BinaryCommitted | Phase::RolledBack
        )
    {
        bail!("updater transaction changed during internal restart handoff");
    }
    let exited_helper = ExitedHelperProbe {
        inner: process_probe,
        helper_pid,
    };
    recover_transaction_until_instant(marker_path, &exited_helper, deadline, &mut now, &mut pause)
}

fn recover_orphan_preparation(
    install_dir: &Path,
    process_probe: &impl ProcessProbe,
) -> Result<RecoveryOutcome> {
    validate_directory(install_dir, "transaction directory")?;
    let (transaction_id, parent_pid) = read_lock_record(install_dir)?;
    if process_probe.is_alive(parent_pid) && !process_probe.is_current_process(parent_pid) {
        return Ok(RecoveryOutcome::Deferred);
    }
    let derived_artifacts = TargetRole::ORDER
        .into_iter()
        .flat_map(|role| {
            [
                new_path(install_dir, &transaction_id, role),
                backup_path(install_dir, &transaction_id, role),
                failed_path(install_dir, &transaction_id, role),
            ]
        })
        .chain([
            ack_path(install_dir, &transaction_id),
            helper_path(install_dir, &transaction_id),
            marker_temp_path(install_dir, &transaction_id),
            marker_temp_path(install_dir, &transaction_id).with_extension("previous"),
        ])
        .collect::<Vec<_>>();
    if derived_artifacts
        .iter()
        .any(|path| fs::symlink_metadata(path).is_ok())
    {
        bail!(
            "orphan updater preparation artifacts preserved for operator recovery: lock={}, directory={}",
            install_dir.join(LOCK_FILE_NAME).display(),
            install_dir.display()
        );
    }
    remove_file_if_present(&install_dir.join(LOCK_FILE_NAME))?;
    sync_parent(install_dir)?;
    Ok(RecoveryOutcome::RolledBack)
}

struct NoFailure;
impl FailureInjector for NoFailure {}

#[cfg(not(target_os = "macos"))]
fn hex_bytes(bytes: &[u8]) -> String {
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut value, "{byte:02x}");
    }
    value
}

use crate::update_security::CHECKSUM_SIGNATURE_NAME;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};

mod apply;
mod manifest;
mod release;
mod staging;
mod transaction;

const SELF_UPDATE_DISABLE_FLAG_NAME: &str = "FLISTWALKER_DISABLE_SELF_UPDATE";
const FORCE_UPDATE_CHECK_FAILURE_FLAG_NAME: &str = "FLISTWALKER_FORCE_UPDATE_CHECK_FAILURE";
const INTERNAL_UPDATE_RESTART_FLAG: &str = "--flistwalker-internal-update-restart";

const INSTALL_CANCEL_REQUESTED: u8 = 1;
const INSTALL_COMMIT_HANDOFF: u8 = 2;
const INSTALL_TERMINAL: u8 = 4;
const BINARY_VARIANT_UNIVERSAL: u8 = 0;
const BINARY_VARIANT_CLI: u8 = 1;

static RUNNING_BINARY_VARIANT: AtomicU8 = AtomicU8::new(BINARY_VARIANT_UNIVERSAL);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BinaryVariant {
    #[default]
    Universal,
    Cli,
}

pub fn set_running_binary_variant(variant: BinaryVariant) {
    let value = match variant {
        BinaryVariant::Universal => BINARY_VARIANT_UNIVERSAL,
        BinaryVariant::Cli => BINARY_VARIANT_CLI,
    };
    RUNNING_BINARY_VARIANT.store(value, Ordering::Release);
}

pub fn running_binary_variant() -> BinaryVariant {
    match RUNNING_BINARY_VARIANT.load(Ordering::Acquire) {
        BINARY_VARIANT_CLI => BinaryVariant::Cli,
        _ => BinaryVariant::Universal,
    }
}

pub fn running_binary_command_name() -> &'static str {
    match running_binary_variant() {
        BinaryVariant::Universal => "flistwalker",
        BinaryVariant::Cli => "fw",
    }
}

/// Shared by the GUI and update worker so close/cancel can interrupt staging
/// without introducing an mpsc request that sits behind the active download.
pub(crate) struct UpdateInstallControl {
    state: AtomicU8,
}

impl Default for UpdateInstallControl {
    fn default() -> Self {
        Self {
            state: AtomicU8::new(0),
        }
    }
}

impl UpdateInstallControl {
    pub(crate) fn request_cancel(&self) -> bool {
        self.state
            .compare_exchange(
                0,
                INSTALL_CANCEL_REQUESTED,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    pub(crate) fn cancel_requested(&self) -> bool {
        self.state.load(Ordering::Acquire) == INSTALL_CANCEL_REQUESTED
    }

    fn begin_commit_handoff(&self) -> bool {
        self.state
            .compare_exchange(
                0,
                INSTALL_COMMIT_HANDOFF,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    fn mark_terminal(&self) {
        self.state.store(INSTALL_TERMINAL, Ordering::Release);
    }

    #[cfg(test)]
    pub(crate) fn commit_handoff_started(&self) -> bool {
        self.state.load(Ordering::Acquire) == INSTALL_COMMIT_HANDOFF
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateRestartMode {
    Gui,
    Headless,
}

impl UpdateRestartMode {
    #[cfg(not(target_os = "macos"))]
    fn helper_argument(self) -> &'static str {
        match self {
            Self::Gui => "gui",
            Self::Headless => "headless",
        }
    }

    fn from_helper_argument(value: &str) -> Result<Self> {
        match value {
            "gui" => Ok(Self::Gui),
            "headless" => Ok(Self::Headless),
            _ => bail!("internal updater helper restart mode is invalid"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateSupport {
    Auto,
    ManualOnly { message: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutoUpdateAssets {
    pub asset_name: String,
    pub asset_url: String,
    pub readme_asset_name: String,
    pub readme_asset_url: String,
    pub license_asset_name: String,
    pub license_asset_url: String,
    pub notices_asset_name: String,
    pub notices_asset_url: String,
    pub checksum_url: String,
    pub checksum_signature_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateCandidate {
    pub current_version: String,
    pub target_version: String,
    pub release_url: String,
    pub support: UpdateSupport,
    pub auto_assets: Option<AutoUpdateAssets>,
}

impl UpdateCandidate {
    fn auto_assets(&self) -> Result<&AutoUpdateAssets> {
        match (&self.support, &self.auto_assets) {
            (UpdateSupport::Auto, Some(assets)) => Ok(assets),
            (UpdateSupport::ManualOnly { message }, None) => bail!("{message}"),
            _ => bail!("invalid update candidate support/payload combination"),
        }
    }
}

pub fn current_version_string() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

pub fn self_update_disabled() -> bool {
    self_update_disabled_for_exe_path(std::env::current_exe().ok().as_deref())
}

pub fn forced_update_check_failure_message() -> Option<String> {
    let value = std::env::var(FORCE_UPDATE_CHECK_FAILURE_FLAG_NAME).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let message = if matches!(
        trimmed.to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    ) {
        "forced startup update check failure for debugging".to_string()
    } else {
        trimmed.to_string()
    };
    Some(format!(
        "{} ({FORCE_UPDATE_CHECK_FAILURE_FLAG_NAME})",
        message
    ))
}

fn self_update_disabled_for_exe_path(current_exe: Option<&Path>) -> bool {
    env_flag(SELF_UPDATE_DISABLE_FLAG_NAME) || self_update_disabled_by_sentinel_file(current_exe)
}

fn self_update_disabled_by_sentinel_file(current_exe: Option<&Path>) -> bool {
    current_exe
        .and_then(Path::parent)
        .map(|dir| dir.join(SELF_UPDATE_DISABLE_FLAG_NAME))
        .and_then(|path| path.try_exists().ok())
        .unwrap_or(false)
}

pub fn check_for_update() -> Result<Option<UpdateCandidate>> {
    check_for_update_with_control(&UpdateInstallControl::default())
}

pub(crate) fn check_for_update_with_control(
    control: &UpdateInstallControl,
) -> Result<Option<UpdateCandidate>> {
    if self_update_disabled() {
        return Ok(None);
    }
    if let Some(message) = forced_update_check_failure_message() {
        bail!("{message}");
    }
    let current_version = release::parse_version(env!("CARGO_PKG_VERSION"))?;
    if control.cancel_requested() {
        bail!("update canceled");
    }
    let latest_release = release::fetch_latest_release(&|| control.cancel_requested())?;
    if control.cancel_requested() {
        bail!("update canceled");
    }
    release::resolve_update_candidate_from_release(&current_version, &latest_release)
}

pub fn prepare_and_start_update(
    candidate: &UpdateCandidate,
    current_exe: &Path,
    restart_mode: UpdateRestartMode,
) -> Result<()> {
    prepare_and_start_update_with_control(
        candidate,
        current_exe,
        restart_mode,
        &UpdateInstallControl::default(),
    )
}

pub(crate) fn prepare_and_start_update_with_control(
    candidate: &UpdateCandidate,
    current_exe: &Path,
    restart_mode: UpdateRestartMode,
    control: &UpdateInstallControl,
) -> Result<()> {
    if self_update_disabled() {
        bail!(
            "self-update is disabled by {} environment variable or sentinel file",
            SELF_UPDATE_DISABLE_FLAG_NAME
        );
    }
    match &candidate.support {
        UpdateSupport::Auto => {
            candidate.auto_assets()?;
        }
        UpdateSupport::ManualOnly { message } => bail!("{message}"),
    }

    let staged = staging::stage_update_assets_with_control(candidate, control)?;
    let mut verified = verify_staged_update(candidate, staged, control)?;
    if !control.begin_commit_handoff() {
        bail!("update canceled");
    }
    let result = apply::spawn_update_helper(current_exe, &mut verified, restart_mode);
    control.mark_terminal();
    result
}

pub fn should_skip_update_prompt(target_version: &str, skipped_version: Option<&str>) -> bool {
    release::should_skip_update_prompt(target_version, skipped_version)
}

pub(super) fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

#[derive(Debug)]
pub(super) struct StagedUpdatePaths {
    pub(super) staged_path: PathBuf,
    pub(super) staged_readme_path: PathBuf,
    pub(super) staged_license_path: PathBuf,
    pub(super) staged_notices_path: PathBuf,
    pub(super) checksum_path: PathBuf,
    pub(super) signature_path: PathBuf,
    pub(super) temp_dir: PathBuf,
    cleanup_armed: bool,
}

impl StagedUpdatePaths {
    fn new(temp_dir: PathBuf, candidate: &UpdateCandidate) -> Result<Self> {
        let assets = candidate.auto_assets()?;
        Ok(Self {
            staged_path: temp_dir.join(&assets.asset_name),
            staged_readme_path: temp_dir.join(&assets.readme_asset_name),
            staged_license_path: temp_dir.join(&assets.license_asset_name),
            staged_notices_path: temp_dir.join(&assets.notices_asset_name),
            checksum_path: temp_dir.join("SHA256SUMS"),
            signature_path: temp_dir.join(CHECKSUM_SIGNATURE_NAME),
            temp_dir,
            cleanup_armed: true,
        })
    }

    fn cleanup_now(&mut self) -> Result<()> {
        if !self.cleanup_armed {
            return Ok(());
        }
        std::fs::remove_dir_all(&self.temp_dir)
            .with_context(|| format!("failed to remove staging {}", self.temp_dir.display()))?;
        self.cleanup_armed = false;
        Ok(())
    }
}

impl Drop for StagedUpdatePaths {
    fn drop(&mut self) {
        if self.cleanup_armed {
            let _ = std::fs::remove_dir_all(&self.temp_dir);
        }
    }
}

pub(super) struct VerifiedUpdateBundle {
    #[cfg(not(target_os = "macos"))]
    pub(super) staged_path: PathBuf,
    #[cfg(not(target_os = "macos"))]
    pub(super) staged_readme_path: PathBuf,
    #[cfg(not(target_os = "macos"))]
    pub(super) staged_license_path: PathBuf,
    #[cfg(not(target_os = "macos"))]
    pub(super) staged_notices_path: PathBuf,
    pub(super) temp_dir: PathBuf,
    cleanup_armed: bool,
}

impl VerifiedUpdateBundle {
    fn new(mut staged: StagedUpdatePaths) -> Self {
        #[cfg(not(target_os = "macos"))]
        {
            let bundle = Self {
                staged_path: staged.staged_path.clone(),
                staged_readme_path: staged.staged_readme_path.clone(),
                staged_license_path: staged.staged_license_path.clone(),
                staged_notices_path: staged.staged_notices_path.clone(),
                temp_dir: staged.temp_dir.clone(),
                cleanup_armed: true,
            };
            staged.cleanup_armed = false;
            bundle
        }
        #[cfg(target_os = "macos")]
        {
            let bundle = Self {
                temp_dir: staged.temp_dir.clone(),
                cleanup_armed: true,
            };
            staged.cleanup_armed = false;
            bundle
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn cleanup_staging(&mut self) -> Result<()> {
        if !self.cleanup_armed {
            bail!("verified staging ownership was already transferred");
        }
        std::fs::remove_dir_all(&self.temp_dir)
            .with_context(|| format!("failed to remove staging {}", self.temp_dir.display()))?;
        self.cleanup_armed = false;
        Ok(())
    }
}

impl Drop for VerifiedUpdateBundle {
    fn drop(&mut self) {
        if self.cleanup_armed {
            let _ = std::fs::remove_dir_all(&self.temp_dir);
        }
    }
}

fn verify_staged_update(
    candidate: &UpdateCandidate,
    mut staged: StagedUpdatePaths,
    control: &UpdateInstallControl,
) -> Result<VerifiedUpdateBundle> {
    let assets = candidate.auto_assets()?;
    let verification = (|| {
        if control.cancel_requested() {
            bail!("update canceled");
        }
        manifest::verify_checksum_manifest_signature(
            &staged.checksum_path,
            &staged.signature_path,
        )?;
        manifest::verify_download_with_cancel(
            &staged.staged_path,
            &staged.checksum_path,
            &assets.asset_name,
            &|| control.cancel_requested(),
        )?;
        manifest::verify_download_with_cancel(
            &staged.staged_readme_path,
            &staged.checksum_path,
            &assets.readme_asset_name,
            &|| control.cancel_requested(),
        )?;
        manifest::verify_download_with_cancel(
            &staged.staged_license_path,
            &staged.checksum_path,
            &assets.license_asset_name,
            &|| control.cancel_requested(),
        )?;
        manifest::verify_download_with_cancel(
            &staged.staged_notices_path,
            &staged.checksum_path,
            &assets.notices_asset_name,
            &|| control.cancel_requested(),
        )?;
        if control.cancel_requested() {
            bail!("update canceled");
        }
        staging::make_staged_binary_executable(&staged.staged_path)
    })();
    if let Err(err) = verification {
        if let Err(cleanup) = staged.cleanup_now() {
            return Err(err).with_context(|| {
                format!(
                    "staging cleanup failed and retained {}: {cleanup}",
                    staged.temp_dir.display()
                )
            });
        }
        return Err(err);
    }
    Ok(VerifiedUpdateBundle::new(staged))
}

pub fn run_internal_updater_command_if_requested() -> Result<bool> {
    let mut arguments = std::env::args_os();
    let _program = arguments.next();
    let Some(flag) = arguments.next() else {
        return Ok(false);
    };
    if flag == INTERNAL_UPDATE_RESTART_FLAG {
        if arguments.next().is_some() {
            bail!("internal updater restart received unexpected arguments");
        }
        let outcome = recover_interrupted_update_after_headless_restart()?;
        if outcome == Some(transaction::RecoveryOutcome::Deferred) {
            bail!("internal updater restart did not reach terminal recovery state");
        }
        return Ok(true);
    }
    if flag != apply::INTERNAL_HELPER_FLAG {
        return Ok(false);
    }
    let marker = arguments
        .next()
        .context("internal updater helper marker argument is missing")?;
    let transaction_id = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .context("internal updater helper transaction ID is missing or invalid")?;
    let start_token = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .context("internal updater helper start token is missing or invalid")?;
    let restart_mode = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .context("internal updater helper restart mode is missing or invalid")?;
    let restart_mode = UpdateRestartMode::from_helper_argument(&restart_mode)?;
    if arguments.next().is_some() {
        bail!("internal updater helper received unexpected arguments");
    }
    transaction::run_internal_helper(
        Path::new(&marker),
        &transaction_id,
        &start_token,
        restart_mode,
    )?;
    Ok(true)
}

pub fn recover_interrupted_update_on_startup() -> Result<Option<String>> {
    let current_exe = std::env::current_exe().context("failed to resolve current executable")?;
    transaction::recover_current_installation(&current_exe)
        .map(|outcome| outcome.map(|value| format!("{value:?}")))
}

fn recover_interrupted_update_after_headless_restart(
) -> Result<Option<transaction::RecoveryOutcome>> {
    let current_exe = std::env::current_exe().context("failed to resolve current executable")?;
    transaction::recover_current_installation_after_headless_restart(&current_exe)
}

pub fn take_previous_update_failure_on_startup() -> Result<Option<String>> {
    let current_exe = std::env::current_exe().context("failed to resolve current executable")?;
    transaction::take_failure_record(&current_exe)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn tc188_cancel_wins_before_commit_handoff() {
        let control = UpdateInstallControl::default();

        assert!(control.request_cancel());
        assert!(control.cancel_requested());
        assert!(!control.begin_commit_handoff());
    }

    #[test]
    fn tc188_commit_handoff_ignores_late_cancel() {
        let control = UpdateInstallControl::default();

        assert!(control.begin_commit_handoff());
        assert!(control.commit_handoff_started());
        assert!(!control.request_cancel());
        assert!(!control.cancel_requested());
    }

    #[test]
    fn tc189_failure_record_is_bounded_sanitized_and_consumed_once() {
        let base = std::env::temp_dir().join(format!(
            "flistwalker-update-failure-record-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir(&base).expect("base");
        let transaction_id = "00112233445566778899aabbccddeeff";

        transaction::write_failure_record(
            &base,
            transaction_id,
            &anyhow::anyhow!("restart failed\u{7}: detail"),
        )
        .expect("write failure record");

        let message = transaction::take_failure_record_from_install_dir(&base)
            .expect("take failure record")
            .expect("failure message");
        assert_eq!(message, "restart failed : detail");
        assert!(transaction::take_failure_record_from_install_dir(&base)
            .expect("take second time")
            .is_none());
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn tc189_failure_record_rejects_oversize_and_wrong_type_without_consuming() {
        let base = std::env::temp_dir().join(format!(
            "flistwalker-update-failure-record-invalid-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir(&base).expect("base");
        let path = base.join(".flistwalker-update-failure.json");

        fs::write(&path, vec![b'x'; 16 * 1024 + 1]).expect("oversize record");
        let error = transaction::take_failure_record_from_install_dir(&base)
            .expect_err("oversize record must fail closed");
        assert!(error.to_string().contains("size limit"));
        assert!(path.is_file(), "invalid record must not be consumed");

        fs::remove_file(&path).expect("remove oversize fixture");
        fs::create_dir(&path).expect("wrong-type record");
        assert!(transaction::take_failure_record_from_install_dir(&base).is_err());
        assert!(path.is_dir(), "wrong-type record must not be consumed");

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn tc189_failure_record_rejects_symbolic_links() {
        let base = std::env::temp_dir().join(format!(
            "flistwalker-update-failure-record-link-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir(&base).expect("base");
        let target = base.join("outside-record.json");
        fs::write(&target, b"do not consume").expect("target");
        let link = base.join(".flistwalker-update-failure.json");
        #[cfg(unix)]
        let link_result = std::os::unix::fs::symlink(&target, &link);
        #[cfg(windows)]
        let link_result = std::os::windows::fs::symlink_file(&target, &link);
        if link_result.is_err() {
            // Windows can disable unprivileged symlink creation. The wrong-type case above still
            // exercises the same fail-closed reader path on such hosts; Unix CI covers the link.
            let _ = fs::remove_dir_all(base);
            return;
        }

        assert!(transaction::take_failure_record_from_install_dir(&base).is_err());
        assert_eq!(
            fs::read(&target).expect("target retained"),
            b"do not consume"
        );
        assert!(fs::symlink_metadata(&link)
            .expect("link retained")
            .file_type()
            .is_symlink());
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn tc157_explicit_cleanup_failure_reports_and_retains_the_owned_path() {
        let base = std::env::temp_dir().join(format!(
            "flistwalker-cleanup-report-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir(&base).expect("base");
        let retained = base.join("retained-as-file");
        fs::write(&retained, b"retain").expect("retained file");
        let mut staged = StagedUpdatePaths {
            staged_path: retained.join("binary"),
            staged_readme_path: retained.join("readme"),
            staged_license_path: retained.join("license"),
            staged_notices_path: retained.join("notices"),
            checksum_path: retained.join("SHA256SUMS"),
            signature_path: retained.join(CHECKSUM_SIGNATURE_NAME),
            temp_dir: retained.clone(),
            cleanup_armed: true,
        };

        let err = staged
            .cleanup_now()
            .expect_err("non-directory owned path must report cleanup failure");

        assert!(err.to_string().contains(&retained.display().to_string()));
        assert!(retained.exists());
        staged.cleanup_armed = false;
        fs::remove_file(retained).expect("cleanup retained fixture");
        fs::remove_dir(base).expect("cleanup base fixture");
    }

    #[test]
    fn self_update_disabled_flag_is_honored() {
        let _env_lock = crate::env_var_test_lock()
            .lock()
            .expect("env var test lock");
        assert!(!self_update_disabled_for_exe_path(None));
        unsafe {
            std::env::set_var(SELF_UPDATE_DISABLE_FLAG_NAME, "1");
        }
        assert!(self_update_disabled_for_exe_path(None));
        unsafe {
            std::env::remove_var(SELF_UPDATE_DISABLE_FLAG_NAME);
        }
    }

    #[test]
    fn check_for_update_short_circuits_when_self_update_is_disabled() {
        let _env_lock = crate::env_var_test_lock()
            .lock()
            .expect("env var test lock");
        unsafe {
            std::env::set_var(SELF_UPDATE_DISABLE_FLAG_NAME, "1");
        }
        let result = check_for_update().expect("disabled updates should skip network access");
        assert!(result.is_none());
        unsafe {
            std::env::remove_var(SELF_UPDATE_DISABLE_FLAG_NAME);
        }
    }

    #[test]
    fn forced_update_check_failure_is_honored_before_network_access() {
        let _env_lock = crate::env_var_test_lock()
            .lock()
            .expect("env var test lock");
        unsafe {
            std::env::set_var(FORCE_UPDATE_CHECK_FAILURE_FLAG_NAME, "1");
        }

        let err = check_for_update().expect_err("forced failure should bypass network");
        assert!(
            err.to_string()
                .contains("forced startup update check failure for debugging"),
            "unexpected error: {err}"
        );

        unsafe {
            std::env::remove_var(FORCE_UPDATE_CHECK_FAILURE_FLAG_NAME);
        }
    }

    #[test]
    fn self_update_disabled_sentinel_file_is_honored() {
        let _env_lock = crate::env_var_test_lock()
            .lock()
            .expect("env var test lock");
        let root = std::env::temp_dir().join(format!(
            "flistwalker-update-disable-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create root");
        let exe = root.join("flistwalker");
        fs::write(&exe, "bin").expect("write exe");
        fs::write(root.join(SELF_UPDATE_DISABLE_FLAG_NAME), "").expect("write sentinel");

        assert!(self_update_disabled_for_exe_path(Some(&exe)));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn self_update_disabled_sentinel_file_is_false_when_missing() {
        let _env_lock = crate::env_var_test_lock()
            .lock()
            .expect("env var test lock");
        let root = std::env::temp_dir().join(format!(
            "flistwalker-update-disable-missing-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create root");
        let exe = root.join("flistwalker");
        fs::write(&exe, "bin").expect("write exe");

        assert!(!self_update_disabled_for_exe_path(Some(&exe)));

        let _ = fs::remove_dir_all(&root);
    }
}

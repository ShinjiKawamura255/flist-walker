use crate::updater::{UpdateRestartMode, VerifiedUpdateBundle};
use anyhow::{bail, Result};
use std::path::Path;

#[cfg(not(target_os = "macos"))]
use crate::updater::transaction::{self, TransactionSources};
#[cfg(not(target_os = "macos"))]
use anyhow::Context;
#[cfg(any(not(target_os = "macos"), test))]
use std::ffi::{OsStr, OsString};
#[cfg(not(target_os = "macos"))]
use std::process::Child;
#[cfg(any(not(target_os = "macos"), test))]
use std::process::Command;
#[cfg(not(target_os = "macos"))]
use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
pub(crate) const INTERNAL_HELPER_FLAG: &str = "--flistwalker-internal-update-helper";

pub(super) fn spawn_update_helper(
    current_exe: &Path,
    bundle: &mut VerifiedUpdateBundle,
    restart_mode: UpdateRestartMode,
) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let _ = (current_exe, bundle, restart_mode);
        bail!("macOS auto-update is unsupported");
    }
    #[cfg(not(target_os = "macos"))]
    {
        let sources = TransactionSources {
            binary: &bundle.staged_path,
            readme: &bundle.staged_readme_path,
            license: &bundle.staged_license_path,
            notices: &bundle.staged_notices_path,
        };
        let mut prepared = transaction::prepare_transaction(current_exe, sources)?;
        bundle.cleanup_staging()?;
        let start_token = transaction::new_start_token();
        let arguments = helper_arguments(
            prepared.marker_path().as_os_str(),
            OsStr::new(prepared.transaction_id()),
            OsStr::new(&start_token),
            OsStr::new(restart_mode.helper_argument()),
        );
        let mut child = spawn_update_helper_process(prepared.helper_path(), &arguments)?;
        if let Err(err) = prepared.register_helper(child.id(), &start_token) {
            stop_unregistered_helper(&mut child);
            return Err(err).context("failed to durably register updater helper");
        }
        if let Err(err) = wait_for_acknowledgement(&prepared, &start_token, &mut child) {
            stop_unregistered_helper(&mut child);
            return Err(err);
        }
        prepared.disarm();
        Ok(())
    }
}

#[cfg(any(not(target_os = "macos"), test))]
fn helper_command(helper_path: &Path, arguments: &[OsString]) -> Command {
    // Regression guard: Windows applies MAX_PATH-style limits to CreateProcessW's current
    // directory even when the executable uses a verbatim path. Do not restore install_dir as
    // current_dir or remove the non-verbatim retry without updating the TC-179 regression tests.
    let mut command = Command::new(helper_path);
    command.args(arguments);
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

#[cfg(not(target_os = "macos"))]
fn spawn_update_helper_process(helper_path: &Path, arguments: &[OsString]) -> Result<Child> {
    attempt_helper_launch(helper_path, |launch_path| {
        helper_command(launch_path, arguments).spawn()
    })
}

#[cfg(any(not(target_os = "macos"), test))]
fn attempt_helper_launch<T>(
    helper_path: &Path,
    mut launch: impl FnMut(&Path) -> std::io::Result<T>,
) -> Result<T> {
    match launch(helper_path) {
        Ok(value) => Ok(value),
        Err(primary_error) => {
            #[cfg(target_os = "windows")]
            if let Some(fallback_path) = windows_non_verbatim_path(helper_path) {
                return launch(&fallback_path).map_err(|fallback_error| {
                    anyhow::anyhow!(
                        "failed to spawn updater helper {}: primary attempt failed: {}; normalized-path retry failed: {}",
                        crate::path_utils::normalize_path_for_display(helper_path),
                        primary_error,
                        fallback_error
                    )
                });
            }

            Err(anyhow::anyhow!(
                "failed to spawn updater helper {}: {}",
                crate::path_utils::normalize_path_for_display(helper_path),
                primary_error
            ))
        }
    }
}

#[cfg(target_os = "windows")]
fn windows_non_verbatim_path(path: &Path) -> Option<std::path::PathBuf> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    const VERBATIM_PREFIX: &[u16] = &[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16];
    const VERBATIM_UNC_PREFIX: &[u16] = &[
        b'\\' as u16,
        b'\\' as u16,
        b'?' as u16,
        b'\\' as u16,
        b'U' as u16,
        b'N' as u16,
        b'C' as u16,
        b'\\' as u16,
    ];

    let wide = path.as_os_str().encode_wide().collect::<Vec<_>>();
    let normalized = if let Some(rest) = wide.strip_prefix(VERBATIM_UNC_PREFIX) {
        [b'\\' as u16, b'\\' as u16]
            .into_iter()
            .chain(rest.iter().copied())
            .collect::<Vec<_>>()
    } else {
        wide.strip_prefix(VERBATIM_PREFIX)?.to_vec()
    };
    Some(std::path::PathBuf::from(std::ffi::OsString::from_wide(
        &normalized,
    )))
}

#[cfg(any(not(target_os = "macos"), test))]
fn helper_arguments(
    marker: &OsStr,
    transaction_id: &OsStr,
    start_token: &OsStr,
    restart_mode: &OsStr,
) -> [OsString; 5] {
    [
        INTERNAL_HELPER_FLAG.into(),
        marker.to_os_string(),
        transaction_id.to_os_string(),
        start_token.to_os_string(),
        restart_mode.to_os_string(),
    ]
}

#[cfg(not(target_os = "macos"))]
fn wait_for_acknowledgement(
    prepared: &transaction::PreparedTransaction,
    start_token: &str,
    child: &mut Child,
) -> Result<()> {
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(10))
        .context("helper acknowledgement deadline overflow")?;
    loop {
        if prepared.acknowledgement_matches(start_token) {
            return Ok(());
        }
        if let Some(status) = child
            .try_wait()
            .context("failed to query updater helper status")?
        {
            bail!("updater helper exited before acknowledgement: {status}");
        }
        if Instant::now() >= deadline {
            bail!("timed out waiting for updater helper acknowledgement");
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[cfg(not(target_os = "macos"))]
fn stop_unregistered_helper(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "windows")]
    use std::path::PathBuf;

    #[test]
    fn tc159_internal_helper_arguments_are_exact_and_positional() {
        let args = helper_arguments(
            OsStr::new("marker"),
            OsStr::new("00112233445566778899aabbccddeeff"),
            OsStr::new("start-token-0123456789"),
            OsStr::new("headless"),
        );

        assert_eq!(
            args,
            [
                OsString::from(INTERNAL_HELPER_FLAG),
                OsString::from("marker"),
                OsString::from("00112233445566778899aabbccddeeff"),
                OsString::from("start-token-0123456789"),
                OsString::from("headless")
            ]
        );
    }

    #[test]
    fn tc179_regression_helper_launch_does_not_force_install_directory_as_current_dir() {
        let args = [OsString::from(INTERNAL_HELPER_FLAG)];
        let command = helper_command(Path::new("helper-program"), &args);

        assert_eq!(command.get_program(), OsStr::new("helper-program"));
        assert_eq!(command.get_current_dir(), None);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn tc179_regression_windows_helper_launch_retries_without_verbatim_prefix() {
        let helper = Path::new(r"\\?\C:\very-long\flistwalker-update-helper.exe");
        let mut attempted = Vec::new();

        let result = attempt_helper_launch(helper, |path| {
            attempted.push(path.to_path_buf());
            if attempted.len() == 1 {
                Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "verbatim launch rejected",
                ))
            } else {
                Ok(())
            }
        });

        result.expect("normalized-path retry should succeed");
        assert_eq!(
            attempted,
            vec![
                helper.to_path_buf(),
                PathBuf::from(r"C:\very-long\flistwalker-update-helper.exe")
            ]
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn tc179_regression_windows_helper_spawn_error_hides_verbatim_prefix() {
        let helper = Path::new(r"\\?\UNC\server\share\flistwalker-update-helper.exe");

        let error = attempt_helper_launch(helper, |_| -> std::io::Result<()> {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "launch rejected",
            ))
        })
        .expect_err("both launch attempts should fail");
        let message = error.to_string();

        assert!(message.contains(r"\\server\share\flistwalker-update-helper.exe"));
        assert!(message.contains("normalized-path retry failed"));
        assert!(!message.contains(r"\\?\"), "unexpected error: {message}");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn tc179_regression_windows_normal_helper_path_is_not_retried() {
        let helper = Path::new(r"C:\tools\flistwalker-update-helper.exe");
        let mut attempts = 0;

        let error = attempt_helper_launch(helper, |_| -> std::io::Result<()> {
            attempts += 1;
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "launch rejected",
            ))
        })
        .expect_err("normal path launch should fail once");

        assert_eq!(attempts, 1);
        assert!(!error.to_string().contains(r"\\?\"));
    }
}

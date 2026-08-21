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
use crate::path_utils::windows_non_verbatim_path;
#[cfg(target_os = "windows")]
const WINDOWS_HELPER_LAUNCH_ROUNDS: usize = 3;
#[cfg(target_os = "windows")]
const WINDOWS_HELPER_RETRY_DELAY: Duration = Duration::from_millis(100);
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
    // current_dir or remove the bounded path retries without updating the TC-179/TC-187 tests.
    #[cfg(target_os = "windows")]
    let mut command = transaction::windows_hidden_child_command(helper_path);
    #[cfg(not(target_os = "windows"))]
    let mut command = Command::new(helper_path);
    command.args(arguments);
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
    launch: impl FnMut(&Path) -> std::io::Result<T>,
) -> Result<T> {
    #[cfg(target_os = "windows")]
    {
        attempt_windows_helper_launch(helper_path, launch, std::thread::sleep)
    }

    #[cfg(not(target_os = "windows"))]
    let mut launch = launch;
    #[cfg(not(target_os = "windows"))]
    match launch(helper_path) {
        Ok(value) => Ok(value),
        Err(error) => Err(anyhow::anyhow!(
            "failed to spawn updater helper {}: {}",
            crate::path_utils::normalize_path_for_display(helper_path),
            error
        )),
    }
}

#[cfg(target_os = "windows")]
fn attempt_windows_helper_launch<T>(
    helper_path: &Path,
    mut launch: impl FnMut(&Path) -> std::io::Result<T>,
    mut pause: impl FnMut(Duration),
) -> Result<T> {
    // Regression guard: copied helpers can be rejected transiently immediately after creation.
    // Keep this retry contract aligned with restart recovery and the TC-187 regression tests.
    let fallback = windows_non_verbatim_path(helper_path);
    let mut failures = Vec::new();
    for round in 1..=WINDOWS_HELPER_LAUNCH_ROUNDS {
        for (route, candidate) in std::iter::once(("primary path", helper_path)).chain(
            fallback
                .as_deref()
                .map(|path| ("normalized-path retry", path)),
        ) {
            match launch(candidate) {
                Ok(value) => return Ok(value),
                Err(error) => failures.push(format!("round {round} {route} failed: {error}")),
            }
        }
        if round < WINDOWS_HELPER_LAUNCH_ROUNDS {
            pause(WINDOWS_HELPER_RETRY_DELAY);
        }
    }
    Err(anyhow::anyhow!(
        "failed to spawn updater helper {} after {} rounds: {}",
        crate::path_utils::normalize_path_for_display(helper_path),
        WINDOWS_HELPER_LAUNCH_ROUNDS,
        failures.join("; ")
    ))
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
        let mut attempts = 0;
        let mut pauses = Vec::new();

        let error = attempt_windows_helper_launch(
            helper,
            |_| -> std::io::Result<()> {
                attempts += 1;
                Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "launch rejected",
                ))
            },
            |delay| pauses.push(delay),
        )
        .expect_err("all launch rounds should fail");
        let message = error.to_string();

        assert_eq!(attempts, WINDOWS_HELPER_LAUNCH_ROUNDS * 2);
        assert_eq!(
            pauses,
            vec![WINDOWS_HELPER_RETRY_DELAY; WINDOWS_HELPER_LAUNCH_ROUNDS - 1]
        );
        assert!(message.contains("after 3 rounds"));
        assert!(message.contains(r"\\server\share\flistwalker-update-helper.exe"));
        assert!(message.contains("normalized-path retry failed"));
        assert!(!message.contains(r"\\?\"), "unexpected error: {message}");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn tc187_regression_windows_helper_retries_transient_normal_path_failure() {
        let helper = Path::new(r"C:\tools\flistwalker-update-helper.exe");
        let mut attempts = 0;
        let mut pauses = Vec::new();

        attempt_windows_helper_launch(
            helper,
            |_| -> std::io::Result<()> {
                attempts += 1;
                if attempts == 1 {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "transient launch rejection",
                    ))
                } else {
                    Ok(())
                }
            },
            |delay| pauses.push(delay),
        )
        .expect("normal helper path should recover from a transient launch failure");

        assert_eq!(attempts, 2);
        assert_eq!(pauses, vec![WINDOWS_HELPER_RETRY_DELAY]);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn tc187_regression_windows_helper_retry_rounds_include_verbatim_fallback() {
        let helper = Path::new(r"\\?\C:\very-long\flistwalker-update-helper.exe");
        let mut attempted = Vec::new();
        let mut pauses = Vec::new();

        attempt_windows_helper_launch(
            helper,
            |path| {
                attempted.push(path.to_path_buf());
                if attempted.len() < 3 {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "transient launch rejection",
                    ))
                } else {
                    Ok(())
                }
            },
            |delay| pauses.push(delay),
        )
        .expect("a later retry round should recover after both path forms fail once");

        assert_eq!(
            attempted,
            vec![
                helper.to_path_buf(),
                PathBuf::from(r"C:\very-long\flistwalker-update-helper.exe"),
                helper.to_path_buf(),
            ]
        );
        assert_eq!(pauses, vec![WINDOWS_HELPER_RETRY_DELAY]);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn tc187_regression_detached_gui_helper_does_not_inherit_stale_stdio() {
        const PROBE_ENV: &str = "FLISTWALKER_TEST_DETACHED_HELPER_STDIO";
        if std::env::var_os(PROBE_ENV).is_none() {
            let status = Command::new(std::env::current_exe().expect("current test executable"))
                .args([
                    "--exact",
                    "updater::apply::tests::tc187_regression_detached_gui_helper_does_not_inherit_stale_stdio",
                ])
                .env(PROBE_ENV, "1")
                .status()
                .expect("spawn detached-stdio probe process");
            assert!(status.success(), "detached-stdio probe failed: {status}");
            return;
        }

        use std::fs::OpenOptions;
        use std::os::windows::io::AsRawHandle;

        #[link(name = "kernel32")]
        extern "system" {
            fn SetStdHandle(standard_handle: u32, handle: *mut std::ffi::c_void) -> i32;
        }

        const STD_INPUT_HANDLE: u32 = -10i32 as u32;
        const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
        const STD_ERROR_HANDLE: u32 = -12i32 as u32;

        let stale = OpenOptions::new()
            .read(true)
            .write(true)
            .open("NUL")
            .expect("open NUL for stale-handle probe");
        for stream in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            assert_ne!(
                unsafe { SetStdHandle(stream, stale.as_raw_handle().cast()) },
                0,
                "set probe standard handle"
            );
        }
        drop(stale);

        let args = [
            OsString::from("--exact"),
            OsString::from("updater::apply::tests::detached_helper_child_noop"),
        ];
        let mut command = helper_command(
            &std::env::current_exe().expect("current test executable"),
            &args,
        );
        command.env_remove(PROBE_ENV);
        let status = command
            .spawn()
            .expect("helper spawn must not depend on detached GUI standard handles")
            .wait()
            .expect("wait for helper stdio probe");
        assert!(status.success(), "helper stdio probe failed: {status}");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn detached_helper_child_noop() {}
}

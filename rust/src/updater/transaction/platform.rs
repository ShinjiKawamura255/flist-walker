use crate::updater::UpdateRestartMode;
#[cfg(any(target_os = "windows", target_os = "macos"))]
use anyhow::bail;
#[cfg(not(target_os = "macos"))]
use anyhow::Context;
use anyhow::Result;
#[cfg(all(unix, not(target_os = "macos")))]
use std::fs;
use std::path::Path;
#[cfg(not(target_os = "macos"))]
use std::path::PathBuf;
#[cfg(not(target_os = "macos"))]
use std::process::Command;
#[cfg(target_os = "windows")]
use std::process::Stdio;
use std::time::Duration;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
#[cfg(target_os = "windows")]
const WINDOWS_RESTART_ROUNDS: usize = 3;
#[cfg(target_os = "windows")]
const WINDOWS_RESTART_RETRY_DELAY: Duration = Duration::from_millis(100);
#[cfg(target_os = "windows")]
const WINDOWS_GUI_STARTUP_GRACE: Duration = Duration::from_millis(500);

#[cfg(target_os = "windows")]
pub(in crate::updater) fn windows_hidden_child_command(target: &Path) -> Command {
    // Regression guard: the GUI path detaches from its console before updater work starts, so
    // inherited standard handles may be stale. Keep all hidden updater children on explicit NUL
    // handles; do not restore implicit stdio inheritance without the TC-187 native probe.
    let mut command = Command::new(target);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

pub(in crate::updater) trait ProcessControl {
    fn wait_for_exit(&mut self, pid: u32, timeout: Duration) -> Result<bool>;
    fn restart(&mut self, target: &Path, mode: UpdateRestartMode) -> Result<()>;
}

pub(in crate::updater) trait ProcessProbe {
    fn is_alive(&self, pid: u32) -> bool;
    fn executable_matches(&self, pid: u32, expected: &Path) -> bool;
    fn is_current_process(&self, _pid: u32) -> bool {
        false
    }
}

pub(super) struct RealProcessControl;

impl ProcessProbe for RealProcessControl {
    fn is_alive(&self, pid: u32) -> bool {
        process_is_alive(pid)
    }

    fn is_current_process(&self, pid: u32) -> bool {
        pid == std::process::id()
    }

    fn executable_matches(&self, pid: u32, expected: &Path) -> bool {
        process_executable_matches(pid, expected)
    }
}

impl ProcessControl for RealProcessControl {
    fn wait_for_exit(&mut self, pid: u32, timeout: Duration) -> Result<bool> {
        wait_for_process_exit(pid, timeout)
    }

    fn restart(&mut self, target: &Path, mode: UpdateRestartMode) -> Result<()> {
        restart_target(target, mode)
    }
}

#[cfg(target_os = "windows")]
pub(super) fn windows_file_replace(
    source: &Path,
    target: &Path,
    backup: Option<&Path>,
) -> Result<()> {
    use std::ffi::c_void;

    #[link(name = "kernel32")]
    extern "system" {
        fn ReplaceFileW(
            replaced_file_name: *const u16,
            replacement_file_name: *const u16,
            backup_file_name: *const u16,
            replace_flags: u32,
            exclude: *mut c_void,
            reserved: *mut c_void,
        ) -> i32;
        fn GetLastError() -> u32;
    }

    let source_wide = windows_replace_wide_path(source)?;
    let target_wide = windows_replace_wide_path(target)?;
    let backup_wide = backup.map(windows_replace_wide_path).transpose()?;
    let backup_pointer = backup_wide
        .as_ref()
        .map_or(std::ptr::null(), |path| path.as_ptr());

    // Regression guard: updater replacement must stay in-process so one transaction cannot
    // flash many PowerShell terminals or fail because an external shell is unavailable.
    // Do not replace this call with a child process without updating the paired regression tests.
    let replaced = unsafe {
        ReplaceFileW(
            target_wide.as_ptr(),
            source_wide.as_ptr(),
            backup_pointer,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if replaced != 0 {
        return Ok(());
    }

    let error = unsafe { GetLastError() };
    Err(std::io::Error::from_raw_os_error(error as i32)).with_context(|| {
        format!(
            "ReplaceFileW failed while replacing {} with {}",
            target.display(),
            source.display()
        )
    })
}

#[cfg(target_os = "windows")]
pub(super) fn windows_replace_wide_path(path: &Path) -> Result<Vec<u16>> {
    use std::os::windows::ffi::OsStrExt;

    let mut wide = path.as_os_str().encode_wide().collect::<Vec<_>>();
    if wide.contains(&0) {
        bail!("Windows replacement path contains an interior NUL");
    }
    wide.push(0);
    Ok(wide)
}

#[cfg(target_os = "windows")]
pub(super) fn process_is_alive(pid: u32) -> bool {
    wait_for_process_exit(pid, Duration::ZERO)
        .map(|exited| !exited)
        .unwrap_or(true)
}

#[cfg(target_os = "windows")]
pub(super) fn wait_for_process_exit(pid: u32, timeout: Duration) -> Result<bool> {
    use std::ffi::c_void;
    type Handle = *mut c_void;
    const SYNCHRONIZE: u32 = 0x0010_0000;
    const WAIT_OBJECT_0: u32 = 0;
    const WAIT_TIMEOUT: u32 = 258;
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, process_id: u32) -> Handle;
        fn WaitForSingleObject(handle: Handle, milliseconds: u32) -> u32;
        fn CloseHandle(handle: Handle) -> i32;
        fn GetLastError() -> u32;
    }
    let handle = unsafe { OpenProcess(SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        const ERROR_INVALID_PARAMETER: u32 = 87;
        let error = unsafe { GetLastError() };
        if error == ERROR_INVALID_PARAMETER {
            return Ok(true);
        }
        bail!("OpenProcess failed while waiting for PID {pid}: Windows error {error}");
    }
    let milliseconds = timeout.as_millis().min(u32::MAX as u128) as u32;
    let wait = unsafe { WaitForSingleObject(handle, milliseconds) };
    let _ = unsafe { CloseHandle(handle) };
    match wait {
        WAIT_OBJECT_0 => Ok(true),
        WAIT_TIMEOUT => Ok(false),
        other => bail!("WaitForSingleObject failed with code {other}"),
    }
}

#[cfg(target_os = "windows")]
pub(super) fn process_executable_matches(pid: u32, expected: &Path) -> bool {
    use std::ffi::{c_void, OsString};
    use std::os::windows::ffi::OsStringExt;
    type Handle = *mut c_void;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, process_id: u32) -> Handle;
        fn QueryFullProcessImageNameW(
            process: Handle,
            flags: u32,
            path: *mut u16,
            size: *mut u32,
        ) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
    }
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return false;
    }
    let mut buffer = vec![0u16; 32_768];
    let mut size = buffer.len() as u32;
    let success = unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut size) };
    let _ = unsafe { CloseHandle(handle) };
    if success == 0 {
        return false;
    }
    let actual = PathBuf::from(OsString::from_wide(&buffer[..size as usize]));
    actual.canonicalize().ok() == expected.canonicalize().ok()
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(super) fn process_is_alive(pid: u32) -> bool {
    PathBuf::from(format!("/proc/{pid}")).exists()
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(super) fn process_executable_matches(pid: u32, expected: &Path) -> bool {
    fs::read_link(format!("/proc/{pid}/exe"))
        .ok()
        .and_then(|path| path.canonicalize().ok())
        == expected.canonicalize().ok()
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(super) fn wait_for_process_exit(pid: u32, timeout: Duration) -> Result<bool> {
    let deadline = std::time::Instant::now() + timeout;
    while process_is_alive(pid) {
        if std::time::Instant::now() >= deadline {
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    Ok(true)
}

#[cfg(target_os = "macos")]
pub(super) fn process_is_alive(_pid: u32) -> bool {
    false
}

#[cfg(target_os = "macos")]
pub(super) fn process_executable_matches(_pid: u32, _expected: &Path) -> bool {
    false
}

#[cfg(target_os = "macos")]
pub(super) fn wait_for_process_exit(_pid: u32, _timeout: Duration) -> Result<bool> {
    Ok(true)
}

#[cfg(target_os = "windows")]
pub(super) fn restart_target(target: &Path, mode: UpdateRestartMode) -> Result<()> {
    // Regression guard: a one-shot CreateProcessW failure used to leave the application closed
    // after rollback. Keep bounded retries, non-verbatim fallback, and the GUI startup grace in
    // sync with the TC-186 regression tests.
    attempt_windows_restart_launch(
        target,
        |launch_path| launch_windows_restart_once(launch_path, mode),
        std::thread::sleep,
    )
}

#[cfg(target_os = "windows")]
fn launch_windows_restart_once(target: &Path, mode: UpdateRestartMode) -> std::io::Result<()> {
    let mut command = windows_hidden_child_command(target);
    // Regression guard: the restarted GUI must identify itself as the transaction handoff owner;
    // a normal startup can misclassify a still-exiting helper and show a false update failure.
    command.arg(mode.internal_restart_flag());
    // The updater must not surface a console even if a Windows build temporarily uses the
    // console subsystem. Headless restart is allowed to exit immediately after recovery, while
    // GUI restart must remain alive long enough to reject an immediate startup failure.
    let mut child = command.spawn()?;
    if mode == UpdateRestartMode::Gui {
        std::thread::sleep(WINDOWS_GUI_STARTUP_GRACE);
        match child.try_wait() {
            Ok(Some(status)) => {
                return Err(std::io::Error::other(format!(
                    "restarted GUI exited during startup: {status}"
                )));
            }
            Ok(None) => {}
            Err(error) => {
                // Do not retry while an unobserved child may still be alive; duplicate GUI
                // processes would race the same recovery marker.
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn attempt_windows_restart_launch(
    target: &Path,
    mut launch: impl FnMut(&Path) -> std::io::Result<()>,
    mut pause: impl FnMut(Duration),
) -> Result<()> {
    let fallback = crate::path_utils::windows_non_verbatim_path(target);
    let mut failures = Vec::new();
    for attempt in 1..=WINDOWS_RESTART_ROUNDS {
        for candidate in std::iter::once(target).chain(fallback.as_deref()) {
            match launch(candidate) {
                Ok(()) => return Ok(()),
                Err(error) => failures.push(format!(
                    "attempt {attempt} via {} failed: {error}",
                    crate::path_utils::normalize_path_for_display(candidate)
                )),
            }
        }
        if attempt < WINDOWS_RESTART_ROUNDS {
            pause(WINDOWS_RESTART_RETRY_DELAY);
        }
    }
    Err(anyhow::anyhow!(
        "failed to restart {} after {} rounds: {}",
        crate::path_utils::normalize_path_for_display(target),
        WINDOWS_RESTART_ROUNDS,
        failures.join("; ")
    ))
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(super) fn restart_target(target: &Path, mode: UpdateRestartMode) -> Result<()> {
    use std::os::unix::process::CommandExt;
    let mut command = Command::new(target);
    command.arg(mode.internal_restart_flag());
    let error = command.exec();
    Err(error).with_context(|| format!("failed to exec {}", target.display()))
}

#[cfg(target_os = "macos")]
pub(super) fn restart_target(_target: &Path, _mode: UpdateRestartMode) -> Result<()> {
    bail!("macOS auto-update is unsupported")
}

#[cfg(all(test, target_os = "windows"))]
mod restart_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn tc186_regression_windows_restart_retries_a_transient_spawn_failure() {
        let target = Path::new(r"C:\tools\flistwalker.exe");
        let mut attempted = Vec::new();
        let mut pauses = Vec::new();

        attempt_windows_restart_launch(
            target,
            |path| {
                attempted.push(path.to_path_buf());
                if attempted.len() == 1 {
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
        .expect("bounded retry should recover a transient launch failure");

        assert_eq!(attempted, vec![target.to_path_buf(), target.to_path_buf()]);
        assert_eq!(pauses, vec![WINDOWS_RESTART_RETRY_DELAY]);
    }

    #[test]
    fn tc186_regression_windows_restart_retries_without_verbatim_prefix() {
        let target = Path::new(r"\\?\C:\very-long\flistwalker.exe");
        let mut attempted = Vec::new();

        attempt_windows_restart_launch(
            target,
            |path| {
                attempted.push(path.to_path_buf());
                if attempted.len() == 1 {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "verbatim launch rejection",
                    ))
                } else {
                    Ok(())
                }
            },
            |_| {},
        )
        .expect("normalized-path fallback should launch the same target");

        assert_eq!(
            attempted,
            vec![
                target.to_path_buf(),
                PathBuf::from(r"C:\very-long\flistwalker.exe")
            ]
        );
    }

    #[test]
    fn tc186_regression_windows_restart_exhaustion_is_bounded_and_diagnostic() {
        let target = Path::new(r"C:\tools\flistwalker.exe");
        let mut attempts = 0;
        let mut pauses = Vec::new();

        let error = attempt_windows_restart_launch(
            target,
            |_| {
                attempts += 1;
                Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!("launch rejection {attempts}"),
                ))
            },
            |delay| pauses.push(delay),
        )
        .expect_err("persistent failure must stop after the bounded retries");

        assert_eq!(attempts, WINDOWS_RESTART_ROUNDS);
        assert_eq!(
            pauses,
            vec![WINDOWS_RESTART_RETRY_DELAY; WINDOWS_RESTART_ROUNDS - 1]
        );
        let message = error.to_string();
        assert!(message.contains("after 3 rounds"));
        assert!(message.contains("launch rejection 1"));
        assert!(message.contains("launch rejection 3"));
    }
}

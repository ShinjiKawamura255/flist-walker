use crate::path_utils::normalize_path_for_display;
#[cfg(target_os = "windows")]
use crate::path_utils::normalize_windows_shell_path;
use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;
#[cfg(target_os = "windows")]
use std::{ffi::OsStr, os::windows::ffi::OsStrExt, ptr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Open,
    Execute,
}

fn normalize_action_path_for_display(path: &Path) -> String {
    normalize_path_for_display(path)
}

#[cfg(target_os = "windows")]
fn encode_wide_null(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(target_os = "windows")]
fn shell_execute_error(code: isize) -> std::io::Error {
    if (2..=32).contains(&code) {
        return std::io::Error::from_raw_os_error(code as i32);
    }
    std::io::Error::other(format!("ShellExecuteW failed with code {code}"))
}

#[cfg(target_os = "windows")]
fn shell_open(path: &Path) -> std::io::Result<()> {
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn ShellExecuteW(
            hwnd: *mut std::ffi::c_void,
            lp_operation: *const u16,
            lp_file: *const u16,
            lp_parameters: *const u16,
            lp_directory: *const u16,
            n_show_cmd: i32,
        ) -> isize;
    }

    const SW_SHOWNORMAL: i32 = 1;
    let operation = encode_wide_null(OsStr::new("open"));
    let target = normalize_windows_shell_path(path);
    let target_wide = encode_wide_null(target.as_os_str());
    let result = unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            operation.as_ptr(),
            target_wide.as_ptr(),
            ptr::null(),
            ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if result <= 32 {
        return Err(shell_execute_error(result));
    }
    Ok(())
}

pub fn choose_action(path: &Path) -> Action {
    if path.is_dir() {
        return Action::Open;
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            let ext = ext.to_ascii_lowercase();
            if ["exe", "com", "bat", "cmd"].contains(&ext.as_str()) {
                return Action::Execute;
            }
        }
        return Action::Open;
    }

    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = path.metadata() {
            if metadata.permissions().mode() & 0o111 != 0 {
                return Action::Execute;
            }
        }
        Action::Open
    }
}

fn spawn_executable(path: &Path) -> std::io::Result<()> {
    Command::new(path).spawn().map(|_| ())
}

fn execute_or_open_with(
    path: &Path,
    execute: impl FnOnce(&Path) -> std::io::Result<()>,
    open: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    match choose_action(path) {
        Action::Open => open(path),
        Action::Execute => {
            let result = execute(path);
            #[cfg(target_os = "windows")]
            {
                match result {
                    Ok(()) => Ok(()),
                    Err(err) if err.raw_os_error() == Some(193) => open(path),
                    Err(err) => Err(err).with_context(|| {
                        format!(
                            "failed to execute {}",
                            normalize_action_path_for_display(path)
                        )
                    }),
                }
            }
            #[cfg(not(target_os = "windows"))]
            {
                result.map(|_| ()).with_context(|| {
                    format!(
                        "failed to execute {}",
                        normalize_action_path_for_display(path)
                    )
                })
            }
        }
    }
}

pub fn execute_or_open(path: &Path) -> Result<()> {
    execute_or_open_with(path, spawn_executable, open_with_default)
}

pub fn open_with_default(path: &Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        shell_open(path).with_context(|| {
            format!("failed to open {}", normalize_action_path_for_display(path))
        })?;
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(path).spawn().with_context(|| {
            format!("failed to open {}", normalize_action_path_for_display(path))
        })?;
        return Ok(());
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .with_context(|| {
                format!("failed to open {}", normalize_action_path_for_display(path))
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::fs;
    #[cfg(target_os = "windows")]
    use std::path::PathBuf;

    #[test]
    fn directory_is_open_action() {
        let dir = std::env::temp_dir();
        assert_eq!(choose_action(&dir), Action::Open);
    }

    #[test]
    fn non_exec_file_is_open_on_unix() {
        #[cfg(not(target_os = "windows"))]
        {
            let root = std::env::temp_dir().join("fff-rs-actions");
            let _ = fs::create_dir_all(&root);
            let file = root.join("note.txt");
            fs::write(&file, "x").expect("write file");
            assert_eq!(choose_action(&file), Action::Open);
        }
    }

    #[test]
    fn executable_file_is_execute_on_unix() {
        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::fs::PermissionsExt;

            let root = std::env::temp_dir().join("fff-rs-actions-exec");
            let _ = fs::create_dir_all(&root);
            let file = root.join("run.sh");
            fs::write(&file, "#!/bin/sh\necho hi\n").expect("write file");
            let mut perms = fs::metadata(&file).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&file, perms).expect("set permissions");
            assert_eq!(choose_action(&file), Action::Execute);
        }
    }

    #[test]
    fn windows_executable_extension_is_execute() {
        #[cfg(target_os = "windows")]
        {
            let root = std::env::temp_dir().join("fff-rs-actions-winext");
            let _ = fs::create_dir_all(&root);
            let exe = root.join("tool.exe");
            fs::write(&exe, "bin").expect("write exe");
            assert_eq!(choose_action(&exe), Action::Execute);
        }
    }

    #[test]
    fn windows_powershell_script_is_open_action() {
        #[cfg(target_os = "windows")]
        {
            let root = std::env::temp_dir().join("fff-rs-actions-winps1");
            let _ = fs::create_dir_all(&root);
            let script = root.join("tool.ps1");
            fs::write(&script, "Write-Host test").expect("write script");
            assert_eq!(choose_action(&script), Action::Open);
        }
    }

    #[test]
    fn open_action_uses_open_handler_and_skips_execute_handler() {
        let path = std::env::temp_dir().join("fff-rs-actions-open");
        let execute_called = Cell::new(false);
        let open_called = Cell::new(false);

        execute_or_open_with(
            &path,
            |_| {
                execute_called.set(true);
                Ok(())
            },
            |_| {
                open_called.set(true);
                Ok(())
            },
        )
        .expect("open path");

        assert!(!execute_called.get());
        assert!(open_called.get());
    }

    #[test]
    fn execute_action_uses_execute_handler_and_skips_open_handler() {
        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::fs::PermissionsExt;

            let root = std::env::temp_dir().join("fff-rs-actions-exec-handler");
            let _ = fs::create_dir_all(&root);
            let file = root.join("run.sh");
            fs::write(&file, "#!/bin/sh\necho hi\n").expect("write file");
            let mut perms = fs::metadata(&file).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&file, perms).expect("set permissions");

            let execute_called = Cell::new(false);
            let open_called = Cell::new(false);

            execute_or_open_with(
                &file,
                |_| {
                    execute_called.set(true);
                    Ok(())
                },
                |_| {
                    open_called.set(true);
                    Ok(())
                },
            )
            .expect("execute path");

            assert!(execute_called.get());
            assert!(!open_called.get());
        }
        #[cfg(target_os = "windows")]
        {
            let root = std::env::temp_dir().join("fff-rs-actions-exec-handler");
            let _ = fs::create_dir_all(&root);
            let exe = root.join("run.exe");
            fs::write(&exe, "bin").expect("write exe");

            let execute_called = Cell::new(false);
            let open_called = Cell::new(false);

            execute_or_open_with(
                &exe,
                |_| {
                    execute_called.set(true);
                    Ok(())
                },
                |_| {
                    open_called.set(true);
                    Ok(())
                },
            )
            .expect("execute path");

            assert!(execute_called.get());
            assert!(!open_called.get());
        }
    }

    #[test]
    fn execute_failure_returns_error_without_open_fallback_on_non_windows() {
        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::fs::PermissionsExt;

            let root = std::env::temp_dir().join("fff-rs-actions-exec-failure");
            let _ = fs::create_dir_all(&root);
            let file = root.join("run.sh");
            fs::write(&file, "#!/bin/sh\necho hi\n").expect("write file");
            let mut perms = fs::metadata(&file).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&file, perms).expect("set permissions");

            let execute_called = Cell::new(false);
            let open_called = Cell::new(false);
            let err = std::io::Error::other("boom");

            let result = execute_or_open_with(
                &file,
                |_| {
                    execute_called.set(true);
                    Err(std::io::Error::other(err.to_string()))
                },
                |_| {
                    open_called.set(true);
                    Ok(())
                },
            );

            assert!(result.is_err());
            assert!(execute_called.get());
            assert!(!open_called.get());
        }
    }

    #[test]
    fn windows_execute_error_193_falls_back_to_open_handler() {
        #[cfg(target_os = "windows")]
        {
            let root = std::env::temp_dir().join("fff-rs-actions-exec-fallback");
            let _ = fs::create_dir_all(&root);
            let exe = root.join("run.exe");
            fs::write(&exe, "bin").expect("write exe");

            let open_called = Cell::new(false);
            let result = execute_or_open_with(
                &exe,
                |_| Err(std::io::Error::from_raw_os_error(193)),
                |_| {
                    open_called.set(true);
                    Ok(())
                },
            );

            assert!(result.is_ok());
            assert!(open_called.get());
        }
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn normalize_action_path_for_display_strips_extended_prefix() {
        assert_eq!(
            normalize_action_path_for_display(Path::new(r"\\?\C:\Users\tester\file.txt")),
            r"C:\Users\tester\file.txt"
        );
        assert_eq!(
            normalize_action_path_for_display(Path::new(r"\\?\UNC\server\share\file.txt")),
            r"\\server\share\file.txt"
        );
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn normalize_windows_shell_path_strips_extended_prefix_and_keeps_special_chars() {
        let normalized =
            normalize_windows_shell_path(Path::new(r"\\?\C:\Users\tester\a&b [c];'!,()^$.txt"));
        assert_eq!(
            normalized,
            PathBuf::from(r"C:\Users\tester\a&b [c];'!,()^$.txt")
        );

        let unc =
            normalize_windows_shell_path(Path::new(r"\\?\UNC\server\share\dir&a\file[1].txt"));
        assert_eq!(unc, PathBuf::from(r"\\server\share\dir&a\file[1].txt"));
    }
}

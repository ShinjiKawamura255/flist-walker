use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Open,
    Execute,
}

pub fn choose_action(path: &Path) -> Action {
    if path.is_dir() {
        return Action::Open;
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            let ext = ext.to_ascii_lowercase();
            if ["exe", "com", "bat", "cmd", "ps1"].contains(&ext.as_str()) {
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

pub fn execute_or_open(path: &Path) -> Result<()> {
    match choose_action(path) {
        Action::Open => open_with_default(path),
        Action::Execute => {
            let result = Command::new(path).spawn();
            #[cfg(target_os = "windows")]
            {
                if let Err(err) = result {
                    if err.raw_os_error() == Some(193) {
                        return open_with_default(path);
                    }
                    return Err(err)
                        .with_context(|| format!("failed to execute {}", path.display()));
                }
                Ok(())
            }
            #[cfg(not(target_os = "windows"))]
            {
                result
                    .map(|_| ())
                    .with_context(|| format!("failed to execute {}", path.display()))
            }
        }
    }
}

pub fn open_with_default(path: &Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "", &path.to_string_lossy()])
            .spawn()
            .with_context(|| format!("failed to open {}", path.display()))?;
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(path)
            .spawn()
            .with_context(|| format!("failed to open {}", path.display()))?;
        return Ok(());
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .with_context(|| format!("failed to open {}", path.display()))?;
        return Ok(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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
}

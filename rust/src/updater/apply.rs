use crate::updater::{staging, VerifiedUpdateBundle};
use anyhow::{Context, Result};
#[cfg(any(test, all(unix, not(target_os = "macos"))))]
use std::fs;
#[cfg(all(unix, not(target_os = "macos")))]
use std::os::unix::fs::PermissionsExt;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::Path;
#[cfg(any(target_os = "windows", all(unix, not(target_os = "macos"))))]
use std::process::Command;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub(super) fn spawn_update_helper(current_exe: &Path, bundle: &VerifiedUpdateBundle) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        spawn_windows_update_helper(current_exe, bundle)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        spawn_linux_update_helper(current_exe, bundle)
    }
    #[cfg(target_os = "macos")]
    {
        let _ = (current_exe, bundle);
        anyhow::bail!("macOS auto-update is unsupported");
    }
}

#[cfg(target_os = "windows")]
fn spawn_windows_update_helper(current_exe: &Path, bundle: &VerifiedUpdateBundle) -> Result<()> {
    let script_path = bundle.temp_dir.join("apply-update.ps1");
    staging::write_new_staged_file(&script_path, windows_update_script())?;
    let mut command = Command::new("powershell.exe");
    command
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-File",
        ])
        .arg(&script_path)
        .arg(current_exe)
        .arg(&bundle.staged_path)
        .arg(&bundle.staged_readme_path)
        .arg(&bundle.staged_license_path)
        .arg(&bundle.staged_notices_path);
    command.creation_flags(CREATE_NO_WINDOW);
    command
        .spawn()
        .with_context(|| format!("failed to spawn updater {}", script_path.display()))?;
    Ok(())
}

#[cfg(any(test, target_os = "windows"))]
fn windows_update_script() -> &'static str {
    r#"
param(
    [string]$TargetPath,
    [string]$StagedPath,
[string]$ReadmePath,
[string]$LicensePath,
[string]$NoticesPath
)
$targetDir = Split-Path -Parent $TargetPath
$readmeTarget = Join-Path $targetDir 'README.txt'
$licenseTarget = Join-Path $targetDir 'LICENSE.txt'
$noticesTarget = Join-Path $targetDir 'THIRD_PARTY_NOTICES.txt'
for ($i = 0; $i -lt 100; $i++) {
    try {
        Copy-Item -LiteralPath $StagedPath -Destination $TargetPath -Force
        if (Test-Path -LiteralPath $ReadmePath) {
            Copy-Item -LiteralPath $ReadmePath -Destination $readmeTarget -Force
            Remove-Item -LiteralPath $ReadmePath -Force -ErrorAction SilentlyContinue
        }
        if (Test-Path -LiteralPath $LicensePath) {
            Copy-Item -LiteralPath $LicensePath -Destination $licenseTarget -Force
            Remove-Item -LiteralPath $LicensePath -Force -ErrorAction SilentlyContinue
        }
        if (Test-Path -LiteralPath $NoticesPath) {
            Copy-Item -LiteralPath $NoticesPath -Destination $noticesTarget -Force
            Remove-Item -LiteralPath $NoticesPath -Force -ErrorAction SilentlyContinue
        }
        Remove-Item -LiteralPath $StagedPath -Force -ErrorAction SilentlyContinue
        Start-Process -FilePath $TargetPath -WorkingDirectory $targetDir
        exit 0
    } catch {
        Start-Sleep -Milliseconds 200
    }
}
exit 1
"#
}

#[cfg(all(unix, not(target_os = "macos")))]
fn spawn_linux_update_helper(current_exe: &Path, bundle: &VerifiedUpdateBundle) -> Result<()> {
    let script_path = bundle.temp_dir.join("apply-update.sh");
    staging::write_new_staged_file(&script_path, linux_update_script())?;
    let mut perms = fs::metadata(&script_path)
        .with_context(|| format!("failed to read metadata {}", script_path.display()))?
        .permissions();
    perms.set_mode(0o700);
    fs::set_permissions(&script_path, perms)
        .with_context(|| format!("failed to chmod {}", script_path.display()))?;
    Command::new("sh")
        .arg(&script_path)
        .arg(current_exe)
        .arg(&bundle.staged_path)
        .arg(&bundle.staged_readme_path)
        .arg(&bundle.staged_license_path)
        .arg(&bundle.staged_notices_path)
        .spawn()
        .with_context(|| format!("failed to spawn updater {}", script_path.display()))?;
    Ok(())
}

#[cfg(any(test, all(unix, not(target_os = "macos"))))]
fn linux_update_script() -> &'static str {
    r#"#!/bin/sh
set -eu
target="$1"
staged="$2"
readme_src="$3"
license_src="$4"
notices_src="$5"
target_dir=$(dirname "$target")
ignore_target="$target_dir/flistwalker.ignore.txt"
readme_target="$target_dir/README.txt"
license_target="$target_dir/LICENSE.txt"
notices_target="$target_dir/THIRD_PARTY_NOTICES.txt"
for _ in $(seq 1 100); do
  if cp "$staged" "$target" 2>/dev/null; then
    if [ -f "$readme_src" ]; then
      cp "$readme_src" "$readme_target" 2>/dev/null || true
      rm -f "$readme_src"
    fi
    if [ -f "$license_src" ]; then
      cp "$license_src" "$license_target" 2>/dev/null || true
      rm -f "$license_src"
    fi
    if [ -f "$notices_src" ]; then
      cp "$notices_src" "$notices_target" 2>/dev/null || true
      rm -f "$notices_src"
    fi
    chmod 755 "$target" || true
    rm -f "$staged"
    cd "$target_dir"
    exec "$target"
  fi
  sleep 0.2
done
exit 1
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_update_script_preserves_argument_order_and_sidecars() {
        let script = windows_update_script();

        assert!(script.contains("[string]$TargetPath"));
        assert!(script.contains("[string]$StagedPath"));
        assert!(script.contains("[string]$ReadmePath"));
        assert!(script.contains("[string]$LicensePath"));
        assert!(script.contains("[string]$NoticesPath"));
        assert!(
            script.contains("Copy-Item -LiteralPath $StagedPath -Destination $TargetPath -Force")
        );
        assert!(script.contains("Start-Process -FilePath $TargetPath -WorkingDirectory $targetDir"));
    }

    #[test]
    fn linux_update_script_preserves_argument_order_and_sidecars() {
        let script = linux_update_script();

        assert!(script.contains("target=\"$1\""));
        assert!(script.contains("staged=\"$2\""));
        assert!(script.contains("readme_src=\"$3\""));
        assert!(script.contains("license_src=\"$4\""));
        assert!(script.contains("notices_src=\"$5\""));
        assert!(script.contains("cp \"$staged\" \"$target\""));
        assert!(script.contains("exec \"$target\""));
    }

    #[test]
    fn helper_script_creation_refuses_existing_path() {
        let dir = staging::test_unique_update_temp_dir().expect("temp dir");
        let script_path = dir.join("apply-update.sh");
        fs::write(&script_path, "existing").expect("write existing script");

        let err = staging::write_new_staged_file(&script_path, linux_update_script())
            .expect_err("helper script must not overwrite existing path");

        assert!(
            err.to_string().contains("failed to create new staged file"),
            "unexpected error: {err}"
        );
        assert_eq!(
            fs::read_to_string(&script_path).expect("read existing"),
            "existing"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}

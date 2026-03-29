use anyhow::{anyhow, bail, Context, Result};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read};
#[cfg(all(unix, not(target_os = "macos")))]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const RELEASES_LATEST_URL: &str =
    "https://api.github.com/repos/ShinjiKawamura255/flist-walker/releases/latest";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateSupport {
    Auto,
    ManualOnly { message: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateCandidate {
    pub current_version: String,
    pub target_version: String,
    pub release_url: String,
    pub asset_name: String,
    pub asset_url: String,
    pub checksum_url: String,
    pub support: UpdateSupport,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PlatformReleaseTarget {
    asset_name: String,
    support: UpdateSupport,
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Clone, Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

pub fn current_version_string() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

pub fn self_update_disabled() -> bool {
    env_flag("FLISTWALKER_DISABLE_SELF_UPDATE")
}

pub fn check_for_update() -> Result<Option<UpdateCandidate>> {
    if self_update_disabled() {
        return Ok(None);
    }
    let current_version = parse_version(env!("CARGO_PKG_VERSION"))?;
    let release = fetch_latest_release()?;
    let target_version = parse_version(&release.tag_name)?;
    if !should_offer_update(&current_version, &target_version) {
        return Ok(None);
    }

    let Some(platform_target) = current_platform_target(&target_version)? else {
        return Ok(None);
    };
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == platform_target.asset_name)
        .cloned()
        .with_context(|| format!("release asset missing: {}", platform_target.asset_name))?;
    let checksum = release
        .assets
        .iter()
        .find(|asset| asset.name == "SHA256SUMS")
        .cloned()
        .context("release asset missing: SHA256SUMS")?;

    Ok(Some(UpdateCandidate {
        current_version: current_version.to_string(),
        target_version: target_version.to_string(),
        release_url: release.html_url,
        asset_name: asset.name,
        asset_url: asset.browser_download_url,
        checksum_url: checksum.browser_download_url,
        support: platform_target.support,
    }))
}

pub fn prepare_and_start_update(candidate: &UpdateCandidate, current_exe: &Path) -> Result<()> {
    if self_update_disabled() {
        bail!("self-update is disabled by FLISTWALKER_DISABLE_SELF_UPDATE");
    }
    match &candidate.support {
        UpdateSupport::Auto => {}
        UpdateSupport::ManualOnly { message } => bail!("{message}"),
    }

    let temp_dir = unique_update_temp_dir()?;
    let staged_path = temp_dir.join(&candidate.asset_name);
    let checksum_path = temp_dir.join("SHA256SUMS");
    download_to_path(&candidate.asset_url, &staged_path)?;
    download_to_path(&candidate.checksum_url, &checksum_path)?;
    verify_download(&staged_path, &checksum_path, &candidate.asset_name)?;

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let mut perms = fs::metadata(&staged_path)
            .with_context(|| format!("failed to read metadata {}", staged_path.display()))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&staged_path, perms)
            .with_context(|| format!("failed to chmod {}", staged_path.display()))?;
    }

    spawn_update_helper(current_exe, &staged_path, &temp_dir)
}

fn fetch_latest_release() -> Result<GitHubRelease> {
    let response = ureq::get(&release_feed_url())
        .set("User-Agent", &format!("flistwalker/{}", current_version_string()))
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|err| anyhow!("failed to query latest release: {err}"))?;
    let body = response
        .into_string()
        .map_err(|err| anyhow!("failed to read latest release response: {err}"))?;
    serde_json::from_str(&body).context("failed to parse latest release response")
}

fn parse_version(text: &str) -> Result<Version> {
    Version::parse(text.trim_start_matches('v'))
        .with_context(|| format!("invalid semver version: {text}"))
}

pub fn should_skip_update_prompt(target_version: &str, skipped_version: Option<&str>) -> bool {
    let Some(skipped_version) = skipped_version.filter(|value| !value.trim().is_empty()) else {
        return false;
    };
    let Ok(target) = parse_version(target_version) else {
        return false;
    };
    let Ok(skipped) = parse_version(skipped_version) else {
        return false;
    };
    target <= skipped
}

fn release_feed_url() -> String {
    std::env::var("FLISTWALKER_UPDATE_FEED_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| RELEASES_LATEST_URL.to_string())
}

fn should_offer_update(current_version: &Version, target_version: &Version) -> bool {
    if target_version > current_version {
        return true;
    }
    if target_version == current_version && update_allow_same_version() {
        return true;
    }
    if target_version < current_version && update_allow_downgrade() {
        return true;
    }
    false
}

fn update_allow_same_version() -> bool {
    env_flag("FLISTWALKER_UPDATE_ALLOW_SAME_VERSION")
}

fn update_allow_downgrade() -> bool {
    env_flag("FLISTWALKER_UPDATE_ALLOW_DOWNGRADE")
}

fn env_flag(name: &str) -> bool {
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

fn current_platform_target(version: &Version) -> Result<Option<PlatformReleaseTarget>> {
    let version = version.to_string();
    #[cfg(target_os = "windows")]
    {
        return Ok(Some(PlatformReleaseTarget {
            asset_name: format!("FlistWalker-{version}-windows-x86_64.exe"),
            support: UpdateSupport::Auto,
        }));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        return Ok(Some(PlatformReleaseTarget {
            asset_name: format!("FlistWalker-{version}-linux-x86_64"),
            support: UpdateSupport::Auto,
        }));
    }
    #[cfg(target_os = "macos")]
    {
        let suffix = if cfg!(target_arch = "aarch64") {
            "macos-arm64"
        } else {
            "macos-x86_64"
        };
        return Ok(Some(PlatformReleaseTarget {
            asset_name: format!("FlistWalker-{version}-{suffix}"),
            support: UpdateSupport::ManualOnly {
                message: "macOS の自動更新は未対応です。GitHub Releases から手動更新してください。"
                    .to_string(),
            },
        }));
    }
    #[allow(unreachable_code)]
    Ok(None)
}

fn unique_update_temp_dir() -> Result<PathBuf> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let dir = std::env::temp_dir().join(format!("flistwalker-update-{nonce}"));
    fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    Ok(dir)
}

fn download_to_path(url: &str, out: &Path) -> Result<()> {
    let response = ureq::get(url)
        .set("User-Agent", &format!("flistwalker/{}", current_version_string()))
        .call()
        .map_err(|err| anyhow!("failed to download {url}: {err}"))?;
    let mut reader = response.into_reader();
    let mut file =
        fs::File::create(out).with_context(|| format!("failed to create {}", out.display()))?;
    std::io::copy(&mut reader, &mut file)
        .with_context(|| format!("failed to write {}", out.display()))?;
    Ok(())
}

fn verify_download(downloaded_file: &Path, checksum_file: &Path, asset_name: &str) -> Result<()> {
    let checksums = parse_sha256sums_file(checksum_file)?;
    let expected = checksums
        .get(asset_name)
        .with_context(|| format!("missing checksum for {asset_name}"))?;
    let actual = sha256_file(downloaded_file)?;
    if &actual != expected {
        bail!(
            "checksum mismatch for {asset_name}: expected {expected}, got {actual}"
        );
    }
    Ok(())
}

fn parse_sha256sums_file(path: &Path) -> Result<HashMap<String, String>> {
    let file = fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut out = HashMap::new();
    for line in BufReader::new(file).lines() {
        let line = line.with_context(|| format!("failed to read {}", path.display()))?;
        if let Some((hash, name)) = parse_checksum_line(&line) {
            out.insert(name.to_string(), hash.to_string());
        }
    }
    Ok(out)
}

fn parse_checksum_line(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut parts = trimmed.split_whitespace();
    let hash = parts.next()?;
    let name = parts.last().or_else(|| trimmed.split("  ").nth(1))?;
    Some((hash, name.trim_start_matches('*')))
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let read = file
            .read(&mut buf)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn spawn_update_helper(current_exe: &Path, staged_path: &Path, temp_dir: &Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        return spawn_windows_update_helper(current_exe, staged_path, temp_dir);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        return spawn_linux_update_helper(current_exe, staged_path, temp_dir);
    }
    #[cfg(target_os = "macos")]
    {
        let _ = (current_exe, staged_path, temp_dir);
        bail!("macOS auto-update is unsupported");
    }
}

#[cfg(target_os = "windows")]
fn spawn_windows_update_helper(current_exe: &Path, staged_path: &Path, temp_dir: &Path) -> Result<()> {
    let script_path = temp_dir.join("apply-update.ps1");
    let script = r#"
param(
    [string]$TargetPath,
    [string]$StagedPath
)
$targetDir = Split-Path -Parent $TargetPath
for ($i = 0; $i -lt 100; $i++) {
    try {
        Copy-Item -LiteralPath $StagedPath -Destination $TargetPath -Force
        Remove-Item -LiteralPath $StagedPath -Force -ErrorAction SilentlyContinue
        Start-Process -FilePath $TargetPath -WorkingDirectory $targetDir
        exit 0
    } catch {
        Start-Sleep -Milliseconds 200
    }
}
exit 1
"#;
    fs::write(&script_path, script)
        .with_context(|| format!("failed to write {}", script_path.display()))?;
    Command::new("powershell.exe")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script_path)
        .arg(current_exe)
        .arg(staged_path)
        .spawn()
        .with_context(|| format!("failed to spawn updater {}", script_path.display()))?;
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn spawn_linux_update_helper(current_exe: &Path, staged_path: &Path, temp_dir: &Path) -> Result<()> {
    let script_path = temp_dir.join("apply-update.sh");
    let script = r#"#!/bin/sh
set -eu
target="$1"
staged="$2"
target_dir=$(dirname "$target")
for _ in $(seq 1 100); do
  if cp "$staged" "$target" 2>/dev/null; then
    chmod 755 "$target" || true
    rm -f "$staged"
    cd "$target_dir"
    exec "$target"
  fi
  sleep 0.2
done
exit 1
"#;
    fs::write(&script_path, script)
        .with_context(|| format!("failed to write {}", script_path.display()))?;
    let mut perms = fs::metadata(&script_path)
        .with_context(|| format!("failed to read metadata {}", script_path.display()))?
        .permissions();
    perms.set_mode(0o700);
    fs::set_permissions(&script_path, perms)
        .with_context(|| format!("failed to chmod {}", script_path.display()))?;
    Command::new("sh")
        .arg(&script_path)
        .arg(current_exe)
        .arg(staged_path)
        .spawn()
        .with_context(|| format!("failed to spawn updater {}", script_path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_checksum_line_supports_sha256sum_format() {
        let (hash, name) =
            parse_checksum_line("abc123  FlistWalker-0.12.3-linux-x86_64").expect("checksum");
        assert_eq!(hash, "abc123");
        assert_eq!(name, "FlistWalker-0.12.3-linux-x86_64");
    }

    #[test]
    fn parse_version_accepts_tag_prefix() {
        let version = parse_version("v0.12.3").expect("version");
        assert_eq!(version, Version::new(0, 12, 3));
    }

    #[test]
    fn should_offer_update_supports_same_version_override() {
        assert!(!should_offer_update(&Version::new(0, 12, 3), &Version::new(0, 12, 3)));
        unsafe {
            std::env::set_var("FLISTWALKER_UPDATE_ALLOW_SAME_VERSION", "1");
        }
        assert!(should_offer_update(&Version::new(0, 12, 3), &Version::new(0, 12, 3)));
        unsafe {
            std::env::remove_var("FLISTWALKER_UPDATE_ALLOW_SAME_VERSION");
        }
    }

    #[test]
    fn should_offer_update_supports_downgrade_override() {
        assert!(!should_offer_update(&Version::new(0, 12, 3), &Version::new(0, 12, 2)));
        unsafe {
            std::env::set_var("FLISTWALKER_UPDATE_ALLOW_DOWNGRADE", "1");
        }
        assert!(should_offer_update(&Version::new(0, 12, 3), &Version::new(0, 12, 2)));
        unsafe {
            std::env::remove_var("FLISTWALKER_UPDATE_ALLOW_DOWNGRADE");
        }
    }

    #[test]
    fn self_update_disabled_flag_is_honored() {
        assert!(!self_update_disabled());
        unsafe {
            std::env::set_var("FLISTWALKER_DISABLE_SELF_UPDATE", "1");
        }
        assert!(self_update_disabled());
        unsafe {
            std::env::remove_var("FLISTWALKER_DISABLE_SELF_UPDATE");
        }
    }

    #[test]
    fn check_for_update_short_circuits_when_self_update_is_disabled() {
        unsafe {
            std::env::set_var("FLISTWALKER_DISABLE_SELF_UPDATE", "1");
        }
        let result = check_for_update().expect("disabled updates should skip network access");
        assert!(result.is_none());
        unsafe {
            std::env::remove_var("FLISTWALKER_DISABLE_SELF_UPDATE");
        }
    }

    #[test]
    fn should_skip_update_prompt_blocks_same_or_older_target_versions() {
        assert!(should_skip_update_prompt("0.12.3", Some("0.12.3")));
        assert!(should_skip_update_prompt("0.12.2", Some("0.12.3")));
        assert!(!should_skip_update_prompt("0.12.4", Some("0.12.3")));
        assert!(!should_skip_update_prompt("0.12.4", None));
    }

    #[test]
    fn checksum_verification_detects_match() {
        let dir = unique_update_temp_dir().expect("temp dir");
        let file_path = dir.join("sample.bin");
        let sums_path = dir.join("SHA256SUMS");
        fs::write(&file_path, b"hello").expect("write sample");
        fs::write(
            &sums_path,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824  sample.bin\n",
        )
        .expect("write sums");

        verify_download(&file_path, &sums_path, "sample.bin").expect("checksum match");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn current_platform_target_matches_release_asset_pattern() {
        let target = current_platform_target(&Version::new(0, 12, 3))
            .expect("platform")
            .expect("target");
        assert!(target.asset_name.starts_with("FlistWalker-0.12.3-"));
        assert_ne!(target.asset_name, "SHA256SUMS");
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn windows_support_is_auto() {
        let target = current_platform_target(&Version::new(0, 12, 3))
            .expect("platform")
            .expect("target");
        assert_eq!(target.support, UpdateSupport::Auto);
        assert!(target.asset_name.ends_with(".exe"));
    }

    #[test]
    #[cfg(all(unix, not(target_os = "macos")))]
    fn linux_support_is_auto() {
        let target = current_platform_target(&Version::new(0, 12, 3))
            .expect("platform")
            .expect("target");
        assert_eq!(target.support, UpdateSupport::Auto);
        assert!(target.asset_name.contains("-linux-"));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn macos_support_is_manual_only() {
        let target = current_platform_target(&Version::new(0, 12, 3))
            .expect("platform")
            .expect("target");
        assert!(matches!(target.support, UpdateSupport::ManualOnly { .. }));
    }
}

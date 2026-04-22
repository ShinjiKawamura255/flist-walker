use crate::update_security::{self, CHECKSUM_SIGNATURE_NAME};
use anyhow::{anyhow, bail, Context, Result};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read};
#[cfg(all(unix, not(target_os = "macos")))]
use std::os::unix::fs::PermissionsExt;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
#[cfg(any(target_os = "windows", all(unix, not(target_os = "macos"))))]
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const RELEASES_LATEST_URL: &str =
    "https://api.github.com/repos/ShinjiKawamura255/flist-walker/releases/latest";
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const SELF_UPDATE_DISABLE_FLAG_NAME: &str = "FLISTWALKER_DISABLE_SELF_UPDATE";
const FORCE_UPDATE_CHECK_FAILURE_FLAG_NAME: &str = "FLISTWALKER_FORCE_UPDATE_CHECK_FAILURE";

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
    pub readme_asset_name: String,
    pub readme_asset_url: String,
    pub license_asset_name: String,
    pub license_asset_url: String,
    pub notices_asset_name: String,
    pub notices_asset_url: String,
    pub ignore_sample_asset_name: Option<String>,
    pub ignore_sample_asset_url: Option<String>,
    pub checksum_url: String,
    pub checksum_signature_url: String,
    pub support: UpdateSupport,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PlatformReleaseTarget {
    asset_name: String,
    readme_asset_name: String,
    license_asset_name: String,
    notices_asset_name: String,
    ignore_sample_asset_name: Option<String>,
    support: UpdateSupport,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct UpdateReleaseAssets {
    asset: GitHubAsset,
    readme_asset: GitHubAsset,
    license_asset: GitHubAsset,
    notices_asset: GitHubAsset,
    ignore_sample_asset: Option<GitHubAsset>,
    checksum: GitHubAsset,
    checksum_signature: GitHubAsset,
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
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
    if self_update_disabled() {
        return Ok(None);
    }
    if let Some(message) = forced_update_check_failure_message() {
        bail!("{message}");
    }
    let current_version = parse_version(env!("CARGO_PKG_VERSION"))?;
    let release = fetch_latest_release()?;
    resolve_update_candidate_from_release(&current_version, &release)
}

fn resolve_update_candidate_from_release(
    current_version: &Version,
    release: &GitHubRelease,
) -> Result<Option<UpdateCandidate>> {
    let target_version = parse_version(&release.tag_name)?;
    if !should_offer_update(current_version, &target_version) {
        return Ok(None);
    }

    let Some(platform_target) = current_platform_target(&target_version)? else {
        return Ok(None);
    };
    let assets = select_release_assets(release, &platform_target)?;
    let support = effective_update_support(platform_target.support);

    Ok(Some(UpdateCandidate {
        current_version: current_version.to_string(),
        target_version: target_version.to_string(),
        release_url: release.html_url.clone(),
        asset_name: assets.asset.name,
        asset_url: assets.asset.browser_download_url,
        readme_asset_name: assets.readme_asset.name,
        readme_asset_url: assets.readme_asset.browser_download_url,
        license_asset_name: assets.license_asset.name,
        license_asset_url: assets.license_asset.browser_download_url,
        notices_asset_name: assets.notices_asset.name,
        notices_asset_url: assets.notices_asset.browser_download_url,
        ignore_sample_asset_name: assets
            .ignore_sample_asset
            .as_ref()
            .map(|asset| asset.name.clone()),
        ignore_sample_asset_url: assets
            .ignore_sample_asset
            .as_ref()
            .map(|asset| asset.browser_download_url.clone()),
        checksum_url: assets.checksum.browser_download_url,
        checksum_signature_url: assets.checksum_signature.browser_download_url,
        support,
    }))
}

fn select_release_assets(
    release: &GitHubRelease,
    platform_target: &PlatformReleaseTarget,
) -> Result<UpdateReleaseAssets> {
    Ok(UpdateReleaseAssets {
        asset: release_asset_by_name(release, &platform_target.asset_name)?,
        readme_asset: release_asset_by_name(release, &platform_target.readme_asset_name)?,
        license_asset: release_asset_by_name(release, &platform_target.license_asset_name)?,
        notices_asset: release_asset_by_name(release, &platform_target.notices_asset_name)?,
        ignore_sample_asset: platform_target
            .ignore_sample_asset_name
            .as_deref()
            .and_then(|name| {
                release
                    .assets
                    .iter()
                    .find(|asset| asset.name == name)
                    .cloned()
            }),
        checksum: release_asset_by_name(release, "SHA256SUMS")?,
        checksum_signature: release_asset_by_name(release, CHECKSUM_SIGNATURE_NAME)?,
    })
}

fn effective_update_support(platform_support: UpdateSupport) -> UpdateSupport {
    if platform_support == UpdateSupport::Auto && !update_security::has_embedded_public_key() {
        return UpdateSupport::ManualOnly {
            message:
                "このビルドには更新署名公開鍵が埋め込まれていないため、自動更新は利用できません。GitHub Releases から手動更新してください。"
                    .to_string(),
        };
    }
    platform_support
}

fn release_asset_by_name(release: &GitHubRelease, name: &str) -> Result<GitHubAsset> {
    release
        .assets
        .iter()
        .find(|asset| asset.name == name)
        .cloned()
        .with_context(|| format!("release asset missing: {name}"))
}

pub fn prepare_and_start_update(candidate: &UpdateCandidate, current_exe: &Path) -> Result<()> {
    if self_update_disabled() {
        bail!(
            "self-update is disabled by {} environment variable or sentinel file",
            SELF_UPDATE_DISABLE_FLAG_NAME
        );
    }
    match &candidate.support {
        UpdateSupport::Auto => {}
        UpdateSupport::ManualOnly { message } => bail!("{message}"),
    }

    let temp_dir = unique_update_temp_dir()?;
    let staged_path = temp_dir.join(&candidate.asset_name);
    let staged_readme_path = temp_dir.join(&candidate.readme_asset_name);
    let staged_license_path = temp_dir.join(&candidate.license_asset_name);
    let staged_notices_path = temp_dir.join(&candidate.notices_asset_name);
    let staged_ignore_sample_path = candidate
        .ignore_sample_asset_name
        .as_ref()
        .map(|name| temp_dir.join(name));
    let checksum_path = temp_dir.join("SHA256SUMS");
    let signature_path = temp_dir.join(CHECKSUM_SIGNATURE_NAME);
    download_to_path(&candidate.asset_url, &staged_path)?;
    download_to_path(&candidate.readme_asset_url, &staged_readme_path)?;
    download_to_path(&candidate.license_asset_url, &staged_license_path)?;
    download_to_path(&candidate.notices_asset_url, &staged_notices_path)?;
    if let (Some(url), Some(path)) = (
        &candidate.ignore_sample_asset_url,
        &staged_ignore_sample_path,
    ) {
        download_to_path(url, path)?;
    }
    download_to_path(&candidate.checksum_url, &checksum_path)?;
    download_to_path(&candidate.checksum_signature_url, &signature_path)?;
    verify_checksum_manifest_signature(&checksum_path, &signature_path)?;
    verify_download(&staged_path, &checksum_path, &candidate.asset_name)?;
    verify_download(
        &staged_readme_path,
        &checksum_path,
        &candidate.readme_asset_name,
    )?;
    verify_download(
        &staged_license_path,
        &checksum_path,
        &candidate.license_asset_name,
    )?;
    verify_download(
        &staged_notices_path,
        &checksum_path,
        &candidate.notices_asset_name,
    )?;
    if let (Some(staged_ignore_sample_path), Some(ignore_sample_asset_name)) = (
        staged_ignore_sample_path.as_deref(),
        candidate.ignore_sample_asset_name.as_deref(),
    ) {
        verify_download(
            staged_ignore_sample_path,
            &checksum_path,
            ignore_sample_asset_name,
        )?;
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let mut perms = fs::metadata(&staged_path)
            .with_context(|| format!("failed to read metadata {}", staged_path.display()))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&staged_path, perms)
            .with_context(|| format!("failed to chmod {}", staged_path.display()))?;
    }

    spawn_update_helper(
        current_exe,
        &staged_path,
        &staged_readme_path,
        &staged_license_path,
        &staged_notices_path,
        staged_ignore_sample_path.as_deref(),
        &temp_dir,
    )
}

fn fetch_latest_release() -> Result<GitHubRelease> {
    let response = ureq::get(&release_feed_url())
        .set(
            "User-Agent",
            &format!("flistwalker/{}", current_version_string()),
        )
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
            readme_asset_name: format!("FlistWalker-{version}-windows-x86_64.README.txt"),
            license_asset_name: format!("FlistWalker-{version}-windows-x86_64.LICENSE.txt"),
            notices_asset_name: format!(
                "FlistWalker-{version}-windows-x86_64.THIRD_PARTY_NOTICES.txt"
            ),
            ignore_sample_asset_name: Some(format!(
                "FlistWalker-{version}-windows-x86_64.ignore.txt.example"
            )),
            support: UpdateSupport::Auto,
        }));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        return Ok(Some(PlatformReleaseTarget {
            asset_name: format!("FlistWalker-{version}-linux-x86_64"),
            readme_asset_name: format!("FlistWalker-{version}-linux-x86_64.README.txt"),
            license_asset_name: format!("FlistWalker-{version}-linux-x86_64.LICENSE.txt"),
            notices_asset_name: format!(
                "FlistWalker-{version}-linux-x86_64.THIRD_PARTY_NOTICES.txt"
            ),
            ignore_sample_asset_name: Some(format!(
                "FlistWalker-{version}-linux-x86_64.ignore.txt.example"
            )),
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
            readme_asset_name: format!("FlistWalker-{version}-{suffix}.README.txt"),
            license_asset_name: format!("FlistWalker-{version}-{suffix}.LICENSE.txt"),
            notices_asset_name: format!("FlistWalker-{version}-{suffix}.THIRD_PARTY_NOTICES.txt"),
            ignore_sample_asset_name: Some(format!(
                "FlistWalker-{version}-{suffix}.ignore.txt.example"
            )),
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
        .set(
            "User-Agent",
            &format!("flistwalker/{}", current_version_string()),
        )
        .call()
        .map_err(|err| anyhow!("failed to download {url}: {err}"))?;
    let mut reader = response.into_reader();
    let mut file =
        fs::File::create(out).with_context(|| format!("failed to create {}", out.display()))?;
    std::io::copy(&mut reader, &mut file)
        .with_context(|| format!("failed to write {}", out.display()))?;
    Ok(())
}

fn verify_checksum_manifest_signature(checksum_file: &Path, signature_file: &Path) -> Result<()> {
    let message = fs::read(checksum_file)
        .with_context(|| format!("failed to read {}", checksum_file.display()))?;
    let signature = fs::read(signature_file)
        .with_context(|| format!("failed to read {}", signature_file.display()))?;
    update_security::verify_embedded_signature(&message, &signature)
        .context("failed to verify update checksum signature")
}

fn verify_download(downloaded_file: &Path, checksum_file: &Path, asset_name: &str) -> Result<()> {
    let checksums = parse_sha256sums_file(checksum_file)?;
    let expected = checksums
        .get(asset_name)
        .with_context(|| format!("missing checksum for {asset_name}"))?;
    let actual = sha256_file(downloaded_file)?;
    if &actual != expected {
        bail!("checksum mismatch for {asset_name}: expected {expected}, got {actual}");
    }
    Ok(())
}

fn parse_sha256sums_file(path: &Path) -> Result<HashMap<String, String>> {
    let file =
        fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
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
    let mut file =
        fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
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

fn spawn_update_helper(
    current_exe: &Path,
    staged_path: &Path,
    staged_readme_path: &Path,
    staged_license_path: &Path,
    staged_notices_path: &Path,
    staged_ignore_sample_path: Option<&Path>,
    temp_dir: &Path,
) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        return spawn_windows_update_helper(
            current_exe,
            staged_path,
            staged_readme_path,
            staged_license_path,
            staged_notices_path,
            staged_ignore_sample_path,
            temp_dir,
        );
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        spawn_linux_update_helper(
            current_exe,
            staged_path,
            staged_readme_path,
            staged_license_path,
            staged_notices_path,
            staged_ignore_sample_path,
            temp_dir,
        )
    }
    #[cfg(target_os = "macos")]
    {
        let _ = (
            current_exe,
            staged_path,
            staged_readme_path,
            staged_license_path,
            staged_notices_path,
            staged_ignore_sample_path,
            temp_dir,
        );
        bail!("macOS auto-update is unsupported");
    }
}

#[cfg(target_os = "windows")]
fn spawn_windows_update_helper(
    current_exe: &Path,
    staged_path: &Path,
    staged_readme_path: &Path,
    staged_license_path: &Path,
    staged_notices_path: &Path,
    staged_ignore_sample_path: Option<&Path>,
    temp_dir: &Path,
) -> Result<()> {
    let script_path = temp_dir.join("apply-update.ps1");
    let script = r#"
param(
    [string]$TargetPath,
    [string]$StagedPath,
[string]$ReadmePath,
[string]$LicensePath,
[string]$NoticesPath,
[string]$IgnoreSamplePath
)
$targetDir = Split-Path -Parent $TargetPath
$ignoreTarget = Join-Path $targetDir 'flistwalker.ignore.txt'
$ignoreSampleTarget = Join-Path $targetDir 'flistwalker.ignore.txt.example'
$readmeTarget = Join-Path $targetDir 'README.txt'
$licenseTarget = Join-Path $targetDir 'LICENSE.txt'
$noticesTarget = Join-Path $targetDir 'THIRD_PARTY_NOTICES.txt'
for ($i = 0; $i -lt 100; $i++) {
    try {
        Copy-Item -LiteralPath $StagedPath -Destination $TargetPath -Force
        if (-not (Test-Path -LiteralPath $ignoreTarget) -and $IgnoreSamplePath) {
            if (Test-Path -LiteralPath $IgnoreSamplePath) {
                Copy-Item -LiteralPath $IgnoreSamplePath -Destination $ignoreSampleTarget -Force
            }
        }
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
"#;
    fs::write(&script_path, script)
        .with_context(|| format!("failed to write {}", script_path.display()))?;
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
        .arg(staged_path)
        .arg(staged_readme_path)
        .arg(staged_license_path)
        .arg(staged_notices_path);
    command.arg(
        staged_ignore_sample_path
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default(),
    );
    command.creation_flags(CREATE_NO_WINDOW);
    command
        .spawn()
        .with_context(|| format!("failed to spawn updater {}", script_path.display()))?;
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn spawn_linux_update_helper(
    current_exe: &Path,
    staged_path: &Path,
    staged_readme_path: &Path,
    staged_license_path: &Path,
    staged_notices_path: &Path,
    staged_ignore_sample_path: Option<&Path>,
    temp_dir: &Path,
) -> Result<()> {
    let script_path = temp_dir.join("apply-update.sh");
    let script = r#"#!/bin/sh
set -eu
target="$1"
staged="$2"
readme_src="$3"
license_src="$4"
notices_src="$5"
ignore_sample_src="$6"
target_dir=$(dirname "$target")
ignore_target="$target_dir/flistwalker.ignore.txt"
ignore_sample_target="$target_dir/flistwalker.ignore.txt.example"
readme_target="$target_dir/README.txt"
license_target="$target_dir/LICENSE.txt"
notices_target="$target_dir/THIRD_PARTY_NOTICES.txt"
for _ in $(seq 1 100); do
  if cp "$staged" "$target" 2>/dev/null; then
    if [ ! -e "$ignore_target" ] && [ -n "$ignore_sample_src" ] && [ -f "$ignore_sample_src" ]; then
      cp "$ignore_sample_src" "$ignore_sample_target" 2>/dev/null || true
    fi
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
        .arg(staged_readme_path)
        .arg(staged_license_path)
        .arg(staged_notices_path)
        .arg(
            staged_ignore_sample_path
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default(),
        )
        .spawn()
        .with_context(|| format!("failed to spawn updater {}", script_path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update_security;

    const TEST_SIGNING_KEY_HEX: &str =
        "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";
    const TEST_PUBLIC_KEY_HEX: &str =
        "79b5562e8fe654f94078b112e8a98ba7901f853ae695bed7e0e3910bad049664";

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
        let _env_lock = crate::env_var_test_lock()
            .lock()
            .expect("env var test lock");
        assert!(!should_offer_update(
            &Version::new(0, 12, 3),
            &Version::new(0, 12, 3)
        ));
        unsafe {
            std::env::set_var("FLISTWALKER_UPDATE_ALLOW_SAME_VERSION", "1");
        }
        assert!(should_offer_update(
            &Version::new(0, 12, 3),
            &Version::new(0, 12, 3)
        ));
        unsafe {
            std::env::remove_var("FLISTWALKER_UPDATE_ALLOW_SAME_VERSION");
        }
    }

    #[test]
    fn should_offer_update_supports_downgrade_override() {
        let _env_lock = crate::env_var_test_lock()
            .lock()
            .expect("env var test lock");
        assert!(!should_offer_update(
            &Version::new(0, 12, 3),
            &Version::new(0, 12, 2)
        ));
        unsafe {
            std::env::set_var("FLISTWALKER_UPDATE_ALLOW_DOWNGRADE", "1");
        }
        assert!(should_offer_update(
            &Version::new(0, 12, 3),
            &Version::new(0, 12, 2)
        ));
        unsafe {
            std::env::remove_var("FLISTWALKER_UPDATE_ALLOW_DOWNGRADE");
        }
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

    #[test]
    fn should_skip_update_prompt_blocks_same_or_older_target_versions() {
        assert!(should_skip_update_prompt("0.12.3", Some("0.12.3")));
        assert!(should_skip_update_prompt("0.12.2", Some("0.12.3")));
        assert!(!should_skip_update_prompt("0.12.4", Some("0.12.3")));
        assert!(!should_skip_update_prompt("0.12.4", None));
    }

    fn test_release(tag_name: &str) -> GitHubRelease {
        GitHubRelease {
            tag_name: tag_name.to_string(),
            html_url: "https://example.invalid/release".to_string(),
            assets: vec![
                GitHubAsset {
                    name: "FlistWalker-0.13.1-windows-x86_64.exe".to_string(),
                    browser_download_url: "https://example.invalid/windows-exe".to_string(),
                },
                GitHubAsset {
                    name: "FlistWalker-0.13.1-windows-x86_64.README.txt".to_string(),
                    browser_download_url: "https://example.invalid/windows-readme".to_string(),
                },
                GitHubAsset {
                    name: "FlistWalker-0.13.1-windows-x86_64.LICENSE.txt".to_string(),
                    browser_download_url: "https://example.invalid/windows-license".to_string(),
                },
                GitHubAsset {
                    name: "FlistWalker-0.13.1-windows-x86_64.THIRD_PARTY_NOTICES.txt".to_string(),
                    browser_download_url: "https://example.invalid/windows-notices".to_string(),
                },
                GitHubAsset {
                    name: "FlistWalker-0.13.1-windows-x86_64.ignore.txt.example".to_string(),
                    browser_download_url: "https://example.invalid/windows-ignore-sample"
                        .to_string(),
                },
                GitHubAsset {
                    name: "FlistWalker-0.13.1-linux-x86_64".to_string(),
                    browser_download_url: "https://example.invalid/linux-bin".to_string(),
                },
                GitHubAsset {
                    name: "FlistWalker-0.13.1-linux-x86_64.README.txt".to_string(),
                    browser_download_url: "https://example.invalid/linux-readme".to_string(),
                },
                GitHubAsset {
                    name: "FlistWalker-0.13.1-linux-x86_64.LICENSE.txt".to_string(),
                    browser_download_url: "https://example.invalid/linux-license".to_string(),
                },
                GitHubAsset {
                    name: "FlistWalker-0.13.1-linux-x86_64.THIRD_PARTY_NOTICES.txt".to_string(),
                    browser_download_url: "https://example.invalid/linux-notices".to_string(),
                },
                GitHubAsset {
                    name: "FlistWalker-0.13.1-linux-x86_64.ignore.txt.example".to_string(),
                    browser_download_url: "https://example.invalid/linux-ignore-sample".to_string(),
                },
                GitHubAsset {
                    name: "FlistWalker-0.13.1-macos-x86_64".to_string(),
                    browser_download_url: "https://example.invalid/macos-x64-bin".to_string(),
                },
                GitHubAsset {
                    name: "FlistWalker-0.13.1-macos-x86_64.README.txt".to_string(),
                    browser_download_url: "https://example.invalid/macos-x64-readme".to_string(),
                },
                GitHubAsset {
                    name: "FlistWalker-0.13.1-macos-x86_64.LICENSE.txt".to_string(),
                    browser_download_url: "https://example.invalid/macos-x64-license".to_string(),
                },
                GitHubAsset {
                    name: "FlistWalker-0.13.1-macos-x86_64.THIRD_PARTY_NOTICES.txt".to_string(),
                    browser_download_url: "https://example.invalid/macos-x64-notices".to_string(),
                },
                GitHubAsset {
                    name: "FlistWalker-0.13.1-macos-x86_64.ignore.txt.example".to_string(),
                    browser_download_url: "https://example.invalid/macos-x64-ignore-sample"
                        .to_string(),
                },
                GitHubAsset {
                    name: "FlistWalker-0.13.1-macos-arm64".to_string(),
                    browser_download_url: "https://example.invalid/macos-arm-bin".to_string(),
                },
                GitHubAsset {
                    name: "FlistWalker-0.13.1-macos-arm64.README.txt".to_string(),
                    browser_download_url: "https://example.invalid/macos-arm-readme".to_string(),
                },
                GitHubAsset {
                    name: "FlistWalker-0.13.1-macos-arm64.LICENSE.txt".to_string(),
                    browser_download_url: "https://example.invalid/macos-arm-license".to_string(),
                },
                GitHubAsset {
                    name: "FlistWalker-0.13.1-macos-arm64.THIRD_PARTY_NOTICES.txt".to_string(),
                    browser_download_url: "https://example.invalid/macos-arm-notices".to_string(),
                },
                GitHubAsset {
                    name: "FlistWalker-0.13.1-macos-arm64.ignore.txt.example".to_string(),
                    browser_download_url: "https://example.invalid/macos-arm-ignore-sample"
                        .to_string(),
                },
                GitHubAsset {
                    name: "SHA256SUMS".to_string(),
                    browser_download_url: "https://example.invalid/SHA256SUMS".to_string(),
                },
                GitHubAsset {
                    name: CHECKSUM_SIGNATURE_NAME.to_string(),
                    browser_download_url: "https://example.invalid/SHA256SUMS.sig".to_string(),
                },
            ],
        }
    }

    #[test]
    fn resolve_update_candidate_from_release_builds_candidate_from_assets() {
        let release = test_release("v0.13.1");
        let target = current_platform_target(&Version::new(0, 13, 1))
            .expect("platform target")
            .expect("target");
        let candidate = resolve_update_candidate_from_release(&Version::new(0, 13, 0), &release)
            .expect("candidate resolution")
            .expect("update candidate");

        assert_eq!(candidate.current_version, "0.13.0");
        assert_eq!(candidate.target_version, "0.13.1");
        assert_eq!(candidate.release_url, "https://example.invalid/release");
        assert_eq!(candidate.asset_name, target.asset_name);
        assert_eq!(
            candidate.checksum_signature_url,
            "https://example.invalid/SHA256SUMS.sig"
        );

        if update_security::has_embedded_public_key() {
            #[cfg(target_os = "macos")]
            assert!(matches!(
                candidate.support,
                UpdateSupport::ManualOnly { .. }
            ));

            #[cfg(not(target_os = "macos"))]
            assert_eq!(candidate.support, UpdateSupport::Auto);
        } else {
            assert!(matches!(
                candidate.support,
                UpdateSupport::ManualOnly { .. }
            ));
        }
    }

    #[test]
    fn select_release_assets_collects_expected_assets() {
        let release = test_release("v0.13.1");
        let target = current_platform_target(&Version::new(0, 13, 1))
            .expect("platform target")
            .expect("target");

        let assets = select_release_assets(&release, &target).expect("release assets");

        assert_eq!(assets.asset.name, target.asset_name);
        assert_eq!(assets.readme_asset.name, target.readme_asset_name);
        assert_eq!(assets.license_asset.name, target.license_asset_name);
        assert_eq!(assets.notices_asset.name, target.notices_asset_name);
        assert_eq!(
            assets
                .ignore_sample_asset
                .as_ref()
                .map(|asset| asset.name.as_str()),
            target.ignore_sample_asset_name.as_deref()
        );
        assert_eq!(assets.checksum.name, "SHA256SUMS");
        assert_eq!(assets.checksum_signature.name, CHECKSUM_SIGNATURE_NAME);
    }

    #[test]
    fn effective_update_support_respects_embedded_public_key_availability() {
        let support = effective_update_support(UpdateSupport::Auto);

        if update_security::has_embedded_public_key() {
            assert_eq!(support, UpdateSupport::Auto);
        } else {
            assert!(matches!(support, UpdateSupport::ManualOnly { .. }));
        }
    }

    #[test]
    fn resolve_update_candidate_from_release_skips_non_newer_versions() {
        let release = test_release("v0.13.0");
        let candidate = resolve_update_candidate_from_release(&Version::new(0, 13, 0), &release)
            .expect("candidate resolution");

        assert!(candidate.is_none());
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
    fn checksum_verification_detects_sidecar_match() {
        let dir = unique_update_temp_dir().expect("temp dir");
        let file_path = dir.join("sample.LICENSE.txt");
        let sums_path = dir.join("SHA256SUMS");
        fs::write(&file_path, b"license text").expect("write sample");
        let hash = sha256_file(&file_path).expect("hash");
        fs::write(&sums_path, format!("{hash}  sample.LICENSE.txt\n")).expect("write sums");

        verify_download(&file_path, &sums_path, "sample.LICENSE.txt").expect("checksum match");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn checksum_manifest_signature_verification_detects_match() {
        let dir = unique_update_temp_dir().expect("temp dir");
        let sums_path = dir.join("SHA256SUMS");
        let signature_path = dir.join(CHECKSUM_SIGNATURE_NAME);
        let message =
            b"2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824  sample.bin\n";
        let signature = update_security::sign_message(message, TEST_SIGNING_KEY_HEX).expect("sign");
        fs::write(&sums_path, message).expect("write sums");
        fs::write(&signature_path, signature).expect("write signature");

        let message = fs::read(&sums_path).expect("read sums");
        let signature = fs::read(&signature_path).expect("read signature");
        update_security::verify_signature(&message, &signature, TEST_PUBLIC_KEY_HEX)
            .expect("signature should verify");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn checksum_manifest_signature_verification_rejects_tampering() {
        let dir = unique_update_temp_dir().expect("temp dir");
        let sums_path = dir.join("SHA256SUMS");
        let signature_path = dir.join(CHECKSUM_SIGNATURE_NAME);
        let signed_message =
            b"2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824  sample.bin\n";
        let tampered_message =
            b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  sample.bin\n";
        let signature =
            update_security::sign_message(signed_message, TEST_SIGNING_KEY_HEX).expect("sign");
        fs::write(&sums_path, tampered_message).expect("write sums");
        fs::write(&signature_path, signature).expect("write signature");

        let message = fs::read(&sums_path).expect("read sums");
        let signature = fs::read(&signature_path).expect("read signature");
        let result = update_security::verify_signature(&message, &signature, TEST_PUBLIC_KEY_HEX);
        assert!(result.is_err(), "tampered manifest must fail verification");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn current_platform_target_matches_release_asset_pattern() {
        let target = current_platform_target(&Version::new(0, 12, 3))
            .expect("platform")
            .expect("target");
        assert!(target.asset_name.starts_with("FlistWalker-0.12.3-"));
        assert_ne!(target.asset_name, "SHA256SUMS");
        assert!(target.readme_asset_name.ends_with(".README.txt"));
        assert!(target.license_asset_name.ends_with(".LICENSE.txt"));
        assert!(target
            .notices_asset_name
            .ends_with(".THIRD_PARTY_NOTICES.txt"));
        assert!(target
            .ignore_sample_asset_name
            .as_deref()
            .is_some_and(|name| name.ends_with(".ignore.txt.example")));
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

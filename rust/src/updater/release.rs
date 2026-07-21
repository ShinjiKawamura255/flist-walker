use crate::update_security::{self, CHECKSUM_SIGNATURE_NAME};
use crate::updater::{env_flag, UpdateCandidate, UpdateSupport};
use anyhow::{Context, Result};
use semver::Version;
use serde::Deserialize;

const RELEASES_LATEST_URL: &str =
    "https://api.github.com/repos/ShinjiKawamura255/flist-walker/releases/latest";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PlatformReleaseTarget {
    pub(super) asset_name: String,
    pub(super) readme_asset_name: String,
    pub(super) license_asset_name: String,
    pub(super) notices_asset_name: String,
    pub(super) support: UpdateSupport,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct UpdateReleaseAssets {
    asset: GitHubAsset,
    readme_asset: GitHubAsset,
    license_asset: GitHubAsset,
    notices_asset: GitHubAsset,
    checksum: GitHubAsset,
    checksum_signature: GitHubAsset,
}

#[derive(Debug, Deserialize)]
pub(super) struct GitHubRelease {
    tag_name: String,
    html_url: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

pub(super) fn fetch_latest_release() -> Result<GitHubRelease> {
    let feed_url = release_feed_url();
    let allow_loopback = std::env::var("FLISTWALKER_UPDATE_FEED_URL")
        .ok()
        .is_some_and(|value| !value.trim().is_empty());
    let body = super::staging::fetch_release_json(&feed_url, allow_loopback)
        .context("failed to query latest release")?;
    serde_json::from_slice(&body).context("failed to parse latest release response")
}

pub(super) fn parse_version(text: &str) -> Result<Version> {
    Version::parse(text.trim_start_matches('v'))
        .with_context(|| format!("invalid semver version: {text}"))
}

pub(super) fn should_skip_update_prompt(
    target_version: &str,
    skipped_version: Option<&str>,
) -> bool {
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

pub(super) fn current_platform_target(version: &Version) -> Result<Option<PlatformReleaseTarget>> {
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
            support: UpdateSupport::ManualOnly {
                message: "macOS の自動更新は未対応です。GitHub Releases から手動更新してください。"
                    .to_string(),
            },
        }));
    }
    #[allow(unreachable_code)]
    Ok(None)
}

pub(super) fn resolve_update_candidate_from_release(
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
    let mut matches = release.assets.iter().filter(|asset| asset.name == name);
    let asset = matches
        .next()
        .cloned()
        .with_context(|| format!("release asset missing: {name}"))?;
    if matches.next().is_some() {
        anyhow::bail!("duplicate release asset: {name}");
    }
    Ok(asset)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(assets.checksum.name, "SHA256SUMS");
        assert_eq!(assets.checksum_signature.name, CHECKSUM_SIGNATURE_NAME);
    }

    #[test]
    fn tc157_candidate_resolution_rejects_duplicate_required_asset_name() {
        let mut release = test_release("v0.13.1");
        let target = current_platform_target(&Version::new(0, 13, 1))
            .expect("platform target")
            .expect("target");
        release.assets.push(GitHubAsset {
            name: target.asset_name,
            browser_download_url: "https://example.invalid/duplicate".to_string(),
        });

        let err = resolve_update_candidate_from_release(&Version::new(0, 13, 0), &release)
            .expect_err("duplicate required release asset must fail closed");

        assert!(
            err.to_string().contains("duplicate"),
            "unexpected error: {err}"
        );
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

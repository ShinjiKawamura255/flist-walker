use crate::updater::{current_version_string, StagedUpdatePaths, UpdateCandidate};
use anyhow::{bail, Context, Result};
use rand_core::{OsRng, RngCore};
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

#[cfg(all(unix, not(target_os = "macos")))]
use std::os::unix::fs::PermissionsExt;

const UPDATE_TEMP_DIR_RANDOM_BYTES: usize = 16;
const UPDATE_TEMP_DIR_MAX_ATTEMPTS: usize = 32;

pub(super) fn stage_update_assets(candidate: &UpdateCandidate) -> Result<StagedUpdatePaths> {
    let temp_dir = unique_update_temp_dir()?;
    let paths = StagedUpdatePaths::new(temp_dir, candidate);
    download_to_path(&candidate.asset_url, &paths.staged_path)?;
    download_to_path(&candidate.readme_asset_url, &paths.staged_readme_path)?;
    download_to_path(&candidate.license_asset_url, &paths.staged_license_path)?;
    download_to_path(&candidate.notices_asset_url, &paths.staged_notices_path)?;
    download_to_path(&candidate.checksum_url, &paths.checksum_path)?;
    download_to_path(&candidate.checksum_signature_url, &paths.signature_path)?;
    Ok(paths)
}

pub(super) fn make_staged_binary_executable(staged_path: &Path) -> Result<()> {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let mut perms = fs::metadata(staged_path)
            .with_context(|| format!("failed to read metadata {}", staged_path.display()))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(staged_path, perms)
            .with_context(|| format!("failed to chmod {}", staged_path.display()))?;
    }
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    {
        let _ = staged_path;
    }
    Ok(())
}

fn unique_update_temp_dir() -> Result<PathBuf> {
    let mut rng = OsRng;
    unique_update_temp_dir_in(&std::env::temp_dir(), || {
        let mut bytes = [0u8; UPDATE_TEMP_DIR_RANDOM_BYTES];
        rng.fill_bytes(&mut bytes);
        bytes
    })
}

#[cfg(test)]
pub(super) fn test_unique_update_temp_dir() -> Result<PathBuf> {
    unique_update_temp_dir()
}

fn unique_update_temp_dir_in(
    base_dir: &Path,
    mut random_bytes: impl FnMut() -> [u8; UPDATE_TEMP_DIR_RANDOM_BYTES],
) -> Result<PathBuf> {
    for _ in 0..UPDATE_TEMP_DIR_MAX_ATTEMPTS {
        let suffix = hex_bytes(&random_bytes());
        let dir = base_dir.join(format!("flistwalker-update-{suffix}"));
        match fs::create_dir(&dir) {
            Ok(()) => {
                set_private_dir_permissions(&dir)?;
                return Ok(dir);
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => {
                return Err(err).with_context(|| format!("failed to create {}", dir.display()));
            }
        }
    }
    bail!(
        "failed to create unique update temp directory after {} attempts",
        UPDATE_TEMP_DIR_MAX_ATTEMPTS
    )
}

#[cfg(all(unix, not(target_os = "macos")))]
fn set_private_dir_permissions(path: &Path) -> Result<()> {
    let mut perms = fs::metadata(path)
        .with_context(|| format!("failed to read metadata {}", path.display()))?
        .permissions();
    perms.set_mode(0o700);
    fs::set_permissions(path, perms).with_context(|| format!("failed to chmod {}", path.display()))
}

#[cfg(not(all(unix, not(target_os = "macos"))))]
fn set_private_dir_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

fn hex_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

pub(super) fn open_new_staged_file(path: &Path) -> Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("failed to create new staged file {}", path.display()))
}

#[cfg(any(test, not(target_os = "macos")))]
pub(super) fn write_new_staged_file(path: &Path, contents: &str) -> Result<()> {
    use std::io::Write as _;
    let mut file = open_new_staged_file(path)?;
    file.write_all(contents.as_bytes())
        .with_context(|| format!("failed to write {}", path.display()))
}

fn download_to_path(url: &str, out: &Path) -> Result<()> {
    let response = ureq::get(url)
        .set(
            "User-Agent",
            &format!("flistwalker/{}", current_version_string()),
        )
        .call()
        .map_err(|err| anyhow::anyhow!("failed to download {url}: {err}"))?;
    let mut reader = response.into_reader();
    let mut file = open_new_staged_file(out)?;
    std::io::copy(&mut reader, &mut file)
        .with_context(|| format!("failed to write {}", out.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_update_temp_dir_in_retries_collisions_and_never_reuses_existing_dir() {
        let base = unique_update_temp_dir().expect("base temp dir");
        let first = [0x11; UPDATE_TEMP_DIR_RANDOM_BYTES];
        let second = [0x22; UPDATE_TEMP_DIR_RANDOM_BYTES];
        let first_path = base.join(format!("flistwalker-update-{}", hex_bytes(&first)));
        let second_path = base.join(format!("flistwalker-update-{}", hex_bytes(&second)));
        fs::create_dir(&first_path).expect("precreate collision dir");
        let mut attempts = [first, second].into_iter();

        let actual = unique_update_temp_dir_in(&base, || attempts.next().expect("random bytes"))
            .expect("create after collision");

        assert_eq!(actual, second_path);
        assert!(first_path.is_dir(), "existing collision dir must remain");
        assert!(second_path.is_dir(), "second attempt should create dir");

        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let mode = fs::metadata(&second_path)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o700);
        }

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn unique_update_temp_dir_in_fails_after_repeated_collisions() {
        let base = unique_update_temp_dir().expect("base temp dir");
        let repeated = [0x33; UPDATE_TEMP_DIR_RANDOM_BYTES];
        let collision_path = base.join(format!("flistwalker-update-{}", hex_bytes(&repeated)));
        fs::create_dir(&collision_path).expect("precreate collision dir");

        let err = unique_update_temp_dir_in(&base, || repeated)
            .expect_err("repeated collisions should fail");

        assert!(
            err.to_string().contains("after 32 attempts"),
            "unexpected error: {err}"
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn open_new_staged_file_refuses_existing_file() {
        let dir = unique_update_temp_dir().expect("temp dir");
        let path = dir.join("existing.bin");
        fs::write(&path, b"existing").expect("write existing file");

        let err = open_new_staged_file(&path).expect_err("existing file must not be overwritten");

        assert!(
            err.to_string().contains("failed to create new staged file"),
            "unexpected error: {err}"
        );
        assert_eq!(fs::read(&path).expect("read existing"), b"existing");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_new_staged_file_refuses_existing_file() {
        let dir = unique_update_temp_dir().expect("temp dir");
        let path = dir.join("apply-update.sh");
        fs::write(&path, "existing").expect("write existing script");

        let err = write_new_staged_file(&path, "replacement")
            .expect_err("existing helper script must not be overwritten");

        assert!(
            err.to_string().contains("failed to create new staged file"),
            "unexpected error: {err}"
        );
        assert_eq!(
            fs::read_to_string(&path).expect("read existing"),
            "existing"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}

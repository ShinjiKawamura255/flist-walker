use crate::update_security;
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Cursor, Read};
use std::path::Path;

pub(super) fn verify_checksum_manifest_signature(
    checksum_file: &Path,
    signature_file: &Path,
) -> Result<()> {
    let message = fs::read(checksum_file)
        .with_context(|| format!("failed to read {}", checksum_file.display()))?;
    let signature = fs::read(signature_file)
        .with_context(|| format!("failed to read {}", signature_file.display()))?;
    update_security::verify_embedded_signature(&message, &signature)
        .context("failed to verify update checksum signature")
}

#[cfg(test)]
pub(super) fn verify_download(
    downloaded_file: &Path,
    checksum_file: &Path,
    asset_name: &str,
) -> Result<()> {
    verify_download_with_cancel(downloaded_file, checksum_file, asset_name, &|| false)
}

pub(super) fn verify_download_with_cancel(
    downloaded_file: &Path,
    checksum_file: &Path,
    asset_name: &str,
    should_cancel: &dyn Fn() -> bool,
) -> Result<()> {
    let checksums = parse_sha256sums_file(checksum_file)?;
    let expected = checksums
        .get(asset_name)
        .with_context(|| format!("missing checksum for {asset_name}"))?;
    let actual = sha256_file_with_cancel(downloaded_file, should_cancel)?;
    if &actual != expected {
        bail!("checksum mismatch for {asset_name}: expected {expected}, got {actual}");
    }
    Ok(())
}

fn parse_sha256sums_file(path: &Path) -> Result<HashMap<String, String>> {
    let file =
        fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    parse_sha256sums_reader(BufReader::new(file), &path.display().to_string())
}

pub(super) fn parse_sha256sums_bytes(bytes: &[u8]) -> Result<HashMap<String, String>> {
    parse_sha256sums_reader(BufReader::new(Cursor::new(bytes)), "checksum manifest")
}

fn parse_sha256sums_reader(reader: impl BufRead, source: &str) -> Result<HashMap<String, String>> {
    let mut out = HashMap::new();
    for (line_index, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("failed to read {source}"))?;
        let (hash, name) = parse_checksum_line(&line)
            .with_context(|| format!("invalid checksum manifest line {}", line_index + 1))?;
        if out.insert(name.to_string(), hash.to_string()).is_some() {
            bail!("duplicate checksum manifest filename: {name}");
        }
    }
    if out.is_empty() {
        bail!("checksum manifest must contain at least one entry");
    }
    Ok(out)
}

fn parse_checksum_line(line: &str) -> Result<(&str, &str)> {
    if line.len() < 67 || !line.is_ascii() {
        bail!("checksum line must be ASCII SHA-256 plus filename");
    }
    let hash = &line[..64];
    if !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("checksum digest must contain exactly 64 hexadecimal digits");
    }
    let separator = &line.as_bytes()[64..66];
    if separator != b"  " {
        bail!("checksum digest and filename must use the two-space text separator");
    }
    let name = &line[66..];
    // This allowlist prevents a signed manifest from naming arbitrary files;
    // both prefixes are required because releases contain GUI and CLI assets.
    let is_allowed_release_asset = name.starts_with("FlistWalker-") || name.starts_with("fw-");
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.bytes().any(|byte| byte.is_ascii_whitespace())
        || name.contains('/')
        || name.contains('\\')
        || !is_allowed_release_asset
    {
        bail!("checksum filename is not an allowed release asset basename");
    }
    Ok((hash, name))
}

#[cfg(test)]
fn sha256_file(path: &Path) -> Result<String> {
    sha256_file_with_cancel(path, &|| false)
}

fn sha256_file_with_cancel(path: &Path, should_cancel: &dyn Fn() -> bool) -> Result<String> {
    let mut file =
        fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        if should_cancel() {
            bail!("update canceled");
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update_security::{self, CHECKSUM_SIGNATURE_NAME};
    use crate::updater::staging;

    const TEST_SIGNING_KEY_HEX: &str =
        "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";
    const TEST_PUBLIC_KEY_HEX: &str =
        "79b5562e8fe654f94078b112e8a98ba7901f853ae695bed7e0e3910bad049664";

    #[test]
    fn tc188_download_verification_checks_cancel_between_hash_chunks() {
        let dir = staging::test_unique_update_temp_dir().expect("temp dir");
        let asset_name = "FlistWalker-1.0.0-linux-x86_64";
        let file_path = dir.join(asset_name);
        let sums_path = dir.join("SHA256SUMS");
        let contents = vec![b'x'; 32 * 1024];
        fs::write(&file_path, &contents).expect("write sample");
        let hash = sha256_file(&file_path).expect("hash");
        fs::write(&sums_path, format!("{hash}  {asset_name}\n")).expect("write sums");
        let calls = std::cell::Cell::new(0usize);

        let error = verify_download_with_cancel(&file_path, &sums_path, asset_name, &|| {
            calls.set(calls.get() + 1);
            calls.get() > 1
        })
        .expect_err("verification must observe cancellation");

        assert!(error.to_string().contains("update canceled"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn parse_checksum_line_supports_sha256sum_format() {
        let digest = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        let line = format!("{digest}  FlistWalker-0.12.3-linux-x86_64");
        let (hash, name) = parse_checksum_line(&line).expect("checksum");
        assert_eq!(hash, digest);
        assert_eq!(name, "FlistWalker-0.12.3-linux-x86_64");
    }

    #[test]
    fn tc194_regression_manifest_accepts_cli_asset_family() {
        let digest = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        let manifest = format!(
            "{digest}  FlistWalker-0.24.3-windows-x86_64.exe\n{digest}  fw-0.24.3-windows-x86_64.exe\n"
        );

        let checksums = parse_sha256sums_bytes(manifest.as_bytes()).expect("checksum manifest");

        assert_eq!(
            checksums.get("FlistWalker-0.24.3-windows-x86_64.exe"),
            Some(&digest.to_string())
        );
        assert_eq!(
            checksums.get("fw-0.24.3-windows-x86_64.exe"),
            Some(&digest.to_string())
        );
    }

    #[test]
    fn tc194_regression_manifest_rejects_nearby_cli_prefix() {
        let digest = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        let line = format!("{digest}  fwx-0.24.3-windows-x86_64.exe");

        assert!(parse_checksum_line(&line).is_err());
    }

    #[test]
    fn tc157_manifest_rejects_duplicate_required_filename() {
        let dir = staging::test_unique_update_temp_dir().expect("temp dir");
        let sums_path = dir.join("SHA256SUMS");
        let digest = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        fs::write(
            &sums_path,
            format!("{digest}  FlistWalker-1.0.0-linux-x86_64\n{digest}  FlistWalker-1.0.0-linux-x86_64\n"),
        )
        .expect("write sums");

        let err = parse_sha256sums_file(&sums_path)
            .expect_err("duplicate manifest filename must fail closed");
        assert!(
            err.to_string().contains("duplicate"),
            "unexpected error: {err}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn tc157_manifest_rejects_invalid_digest_and_unsafe_filename() {
        for line in [
            "abc123  FlistWalker-1.0.0-linux-x86_64\n",
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824 *FlistWalker-1.0.0-linux-x86_64\n",
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824  ../FlistWalker-1.0.0-linux-x86_64\n",
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824  unrelated.bin\n",
        ] {
            let dir = staging::test_unique_update_temp_dir().expect("temp dir");
            let sums_path = dir.join("SHA256SUMS");
            fs::write(&sums_path, line).expect("write sums");

            assert!(
                parse_sha256sums_file(&sums_path).is_err(),
                "invalid manifest line must fail closed: {line:?}"
            );

            let _ = fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn checksum_verification_detects_match() {
        let dir = staging::test_unique_update_temp_dir().expect("temp dir");
        let asset_name = "FlistWalker-1.0.0-linux-x86_64";
        let file_path = dir.join(asset_name);
        let sums_path = dir.join("SHA256SUMS");
        fs::write(&file_path, b"hello").expect("write sample");
        fs::write(
            &sums_path,
            format!(
                "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824  {asset_name}\n"
            ),
        )
        .expect("write sums");

        verify_download(&file_path, &sums_path, asset_name).expect("checksum match");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn checksum_verification_detects_sidecar_match() {
        let dir = staging::test_unique_update_temp_dir().expect("temp dir");
        let asset_name = "FlistWalker-1.0.0-linux-x86_64.LICENSE.txt";
        let file_path = dir.join(asset_name);
        let sums_path = dir.join("SHA256SUMS");
        fs::write(&file_path, b"license text").expect("write sample");
        let hash = sha256_file(&file_path).expect("hash");
        fs::write(&sums_path, format!("{hash}  {asset_name}\n")).expect("write sums");

        verify_download(&file_path, &sums_path, asset_name).expect("checksum match");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn checksum_manifest_signature_verification_detects_match() {
        let dir = staging::test_unique_update_temp_dir().expect("temp dir");
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
        let dir = staging::test_unique_update_temp_dir().expect("temp dir");
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
}

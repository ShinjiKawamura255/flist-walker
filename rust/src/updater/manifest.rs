use crate::update_security;
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read};
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

pub(super) fn verify_download(
    downloaded_file: &Path,
    checksum_file: &Path,
    asset_name: &str,
) -> Result<()> {
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
    fn parse_checksum_line_supports_sha256sum_format() {
        let (hash, name) =
            parse_checksum_line("abc123  FlistWalker-0.12.3-linux-x86_64").expect("checksum");
        assert_eq!(hash, "abc123");
        assert_eq!(name, "FlistWalker-0.12.3-linux-x86_64");
    }

    #[test]
    fn checksum_verification_detects_match() {
        let dir = staging::test_unique_update_temp_dir().expect("temp dir");
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
        let dir = staging::test_unique_update_temp_dir().expect("temp dir");
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

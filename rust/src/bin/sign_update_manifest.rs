use anyhow::{Context, Result};
use flist_walker::update_security::{
    embedded_public_key_hex, public_key_hex_from_signing_key, sign_message, verify_signature,
    CHECKSUM_SIGNATURE_NAME, UPDATE_PUBLIC_KEY_ENV, UPDATE_SIGNING_KEY_ENV,
};
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() -> Result<()> {
    let mut args = env::args_os().skip(1);
    let manifest_path = args
        .next()
        .map(PathBuf::from)
        .context("usage: sign_update_manifest <manifest-path> [signature-path]")?;
    let signature_path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest_path.with_file_name(CHECKSUM_SIGNATURE_NAME));
    if args.next().is_some() {
        anyhow::bail!("usage: sign_update_manifest <manifest-path> [signature-path]");
    }

    let message = fs::read(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let signing_key = env::var(UPDATE_SIGNING_KEY_ENV)
        .with_context(|| format!("{UPDATE_SIGNING_KEY_ENV} is not set"))?;
    let configured_public_key = env::var(UPDATE_PUBLIC_KEY_ENV)
        .with_context(|| format!("{UPDATE_PUBLIC_KEY_ENV} is not set"))?;
    let derived_public_key = public_key_hex_from_signing_key(&signing_key)?;
    if !derived_public_key.eq_ignore_ascii_case(configured_public_key.trim()) {
        anyhow::bail!(
            "{UPDATE_PUBLIC_KEY_ENV} does not match the public key derived from {UPDATE_SIGNING_KEY_ENV}"
        );
    }
    let embedded_public_key = embedded_public_key_hex()
        .context("update public key is not embedded in sign_update_manifest")?;
    if !derived_public_key.eq_ignore_ascii_case(embedded_public_key.trim()) {
        anyhow::bail!("embedded update public key does not match the signing key");
    }

    let signature = sign_message(&message, &signing_key)?;
    verify_signature(&message, &signature, &configured_public_key)?;
    verify_signature(&message, &signature, embedded_public_key)?;
    fs::write(&signature_path, signature)
        .with_context(|| format!("failed to write {}", signature_path.display()))?;
    println!("{}", signature_path.display());
    Ok(())
}

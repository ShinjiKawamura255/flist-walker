use anyhow::{Context, Result};
use flist_walker::update_security::{sign_with_env_key, CHECKSUM_SIGNATURE_NAME};
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
    let signature = sign_with_env_key(&message)?;
    fs::write(&signature_path, signature)
        .with_context(|| format!("failed to write {}", signature_path.display()))?;
    println!("{}", signature_path.display());
    Ok(())
}

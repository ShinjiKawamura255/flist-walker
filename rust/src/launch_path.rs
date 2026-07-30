use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub(crate) fn resolve_root(root: &Path) -> Result<PathBuf> {
    let root = root
        .canonicalize()
        .with_context(|| format!("failed to canonicalize root: {}", root.display()))?;
    if !root.is_dir() {
        anyhow::bail!("root is not a directory: {}", root.display());
    }
    Ok(root)
}

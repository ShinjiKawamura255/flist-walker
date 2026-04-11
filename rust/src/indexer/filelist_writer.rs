use crate::fs_atomic::write_text_atomic;
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::fs::File;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use super::filelist_reader::{looks_like_windows_absolute_path, strip_wrapping_quotes};

pub fn build_filelist_text(entries: &[PathBuf], root: &Path) -> String {
    build_filelist_text_cancellable(entries, root, &|| false)
        .expect("build_filelist_text without cancellation should not fail")
}

pub fn build_filelist_text_cancellable<C>(
    entries: &[PathBuf],
    root: &Path,
    should_cancel: &C,
) -> Result<String>
where
    C: Fn() -> bool,
{
    let root_lexical = root.to_path_buf();
    let root_canonical = root.canonicalize().ok();
    let mut seen = HashSet::new();
    let mut lines = Vec::new();
    for entry in entries {
        if should_cancel() {
            anyhow::bail!("filelist creation canceled");
        }
        let line = filelist_line_for_entry(entry, &root_lexical, root_canonical.as_deref());
        if seen.insert(line.clone()) {
            lines.push(line);
        }
    }
    Ok(if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    })
}

pub(super) fn filelist_line_for_entry(
    entry: &Path,
    root_lexical: &Path,
    root_canonical: Option<&Path>,
) -> String {
    if let Ok(relative) = entry.strip_prefix(root_lexical) {
        return normalize_relative_lexically(relative)
            .to_string_lossy()
            .to_string();
    }
    if let Some(root) = root_canonical {
        if let Ok(relative) = entry.strip_prefix(root) {
            return normalize_relative_lexically(relative)
                .to_string_lossy()
                .to_string();
        }
    }

    if let Some(root) = root_canonical {
        if let Ok(canonical_entry) = entry.canonicalize() {
            if let Ok(relative) = canonical_entry.strip_prefix(root) {
                return relative.to_string_lossy().to_string();
            }
            return canonical_entry.to_string_lossy().to_string();
        }
    }

    entry.to_string_lossy().to_string()
}

fn normalize_relative_lexically(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                let popped = out.pop();
                if !popped {
                    out.push(component.as_os_str());
                }
            }
            Component::Normal(segment) => out.push(segment),
            Component::RootDir | Component::Prefix(_) => out.push(component.as_os_str()),
        }
    }
    out
}

pub(crate) fn filelist_modified_time(path: &Path) -> Option<SystemTime> {
    fs::metadata(path)
        .ok()
        .and_then(|meta| meta.modified().ok())
}

fn write_text_to_path(out: &Path, text: &str) -> Result<()> {
    write_text_atomic(out, text)
        .map_err(|err| annotate_write_target_error(out, err))
        .with_context(|| format!("failed to write {}", out.display()))
}

pub(crate) fn annotate_write_target_error(out: &Path, err: std::io::Error) -> anyhow::Error {
    if err.kind() == std::io::ErrorKind::PermissionDenied {
        return anyhow::anyhow!(
            "permission denied while writing {}. destination directory may be protected (for example C:\\ root/UAC), existing FileList.txt may be read-only, or another process may be locking the file. original error: {}",
            out.display(),
            err
        );
    }
    anyhow::Error::new(err)
}

pub(crate) fn visit_ancestor_directories(path: &Path, mut visit: impl FnMut(&Path) -> bool) {
    let mut current = path.parent();
    while let Some(ancestor) = current {
        if !visit(ancestor) {
            break;
        }
        current = ancestor.parent();
    }
}

fn find_all_filelists_in_directory(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut matches = Vec::new();
    let mut seen = HashSet::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if !name.eq_ignore_ascii_case("filelist.txt") {
            continue;
        }
        if !entry.file_type()?.is_file() {
            continue;
        }
        if seen.insert(path.clone()) {
            matches.push(path);
        }
    }
    matches.sort_by_key(|path| path.to_string_lossy().to_ascii_lowercase());
    Ok(matches)
}

pub(crate) fn normalize_filelist_entry_for_text_compare(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }

    let raw = strip_wrapping_quotes(trimmed);
    if raw.is_empty() {
        return None;
    }

    let normalized = if looks_like_windows_absolute_path(raw) || raw.starts_with("//") {
        raw.replace('\\', "/").to_ascii_lowercase()
    } else {
        let lexical = normalize_relative_lexically(Path::new(&raw.replace('\\', "/")));
        lexical.to_string_lossy().replace('\\', "/")
    };
    let normalized = normalized.trim_start_matches("./").to_string();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn child_filelist_reference_keys(parent_dir: &Path, child_filelist: &Path) -> HashSet<String> {
    let mut keys = HashSet::new();
    let parent_canonical = parent_dir.canonicalize().ok();
    let relative = filelist_line_for_entry(child_filelist, parent_dir, parent_canonical.as_deref());
    if let Some(key) = normalize_filelist_entry_for_text_compare(&relative) {
        keys.insert(key);
    }
    if let Some(key) =
        normalize_filelist_entry_for_text_compare(child_filelist.to_string_lossy().as_ref())
    {
        keys.insert(key);
    }
    if let Ok(canonical) = child_filelist.canonicalize() {
        if let Some(key) =
            normalize_filelist_entry_for_text_compare(canonical.to_string_lossy().as_ref())
        {
            keys.insert(key);
        }
    }
    keys
}

fn restore_file_mtime(
    path: &Path,
    modified: SystemTime,
    accessed: Option<SystemTime>,
) -> std::io::Result<()> {
    let file = File::options().write(true).open(path)?;
    let times = match accessed {
        Some(accessed) => fs::FileTimes::new()
            .set_accessed(accessed)
            .set_modified(modified),
        None => fs::FileTimes::new().set_modified(modified),
    };
    file.set_times(times)
}

fn append_child_filelist_to_parent_filelist_if_missing(
    parent_filelist: &Path,
    child_filelist: &Path,
    should_cancel: &impl Fn() -> bool,
) -> std::io::Result<()> {
    if should_cancel() {
        return Err(std::io::Error::other("filelist creation canceled"));
    }
    let Some(parent_dir) = parent_filelist.parent() else {
        return Ok(());
    };
    let metadata = fs::metadata(parent_filelist)?;
    let modified = metadata.modified()?;
    let accessed = metadata.accessed().ok();
    let mut content = fs::read_to_string(parent_filelist)?;
    let child_keys = child_filelist_reference_keys(parent_dir, child_filelist);
    if content
        .lines()
        .filter_map(normalize_filelist_entry_for_text_compare)
        .any(|line| child_keys.contains(&line))
    {
        return Ok(());
    }
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    if should_cancel() {
        return Err(std::io::Error::other("filelist creation canceled"));
    }
    content.push_str(&filelist_line_for_entry(
        child_filelist,
        parent_dir,
        parent_dir.canonicalize().ok().as_deref(),
    ));
    content.push('\n');

    if should_cancel() {
        return Err(std::io::Error::other("filelist creation canceled"));
    }
    write_text_to_path(parent_filelist, &content)
        .map_err(|err| std::io::Error::other(err.to_string()))?;
    restore_file_mtime(parent_filelist, modified, accessed)?;
    Ok(())
}

fn propagate_child_filelist_to_ancestor_filelists(
    child_filelist: &Path,
    should_cancel: &impl Fn() -> bool,
) -> Result<()> {
    let Some(root_dir) = child_filelist.parent() else {
        return Ok(());
    };
    if should_cancel() {
        anyhow::bail!("filelist creation canceled");
    }
    visit_ancestor_directories(root_dir, |ancestor_dir| {
        if should_cancel() {
            return false;
        }
        let parent_filelists = match find_all_filelists_in_directory(ancestor_dir) {
            Ok(parent_filelists) => parent_filelists,
            Err(_) => return false,
        };
        for parent_filelist in parent_filelists {
            if append_child_filelist_to_parent_filelist_if_missing(
                &parent_filelist,
                child_filelist,
                should_cancel,
            )
            .is_err()
            {
                return false;
            }
        }
        true
    });
    if should_cancel() {
        anyhow::bail!("filelist creation canceled");
    }
    Ok(())
}

pub fn has_ancestor_filelists(root: &Path) -> bool {
    let mut found = false;
    visit_ancestor_directories(root, |ancestor_dir| {
        match find_all_filelists_in_directory(ancestor_dir) {
            Ok(parent_filelists) => {
                if parent_filelists.is_empty() {
                    true
                } else {
                    found = true;
                    false
                }
            }
            Err(_) => false,
        }
    });
    found
}

pub fn write_filelist(
    root: &Path,
    entries: &[PathBuf],
    filename: &str,
    propagate_to_ancestors: bool,
) -> Result<PathBuf> {
    write_filelist_cancellable(root, entries, filename, propagate_to_ancestors, &|| false)
}

pub fn write_filelist_cancellable<C>(
    root: &Path,
    entries: &[PathBuf],
    filename: &str,
    propagate_to_ancestors: bool,
    should_cancel: &C,
) -> Result<PathBuf>
where
    C: Fn() -> bool,
{
    let out = root.join(filename);
    let text = build_filelist_text_cancellable(entries, root, should_cancel)?;
    if should_cancel() {
        anyhow::bail!("filelist creation canceled");
    }
    write_text_to_path(&out, &text)?;
    if propagate_to_ancestors {
        propagate_child_filelist_to_ancestor_filelists(&out, should_cancel)?;
    }

    Ok(out)
}

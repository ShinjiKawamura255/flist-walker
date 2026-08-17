use crate::fs_atomic::write_bytes_atomic;
use anyhow::{Context, Result};
use std::any::Any;
use std::collections::HashSet;
use std::fs;
use std::fs::File;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use super::filelist_reader::{
    looks_like_windows_absolute_path, read_filelist_text_strict, strip_wrapping_quotes,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileListWriteTargetKind {
    Root,
    Ancestor,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FileListWriteOptions {
    pub allow_root_overwrite: bool,
    pub propagate_to_ancestors: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileListWriteFailure {
    pub path: PathBuf,
    pub error: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileListWriteStatus {
    Completed,
    Canceled,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileListWriteReport {
    pub status: FileListWriteStatus,
    pub root_target: PathBuf,
    pub committed: Vec<PathBuf>,
    pub failed: Vec<FileListWriteFailure>,
    pub rolled_back: Vec<PathBuf>,
    pub rollback_failed: Vec<FileListWriteFailure>,
}

impl FileListWriteReport {
    fn completed(root_target: PathBuf) -> Self {
        Self {
            status: FileListWriteStatus::Completed,
            root_target,
            committed: Vec::new(),
            failed: Vec::new(),
            rolled_back: Vec::new(),
            rollback_failed: Vec::new(),
        }
    }

    fn preflight_failed(root_target: PathBuf, failure_path: PathBuf, error: impl ToString) -> Self {
        Self {
            status: FileListWriteStatus::Failed,
            root_target: root_target.clone(),
            committed: Vec::new(),
            failed: vec![FileListWriteFailure {
                path: failure_path,
                error: error.to_string(),
            }],
            rolled_back: Vec::new(),
            rollback_failed: Vec::new(),
        }
    }

    fn canceled(root_target: PathBuf) -> Self {
        Self {
            status: FileListWriteStatus::Canceled,
            root_target,
            committed: Vec::new(),
            failed: Vec::new(),
            rolled_back: Vec::new(),
            rollback_failed: Vec::new(),
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self.status {
            FileListWriteStatus::Completed => 0,
            FileListWriteStatus::Canceled
                if self.failed.is_empty() && self.rollback_failed.is_empty() =>
            {
                130
            }
            FileListWriteStatus::Canceled | FileListWriteStatus::Failed => 1,
        }
    }

    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        for failure in self.failed.iter().chain(&self.rollback_failed) {
            parts.push(format!("{}: {}", failure.path.display(), failure.error));
        }
        if parts.is_empty() && self.status == FileListWriteStatus::Canceled {
            "filelist creation canceled".to_string()
        } else if parts.is_empty() {
            "filelist write failed".to_string()
        } else {
            parts.join("; ")
        }
    }
}

#[derive(Debug, Clone)]
struct PriorFileState {
    bytes: Option<Vec<u8>>,
    permissions: Option<fs::Permissions>,
    modified: Option<SystemTime>,
    accessed: Option<SystemTime>,
}

#[derive(Debug, Clone)]
pub struct FileListWriteTarget {
    pub path: PathBuf,
    pub kind: FileListWriteTargetKind,
    content: Vec<u8>,
    prior: PriorFileState,
    preserve_mtime_on_success: bool,
}

#[derive(Debug, Clone)]
pub struct FileListWritePlan {
    root_target: PathBuf,
    targets: Vec<FileListWriteTarget>,
}

impl FileListWritePlan {
    pub fn root_target(&self) -> &Path {
        &self.root_target
    }

    pub fn targets(&self) -> &[FileListWriteTarget] {
        &self.targets
    }
}

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

#[cfg(test)]
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
        if seen.insert(path.clone()) {
            matches.push(path);
        }
    }
    matches.sort_by(|left, right| {
        filelist_name_precedence(left)
            .cmp(&filelist_name_precedence(right))
            .then_with(|| left.to_string_lossy().cmp(&right.to_string_lossy()))
    });
    Ok(matches)
}

fn filelist_name_precedence(path: &Path) -> (u8, String) {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let rank = match name {
        "FileList.txt" => 0,
        "filelist.txt" => 1,
        _ => 2,
    };
    (rank, name.to_ascii_lowercase())
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

fn parent_filelist_contains_child_reference(
    parent_filelist: &Path,
    child_filelist: &Path,
) -> std::io::Result<bool> {
    let content = read_filelist_text_strict(parent_filelist)?;
    Ok(parent_filelist_content_contains_child_reference(
        parent_filelist,
        child_filelist,
        &content,
    ))
}

fn parent_filelist_content_contains_child_reference(
    parent_filelist: &Path,
    child_filelist: &Path,
    content: &str,
) -> bool {
    let Some(parent_dir) = parent_filelist.parent() else {
        return false;
    };
    let child_keys = child_filelist_reference_keys(parent_dir, child_filelist);
    content
        .lines()
        .filter_map(normalize_filelist_entry_for_text_compare)
        .any(|line| child_keys.contains(&line))
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

pub fn ancestor_filelist_propagation_needed(root: &Path) -> bool {
    let child_filelist = root.join("FileList.txt");
    let mut needs_confirmation = false;
    visit_ancestor_directories(root, |ancestor_dir| {
        let parent_filelists = match find_all_filelists_in_directory(ancestor_dir) {
            Ok(parent_filelists) => parent_filelists,
            Err(_) => return false,
        };
        for parent_filelist in parent_filelists {
            match parent_filelist_contains_child_reference(&parent_filelist, &child_filelist) {
                Ok(true) => {}
                Ok(false) => {
                    needs_confirmation = true;
                    return false;
                }
                Err(_) => return false,
            }
        }
        true
    });
    needs_confirmation
}

/// Precompute every FileList replacement, including the prior bytes and
/// metadata required to restore already committed targets after a later error.
/// This function performs no writes.
pub fn plan_filelist_write(
    root: &Path,
    entries: &[PathBuf],
    options: FileListWriteOptions,
) -> std::result::Result<FileListWritePlan, Box<FileListWriteReport>> {
    plan_filelist_write_cancellable(root, entries, options, &|| false)
}

/// Build a write plan without mutating the filesystem. Cancellation during
/// entry serialization or ancestor discovery returns a clean cancellation
/// report; no replacement has started at that point.
pub fn plan_filelist_write_cancellable<C>(
    root: &Path,
    entries: &[PathBuf],
    options: FileListWriteOptions,
    should_cancel: &C,
) -> std::result::Result<FileListWritePlan, Box<FileListWriteReport>>
where
    C: Fn() -> bool,
{
    plan_filelist_write_cancellable_inner(root, entries, options, None, should_cancel)
}

#[cfg(test)]
pub(crate) fn plan_filelist_write_cancellable_with_ancestor_boundary<C>(
    root: &Path,
    entries: &[PathBuf],
    options: FileListWriteOptions,
    exclusive_ancestor_boundary: &Path,
    should_cancel: &C,
) -> std::result::Result<FileListWritePlan, Box<FileListWriteReport>>
where
    C: Fn() -> bool,
{
    plan_filelist_write_cancellable_inner(
        root,
        entries,
        options,
        Some(exclusive_ancestor_boundary),
        should_cancel,
    )
}

fn plan_filelist_write_cancellable_inner<C>(
    root: &Path,
    entries: &[PathBuf],
    options: FileListWriteOptions,
    exclusive_ancestor_boundary: Option<&Path>,
    should_cancel: &C,
) -> std::result::Result<FileListWritePlan, Box<FileListWriteReport>>
where
    C: Fn() -> bool,
{
    let fallback_target = root.join("FileList.txt");
    if should_cancel() {
        return Err(Box::new(FileListWriteReport::canceled(fallback_target)));
    }
    validate_filelist_directory(root).map_err(|error| {
        Box::new(FileListWriteReport::preflight_failed(
            fallback_target.clone(),
            root.to_path_buf(),
            error,
        ))
    })?;
    let root_target = find_all_filelists_in_directory(root)
        .map_err(|error| {
            Box::new(FileListWriteReport::preflight_failed(
                fallback_target.clone(),
                root.to_path_buf(),
                error,
            ))
        })?
        .into_iter()
        .next()
        .unwrap_or_else(|| fallback_target.clone());
    if fs::symlink_metadata(&root_target).is_ok() && !options.allow_root_overwrite {
        return Err(Box::new(FileListWriteReport::preflight_failed(
            root_target.clone(),
            root_target.clone(),
            format!(
                "refusing to overwrite existing FileList target {} without explicit consent",
                root_target.display()
            ),
        )));
    }

    let root_text =
        build_filelist_text_cancellable(entries, root, should_cancel).map_err(|error| {
            if should_cancel() {
                Box::new(FileListWriteReport::canceled(root_target.clone()))
            } else {
                Box::new(FileListWriteReport::preflight_failed(
                    root_target.clone(),
                    root_target.clone(),
                    error,
                ))
            }
        })?;
    let mut targets = vec![prepare_filelist_target(
        root_target.clone(),
        FileListWriteTargetKind::Root,
        root_text.into_bytes(),
        false,
    )
    .map_err(|error| {
        Box::new(FileListWriteReport::preflight_failed(
            root_target.clone(),
            root_target.clone(),
            error,
        ))
    })?];

    if options.propagate_to_ancestors {
        let mut ancestor = root.parent();
        while let Some(directory) = ancestor {
            // Regression guard: tests inject an exclusive fixture boundary so
            // ancestor discovery cannot observe or mutate a developer's real
            // FileList. Production callers pass no boundary and keep full traversal.
            if exclusive_ancestor_boundary.is_some_and(|boundary| directory == boundary) {
                break;
            }
            if should_cancel() {
                return Err(Box::new(FileListWriteReport::canceled(root_target.clone())));
            }
            let parent_filelists = find_all_filelists_in_directory(directory).map_err(|error| {
                Box::new(FileListWriteReport::preflight_failed(
                    root_target.clone(),
                    directory.to_path_buf(),
                    error,
                ))
            })?;
            for parent_filelist in parent_filelists {
                if should_cancel() {
                    return Err(Box::new(FileListWriteReport::canceled(root_target.clone())));
                }
                inspect_filelist_target(&parent_filelist).map_err(|error| {
                    Box::new(FileListWriteReport::preflight_failed(
                        root_target.clone(),
                        parent_filelist.clone(),
                        error,
                    ))
                })?;
                let mut content = read_filelist_text_strict(&parent_filelist).map_err(|error| {
                    Box::new(FileListWriteReport::preflight_failed(
                        root_target.clone(),
                        parent_filelist.clone(),
                        error,
                    ))
                })?;
                if should_cancel() {
                    return Err(Box::new(FileListWriteReport::canceled(root_target.clone())));
                }
                if !parent_filelist_content_contains_child_reference(
                    &parent_filelist,
                    &root_target,
                    &content,
                ) {
                    if !content.is_empty() && !content.ends_with('\n') {
                        content.push('\n');
                    }
                    content.push_str(&filelist_line_for_entry(&root_target, directory, None));
                    content.push('\n');
                    targets.push(
                        prepare_filelist_target(
                            parent_filelist.clone(),
                            FileListWriteTargetKind::Ancestor,
                            content.into_bytes(),
                            true,
                        )
                        .map_err(|error| {
                            Box::new(FileListWriteReport::preflight_failed(
                                root_target.clone(),
                                parent_filelist.clone(),
                                error,
                            ))
                        })?,
                    );
                }
            }
            ancestor = directory.parent();
        }
    }

    Ok(FileListWritePlan {
        root_target,
        targets,
    })
}

/// Execute a previously validated plan. Cancellation is sampled immediately
/// before every target replacement; any committed target is then restored in
/// reverse order before the report is returned.
pub fn execute_filelist_write_plan<C>(
    plan: &FileListWritePlan,
    should_cancel: &C,
) -> FileListWriteReport
where
    C: Fn() -> bool,
{
    execute_filelist_write_plan_with(plan, should_cancel, &mut |path, bytes| {
        write_bytes_atomic(path, bytes)
    })
}

pub(super) fn execute_filelist_write_plan_with<C, W>(
    plan: &FileListWritePlan,
    should_cancel: &C,
    replace: &mut W,
) -> FileListWriteReport
where
    C: Fn() -> bool,
    W: FnMut(&Path, &[u8]) -> std::io::Result<()>,
{
    let mut report = FileListWriteReport::completed(plan.root_target.clone());
    let mut committed_indexes = Vec::new();
    for (index, target) in plan.targets.iter().enumerate() {
        if should_cancel() {
            report.status = FileListWriteStatus::Canceled;
            rollback_committed_targets(&plan.targets, &committed_indexes, &mut report, replace);
            return report;
        }
        if let Err(error) = revalidate_filelist_target(target) {
            report.status = FileListWriteStatus::Failed;
            report.failed.push(FileListWriteFailure {
                path: target.path.clone(),
                error: error.to_string(),
            });
            rollback_committed_targets(&plan.targets, &committed_indexes, &mut report, replace);
            return report;
        }
        if should_cancel() {
            report.status = FileListWriteStatus::Canceled;
            rollback_committed_targets(&plan.targets, &committed_indexes, &mut report, replace);
            return report;
        }
        if let Err(error) = replace_filelist_target(replace, &target.path, &target.content) {
            report.status = FileListWriteStatus::Failed;
            report.failed.push(FileListWriteFailure {
                path: target.path.clone(),
                error: error.to_string(),
            });
            rollback_committed_targets(&plan.targets, &committed_indexes, &mut report, replace);
            return report;
        }
        report.committed.push(target.path.clone());
        committed_indexes.push(index);
        if let Err(error) = restore_success_metadata(target) {
            report.status = FileListWriteStatus::Failed;
            report.failed.push(FileListWriteFailure {
                path: target.path.clone(),
                error: error.to_string(),
            });
            rollback_committed_targets(&plan.targets, &committed_indexes, &mut report, replace);
            return report;
        }
    }
    report
}

fn replace_filelist_target<W>(replace: &mut W, path: &Path, bytes: &[u8]) -> std::io::Result<()>
where
    W: FnMut(&Path, &[u8]) -> std::io::Result<()>,
{
    catch_unwind(AssertUnwindSafe(|| replace(path, bytes))).map_err(replacement_panic_error)?
}

fn replacement_panic_error(payload: Box<dyn Any + Send>) -> std::io::Error {
    let detail = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .and_then(concise_panic_detail);
    let message = match detail {
        Some(detail) => format!("FileList replacement panicked: {detail}"),
        None => "FileList replacement panicked".to_string(),
    };
    std::io::Error::other(message)
}

fn concise_panic_detail(message: &str) -> Option<String> {
    const MAX_CHARS: usize = 160;

    let normalized = message.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return None;
    }
    let mut detail: String = normalized.chars().take(MAX_CHARS).collect();
    if normalized.chars().nth(MAX_CHARS).is_some() {
        detail.push('…');
    }
    Some(detail)
}

fn validate_filelist_directory(directory: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(directory).with_context(|| {
        format!(
            "failed to inspect FileList directory {}",
            directory.display()
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        anyhow::bail!("refusing unsafe FileList directory {}", directory.display());
    }
    if metadata.permissions().readonly() {
        anyhow::bail!(
            "refusing read-only FileList directory {}",
            directory.display()
        );
    }
    Ok(())
}

fn inspect_filelist_target(path: &Path) -> Result<Option<fs::Metadata>> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("FileList target has no parent: {}", path.display()))?;
    validate_filelist_directory(parent)?;
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                anyhow::bail!("refusing unsafe FileList target {}", path.display());
            }
            if metadata.permissions().readonly() {
                anyhow::bail!("refusing read-only FileList target {}", path.display());
            }
            Ok(Some(metadata))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn revalidate_filelist_target(target: &FileListWriteTarget) -> std::io::Result<()> {
    let metadata = inspect_filelist_target(&target.path)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    match (&target.prior.bytes, metadata) {
        (None, None) => Ok(()),
        (Some(expected), Some(_)) => {
            let actual = fs::read(&target.path)?;
            if actual == *expected {
                Ok(())
            } else {
                Err(std::io::Error::other(format!(
                    "FileList target changed after planning: {}",
                    target.path.display()
                )))
            }
        }
        (None, Some(_)) => Err(std::io::Error::other(format!(
            "FileList target appeared after planning: {}",
            target.path.display()
        ))),
        (Some(_), None) => Err(std::io::Error::other(format!(
            "FileList target disappeared after planning: {}",
            target.path.display()
        ))),
    }
}

fn prepare_filelist_target(
    path: PathBuf,
    kind: FileListWriteTargetKind,
    content: Vec<u8>,
    preserve_mtime_on_success: bool,
) -> Result<FileListWriteTarget> {
    let prior = match inspect_filelist_target(&path)? {
        Some(metadata) => {
            let accessed = match metadata.accessed() {
                Ok(time) => Some(time),
                Err(error) if error.kind() == std::io::ErrorKind::Unsupported => None,
                Err(error) => return Err(error.into()),
            };
            PriorFileState {
                bytes: Some(fs::read(&path).with_context(|| {
                    format!("failed to read prior FileList target {}", path.display())
                })?),
                permissions: Some(metadata.permissions()),
                modified: Some(metadata.modified()?),
                accessed,
            }
        }
        None => PriorFileState {
            bytes: None,
            permissions: None,
            modified: None,
            accessed: None,
        },
    };
    Ok(FileListWriteTarget {
        path,
        kind,
        content,
        prior,
        preserve_mtime_on_success,
    })
}

fn restore_success_metadata(target: &FileListWriteTarget) -> std::io::Result<()> {
    if let Some(permissions) = &target.prior.permissions {
        fs::set_permissions(&target.path, permissions.clone())?;
    }
    if target.preserve_mtime_on_success {
        restore_file_metadata(&target.path, &target.prior)?;
    }
    Ok(())
}

fn restore_file_metadata(path: &Path, prior: &PriorFileState) -> std::io::Result<()> {
    if let Some(permissions) = &prior.permissions {
        fs::set_permissions(path, permissions.clone())?;
    }
    let Some(modified) = prior.modified else {
        return Ok(());
    };
    let file = File::options().write(true).open(path)?;
    let times = match prior.accessed {
        Some(accessed) => fs::FileTimes::new()
            .set_accessed(accessed)
            .set_modified(modified),
        None => fs::FileTimes::new().set_modified(modified),
    };
    file.set_times(times)
}

fn rollback_committed_targets<W>(
    targets: &[FileListWriteTarget],
    committed_indexes: &[usize],
    report: &mut FileListWriteReport,
    replace: &mut W,
) where
    W: FnMut(&Path, &[u8]) -> std::io::Result<()>,
{
    for index in committed_indexes.iter().rev().copied() {
        let target = &targets[index];
        let result = match &target.prior.bytes {
            Some(bytes) => replace_filelist_target(replace, &target.path, bytes)
                .and_then(|_| restore_file_metadata(&target.path, &target.prior)),
            None => fs::remove_file(&target.path),
        };
        match result {
            Ok(()) => report.rolled_back.push(target.path.clone()),
            Err(error) => report.rollback_failed.push(FileListWriteFailure {
                path: target.path.clone(),
                error: error.to_string(),
            }),
        }
    }
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
    write_filelist_cancellable_inner(
        root,
        entries,
        filename,
        propagate_to_ancestors,
        None,
        should_cancel,
    )
}

#[cfg(test)]
pub(crate) fn write_filelist_cancellable_with_ancestor_boundary<C>(
    root: &Path,
    entries: &[PathBuf],
    filename: &str,
    propagate_to_ancestors: bool,
    exclusive_ancestor_boundary: &Path,
    should_cancel: &C,
) -> Result<PathBuf>
where
    C: Fn() -> bool,
{
    write_filelist_cancellable_inner(
        root,
        entries,
        filename,
        propagate_to_ancestors,
        Some(exclusive_ancestor_boundary),
        should_cancel,
    )
}

fn write_filelist_cancellable_inner<C>(
    root: &Path,
    entries: &[PathBuf],
    filename: &str,
    propagate_to_ancestors: bool,
    exclusive_ancestor_boundary: Option<&Path>,
    should_cancel: &C,
) -> Result<PathBuf>
where
    C: Fn() -> bool,
{
    if filename != "FileList.txt" && filename != "filelist.txt" {
        anyhow::bail!("unsupported FileList filename {filename}");
    }
    if should_cancel() {
        anyhow::bail!("filelist creation canceled");
    }
    let plan = plan_filelist_write_cancellable_inner(
        root,
        entries,
        FileListWriteOptions {
            // The legacy GUI reaches this adapter only after its overwrite
            // confirmation. New CLI/TUI callers use the public option model.
            allow_root_overwrite: true,
            propagate_to_ancestors,
        },
        exclusive_ancestor_boundary,
        should_cancel,
    )
    .map_err(|report| anyhow::anyhow!(report.summary()))?;
    let root_target = plan.root_target.clone();
    let report = execute_filelist_write_plan(&plan, should_cancel);
    if report.exit_code() == 0 {
        Ok(root_target)
    } else {
        Err(anyhow::anyhow!(report.summary()))
    }
}

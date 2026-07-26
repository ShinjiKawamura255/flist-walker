use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalkCancelled;

impl std::fmt::Display for WalkCancelled {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("walk cancelled")
    }
}

impl std::error::Error for WalkCancelled {}

fn walk(root: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    walk_into(root, &mut files, &mut dirs);
    (files, dirs)
}

fn walk_into(root: &Path, files: &mut Vec<PathBuf>, dirs: &mut Vec<PathBuf>) {
    let Ok(read_dir) = fs::read_dir(root) else {
        return;
    };
    for child in read_dir.flatten() {
        let Ok(file_type) = child.file_type() else {
            continue;
        };
        let path = child.path();
        if file_type.is_dir() {
            dirs.push(path.clone());
            if !file_type.is_symlink() {
                walk_into(&path, files, dirs);
            }
        } else {
            files.push(path);
        }
    }
}

pub fn walk_files(root: &Path) -> Vec<PathBuf> {
    walk(root).0
}

pub fn walk_dirs(root: &Path) -> Vec<PathBuf> {
    walk(root).1
}

pub fn walk_entries(root: &Path, include_files: bool, include_dirs: bool) -> Vec<PathBuf> {
    let (files, dirs) = walk(root);
    let mut out = Vec::new();
    if include_files {
        out.extend(files);
    }
    if include_dirs {
        out.extend(dirs);
    }
    out
}

pub fn walk_entries_cancellable<C>(
    root: &Path,
    include_files: bool,
    include_dirs: bool,
    should_cancel: C,
) -> Result<Vec<PathBuf>, WalkCancelled>
where
    C: Fn() -> bool,
{
    fn visit<C>(
        root: &Path,
        files: &mut Vec<PathBuf>,
        dirs: &mut Vec<PathBuf>,
        should_cancel: &C,
    ) -> Result<(), WalkCancelled>
    where
        C: Fn() -> bool,
    {
        if should_cancel() {
            return Err(WalkCancelled);
        }
        let Ok(read_dir) = fs::read_dir(root) else {
            return Ok(());
        };
        for child in read_dir.flatten() {
            if should_cancel() {
                return Err(WalkCancelled);
            }
            let Ok(file_type) = child.file_type() else {
                continue;
            };
            let path = child.path();
            if file_type.is_dir() {
                dirs.push(path.clone());
                if !file_type.is_symlink() {
                    visit(&path, files, dirs, should_cancel)?;
                }
            } else {
                files.push(path);
            }
        }
        if should_cancel() {
            return Err(WalkCancelled);
        }
        Ok(())
    }

    let mut files = Vec::new();
    let mut dirs = Vec::new();
    visit(root, &mut files, &mut dirs, &should_cancel)?;
    let mut out = Vec::new();
    if include_files {
        out.extend(files);
    }
    if include_dirs {
        out.extend(dirs);
    }
    Ok(out)
}

pub fn walk_entries_stream<F>(root: &Path, include_files: bool, include_dirs: bool, mut on_entry: F)
where
    F: FnMut(PathBuf),
{
    let result =
        walk_entries_stream_cancellable(root, include_files, include_dirs, || false, &mut on_entry);
    debug_assert_eq!(result, Ok(()));
}

pub fn walk_entries_stream_cancellable<F, C>(
    root: &Path,
    include_files: bool,
    include_dirs: bool,
    should_cancel: C,
    mut on_entry: F,
) -> Result<(), WalkCancelled>
where
    F: FnMut(PathBuf),
    C: Fn() -> bool,
{
    fn visit<F, C>(
        root: &Path,
        include_files: bool,
        include_dirs: bool,
        should_cancel: &C,
        on_entry: &mut F,
    ) -> Result<(), WalkCancelled>
    where
        F: FnMut(PathBuf),
        C: Fn() -> bool,
    {
        if should_cancel() {
            return Err(WalkCancelled);
        }
        let Ok(read_dir) = fs::read_dir(root) else {
            return Ok(());
        };
        for child in read_dir.flatten() {
            if should_cancel() {
                return Err(WalkCancelled);
            }
            let Ok(file_type) = child.file_type() else {
                continue;
            };
            let path = child.path();
            if file_type.is_dir() {
                if include_dirs {
                    if should_cancel() {
                        return Err(WalkCancelled);
                    }
                    on_entry(path.clone());
                }
                if !file_type.is_symlink() {
                    visit(&path, include_files, include_dirs, should_cancel, on_entry)?;
                }
            } else if include_files {
                if should_cancel() {
                    return Err(WalkCancelled);
                }
                on_entry(path);
            }
        }
        if should_cancel() {
            return Err(WalkCancelled);
        }
        Ok(())
    }

    visit(
        root,
        include_files,
        include_dirs,
        &should_cancel,
        &mut on_entry,
    )
}

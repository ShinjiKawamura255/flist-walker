use jwalk::WalkDir;
use std::path::{Path, PathBuf};

fn walk(root: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut files = Vec::new();
    let mut dirs = Vec::new();

    for entry in WalkDir::new(root)
        .follow_links(false)
        .min_depth(1)
        .into_iter()
        .flatten()
    {
        let path = entry.path().to_path_buf();
        if entry.file_type().is_dir() {
            dirs.push(path);
        } else {
            files.push(path);
        }
    }
    (files, dirs)
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

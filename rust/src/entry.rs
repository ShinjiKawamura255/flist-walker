use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryDisplayKind {
    File,
    Dir,
    Link,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryKind {
    pub display: EntryDisplayKind,
    pub is_dir: bool,
}

impl EntryKind {
    pub const fn file() -> Self {
        Self {
            display: EntryDisplayKind::File,
            is_dir: false,
        }
    }

    pub const fn dir() -> Self {
        Self {
            display: EntryDisplayKind::Dir,
            is_dir: true,
        }
    }

    pub const fn link(is_dir: bool) -> Self {
        Self {
            display: EntryDisplayKind::Link,
            is_dir,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub kind: Option<EntryKind>,
}

impl Entry {
    pub fn new(path: PathBuf, kind: Option<EntryKind>) -> Self {
        Self { path, kind }
    }

    pub fn unknown(path: PathBuf) -> Self {
        Self::new(path, None)
    }

    pub fn file(path: PathBuf) -> Self {
        Self::new(path, Some(EntryKind::file()))
    }

    pub fn dir(path: PathBuf) -> Self {
        Self::new(path, Some(EntryKind::dir()))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn is_visible_for_flags(&self, include_files: bool, include_dirs: bool) -> bool {
        match self.kind {
            Some(kind) => (kind.is_dir && include_dirs) || (!kind.is_dir && include_files),
            None => include_files && include_dirs,
        }
    }
}

impl From<PathBuf> for Entry {
    fn from(path: PathBuf) -> Self {
        Self::unknown(path)
    }
}

impl AsRef<Path> for Entry {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl PartialEq<PathBuf> for Entry {
    fn eq(&self, other: &PathBuf) -> bool {
        self.path == *other
    }
}

impl PartialEq<Entry> for PathBuf {
    fn eq(&self, other: &Entry) -> bool {
        *self == other.path
    }
}

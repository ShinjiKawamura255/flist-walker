use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryDisplayKind {
    File,
    Dir,
    Link,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryKind {
    pub display: EntryDisplayKind,
    /// Target directory state. `None` is used only for a link whose target
    /// has not been resolved yet, or for an entry that is not file-like.
    pub is_dir: Option<bool>,
}

impl EntryKind {
    pub const fn file() -> Self {
        Self {
            display: EntryDisplayKind::File,
            is_dir: Some(false),
        }
    }

    pub const fn dir() -> Self {
        Self {
            display: EntryDisplayKind::Dir,
            is_dir: Some(true),
        }
    }

    pub const fn link(is_dir: bool) -> Self {
        Self {
            display: EntryDisplayKind::Link,
            is_dir: Some(is_dir),
        }
    }

    pub const fn link_unknown() -> Self {
        Self {
            display: EntryDisplayKind::Link,
            is_dir: None,
        }
    }

    pub const fn other() -> Self {
        Self {
            display: EntryDisplayKind::Other,
            is_dir: None,
        }
    }

    pub const fn needs_resolution(self) -> bool {
        matches!(self.display, EntryDisplayKind::Link) && self.is_dir.is_none()
    }

    pub const fn is_link(self) -> bool {
        matches!(self.display, EntryDisplayKind::Link)
    }

    pub const fn is_visible_for_flags(self, include_files: bool, include_dirs: bool) -> bool {
        match self.display {
            EntryDisplayKind::File => include_files,
            EntryDisplayKind::Dir => include_dirs,
            EntryDisplayKind::Link => match self.is_dir {
                Some(true) => include_dirs,
                Some(false) => include_files,
                None => include_files && include_dirs,
            },
            EntryDisplayKind::Other => false,
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
            Some(kind) => kind.is_visible_for_flags(include_files, include_dirs),
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

#[cfg(test)]
mod tests {
    use super::{Entry, EntryKind};
    use std::path::PathBuf;

    #[test]
    fn unresolved_link_is_visible_only_when_both_filters_are_enabled() {
        let entry = Entry::new(PathBuf::from("link"), Some(EntryKind::link_unknown()));

        assert!(entry.is_visible_for_flags(true, true));
        assert!(!entry.is_visible_for_flags(true, false));
        assert!(!entry.is_visible_for_flags(false, true));
    }

    #[test]
    fn other_entries_are_never_visible_as_files_or_directories() {
        let entry = Entry::new(PathBuf::from("socket"), Some(EntryKind::other()));

        assert!(!entry.is_visible_for_flags(true, true));
        assert!(!entry.is_visible_for_flags(true, false));
        assert!(!entry.is_visible_for_flags(false, true));
    }
}

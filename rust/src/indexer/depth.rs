use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;
use std::path::{Component, Path};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MaxDepth(Option<NonZeroUsize>);

impl MaxDepth {
    pub const fn unlimited() -> Self {
        Self(None)
    }

    pub fn limited(value: usize) -> Option<Self> {
        NonZeroUsize::new(value).map(|value| Self(Some(value)))
    }

    pub const fn is_unlimited(self) -> bool {
        self.0.is_none()
    }

    pub const fn value(self) -> Option<usize> {
        match self.0 {
            Some(value) => Some(value.get()),
            None => None,
        }
    }

    pub(crate) fn allows_depth(self, depth: usize) -> bool {
        depth > 0 && self.value().is_none_or(|maximum| depth <= maximum)
    }

    pub(crate) fn should_descend_from(self, depth: usize) -> bool {
        self.value().is_none_or(|maximum| depth < maximum)
    }

    pub(crate) fn includes_path(self, root: &Path, path: &Path) -> bool {
        if self.is_unlimited() {
            return true;
        }
        lexical_depth_from_root(root, path).is_some_and(|depth| self.allows_depth(depth))
    }
}

impl From<NonZeroUsize> for MaxDepth {
    fn from(value: NonZeroUsize) -> Self {
        Self(Some(value))
    }
}

fn lexical_depth_from_root(root: &Path, path: &Path) -> Option<usize> {
    let relative = path.strip_prefix(root).ok()?;
    let mut depth = 0usize;
    for component in relative.components() {
        match component {
            Component::Normal(_) => depth = depth.saturating_add(1),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(depth)
}

#[cfg(test)]
mod tests {
    use super::MaxDepth;
    use std::path::Path;

    #[test]
    fn tc_180_lexical_depth_rejects_root_escape_without_io() {
        let depth = MaxDepth::limited(2).expect("valid depth");
        let root = Path::new("root");

        assert!(depth.includes_path(root, Path::new("root/child")));
        assert!(depth.includes_path(root, Path::new("root/child/file")));
        assert!(!depth.includes_path(root, Path::new("root/child/grand/file")));
        assert!(!depth.includes_path(root, Path::new("root/../outside")));
        assert!(!depth.includes_path(root, Path::new("outside")));
    }
}

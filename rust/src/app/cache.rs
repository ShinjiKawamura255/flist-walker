use super::*;
use crate::path_utils::path_key;

#[derive(Default)]
pub(super) struct PreviewCacheState {
    entries: HashMap<PathBuf, String>,
    order: VecDeque<PathBuf>,
    total_bytes: usize,
}

#[derive(Default)]
pub(super) struct HighlightCacheState {
    scope_query: String,
    scope_root: PathBuf,
    scope_use_regex: bool,
    scope_ignore_case: bool,
    scope_prefer_relative: bool,
    entries: HashMap<HighlightCacheKey, Arc<Vec<u16>>>,
    order: VecDeque<HighlightCacheKey>,
}

#[derive(Default)]
pub(super) struct SortMetadataCacheState {
    entries: HashMap<PathBuf, SortMetadata>,
    order: VecDeque<PathBuf>,
}

#[derive(Default)]
pub(super) struct EntryKindCacheState {
    pub(super) entries: HashMap<PathBuf, EntryKind>,
}

impl PreviewCacheState {
    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.total_bytes = 0;
    }

    pub(super) fn get(&self, path: &Path) -> Option<&String> {
        self.entries.get(path)
    }

    pub(super) fn insert_bounded(&mut self, path: PathBuf, preview: String, max_bytes: usize) {
        let new_bytes = preview.len();
        if let Some(old) = self.entries.get(&path) {
            self.total_bytes = self.total_bytes.saturating_sub(old.len());
        }
        if !self.entries.contains_key(&path) {
            self.order.push_back(path.clone());
        }
        self.entries.insert(path, preview);
        self.total_bytes = self.total_bytes.saturating_add(new_bytes);
        while self.total_bytes > max_bytes {
            if let Some(oldest) = self.order.pop_front() {
                if let Some(evicted) = self.entries.remove(&oldest) {
                    self.total_bytes = self.total_bytes.saturating_sub(evicted.len());
                }
            } else {
                break;
            }
        }
    }

    #[cfg(test)]
    pub(super) fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    pub(super) fn order_len(&self) -> usize {
        self.order.len()
    }

    #[cfg(test)]
    pub(super) fn contains(&self, path: &Path) -> bool {
        self.entries.contains_key(path)
    }
}

impl HighlightCacheState {
    pub(super) fn with_scope_ignore_case(scope_ignore_case: bool) -> Self {
        Self {
            scope_ignore_case,
            ..Self::default()
        }
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }

    pub(super) fn matches_scope(
        &self,
        query: &str,
        root: &Path,
        use_regex: bool,
        ignore_case: bool,
        prefer_relative: bool,
    ) -> bool {
        self.scope_query == query
            && path_key(&self.scope_root) == path_key(root)
            && self.scope_use_regex == use_regex
            && self.scope_ignore_case == ignore_case
            && self.scope_prefer_relative == prefer_relative
    }

    pub(super) fn reset_scope(
        &mut self,
        query: String,
        root: PathBuf,
        use_regex: bool,
        ignore_case: bool,
        prefer_relative: bool,
    ) {
        self.scope_query = query;
        self.scope_root = root;
        self.scope_use_regex = use_regex;
        self.scope_ignore_case = ignore_case;
        self.scope_prefer_relative = prefer_relative;
        self.clear();
    }

    pub(super) fn get(&self, key: &HighlightCacheKey) -> Option<Arc<Vec<u16>>> {
        self.entries.get(key).cloned()
    }

    pub(super) fn insert_bounded(
        &mut self,
        key: HighlightCacheKey,
        positions: Vec<u16>,
        max_entries: usize,
    ) {
        if !self.entries.contains_key(&key) {
            self.order.push_back(key.clone());
        }
        self.entries.insert(key, Arc::new(positions));
        while self.order.len() > max_entries {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
    }
}

impl SortMetadataCacheState {
    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }

    pub(super) fn contains(&self, path: &Path) -> bool {
        self.entries.contains_key(path)
    }

    pub(super) fn get_map(&self) -> &HashMap<PathBuf, SortMetadata> {
        &self.entries
    }

    pub(super) fn insert_bounded(
        &mut self,
        path: PathBuf,
        metadata: SortMetadata,
        max_entries: usize,
    ) {
        if !self.entries.contains_key(&path) {
            self.order.push_back(path.clone());
        }
        self.entries.insert(path.clone(), metadata);
        while self.order.len() > max_entries {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
        if !self.entries.contains_key(&path) {
            self.order.retain(|entry| entry != &path);
        }
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    pub(super) fn order_len(&self) -> usize {
        self.order.len()
    }

    #[cfg(test)]
    pub(super) fn contains_public(&self, path: &Path) -> bool {
        self.contains(path)
    }
}

impl EntryKindCacheState {
    pub(super) fn clear(&mut self) {
        self.entries.clear();
    }

    pub(super) fn get(&self, path: &Path) -> Option<EntryKind> {
        self.entries.get(path).copied()
    }

    pub(super) fn set(&mut self, path: PathBuf, kind: EntryKind) {
        self.entries.insert(path, kind);
    }

    pub(super) fn rebuild_from_entries(&mut self, entries: &[Entry]) {
        for entry in entries {
            if let Some(kind) = entry.kind {
                self.entries.insert(entry.path.clone(), kind);
            }
        }
    }

    pub(super) fn rebuild_from_sources(&mut self, sources: &[&[Entry]]) {
        self.clear();
        for entries in sources {
            self.rebuild_from_entries(entries);
        }
    }
}

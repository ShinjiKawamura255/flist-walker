use crate::entry::Entry;
use std::collections::VecDeque;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(crate) struct SearchEntriesSnapshotKey {
    pub(crate) ptr: usize,
    pub(crate) len: usize,
}

impl SearchEntriesSnapshotKey {
    pub(crate) fn from_entries(entries: &Arc<Vec<Entry>>) -> Self {
        Self {
            ptr: Arc::as_ptr(entries) as usize,
            len: entries.len(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SearchPrefixCacheEntry {
    snapshot: SearchEntriesSnapshotKey,
    query: String,
    matched_indices: Arc<Vec<usize>>,
    approx_bytes: usize,
}

#[derive(Default)]
pub(crate) struct SearchPrefixCache {
    pub(crate) entries: VecDeque<SearchPrefixCacheEntry>,
    pub(crate) total_bytes: usize,
}

impl SearchPrefixCache {
    pub(crate) const MAX_ENTRIES: usize = 64;
    pub(crate) const MAX_BYTES: usize = 8 * 1024 * 1024;
    pub(crate) const MAX_MATCHED_INDICES: usize = 20_000;
    const MIN_QUERY_LEN: usize = 3;

    pub(crate) fn is_cacheable_query(query: &str) -> bool {
        let q = query.trim();
        if q.len() < Self::MIN_QUERY_LEN {
            return false;
        }
        if q.contains(char::is_whitespace) {
            return false;
        }
        !q.contains(['|', '!', '\'', '^', '$'])
    }

    pub(crate) fn is_safe_prefix_extension(prefix: &str, query: &str) -> bool {
        if !Self::is_cacheable_query(prefix) || !Self::is_cacheable_query(query) {
            return false;
        }
        query.starts_with(prefix) && query.len() > prefix.len()
    }

    pub(crate) fn lookup_candidates(
        &mut self,
        snapshot: SearchEntriesSnapshotKey,
        query: &str,
    ) -> Option<Arc<Vec<usize>>> {
        if !Self::is_cacheable_query(query) {
            return None;
        }

        let mut best_idx = None;
        let mut best_len = 0usize;
        for (idx, entry) in self.entries.iter().enumerate() {
            if entry.snapshot != snapshot {
                continue;
            }
            if !Self::is_safe_prefix_extension(&entry.query, query) {
                continue;
            }
            if entry.query.len() > best_len {
                best_len = entry.query.len();
                best_idx = Some(idx);
            }
        }

        let idx = best_idx?;
        let entry = self.entries.remove(idx)?;
        let matched = Arc::clone(&entry.matched_indices);
        self.entries.push_back(entry);
        Some(matched)
    }

    pub(crate) fn maybe_store(
        &mut self,
        snapshot: SearchEntriesSnapshotKey,
        query: &str,
        matched_indices: Vec<usize>,
    ) {
        if !Self::is_cacheable_query(query) {
            return;
        }
        if matched_indices.is_empty() || matched_indices.len() > Self::MAX_MATCHED_INDICES {
            return;
        }

        let query = query.trim().to_string();
        let approx_bytes = query.len().saturating_add(
            matched_indices
                .len()
                .saturating_mul(std::mem::size_of::<usize>()),
        );
        if approx_bytes > Self::MAX_BYTES {
            return;
        }

        if let Some(existing_pos) = self
            .entries
            .iter()
            .position(|entry| entry.snapshot == snapshot && entry.query == query)
        {
            if let Some(old) = self.entries.remove(existing_pos) {
                self.total_bytes = self.total_bytes.saturating_sub(old.approx_bytes);
            }
        }

        self.total_bytes = self.total_bytes.saturating_add(approx_bytes);
        self.entries.push_back(SearchPrefixCacheEntry {
            snapshot,
            query,
            matched_indices: Arc::new(matched_indices),
            approx_bytes,
        });
        self.evict_over_limit();
    }

    fn evict_over_limit(&mut self) {
        while self.entries.len() > Self::MAX_ENTRIES || self.total_bytes > Self::MAX_BYTES {
            let Some(oldest) = self.entries.pop_front() else {
                break;
            };
            self.total_bytes = self.total_bytes.saturating_sub(oldest.approx_bytes);
        }
    }
}

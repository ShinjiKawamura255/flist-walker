use crate::entry::Entry;
use crate::search::{
    sort_scored_matches, top_ranked_scores, try_collect_search_matches, IndexedScore,
};
use crate::ui_model::{has_visible_match, normalize_path_for_display};
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
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

pub(crate) fn filter_search_results(
    results: Vec<(PathBuf, f64)>,
    root: &Path,
    query: &str,
    prefer_relative: bool,
    use_regex: bool,
    ignore_case: bool,
) -> Vec<(PathBuf, f64)> {
    if use_regex {
        return results;
    }

    results
        .into_iter()
        .filter(|(path, _)| has_visible_match(path, root, query, prefer_relative, ignore_case))
        .collect()
}

pub(crate) fn action_notice_for_targets(targets: &[PathBuf]) -> String {
    if targets.len() == 1 {
        format!("Action: {}", normalize_path_for_display(&targets[0]))
    } else {
        format!("Action: launched {} items", targets.len())
    }
}

pub(crate) fn action_targets_for_request(
    paths: &[PathBuf],
    open_parent_for_files: bool,
) -> Vec<PathBuf> {
    if !open_parent_for_files {
        return paths.to_vec();
    }

    let mut unique = HashSet::with_capacity(paths.len());
    let mut targets = Vec::with_capacity(paths.len());
    for path in paths {
        let target = action_target_path_for_open_in_folder(path);
        if unique.insert(target.clone()) {
            targets.push(target);
        }
    }
    targets
}

pub(crate) fn action_target_path_for_open_in_folder(path: &Path) -> PathBuf {
    if path.is_dir() {
        return path.to_path_buf();
    }
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| path.to_path_buf())
}

fn scored_indices_to_paths(
    entries: &[Entry],
    scored: &[IndexedScore],
    limit: usize,
) -> Vec<(PathBuf, f64)> {
    if limit == 0 || scored.is_empty() {
        return Vec::new();
    }
    scored
        .iter()
        .take(limit)
        .filter_map(|item| {
            entries
                .get(item.index)
                .map(|entry| (entry.path.clone(), item.score))
        })
        .collect()
}

pub(crate) fn rank_search_results(
    entries: &Arc<Vec<Entry>>,
    query: &str,
    root: &Path,
    limit: usize,
    use_regex: bool,
    ignore_case: bool,
    prefer_relative: bool,
    prefix_cache: &mut SearchPrefixCache,
) -> (Vec<(PathBuf, f64)>, Option<String>) {
    let query_trimmed = query.trim().to_string();
    let snapshot = SearchEntriesSnapshotKey::from_entries(entries);
    let cached_candidates = if use_regex {
        None
    } else {
        prefix_cache.lookup_candidates(snapshot, &query_trimmed)
    };
    let scored_matches = match try_collect_search_matches(
        query,
        &entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>(),
        use_regex,
        ignore_case,
        Some(root),
        prefer_relative,
        cached_candidates.as_ref().map(|items| items.as_slice()),
    ) {
        Ok(scored_matches) => scored_matches,
        Err(err) => return (Vec::new(), Some(err)),
    };
    if SearchPrefixCache::is_cacheable_query(&query_trimmed)
        && scored_matches.scored.len() <= SearchPrefixCache::MAX_MATCHED_INDICES
    {
        let mut ranked = scored_matches.scored.clone();
        sort_scored_matches(&mut ranked);
        let matched_indices = ranked.iter().map(|item| item.index).collect();
        prefix_cache.maybe_store(snapshot, &query_trimmed, matched_indices);
    }
    let ranked = top_ranked_scores(scored_matches.scored, limit);
    let raw_results = scored_indices_to_paths(entries, &ranked, limit);
    (
        filter_search_results(
            raw_results,
            root,
            query,
            prefer_relative,
            use_regex,
            ignore_case,
        ),
        None,
    )
}

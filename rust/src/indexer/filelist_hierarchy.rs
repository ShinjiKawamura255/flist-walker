use super::filelist_reader::parse_filelist_collect_with_max_depth;
use super::filelist_writer::filelist_modified_time;
use super::MaxDepth;
use anyhow::Result;
use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[cfg(test)]
thread_local! {
    static SUBTREE_INDEX_PROBES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static ORDERED_INDEX_BUILD_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static REPLACEMENT_MUTATION_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

struct OrderedPathIndex {
    by_components: BTreeMap<Vec<OsString>, u64>,
    by_order: BTreeMap<u64, PathBuf>,
    next_order: u64,
}

impl OrderedPathIndex {
    #[cfg(test)]
    fn from_entries(entries: Vec<PathBuf>) -> Self {
        match Self::from_entries_cancellable(entries, &|| false) {
            Ok(index) => index,
            Err(_) => unreachable!("non-cancellable index build"),
        }
    }

    fn from_entries_cancellable<C>(
        entries: Vec<PathBuf>,
        should_cancel: &C,
    ) -> std::result::Result<Self, Vec<PathBuf>>
    where
        C: Fn() -> bool,
    {
        let mut index = Self {
            by_components: BTreeMap::new(),
            by_order: BTreeMap::new(),
            next_order: 0,
        };
        let mut remaining = entries.into_iter();
        while let Some(entry) = remaining.next() {
            if should_cancel() {
                let mut restored = index.into_entries();
                restored.push(entry);
                restored.extend(remaining);
                return Err(restored);
            }
            #[cfg(test)]
            ORDERED_INDEX_BUILD_VISITS.with(|visits| visits.set(visits.get().saturating_add(1)));
            index.insert(entry);
        }
        if should_cancel() {
            return Err(index.into_entries());
        }
        Ok(index)
    }

    fn components(path: &Path) -> Vec<OsString> {
        path.components()
            .map(|component| component.as_os_str().to_os_string())
            .collect()
    }

    fn insert(&mut self, path: PathBuf) -> bool {
        let key = Self::components(&path);
        if self.by_components.contains_key(&key) {
            return false;
        }
        let order = self.next_order;
        self.next_order = self.next_order.saturating_add(1);
        self.by_components.insert(key, order);
        self.by_order.insert(order, path);
        true
    }

    fn replace_subtree<C>(
        &mut self,
        subtree_root: &Path,
        replacements: Vec<PathBuf>,
        should_cancel: &C,
    ) -> Result<bool>
    where
        C: Fn() -> bool,
    {
        let before_len = self.by_order.len();
        let prefix = Self::components(subtree_root);
        // Regression guard: component keys keep a subtree contiguous in the BTree.
        // Do not replace this range lookup with a scan of every catalog entry.
        let mut removed = Vec::new();
        for (key, order) in self.by_components.range(prefix.clone()..) {
            #[cfg(test)]
            SUBTREE_INDEX_PROBES.with(|visits| visits.set(visits.get().saturating_add(1)));
            if !key.starts_with(&prefix) {
                break;
            }
            if should_cancel() {
                anyhow::bail!("superseded");
            }
            removed.push((key.clone(), *order));
        }
        if should_cancel() {
            anyhow::bail!("superseded");
        }

        let mut removed_keys = HashSet::with_capacity(removed.len());
        for (key, _) in &removed {
            if should_cancel() {
                anyhow::bail!("superseded");
            }
            removed_keys.insert(key.clone());
        }
        let mut staged_keys = HashSet::new();
        let mut staged_replacements = Vec::new();
        for replacement in replacements {
            if should_cancel() {
                anyhow::bail!("superseded");
            }
            let key = Self::components(&replacement);
            let exists_outside_subtree =
                self.by_components.contains_key(&key) && !removed_keys.contains(&key);
            if !exists_outside_subtree && staged_keys.insert(key.clone()) {
                staged_replacements.push((key, replacement));
            }
        }

        let original_next_order = self.next_order;
        let mut deleted = Vec::with_capacity(removed.len());
        for (key, order) in removed {
            if should_cancel() {
                self.restore_replacement(deleted, Vec::new(), original_next_order);
                anyhow::bail!("superseded");
            }
            self.by_components.remove(&key);
            let path = self
                .by_order
                .remove(&order)
                .expect("component and order indexes stay aligned");
            deleted.push((key, order, path));
            #[cfg(test)]
            REPLACEMENT_MUTATION_VISITS.with(|visits| visits.set(visits.get().saturating_add(1)));
        }

        let mut inserted = Vec::with_capacity(staged_replacements.len());
        for (key, replacement) in staged_replacements {
            if should_cancel() {
                self.restore_replacement(deleted, inserted, original_next_order);
                anyhow::bail!("superseded");
            }
            let order = self.next_order;
            self.next_order = self.next_order.saturating_add(1);
            self.by_components.insert(key.clone(), order);
            self.by_order.insert(order, replacement);
            inserted.push((key, order));
            #[cfg(test)]
            REPLACEMENT_MUTATION_VISITS.with(|visits| visits.set(visits.get().saturating_add(1)));
        }
        if should_cancel() {
            self.restore_replacement(deleted, inserted, original_next_order);
            anyhow::bail!("superseded");
        }
        Ok(before_len != self.by_order.len() || !inserted.is_empty())
    }

    fn restore_replacement(
        &mut self,
        deleted: Vec<(Vec<OsString>, u64, PathBuf)>,
        inserted: Vec<(Vec<OsString>, u64)>,
        original_next_order: u64,
    ) {
        for (key, order) in inserted {
            self.by_components.remove(&key);
            self.by_order.remove(&order);
        }
        for (key, order, path) in deleted {
            self.by_components.insert(key, order);
            self.by_order.insert(order, path);
        }
        self.next_order = original_next_order;
    }

    fn into_entries(self) -> Vec<PathBuf> {
        self.by_order.into_values().collect()
    }
}

pub(super) fn apply_nested_filelist_overrides<C>(
    root_filelist: &Path,
    root: &Path,
    root_modified: Option<SystemTime>,
    entries: &mut Vec<PathBuf>,
    entry_types: (bool, bool),
    max_depth: MaxDepth,
    should_cancel: &C,
) -> Result<bool>
where
    C: Fn() -> bool,
{
    type PendingFileList = (Reverse<usize>, u64, PathBuf);
    let (include_files, include_dirs) = entry_types;

    let mut changed = false;
    let mut active_filelist_modified: HashMap<PathBuf, Option<SystemTime>> = HashMap::new();
    active_filelist_modified.insert(root.to_path_buf(), root_modified);

    let mut discovered = HashSet::new();
    let mut pending: std::collections::BinaryHeap<PendingFileList> =
        std::collections::BinaryHeap::new();
    let mut pending_seq = 0u64;
    enqueue_nested_filelists_from_entries(
        entries,
        root_filelist,
        root,
        &mut discovered,
        &mut pending,
        &mut pending_seq,
        should_cancel,
    )?;
    if pending.is_empty() {
        return Ok(false);
    }

    let mut indexed_entries = None;
    let processing_result = (|| -> Result<()> {
        while let Some((_depth, _seq, child_filelist)) = pending.pop() {
            if should_cancel() {
                anyhow::bail!("superseded");
            }
            let Some(child_root) = child_filelist.parent().map(Path::to_path_buf) else {
                continue;
            };
            let active_modified =
                nearest_active_modified(&child_root, root, &active_filelist_modified).flatten();
            let child_modified = filelist_modified_time(&child_filelist);
            if !is_filelist_newer(child_modified, active_modified) {
                continue;
            }
            let child_entries = parse_filelist_collect_with_max_depth(
                &child_filelist,
                root,
                include_files,
                include_dirs,
                max_depth,
                should_cancel,
            )?;
            enqueue_nested_filelists_from_entries(
                &child_entries,
                root_filelist,
                root,
                &mut discovered,
                &mut pending,
                &mut pending_seq,
                should_cancel,
            )?;
            if indexed_entries.is_none() {
                // Regression guard: stale child FileLists must not pay the full
                // catalog indexing cost. Build only after an accepted child parses.
                let source_entries = std::mem::take(entries);
                indexed_entries =
                    match OrderedPathIndex::from_entries_cancellable(source_entries, should_cancel)
                    {
                        Ok(index) => Some(index),
                        Err(restored) => {
                            *entries = restored;
                            anyhow::bail!("superseded");
                        }
                    };
            }
            changed |= indexed_entries
                .as_mut()
                .expect("accepted child initializes the index")
                .replace_subtree(&child_root, child_entries, should_cancel)?;
            active_filelist_modified.insert(child_root, child_modified);
        }
        Ok(())
    })();
    if let Some(indexed_entries) = indexed_entries {
        *entries = indexed_entries.into_entries();
    }
    processing_result?;
    Ok(changed)
}

fn enqueue_nested_filelists_from_entries(
    entries: &[PathBuf],
    root_filelist: &Path,
    root: &Path,
    discovered: &mut HashSet<PathBuf>,
    pending: &mut std::collections::BinaryHeap<(Reverse<usize>, u64, PathBuf)>,
    pending_seq: &mut u64,
    should_cancel: &impl Fn() -> bool,
) -> Result<()> {
    for path in entries {
        if should_cancel() {
            anyhow::bail!("superseded");
        }
        if path == root_filelist {
            continue;
        }
        if !path.starts_with(root) {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if !matches!(name, "FileList.txt" | "filelist.txt") {
            continue;
        }
        if !path.is_file() {
            continue;
        }
        if discovered.insert(path.clone()) {
            let depth = path_depth_from_root(path, root).unwrap_or(usize::MAX);
            pending.push((Reverse(depth), *pending_seq, path.clone()));
            *pending_seq = pending_seq.saturating_add(1);
        }
    }
    Ok(())
}

fn path_depth_from_root(path: &Path, root: &Path) -> Option<usize> {
    path.parent()
        .and_then(|parent| parent.strip_prefix(root).ok())
        .map(|rel| rel.components().count())
}

fn nearest_active_modified(
    subtree_root: &Path,
    root: &Path,
    active_filelist_modified: &HashMap<PathBuf, Option<SystemTime>>,
) -> Option<Option<SystemTime>> {
    let mut current = Some(subtree_root);
    while let Some(path) = current {
        if let Some(found) = active_filelist_modified.get(path) {
            return Some(*found);
        }
        if path == root {
            break;
        }
        current = path.parent();
    }
    None
}

fn is_filelist_newer(candidate: Option<SystemTime>, baseline: Option<SystemTime>) -> bool {
    match (candidate, baseline) {
        (Some(lhs), Some(rhs)) => lhs > rhs,
        (Some(_), None) => true,
        _ => false,
    }
}

#[cfg(test)]
mod alignment_tests {
    use super::*;
    use std::fs;
    use std::time::{Duration, UNIX_EPOCH};

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "flistwalker-filelist-hierarchy-{name}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn alignment_nested_discovery_cancels_without_a_child_filelist() {
        let root = Path::new("root");
        let mut entries = (0..100).map(|i| root.join(format!("file-{i}"))).collect();
        let calls = std::cell::Cell::new(0);
        let result = apply_nested_filelist_overrides(
            &root.join("FileList.txt"),
            root,
            None,
            &mut entries,
            (true, true),
            MaxDepth::unlimited(),
            &|| {
                calls.set(calls.get() + 1);
                calls.get() >= 3
            },
        );
        assert!(result.is_err());
        assert_eq!(calls.get(), 3);
    }

    #[test]
    fn regression_sibling_subtree_replacements_do_not_rescan_the_whole_catalog() {
        let root = Path::new("root");
        let entries = (0..10_000)
            .map(|index| root.join(format!("dir-{index:05}/old.txt")))
            .collect::<Vec<_>>();
        let mut catalog = OrderedPathIndex::from_entries(entries);
        SUBTREE_INDEX_PROBES.with(|visits| visits.set(0));
        for ordinal in 0..100 {
            let subtree = root.join(format!("dir-{ordinal:05}"));
            catalog
                .replace_subtree(&subtree, vec![subtree.join("replacement.txt")], &|| false)
                .expect("replace subtree");
        }

        let entries = catalog.into_entries();
        assert_eq!(entries.len(), 10_000);
        assert!(
            SUBTREE_INDEX_PROBES.with(|visits| visits.get()) < 50_000,
            "each sibling override must touch its subtree instead of every catalog entry"
        );
    }

    #[test]
    fn regression_ordered_subtree_replacement_preserves_append_order_and_deduplication() {
        let root = Path::new("root");
        let outside = root.join("outside.txt");
        let mut catalog = OrderedPathIndex::from_entries(vec![
            root.join("a/old.txt"),
            root.join("b/old.txt"),
            outside.clone(),
        ]);

        assert!(catalog
            .replace_subtree(
                &root.join("a"),
                vec![root.join("a/new.txt"), outside.clone()],
                &|| false,
            )
            .expect("replace a"));
        assert!(catalog
            .replace_subtree(&root.join("b"), vec![root.join("b/new.txt")], &|| false,)
            .expect("replace b"));

        assert_eq!(
            catalog.into_entries(),
            vec![outside, root.join("a/new.txt"), root.join("b/new.txt")]
        );
    }

    #[test]
    fn regression_component_prefix_does_not_remove_similarly_named_sibling() {
        let root = Path::new("root");
        let sibling = root.join("a-file/keep.txt");
        let mut catalog =
            OrderedPathIndex::from_entries(vec![root.join("a/old.txt"), sibling.clone()]);

        catalog
            .replace_subtree(&root.join("a"), vec![root.join("a/new.txt")], &|| false)
            .expect("replace subtree");

        assert_eq!(
            catalog.into_entries(),
            vec![sibling, root.join("a/new.txt")]
        );
    }

    #[test]
    fn regression_older_child_filelist_does_not_build_the_catalog_index() {
        let root = test_root("older-child");
        let child_root = root.join("child");
        fs::create_dir_all(&child_root).expect("create child dir");
        let root_filelist = root.join("FileList.txt");
        let child_filelist = child_root.join("FileList.txt");
        fs::write(&root_filelist, "child/FileList.txt\n").expect("write root filelist");
        fs::write(&child_filelist, "replacement.txt\n").expect("write child filelist");
        let mut entries = vec![child_filelist.clone()];
        entries.extend((0..10_000).map(|index| root.join(format!("item-{index:05}.txt"))));
        let before = entries.clone();
        ORDERED_INDEX_BUILD_VISITS.with(|visits| visits.set(0));

        let changed = apply_nested_filelist_overrides(
            &root_filelist,
            &root,
            Some(SystemTime::now() + Duration::from_secs(60)),
            &mut entries,
            (true, true),
            MaxDepth::unlimited(),
            &|| false,
        )
        .expect("ignore older child filelist");

        assert!(!changed);
        assert_eq!(entries, before);
        assert_eq!(ORDERED_INDEX_BUILD_VISITS.with(|visits| visits.get()), 0);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn regression_cancel_during_index_build_restores_caller_entries() {
        let root = test_root("cancel-build");
        let child_root = root.join("child");
        fs::create_dir_all(&child_root).expect("create child dir");
        let root_filelist = root.join("FileList.txt");
        let child_filelist = child_root.join("FileList.txt");
        fs::write(&root_filelist, "child/FileList.txt\n").expect("write root filelist");
        fs::write(&child_filelist, "replacement.txt\n").expect("write child filelist");
        let mut entries = vec![child_filelist];
        entries.extend((0..10_000).map(|index| root.join(format!("item-{index:05}.txt"))));
        let before = entries.clone();
        ORDERED_INDEX_BUILD_VISITS.with(|visits| visits.set(0));

        let result = apply_nested_filelist_overrides(
            &root_filelist,
            &root,
            Some(UNIX_EPOCH),
            &mut entries,
            (true, true),
            MaxDepth::unlimited(),
            &|| ORDERED_INDEX_BUILD_VISITS.with(|visits| visits.get()) >= 64,
        );

        assert!(result.is_err());
        assert_eq!(entries, before);
        let visits = ORDERED_INDEX_BUILD_VISITS.with(|visits| visits.get());
        assert!((64..10_000).contains(&visits), "build visits: {visits}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn regression_cancel_during_subtree_mutation_restores_caller_entries() {
        let root = test_root("cancel-mutation");
        let child_root = root.join("child");
        fs::create_dir_all(&child_root).expect("create child dir");
        let root_filelist = root.join("FileList.txt");
        let child_filelist = child_root.join("FileList.txt");
        fs::write(&root_filelist, "child/FileList.txt\n").expect("write root filelist");
        let replacement_text = (0..1_000)
            .map(|index| format!("new-{index:04}.txt"))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&child_filelist, replacement_text).expect("write child filelist");
        let mut entries = vec![child_filelist];
        entries.extend((0..1_000).map(|index| child_root.join(format!("old-{index:04}.txt"))));
        entries.push(root.join("outside.txt"));
        let before = entries.clone();
        REPLACEMENT_MUTATION_VISITS.with(|visits| visits.set(0));

        let result = apply_nested_filelist_overrides(
            &root_filelist,
            &root,
            Some(UNIX_EPOCH),
            &mut entries,
            (true, true),
            MaxDepth::unlimited(),
            &|| REPLACEMENT_MUTATION_VISITS.with(|visits| visits.get()) >= 64,
        );

        assert!(result.is_err());
        assert_eq!(entries, before);
        assert_eq!(REPLACEMENT_MUTATION_VISITS.with(|visits| visits.get()), 64);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn regression_nested_sibling_overrides_bound_end_to_end_index_visits() {
        let root = test_root("sibling-end-to-end");
        fs::create_dir_all(&root).expect("create root dir");
        let root_filelist = root.join("FileList.txt");
        fs::write(&root_filelist, "root\n").expect("write root filelist");
        let mut entries = Vec::with_capacity(10_100);
        for ordinal in 0..100 {
            let child_root = root.join(format!("dir-{ordinal:05}"));
            fs::create_dir_all(&child_root).expect("create child dir");
            let child_filelist = child_root.join("FileList.txt");
            fs::write(&child_filelist, "replacement.txt\n").expect("write child filelist");
            entries.push(child_filelist);
            entries.push(child_root.join("old.txt"));
        }
        entries.extend((100..10_000).map(|ordinal| root.join(format!("dir-{ordinal:05}/old.txt"))));
        SUBTREE_INDEX_PROBES.with(|visits| visits.set(0));
        ORDERED_INDEX_BUILD_VISITS.with(|visits| visits.set(0));

        let changed = apply_nested_filelist_overrides(
            &root_filelist,
            &root,
            Some(UNIX_EPOCH),
            &mut entries,
            (true, true),
            MaxDepth::unlimited(),
            &|| false,
        )
        .expect("apply sibling overrides");

        assert!(changed);
        assert_eq!(entries.len(), 10_000);
        assert_eq!(
            ORDERED_INDEX_BUILD_VISITS.with(|visits| visits.get()),
            10_100
        );
        let probes = SUBTREE_INDEX_PROBES.with(|visits| visits.get());
        assert!(probes < 50_000, "subtree index probes: {probes}");
        let _ = fs::remove_dir_all(&root);
    }
}

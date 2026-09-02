use super::IndexEntry;
use crate::entry::Entry;
use crate::indexer::IndexSource;
use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug)]
pub(super) struct BackgroundIndexFinalizeIdentity {
    pub(super) tab_id: u64,
    pub(super) request_id: u64,
    pub(super) source: IndexSource,
}

#[derive(Debug)]
pub(super) struct BackgroundIndexFinalizePolicy {
    pub(super) include_files: bool,
    pub(super) include_dirs: bool,
    pub(super) root: PathBuf,
    pub(super) prefer_relative: bool,
    pub(super) ignore_case: bool,
    pub(super) ignore_list_enabled: bool,
    pub(super) ignore_terms_source: Arc<Vec<String>>,
}

#[derive(Debug)]
pub(super) struct BackgroundIndexFinalizeInputs {
    pub(super) initial_entries: VecDeque<Entry>,
    pub(super) pending_entries: VecDeque<IndexEntry>,
    pub(super) continuation_entries: VecDeque<Entry>,
    pub(super) discarded_entries: VecDeque<Entry>,
    pub(super) discarded_pending_entries: VecDeque<IndexEntry>,
    pub(super) capture_filelist_paths: bool,
}

#[derive(Debug)]
pub(super) struct PendingBackgroundIndexFinalize {
    pub(super) tab_id: u64,
    pub(super) request_id: u64,
    pub(super) source: IndexSource,
    pub(super) include_files: bool,
    pub(super) include_dirs: bool,
    pub(super) root: PathBuf,
    pub(super) prefer_relative: bool,
    pub(super) ignore_case: bool,
    pub(super) ignore_list_enabled: bool,
    pub(super) ignore_terms_source: Arc<Vec<String>>,
    pub(super) ignore_terms: Option<Arc<crate::query::CompiledIgnoreTerms>>,
    pub(super) initial_entries: VecDeque<Entry>,
    pub(super) pending_entries: VecDeque<IndexEntry>,
    pub(super) continuation_entries: VecDeque<Entry>,
    pub(super) discarded_entries: VecDeque<Entry>,
    pub(super) discarded_pending_entries: VecDeque<IndexEntry>,
    pub(super) completed_entries: Vec<Entry>,
    pub(super) filtered_entries: Option<Vec<Entry>>,
    pub(super) filter_cursor: usize,
    pub(super) unresolved_kind_paths: VecDeque<PathBuf>,
    pub(super) unresolved_kind_paths_set: HashSet<PathBuf>,
    pub(super) kind_cursor: usize,
    pub(super) filelist_paths: Option<Vec<PathBuf>>,
    pub(super) scratch_reclaimed: bool,
}

#[derive(Debug, Default)]
pub(super) struct BackgroundIndexFinalizeScratch {
    initial_entries: VecDeque<Entry>,
    pending_entries: VecDeque<IndexEntry>,
    continuation_entries: VecDeque<Entry>,
    discarded_entries: VecDeque<Entry>,
    discarded_pending_entries: VecDeque<IndexEntry>,
}

#[derive(Debug, Default)]
pub(super) struct BackgroundIndexFilterScratch {
    filtered_entries: Option<Vec<Entry>>,
    filter_cursor: usize,
    unresolved_kind_paths: VecDeque<PathBuf>,
    unresolved_kind_paths_set: HashSet<PathBuf>,
    kind_cursor: usize,
}

impl BackgroundIndexFinalizeScratch {
    pub(super) fn heavy_resource_weight(&self) -> usize {
        self.initial_entries
            .capacity()
            .saturating_add(self.pending_entries.capacity())
            .saturating_add(self.continuation_entries.capacity())
            .saturating_add(self.discarded_entries.capacity())
            .saturating_add(self.discarded_pending_entries.capacity())
    }
}

impl BackgroundIndexFilterScratch {
    pub(super) fn heavy_resource_weight(&self) -> usize {
        self.filtered_entries
            .as_ref()
            .map_or(0, Vec::capacity)
            .saturating_add(self.unresolved_kind_paths.capacity())
            .saturating_add(self.unresolved_kind_paths_set.capacity())
    }
}

impl PendingBackgroundIndexFinalize {
    pub(super) fn new(
        identity: BackgroundIndexFinalizeIdentity,
        policy: BackgroundIndexFinalizePolicy,
        inputs: BackgroundIndexFinalizeInputs,
    ) -> Self {
        let BackgroundIndexFinalizeIdentity {
            tab_id,
            request_id,
            source,
        } = identity;
        let BackgroundIndexFinalizePolicy {
            include_files,
            include_dirs,
            root,
            prefer_relative,
            ignore_case,
            ignore_list_enabled,
            ignore_terms_source,
        } = policy;
        let BackgroundIndexFinalizeInputs {
            initial_entries,
            pending_entries,
            continuation_entries,
            discarded_entries,
            discarded_pending_entries,
            capture_filelist_paths,
        } = inputs;
        let selected_len = initial_entries
            .len()
            .saturating_add(pending_entries.len())
            .saturating_add(continuation_entries.len());
        let ignore_list_enabled = ignore_list_enabled && !ignore_terms_source.is_empty();
        let ignore_terms = ignore_list_enabled.then(|| {
            Arc::new(crate::query::CompiledIgnoreTerms::compile(
                ignore_terms_source.as_slice(),
                ignore_case,
            ))
        });
        let needs_filter = !include_files || !include_dirs || ignore_terms.is_some();
        let tracks_unknown_kinds =
            matches!(source, IndexSource::Walker) && (!include_files || !include_dirs);
        Self {
            tab_id,
            request_id,
            source,
            include_files,
            include_dirs,
            root,
            prefer_relative,
            ignore_case,
            ignore_list_enabled,
            ignore_terms_source,
            ignore_terms,
            initial_entries,
            pending_entries,
            continuation_entries,
            discarded_entries,
            discarded_pending_entries,
            completed_entries: Vec::with_capacity(selected_len),
            filtered_entries: needs_filter.then(|| Vec::with_capacity(selected_len)),
            filter_cursor: 0,
            unresolved_kind_paths: if tracks_unknown_kinds {
                VecDeque::with_capacity(selected_len)
            } else {
                VecDeque::new()
            },
            unresolved_kind_paths_set: if tracks_unknown_kinds {
                HashSet::with_capacity(selected_len)
            } else {
                HashSet::new()
            },
            kind_cursor: 0,
            filelist_paths: capture_filelist_paths.then(|| Vec::with_capacity(selected_len)),
            scratch_reclaimed: false,
        }
    }

    pub(super) fn heavy_resource_weight(&self) -> usize {
        self.initial_entries
            .capacity()
            .saturating_add(self.pending_entries.capacity())
            .saturating_add(self.continuation_entries.capacity())
            .saturating_add(self.discarded_entries.capacity())
            .saturating_add(self.discarded_pending_entries.capacity())
            .saturating_add(self.completed_entries.capacity())
            .saturating_add(self.filtered_entries.as_ref().map_or(0, Vec::capacity))
            .saturating_add(self.unresolved_kind_paths.capacity())
            .saturating_add(self.unresolved_kind_paths_set.capacity())
            .saturating_add(self.filelist_paths.as_ref().map_or(0, Vec::capacity))
    }

    pub(super) fn is_complete(&self) -> bool {
        self.initial_entries.is_empty()
            && self.pending_entries.is_empty()
            && self.continuation_entries.is_empty()
            && self
                .filtered_entries
                .as_ref()
                .is_none_or(|_| self.filter_cursor == self.completed_entries.len())
            && (!matches!(self.source, IndexSource::Walker)
                || (self.include_files && self.include_dirs)
                || self.kind_cursor == self.completed_entries.len())
    }

    pub(super) fn filter_policy_matches(
        &self,
        include_files: bool,
        include_dirs: bool,
        ignore_case: bool,
        enabled: bool,
        terms: &Arc<Vec<String>>,
    ) -> bool {
        let enabled = enabled && !terms.is_empty();
        self.include_files == include_files
            && self.include_dirs == include_dirs
            && self.ignore_case == ignore_case
            && self.ignore_list_enabled == enabled
            && Arc::ptr_eq(&self.ignore_terms_source, terms)
    }

    pub(super) fn take_filter_scratch(&mut self) -> BackgroundIndexFilterScratch {
        BackgroundIndexFilterScratch {
            filtered_entries: self.filtered_entries.take(),
            filter_cursor: self.filter_cursor,
            unresolved_kind_paths: std::mem::take(&mut self.unresolved_kind_paths),
            unresolved_kind_paths_set: std::mem::take(&mut self.unresolved_kind_paths_set),
            kind_cursor: self.kind_cursor,
        }
    }

    pub(super) fn restore_filter_scratch(&mut self, scratch: BackgroundIndexFilterScratch) {
        self.filtered_entries = scratch.filtered_entries;
        self.filter_cursor = scratch.filter_cursor;
        self.unresolved_kind_paths = scratch.unresolved_kind_paths;
        self.unresolved_kind_paths_set = scratch.unresolved_kind_paths_set;
        self.kind_cursor = scratch.kind_cursor;
    }

    pub(super) fn apply_filter_policy(
        &mut self,
        include_files: bool,
        include_dirs: bool,
        ignore_case: bool,
        enabled: bool,
        terms: Arc<Vec<String>>,
    ) {
        let enabled = enabled && !terms.is_empty();
        self.include_files = include_files;
        self.include_dirs = include_dirs;
        self.ignore_case = ignore_case;
        self.ignore_list_enabled = enabled;
        self.ignore_terms_source = Arc::clone(&terms);
        self.ignore_terms = enabled.then(|| {
            Arc::new(crate::query::CompiledIgnoreTerms::compile(
                terms.as_slice(),
                self.ignore_case,
            ))
        });
        let needs_filter = !self.include_files || !self.include_dirs || enabled;
        self.filtered_entries =
            needs_filter.then(|| Vec::with_capacity(self.completed_entries.capacity()));
        self.filter_cursor = 0;
        self.unresolved_kind_paths = if matches!(self.source, IndexSource::Walker)
            && (!self.include_files || !self.include_dirs)
        {
            VecDeque::with_capacity(self.completed_entries.capacity())
        } else {
            VecDeque::new()
        };
        self.unresolved_kind_paths_set = if matches!(self.source, IndexSource::Walker)
            && (!self.include_files || !self.include_dirs)
        {
            HashSet::with_capacity(self.completed_entries.capacity())
        } else {
            HashSet::new()
        };
        self.kind_cursor = 0;
    }

    pub(super) fn take_scratch(&mut self) -> BackgroundIndexFinalizeScratch {
        BackgroundIndexFinalizeScratch {
            initial_entries: std::mem::take(&mut self.initial_entries),
            pending_entries: std::mem::take(&mut self.pending_entries),
            continuation_entries: std::mem::take(&mut self.continuation_entries),
            discarded_entries: std::mem::take(&mut self.discarded_entries),
            discarded_pending_entries: std::mem::take(&mut self.discarded_pending_entries),
        }
    }

    pub(super) fn restore_scratch(&mut self, scratch: BackgroundIndexFinalizeScratch) {
        self.initial_entries = scratch.initial_entries;
        self.pending_entries = scratch.pending_entries;
        self.continuation_entries = scratch.continuation_entries;
        self.discarded_entries = scratch.discarded_entries;
        self.discarded_pending_entries = scratch.discarded_pending_entries;
    }

    pub(super) fn advance(&mut self, max_entries: usize, frame_budget: std::time::Duration) {
        let started = std::time::Instant::now();
        let mut processed = 0usize;
        while processed < max_entries && started.elapsed() < frame_budget {
            let next = self
                .initial_entries
                .pop_front()
                .or_else(|| self.pending_entries.pop_front().map(Entry::from))
                .or_else(|| self.continuation_entries.pop_front());
            if let Some(entry) = next {
                if let Some(paths) = self.filelist_paths.as_mut() {
                    paths.push(entry.path.clone());
                }
                self.completed_entries.push(entry);
            } else {
                break;
            }
            processed = processed.saturating_add(1);
        }
        while processed < max_entries
            && started.elapsed() < frame_budget
            && self.initial_entries.is_empty()
            && self.pending_entries.is_empty()
            && self.continuation_entries.is_empty()
            && matches!(self.source, IndexSource::Walker)
            && (!self.include_files || !self.include_dirs)
            && self.kind_cursor < self.completed_entries.len()
        {
            let entry = &self.completed_entries[self.kind_cursor];
            if entry.kind.is_none() {
                self.unresolved_kind_paths_set.insert(entry.path.clone());
                self.unresolved_kind_paths.push_back(entry.path.clone());
            }
            self.kind_cursor = self.kind_cursor.saturating_add(1);
            processed = processed.saturating_add(1);
        }
        while processed < max_entries
            && started.elapsed() < frame_budget
            && self.initial_entries.is_empty()
            && self.pending_entries.is_empty()
            && self.continuation_entries.is_empty()
            && self.filtered_entries.is_some()
            && self.filter_cursor < self.completed_entries.len()
        {
            let entry = &self.completed_entries[self.filter_cursor];
            let ignored = self.ignore_terms.as_ref().is_some_and(|compiled| {
                compiled.matches_path(
                    entry.path(),
                    crate::query::QueryScope {
                        root: Some(&self.root),
                        prefer_relative: self.prefer_relative,
                        ignore_case: self.ignore_case,
                    },
                )
            });
            if !ignored && entry.is_visible_for_flags(self.include_files, self.include_dirs) {
                if let Some(filtered_entries) = self.filtered_entries.as_mut() {
                    filtered_entries.push(entry.clone());
                }
            }
            self.filter_cursor = self.filter_cursor.saturating_add(1);
            processed = processed.saturating_add(1);
        }
    }
}

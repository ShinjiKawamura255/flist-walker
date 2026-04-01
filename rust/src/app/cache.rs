use super::*;

#[derive(Default)]
pub(super) struct PreviewCacheState {
    pub(super) entries: HashMap<PathBuf, String>,
    pub(super) order: VecDeque<PathBuf>,
    pub(super) total_bytes: usize,
}

#[derive(Default)]
pub(super) struct HighlightCacheState {
    pub(super) scope_query: String,
    pub(super) scope_root: PathBuf,
    pub(super) scope_use_regex: bool,
    pub(super) scope_ignore_case: bool,
    pub(super) scope_prefer_relative: bool,
    pub(super) entries: HashMap<HighlightCacheKey, Arc<Vec<u16>>>,
    pub(super) order: VecDeque<HighlightCacheKey>,
}

#[derive(Default)]
pub(super) struct SortMetadataCacheState {
    pub(super) entries: HashMap<PathBuf, SortMetadata>,
    pub(super) order: VecDeque<PathBuf>,
}

impl FlistWalkerApp {
    pub(super) fn poll_preview_response(&mut self) {
        while let Ok(response) = self.preview_rx.try_recv() {
            let target_tab_id = self.preview_request_tabs.remove(&response.request_id);
            if Some(response.request_id) == self.pending_preview_request_id {
                self.pending_preview_request_id = None;
                self.preview_in_progress = false;
                self.cache_preview(response.path.clone(), response.preview.clone());
                if let Some(row) = self.current_row {
                    if let Some((current_path, _)) = self.results.get(row) {
                        if *current_path == response.path {
                            self.preview = response.preview;
                        }
                    }
                }
                continue;
            }
            let Some(tab_id) = target_tab_id else {
                continue;
            };
            let Some(tab_index) = self.find_tab_index_by_id(tab_id) else {
                continue;
            };
            self.cache_preview(response.path.clone(), response.preview.clone());
            if let Some(tab) = self.tabs.get_mut(tab_index) {
                tab.pending_preview_request_id = None;
                tab.preview_in_progress = false;
                let current_path = if tab.result_state.results_compacted {
                    tab.result_state.current_row.and_then(|row| {
                        tab.result_state.base_results.get(row).map(|(path, _)| path)
                    })
                } else {
                    tab.result_state
                        .current_row
                        .and_then(|row| tab.result_state.results.get(row).map(|(path, _)| path))
                };
                if current_path.is_some_and(|current_path| *current_path == response.path) {
                    tab.result_state.preview = response.preview;
                }
            }
        }
    }

    pub(super) fn clear_preview_cache(&mut self) {
        self.preview_cache.entries.clear();
        self.preview_cache.order.clear();
        self.preview_cache.total_bytes = 0;
    }

    pub(super) fn cache_preview(&mut self, path: PathBuf, preview: String) {
        let new_bytes = preview.len();
        if let Some(old) = self.preview_cache.entries.get(&path) {
            self.preview_cache.total_bytes =
                self.preview_cache.total_bytes.saturating_sub(old.len());
        }
        if !self.preview_cache.entries.contains_key(&path) {
            self.preview_cache.order.push_back(path.clone());
        }
        self.preview_cache.entries.insert(path, preview);
        self.preview_cache.total_bytes = self.preview_cache.total_bytes.saturating_add(new_bytes);
        while self.preview_cache.total_bytes > Self::PREVIEW_CACHE_MAX_BYTES {
            if let Some(oldest) = self.preview_cache.order.pop_front() {
                if let Some(evicted) = self.preview_cache.entries.remove(&oldest) {
                    self.preview_cache.total_bytes =
                        self.preview_cache.total_bytes.saturating_sub(evicted.len());
                }
            } else {
                break;
            }
        }
    }

    pub(super) fn clear_highlight_cache(&mut self) {
        self.highlight_cache.entries.clear();
        self.highlight_cache.order.clear();
    }

    pub(super) fn ensure_highlight_cache_scope(&mut self, prefer_relative: bool) {
        if self.highlight_cache.scope_query == self.query
            && Self::path_key(&self.highlight_cache.scope_root) == Self::path_key(&self.root)
            && self.highlight_cache.scope_use_regex == self.use_regex
            && self.highlight_cache.scope_ignore_case == self.ignore_case
            && self.highlight_cache.scope_prefer_relative == prefer_relative
        {
            return;
        }
        self.highlight_cache.scope_query = self.query.clone();
        self.highlight_cache.scope_root = self.root.clone();
        self.highlight_cache.scope_use_regex = self.use_regex;
        self.highlight_cache.scope_ignore_case = self.ignore_case;
        self.highlight_cache.scope_prefer_relative = prefer_relative;
        self.clear_highlight_cache();
    }

    fn cache_highlight_positions_for_key(&mut self, key: HighlightCacheKey, positions: Vec<u16>) {
        if !self.highlight_cache.entries.contains_key(&key) {
            self.highlight_cache.order.push_back(key.clone());
        }
        self.highlight_cache.entries.insert(key, Arc::new(positions));
        while self.highlight_cache.order.len() > Self::HIGHLIGHT_CACHE_MAX {
            if let Some(oldest) = self.highlight_cache.order.pop_front() {
                self.highlight_cache.entries.remove(&oldest);
            }
        }
    }

    fn compact_highlight_positions(positions: HashSet<usize>) -> Vec<u16> {
        let mut compact = positions
            .into_iter()
            .filter_map(|idx| u16::try_from(idx).ok())
            .collect::<Vec<_>>();
        compact.sort_unstable();
        compact.dedup();
        compact
    }

    pub(super) fn highlight_positions_for_path_cached(
        &mut self,
        path: &Path,
        prefer_relative: bool,
    ) -> Arc<Vec<u16>> {
        static EMPTY: OnceLock<Arc<Vec<u16>>> = OnceLock::new();

        self.ensure_highlight_cache_scope(prefer_relative);
        if self.query.trim().is_empty() {
            return Arc::clone(EMPTY.get_or_init(|| Arc::new(Vec::new())));
        }

        let key = HighlightCacheKey {
            path: path.to_path_buf(),
            prefer_relative,
            use_regex: self.use_regex,
            ignore_case: self.ignore_case,
        };

        if let Some(positions) = self.highlight_cache.entries.get(&key) {
            return Arc::clone(positions);
        }

        let positions = Self::compact_highlight_positions(match_positions_for_path(
            path,
            &self.root,
            &self.query,
            prefer_relative,
            self.use_regex,
            self.ignore_case,
        ));
        self.cache_highlight_positions_for_key(key.clone(), positions);
        self.highlight_cache
            .entries
            .get(&key)
            .cloned()
            .unwrap_or_else(|| Arc::clone(EMPTY.get_or_init(|| Arc::new(Vec::new()))))
    }

    pub(super) fn is_highlighted_position(positions: &[u16], idx: usize) -> bool {
        let Ok(idx16) = u16::try_from(idx) else {
            return false;
        };
        positions.binary_search(&idx16).is_ok()
    }

    fn current_result_kind(&self) -> Option<EntryKind> {
        let row = self.current_row?;
        let (path, _) = self.results.get(row)?;
        self.entry_kinds.get(path).copied()
    }

    pub(super) fn request_preview_for_current(&mut self) {
        if !self.show_preview {
            self.preview.clear();
            self.preview_in_progress = false;
            self.pending_preview_request_id = None;
            return;
        }

        if let Some(row) = self.current_row {
            if let Some((path, _)) = self.results.get(row) {
                if let Some(cached) = self.preview_cache.entries.get(path) {
                    self.preview = cached.clone();
                    self.preview_in_progress = false;
                    self.pending_preview_request_id = None;
                    return;
                }

                let Some(kind) = self.current_result_kind() else {
                    self.preview = "Resolving entry type...".to_string();
                    self.queue_kind_resolution(path.clone());
                    self.pump_kind_resolution_requests();
                    self.preview_in_progress = false;
                    self.pending_preview_request_id = None;
                    return;
                };
                let is_dir = kind.is_dir;
                if should_skip_preview(path, is_dir) {
                    let preview = build_preview_text_with_kind(path, is_dir);
                    self.cache_preview(path.clone(), preview.clone());
                    self.preview = preview;
                    self.preview_in_progress = false;
                    self.pending_preview_request_id = None;
                    return;
                }
                self.preview = "Loading preview...".to_string();
                let request_id = self.next_preview_request_id;
                self.next_preview_request_id = self.next_preview_request_id.saturating_add(1);
                self.pending_preview_request_id = Some(request_id);
                if let Some(tab_id) = self.current_tab_id() {
                    self.preview_request_tabs.insert(request_id, tab_id);
                }
                self.preview_in_progress = true;
                let req = PreviewRequest {
                    request_id,
                    path: path.clone(),
                    is_dir,
                };
                if self.preview_tx.send(req).is_err() {
                    self.preview_in_progress = false;
                    self.pending_preview_request_id = None;
                    self.preview = "<preview unavailable>".to_string();
                }
                return;
            }
        }
        self.preview.clear();
        self.preview_in_progress = false;
        self.pending_preview_request_id = None;
    }
}

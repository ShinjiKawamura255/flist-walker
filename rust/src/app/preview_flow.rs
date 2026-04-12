use super::*;
impl FlistWalkerApp {
    pub(super) fn clear_preview_cache(&mut self) {
        self.cache.preview.clear();
    }

    pub(super) fn cache_preview(&mut self, path: PathBuf, preview: String) {
        self.cache
            .preview
            .insert_bounded(path, preview, Self::PREVIEW_CACHE_MAX_BYTES);
    }

    pub(super) fn clear_highlight_cache(&mut self) {
        self.cache.highlight.clear();
    }

    pub(super) fn ensure_highlight_cache_scope(&mut self, prefer_relative: bool) {
        let query = self.runtime.query_state.query.clone();
        let root = self.runtime.root.clone();
        let use_regex = self.runtime.use_regex;
        let ignore_case = self.runtime.ignore_case;
        if self
            .cache
            .highlight
            .matches_scope(&query, &root, use_regex, ignore_case, prefer_relative)
        {
            return;
        }
        self.cache
            .highlight
            .reset_scope(query, root, use_regex, ignore_case, prefer_relative);
    }

    fn cache_highlight_positions_for_key(&mut self, key: HighlightCacheKey, positions: Vec<u16>) {
        self.cache
            .highlight
            .insert_bounded(key, positions, Self::HIGHLIGHT_CACHE_MAX);
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
        if self.runtime.query_state.query.trim().is_empty() {
            return Arc::clone(EMPTY.get_or_init(|| Arc::new(Vec::new())));
        }

        let key = HighlightCacheKey {
            path: path.to_path_buf(),
            prefer_relative,
            use_regex: self.runtime.use_regex,
            ignore_case: self.runtime.ignore_case,
        };

        if let Some(positions) = self.cache.highlight.get(&key) {
            return positions;
        }

        let positions = Self::compact_highlight_positions(match_positions_for_path(
            path,
            &self.runtime.root,
            &self.runtime.query_state.query,
            prefer_relative,
            self.runtime.use_regex,
            self.runtime.ignore_case,
        ));
        self.cache_highlight_positions_for_key(key.clone(), positions);
        self.cache
            .highlight
            .get(&key)
            .unwrap_or_else(|| Arc::clone(EMPTY.get_or_init(|| Arc::new(Vec::new()))))
    }

    pub(super) fn is_highlighted_position(positions: &[u16], idx: usize) -> bool {
        let Ok(idx16) = u16::try_from(idx) else {
            return false;
        };
        positions.binary_search(&idx16).is_ok()
    }

    fn current_result_kind(&self) -> Option<EntryKind> {
        let row = self.runtime.current_row?;
        let (path, _) = self.runtime.results.get(row)?;
        self.find_entry_kind(path)
    }

    pub(super) fn request_preview_for_current(&mut self) {
        if !self.ui.show_preview {
            self.runtime.preview.clear();
            self.worker_bus.preview.in_progress = false;
            self.worker_bus.preview.pending_request_id = None;
            return;
        }

        if let Some(row) = self.runtime.current_row {
            if let Some((path, _)) = self.runtime.results.get(row) {
                if let Some(cached) = self.cache.preview.get(path) {
                    self.runtime.preview = cached.to_string();
                    self.worker_bus.preview.in_progress = false;
                    self.worker_bus.preview.pending_request_id = None;
                    return;
                }
                let path = path.clone();

                let Some(kind) = self.current_result_kind() else {
                    self.runtime.preview = "Resolving entry type...".to_string();
                    self.queue_kind_resolution(path);
                    self.pump_kind_resolution_requests();
                    self.worker_bus.preview.in_progress = false;
                    self.worker_bus.preview.pending_request_id = None;
                    return;
                };
                let is_dir = kind.is_dir;
                if should_skip_preview(&path, is_dir) {
                    let preview = build_preview_text_with_kind(&path, is_dir);
                    self.cache_preview(path.clone(), preview.clone());
                    self.runtime.preview = preview;
                    self.worker_bus.preview.in_progress = false;
                    self.worker_bus.preview.pending_request_id = None;
                    return;
                }
                self.runtime.preview = "Loading preview...".to_string();
                let request_id = self.worker_bus.preview.next_request_id;
                self.worker_bus.preview.next_request_id =
                    self.worker_bus.preview.next_request_id.saturating_add(1);
                self.worker_bus.preview.pending_request_id = Some(request_id);
                self.bind_preview_request_to_current_tab(request_id);
                self.worker_bus.preview.in_progress = true;
                let req = PreviewRequest {
                    request_id,
                    path,
                    is_dir,
                };
                if self.worker_bus.preview.tx.send(req).is_err() {
                    self.worker_bus.preview.in_progress = false;
                    self.worker_bus.preview.pending_request_id = None;
                    self.runtime.preview = "<preview unavailable>".to_string();
                }
                return;
            }
        }
        self.runtime.preview.clear();
        self.worker_bus.preview.in_progress = false;
        self.worker_bus.preview.pending_request_id = None;
    }
}

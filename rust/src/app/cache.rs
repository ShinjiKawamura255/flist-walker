use super::*;

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

    fn get(&self, path: &Path) -> Option<&String> {
        self.entries.get(path)
    }

    fn insert_bounded(&mut self, path: PathBuf, preview: String, max_bytes: usize) {
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

    fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }

    fn matches_scope(
        &self,
        query: &str,
        root: &Path,
        use_regex: bool,
        ignore_case: bool,
        prefer_relative: bool,
    ) -> bool {
        self.scope_query == query
            && FlistWalkerApp::path_key(&self.scope_root) == FlistWalkerApp::path_key(root)
            && self.scope_use_regex == use_regex
            && self.scope_ignore_case == ignore_case
            && self.scope_prefer_relative == prefer_relative
    }

    fn reset_scope(
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

    fn get(&self, key: &HighlightCacheKey) -> Option<Arc<Vec<u16>>> {
        self.entries.get(key).cloned()
    }

    fn insert_bounded(&mut self, key: HighlightCacheKey, positions: Vec<u16>, max_entries: usize) {
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

impl FlistWalkerApp {
    /// sort worker の応答を cache と tab state へ適用する。
    pub(super) fn poll_sort_response(&mut self) {
        while let Ok(response) = self.worker_bus.sort.rx.try_recv() {
            for (path, metadata) in &response.entries {
                self.cache_sort_metadata(path.clone(), *metadata);
            }

            if self.apply_active_sort_response(&response) {
                continue;
            }
            self.apply_background_sort_response(response);
        }
    }

    /// root 単位で破棄すべき sort metadata cache をまとめて消す。
    pub(super) fn clear_sort_metadata_cache(&mut self) {
        self.cache.sort_metadata.clear();
    }

    /// 結果ソートに使う時刻属性を上限付き cache へ保存する。
    pub(super) fn cache_sort_metadata(&mut self, path: PathBuf, metadata: SortMetadata) {
        self.cache
            .sort_metadata
            .insert_bounded(path, metadata, Self::SORT_METADATA_CACHE_MAX);
    }

    /// sort mode ごとに比較対象の timestamp を取り出す。
    fn sort_metadata_value(metadata: SortMetadata, mode: ResultSortMode) -> Option<SystemTime> {
        match mode {
            ResultSortMode::ModifiedDesc | ResultSortMode::ModifiedAsc => metadata.modified,
            ResultSortMode::CreatedDesc | ResultSortMode::CreatedAsc => metadata.created,
            _ => None,
        }
    }

    /// 指定 path の timestamp sort key を cache から取得する。
    fn sort_timestamp_for_path(
        cache: &HashMap<PathBuf, SortMetadata>,
        path: &Path,
        mode: ResultSortMode,
    ) -> Option<SystemTime> {
        cache
            .get(path)
            .copied()
            .and_then(|metadata| Self::sort_metadata_value(metadata, mode))
    }

    /// Name sort 用の比較キーをファイル名優先で正規化する。
    fn path_name_key(path: &Path) -> String {
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
    }

    /// base result snapshot から指定 sort mode の表示順を再構築する。
    pub(super) fn build_sorted_results_from(
        base_results: &[(PathBuf, f64)],
        mode: ResultSortMode,
        cache: &HashMap<PathBuf, SortMetadata>,
    ) -> Vec<(PathBuf, f64)> {
        let mut items = base_results.iter().cloned().enumerate().collect::<Vec<_>>();
        match mode {
            ResultSortMode::Score => return base_results.to_vec(),
            ResultSortMode::NameAsc | ResultSortMode::NameDesc => {
                let desc = matches!(mode, ResultSortMode::NameDesc);
                items.sort_by(|(idx_a, (path_a, _)), (idx_b, (path_b, _))| {
                    let cmp = Self::path_name_key(path_a)
                        .cmp(&Self::path_name_key(path_b))
                        .then_with(|| {
                            normalized_compare_key(path_a).cmp(&normalized_compare_key(path_b))
                        })
                        .then_with(|| idx_a.cmp(idx_b));
                    if desc {
                        cmp.reverse()
                    } else {
                        cmp
                    }
                });
            }
            ResultSortMode::ModifiedDesc
            | ResultSortMode::ModifiedAsc
            | ResultSortMode::CreatedDesc
            | ResultSortMode::CreatedAsc => {
                let desc = matches!(
                    mode,
                    ResultSortMode::ModifiedDesc | ResultSortMode::CreatedDesc
                );
                items.sort_by(|(idx_a, (path_a, _)), (idx_b, (path_b, _))| {
                    let time_a = Self::sort_timestamp_for_path(cache, path_a, mode);
                    let time_b = Self::sort_timestamp_for_path(cache, path_b, mode);
                    match (time_a, time_b) {
                        (Some(a), Some(b)) => {
                            if desc {
                                b.cmp(&a)
                            } else {
                                a.cmp(&b)
                            }
                        }
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (None, None) => std::cmp::Ordering::Equal,
                    }
                    .then_with(|| Self::path_name_key(path_a).cmp(&Self::path_name_key(path_b)))
                    .then_with(|| {
                        normalized_compare_key(path_a).cmp(&normalized_compare_key(path_b))
                    })
                    .then_with(|| idx_a.cmp(idx_b))
                });
            }
        }
        items.into_iter().map(|(_, entry)| entry).collect()
    }

    /// 現在の base result snapshot から表示用の整列結果を生成する。
    fn build_sorted_results(&self, mode: ResultSortMode) -> Vec<(PathBuf, f64)> {
        Self::build_sorted_results_from(
            &self.base_results,
            mode,
            self.cache.sort_metadata.get_map(),
        )
    }

    /// 結果一覧を差し替えつつ current row と scroll 方針を維持する。
    pub(super) fn replace_results_snapshot(
        &mut self,
        results: Vec<(PathBuf, f64)>,
        keep_scroll_position: bool,
    ) {
        self.worker_bus.sort.pending_request_id = None;
        self.worker_bus.sort.in_progress = false;
        self.result_sort_mode = ResultSortMode::Score;
        self.base_results = results.clone();
        // Regression guard: search refreshes must keep the cursor on the same row number.
        // Following the previous path here makes the highlight jump when the query changes.
        self.apply_results_with_selection_policy(results, keep_scroll_position, false);
    }

    /// 非 score sort を解除し、必要なら base snapshot を前面へ戻す。
    pub(super) fn invalidate_result_sort(&mut self, keep_scroll_position: bool) {
        let had_non_score_sort = self.result_sort_mode != ResultSortMode::Score;
        self.worker_bus.sort.pending_request_id = None;
        self.worker_bus.sort.in_progress = false;
        self.result_sort_mode = ResultSortMode::Score;
        if had_non_score_sort && !self.base_results.is_empty() && self.results != self.base_results
        {
            self.apply_results_with_selection_policy(
                self.base_results.clone(),
                keep_scroll_position,
                true,
            );
        } else {
            self.refresh_status_line();
        }
    }

    /// 欠けている metadata だけを sort worker に依頼する。
    fn request_sort_metadata(&mut self, mode: ResultSortMode, missing_paths: Vec<PathBuf>) {
        let request_id = self.worker_bus.sort.next_request_id;
        self.worker_bus.sort.next_request_id =
            self.worker_bus.sort.next_request_id.saturating_add(1);
        self.worker_bus.sort.pending_request_id = Some(request_id);
        self.worker_bus.sort.in_progress = true;
        self.bind_sort_request_to_current_tab(request_id);
        self.refresh_status_line();
        if self
            .worker_bus
            .sort
            .tx
            .send(SortMetadataRequest {
                request_id,
                paths: missing_paths,
                mode,
            })
            .is_err()
        {
            self.worker_bus.sort.pending_request_id = None;
            self.worker_bus.sort.in_progress = false;
            self.set_notice("Sort worker is unavailable");
        }
    }

    /// 現在の sort mode を結果スナップショットへ反映する。
    pub(super) fn apply_result_sort(&mut self, keep_scroll_position: bool) {
        if self.base_results.is_empty() {
            self.worker_bus.sort.pending_request_id = None;
            self.worker_bus.sort.in_progress = false;
            self.refresh_status_line();
            return;
        }
        if !self.result_sort_mode.uses_metadata() {
            let sorted = self.build_sorted_results(self.result_sort_mode);
            self.worker_bus.sort.pending_request_id = None;
            self.worker_bus.sort.in_progress = false;
            self.apply_results_with_selection_policy(sorted, keep_scroll_position, false);
            return;
        }

        let missing_paths = self
            .base_results
            .iter()
            .map(|(path, _)| path.clone())
            .filter(|path| !self.cache.sort_metadata.contains(path))
            .collect::<Vec<_>>();
        if missing_paths.is_empty() {
            let sorted = self.build_sorted_results(self.result_sort_mode);
            self.worker_bus.sort.pending_request_id = None;
            self.worker_bus.sort.in_progress = false;
            self.apply_results_with_selection_policy(sorted, keep_scroll_position, false);
            return;
        }

        self.request_sort_metadata(self.result_sort_mode, missing_paths);
    }

    /// sort mode を切り替え、即時適用または metadata 解決を始める。
    pub(super) fn set_result_sort_mode(&mut self, mode: ResultSortMode) {
        self.result_sort_mode = mode;
        self.apply_result_sort(false);
    }
}

impl FlistWalkerApp {
    pub(super) fn bind_preview_request_to_tab(&mut self, request_id: u64, tab_id: u64) {
        self.request_tab_routing.bind_preview(request_id, tab_id);
    }

    fn bind_preview_request_to_current_tab(&mut self, request_id: u64) {
        if let Some(tab_id) = self.current_tab_id() {
            self.bind_preview_request_to_tab(request_id, tab_id);
        }
    }

    fn take_preview_request_tab(&mut self, request_id: u64) -> Option<u64> {
        self.request_tab_routing.take_preview(request_id)
    }

    pub(super) fn clear_preview_request_routing_for_tab(&mut self, tab_id: u64) {
        self.request_tab_routing.clear_preview_for_tab(tab_id);
    }

    #[cfg(test)]
    pub(super) fn preview_request_tab(&self, request_id: u64) -> Option<u64> {
        self.request_tab_routing.preview.get(&request_id).copied()
    }

    fn apply_background_preview_response(&mut self, response: PreviewResponse) {
        let Some(tab_id) = self.take_preview_request_tab(response.request_id) else {
            return;
        };
        let Some(tab_index) = self.find_tab_index_by_id(tab_id) else {
            return;
        };
        self.cache_preview(response.path.clone(), response.preview.clone());
        if let Some(tab) = self.tabs.get_mut(tab_index) {
            tab.pending_preview_request_id = None;
            tab.preview_in_progress = false;
            let current_path = if tab.result_state.results_compacted {
                tab.result_state
                    .current_row
                    .and_then(|row| tab.result_state.base_results.get(row).map(|(path, _)| path))
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

    pub(super) fn poll_preview_response(&mut self) {
        while let Ok(response) = self.worker_bus.preview.rx.try_recv() {
            if Some(response.request_id) == self.worker_bus.preview.pending_request_id {
                self.take_preview_request_tab(response.request_id);
                self.worker_bus.preview.pending_request_id = None;
                self.worker_bus.preview.in_progress = false;
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
            self.apply_background_preview_response(response);
        }
    }

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
        if self.cache.highlight.matches_scope(
            &self.query_state.query,
            &self.root,
            self.use_regex,
            self.ignore_case,
            prefer_relative,
        ) {
            return;
        }
        self.cache.highlight.reset_scope(
            self.query_state.query.clone(),
            self.root.clone(),
            self.use_regex,
            self.ignore_case,
            prefer_relative,
        );
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
        if self.query_state.query.trim().is_empty() {
            return Arc::clone(EMPTY.get_or_init(|| Arc::new(Vec::new())));
        }

        let key = HighlightCacheKey {
            path: path.to_path_buf(),
            prefer_relative,
            use_regex: self.use_regex,
            ignore_case: self.ignore_case,
        };

        if let Some(positions) = self.cache.highlight.get(&key) {
            return positions;
        }

        let positions = Self::compact_highlight_positions(match_positions_for_path(
            path,
            &self.root,
            &self.query_state.query,
            prefer_relative,
            self.use_regex,
            self.ignore_case,
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
        let row = self.current_row?;
        let (path, _) = self.results.get(row)?;
        self.find_entry_kind(path)
    }

    pub(super) fn request_preview_for_current(&mut self) {
        if !self.ui.show_preview {
            self.preview.clear();
            self.worker_bus.preview.in_progress = false;
            self.worker_bus.preview.pending_request_id = None;
            return;
        }

        if let Some(row) = self.current_row {
            if let Some((path, _)) = self.results.get(row) {
                if let Some(cached) = self.cache.preview.get(path) {
                    self.preview = cached.to_string();
                    self.worker_bus.preview.in_progress = false;
                    self.worker_bus.preview.pending_request_id = None;
                    return;
                }
                let path = path.clone();

                let Some(kind) = self.current_result_kind() else {
                    self.preview = "Resolving entry type...".to_string();
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
                    self.preview = preview;
                    self.worker_bus.preview.in_progress = false;
                    self.worker_bus.preview.pending_request_id = None;
                    return;
                }
                self.preview = "Loading preview...".to_string();
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
                    self.preview = "<preview unavailable>".to_string();
                }
                return;
            }
        }
        self.preview.clear();
        self.worker_bus.preview.in_progress = false;
        self.worker_bus.preview.pending_request_id = None;
    }
}

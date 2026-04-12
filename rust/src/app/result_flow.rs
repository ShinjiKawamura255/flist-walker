use super::*;

impl FlistWalkerApp {
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
    pub(super) fn build_sorted_results(&self, mode: ResultSortMode) -> Vec<(PathBuf, f64)> {
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
        result_reducer::replace_results_snapshot(self, results, keep_scroll_position);
    }

    /// 非 score sort を解除し、必要なら base snapshot を前面へ戻す。
    pub(super) fn invalidate_result_sort(&mut self, keep_scroll_position: bool) {
        result_reducer::invalidate_result_sort(self, keep_scroll_position);
    }

    /// 現在の sort mode を結果スナップショットへ反映する。
    pub(super) fn apply_result_sort(&mut self, keep_scroll_position: bool) {
        result_reducer::apply_result_sort(self, keep_scroll_position);
    }

    /// sort mode を切り替え、即時適用または metadata 解決を始める。
    pub(super) fn set_result_sort_mode(&mut self, mode: ResultSortMode) {
        result_reducer::set_result_sort_mode(self, mode);
    }
}

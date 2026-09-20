use super::{result_reducer, FlistWalkerApp, ResultSortMode, SortMetadata};
use std::collections::HashMap;
use std::path::PathBuf;

impl FlistWalkerApp {
    /// root 単位で破棄すべき sort metadata cache をまとめて消す。
    pub(super) fn clear_sort_metadata_cache(&mut self) {
        self.shell.cache.sort_metadata.clear();
    }

    /// 結果ソートに使う時刻属性を上限付き cache へ保存する。
    pub(super) fn cache_sort_metadata(&mut self, path: PathBuf, metadata: SortMetadata) {
        self.shell.cache.sort_metadata.insert_bounded(
            path,
            metadata,
            Self::SORT_METADATA_CACHE_MAX,
        );
    }

    /// Adapter for the shared, data-only result ordering policy.
    pub(super) fn build_sorted_results_from(
        base_results: &[(PathBuf, f64)],
        mode: ResultSortMode,
        cache: &HashMap<PathBuf, SortMetadata>,
    ) -> Vec<(PathBuf, f64)> {
        super::result_policy::build_sorted_results_from(base_results, mode, cache)
    }

    /// 現在の base result snapshot から表示用の整列結果を生成する。
    pub(super) fn build_sorted_results(&self, mode: ResultSortMode) -> Vec<(PathBuf, f64)> {
        Self::build_sorted_results_from(
            &self.shell.runtime.base_results,
            mode,
            self.shell.cache.sort_metadata.get_map(),
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
    pub(super) fn apply_result_sort(
        &mut self,
        keep_scroll_position: bool,
    ) -> result_reducer::ResultSortApplyOutcome {
        result_reducer::apply_result_sort(self, keep_scroll_position)
    }

    /// sort mode を切り替え、即時適用または metadata 解決を始める。
    pub(super) fn set_result_sort_mode(&mut self, mode: ResultSortMode) {
        result_reducer::set_result_sort_mode(self, mode);
    }

    /// Apply a deliberate GUI sort choice without changing restoration policy.
    pub(super) fn select_result_sort_mode(&mut self, mode: ResultSortMode) {
        // Only a new user choice supplies the default scope. Re-selecting the
        // current mode must preserve an explicit Shown results choice.
        if self.shell.runtime.result_sort_mode != mode && mode.uses_metadata() {
            self.shell.runtime.result_sort_scope = super::ResultSortScope::AllMatches;
        }
        self.set_result_sort_mode(mode);
    }

    /// sort scope を切り替え、必要なら全マッチ検索を再実行する。
    pub(super) fn set_result_sort_scope(&mut self, scope: super::ResultSortScope) {
        result_reducer::set_result_sort_scope(self, scope);
    }
}

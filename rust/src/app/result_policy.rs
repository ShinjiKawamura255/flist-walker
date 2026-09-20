//! Result decisions shared by active and background tabs. No I/O or dispatch.
use super::coordinator::normalized_compare_key;
use super::state::SortMetadata;
use crate::search::SearchSortMode as ResultSortMode;
use crate::search::{SearchSortMode, SearchSortScope};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// A snapshot publication decision. Deferred local sorting must keep both the
/// old visible rows and their denominator until metadata/sorting completes.
pub(super) struct SearchResultUpdate {
    pub(super) base_results: Vec<(PathBuf, f64)>,
    pub(super) visible_results: Option<Vec<(PathBuf, f64)>>,
    pub(super) total_match_count: usize,
    pub(super) base_results_are_score_ranked: bool,
    pub(super) sort_mode: SearchSortMode,
    pub(super) sort_scope: SearchSortScope,
    pub(super) search_error: Option<(String, String)>,
    pub(super) notice: String,
    pub(super) failed: bool,
}

impl SearchResultUpdate {
    pub(super) fn prepare(
        query: &str,
        results: Vec<(PathBuf, f64)>,
        total_match_count: usize,
        sort_mode: SearchSortMode,
        sort_scope: SearchSortScope,
        error: Option<String>,
    ) -> Self {
        let local_sort =
            sort_scope == SearchSortScope::ShownResults && sort_mode != SearchSortMode::Score;
        let visible_results = (!local_sort).then(|| results.clone());
        Self {
            base_results: results,
            visible_results,
            total_match_count,
            base_results_are_score_ranked: !sort_scope.sorts_all_matches_before_limit(sort_mode),
            sort_mode,
            sort_scope,
            failed: error.is_some(),
            notice: error
                .as_ref()
                .map(|error| format!("Search failed: {error}"))
                .unwrap_or_default(),
            search_error: error.map(|error| (query.to_owned(), error)),
        }
    }
}

/// Even an empty query needs the search worker when ordering all matches before
/// the limit. Sorting only the displayed subset cannot recover omitted entries.
pub(super) fn needs_search_worker(
    query: &str,
    mode: SearchSortMode,
    scope: SearchSortScope,
) -> bool {
    !query.trim().is_empty() || scope.sorts_all_matches_before_limit(mode)
}

struct SortableResult {
    original_index: usize,
    entry: (PathBuf, f64),
    name_key: String,
    path_key: String,
    timestamp: Option<SystemTime>,
    size_bytes: Option<u64>,
}

/// sort mode ごとに比較対象の timestamp を取り出す。
fn sort_metadata_value(metadata: SortMetadata, mode: ResultSortMode) -> Option<SystemTime> {
    match mode {
        ResultSortMode::ModifiedDesc | ResultSortMode::ModifiedAsc => metadata.modified,
        ResultSortMode::CreatedDesc | ResultSortMode::CreatedAsc => metadata.created,
        _ => None,
    }
}

fn sort_size_for_path(
    cache: &HashMap<PathBuf, SortMetadata>,
    path: &Path,
    mode: ResultSortMode,
) -> Option<u64> {
    match mode {
        ResultSortMode::SizeDesc | ResultSortMode::SizeAsc => {
            cache.get(path).and_then(|metadata| metadata.size_bytes)
        }
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
        .and_then(|metadata| sort_metadata_value(metadata, mode))
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
    if mode == ResultSortMode::Score {
        return base_results.to_vec();
    }

    let mut items = base_results
        .iter()
        .cloned()
        .enumerate()
        .map(|(original_index, entry)| {
            let timestamp = sort_timestamp_for_path(cache, &entry.0, mode);
            let size_bytes = sort_size_for_path(cache, &entry.0, mode);
            let name_key = path_name_key(&entry.0);
            let path_key = normalized_compare_key(&entry.0);
            SortableResult {
                original_index,
                entry,
                name_key,
                path_key,
                timestamp,
                size_bytes,
            }
        })
        .collect::<Vec<_>>();
    match mode {
        ResultSortMode::Score => unreachable!("score mode returns before sorting"),
        ResultSortMode::NameAsc | ResultSortMode::NameDesc => {
            let desc = matches!(mode, ResultSortMode::NameDesc);
            items.sort_by(|a, b| {
                let cmp = a
                    .name_key
                    .cmp(&b.name_key)
                    .then_with(|| a.path_key.cmp(&b.path_key))
                    .then_with(|| a.original_index.cmp(&b.original_index));
                if desc {
                    cmp.reverse()
                } else {
                    cmp
                }
            });
        }
        ResultSortMode::PathAsc | ResultSortMode::PathDesc => {
            let desc = matches!(mode, ResultSortMode::PathDesc);
            items.sort_by(|a, b| {
                let cmp = a
                    .path_key
                    .cmp(&b.path_key)
                    .then_with(|| a.original_index.cmp(&b.original_index));
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
        | ResultSortMode::CreatedAsc
        | ResultSortMode::SizeDesc
        | ResultSortMode::SizeAsc => {
            let desc = matches!(
                mode,
                ResultSortMode::ModifiedDesc
                    | ResultSortMode::CreatedDesc
                    | ResultSortMode::SizeDesc
            );
            items.sort_by(|a, b| {
                let value_cmp =
                    if matches!(mode, ResultSortMode::SizeDesc | ResultSortMode::SizeAsc) {
                        compare_optional_sort_value(a.size_bytes, b.size_bytes, desc)
                    } else {
                        compare_optional_sort_value(a.timestamp, b.timestamp, desc)
                    };
                value_cmp
                    .then_with(|| a.name_key.cmp(&b.name_key))
                    .then_with(|| a.path_key.cmp(&b.path_key))
                    .then_with(|| a.original_index.cmp(&b.original_index))
            });
        }
    }
    items.into_iter().map(|item| item.entry).collect()
}

fn compare_optional_sort_value<T: Ord>(
    a: Option<T>,
    b: Option<T>,
    desc: bool,
) -> std::cmp::Ordering {
    match (a, b) {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publication_matrix_preserves_pending_snapshot_and_ranking_membership() {
        for mode in [
            SearchSortMode::Score,
            SearchSortMode::NameAsc,
            SearchSortMode::ModifiedDesc,
        ] {
            for scope in [SearchSortScope::ShownResults, SearchSortScope::AllMatches] {
                for empty in [false, true] {
                    for error in [None, Some("invalid regex".to_string())] {
                        let results = if empty {
                            Vec::new()
                        } else {
                            vec![(PathBuf::from("日本語.txt"), 7.0)]
                        };
                        let update = SearchResultUpdate::prepare(
                            "日本語",
                            results.clone(),
                            42,
                            mode,
                            scope,
                            error.clone(),
                        );
                        assert_eq!(update.base_results, results);
                        assert_eq!(update.total_match_count, 42);
                        assert_eq!(
                            update.visible_results,
                            if scope == SearchSortScope::ShownResults
                                && mode != SearchSortMode::Score
                            {
                                None
                            } else {
                                Some(results)
                            }
                        );
                        assert_eq!(
                            update.base_results_are_score_ranked,
                            scope == SearchSortScope::ShownResults || mode == SearchSortMode::Score
                        );
                        assert_eq!(
                            update.search_error,
                            error.clone().map(|error| ("日本語".to_string(), error))
                        );
                        assert_eq!(update.failed, error.is_some());
                        assert_eq!(
                            update.notice,
                            error
                                .map(|error| format!("Search failed: {error}"))
                                .unwrap_or_default()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn empty_query_all_matches_uses_worker_before_limiting() {
        assert!(needs_search_worker(
            "",
            SearchSortMode::NameAsc,
            SearchSortScope::AllMatches
        ));
        assert!(!needs_search_worker(
            " \t",
            SearchSortMode::Score,
            SearchSortScope::AllMatches
        ));
        assert!(!needs_search_worker(
            "",
            SearchSortMode::ModifiedDesc,
            SearchSortScope::ShownResults
        ));
        assert!(needs_search_worker(
            "'needle",
            SearchSortMode::Score,
            SearchSortScope::ShownResults
        ));
    }
}
